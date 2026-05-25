use serde_json::{Value, json};

use crate::routes::{BridgeContext, failed, session_ref_from_payload};

pub(crate) async fn list_recycle_bin(ctx: BridgeContext) -> Value {
    let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(ctx.db_path);
    tokio::task::spawn_blocking(move || adapter.list_undo_backups())
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(|error| error.to_string()))
        .map(|result| json!({"status": "ok", "result": result}))
        .unwrap_or_else(|message| failed(message))
}

pub(crate) async fn delete_recycle_bin_entry(ctx: BridgeContext, payload: Value) -> Value {
    let Some(token) = payload.get("undo_token").and_then(Value::as_str) else {
        return failed("missing undo_token");
    };
    let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(ctx.db_path);
    let token = token.to_string();
    tokio::task::spawn_blocking(move || adapter.delete_undo_backup(&token))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(|error| error.to_string()))
        .map(|result| json!({"status": "ok", "result": result}))
        .unwrap_or_else(|message| failed(message))
}

pub(crate) async fn delete_session(ctx: BridgeContext, payload: Value) -> Value {
    let Some(session) = session_ref_from_payload(&payload) else {
        return failed("missing session id");
    };
    let log_session = session.clone();
    let delete_session = session.clone();
    let inspect_adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(ctx.db_path.clone());
    if let Ok(detail) =
        tokio::task::spawn_blocking(move || inspect_adapter.inspect_delete_local(&log_session))
            .await
    {
        if let Ok(detail) = detail {
            let _ = crate::diagnostic_log::append("session.delete.inspect", detail);
        }
    }
    let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(ctx.db_path);
    tokio::task::spawn_blocking(move || adapter.delete_local(&delete_session))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(|error| error.to_string()))
        .map(|result| {
            let _ = crate::diagnostic_log::append(
                "session.delete.result",
                json!({
                    "requested_id": session.id,
                    "normalized_id": session.normalized_id(),
                    "title": session.title,
                    "status": result.status,
                    "message": result.message,
                    "deleted_session_id": result.session_id,
                }),
            );
            json!({"status": "ok", "result": result})
        })
        .unwrap_or_else(|message| failed(message))
}

pub(crate) async fn undo_session(ctx: BridgeContext, payload: Value) -> Value {
    let Some(token) = payload.get("undo_token").and_then(Value::as_str) else {
        return failed("missing undo_token");
    };
    let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(ctx.db_path);
    let token = token.to_string();
    tokio::task::spawn_blocking(move || adapter.undo(&token))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(|error| error.to_string()))
        .map(|result| json!({"status": "ok", "result": result}))
        .unwrap_or_else(|message| failed(message))
}

pub(crate) async fn export_markdown(ctx: BridgeContext, payload: Value) -> Value {
    let Some(session) = session_ref_from_payload(&payload) else {
        return failed("missing session id");
    };
    let service = codex_pilot_data::markdown::MarkdownExportService::new(ctx.db_path);
    tokio::task::spawn_blocking(move || service.export_markdown(&session))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(|error| error.to_string()))
        .map(|result| json!({"status": "ok", "result": result}))
        .unwrap_or_else(|message| failed(message))
}

pub(crate) async fn export_html(ctx: BridgeContext, payload: Value) -> Value {
    let Some(session) = session_ref_from_payload(&payload) else {
        return failed("missing session id");
    };
    let service = codex_pilot_data::markdown::MarkdownExportService::new(ctx.db_path);
    tokio::task::spawn_blocking(move || service.export_html(&session))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(|error| error.to_string()))
        .map(|result| json!({"status": "ok", "result": result}))
        .unwrap_or_else(|message| failed(message))
}

pub(crate) async fn archived_thread(ctx: BridgeContext, payload: Value) -> Value {
    let title = payload
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if title.is_empty() {
        return failed("missing title");
    }
    let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(ctx.db_path);
    tokio::task::spawn_blocking(move || adapter.find_archived_thread_by_title(&title))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(|error| error.to_string()))
        .map(|result| {
            result.map_or_else(
                || json!({"status": "not_found", "message": "未找到归档会话"}),
                |session| json!({"status": "ok", "result": session}),
            )
        })
        .unwrap_or_else(|message| failed(message))
}

pub(crate) async fn move_thread_workspace(ctx: BridgeContext, payload: Value) -> Value {
    let Some(session) = session_ref_from_payload(&payload) else {
        return failed("missing session id");
    };
    let target_cwd = payload
        .get("target_cwd")
        .or_else(|| payload.get("targetCwd"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(ctx.db_path);
    tokio::task::spawn_blocking(move || adapter.move_codex_thread_workspace(&session, &target_cwd))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(|error| error.to_string()))
        .map(|result| json!({"status": "ok", "result": result}))
        .unwrap_or_else(|message| failed(message))
}

pub(crate) async fn thread_sort_key(ctx: BridgeContext, payload: Value) -> Value {
    let Some(session) = session_ref_from_payload(&payload) else {
        return failed("missing session id");
    };
    let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(ctx.db_path);
    tokio::task::spawn_blocking(move || adapter.codex_thread_sort_key(&session))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(|error| error.to_string()))
        .map(|result| json!({"status": "ok", "result": result}))
        .unwrap_or_else(|message| failed(message))
}

pub(crate) async fn thread_sort_keys(ctx: BridgeContext, payload: Value) -> Value {
    let sessions = payload
        .get("sessions")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(session_ref_from_payload)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(ctx.db_path);
    tokio::task::spawn_blocking(move || adapter.codex_thread_sort_keys(&sessions))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(|error| error.to_string()))
        .map(|result| json!({"status": "ok", "result": result}))
        .unwrap_or_else(|message| failed(message))
}
