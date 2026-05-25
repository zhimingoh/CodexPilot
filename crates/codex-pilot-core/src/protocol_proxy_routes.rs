use serde::{Deserialize, Serialize};

pub const DEFAULT_PROTOCOL_PROXY_PORT: u16 = crate::ports::DEFAULT_HELPER_PORT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum UpstreamProtocol {
    #[default]
    Responses,
    ChatCompletions,
    AnthropicMessages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMode {
    Direct,
    LocalProxy,
}

#[derive(Debug, Clone)]
pub struct ActiveProxyTarget {
    pub base_url: String,
    pub api_key: String,
    pub protocol: UpstreamProtocol,
}

pub fn local_responses_proxy_base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/v1")
}

pub fn route_mode_for_protocol(protocol: UpstreamProtocol) -> RouteMode {
    match protocol {
        UpstreamProtocol::Responses => RouteMode::Direct,
        UpstreamProtocol::ChatCompletions | UpstreamProtocol::AnthropicMessages => {
            RouteMode::LocalProxy
        }
    }
}

pub fn proxy_base_url_for_protocol(
    base_url: &str,
    protocol: UpstreamProtocol,
    helper_port: u16,
) -> String {
    match route_mode_for_protocol(protocol) {
        RouteMode::Direct => base_url.trim().to_string(),
        RouteMode::LocalProxy => local_responses_proxy_base_url(helper_port),
    }
}

pub fn is_responses_proxy_path(path: &str) -> bool {
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    matches!(
        path,
        "/responses" | "/v1/responses" | "/responses/compact" | "/v1/responses/compact"
    )
}

pub fn is_models_proxy_path(path: &str) -> bool {
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    matches!(path, "/models" | "/v1/models")
}

pub fn chat_completions_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.to_ascii_lowercase().ends_with("/chat/completions") {
        return base.to_string();
    }
    let origin_only = base
        .split_once("://")
        .map_or(!base.contains('/'), |(_, rest)| !rest.contains('/'));
    let mut url = if base.ends_with("/v1") || !origin_only {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    };
    while url.contains("/v1/v1") {
        url = url.replace("/v1/v1", "/v1");
    }
    url
}

pub fn anthropic_messages_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.to_ascii_lowercase().ends_with("/messages") {
        return base.to_string();
    }
    let origin_only = base
        .split_once("://")
        .map_or(!base.contains('/'), |(_, rest)| !rest.contains('/'));
    let mut url = if base.ends_with("/v1") || !origin_only {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    };
    while url.contains("/v1/v1") {
        url = url.replace("/v1/v1", "/v1");
    }
    url
}

pub fn models_url(base_url: &str) -> String {
    let mut base = base_url.trim().trim_end_matches('/').to_string();
    if base.to_ascii_lowercase().ends_with("/chat/completions") {
        base.truncate(base.len() - "/chat/completions".len());
    }
    if base.to_ascii_lowercase().ends_with("/models") {
        return base;
    }
    let origin_only = base
        .split_once("://")
        .map_or(!base.contains('/'), |(_, rest)| !rest.contains('/'));
    let mut url = if base.ends_with("/v1") || !origin_only {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    };
    while url.contains("/v1/v1") {
        url = url.replace("/v1/v1", "/v1");
    }
    url
}

pub(crate) fn responses_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.to_ascii_lowercase().ends_with("/responses") {
        return base.to_string();
    }
    if base.ends_with("/v1") {
        return format!("{base}/responses");
    }
    format!("{base}/v1/responses")
}
