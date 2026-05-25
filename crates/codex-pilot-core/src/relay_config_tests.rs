use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::protocol_proxy::UpstreamProtocol;
use crate::relay_config::{
    apply_api_provider_config_to_home_with_protocol, apply_relay_provider_config_to_home,
    apply_relay_provider_config_to_path, chatgpt_auth_status_from_home,
    relay_provider_config_from_contents, ChatGptAuthStatus, RELAY_PROVIDER,
};
use crate::relay_config_toml::{
    remove_root_key, remove_table, upsert_api_provider_config, upsert_relay_provider_config,
};

#[test]
fn reads_active_relay_provider_config() {
    let contents = r#"model_provider = "CodexPilot"

[model_providers.CodexPilot]
name = "CodexPilot"
wire_api = "responses"
requires_openai_auth = true
base_url = "https://relay.example/v1"
experimental_bearer_token = "sk-test"
"#;

    let status = relay_provider_config_from_contents(
        contents,
        Path::new("/tmp/config.toml"),
        ChatGptAuthStatus {
            authenticated: true,
            source: "/tmp/auth.json".to_string(),
            account_label: Some("user@example.com".to_string()),
            message: "已检测到 ChatGPT 登录状态。".to_string(),
        },
    );

    assert!(status.active);
    assert!(status.configured);
    assert_eq!(status.mode, "hybridApi");
    assert!(status.authenticated);
    assert_eq!(status.account_label.as_deref(), Some("user@example.com"));
    assert!(status.requires_openai_auth);
    assert!(status.has_bearer_token);
    assert_eq!(status.base_url.as_deref(), Some("https://relay.example/v1"));
}

#[test]
fn apply_preserves_unrelated_config_and_replaces_owned_provider() {
    let contents = r#"# keep this
model = "gpt-5"
model_provider = "openai"

[model_providers.CodexPilot]
name = "old"
base_url = "https://old.example/v1"

[model_providers.other]
name = "Other"
base_url = "https://other.example/v1"

[mcp_servers.local]
base_url = "http://127.0.0.1:8000"
"#;

    let updated = upsert_relay_provider_config(
        contents,
        "https://new.example/v1",
        "sk-new",
        UpstreamProtocol::Responses,
    );

    assert!(updated.contains("# keep this"));
    assert!(updated.contains("model = \"gpt-5\""));
    assert!(updated.contains("model_provider = \"CodexPilot\""));
    assert!(!updated.contains("OPENAI_API_KEY"));
    assert!(updated.contains("[model_providers.CodexPilot]"));
    assert!(updated.contains("wire_api = \"responses\""));
    assert!(updated.contains("requires_openai_auth = true"));
    assert!(updated.contains("base_url = \"https://new.example/v1\""));
    assert!(updated.contains("experimental_bearer_token = \"sk-new\""));
    assert!(updated.contains("[model_providers.other]"));
    assert!(updated.contains("[mcp_servers.local]"));
    assert!(updated.contains("base_url = \"http://127.0.0.1:8000\""));
    assert!(!updated.contains("https://old.example/v1"));
}

#[test]
fn apply_api_provider_writes_openai_api_key_provider() {
    let contents = r#"# keep this
model = "gpt-5"
model_provider = "chatgpt"
OPENAI_API_KEY = "old"

[model_providers.CodexPilot]
name = "old"
requires_openai_auth = true
base_url = "https://old.example/v1"

[mcp_servers.local]
base_url = "http://127.0.0.1:8000"
"#;

    let updated = upsert_api_provider_config(
        contents,
        "https://api.example/v1",
        "sk-api",
        UpstreamProtocol::Responses,
    );

    assert!(updated.contains("# keep this"));
    assert!(updated.contains("model = \"gpt-5\""));
    assert!(updated.contains("model_provider = \"CodexPilot\""));
    assert!(updated.contains("[model_providers.CodexPilot]"));
    assert!(updated.contains("wire_api = \"responses\""));
    assert!(updated.contains("requires_openai_auth = true"));
    assert!(updated.contains("codex_pilot_channel_mode = \"api\""));
    assert!(updated.contains("base_url = \"https://api.example/v1\""));
    assert!(updated.contains("experimental_bearer_token = \"sk-api\""));
    assert!(!updated.contains("OPENAI_API_KEY = \"sk-api\""));
    assert!(!updated.contains("env_key = \"OPENAI_API_KEY\""));
    assert!(!updated.contains("https://old.example/v1"));
    assert!(updated.contains("[mcp_servers.local]"));
}

#[test]
fn reads_active_api_provider_config() {
    let contents = r#"model_provider = "CodexPilot"
OPENAI_API_KEY = "sk-api"

[model_providers.CodexPilot]
name = "CodexPilot"
wire_api = "responses"
requires_openai_auth = true
base_url = "https://api.example/v1"
codex_pilot_channel_mode = "api"
experimental_bearer_token = "sk-api"
"#;

    let status = relay_provider_config_from_contents(
        contents,
        Path::new("/tmp/config.toml"),
        ChatGptAuthStatus {
            authenticated: false,
            source: String::new(),
            account_label: None,
            message: "未检测到 ChatGPT 登录状态，请先在 Codex 中完成官方登录。".to_string(),
        },
    );

    assert!(status.active);
    assert!(status.configured);
    assert_eq!(status.mode, "api");
    assert!(!status.authenticated);
    assert!(status.requires_openai_auth);
    assert!(status.has_bearer_token);
    assert_eq!(status.base_url.as_deref(), Some("https://api.example/v1"));
}

#[test]
fn apply_api_provider_writes_pure_api_auth_json_and_config() {
    let root = unique_temp_dir();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("auth.json"),
        r#"{"auth_mode":"chatgpt","tokens":{"access_token":"token"}}"#,
    )
    .unwrap();

    let apply = apply_api_provider_config_to_home_with_protocol(
        &root,
        "https://api.example/v1",
        "sk-api",
        UpstreamProtocol::Responses,
    )
    .unwrap();

    assert!(apply.configured);
    let auth = std::fs::read_to_string(root.join("auth.json")).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&auth).unwrap(),
        serde_json::json!({"OPENAI_API_KEY": "sk-api"})
    );
    let config = std::fs::read_to_string(root.join("config.toml")).unwrap();
    assert!(config.contains("codex_pilot_channel_mode = \"api\""));
    assert!(config.contains("requires_openai_auth = true"));
    assert!(config.contains("experimental_bearer_token = \"sk-api\""));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn clear_removes_owned_provider_and_keeps_other_tables() {
    let contents = r#"OPENAI_API_KEY = "old"
model_provider = "CodexPilot"

[model_providers.CodexPilot]
name = "CodexPilot"
base_url = "https://relay.example/v1"
experimental_bearer_token = "sk-test"

[model_providers.other]
name = "Other"

[mcp_servers.local]
base_url = "http://127.0.0.1:8000"
"#;

    let without_relay = remove_table(contents, &format!("model_providers.{RELAY_PROVIDER}"));
    let without_key = remove_root_key(&without_relay, "OPENAI_API_KEY");
    let updated = remove_root_key(&without_key, "model_provider");

    assert!(!updated.contains("model_provider ="));
    assert!(!updated.contains("[model_providers.CodexPilot]"));
    assert!(!updated.contains("OPENAI_API_KEY"));
    assert!(updated.contains("[model_providers.other]"));
    assert!(updated.contains("[mcp_servers.local]"));
    assert!(updated.contains("base_url = \"http://127.0.0.1:8000\""));
}

#[test]
fn apply_and_clear_create_backups_when_file_exists() {
    let root = unique_temp_dir();
    let config_path = root.join("config.toml");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(&config_path, "model = \"gpt-5\"\n").unwrap();

    let apply = apply_relay_provider_config_to_path(
        &config_path,
        "https://relay.example/v1",
        "sk-test",
    )
    .unwrap();
    assert!(apply.configured);
    let apply_backup = PathBuf::from(apply.backup_path.unwrap());
    assert_eq!(
        std::fs::read_to_string(&apply_backup).unwrap(),
        "model = \"gpt-5\"\n"
    );

    let clear = crate::relay_config::clear_relay_provider_config_from_path(&config_path).unwrap();
    assert!(!clear.configured);
    assert!(PathBuf::from(clear.backup_path.unwrap()).exists());
    let final_contents = std::fs::read_to_string(&config_path).unwrap();
    assert!(!final_contents.contains("model_provider ="));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn detects_chatgpt_auth_from_auth_json() {
    let root = unique_temp_dir();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("auth.json"),
        r#"{"auth_mode":"chatgpt","tokens":{"access_token":"header.eyJlbWFpbCI6InVzZXJAZXhhbXBsZS5jb20ifQ.signature"}}"#,
    )
    .unwrap();

    let auth = chatgpt_auth_status_from_home(&root);
    assert!(auth.authenticated);
    assert_eq!(auth.account_label.as_deref(), Some("user@example.com"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn apply_to_home_requires_chatgpt_auth() {
    let root = unique_temp_dir();
    std::fs::create_dir_all(&root).unwrap();

    let error =
        apply_relay_provider_config_to_home(&root, "https://relay.example/v1", "sk-test")
            .unwrap_err();
    assert!(error.to_string().contains("未检测到 ChatGPT 登录状态"));

    std::fs::remove_dir_all(root).unwrap();
}

fn unique_temp_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "codex-pilot-relay-config-test-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}
