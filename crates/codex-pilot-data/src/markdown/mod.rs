mod export;
mod format;
mod models;
mod render;

use crate::storage::{SchemaKind, SessionRef, schema_kind};
use export::{export_codex_thread, export_generic_session};
use format::{
    TextPart, exported_at_label, normalize_newlines, parse_time_hhmm, split_fenced_code,
    strip_image_tags,
};
pub use models::{ExportResult, ExportStatus};
use models::{Message, MessageBlock, exported, failed, not_found};
use render::{render_html, render_markdown};
use rusqlite::Connection;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct MarkdownExportService {
    db_path: PathBuf,
}

impl MarkdownExportService {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    pub fn export(&self, session: &SessionRef) -> anyhow::Result<ExportResult> {
        self.export_markdown(session)
    }

    pub fn export_markdown(&self, session: &SessionRef) -> anyhow::Result<ExportResult> {
        self.export_with(session, ExportFormat::Markdown)
    }

    pub fn export_html(&self, session: &SessionRef) -> anyhow::Result<ExportResult> {
        self.export_with(session, ExportFormat::Html)
    }

    fn export_with(
        &self,
        session: &SessionRef,
        format: ExportFormat,
    ) -> anyhow::Result<ExportResult> {
        if !self.db_path.exists() {
            return Ok(failed(
                &session.normalized_id(),
                format!("database not found: {}", self.db_path.display()),
            ));
        }

        let db = Connection::open(&self.db_path)?;
        match schema_kind(&db)? {
            Some(SchemaKind::GenericSessions) => export_generic_session(&db, session, format),
            Some(SchemaKind::CodexThreads) => export_codex_thread(&db, session, format),
            None => Ok(failed(
                &session.normalized_id(),
                "unsupported local storage schema",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ExportFormat {
    Markdown,
    Html,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_path(name: &str, extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "codex-pilot-data-{name}-{}.{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            extension
        ))
    }

    #[test]
    fn exports_generic_session_markdown() {
        let db_path = unique_temp_path("generic-export", "sqlite");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            r#"
            CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT);
            CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, body TEXT, created_at TEXT);
            INSERT INTO sessions VALUES ('s1', 'Fixture');
            INSERT INTO messages (session_id, role, body, created_at) VALUES ('s1', 'user', 'hello', '2026-01-01T00:00:00Z');
            INSERT INTO messages (session_id, role, body, created_at) VALUES ('s1', 'assistant', 'hi', '2026-01-01T00:00:01Z');
            "#,
        )
        .unwrap();
        drop(db);

        let service = MarkdownExportService::new(db_path.clone());
        let result = service.export(&SessionRef::new("s1", None)).unwrap();
        assert_eq!(result.status, ExportStatus::Exported);
        let markdown = result.markdown.unwrap();
        assert!(markdown.contains("# Fixture"));
        assert!(markdown.contains("## User"));
        assert!(markdown.contains("hello"));
        assert!(markdown.contains("## Assistant"));
        assert!(markdown.contains("hi"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn exports_codex_rollout_markdown() {
        let db_path = unique_temp_path("codex-export", "sqlite");
        let rollout_path = unique_temp_path("codex-rollout", "jsonl");
        fs::write(
            &rollout_path,
            r#"{"type":"response_item","timestamp":"2026-01-01T00:00:00Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}}"#,
        )
        .unwrap();
        let db = Connection::open(&db_path).unwrap();
        db.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT, rollout_path TEXT)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO threads VALUES (?1, ?2, ?3)",
            ("t1", "Thread", rollout_path.to_string_lossy().as_ref()),
        )
        .unwrap();
        drop(db);

        let service = MarkdownExportService::new(db_path.clone());
        let result = service.export(&SessionRef::new("local:t1", None)).unwrap();
        assert_eq!(result.status, ExportStatus::Exported);
        let markdown = result.markdown.unwrap();
        assert!(markdown.contains("# Thread"));
        assert!(markdown.contains("## User"));
        assert!(markdown.contains("hello"));

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(rollout_path);
    }

    #[test]
    fn exports_html_with_escaped_content() {
        let db_path = unique_temp_path("html-export", "sqlite");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            r#"
            CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT);
            CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, body TEXT, created_at TEXT);
            INSERT INTO sessions VALUES ('s1', 'Display <Thread>');
            INSERT INTO messages (session_id, role, body, created_at) VALUES ('s1', 'user', '<script>alert(1)</script>', '2026-01-01T09:47:03Z');
            INSERT INTO messages (session_id, role, body, created_at) VALUES ('s1', 'assistant', '```rust
fn main() {}
```', '2026-01-01T09:48:03Z');
            "#,
        )
        .unwrap();
        drop(db);

        let service = MarkdownExportService::new(db_path.clone());
        let result = service.export_html(&SessionRef::new("s1", None)).unwrap();
        assert_eq!(result.status, ExportStatus::Exported);
        assert_eq!(result.filename.as_deref(), Some("Display Thread-s1.html"));
        assert!(result.markdown.is_none());
        let html = result.html.unwrap();
        assert!(html.contains("CodexPilot Export"));
        assert!(html.contains("Display &lt;Thread&gt;"));
        assert!(html.contains(r#"<section class="message user">"#));
        assert!(html.contains(r#"<section class="message assistant">"#));
        assert!(html.contains(r#">You<span class="time">09:47</span>"#));
        assert!(html.contains(r#">AI<span class="time">09:48</span>"#));
        assert!(html.contains(r#"<pre class="code-block"><code>fn main() {}"#));
        assert!(!html.contains("```rust"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn exports_rollout_html_with_clean_image_attachment() {
        let db_path = unique_temp_path("html-image-export", "sqlite");
        let rollout_path = unique_temp_path("html-image-rollout", "jsonl");
        fs::write(
            &rollout_path,
            r#"{"type":"response_item","timestamp":"2026-05-21T09:47:03.505Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"点了同步没反应\n<image>"},{"type":"input_image","image_url":"data:image/png;base64,abc"},{"type":"input_text","text":"</image>"}]}}"#,
        )
        .unwrap();
        let db = Connection::open(&db_path).unwrap();
        db.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT, rollout_path TEXT)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO threads VALUES (?1, ?2, ?3)",
            ("t1", "Thread", rollout_path.to_string_lossy().as_ref()),
        )
        .unwrap();
        drop(db);

        let service = MarkdownExportService::new(db_path.clone());
        let result = service
            .export_html(&SessionRef::new("local:t1", None))
            .unwrap();
        assert_eq!(result.status, ExportStatus::Exported);
        let html = result.html.unwrap();
        assert!(html.contains("点了同步没反应"));
        assert!(html.contains(r#"<span class="time">09:47</span>"#));
        assert!(html.contains(r#"<img src="data:image/png;base64,abc" alt="Image attachment">"#));
        assert!(!html.contains("&lt;image&gt;"));
        assert!(!html.contains("&lt;/image&gt;"));
        assert!(!html.contains("Image attachment</div>"));

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(rollout_path);
    }

    #[test]
    fn exports_rollout_html_image_sources_and_placeholder() {
        let db_path = unique_temp_path("html-image-sources-export", "sqlite");
        let rollout_path = unique_temp_path("html-image-sources-rollout", "jsonl");
        fs::write(
            &rollout_path,
            r#"{"type":"response_item","timestamp":"2026-05-21T09:47:03.505Z","payload":{"type":"message","role":"user","content":[{"type":"input_image","image_url":" https://example.com/a.png "},{"type":"input_image","path":"/tmp/local-image.png"},{"type":"input_image"}]}}"#,
        )
        .unwrap();
        let db = Connection::open(&db_path).unwrap();
        db.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT, rollout_path TEXT)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO threads VALUES (?1, ?2, ?3)",
            ("t1", "Thread", rollout_path.to_string_lossy().as_ref()),
        )
        .unwrap();
        drop(db);

        let service = MarkdownExportService::new(db_path.clone());
        let result = service
            .export_html(&SessionRef::new("local:t1", None))
            .unwrap();
        assert_eq!(result.status, ExportStatus::Exported);
        let html = result.html.unwrap();
        assert!(html.contains(r#"<img src="https://example.com/a.png" alt="Image attachment">"#));
        assert!(html.contains(r#"<img src="/tmp/local-image.png" alt="Image attachment">"#));
        assert!(html.contains("<span>图片附件</span>"));

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(rollout_path);
    }

    #[test]
    fn markdown_export_keeps_image_wrappers_and_uses_safe_image_placeholder() {
        let db_path = unique_temp_path("markdown-image-export", "sqlite");
        let rollout_path = unique_temp_path("markdown-image-rollout", "jsonl");
        fs::write(
            &rollout_path,
            r#"{"type":"response_item","timestamp":"2026-05-21T09:47:03.505Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"keep <image>inside</image>"},{"type":"input_image","image_url":"https://example.com/a).png"}]}}"#,
        )
        .unwrap();
        let db = Connection::open(&db_path).unwrap();
        db.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT, rollout_path TEXT)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO threads VALUES (?1, ?2, ?3)",
            ("t1", "Thread", rollout_path.to_string_lossy().as_ref()),
        )
        .unwrap();
        drop(db);

        let service = MarkdownExportService::new(db_path.clone());
        let result = service
            .export_markdown(&SessionRef::new("local:t1", None))
            .unwrap();
        assert_eq!(result.status, ExportStatus::Exported);
        let markdown = result.markdown.unwrap();
        assert!(markdown.contains("keep <image>inside</image>"));
        assert!(markdown.contains("> Image attachment"));
        assert!(!markdown.contains("https://example.com/a).png"));

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(rollout_path);
    }
}
