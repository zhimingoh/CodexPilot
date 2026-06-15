use serde_json::Value;

use crate::protocol_proxy::{ProxyHttpResponse, UpstreamStreamResponse};
use crate::protocol_proxy_conversion::{
    responses_to_anthropic_messages, responses_to_chat_completions,
};
use crate::protocol_proxy_routes::{
    ActiveProxyTarget, anthropic_messages_url, chat_completions_url, models_url,
};

pub(crate) async fn passthrough_responses_request(
    target: &ActiveProxyTarget,
    body: &str,
    responses_url: &str,
) -> anyhow::Result<ProxyHttpResponse> {
    let payload: Value = serde_json::from_str(body)?;
    let mut request = crate::http_client::shared()
        .post(responses_url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&payload);
    if !target.api_key.trim().is_empty() {
        request = request.bearer_auth(target.api_key.trim());
    }
    let response = request.send().await?;
    proxy_http_response_from_reqwest(response).await
}

pub(crate) async fn passthrough_models_request(
    target: &ActiveProxyTarget,
) -> anyhow::Result<ProxyHttpResponse> {
    let mut request = crate::http_client::shared().get(models_url(&target.base_url));
    if !target.api_key.trim().is_empty() {
        request = request.bearer_auth(target.api_key.trim());
    }
    let response = request.send().await?;
    proxy_http_response_from_reqwest(response).await
}

pub(crate) async fn passthrough_chat_models_request(
    target: &ActiveProxyTarget,
) -> anyhow::Result<ProxyHttpResponse> {
    let mut request = crate::http_client::shared().get(models_url(&target.base_url));
    if !target.api_key.trim().is_empty() {
        request = request.bearer_auth(target.api_key.trim());
    }
    let response = request.send().await?;
    proxy_http_response_from_reqwest(response).await
}

pub(crate) async fn passthrough_anthropic_models_request(
    target: &ActiveProxyTarget,
) -> anyhow::Result<ProxyHttpResponse> {
    let mut request = crate::http_client::shared().get(models_url(&target.base_url));
    request = request
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-dangerous-direct-browser-access", "true");
    if !target.api_key.trim().is_empty() {
        request = request.header("x-api-key", target.api_key.trim());
    }
    let response = request.send().await?;
    proxy_http_response_from_reqwest(response).await
}

pub(crate) async fn open_chat_completions_upstream(
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<UpstreamStreamResponse> {
    let request_json: Value = serde_json::from_str(body)?;
    let chat_request = responses_to_chat_completions(request_json)?;
    let mut request = crate::http_client::shared()
        .post(chat_completions_url(&target.base_url))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&chat_request);
    if !target.api_key.trim().is_empty() {
        request = request.bearer_auth(target.api_key.trim());
    }
    let response = request.send().await?;
    let status_code = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/json; charset=utf-8")
        .to_string();
    Ok(UpstreamStreamResponse {
        status_code,
        content_type,
        response,
    })
}

pub(crate) async fn open_anthropic_messages_upstream(
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<UpstreamStreamResponse> {
    let request_json: Value = serde_json::from_str(body)?;
    let anthropic_request = responses_to_anthropic_messages(request_json)?;
    let mut request = crate::http_client::shared()
        .post(anthropic_messages_url(&target.base_url))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-dangerous-direct-browser-access", "true")
        .json(&anthropic_request);
    if !target.api_key.trim().is_empty() {
        request = request.header("x-api-key", target.api_key.trim());
    }
    let response = request.send().await?;
    let status_code = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/json; charset=utf-8")
        .to_string();
    Ok(UpstreamStreamResponse {
        status_code,
        content_type,
        response,
    })
}

pub(crate) async fn proxy_http_response_from_reqwest(
    response: reqwest::Response,
) -> anyhow::Result<ProxyHttpResponse> {
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/json; charset=utf-8")
        .to_string();
    let body = response.bytes().await?.to_vec();
    Ok(ProxyHttpResponse {
        status: super::protocol_proxy::http_status_line(status.as_u16()),
        content_type,
        body,
    })
}
