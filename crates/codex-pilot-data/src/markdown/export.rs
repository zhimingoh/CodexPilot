use super::format::{extract_image_src, replace_filename_chars, text_blocks};
use super::{
    ExportFormat, ExportResult, Message, MessageBlock, exported, failed, normalize_newlines,
    not_found,
};
use crate::storage::{SessionRef, has_columns, normalize_session_id};
use anyhow::Context;
use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::Path;

pub(super) fn export_generic_session(
    db: &Connection,
    session: &SessionRef,
    format: ExportFormat,
) -> anyhow::Result<ExportResult> {
    let session_id = session.normalized_id();
    let title = match fetch_optional_title(db, "sessions", "id", &session_id)? {
        Some(title) => display_title(&title),
        None => return Ok(not_found(&session_id, "session not found in local storage")),
    };
    let messages = fetch_generic_messages(db, &session_id)?;
    Ok(exported(session_id, &title, &messages, format))
}

pub(super) fn export_codex_thread(
    db: &Connection,
    session: &SessionRef,
    format: ExportFormat,
) -> anyhow::Result<ExportResult> {
    let thread_id = normalize_session_id(&session.id);
    let row = db.query_row(
        "SELECT title, rollout_path FROM threads WHERE id = ?1",
        [&thread_id],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        },
    );
    let (title, rollout_path) = match row {
        Ok(row) => row,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Ok(not_found(&thread_id, "thread not found in local storage"));
        }
        Err(err) => return Err(err.into()),
    };
    let title = display_title(title.as_deref().unwrap_or("Untitled session"));
    let Some(rollout_path) = rollout_path.filter(|path| !path.trim().is_empty()) else {
        return Ok(failed(&thread_id, "thread has no rollout_path"));
    };
    let messages = load_rollout_messages(Path::new(&rollout_path))
        .with_context(|| format!("read rollout {}", rollout_path))?;
    Ok(exported(thread_id, &title, &messages, format))
}

fn fetch_optional_title(
    db: &Connection,
    table: &str,
    id_column: &str,
    id: &str,
) -> anyhow::Result<Option<String>> {
    if has_columns(db, table, &["title"])? {
        let sql = format!("SELECT title FROM {table} WHERE {id_column} = ?1");
        let row = db.query_row(&sql, [id], |row| row.get::<_, Option<String>>(0));
        match row {
            Ok(title) => Ok(title),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    } else {
        let sql = format!("SELECT {id_column} FROM {table} WHERE {id_column} = ?1");
        let row = db.query_row(&sql, [id], |_| Ok(()));
        match row {
            Ok(()) => Ok(Some(id.to_string())),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }
}

fn fetch_generic_messages(db: &Connection, session_id: &str) -> anyhow::Result<Vec<Message>> {
    if !crate::storage::has_table(db, "messages")? {
        return Ok(Vec::new());
    }
    let has_role = has_columns(db, "messages", &["role"])?;
    let body_column = ["body", "content", "text"]
        .iter()
        .find(|column| has_columns(db, "messages", &[*column]).unwrap_or(false))
        .copied();
    let Some(body_column) = body_column else {
        return Ok(Vec::new());
    };
    let order_clause = if has_columns(db, "messages", &["created_at"])? {
        " ORDER BY created_at, id"
    } else if has_columns(db, "messages", &["id"])? {
        " ORDER BY id"
    } else {
        ""
    };
    let sql = format!(
        "SELECT {}{}{} FROM messages WHERE session_id = ?1{}",
        if has_role { "role, " } else { "" },
        body_column,
        if has_columns(db, "messages", &["created_at"])? {
            ", created_at"
        } else {
            ""
        },
        order_clause
    );
    let mut stmt = db.prepare(&sql)?;
    let messages = stmt
        .query_map([session_id], |row| {
            let mut index = 0;
            let role = if has_role {
                let value = row.get::<_, Option<String>>(index)?.unwrap_or_default();
                index += 1;
                value
            } else {
                String::new()
            };
            let body = row.get::<_, Option<String>>(index)?.unwrap_or_default();
            index += 1;
            let timestamp = if has_columns(db, "messages", &["created_at"]).unwrap_or(false) {
                row.get::<_, Option<String>>(index)?
            } else {
                None
            };
            Ok(Message {
                speaker: display_role(&role),
                timestamp,
                blocks: text_blocks(body),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(messages
        .into_iter()
        .filter(|message| !message.blocks.is_empty())
        .collect())
}

fn load_rollout_messages(path: &Path) -> anyhow::Result<Vec<Message>> {
    let mut messages = Vec::new();
    for raw in fs::read_to_string(path)?.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let event: Value = serde_json::from_str(raw)?;
        if event.get("type").and_then(Value::as_str) != Some("response_item") {
            continue;
        }
        let payload = &event["payload"];
        if payload.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let role = payload.get("role").and_then(Value::as_str).unwrap_or("");
        if !matches!(role, "user" | "assistant" | "system") {
            continue;
        }
        let blocks = serialize_message_content(&payload["content"]);
        if blocks.is_empty() {
            continue;
        }
        messages.push(Message {
            speaker: display_role(role),
            timestamp: event
                .get("timestamp")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            blocks,
        });
    }
    Ok(messages)
}

fn serialize_message_content(content: &Value) -> Vec<MessageBlock> {
    let Some(items) = content.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .flat_map(|block| {
            let block_type = block.get("type").and_then(Value::as_str)?;
            match block_type {
                "input_text" | "output_text" | "text" => block
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|text| text_blocks(text.to_string())),
                "input_image" => Some(vec![MessageBlock::Image(extract_image_src(block))]),
                _ => None,
            }
        })
        .flatten()
        .collect()
}

fn display_role(role: &str) -> String {
    match role {
        "user" => "User".to_string(),
        "assistant" => "Assistant".to_string(),
        "system" => "System".to_string(),
        value if !value.trim().is_empty() => display_title(value),
        _ => "Message".to_string(),
    }
}

fn display_title(value: &str) -> String {
    let normalized = normalize_newlines(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() {
        "Untitled session".to_string()
    } else {
        normalized
    }
}

pub(super) fn build_filename(title: &str, session_id: &str, extension: &str) -> String {
    let cleaned = replace_filename_chars(title, " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut safe_title = cleaned
        .trim_matches([' ', '.'])
        .chars()
        .take(80)
        .collect::<String>();
    if safe_title.is_empty() {
        safe_title = "Untitled session".to_string();
    }
    format!(
        "{}-{}.{}",
        safe_title,
        replace_filename_chars(session_id, "-").trim_matches('-'),
        extension
    )
}
