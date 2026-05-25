use serde_json::{Value, json};

use crate::protocol_proxy_conversion_request_mapping::{
    append_responses_input, append_responses_input_as_anthropic,
    responses_tool_choice_to_anthropic, responses_tool_choice_to_chat,
    responses_tool_to_anthropic_tool, responses_tool_to_chat_tool,
};
use crate::protocol_proxy_conversion_shared::{
    response_text, supports_max_completion_tokens, supports_reasoning_effort,
};

const EXTRA_CHAT_PASSTHROUGH_FIELDS: &[&str] = &[
    "frequency_penalty",
    "logit_bias",
    "logprobs",
    "metadata",
    "n",
    "parallel_tool_calls",
    "presence_penalty",
    "response_format",
    "seed",
    "service_tier",
    "stop",
    "stream_options",
    "top_logprobs",
    "user",
];

pub fn responses_to_chat_completions(body: Value) -> anyhow::Result<Value> {
    let mut result = json!({});

    if let Some(model) = body.get("model") {
        result["model"] = model.clone();
    }

    let mut messages = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        let text = response_text(instructions);
        if !text.is_empty() {
            messages.push(json!({ "role": "system", "content": text }));
        }
    }

    if let Some(input) = body.get("input") {
        append_responses_input(input, &mut messages);
    }
    result["messages"] = json!(messages);

    let model = body.get("model").and_then(Value::as_str).unwrap_or("");
    if let Some(value) = body.get("max_output_tokens") {
        if supports_max_completion_tokens(model) {
            result["max_completion_tokens"] = value.clone();
        } else {
            result["max_tokens"] = value.clone();
        }
    }
    if let Some(value) = body.get("max_tokens") {
        result["max_tokens"] = value.clone();
    }
    if let Some(value) = body.get("max_completion_tokens") {
        result["max_completion_tokens"] = value.clone();
    }

    for key in ["temperature", "top_p", "stream"] {
        if let Some(value) = body.get(key) {
            result[key] = value.clone();
        }
    }

    if supports_reasoning_effort(model)
        && let Some(effort) = body.pointer("/reasoning/effort")
    {
        result["reasoning_effort"] = effort.clone();
    }

    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        let converted = tools
            .iter()
            .filter_map(responses_tool_to_chat_tool)
            .collect::<Vec<_>>();
        if !converted.is_empty() {
            result["tools"] = json!(converted);
        }
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        result["tool_choice"] = responses_tool_choice_to_chat(tool_choice);
    }

    for key in EXTRA_CHAT_PASSTHROUGH_FIELDS {
        if let Some(value) = body.get(*key) {
            result[*key] = value.clone();
        }
    }

    Ok(result)
}

pub fn responses_to_anthropic_messages(body: Value) -> anyhow::Result<Value> {
    let mut result = json!({});

    if let Some(model) = body.get("model") {
        result["model"] = model.clone();
    }

    if let Some(instructions) = body.get("instructions") {
        let text = response_text(instructions);
        if !text.is_empty() {
            result["system"] = json!(text);
        }
    }

    let mut messages = Vec::new();
    if let Some(input) = body.get("input") {
        append_responses_input_as_anthropic(input, &mut messages);
    }
    result["messages"] = json!(messages);

    if let Some(value) = body.get("max_output_tokens") {
        result["max_tokens"] = value.clone();
    } else if let Some(value) = body.get("max_tokens") {
        result["max_tokens"] = value.clone();
    } else if let Some(value) = body.get("max_completion_tokens") {
        result["max_tokens"] = value.clone();
    } else {
        result["max_tokens"] = json!(4096);
    }

    for key in ["temperature", "top_p", "stream"] {
        if let Some(value) = body.get(key) {
            result[key] = value.clone();
        }
    }

    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        let converted = tools
            .iter()
            .filter_map(responses_tool_to_anthropic_tool)
            .collect::<Vec<_>>();
        if !converted.is_empty() {
            result["tools"] = json!(converted);
        }
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        let mapped = responses_tool_choice_to_anthropic(tool_choice);
        if !mapped.is_null() {
            result["tool_choice"] = mapped;
        }
    }

    Ok(result)
}
