use super::super::*;
use codex_pilot_core::protocol_proxy::UpstreamProtocol;
use codex_pilot_core::provider_txn::{self, ProviderMode, ProviderTxn};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── 数据模型 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub bearer_token: String,
    pub upstream_protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfilesStore {
    pub active_profile_id: String,
    pub profiles: Vec<ProviderProfile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_config_snapshot: Option<OfficialConfigSnapshotData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialConfigSnapshotData {
    pub config_toml: String,
    pub captured_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    pub mode: Option<String>,
    pub owned_by_codex_pilot: bool,
    pub external_provider: bool,
    pub chatgpt_authenticated: bool,
    pub chatgpt_account_label: Option<String>,
    pub official_snapshot_available: bool,
    pub profiles: Vec<ProviderProfile>,
    pub active_profile_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchProviderModeRequest {
    pub mode: String,
    #[serde(default)]
    pub profile_id: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub upstream_protocol: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProviderProfileRequest {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub bearer_token: String,
    pub upstream_protocol: String,
}

// ── helpers ──

fn provider_profiles_path() -> PathBuf {
    codex_pilot_core::app_paths::app_state_dir().join("provider-profiles.json")
}

fn load_provider_profiles() -> ProviderProfilesStore {
    let path = provider_profiles_path();
    if !path.exists() {
        return ProviderProfilesStore::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn save_provider_profiles(store: &ProviderProfilesStore) -> Result<(), String> {
    let path = provider_profiles_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
    }
    let json = serde_json::to_vec_pretty(store).map_err(|e| format!("序列化失败：{e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("写入失败：{e}"))?;
    Ok(())
}

fn parse_upstream_protocol(value: &str) -> UpstreamProtocol {
    match value {
        "chatCompletions" => UpstreamProtocol::ChatCompletions,
        "anthropicMessages" => UpstreamProtocol::AnthropicMessages,
        _ => UpstreamProtocol::Responses,
    }
}

fn upstream_protocol_str(protocol: UpstreamProtocol) -> &'static str {
    match protocol {
        UpstreamProtocol::Responses => "responses",
        UpstreamProtocol::ChatCompletions => "chatCompletions",
        UpstreamProtocol::AnthropicMessages => "anthropicMessages",
    }
}

fn mode_str(mode: Option<ProviderMode>) -> Option<String> {
    mode.map(|m| m.sentinel_value().to_string())
}

fn chatgpt_auth_status() -> (bool, Option<String>) {
    let home = codex_pilot_core::app_paths::codex_home_dir();
    let auth = codex_pilot_core::relay_config::chatgpt_auth_status_from_home(&home);
    (auth.authenticated, auth.account_label)
}

fn has_official_snapshot() -> bool {
    let store = load_provider_profiles();
    store
        .official_config_snapshot
        .as_ref()
        .map(|s| !s.config_toml.is_empty())
        .unwrap_or(false)
}

fn capture_official_snapshot_if_external() -> Result<(), String> {
    let reading =
        provider_txn::read_current_mode().map_err(|e| format!("读取当前模式失败：{e}"))?;
    // 仅当非 CodexPilot 托管时才捕获（获得真·官方/外部基线）
    if reading.owned_by_codex_pilot {
        return Ok(());
    }
    let home = codex_pilot_core::app_paths::codex_home_dir();
    let config_path = home.join("config.toml");
    let config_toml = if config_path.exists() {
        std::fs::read_to_string(&config_path).unwrap_or_default()
    } else {
        String::new()
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let mut store = load_provider_profiles();
    store.official_config_snapshot = Some(OfficialConfigSnapshotData {
        config_toml,
        captured_at_ms: now_ms,
    });
    save_provider_profiles(&store)?;
    Ok(())
}

// ── Tauri 命令 ──

#[tauri::command]
pub(crate) async fn provider_snapshot() -> Result<ProviderSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let reading = provider_txn::read_current_mode().map_err(|e| e.to_string())?;
        let (chatgpt_authenticated, chatgpt_account_label) = chatgpt_auth_status();
        let store = load_provider_profiles();
        let official_snapshot_available = has_official_snapshot();
        Ok(ProviderSnapshot {
            mode: mode_str(reading.mode),
            owned_by_codex_pilot: reading.owned_by_codex_pilot,
            external_provider: reading.external_provider,
            chatgpt_authenticated,
            chatgpt_account_label,
            official_snapshot_available,
            profiles: store.profiles,
            active_profile_id: store.active_profile_id,
        })
    })
    .await
    .map_err(|e| format!("provider_snapshot 任务失败：{e}"))?
}

#[tauri::command]
pub(crate) async fn switch_provider_mode(
    request: SwitchProviderModeRequest,
) -> Result<ProviderSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mode = match request.mode.as_str() {
            "official" => ProviderMode::Official,
            "hybrid" => ProviderMode::Hybrid,
            "api" => ProviderMode::Api,
            _ => return Err(format!("未知模式：{}", request.mode)),
        };

        // 如果是 hybrid 或 api 且未传 base_url/api_key，从 profile 取
        let (base_url, api_key, protocol) =
            if matches!(mode, ProviderMode::Hybrid | ProviderMode::Api) {
                if !request.profile_id.is_empty() {
                    let store = load_provider_profiles();
                    let profile = store
                        .profiles
                        .iter()
                        .find(|p| p.id == request.profile_id)
                        .ok_or_else(|| format!("未找到 profile：{}", request.profile_id))?;
                    (
                        profile.base_url.clone(),
                        profile.bearer_token.clone(),
                        parse_upstream_protocol(&profile.upstream_protocol),
                    )
                } else {
                    (
                        request.base_url.clone(),
                        request.api_key.clone(),
                        parse_upstream_protocol(&request.upstream_protocol),
                    )
                }
            } else {
                (String::new(), String::new(), UpstreamProtocol::Responses)
            };

        let txn = ProviderTxn::begin().map_err(|e| e.to_string())?;
        let _result = match mode {
            ProviderMode::Official => txn.commit_official().map_err(|e| e.to_string())?,
            ProviderMode::Hybrid => txn
                .commit_hybrid(&base_url, &api_key, protocol)
                .map_err(|e| e.to_string())?,
            ProviderMode::Api => txn
                .commit_api(&base_url, &api_key, protocol)
                .map_err(|e| e.to_string())?,
        };

        // 切换后重新读快照
        let reading = provider_txn::read_current_mode().map_err(|e| e.to_string())?;
        let (chatgpt_authenticated, chatgpt_account_label) = chatgpt_auth_status();
        let store = load_provider_profiles();
        Ok(ProviderSnapshot {
            mode: mode_str(reading.mode),
            owned_by_codex_pilot: reading.owned_by_codex_pilot,
            external_provider: reading.external_provider,
            chatgpt_authenticated,
            chatgpt_account_label,
            official_snapshot_available: has_official_snapshot(),
            profiles: store.profiles,
            active_profile_id: store.active_profile_id,
        })
    })
    .await
    .map_err(|e| format!("switch_provider_mode 任务失败：{e}"))?
}

#[tauri::command]
pub(crate) async fn save_provider_profile(
    request: SaveProviderProfileRequest,
) -> Result<ProviderSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // 保存前尝试捕获官方快照（仅非托管时）
        let _ = capture_official_snapshot_if_external();

        let mut store = load_provider_profiles();
        let existing_idx = store.profiles.iter().position(|p| p.id == request.id);

        let profile = ProviderProfile {
            id: request.id,
            name: request.name,
            base_url: request.base_url,
            bearer_token: request.bearer_token,
            upstream_protocol: request.upstream_protocol,
        };

        if let Some(idx) = existing_idx {
            store.profiles[idx] = profile;
        } else {
            store.profiles.push(profile);
        }

        save_provider_profiles(&store)?;

        let reading = provider_txn::read_current_mode().map_err(|e| e.to_string())?;
        let (chatgpt_authenticated, chatgpt_account_label) = chatgpt_auth_status();
        Ok(ProviderSnapshot {
            mode: mode_str(reading.mode),
            owned_by_codex_pilot: reading.owned_by_codex_pilot,
            external_provider: reading.external_provider,
            chatgpt_authenticated,
            chatgpt_account_label,
            official_snapshot_available: has_official_snapshot(),
            profiles: store.profiles,
            active_profile_id: store.active_profile_id,
        })
    })
    .await
    .map_err(|e| format!("save_provider_profile 任务失败：{e}"))?
}

#[tauri::command]
pub(crate) async fn activate_provider_profile(
    profile_id: String,
) -> Result<ProviderSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut store = load_provider_profiles();
        if !store.profiles.iter().any(|p| p.id == profile_id) {
            return Err(format!("未找到 profile：{profile_id}"));
        }
        store.active_profile_id = profile_id;
        save_provider_profiles(&store)?;

        let reading = provider_txn::read_current_mode().map_err(|e| e.to_string())?;
        let (chatgpt_authenticated, chatgpt_account_label) = chatgpt_auth_status();
        Ok(ProviderSnapshot {
            mode: mode_str(reading.mode),
            owned_by_codex_pilot: reading.owned_by_codex_pilot,
            external_provider: reading.external_provider,
            chatgpt_authenticated,
            chatgpt_account_label,
            official_snapshot_available: has_official_snapshot(),
            profiles: store.profiles,
            active_profile_id: store.active_profile_id,
        })
    })
    .await
    .map_err(|e| format!("activate_provider_profile 任务失败：{e}"))?
}

#[tauri::command]
pub(crate) async fn delete_provider_profile(
    profile_id: String,
) -> Result<ProviderSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut store = load_provider_profiles();
        store.profiles.retain(|p| p.id != profile_id);
        if store.active_profile_id == profile_id {
            store.active_profile_id = store
                .profiles
                .first()
                .map(|p| p.id.clone())
                .unwrap_or_default();
        }
        save_provider_profiles(&store)?;

        let reading = provider_txn::read_current_mode().map_err(|e| e.to_string())?;
        let (chatgpt_authenticated, chatgpt_account_label) = chatgpt_auth_status();
        Ok(ProviderSnapshot {
            mode: mode_str(reading.mode),
            owned_by_codex_pilot: reading.owned_by_codex_pilot,
            external_provider: reading.external_provider,
            chatgpt_authenticated,
            chatgpt_account_label,
            official_snapshot_available: has_official_snapshot(),
            profiles: store.profiles,
            active_profile_id: store.active_profile_id,
        })
    })
    .await
    .map_err(|e| format!("delete_provider_profile 任务失败：{e}"))?
}
