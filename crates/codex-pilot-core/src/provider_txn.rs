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

        // 解析快照（缺失/空串 = None，绝不退化成"写空 config.toml"）。
        let snapshot = self.read_official_snapshot()?;

        // fail-fast 零写盘安全判定：
        // 当前若由 CodexPilot 托管（中转/纯API 留下哨兵键），恢复登录态必须有官方基线快照，
        // 否则无法还原原始 config，宁可报错也不清空当前配置。
        if snapshot.is_none() && self.config_is_codex_managed() {
            anyhow::bail!(
                "未捕获官方原版快照，无法安全恢复登录态（避免清空当前配置）。\
                 请确保切换到中转/纯API态前已捕获官方快照。"
            );
        }

        if let Err(e) = self.apply_official_inner(snapshot.as_ref()) {
            let _ = self.rollback();
            anyhow::bail!(e);
        }

        Ok(TxnResult {
            mode: ProviderMode::Official,
            config_backup_path: None,
        })
    }

    /// 读取官方快照。**仅当字段存在且 configToml 非空**时返回 Some；
    /// 字段缺失或为空串一律返回 None（不把"无快照"塌成"空配置"）。
    fn read_official_snapshot(&self) -> anyhow::Result<Option<OfficialConfigSnapshot>> {
        let FileSnapshot::Exists(data) = &self.profiles_json else {
            return Ok(None);
        };
        let v: serde_json::Value =
            serde_json::from_slice(data).context("无法解析 provider-profiles.json")?;
        let toml_str = v
            .get("officialConfigSnapshot")
            .and_then(|s| s.get("configToml"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if toml_str.is_empty() {
            return Ok(None);
        }
        Ok(Some(OfficialConfigSnapshot {
            config_toml: toml_str.to_string(),
            captured_at_ms: 0,
        }))
    }

    /// 当前 config.toml 是否由 CodexPilot 托管（带哨兵键）。
    fn config_is_codex_managed(&self) -> bool {
        match &self.config_toml {
            FileSnapshot::Exists(data) => String::from_utf8_lossy(data)
                .lines()
                .any(|line| line.trim().starts_with("codex_pilot_channel_mode")),
            FileSnapshot::Missing => false,
        }
    }

    fn apply_official_inner(
        &self,
        snapshot: Option<&OfficialConfigSnapshot>,
    ) -> anyhow::Result<()> {
        let config_path = self.config_path();

        match snapshot {
            // 有官方基线快照：备份当前 → 写回快照。
            Some(snapshot) => {
                let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
                backup_existing_config(&config_path, &existing)?;
                std::fs::write(&config_path, &snapshot.config_toml)
                    .context("恢复官方 config.toml")?;
            }
            // 无快照：到这里一定是"当前非托管"（托管+无快照已在 commit 前报错）。
            // 当前配置即官方/外部基线，**不触碰 config.toml**，仅清 API key。
            None => {}
        }

        // 清除 auth.json 中的 OPENAI_API_KEY（保留 ChatGPT tokens）
        clear_api_key_auth_json(&self.auth_path())
            .with_context(|| "清除 auth.json API key 失败")?;

        Ok(())
    }

    /// 切换到中转态(hybrid)：写 relay config + 哨兵 "hybrid"，保留 auth.json。
    ///
    /// `helper_port` 必须是 helper 实际监听的端口（manager 从启动偏好读取），
    /// 而非硬编码常量——否则 58888 被占回退到随机端口时，写进 config 的本地代理
    /// 地址会指向没人监听的端口，混合中转静默失效。
    pub fn commit_hybrid(
        self,
        base_url: &str,
        bearer_token: &str,
        upstream_protocol: UpstreamProtocol,
        helper_port: u16,
    ) -> anyhow::Result<TxnResult> {
        self.precheck_hybrid()?;

        if base_url.trim().is_empty() {
            anyhow::bail!("中转态需要填写 Base URL。");
        }

        if let Err(e) =
            self.apply_hybrid_inner(base_url, bearer_token, upstream_protocol, helper_port)
        {
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
        helper_port: u16,
    ) -> anyhow::Result<()> {
        let config_path = self.config_path();
        let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
        backup_existing_config(&config_path, &existing)?;

        let codex_base_url =
            protocol_proxy::proxy_base_url_for_protocol(base_url, upstream_protocol, helper_port);
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

    /// Activates a profile and reapplies the live route when CodexPilot owns it.
    ///
    /// The updated profile store and the Codex config/auth files share one
    /// transaction snapshot, so a failed route write restores all three files.
    pub fn commit_profile_activation(
        self,
        profiles_json: &[u8],
        current_mode: Option<ProviderMode>,
        base_url: &str,
        api_key: &str,
        upstream_protocol: UpstreamProtocol,
        helper_port: u16,
    ) -> anyhow::Result<()> {
        serde_json::from_slice::<serde_json::Value>(profiles_json)
            .context("invalid provider-profiles.json for activation")?;

        match current_mode {
            Some(ProviderMode::Hybrid) => self.precheck_hybrid()?,
            Some(ProviderMode::Api) => Self::precheck_api(base_url, api_key)?,
            _ => {}
        }

        let apply_result = (|| {
            std::fs::create_dir_all(&self.state_dir).with_context(|| {
                format!(
                    "failed to create provider state directory: {}",
                    self.state_dir.display()
                )
            })?;
            std::fs::write(self.profiles_path(), profiles_json)
                .context("failed to write activated provider-profiles.json")?;

            match current_mode {
                Some(ProviderMode::Hybrid) => {
                    self.apply_hybrid_inner(base_url, api_key, upstream_protocol, helper_port)
                }
                Some(ProviderMode::Api) => {
                    self.apply_api_inner(base_url, api_key, upstream_protocol)
                }
                _ => Ok(()),
            }
        })();

        if let Err(error) = apply_result {
            let _ = self.rollback();
            anyhow::bail!(error);
        }

        Ok(())
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

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        crate::app_paths::test_dirs_guard()
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

    fn activation_profiles_json(active_profile_id: &str) -> Vec<u8> {
        serde_json::to_vec_pretty(&serde_json::json!({
            "activeProfileId": active_profile_id,
            "profiles": [
                {
                    "id": "direct",
                    "name": "Direct",
                    "baseUrl": "https://direct.example.com/v1",
                    "bearerToken": "sk-direct",
                    "upstreamProtocol": "responses"
                },
                {
                    "id": "proxy",
                    "name": "Proxy",
                    "baseUrl": "https://proxy.example.com/v1",
                    "bearerToken": "sk-proxy",
                    "upstreamProtocol": "chatCompletions"
                }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn profile_activation_reapplies_hybrid_route_across_protocol_modes() {
        let _guard = test_guard();
        let home = unique_temp_dir("activate-hybrid");
        let state = unique_temp_dir("activate-hybrid-state");
        let initial_profiles = activation_profiles_json("direct");
        write_txn_test_files(
            &home,
            &state,
            "model_provider = \"CodexPilot\"\ncodex_pilot_channel_mode = \"hybrid\"\n",
            chatgpt_auth_json(),
            std::str::from_utf8(&initial_profiles).unwrap(),
        );
        set_test_dirs(&home, &state);

        let proxy_profiles = activation_profiles_json("proxy");
        ProviderTxn::begin()
            .unwrap()
            .commit_profile_activation(
                &proxy_profiles,
                Some(ProviderMode::Hybrid),
                "https://proxy.example.com/v1",
                "sk-proxy",
                UpstreamProtocol::ChatCompletions,
                59999,
            )
            .unwrap();

        let proxy_config = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert!(proxy_config.contains("base_url = \"http://127.0.0.1:59999/v1\""));
        assert!(proxy_config.contains("codex_pilot_upstream_protocol = \"chat_completions\""));
        let stored_profiles =
            std::fs::read_to_string(state.join("provider-profiles.json")).unwrap();
        assert!(stored_profiles.contains("\"activeProfileId\": \"proxy\""));

        let direct_profiles = activation_profiles_json("direct");
        ProviderTxn::begin()
            .unwrap()
            .commit_profile_activation(
                &direct_profiles,
                Some(ProviderMode::Hybrid),
                "https://direct.example.com/v1",
                "sk-direct",
                UpstreamProtocol::Responses,
                59999,
            )
            .unwrap();

        let direct_config = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert!(direct_config.contains("base_url = \"https://direct.example.com/v1\""));
        assert!(direct_config.contains("codex_pilot_upstream_protocol = \"responses\""));
        let stored_profiles =
            std::fs::read_to_string(state.join("provider-profiles.json")).unwrap();
        assert!(stored_profiles.contains("\"activeProfileId\": \"direct\""));

        clear_test_dirs();
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn profile_activation_reapplies_api_url_and_key() {
        let _guard = test_guard();
        let home = unique_temp_dir("activate-api");
        let state = unique_temp_dir("activate-api-state");
        let initial_profiles = activation_profiles_json("direct");
        write_txn_test_files(
            &home,
            &state,
            "model_provider = \"CodexPilot\"\ncodex_pilot_channel_mode = \"api\"\n",
            r#"{"OPENAI_API_KEY":"sk-old"}"#,
            std::str::from_utf8(&initial_profiles).unwrap(),
        );
        set_test_dirs(&home, &state);

        let updated_profiles = activation_profiles_json("direct");
        ProviderTxn::begin()
            .unwrap()
            .commit_profile_activation(
                &updated_profiles,
                Some(ProviderMode::Api),
                "https://direct.example.com/v1",
                "sk-direct",
                UpstreamProtocol::Responses,
                59999,
            )
            .unwrap();

        let config = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert!(config.contains("base_url = \"https://direct.example.com/v1\""));
        assert!(config.contains("codex_pilot_channel_mode = \"api\""));
        let auth = std::fs::read_to_string(home.join("auth.json")).unwrap();
        assert!(auth.contains("sk-direct"));
        assert!(!auth.contains("sk-old"));

        clear_test_dirs();
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&state);
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
            58888,
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
            58888,
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
    fn hybrid_config_points_codex_at_actual_helper_port() {
        // 回归测试:写进 config 的本地代理地址必须用传入的真实 helper 端口,
        // 而非硬编码常量(58888 被占回退随机端口时不能对不上)。
        let _guard = test_guard();
        let home = unique_temp_dir("hybrid-port");
        let state = unique_temp_dir("hybrid-port-state");
        write_txn_test_files(
            &home,
            &state,
            "model = \"gpt-5\"\n",
            chatgpt_auth_json(),
            relay_profile_json().as_str(),
        );
        set_test_dirs(&home, &state);

        let custom_port: u16 = 61234;
        let txn = ProviderTxn::begin().unwrap();
        let result = txn.commit_hybrid(
            "https://relay.example.com/v1",
            "sk-test",
            // ChatCompletions → LocalProxy 路由,base_url 改写成本地 helper 地址
            UpstreamProtocol::ChatCompletions,
            custom_port,
        );
        assert!(result.is_ok(), "hybrid commit failed: {:?}", result.err());

        let config = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert!(
            config.contains(&format!("127.0.0.1:{custom_port}")),
            "config must point Codex at the real helper port {custom_port}: {config}"
        );
        assert!(
            !config.contains("127.0.0.1:58888"),
            "config must not hardcode the default port: {config}"
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
    fn official_without_snapshot_does_not_wipe_unmanaged_config() {
        // 回归测试:官方态(非托管)+ 有 ChatGPT 登录 + 无快照 → 切登录态绝不清空 config.toml。
        let _guard = test_guard();
        let home = unique_temp_dir("official-nowipe");
        let state = unique_temp_dir("official-nowipe-state");
        let original_config = "model = \"gpt-5\"\nmodel_provider = \"chatgpt\"\n";
        write_txn_test_files(
            &home,
            &state,
            original_config,
            chatgpt_auth_json(),
            // profiles 存在但无 officialConfigSnapshot 字段
            r#"{"activeProfileId":"","profiles":[]}"#,
        );
        set_test_dirs(&home, &state);

        let txn = ProviderTxn::begin().unwrap();
        let result = txn.commit_official();
        assert!(result.is_ok(), "official commit failed: {:?}", result.err());

        // config.toml 必须原样保留,绝不被写空
        let config = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert_eq!(
            config, original_config,
            "non-managed config must not be touched when no snapshot exists"
        );

        clear_test_dirs();
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn official_without_snapshot_errors_when_codex_managed() {
        // 托管态(哨兵键)+ 无快照 → 必须 fail-fast 报错,且零写盘(不清空 config)。
        let _guard = test_guard();
        let home = unique_temp_dir("official-managed-err");
        let state = unique_temp_dir("official-managed-err-state");
        let managed_config =
            "model_provider = \"CodexPilot\"\ncodex_pilot_channel_mode = \"hybrid\"\n";
        write_txn_test_files(
            &home,
            &state,
            managed_config,
            chatgpt_auth_json(),
            r#"{"activeProfileId":"","profiles":[]}"#,
        );
        set_test_dirs(&home, &state);

        let txn = ProviderTxn::begin().unwrap();
        let result = txn.commit_official();
        assert!(result.is_err(), "should refuse to restore without baseline");

        // config.toml 不被改动
        let config = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert_eq!(
            config, managed_config,
            "managed config must stay intact on error"
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
