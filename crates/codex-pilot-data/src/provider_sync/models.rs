use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSyncStatus {
    Skipped,
    Synced,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSyncResult {
    pub status: ProviderSyncStatus,
    pub message: String,
    pub target_provider: String,
    pub backup_dir: Option<PathBuf>,
    pub changed_session_files: usize,
    pub sqlite_rows_updated: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCount {
    pub provider: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSyncInspection {
    pub target_provider: String,
    pub rollout_files: usize,
    pub rollout_rewrite_needed: usize,
    pub sqlite_rows: usize,
    pub sqlite_provider_rows_needing_sync: usize,
    pub sqlite_total_updates_needed: usize,
    pub rollout_providers: Vec<ProviderCount>,
    pub sqlite_providers: Vec<ProviderCount>,
}

#[derive(Debug, Clone)]
pub(super) struct SessionChange {
    pub path: PathBuf,
    pub original_first_line: String,
    pub next_first_line: String,
    pub separator: String,
    pub thread_id: Option<String>,
    pub cwd: Option<String>,
    pub has_user_event: bool,
    pub rewrite_needed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ProviderDriftDetail {
    pub id: String,
    pub title: String,
    pub source: String,
    pub thread_source: String,
    pub sqlite_provider: String,
    pub rollout_provider: Option<String>,
    pub updated_at_ms: Option<i64>,
    pub rollout_path: String,
}

pub(super) fn result(
    status: ProviderSyncStatus,
    message: impl Into<String>,
    target_provider: &str,
    backup_dir: Option<PathBuf>,
    changed_session_files: usize,
    sqlite_rows_updated: usize,
) -> ProviderSyncResult {
    ProviderSyncResult {
        status,
        message: message.into(),
        target_provider: target_provider.to_string(),
        backup_dir,
        changed_session_files,
        sqlite_rows_updated,
    }
}

pub(super) fn provider_counts(values: impl IntoIterator<Item = String>) -> Vec<ProviderCount> {
    let mut counts = HashMap::<String, usize>::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    let mut items = counts
        .into_iter()
        .map(|(provider, count)| ProviderCount { provider, count })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.provider.cmp(&right.provider))
    });
    items
}
