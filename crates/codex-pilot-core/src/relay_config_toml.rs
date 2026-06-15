use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use serde_json::Value;

use crate::protocol_proxy::UpstreamProtocol;
use crate::relay_config::{CHANNEL_MODE_KEY, RELAY_PROVIDER};

pub(crate) fn backup_existing_config(
    config_path: &Path,
    existing: &str,
) -> anyhow::Result<Option<PathBuf>> {
    if !config_path.exists() {
        return Ok(None);
    }
    let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
    let backup_path = parent.join(format!("config.toml.codex-pilot-backup-{}.bak", now_ms()));
    std::fs::write(&backup_path, existing)
        .with_context(|| format!("failed to write {}", backup_path.display()))?;
    Ok(Some(backup_path))
}

pub(crate) fn upsert_relay_provider_config(
    contents: &str,
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> String {
    let mut updated = upsert_root_keys(
        contents,
        &[(
            "model_provider",
            format!("\"{}\"", toml_escape(RELAY_PROVIDER)),
        )],
    );
    updated = remove_table(&updated, &format!("model_providers.{RELAY_PROVIDER}"));
    updated = remove_root_key(&updated, "OPENAI_API_KEY");

    let mut lines = updated.lines().map(ToString::to_string).collect::<Vec<_>>();
    let insert_at = first_non_provider_table_index(&lines).unwrap_or(lines.len());
    let provider_lines = vec![
        format!("[model_providers.{RELAY_PROVIDER}]"),
        format!("name = \"{}\"", toml_escape(RELAY_PROVIDER)),
        "wire_api = \"responses\"".to_string(),
        "requires_openai_auth = true".to_string(),
        format!("base_url = \"{}\"", toml_escape(base_url)),
        format!("{CHANNEL_MODE_KEY} = \"hybrid\""),
        format!(
            "codex_pilot_upstream_protocol = \"{}\"",
            upstream_protocol.as_config_value()
        ),
        format!(
            "experimental_bearer_token = \"{}\"",
            toml_escape(bearer_token)
        ),
        String::new(),
    ];
    lines.splice(insert_at..insert_at, provider_lines);
    finish_lines(lines)
}

pub(crate) fn upsert_api_provider_config(
    contents: &str,
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> String {
    let mut updated = upsert_root_keys(
        contents,
        &[(
            "model_provider",
            format!("\"{}\"", toml_escape(RELAY_PROVIDER)),
        )],
    );
    updated = remove_table(&updated, &format!("model_providers.{RELAY_PROVIDER}"));
    updated = remove_root_key(&updated, "OPENAI_API_KEY");

    let mut lines = updated.lines().map(ToString::to_string).collect::<Vec<_>>();
    let insert_at = first_non_provider_table_index(&lines).unwrap_or(lines.len());
    let provider_lines = vec![
        format!("[model_providers.{RELAY_PROVIDER}]"),
        format!("name = \"{}\"", toml_escape(RELAY_PROVIDER)),
        "wire_api = \"responses\"".to_string(),
        "requires_openai_auth = true".to_string(),
        format!("base_url = \"{}\"", toml_escape(base_url)),
        format!("{CHANNEL_MODE_KEY} = \"api\""),
        format!(
            "codex_pilot_upstream_protocol = \"{}\"",
            upstream_protocol.as_config_value()
        ),
        format!(
            "experimental_bearer_token = \"{}\"",
            toml_escape(bearer_token)
        ),
        String::new(),
    ];
    lines.splice(insert_at..insert_at, provider_lines);
    finish_lines(lines)
}

pub(crate) fn write_pure_api_auth_json(home: &Path, bearer_token: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(home)
        .with_context(|| format!("failed to create {}", home.display()))?;
    let auth_path = home.join("auth.json");
    let value = serde_json::json!({
        "OPENAI_API_KEY": bearer_token.trim()
    });
    std::fs::write(&auth_path, serde_json::to_vec_pretty(&value)?)
        .with_context(|| format!("failed to write {}", auth_path.display()))?;
    Ok(())
}

pub(crate) fn clear_api_key_auth_json(auth_path: &Path) -> anyhow::Result<()> {
    if !auth_path.exists() {
        return Ok(());
    }
    let existing = std::fs::read_to_string(auth_path)
        .with_context(|| format!("failed to read {}", auth_path.display()))?;
    let Ok(mut value) = serde_json::from_str::<Value>(&existing) else {
        return Ok(());
    };
    let Some(object) = value.as_object_mut() else {
        return Ok(());
    };
    if object.remove("OPENAI_API_KEY").is_none() {
        return Ok(());
    }
    std::fs::write(auth_path, serde_json::to_vec_pretty(&value)?)
        .with_context(|| format!("failed to write {}", auth_path.display()))?;
    Ok(())
}

pub(crate) fn root_key_string(contents: &str, key: &str) -> Option<String> {
    root_key_value(contents, key).map(unquote_toml_string)
}

fn root_key_value<'a>(contents: &'a str, key: &str) -> Option<&'a str> {
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            return None;
        }
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        if name.trim() == key {
            return Some(value);
        }
    }
    None
}

fn upsert_root_keys(contents: &str, entries: &[(&str, String)]) -> String {
    let mut lines = contents
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let root_end = lines
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .unwrap_or(lines.len());

    for (key, value) in entries {
        if let Some(index) = lines[..root_end]
            .iter()
            .position(|line| root_line_key(line) == Some(*key))
        {
            lines[index] = format!("{key} = {value}");
        } else {
            lines.insert(root_end, format!("{key} = {value}"));
        }
    }

    finish_lines(lines)
}

pub(crate) fn remove_table(contents: &str, table: &str) -> String {
    let header = format!("[{table}]");
    let mut lines = Vec::new();
    let mut skipping = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if trimmed == header {
                skipping = true;
                continue;
            }
            skipping = false;
        }
        if !skipping {
            lines.push(line.to_string());
        }
    }
    lines.join("\n")
}

pub(crate) fn remove_root_key(contents: &str, key: &str) -> String {
    let mut lines = Vec::new();
    let mut in_root = true;
    for line in contents.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_root = false;
        }
        if in_root && root_line_key(line) == Some(key) {
            continue;
        }
        lines.push(line.to_string());
    }
    lines.join("\n")
}

fn first_non_provider_table_index(lines: &[String]) -> Option<usize> {
    lines.iter().position(|line| {
        let trimmed = line.trim();
        trimmed.starts_with('[') && !trimmed.starts_with("[model_providers.")
    })
}

pub(crate) fn table_values(contents: &str, table: &str) -> Option<HashMap<String, String>> {
    let header = format!("[{table}]");
    let mut in_table = false;
    let mut values = HashMap::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_table {
                break;
            }
            in_table = trimmed == header;
            continue;
        }
        if !in_table || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            values.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    in_table.then_some(values)
}

pub(crate) fn unquote_toml_string(value: &str) -> String {
    let value = value.trim();
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .to_string()
}

fn root_line_key(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') || trimmed.starts_with('[') {
        return None;
    }
    trimmed.split_once('=').map(|(key, _)| key.trim())
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(crate) fn infer_upstream_protocol_from_base_url(base_url: &str) -> UpstreamProtocol {
    if base_url.starts_with("http://127.0.0.1:") || base_url.starts_with("http://localhost:") {
        return UpstreamProtocol::ChatCompletions;
    }
    UpstreamProtocol::Responses
}

fn finish_lines(lines: Vec<String>) -> String {
    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

pub(crate) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
