use crate::provider_sync::filesystem::to_desktop_workspace_path;
use crate::provider_sync::models::SessionChange;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const SESSION_DIRS: [&str; 2] = ["sessions", "archived_sessions"];

pub(super) fn collect_session_changes(
    home: &Path,
    target_provider: &str,
) -> anyhow::Result<Vec<SessionChange>> {
    let mut changes = Vec::new();
    for path in rollout_files(home)? {
        let text = fs::read_to_string(&path)?;
        let (first_line, separator) = split_first_line(&text);
        if first_line.trim().is_empty() {
            continue;
        }
        let Ok(mut record) = serde_json::from_str::<Value>(&first_line) else {
            continue;
        };
        let Some(payload) = record.get_mut("payload").and_then(Value::as_object_mut) else {
            continue;
        };
        let thread_id = payload
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let cwd = payload
            .get("cwd")
            .and_then(Value::as_str)
            .and_then(to_desktop_workspace_path);
        let has_user_event =
            separator.contains("\"user_message\"") || separator.contains("\"user_input\"");
        let rewrite_needed =
            payload.get("model_provider").and_then(Value::as_str) != Some(target_provider);
        if rewrite_needed {
            payload.insert("model_provider".to_string(), json!(target_provider));
        }
        let next_first_line = if rewrite_needed {
            serde_json::to_string(&record)?
        } else {
            first_line.clone()
        };
        changes.push(SessionChange {
            path,
            original_first_line: first_line,
            next_first_line,
            separator,
            thread_id,
            cwd,
            has_user_event,
            rewrite_needed,
        });
    }
    Ok(changes)
}

pub(super) fn rollout_provider_from_first_line(first_line: &str) -> Option<String> {
    let record = serde_json::from_str::<Value>(first_line).ok()?;
    record
        .get("payload")
        .and_then(Value::as_object)
        .and_then(|payload| payload.get("model_provider"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn rollout_files(home: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for dirname in SESSION_DIRS {
        let root = home.join(dirname);
        if root.exists() {
            collect_rollout_files(&root, &mut files)?;
        }
    }
    files.sort();
    Ok(files)
}

fn collect_rollout_files(root: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rollout_files(&path, files)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn split_first_line(text: &str) -> (String, String) {
    if let Some(index) = text.find('\n') {
        (text[..index].to_string(), text[index..].to_string())
    } else {
        (text.to_string(), String::new())
    }
}

pub(super) fn rollout_provider_from_path(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let first_line = text.lines().next()?;
    rollout_provider_from_first_line(first_line)
}
