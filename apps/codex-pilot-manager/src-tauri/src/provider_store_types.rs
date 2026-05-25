use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderSnapshot {
    pub(crate) active_provider: String,
    pub(crate) mode: String,
    pub(crate) profile: String,
    pub(crate) source: String,
    pub(crate) auth_path: String,
    pub(crate) configured: bool,
    pub(crate) authenticated: bool,
    pub(crate) account_label: Option<String>,
    pub(crate) route_label: String,
    pub(crate) status_message: String,
    pub(crate) degraded: bool,
    pub(crate) official_snapshot_available: bool,
    pub(crate) backup_snapshot_available: bool,
    pub(crate) profiles: Vec<ProviderProfile>,
    pub(crate) active_profile_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CcsProviderSnapshot {
    pub(crate) db_path: String,
    pub(crate) available_count: usize,
    pub(crate) importable_count: usize,
    pub(crate) status: String,
    pub(crate) message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CcsImportResult {
    pub(crate) imported_count: usize,
    pub(crate) skipped_count: usize,
    pub(crate) renamed_count: usize,
    pub(crate) provider: ProviderSnapshot,
    pub(crate) ccs: CcsProviderSnapshot,
    pub(crate) message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderSyncSnapshot {
    pub(crate) target_provider: String,
    pub(crate) current_provider: String,
    pub(crate) available_providers: Vec<String>,
    pub(crate) rollout_files: usize,
    pub(crate) rollout_rewrite_needed: usize,
    pub(crate) sqlite_rows: usize,
    pub(crate) sqlite_provider_rows_needing_sync: usize,
    pub(crate) sqlite_total_updates_needed: usize,
    pub(crate) rollout_providers: Vec<codex_pilot_data::provider_sync::ProviderCount>,
    pub(crate) sqlite_providers: Vec<codex_pilot_data::provider_sync::ProviderCount>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderApplyRequest {
    pub(crate) profile_id: Option<String>,
    pub(crate) mode: Option<ProviderProfileMode>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProviderProfileMode {
    HybridApi,
    Api,
}

impl ProviderProfileMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ProviderProfileMode::HybridApi => "混合中转",
            ProviderProfileMode::Api => "API 中转",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AuthenticatedBehavior {
    Relay,
    OfficialDirect,
}

impl AuthenticatedBehavior {}

pub(crate) fn default_provider_profile_mode() -> ProviderProfileMode {
    ProviderProfileMode::HybridApi
}

pub(crate) fn default_authenticated_behavior() -> AuthenticatedBehavior {
    AuthenticatedBehavior::Relay
}

pub(crate) fn default_upstream_protocol() -> codex_pilot_core::protocol_proxy::UpstreamProtocol {
    codex_pilot_core::protocol_proxy::UpstreamProtocol::Responses
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderProfile {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) base_url: String,
    pub(crate) bearer_token: String,
    #[serde(default = "default_provider_profile_mode")]
    pub(crate) mode: ProviderProfileMode,
    #[serde(default = "default_upstream_protocol")]
    pub(crate) upstream_protocol: codex_pilot_core::protocol_proxy::UpstreamProtocol,
    #[serde(default = "default_authenticated_behavior")]
    pub(crate) authenticated_behavior: AuthenticatedBehavior,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OfficialConfigSnapshot {
    pub(crate) config_toml: String,
    pub(crate) captured_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderProfilesState {
    pub(crate) active_profile_id: String,
    pub(crate) official_config_snapshot: Option<OfficialConfigSnapshot>,
    pub(crate) profiles: Vec<ProviderProfile>,
}

impl Default for ProviderProfilesState {
    fn default() -> Self {
        Self {
            active_profile_id: "default".to_string(),
            official_config_snapshot: None,
            profiles: vec![ProviderProfile {
                id: "default".to_string(),
                name: "默认中转".to_string(),
                base_url: String::new(),
                bearer_token: String::new(),
                mode: ProviderProfileMode::HybridApi,
                upstream_protocol: default_upstream_protocol(),
                authenticated_behavior: default_authenticated_behavior(),
            }],
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderProfileSaveRequest {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) base_url: String,
    pub(crate) bearer_token: String,
    pub(crate) mode: ProviderProfileMode,
    #[serde(default = "default_upstream_protocol")]
    pub(crate) upstream_protocol: codex_pilot_core::protocol_proxy::UpstreamProtocol,
    #[serde(default = "default_authenticated_behavior")]
    pub(crate) authenticated_behavior: AuthenticatedBehavior,
    pub(crate) activate: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderProfileSaveResponse {
    pub(crate) id: String,
    pub(crate) message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OfficialSnapshotImportResult {
    pub(crate) message: String,
    pub(crate) provider: ProviderSnapshot,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OfficialSnapshotPrepareResult {
    pub(crate) message: String,
    pub(crate) provider: ProviderSnapshot,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderProfileIdRequest {
    pub(crate) id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EffectiveRoute {
    OfficialDirect,
    RelayAuthenticated,
    RelayApi,
    DegradedRelay,
}

impl EffectiveRoute {
    pub(crate) fn label(self) -> &'static str {
        match self {
            EffectiveRoute::OfficialDirect => "官方直连",
            EffectiveRoute::RelayAuthenticated => "自动中转（登录态）",
            EffectiveRoute::RelayApi => "自动中转（API）",
            EffectiveRoute::DegradedRelay => "已退化为自动中转",
        }
    }
}

pub(crate) struct AppliedProfileResult {
    pub(crate) message: String,
}

pub(crate) struct BackupCandidate {
    pub(crate) path: PathBuf,
    pub(crate) modified_at_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderSyncRequest {
    pub(crate) target_provider: Option<String>,
}
