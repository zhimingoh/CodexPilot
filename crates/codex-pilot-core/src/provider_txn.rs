//! Provider 三态事务写入器。
//!
//! 保证 config.toml / auth.json / provider-profiles.json 三文件的事务边界：
//! snapshot → 前置校验(fail-fast 零写盘) → 顺序写入 → 失败时 rollback。
//!
//! 设计依据：docs/development/provider-restore-design.md §三、核心防 bug 设计

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::relay_config::OfficialConfigSnapshot;
use crate::relay_config_auth::auth_json_chatgpt_account_label;
use crate::relay_config_toml::{
    backup_existing_config, clear_api_key_auth_json, upsert_api_provider_config,
    upsert_relay_provider_config, write_pure_api_auth_json,
};

use crate::protocol_proxy::{self, UpstreamProtocol};

/// 三个 Provider 运行态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderMode {
    /// 登录态：官方基线，ChatGPT OAuth。
    Official,
    /// 中转态(混合中转)：本地 protocol_proxy 拦截模型请求，保留 ChatGPT 登录。
    Hybrid,
    /// 纯API态：API key 直连。
    Api,
}

impl ProviderMode {
    pub fn sentinel_value(&self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Hybrid => "hybrid",
            Self::Api => "api",
        }
    }
}

/// 事务执行结果。
#[derive(Debug, Clone)]
pub struct TxnResult {
    pub mode: ProviderMode,
    pub config_backup_path: Option<String>,
}

/// 文件快照（含"文件不存在"状态）。
enum FileSnapshot {
    Exists(Vec<u8>),
    Missing,
}

/// 事务上下文：持有三文件快照 + 写回路径。
pub struct ProviderTxn {
    home_dir: PathBuf,
    state_dir: PathBuf,
    config_toml: FileSnapshot,
    auth_json: FileSnapshot,
    profiles_json: FileSnapshot,
}

impl ProviderTxn {
    /// begin()：快照三文件当前内容到内存。
    pub fn begin() -> anyhow::Result<Self> {
        let home_dir = crate::app_paths::codex_home_dir();
        let state_dir = crate::app_paths::app_state_dir();
        let config_toml = Self::snapshot_file(&home_dir.join("config.toml"))?;
        let auth_json = Self::snapshot_file(&home_dir.join("auth.json"))?;
        let profiles_json = Self::snapshot_file(&state_dir.join("provider-profiles.json"))?;
        Ok(Self {
            home_dir,
            state_dir,
            config_toml,
            auth_json,
            profiles_json,
        })
    }

    fn snapshot_file(path: &Path) -> anyhow::Result<FileSnapshot> {
        if path.exists() {
            Ok(FileSnapshot::Exists(
                std::fs::read(path).with_context(|| format!("snapshot {}", path.display()))?,
            ))
        } else {
            Ok(FileSnapshot::Missing)
        }
    }

    fn config_path(&self) -> PathBuf {
        self.home_dir.join("config.toml")
    }

    fn auth_path(&self) -> PathBuf {
        self.home_dir.join("auth.json")
    }

    fn profiles_path(&self) -> PathBuf {
        self.state_dir.join("provider-profiles.json")
    }

    // ── 前置校验 ──

    /// 切换到登录态的前置条件：有官方快照 或 当前有 ChatGPT 登录。
    fn precheck_official(&self) -> anyhow::Result<()> {
        let has_snapshot = match &self.profiles_json {
            FileSnapshot::Exists(data) => {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(data) {
                    v.get("officialConfigSnapshot")
                        .and_then(|s| s.get("configToml"))
                        .and_then(serde_json::Value::as_str)
                        .map(|s| !s.is_empty())
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            FileSnapshot::Missing => false,
        };
        let has_chatgpt = auth_json_chatgpt_account_label(&self.auth_path()).is_some();
        if !has_snapshot && !has_chatgpt {
            anyhow::bail!(
                "未检测到 ChatGPT 登录，也没有可恢复的官方快照。请先登录 ChatGPT 或手动保存官方快照。"
            );
        }
        Ok(())
    }

    /// 切换到中转态的前置条件：有 ChatGPT 登录（中转态要保留它）。
    fn precheck_hybrid(&self) -> anyhow::Result<()> {
        if auth_json_chatgpt_account_label(&self.auth_path()).is_none() {
            anyhow::bail!("中转态需要先登录 ChatGPT。若只想用 API，请选择纯API态。");
        }
        Ok(())
    }

    /// 切换到纯API态的前置条件：提供了 base_url + key。
    fn precheck_api(base_url: &str, api_key: &str) -> anyhow::Result<()> {
        if base_url.trim().is_empty() {
            anyhow::bail!("纯API态需要填写 Base URL。");
        }
        if api_key.trim().is_empty() {
            anyhow::bail!("纯API态需要填写 API Key。");
        }
        Ok(())
    }

    // ── 核心执行 ──

    /// 切换到登录态：恢复官方快照 → 清除 API key → 保留 ChatGPT tokens。
    pub fn commit_official(self) -> anyhow::Result<TxnResult> {
        self.precheck_official()?;

        let snapshot = self.read_official_snapshot()?;

        if let Err(e) = self.apply_official_inner(&snapshot) {
            let _ = self.rollback();
            anyhow::bail!(e);
        }

        Ok(TxnResult {
            mode: ProviderMode::Official,
            config_backup_path: None,
        })
    }

    fn read_official_snapshot(&self) -> anyhow::Result<OfficialConfigSnapshot> {
        match &self.profiles_json {
            FileSnapshot::Exists(data) => {
                let v: serde_json::Value =
                    serde_json::from_slice(data).context("无法解析 provider-profiles.json")?;
                let toml_str = v
                    .get("officialConfigSnapshot")
                    .and_then(|s| s.get("configToml"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                Ok(OfficialConfigSnapshot {
                    config_toml: toml_str.to_string(),
                    captured_at_ms: 0,
                })
            }
            FileSnapshot::Missing => Ok(OfficialConfigSnapshot {
                config_toml: String::new(),
                captured_at_ms: 0,
            }),
        }
    }

    fn apply_official_inner(&self, snapshot: &OfficialConfigSnapshot) -> anyhow::Result<()> {
        let config_path = self.config_path();
        let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
        backup_existing_config(&config_path, &existing)?;

        // 写回快照内容（空串=config.toml 原本不存在→写空）
        if snapshot.config_toml.is_empty() {
            std::fs::write(&config_path, "").context("恢复空 config.toml")?;
        } else {
            std::fs::write(&config_path, &snapshot.config_toml).context("恢复官方 config.toml")?;
        }

        // 清除 auth.json 中的 OPENAI_API_KEY（保留 ChatGPT tokens）
        clear_api_key_auth_json(&self.auth_path())
            .with_context(|| "清除 auth.json API key 失败")?;

        Ok(())
    }

    /// 切换到中转态(hybrid)：写 relay config + 哨兵 "hybrid"，保留 auth.json。
    pub fn commit_hybrid(
        self,
        base_url: &str,
        bearer_token: &str,
        upstream_protocol: UpstreamProtocol,
    ) -> anyhow::Result<TxnResult> {
        self.precheck_hybrid()?;

        if base_url.trim().is_empty() {
            anyhow::bail!("中转态需要填写 Base URL。");
        }

        if let Err(e) = self.apply_hybrid_inner(base_url, bearer_token, upstream_protocol) {
            let _ = self.rollback();
            anyhow::bail!(e);
        }

        Ok(TxnResult {
            mode: ProviderMode::Hybrid,
            config_backup_path: None,
        })
    }

    fn apply_hybrid_inner(
        &self,
        base_url: &str,
        bearer_token: &str,
        upstream_protocol: UpstreamProtocol,
    ) -> anyhow::Result<()> {
        let config_path = self.config_path();
        let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
        backup_existing_config(&config_path, &existing)?;

        let codex_base_url = protocol_proxy::proxy_base_url_for_protocol(
            base_url,
            upstream_protocol,
            protocol_proxy::DEFAULT_PROTOCOL_PROXY_PORT,
        );
        let updated = upsert_relay_provider_config(
            &existing,
            &codex_base_url,
            bearer_token,
            upstream_protocol,
        );
        std::fs::write(&config_path, updated)
            .with_context(|| format!("写入中转 config.toml 失败: {}", config_path.display()))?;

        // 不碰 auth.json —— 保留 ChatGPT tokens
        Ok(())
    }

    /// 切换到纯API态：写 auth.json API key → 写 API config + 哨兵 "api"。
    pub fn commit_api(
        self,
        base_url: &str,
        api_key: &str,
        upstream_protocol: UpstreamProtocol,
    ) -> anyhow::Result<TxnResult> {
        Self::precheck_api(base_url, api_key)?;

        if let Err(e) = self.apply_api_inner(base_url, api_key, upstream_protocol) {
            let _ = self.rollback();
            anyhow::bail!(e);
        }

        Ok(TxnResult {
            mode: ProviderMode::Api,
            config_backup_path: None,
        })
    }

    fn apply_api_inner(
        &self,
        base_url: &str,
        api_key: &str,
        upstream_protocol: UpstreamProtocol,
    ) -> anyhow::Result<()> {
        // 先写 auth.json
        write_pure_api_auth_json(&self.home_dir, api_key)?;

        // 再写 config.toml
        let config_path = self.config_path();
        let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
        backup_existing_config(&config_path, &existing)?;

        let codex_base_url = protocol_proxy::proxy_base_url_for_protocol(
            base_url,
            upstream_protocol,
            protocol_proxy::DEFAULT_PROTOCOL_PROXY_PORT,
        );
        let updated =
            upsert_api_provider_config(&existing, &codex_base_url, api_key, upstream_protocol);
        std::fs::write(&config_path, updated)
            .with_context(|| format!("写入 API config.toml 失败: {}", config_path.display()))?;

        Ok(())
    }

    // ── Rollback ──

    /// 用快照回滚全部三个文件。失败时返回错误。
    fn rollback(&self) -> anyhow::Result<()> {
        let mut errors: Vec<String> = Vec::new();

        if let Err(e) = Self::restore_snapshot(&self.config_path(), &self.config_toml) {
            errors.push(format!("回滚 config.toml 失败: {e}"));
        }
        if let Err(e) = Self::restore_snapshot(&self.auth_path(), &self.auth_json) {
            errors.push(format!("回滚 auth.json 失败: {e}"));
        }
        if let Err(e) = Self::restore_snapshot(&self.profiles_path(), &self.profiles_json) {
            errors.push(format!("回滚 provider-profiles.json 失败: {e}"));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            anyhow::bail!("回滚失败: {}", errors.join("; "))
        }
    }

    fn restore_snapshot(path: &Path, snapshot: &FileSnapshot) -> anyhow::Result<()> {
        match snapshot {
            FileSnapshot::Exists(data) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::write(path, data)
                    .with_context(|| format!("回滚写入 {}", path.display()))?;
            }
            FileSnapshot::Missing => {
                if path.exists() {
                    std::fs::remove_file(path)
                        .with_context(|| format!("回滚删除 {}", path.display()))?;
                }
            }
        }
        Ok(())
    }
}

// ── 哨兵键读写辅助 ──

/// 读取当前 Provider 模式（基于 config.toml 哨兵键）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModeReading {
    pub mode: Option<ProviderMode>,
    /// true = CodexPilot 托管(有哨兵键)
    pub owned_by_codex_pilot: bool,
    /// true = 外部配置了 model_provider 但无哨兵键 (ccSwitch 等)
    pub external_provider: bool,
}

/// 读取 config.toml 判断当前态与所有权。
pub fn read_current_mode() -> anyhow::Result<ProviderModeReading> {
    let config_path = crate::app_paths::codex_config_path();
    if !config_path.exists() {
        let chatgpt_auth =
            auth_json_chatgpt_account_label(&crate::app_paths::codex_home_dir().join("auth.json"));
        return Ok(ProviderModeReading {
            mode: if chatgpt_auth.is_some() {
                Some(ProviderMode::Official)
            } else {
                None
            },
            owned_by_codex_pilot: false,
            external_provider: false,
        });
    }
    let contents = std::fs::read_to_string(&config_path)?;
    let mut sentinel: Option<&str> = None;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("codex_pilot_channel_mode") {
            sentinel = trimmed
                .split('=')
                .nth(1)
                .map(|v| v.trim().trim_matches('"'));
            break;
        }
    }

    let has_custom_provider = contents.contains("model_provider");
    let chatgpt_auth =
        auth_json_chatgpt_account_label(&crate::app_paths::codex_home_dir().join("auth.json"));

    match sentinel {
        Some("hybrid") => Ok(ProviderModeReading {
            mode: Some(ProviderMode::Hybrid),
            owned_by_codex_pilot: true,
            external_provider: false,
        }),
        Some("api") => Ok(ProviderModeReading {
            mode: Some(ProviderMode::Api),
            owned_by_codex_pilot: true,
            external_provider: false,
        }),
        Some("official") => Ok(ProviderModeReading {
            mode: Some(ProviderMode::Official),
            owned_by_codex_pilot: true,
            external_provider: false,
        }),
        None if has_custom_provider => Ok(ProviderModeReading {
            mode: Some(ProviderMode::Api),
            owned_by_codex_pilot: false,
            external_provider: true,
        }),
        None if chatgpt_auth.is_some() => Ok(ProviderModeReading {
            mode: Some(ProviderMode::Official),
            owned_by_codex_pilot: false,
            external_provider: false,
        }),
        _ => Ok(ProviderModeReading {
            mode: None,
            owned_by_codex_pilot: false,
            external_provider: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
        GUARD
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "codex-pilot-txn-{name}-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }

    fn write_txn_test_files(
        home: &Path,
        state_dir: &Path,
        config_content: &str,
        auth_content: &str,
        profiles_content: &str,
    ) {
        std::fs::create_dir_all(home).unwrap();
        std::fs::create_dir_all(state_dir).unwrap();
        if !config_content.is_empty() {
            std::fs::write(home.join("config.toml"), config_content).unwrap();
        }
        if !auth_content.is_empty() {
            std::fs::write(home.join("auth.json"), auth_content).unwrap();
        }
        if !profiles_content.is_empty() {
            std::fs::write(state_dir.join("provider-profiles.json"), profiles_content).unwrap();
        }
    }

    fn set_test_dirs(home: &Path, state: &Path) {
        crate::app_paths::set_test_codex_home_dir(Some(home.to_path_buf()));
        crate::app_paths::set_test_app_state_dir(Some(state.to_path_buf()));
    }

    fn clear_test_dirs() {
        crate::app_paths::set_test_codex_home_dir(None);
        crate::app_paths::set_test_app_state_dir(None);
    }

    #[test]
    fn begin_snapshots_existing_files() {
        let _guard = test_guard();
        let home = unique_temp_dir("snap-existing");
        let state = unique_temp_dir("snap-state");
        write_txn_test_files(
            &home,
            &state,
            "model_provider = \"CodexPilot\"",
            r#"{"openai":{}}"#,
            r#"{"profiles":[]}"#,
        );
        set_test_dirs(&home, &state);

        let txn = ProviderTxn::begin().unwrap();
        assert!(matches!(txn.config_toml, FileSnapshot::Exists(_)));
        assert!(matches!(txn.auth_json, FileSnapshot::Exists(_)));
        assert!(matches!(txn.profiles_json, FileSnapshot::Exists(_)));

        clear_test_dirs();
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn rollback_restores_from_snapshot() {
        let _guard = test_guard();
        let home = unique_temp_dir("rollback");
        let state = unique_temp_dir("rollback-state");
        let original_config = "model_provider = \"OpenAI\"\n";
        write_txn_test_files(
            &home,
            &state,
            original_config,
            r#"{}"#,
            r#"{"profiles":[]}"#,
        );
        set_test_dirs(&home, &state);

        let txn = ProviderTxn::begin().unwrap();

        // Corrupt config.toml
        std::fs::write(home.join("config.toml"), "corrupted\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(home.join("config.toml")).unwrap(),
            "corrupted\n"
        );

        txn.rollback().unwrap();
        assert_eq!(
            std::fs::read_to_string(home.join("config.toml")).unwrap(),
            original_config
        );

        clear_test_dirs();
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn rollback_deletes_newly_created_files() {
        let _guard = test_guard();
        let home = unique_temp_dir("rollback-delete");
        let state = unique_temp_dir("rollback-delete-state");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        // No config.toml initially
        std::fs::write(home.join("auth.json"), r#"{}"#).unwrap();
        std::fs::write(state.join("provider-profiles.json"), r#"{}"#).unwrap();
        set_test_dirs(&home, &state);

        let txn = ProviderTxn::begin().unwrap();
        assert!(matches!(txn.config_toml, FileSnapshot::Missing));

        // Write a new config.toml
        std::fs::write(home.join("config.toml"), "new\n").unwrap();
        assert!(home.join("config.toml").exists());

        txn.rollback().unwrap();
        assert!(!home.join("config.toml").exists());

        clear_test_dirs();
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&state);
    }

    // ── 事务一致性测试 ──

    fn chatgpt_auth_json() -> &'static str {
        r#"{"auth_mode":"chatgpt","tokens":{"access_token":"eyJhbGciOiJIUzI1NiJ9.eyJlbWFpbCI6InRlc3RAZXhhbXBsZS5jb20ifQ.fake","id_token":"eyJhbGciOiJIUzI1NiJ9.eyJlbWFpbCI6InRlc3RAZXhhbXBsZS5jb20ifQ.fake","refresh_token":"refresh"}}"#
    }

    fn relay_profile_json() -> String {
        r#"{"activeProfileId":"p1","profiles":[{"id":"p1","name":"Relay","baseUrl":"https://relay.example.com/v1","bearerToken":"sk-test","upstreamProtocol":"chatCompletions"}]}"#.to_string()
    }

    fn api_profile_json() -> &'static str {
        r#"{"activeProfileId":"p2","profiles":[{"id":"p2","name":"API","baseUrl":"https://api.example.com/v1","bearerToken":"sk-api","upstreamProtocol":"responses"}]}"#
    }

    #[test]
    fn precheck_hybrid_fails_without_chatgpt_login() {
        let _guard = test_guard();
        let home = unique_temp_dir("precheck-hybrid");
        let state = unique_temp_dir("precheck-hybrid-state");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        // No auth.json → no ChatGPT login
        std::fs::write(home.join("config.toml"), "model = \"gpt-5\"\n").unwrap();
        std::fs::write(
            state.join("provider-profiles.json"),
            relay_profile_json().as_bytes(),
        )
        .unwrap();
        set_test_dirs(&home, &state);

        let txn = ProviderTxn::begin().unwrap();
        // This should fail before any disk writes
        let result = txn.commit_hybrid(
            "https://relay.example.com/v1",
            "sk-test",
            UpstreamProtocol::ChatCompletions,
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("中转态需要先登录 ChatGPT") || err.contains("ChatGPT"));

        // Verify no disk changes (only auth.json might have been written by backup, rest should be intact)
        // Actually, since precheck runs first, nothing should be written
        clear_test_dirs();
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn switch_hybrid_writes_sentinel_and_preserves_auth() {
        let _guard = test_guard();
        let home = unique_temp_dir("sw-hybrid");
        let state = unique_temp_dir("sw-hybrid-state");
        write_txn_test_files(
            &home,
            &state,
            "model = \"gpt-5\"\nmodel_provider = \"chatgpt\"\n",
            chatgpt_auth_json(),
            relay_profile_json().as_str(),
        );
        set_test_dirs(&home, &state);

        let txn = ProviderTxn::begin().unwrap();
        let result = txn.commit_hybrid(
            "https://relay.example.com/v1",
            "sk-test",
            UpstreamProtocol::ChatCompletions,
        );
        assert!(result.is_ok(), "hybrid commit failed: {:?}", result.err());

        // Verify config.toml has sentinel "hybrid"
        let config = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert!(
            config.contains("codex_pilot_channel_mode = \"hybrid\""),
            "config missing hybrid sentinel: {config}"
        );
        assert!(
            config.contains("model_provider = \"CodexPilot\""),
            "config missing CodexPilot provider"
        );

        // Verify auth.json still has ChatGPT tokens (not overwritten)
        let auth = std::fs::read_to_string(home.join("auth.json")).unwrap();
        assert!(
            auth.contains("chatgpt"),
            "auth.json should still have chatgpt: {auth}"
        );
        assert!(
            !auth.contains("OPENAI_API_KEY"),
            "auth.json should NOT have OPENAI_API_KEY in hybrid mode"
        );

        clear_test_dirs();
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn switch_api_writes_auth_json_and_sentinel() {
        let _guard = test_guard();
        let home = unique_temp_dir("sw-api");
        let state = unique_temp_dir("sw-api-state");
        write_txn_test_files(
            &home,
            &state,
            "model = \"gpt-5\"\n",
            "{}",
            api_profile_json(),
        );
        set_test_dirs(&home, &state);

        let txn = ProviderTxn::begin().unwrap();
        let result = txn.commit_api(
            "https://api.example.com/v1",
            "sk-api",
            UpstreamProtocol::Responses,
        );
        assert!(result.is_ok(), "api commit failed: {:?}", result.err());

        // Verify auth.json has OPENAI_API_KEY
        let auth = std::fs::read_to_string(home.join("auth.json")).unwrap();
        assert!(
            auth.contains("OPENAI_API_KEY"),
            "auth.json missing API key: {auth}"
        );
        assert!(auth.contains("sk-api"));

        // Verify config.toml has sentinel "api"
        let config = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert!(
            config.contains("codex_pilot_channel_mode = \"api\""),
            "config missing api sentinel: {config}"
        );

        clear_test_dirs();
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn switch_official_restores_snapshot_and_clears_api_key() {
        let _guard = test_guard();
        let home = unique_temp_dir("sw-official");
        let state = unique_temp_dir("sw-official-state");
        let official_snapshot = crate::relay_config::OfficialConfigSnapshot {
            config_toml: "model = \"gpt-5\"\nmodel_provider = \"chatgpt\"\n".to_string(),
            captured_at_ms: 1000,
        };
        let profiles = serde_json::json!({
            "activeProfileId": "",
            "profiles": [],
            "officialConfigSnapshot": {
                "configToml": &official_snapshot.config_toml,
                "capturedAtMs": official_snapshot.captured_at_ms,
            }
        });
        write_txn_test_files(
            &home,
            &state,
            // Current state: in API mode
            "model_provider = \"CodexPilot\"\ncodex_pilot_channel_mode = \"api\"\n",
            r#"{"OPENAI_API_KEY":"sk-old"}"#,
            &serde_json::to_string(&profiles).unwrap(),
        );
        set_test_dirs(&home, &state);

        let txn = ProviderTxn::begin().unwrap();
        let result = txn.commit_official();
        assert!(result.is_ok(), "official commit failed: {:?}", result.err());

        // Verify config.toml restored
        let config = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert!(
            config.contains("model_provider = \"chatgpt\""),
            "config should be restored: {config}"
        );
        assert!(
            !config.contains("codex_pilot_channel_mode"),
            "sentinel should be gone: {config}"
        );

        // Verify auth.json has no OPENAI_API_KEY
        let auth = std::fs::read_to_string(home.join("auth.json")).unwrap();
        assert!(
            !auth.contains("OPENAI_API_KEY"),
            "API key should be cleared: {auth}"
        );

        clear_test_dirs();
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn read_current_mode_detects_external_provider() {
        let _guard = test_guard();
        let home = unique_temp_dir("read-ext");
        let state = unique_temp_dir("read-ext-state");
        write_txn_test_files(
            &home,
            &state,
            "model_provider = \"ccswitch-custom\"\n",
            "{}",
            "{}",
        );
        set_test_dirs(&home, &state);

        let reading = read_current_mode().unwrap();
        assert!(reading.external_provider, "should detect external provider");
        assert!(
            !reading.owned_by_codex_pilot,
            "should not be owned by CodexPilot"
        );
        assert_eq!(reading.mode, Some(ProviderMode::Api));

        clear_test_dirs();
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn read_current_mode_detects_codex_pilot_hybrid() {
        let _guard = test_guard();
        let home = unique_temp_dir("read-hybrid");
        let state = unique_temp_dir("read-hybrid-state");
        write_txn_test_files(
            &home,
            &state,
            "model_provider = \"CodexPilot\"\ncodex_pilot_channel_mode = \"hybrid\"\n",
            chatgpt_auth_json(),
            "{}",
        );
        set_test_dirs(&home, &state);

        let reading = read_current_mode().unwrap();
        assert!(
            reading.owned_by_codex_pilot,
            "should be owned by CodexPilot"
        );
        assert!(!reading.external_provider);
        assert_eq!(reading.mode, Some(ProviderMode::Hybrid));

        clear_test_dirs();
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&state);
    }
}
