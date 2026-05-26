use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::relay_config_auth::auth_json_chatgpt_account_label;
use crate::relay_config_toml::{
    backup_existing_config, clear_api_key_auth_json, infer_upstream_protocol_from_base_url, now_ms,
    remove_root_key, remove_table, root_key_string, table_values, unquote_toml_string,
    upsert_api_provider_config, upsert_relay_provider_config, write_pure_api_auth_json,
};

pub const RELAY_PROVIDER: &str = "CodexPilot";
pub(crate) const CHANNEL_MODE_KEY: &str = "codex_pilot_channel_mode";

pub use crate::protocol_proxy::UpstreamProtocol;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptAuthStatus {
    pub authenticated: bool,
    pub source: String,
    pub account_label: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayProviderConfig {
    pub provider: String,
    pub active: bool,
    pub mode: String,
    pub configured: bool,
    pub authenticated: bool,
    pub account_label: Option<String>,
    pub requires_openai_auth: bool,
    pub has_bearer_token: bool,
    pub base_url: Option<String>,
    pub upstream_protocol: UpstreamProtocol,
    pub config_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayApplyResult {
    pub config_path: String,
    pub backup_path: Option<String>,
    pub configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialConfigSnapshot {
    pub config_toml: String,
    pub captured_at_ms: u64,
}

pub fn default_relay_provider_config() -> RelayProviderConfig {
    relay_provider_config_from_home(&codex_home_dir())
}

pub fn relay_provider_config_from_home(home: &Path) -> RelayProviderConfig {
    let auth = chatgpt_auth_status_from_home(home);
    relay_provider_config_from_path_and_auth(&home.join("config.toml"), auth)
}

pub fn relay_provider_config_from_path(config_path: &Path) -> RelayProviderConfig {
    relay_provider_config_from_path_and_auth(
        config_path,
        ChatGptAuthStatus {
            authenticated: false,
            source: String::new(),
            account_label: None,
            message: "未检查 ChatGPT 登录状态。".to_string(),
        },
    )
}

fn relay_provider_config_from_path_and_auth(
    config_path: &Path,
    auth: ChatGptAuthStatus,
) -> RelayProviderConfig {
    let contents = std::fs::read_to_string(config_path).unwrap_or_default();
    relay_provider_config_from_contents(&contents, config_path, auth)
}

pub fn default_chatgpt_auth_status() -> ChatGptAuthStatus {
    chatgpt_auth_status_from_home(&codex_home_dir())
}

pub fn chatgpt_auth_status_from_home(home: &Path) -> ChatGptAuthStatus {
    let auth_path = home.join("auth.json");
    if let Some(account_label) = auth_json_chatgpt_account_label(&auth_path) {
        return ChatGptAuthStatus {
            authenticated: true,
            source: auth_path.to_string_lossy().to_string(),
            account_label,
            message: "已检测到 ChatGPT 登录状态。".to_string(),
        };
    }

    ChatGptAuthStatus {
        authenticated: false,
        source: String::new(),
        account_label: None,
        message: "未检测到 ChatGPT 登录状态，请先在 Codex 中完成官方登录。".to_string(),
    }
}

pub fn apply_relay_provider_config(
    base_url: &str,
    bearer_token: &str,
) -> anyhow::Result<RelayApplyResult> {
    apply_relay_provider_config_with_protocol(base_url, bearer_token, UpstreamProtocol::Responses)
}

pub fn apply_relay_provider_config_to_home(
    home: &Path,
    base_url: &str,
    bearer_token: &str,
) -> anyhow::Result<RelayApplyResult> {
    apply_relay_provider_config_to_home_with_protocol(
        home,
        base_url,
        bearer_token,
        UpstreamProtocol::Responses,
    )
}

pub fn apply_relay_provider_config_with_protocol(
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> anyhow::Result<RelayApplyResult> {
    apply_relay_provider_config_to_home_with_protocol(
        &codex_home_dir(),
        base_url,
        bearer_token,
        upstream_protocol,
    )
}

pub fn apply_relay_provider_config_to_home_with_protocol(
    home: &Path,
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> anyhow::Result<RelayApplyResult> {
    let auth = chatgpt_auth_status_from_home(home);
    if !auth.authenticated {
        anyhow::bail!("{}", auth.message);
    }
    apply_relay_provider_config_to_path_with_protocol(
        &home.join("config.toml"),
        base_url,
        bearer_token,
        upstream_protocol,
    )
}

pub fn apply_api_provider_config(
    base_url: &str,
    bearer_token: &str,
) -> anyhow::Result<RelayApplyResult> {
    apply_api_provider_config_with_protocol(base_url, bearer_token, UpstreamProtocol::Responses)
}

pub fn apply_api_provider_config_with_protocol(
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> anyhow::Result<RelayApplyResult> {
    apply_api_provider_config_to_home_with_protocol(
        &crate::app_paths::codex_home_dir(),
        base_url,
        bearer_token,
        upstream_protocol,
    )
}

pub fn apply_api_provider_config_to_home_with_protocol(
    home: &Path,
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> anyhow::Result<RelayApplyResult> {
    write_pure_api_auth_json(home, bearer_token)?;
    apply_api_provider_config_to_path_with_protocol(
        &home.join("config.toml"),
        base_url,
        bearer_token,
        upstream_protocol,
    )
}

pub fn apply_relay_provider_config_to_path(
    config_path: &Path,
    base_url: &str,
    bearer_token: &str,
) -> anyhow::Result<RelayApplyResult> {
    apply_relay_provider_config_to_path_with_protocol(
        config_path,
        base_url,
        bearer_token,
        UpstreamProtocol::Responses,
    )
}

pub fn apply_relay_provider_config_to_path_with_protocol(
    config_path: &Path,
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> anyhow::Result<RelayApplyResult> {
    let base_url = base_url.trim();
    if base_url.is_empty() {
        anyhow::bail!("relay Base URL cannot be empty");
    }
    let bearer_token = bearer_token.trim();
    if bearer_token.is_empty() {
        anyhow::bail!("relay bearer token cannot be empty");
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let existing = std::fs::read_to_string(config_path).unwrap_or_default();
    let backup_path = backup_existing_config(config_path, &existing)?;
    let codex_base_url = crate::protocol_proxy::proxy_base_url_for_protocol(
        base_url,
        upstream_protocol,
        crate::protocol_proxy::DEFAULT_PROTOCOL_PROXY_PORT,
    );
    let updated =
        upsert_relay_provider_config(&existing, &codex_base_url, bearer_token, upstream_protocol);
    std::fs::write(config_path, updated)
        .with_context(|| format!("failed to write {}", config_path.display()))?;

    let status = relay_provider_config_from_path(config_path);
    Ok(RelayApplyResult {
        config_path: status.config_path,
        backup_path: backup_path.map(|path| path.to_string_lossy().to_string()),
        configured: status.configured,
    })
}

pub fn apply_api_provider_config_to_path(
    config_path: &Path,
    base_url: &str,
    bearer_token: &str,
) -> anyhow::Result<RelayApplyResult> {
    apply_api_provider_config_to_path_with_protocol(
        config_path,
        base_url,
        bearer_token,
        UpstreamProtocol::Responses,
    )
}

pub fn apply_api_provider_config_to_path_with_protocol(
    config_path: &Path,
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> anyhow::Result<RelayApplyResult> {
    let base_url = base_url.trim();
    if base_url.is_empty() {
        anyhow::bail!("API Base URL cannot be empty");
    }
    let bearer_token = bearer_token.trim();
    if bearer_token.is_empty() {
        anyhow::bail!("API key cannot be empty");
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let existing = std::fs::read_to_string(config_path).unwrap_or_default();
    let backup_path = backup_existing_config(config_path, &existing)?;
    let codex_base_url = crate::protocol_proxy::proxy_base_url_for_protocol(
        base_url,
        upstream_protocol,
        crate::protocol_proxy::DEFAULT_PROTOCOL_PROXY_PORT,
    );
    let updated =
        upsert_api_provider_config(&existing, &codex_base_url, bearer_token, upstream_protocol);
    std::fs::write(config_path, updated)
        .with_context(|| format!("failed to write {}", config_path.display()))?;

    let status = relay_provider_config_from_path(config_path);
    Ok(RelayApplyResult {
        config_path: status.config_path,
        backup_path: backup_path.map(|path| path.to_string_lossy().to_string()),
        configured: status.configured,
    })
}

pub fn clear_relay_provider_config() -> anyhow::Result<RelayApplyResult> {
    clear_relay_provider_config_from_path(&crate::app_paths::codex_config_path())
}

pub fn clear_relay_provider_config_from_home(home: &Path) -> anyhow::Result<RelayApplyResult> {
    clear_relay_provider_config_from_path(&home.join("config.toml"))
}

pub fn capture_official_config_snapshot_from_home(
    home: &Path,
) -> anyhow::Result<Option<OfficialConfigSnapshot>> {
    let config_path = home.join("config.toml");
    let current = relay_provider_config_from_home(home);
    if current.active {
        return Ok(None);
    }
    let config_toml = std::fs::read_to_string(&config_path).unwrap_or_default();
    Ok(Some(OfficialConfigSnapshot {
        config_toml,
        captured_at_ms: now_ms() as u64,
    }))
}

pub fn restore_official_config_snapshot_from_home(
    home: &Path,
    snapshot: &OfficialConfigSnapshot,
) -> anyhow::Result<RelayApplyResult> {
    restore_official_config_snapshot_to_path(&home.join("config.toml"), snapshot)
}

pub fn restore_official_config_snapshot_to_path(
    config_path: &Path,
    snapshot: &OfficialConfigSnapshot,
) -> anyhow::Result<RelayApplyResult> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let existing = std::fs::read_to_string(config_path).unwrap_or_default();
    let backup_path = backup_existing_config(config_path, &existing)?;
    std::fs::write(config_path, &snapshot.config_toml)
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    let _ = clear_api_key_auth_json(
        &config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("auth.json"),
    );

    let status = relay_provider_config_from_path(config_path);
    Ok(RelayApplyResult {
        config_path: status.config_path,
        backup_path: backup_path.map(|path| path.to_string_lossy().to_string()),
        configured: status.configured,
    })
}

pub fn clear_relay_provider_config_from_path(
    config_path: &Path,
) -> anyhow::Result<RelayApplyResult> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let existing = std::fs::read_to_string(config_path).unwrap_or_default();
    let backup_path = backup_existing_config(config_path, &existing)?;
    let without_relay = remove_table(&existing, &format!("model_providers.{RELAY_PROVIDER}"));
    let without_key = remove_root_key(&without_relay, "OPENAI_API_KEY");
    let updated = remove_root_key(&without_key, "model_provider");
    std::fs::write(config_path, updated)
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    let _ = clear_api_key_auth_json(
        &config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("auth.json"),
    );

    let status = relay_provider_config_from_path(config_path);
    Ok(RelayApplyResult {
        config_path: status.config_path,
        backup_path: backup_path.map(|path| path.to_string_lossy().to_string()),
        configured: status.configured,
    })
}

pub(crate) fn relay_provider_config_from_contents(
    contents: &str,
    config_path: &Path,
    auth: ChatGptAuthStatus,
) -> RelayProviderConfig {
    let active = root_key_string(contents, "model_provider")
        .map(|value| value == RELAY_PROVIDER)
        .unwrap_or(false);
    let provider = table_values(contents, &format!("model_providers.{RELAY_PROVIDER}"));
    let requires_openai_auth = provider
        .as_ref()
        .and_then(|values| values.get("requires_openai_auth"))
        .map(|value| value.trim() == "true")
        .unwrap_or(false);
    let has_bearer_token = provider
        .as_ref()
        .and_then(|values| values.get("experimental_bearer_token"))
        .map(|value| !unquote_toml_string(value).trim().is_empty())
        .unwrap_or(false)
        || root_key_string(contents, "OPENAI_API_KEY")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
    let has_experimental_bearer_token = provider
        .as_ref()
        .and_then(|values| values.get("experimental_bearer_token"))
        .map(|value| !unquote_toml_string(value).trim().is_empty())
        .unwrap_or(false);
    let base_url = provider
        .as_ref()
        .and_then(|values| values.get("base_url"))
        .map(|value| unquote_toml_string(value))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let stored_mode = provider
        .as_ref()
        .and_then(|values| values.get(CHANNEL_MODE_KEY))
        .map(|value| unquote_toml_string(value))
        .filter(|value| !value.trim().is_empty());
    let upstream_protocol = base_url
        .as_deref()
        .map(infer_upstream_protocol_from_base_url)
        .unwrap_or_default();
    let mode = if active && stored_mode.as_deref() == Some("api") {
        "api"
    } else if active && stored_mode.as_deref() == Some("hybridApi") {
        "hybridApi"
    } else if active && requires_openai_auth {
        "hybridApi"
    } else if active && base_url.is_some() && has_bearer_token {
        "api"
    } else {
        "official"
    };
    let configured = active
        && base_url.is_some()
        && ((requires_openai_auth && has_experimental_bearer_token)
            || (!requires_openai_auth && has_bearer_token));

    RelayProviderConfig {
        provider: RELAY_PROVIDER.to_string(),
        active,
        mode: mode.to_string(),
        configured,
        authenticated: auth.authenticated,
        account_label: auth.account_label,
        requires_openai_auth,
        has_bearer_token,
        base_url,
        upstream_protocol,
        config_path: config_path.to_string_lossy().to_string(),
    }
}

fn codex_home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(".codex"))
        .unwrap_or_else(|| PathBuf::from(".codex"))
}

impl UpstreamProtocol {
    pub(crate) fn as_config_value(self) -> &'static str {
        match self {
            UpstreamProtocol::Responses => "responses",
            UpstreamProtocol::ChatCompletions => "chat_completions",
            UpstreamProtocol::AnthropicMessages => "anthropic_messages",
        }
    }
}

#[cfg(test)]
#[path = "relay_config_tests.rs"]
mod tests;
