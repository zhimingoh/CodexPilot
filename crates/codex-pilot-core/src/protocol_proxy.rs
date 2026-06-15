use serde_json::Value;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyHttpResponse {
    pub status: String,
    pub content_type: String,
    pub body: Vec<u8>,
}

pub struct UpstreamStreamResponse {
    pub status_code: u16,
    pub content_type: String,
    pub response: reqwest::Response,
}

pub use crate::protocol_proxy_conversion::{
    anthropic_message_to_response, chat_completion_to_response, responses_to_anthropic_messages,
    responses_to_chat_completions,
};
use crate::protocol_proxy_routes::responses_url;
pub use crate::protocol_proxy_routes::{
    ActiveProxyTarget, DEFAULT_PROTOCOL_PROXY_PORT, RouteMode, UpstreamProtocol,
    anthropic_messages_url, chat_completions_url, is_models_proxy_path, is_responses_proxy_path,
    local_responses_proxy_base_url, models_url, proxy_base_url_for_protocol,
    route_mode_for_protocol,
};
pub use crate::protocol_proxy_sse::{
    AnthropicSseToResponsesConverter, ChatSseToResponsesConverter, anthropic_sse_to_responses_sse,
    chat_sse_to_responses_sse,
};
use crate::protocol_proxy_transport::{
    open_anthropic_messages_upstream, open_chat_completions_upstream,
    passthrough_anthropic_models_request, passthrough_chat_models_request,
    passthrough_models_request, passthrough_responses_request,
};

pub async fn handle_responses_proxy_request(
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<ProxyHttpResponse> {
    match target.protocol {
        UpstreamProtocol::Responses => {
            passthrough_responses_request(target, body, &responses_url(&target.base_url)).await
        }
        UpstreamProtocol::ChatCompletions => chat_completions_responses_request(target, body).await,
        UpstreamProtocol::AnthropicMessages => {
            anthropic_messages_responses_request(target, body).await
        }
    }
}

pub fn responses_request_wants_stream(body: &str) -> bool {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| value.get("stream").and_then(Value::as_bool))
        .unwrap_or(false)
}

pub async fn handle_models_proxy_request(
    target: &ActiveProxyTarget,
) -> anyhow::Result<ProxyHttpResponse> {
    match target.protocol {
        UpstreamProtocol::Responses => passthrough_models_request(target).await,
        UpstreamProtocol::ChatCompletions => passthrough_chat_models_request(target).await,
        UpstreamProtocol::AnthropicMessages => passthrough_anthropic_models_request(target).await,
    }
}

async fn chat_completions_responses_request(
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<ProxyHttpResponse> {
    let is_stream = responses_request_wants_stream(body);
    let upstream = open_chat_completions_upstream(target, body).await?;
    let status_code = upstream.status_code;
    let content_type = upstream.content_type.clone();
    let upstream_body = upstream.response.bytes().await?;

    if !(200..300).contains(&status_code) {
        return Ok(ProxyHttpResponse {
            status: http_status_line(status_code),
            content_type,
            body: upstream_body.to_vec(),
        });
    }

    if is_stream || content_type.contains("text/event-stream") {
        let text = String::from_utf8_lossy(&upstream_body);
        return Ok(ProxyHttpResponse {
            status: "200 OK".to_string(),
            content_type: "text/event-stream; charset=utf-8".to_string(),
            body: chat_sse_to_responses_sse(&text).into_bytes(),
        });
    }

    let chat_json: Value = serde_json::from_slice(&upstream_body)?;
    let response_json = chat_completion_to_response(chat_json)?;
    Ok(ProxyHttpResponse {
        status: "200 OK".to_string(),
        content_type: "application/json; charset=utf-8".to_string(),
        body: serde_json::to_vec(&response_json)?,
    })
}

async fn anthropic_messages_responses_request(
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<ProxyHttpResponse> {
    let is_stream = responses_request_wants_stream(body);
    let upstream = open_anthropic_messages_upstream(target, body).await?;
    let status_code = upstream.status_code;
    let content_type = upstream.content_type.clone();
    let upstream_body = upstream.response.bytes().await?;

    if !(200..300).contains(&status_code) {
        return Ok(ProxyHttpResponse {
            status: http_status_line(status_code),
            content_type,
            body: upstream_body.to_vec(),
        });
    }

    if is_stream || content_type.contains("text/event-stream") {
        let text = String::from_utf8_lossy(&upstream_body);
        return Ok(ProxyHttpResponse {
            status: "200 OK".to_string(),
            content_type: "text/event-stream; charset=utf-8".to_string(),
            body: anthropic_sse_to_responses_sse(&text).into_bytes(),
        });
    }

    let anthropic_json: Value = serde_json::from_slice(&upstream_body)?;
    let response_json = anthropic_message_to_response(anthropic_json)?;
    Ok(ProxyHttpResponse {
        status: "200 OK".to_string(),
        content_type: "application/json; charset=utf-8".to_string(),
        body: serde_json::to_vec(&response_json)?,
    })
}

pub async fn stream_chat_completions_as_responses(
    stream: &mut tokio::net::TcpStream,
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<()> {
    let upstream = open_chat_completions_upstream(target, body).await?;
    if !(200..300).contains(&upstream.status_code) {
        let body = upstream.response.bytes().await?.to_vec();
        write_http_response(
            stream,
            &http_status_line(upstream.status_code),
            if upstream.content_type.is_empty() {
                "application/json; charset=utf-8"
            } else {
                &upstream.content_type
            },
            &body,
        )
        .await?;
        return Ok(());
    }

    write_http_stream_headers(stream, "200 OK", "text/event-stream; charset=utf-8").await?;
    let mut converter = ChatSseToResponsesConverter::default();
    let mut response = upstream.response;
    loop {
        match response.chunk().await {
            Ok(Some(bytes)) => {
                let converted = converter.push_bytes(&bytes);
                if !converted.is_empty() {
                    stream.write_all(&converted).await?;
                }
            }
            Ok(None) => break,
            Err(error) => {
                let failed = converter.fail_stream(format!("Stream error: {error}"));
                if !failed.is_empty() {
                    stream.write_all(&failed).await?;
                }
                stream.shutdown().await?;
                return Ok(());
            }
        }
    }

    let tail = converter.finish();
    if !tail.is_empty() {
        stream.write_all(&tail).await?;
    }
    stream.shutdown().await?;
    Ok(())
}

pub async fn stream_anthropic_messages_as_responses(
    stream: &mut tokio::net::TcpStream,
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<()> {
    let upstream = open_anthropic_messages_upstream(target, body).await?;
    if !(200..300).contains(&upstream.status_code) {
        let body = upstream.response.bytes().await?.to_vec();
        write_http_response(
            stream,
            &http_status_line(upstream.status_code),
            if upstream.content_type.is_empty() {
                "application/json; charset=utf-8"
            } else {
                &upstream.content_type
            },
            &body,
        )
        .await?;
        return Ok(());
    }

    write_http_stream_headers(stream, "200 OK", "text/event-stream; charset=utf-8").await?;
    let mut converter = AnthropicSseToResponsesConverter::default();
    let mut response = upstream.response;
    loop {
        match response.chunk().await {
            Ok(Some(bytes)) => {
                let converted = converter.push_bytes(&bytes);
                if !converted.is_empty() {
                    stream.write_all(&converted).await?;
                }
            }
            Ok(None) => break,
            Err(error) => {
                let failed = converter.fail_stream(format!("Stream error: {error}"));
                if !failed.is_empty() {
                    stream.write_all(&failed).await?;
                }
                stream.shutdown().await?;
                return Ok(());
            }
        }
    }

    let tail = converter.finish();
    if !tail.is_empty() {
        stream.write_all(&tail).await?;
    }
    stream.shutdown().await?;
    Ok(())
}

pub(crate) fn http_status_line(status: u16) -> String {
    match status {
        200 => "200 OK".to_string(),
        204 => "204 No Content".to_string(),
        400 => "400 Bad Request".to_string(),
        401 => "401 Unauthorized".to_string(),
        403 => "403 Forbidden".to_string(),
        404 => "404 Not Found".to_string(),
        429 => "429 Too Many Requests".to_string(),
        500 => "500 Internal Server Error".to_string(),
        502 => "502 Bad Gateway".to_string(),
        503 => "503 Service Unavailable".to_string(),
        _ => format!("{status} Upstream"),
    }
}

async fn write_http_response(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> anyhow::Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.write_all(body).await?;
    Ok(())
}

async fn write_http_stream_headers(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    content_type: &str,
) -> anyhow::Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
#[path = "protocol_proxy_tests.rs"]
mod tests;
