use super::*;
use serde_json::json;

#[test]
fn chat_and_anthropic_use_local_proxy_route_mode() {
    assert_eq!(
        route_mode_for_protocol(UpstreamProtocol::ChatCompletions),
        RouteMode::LocalProxy
    );
    assert_eq!(
        route_mode_for_protocol(UpstreamProtocol::AnthropicMessages),
        RouteMode::LocalProxy
    );
    assert_eq!(
        route_mode_for_protocol(UpstreamProtocol::Responses),
        RouteMode::Direct
    );
}

#[test]
fn proxy_base_url_uses_helper_for_proxy_protocols() {
    assert_eq!(
        proxy_base_url_for_protocol(
            "https://api.example.test/v1",
            UpstreamProtocol::ChatCompletions,
            58888
        ),
        "http://127.0.0.1:58888/v1"
    );
    assert_eq!(
        proxy_base_url_for_protocol(
            "https://api.example.test/v1",
            UpstreamProtocol::Responses,
            58888
        ),
        "https://api.example.test/v1"
    );
}

#[test]
fn proxy_path_matchers_cover_v1_routes() {
    assert!(is_responses_proxy_path("/v1/responses"));
    assert!(is_responses_proxy_path("/responses/compact"));
    assert!(is_models_proxy_path("/v1/models?limit=10"));
    assert!(!is_models_proxy_path("/v1/responses"));
}

#[test]
fn responses_request_converts_to_chat_completions() {
    let converted = responses_to_chat_completions(json!({
        "model": "gpt-5-mini",
        "instructions": "You are helpful.",
        "input": [
            {
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "hello" }
                ]
            }
        ],
        "max_output_tokens": 512,
        "temperature": 0.2,
        "stream": true
    }))
    .unwrap();

    assert_eq!(converted["messages"][0]["role"], "system");
    assert_eq!(converted["messages"][1]["content"], "hello");
    assert_eq!(converted["stream"], true);
}

#[test]
fn chat_completion_response_converts_to_responses_response() {
    let converted = chat_completion_to_response(json!({
        "id": "chatcmpl_123",
        "created": 1710000000,
        "model": "gpt-5-mini",
        "choices": [
            {
                "finish_reason": "stop",
                "message": {
                    "role": "assistant",
                    "content": "hi there"
                }
            }
        ],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15
        }
    }))
    .unwrap();

    assert_eq!(converted["object"], "response");
    assert_eq!(converted["output"][0]["type"], "message");
    assert_eq!(converted["usage"]["input_tokens"], 10);
}

#[test]
fn chat_sse_converts_to_responses_sse_events() {
    let converted = chat_sse_to_responses_sse(
        r#"data: {"id":"chatcmpl_1","created":1710000000,"model":"gpt-5-mini","choices":[{"delta":{"content":"hel"},"finish_reason":null}]}

data: {"id":"chatcmpl_1","created":1710000000,"model":"gpt-5-mini","choices":[{"delta":{"content":"lo"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}

data: [DONE]

"#,
    );

    assert!(converted.contains("event: response.created"));
    assert!(converted.contains("event: response.output_text.delta"));
    assert!(converted.contains("data: [DONE]"));
}

#[test]
fn responses_request_converts_to_anthropic_messages() {
    let converted = responses_to_anthropic_messages(json!({
        "model": "claude-sonnet-4-20250514",
        "instructions": "Be careful.",
        "input": [
            {
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "hello" }
                ]
            }
        ],
        "tools": [{
            "type": "function",
            "name": "lookup_weather",
            "description": "check weather",
            "parameters": { "type": "object" }
        }],
        "tool_choice": { "type": "auto" },
        "max_output_tokens": 1024,
        "stream": true
    }))
    .unwrap();

    assert_eq!(converted["system"], "Be careful.");
    assert_eq!(converted["messages"][0]["role"], "user");
    assert_eq!(converted["messages"][0]["content"][0]["text"], "hello");
    assert_eq!(converted["tools"][0]["name"], "lookup_weather");
    assert_eq!(converted["tool_choice"]["type"], "auto");
    assert_eq!(converted["max_tokens"], 1024);
    assert_eq!(converted["stream"], true);
}

#[test]
fn anthropic_message_response_converts_to_responses_response() {
    let converted = anthropic_message_to_response(json!({
        "id": "msg_123",
        "type": "message",
        "role": "assistant",
        "model": "claude-sonnet-4-20250514",
        "created_at": "2026-05-22T12:00:00Z",
        "stop_reason": "end_turn",
        "content": [
            { "type": "thinking", "thinking": "first reason" },
            { "type": "text", "text": "hi there" },
            { "type": "tool_use", "id": "toolu_1", "name": "lookup_weather", "input": { "city": "Shanghai" } }
        ],
        "usage": {
            "input_tokens": 10,
            "output_tokens": 5
        }
    }))
    .unwrap();

    assert_eq!(converted["object"], "response");
    assert_eq!(converted["output"][0]["type"], "reasoning");
    assert_eq!(converted["output"][1]["type"], "message");
    assert_eq!(converted["output"][2]["type"], "function_call");
    assert_eq!(converted["usage"]["input_tokens"], 10);
    assert_eq!(converted["usage"]["output_tokens"], 5);
}

#[test]
fn anthropic_sse_converts_to_responses_sse_events() {
    let converted = anthropic_sse_to_responses_sse(
        r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_1","model":"claude-sonnet-4-20250514","role":"assistant","created_at":"2026-05-22T12:00:00Z","usage":{"input_tokens":3,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"reason"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text"}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"hello"}}

event: content_block_stop
data: {"type":"content_block_stop","index":1}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}

event: message_stop
data: {"type":"message_stop"}

"#,
    );

    assert!(converted.contains("event: response.created"));
    assert!(converted.contains("event: response.reasoning_summary_text.delta"));
    assert!(converted.contains("event: response.output_text.delta"));
    assert!(converted.contains("data: [DONE]"));
}
