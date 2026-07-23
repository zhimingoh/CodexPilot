use anyhow::{Context, bail};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

const CDP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CDP_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct CdpTarget {
    pub id: String,
    #[serde(rename = "type")]
    pub target_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    pub web_socket_debugger_url: Option<String>,
}

pub async fn list_targets(debug_port: u16) -> anyhow::Result<Vec<CdpTarget>> {
    let url = format!("http://127.0.0.1:{debug_port}/json");
    let _ = crate::diagnostic_log::append(
        "cdp.list_targets",
        serde_json::json!({ "debug_port": debug_port }),
    );
    let client = crate::http_client::shared();
    let response = client
        .get(url)
        .send()
        .await
        .context("failed to query CDP targets")?
        .error_for_status()
        .context("CDP target query failed")?;

    response
        .json::<Vec<CdpTarget>>()
        .await
        .context("failed to deserialize CDP targets")
}

pub fn pick_page_target(targets: &[CdpTarget]) -> anyhow::Result<CdpTarget> {
    targets
        .iter()
        .filter(|target| {
            target.target_type == "page"
                && target
                    .web_socket_debugger_url
                    .as_deref()
                    .is_some_and(|url| !url.is_empty())
        })
        .filter_map(|target| target_score(target).map(|score| (score, target)))
        .max_by_key(|(score, _)| *score)
        .map(|(_, target)| target.clone())
        .ok_or_else(|| anyhow::anyhow!("No injectable Codex or ChatGPT page target found"))
}

fn target_score(target: &CdpTarget) -> Option<u8> {
    let title = target.title.to_lowercase();
    let url = target.url.to_lowercase();
    let haystack = format!("{title} {url}");
    let is_chatgpt = url.contains("chatgpt.com") || title.contains("chatgpt");
    let is_codex = haystack.contains("codex");

    if is_chatgpt && (url.contains("/codex") || is_codex) {
        Some(100)
    } else if is_codex {
        Some(80)
    } else if is_chatgpt {
        Some(60)
    } else {
        None
    }
}

pub async fn evaluate_script(websocket_url: &str, script: &str) -> anyhow::Result<Value> {
    let (mut socket, _) = tokio::time::timeout(
        CDP_CONNECT_TIMEOUT,
        tokio_tungstenite::connect_async(websocket_url),
    )
    .await
    .with_context(|| {
        format!(
            "timed out connecting CDP websocket after {}s",
            CDP_CONNECT_TIMEOUT.as_secs()
        )
    })?
    .context("failed to connect CDP websocket")?;

    let id = 1_u64;
    socket
        .send(Message::Text(
            json!({
                "id": id,
                "method": "Runtime.evaluate",
                "params": {
                    "expression": script,
                    "awaitPromise": false,
                    "allowUnsafeEvalBlockedByCSP": true
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .context("failed to send CDP Runtime.evaluate command")?;

    loop {
        let message = tokio::time::timeout(CDP_COMMAND_TIMEOUT, socket.next())
            .await
            .with_context(|| {
                format!(
                    "timed out waiting for CDP Runtime.evaluate response after {}s",
                    CDP_COMMAND_TIMEOUT.as_secs()
                )
            })?
            .ok_or_else(|| anyhow::anyhow!("CDP websocket closed before response"))?
            .context("failed to read CDP websocket message")?;
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&text).context("failed to parse CDP message")?;
        if value.get("id").and_then(Value::as_u64) == Some(id) {
            if let Some(error) = value.get("error") {
                bail!("CDP Runtime.evaluate failed: {error}");
            }
            return Ok(value);
        }
    }
}

pub async fn inject_script(debug_port: u16, script: &str) -> anyhow::Result<()> {
    let targets = list_targets(debug_port).await?;
    let target = pick_page_target(&targets)?;
    let websocket_url = target
        .web_socket_debugger_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("selected CDP target has no websocket URL"))?;
    evaluate_script(websocket_url, script).await?;
    Ok(())
}

pub async fn selected_page_websocket_url(debug_port: u16) -> anyhow::Result<String> {
    let targets = list_targets(debug_port).await?;
    let target = pick_page_target(&targets)?;
    let _ = crate::diagnostic_log::append(
        "cdp.selected_target",
        serde_json::json!({
            "id": target.id,
            "title": target.title,
            "url": target.url
        }),
    );
    target
        .web_socket_debugger_url
        .ok_or_else(|| anyhow::anyhow!("selected CDP target has no websocket URL"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(id: &str, title: &str, url: &str) -> CdpTarget {
        CdpTarget {
            id: id.to_string(),
            target_type: "page".to_string(),
            title: title.to_string(),
            url: url.to_string(),
            web_socket_debugger_url: Some(format!("ws://127.0.0.1/{id}")),
        }
    }

    #[test]
    fn pick_page_target_prefers_chatgpt_page_over_unrelated_first_page() {
        let targets = vec![
            target("first", "Localhost", "http://localhost:1420"),
            target("chatgpt", "ChatGPT", "https://chatgpt.com/"),
        ];

        let selected = pick_page_target(&targets).unwrap();

        assert_eq!(selected.id, "chatgpt");
    }

    #[test]
    fn pick_page_target_prefers_codex_route_inside_chatgpt() {
        let targets = vec![
            target("chat", "ChatGPT", "https://chatgpt.com/"),
            target("codex", "ChatGPT", "https://chatgpt.com/codex"),
        ];

        let selected = pick_page_target(&targets).unwrap();

        assert_eq!(selected.id, "codex");
    }

    #[test]
    fn pick_page_target_rejects_unrelated_pages() {
        let targets = vec![
            target("manager", "Pilot Manager", "http://localhost:1420"),
            target("docs", "Rust Docs", "https://doc.rust-lang.org/"),
        ];

        let error = pick_page_target(&targets).unwrap_err().to_string();

        assert!(error.contains("No injectable Codex or ChatGPT page target"));
    }
}
