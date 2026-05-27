use super::ExportFormat;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportStatus {
    Exported,
    NotFound,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ExportResult {
    pub status: ExportStatus,
    pub session_id: String,
    pub message: String,
    pub filename: Option<String>,
    pub markdown: Option<String>,
    pub html: Option<String>,
}

#[derive(Debug)]
pub(super) struct Message {
    pub(super) speaker: String,
    pub(super) timestamp: Option<String>,
    pub(super) blocks: Vec<MessageBlock>,
}

#[derive(Debug)]
pub(super) enum MessageBlock {
    Text(String),
    Image(Option<String>),
}

pub(super) fn exported(
    session_id: String,
    title: &str,
    messages: &[Message],
    format: ExportFormat,
) -> ExportResult {
    match format {
        ExportFormat::Markdown => ExportResult {
            status: ExportStatus::Exported,
            session_id: session_id.clone(),
            message: "session exported as Markdown".to_string(),
            filename: Some(super::build_filename(title, &session_id, "md")),
            markdown: Some(super::render_markdown(title, messages)),
            html: None,
        },
        ExportFormat::Html => ExportResult {
            status: ExportStatus::Exported,
            session_id: session_id.clone(),
            message: "session exported as HTML".to_string(),
            filename: Some(super::build_filename(title, &session_id, "html")),
            markdown: None,
            html: Some(super::render_html(title, messages)),
        },
    }
}

pub(super) fn not_found(session_id: &str, message: &str) -> ExportResult {
    ExportResult {
        status: ExportStatus::NotFound,
        session_id: session_id.to_string(),
        message: message.to_string(),
        filename: None,
        markdown: None,
        html: None,
    }
}

pub(super) fn failed(session_id: &str, message: impl Into<String>) -> ExportResult {
    ExportResult {
        status: ExportStatus::Failed,
        session_id: session_id.to_string(),
        message: message.into(),
        filename: None,
        markdown: None,
        html: None,
    }
}
