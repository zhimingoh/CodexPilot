use serde_json::{Value, json};

use crate::protocol_proxy_conversion_shared::{json_string, parse_json_or_string, response_text};

pub(crate) fn append_responses_input(input: &Value, messages: &mut Vec<Value>) {
    match input {
        Value::String(text) => messages.push(json!({ "role": "user", "content": text })),
        Value::Array(items) => {
            let mut pending_tool_calls = Vec::new();
            let mut pending_reasoning = None;
            for item in items {
                append_responses_item(
                    item,
                    messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                );
            }
            flush_tool_calls(messages, &mut pending_tool_calls, &mut pending_reasoning);
            flush_pending_reasoning(messages, &mut pending_reasoning);
        }
        Value::Object(_) => {
            let mut pending_tool_calls = Vec::new();
            let mut pending_reasoning = None;
            append_responses_item(
                input,
                messages,
                &mut pending_tool_calls,
                &mut pending_reasoning,
            );
            flush_tool_calls(messages, &mut pending_tool_calls, &mut pending_reasoning);
            flush_pending_reasoning(messages, &mut pending_reasoning);
        }
        _ => {}
    }
}

pub(crate) fn append_responses_input_as_anthropic(input: &Value, messages: &mut Vec<Value>) {
    match input {
        Value::String(text) => messages.push(json!({
            "role": "user",
            "content": [{ "type": "text", "text": text }]
        })),
        Value::Array(items) => {
            let mut pending_assistant_tool_calls = Vec::new();
            for item in items {
                append_responses_item_as_anthropic(
                    item,
                    messages,
                    &mut pending_assistant_tool_calls,
                );
            }
            flush_anthropic_tool_calls(messages, &mut pending_assistant_tool_calls);
        }
        Value::Object(_) => {
            let mut pending_assistant_tool_calls = Vec::new();
            append_responses_item_as_anthropic(input, messages, &mut pending_assistant_tool_calls);
            flush_anthropic_tool_calls(messages, &mut pending_assistant_tool_calls);
        }
        _ => {}
    }
}

pub(crate) fn responses_tool_to_chat_tool(tool: &Value) -> Option<Value> {
    if tool.get("type").and_then(Value::as_str) != Some("function") {
        return None;
    }
    if tool.get("function").is_some() {
        return Some(tool.clone());
    }
    Some(json!({
        "type": "function",
        "function": {
            "name": tool.get("name").and_then(Value::as_str).unwrap_or(""),
            "description": tool.get("description").cloned().unwrap_or(Value::Null),
            "parameters": tool.get("parameters").cloned().unwrap_or_else(|| json!({}))
        }
    }))
}

pub(crate) fn responses_tool_to_anthropic_tool(tool: &Value) -> Option<Value> {
    if tool.get("type").and_then(Value::as_str) != Some("function") {
        return None;
    }
    let function = tool.get("function").unwrap_or(tool);
    Some(json!({
        "name": function.get("name").and_then(Value::as_str).unwrap_or(""),
        "description": function.get("description").cloned().unwrap_or(Value::Null),
        "input_schema": function.get("parameters").cloned().unwrap_or_else(|| json!({}))
    }))
}

pub(crate) fn responses_tool_choice_to_chat(tool_choice: &Value) -> Value {
    match tool_choice {
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("function") => {
            json!({
                "type": "function",
                "function": {
                    "name": object.get("name").and_then(Value::as_str).unwrap_or("")
                }
            })
        }
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("required") => {
            json!("required")
        }
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("auto") => {
            json!("auto")
        }
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("none") => {
            json!("none")
        }
        other => other.clone(),
    }
}

pub(crate) fn responses_tool_choice_to_anthropic(tool_choice: &Value) -> Value {
    match tool_choice {
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("function") => {
            json!({
                "type": "tool",
                "name": object.get("name").and_then(Value::as_str).unwrap_or("")
            })
        }
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("required") => {
            json!({ "type": "any" })
        }
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("auto") => {
            json!({ "type": "auto" })
        }
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("none") => {
            json!({ "type": "none" })
        }
        Value::String(text) => match text.as_str() {
            "required" => json!({ "type": "any" }),
            "auto" => json!({ "type": "auto" }),
            "none" => json!({ "type": "none" }),
            _ => Value::Null,
        },
        _ => Value::Null,
    }
}

fn append_responses_item(
    item: &Value,
    messages: &mut Vec<Value>,
    pending_tool_calls: &mut Vec<Value>,
    pending_reasoning: &mut Option<String>,
) {
    match item.get("type").and_then(Value::as_str) {
        Some("function_call") => pending_tool_calls.push(json!({
            "id": item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .unwrap_or(""),
            "type": "function",
            "function": {
                "name": item.get("name").and_then(Value::as_str).unwrap_or(""),
                "arguments": json_string(item.get("arguments").unwrap_or(&json!({})))
            }
        })),
        Some("function_call_output") => {
            flush_tool_calls(messages, pending_tool_calls, pending_reasoning);
            messages.push(json!({
                "role": "tool",
                "tool_call_id": item.get("call_id").and_then(Value::as_str).unwrap_or(""),
                "content": response_text(item.get("output").unwrap_or(&Value::Null))
            }));
        }
        Some("reasoning") => {
            if let Some(text) = responses_reasoning_text(item) && !text.is_empty() {
                *pending_reasoning = Some(text);
            }
        }
        _ => {
            flush_tool_calls(messages, pending_tool_calls, pending_reasoning);
            if item.get("role").is_some() || item.get("content").is_some() {
                let role = responses_role_to_chat_role(item.get("role").and_then(Value::as_str));
                let mut message = json!({
                    "role": role,
                    "content": responses_content_to_chat_content(
                        item.get("content").unwrap_or(&Value::Null)
                    )
                });
                if role == "assistant" {
                    if let Some(reasoning) = pending_reasoning.take() {
                        message["reasoning_content"] = json!(reasoning);
                    }
                }
                messages.push(message);
            }
        }
    }
}

fn append_responses_item_as_anthropic(
    item: &Value,
    messages: &mut Vec<Value>,
    pending_assistant_tool_calls: &mut Vec<Value>,
) {
    match item.get("type").and_then(Value::as_str) {
        Some("function_call") => pending_assistant_tool_calls.push(json!({
            "type": "tool_use",
            "id": item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .unwrap_or(""),
            "name": item.get("name").and_then(Value::as_str).unwrap_or(""),
            "input": parse_json_or_string(item.get("arguments").unwrap_or(&json!({})))
        })),
        Some("function_call_output") => {
            flush_anthropic_tool_calls(messages, pending_assistant_tool_calls);
            messages.push(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": item.get("call_id").and_then(Value::as_str).unwrap_or(""),
                    "content": response_text(item.get("output").unwrap_or(&Value::Null))
                }]
            }));
        }
        Some("reasoning") => {
            let text = responses_reasoning_text(item).unwrap_or_default();
            if !text.is_empty() {
                messages.push(json!({
                    "role": "assistant",
                    "content": [{ "type": "thinking", "thinking": text }]
                }));
            }
        }
        _ => {
            flush_anthropic_tool_calls(messages, pending_assistant_tool_calls);
            if item.get("role").is_some() || item.get("content").is_some() {
                let role =
                    responses_role_to_anthropic_role(item.get("role").and_then(Value::as_str));
                let content = responses_content_to_anthropic_content(
                    item.get("content").unwrap_or(&Value::Null),
                );
                if !content.is_empty() {
                    messages.push(json!({
                        "role": role,
                        "content": content
                    }));
                }
            }
        }
    }
}

fn responses_role_to_chat_role(role: Option<&str>) -> &'static str {
    match role {
        Some("developer") | Some("system") => "system",
        Some("assistant") => "assistant",
        Some("tool") => "tool",
        Some("user") | None => "user",
        Some(_) => "user",
    }
}

fn responses_role_to_anthropic_role(role: Option<&str>) -> &'static str {
    match role {
        Some("assistant") => "assistant",
        Some("tool") => "user",
        Some("developer") | Some("system") | Some("user") | None => "user",
        Some(_) => "user",
    }
}

fn flush_tool_calls(
    messages: &mut Vec<Value>,
    pending_tool_calls: &mut Vec<Value>,
    pending_reasoning: &mut Option<String>,
) {
    if pending_tool_calls.is_empty() {
        return;
    }
    let mut message = json!({
        "role": "assistant",
        "content": Value::Null,
        "tool_calls": std::mem::take(pending_tool_calls)
    });
    if let Some(reasoning) = pending_reasoning.take() {
        message["reasoning_content"] = json!(reasoning);
    }
    messages.push(message);
}

fn flush_anthropic_tool_calls(
    messages: &mut Vec<Value>,
    pending_assistant_tool_calls: &mut Vec<Value>,
) {
    if pending_assistant_tool_calls.is_empty() {
        return;
    }
    messages.push(json!({
        "role": "assistant",
        "content": std::mem::take(pending_assistant_tool_calls)
    }));
}

fn flush_pending_reasoning(messages: &mut Vec<Value>, pending_reasoning: &mut Option<String>) {
    let Some(reasoning) = pending_reasoning.take() else {
        return;
    };
    messages.push(json!({
        "role": "assistant",
        "content": Value::Null,
        "reasoning_content": reasoning
    }));
}

fn responses_reasoning_text(item: &Value) -> Option<String> {
    if let Some(text) = item.get("reasoning_content").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    if let Some(text) = item.get("text").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    if let Some(text) = item.get("content").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    if let Some(summary) = item.get("summary").and_then(Value::as_array) {
        let text = summary
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .or_else(|| part.get("content"))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>()
            .join("");
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

fn responses_content_to_chat_content(content: &Value) -> Value {
    if content.is_null() || content.is_string() {
        return content.clone();
    }

    let Some(parts) = content.as_array() else {
        return content.clone();
    };
    let mut text = Vec::new();
    let mut rich_parts = Vec::new();
    let mut has_non_text = false;

    for part in parts {
        match part.get("type").and_then(Value::as_str).unwrap_or("") {
            "input_text" | "output_text" | "text" => {
                if let Some(value) = part.get("text").and_then(Value::as_str)
                    && !value.is_empty()
                {
                    text.push(value.to_string());
                    rich_parts.push(json!({ "type": "text", "text": value }));
                }
            }
            "refusal" => {
                if let Some(value) = part.get("refusal").and_then(Value::as_str)
                    && !value.is_empty()
                {
                    text.push(value.to_string());
                    rich_parts.push(json!({ "type": "text", "text": value }));
                }
            }
            "input_image" => {
                has_non_text = true;
                if let Some(image_url) = part.get("image_url") {
                    let image_url = if image_url.is_object() {
                        image_url.clone()
                    } else {
                        json!({ "url": image_url.as_str().unwrap_or_default() })
                    };
                    rich_parts.push(json!({ "type": "image_url", "image_url": image_url }));
                }
            }
            _ => {}
        }
    }

    if has_non_text {
        Value::Array(rich_parts)
    } else {
        Value::String(text.join("\n"))
    }
}

fn responses_content_to_anthropic_content(content: &Value) -> Vec<Value> {
    if content.is_null() {
        return Vec::new();
    }
    if let Some(text) = content.as_str() {
        if text.is_empty() {
            return Vec::new();
        }
        return vec![json!({ "type": "text", "text": text })];
    }

    let Some(parts) = content.as_array() else {
        let text = response_text(content);
        return if text.is_empty() {
            Vec::new()
        } else {
            vec![json!({ "type": "text", "text": text })]
        };
    };

    let mut out = Vec::new();
    for part in parts {
        match part.get("type").and_then(Value::as_str).unwrap_or("") {
            "input_text" | "output_text" | "text" => {
                if let Some(text) = part.get("text").and_then(Value::as_str)
                    && !text.is_empty()
                {
                    out.push(json!({ "type": "text", "text": text }));
                }
            }
            "refusal" => {
                if let Some(text) = part.get("refusal").and_then(Value::as_str)
                    && !text.is_empty()
                {
                    out.push(json!({ "type": "text", "text": text }));
                }
            }
            _ => {}
        }
    }
    out
}
