pub(crate) use crate::provider_store_rules::{
    infer_effective_route, profiles_equivalent, provider_status_message, sanitize_provider_profile,
    sanitize_provider_profiles_state, unique_imported_profile_name, unique_profile_id,
};
pub(crate) use crate::provider_store_types::{
    AppliedProfileResult, AuthenticatedBehavior, BackupCandidate, CcsImportResult,
    CcsProviderSnapshot, EffectiveRoute, OfficialConfigSnapshot, OfficialSnapshotImportResult,
    OfficialSnapshotPrepareResult, ProviderApplyRequest, ProviderProfile, ProviderProfileIdRequest,
    ProviderProfileMode, ProviderProfileSaveRequest, ProviderProfileSaveResponse,
    ProviderProfilesState, ProviderSnapshot, ProviderSyncRequest, ProviderSyncSnapshot,
    default_authenticated_behavior,
};
use std::path::{Path, PathBuf};

pub(crate) fn provider_profiles_path() -> PathBuf {
    codex_pilot_core::app_paths::app_state_dir().join("provider-profiles.json")
}

pub(crate) fn latest_official_backup_candidate() -> Option<BackupCandidate> {
    let config_path = codex_pilot_core::app_paths::codex_config_path();
    let parent = config_path.parent()?;
    let entries = std::fs::read_dir(parent).ok()?;
    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            if !name.starts_with("config.toml.codex-pilot-backup-") || !name.ends_with(".bak") {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            let modified_at_ms = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(0);
            Some(BackupCandidate {
                path,
                modified_at_ms,
            })
        })
        .max_by_key(|candidate| candidate.modified_at_ms)
}

pub(crate) fn load_provider_profiles() -> ProviderProfilesState {
    load_provider_profiles_from_path(&provider_profiles_path()).unwrap_or_default()
}

pub(crate) fn load_provider_profiles_from_path(
    path: &Path,
) -> Result<ProviderProfilesState, String> {
    if !path.exists() {
        return Ok(ProviderProfilesState::default());
    }
    let contents =
        std::fs::read_to_string(path).map_err(|error| format!("读取中转配置档失败：{error}"))?;
    let state = serde_json::from_str::<ProviderProfilesState>(&contents)
        .map_err(|error| format!("解析中转配置档失败：{error}"))?;
    sanitize_provider_profiles_state(state)
}

pub(crate) fn save_provider_profiles_to_path(
    path: &Path,
    state: &ProviderProfilesState,
) -> Result<(), String> {
    let state = sanitize_provider_profiles_state(state.clone())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败：{error}"))?;
    }
    let contents = serde_json::to_string_pretty(&state)
        .map_err(|error| format!("序列化中转配置档失败：{error}"))?;
    std::fs::write(path, contents).map_err(|error| format!("写入中转配置档失败：{error}"))
}

pub(crate) fn ccs_provider_snapshot_for_state(
    state: &ProviderProfilesState,
) -> CcsProviderSnapshot {
    let db_path = codex_pilot_core::ccs_import::default_ccs_db_path();
    match codex_pilot_core::ccs_import::list_codex_providers_from_db(&db_path) {
        Ok(candidates) => {
            let available_count = candidates.len();
            let importable_count = candidates
                .iter()
                .filter(|candidate| {
                    !state.profiles.iter().any(|profile| {
                        profiles_equivalent(profile, candidate, ProviderProfileMode::Api)
                    })
                })
                .count();
            let (status, message) = if available_count == 0 {
                if db_path.exists() {
                    (
                        "empty".to_string(),
                        "未发现 CCSwitch Codex 配置。".to_string(),
                    )
                } else {
                    (
                        "missing".to_string(),
                        "未找到 CCSwitch 数据库。".to_string(),
                    )
                }
            } else {
                (
                    "ready".to_string(),
                    format!("已发现 {importable_count} 个可导入配置。"),
                )
            };
            CcsProviderSnapshot {
                db_path: db_path.to_string_lossy().to_string(),
                available_count,
                importable_count,
                status,
                message,
            }
        }
        Err(error) => CcsProviderSnapshot {
            db_path: db_path.to_string_lossy().to_string(),
            available_count: 0,
            importable_count: 0,
            status: "error".to_string(),
            message: format!("读取 CCSwitch 配置失败：{error}"),
        },
    }
}

pub(crate) fn profile_by_id(
    state: &ProviderProfilesState,
    id: Option<&str>,
) -> Result<ProviderProfile, String> {
    let id = id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&state.active_profile_id);
    state
        .profiles
        .iter()
        .find(|profile| profile.id == id)
        .or_else(|| state.profiles.first())
        .cloned()
        .ok_or_else(|| "没有可用的中转配置档。".to_string())
}

pub(crate) fn active_profile(state: &ProviderProfilesState) -> Option<&ProviderProfile> {
    state
        .profiles
        .iter()
        .find(|profile| profile.id == state.active_profile_id)
        .or_else(|| state.profiles.first())
}

pub(crate) fn capture_official_snapshot_if_missing(
    state: &mut ProviderProfilesState,
) -> Result<(), String> {
    if state.official_config_snapshot.is_some() {
        return Ok(());
    }
    let snapshot = codex_pilot_core::relay_config::capture_official_config_snapshot_from_home(
        &codex_pilot_core::app_paths::codex_home_dir(),
    )
    .map_err(|error| format!("捕获官方原版配置失败：{error}"))?;
    state.official_config_snapshot = snapshot.map(|snapshot| OfficialConfigSnapshot {
        config_toml: snapshot.config_toml,
        captured_at_ms: snapshot.captured_at_ms,
    });
    Ok(())
}

pub(crate) fn apply_active_profile(state: &ProviderProfilesState) -> Result<String, String> {
    let profile = active_profile(state)
        .cloned()
        .ok_or_else(|| "没有可用的中转配置档。".to_string())?;
    let snapshot = state.official_config_snapshot.clone();
    tauri::async_runtime::block_on(async move {
        tauri::async_runtime::spawn_blocking(move || {
            let applied = apply_profile_now(&profile, snapshot.as_ref())?;
            Ok(applied.message)
        })
        .await
        .map_err(|error| format!("应用中转配置档任务失败：{error}"))?
    })
}

pub(crate) fn apply_profile_now(
    profile: &ProviderProfile,
    official_snapshot: Option<&OfficialConfigSnapshot>,
) -> Result<AppliedProfileResult, String> {
    let auth = codex_pilot_core::relay_config::default_chatgpt_auth_status();
    let upstream_protocol = profile.upstream_protocol;
    let relay_result = || match auth.authenticated {
        true => codex_pilot_core::relay_config::apply_relay_provider_config_with_protocol(
            &profile.base_url,
            &profile.bearer_token,
            upstream_protocol,
        )
        .map_err(|error| format!("应用自动中转失败：{error}")),
        false => codex_pilot_core::relay_config::apply_api_provider_config_with_protocol(
            &profile.base_url,
            &profile.bearer_token,
            upstream_protocol,
        )
        .map_err(|error| format!("应用 API 中转失败：{error}")),
    };

    if auth.authenticated && profile.authenticated_behavior == AuthenticatedBehavior::OfficialDirect
    {
        if let Some(snapshot) = official_snapshot {
            let result =
                codex_pilot_core::relay_config::restore_official_config_snapshot_from_home(
                    &codex_pilot_core::app_paths::codex_home_dir(),
                    &codex_pilot_core::relay_config::OfficialConfigSnapshot {
                        config_toml: snapshot.config_toml.clone(),
                        captured_at_ms: snapshot.captured_at_ms,
                    },
                )
                .map_err(|error| format!("恢复官方原版配置失败：{error}"))?;
            let message = result
                .backup_path
                .map(|path| format!("已恢复官方原版配置，备份：{path}。"))
                .unwrap_or_else(|| "已恢复官方原版配置。".to_string());
            return Ok(AppliedProfileResult { message });
        }

        relay_result()?;
        return Ok(AppliedProfileResult {
            message: "未找到官方原版快照，已退化为自动中转。".to_string(),
        });
    }

    if !auth.authenticated
        && profile.authenticated_behavior == AuthenticatedBehavior::OfficialDirect
    {
        relay_result()?;
        return Ok(AppliedProfileResult {
            message: "未检测到官方登录，当前已按 API 中转应用。".to_string(),
        });
    }

    relay_result()?;
    Ok(AppliedProfileResult {
        message: if auth.authenticated {
            "已按登录态应用自动中转。".to_string()
        } else {
            "已按 API 中转应用。".to_string()
        },
    })
}
