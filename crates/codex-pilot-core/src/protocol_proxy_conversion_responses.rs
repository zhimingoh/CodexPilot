use serde_json::{Value, json};

use crate::protocol_proxy_conversion_shared::{
    anthropic_stop_reason_to_response_status, anthropic_usage_to_responses_usage,
    chat_usage_to_responses_usage, json_string, parse_iso8601_timestamp, response_id_from_chat_id,
    response_status, split_leading_think_block,
};

pub fn chat_completion_to_response(body: Value) -> anyhow::Result<Value> {
    let choices = body
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("chat response missing choices"))?;
    let choice = choices
        .first()
        .ok_or_else(|| anyhow::anyhow!("chat response choices is empty"))?;
    let message = choice
        .get("message")
        .ok_or_else(|| anyhow::anyhow!("chat response choice missing message"))?;

    let response_id = response_id_from_chat_id(body.get("id").and_then(Value::as_str));
    let mut output = Vec::new();
    if let Some(reasoning) = chat_reasoning_to_response_output_item(message, &response_id) {
        output.push(reasoning);
    }
    if let Some(message) = chat_message_to_response_output_item(message, &response_id) {
        output.push(message);
    }
    output.extend(chat_tool_calls_to_response_output_items(message));

    let mut response = json!({
        "id": response_id,
        "object": "response",
        "created_at": body.get("created").and_then(Value::as_u64).unwrap_or(0),
        "status": response_status(choice.get("finish_reason").and_then(Value::as_str)),
        "model": body.get("model").and_then(Value::as_str).unwrap_or(""),
        "output": output,
        "usage": chat_usage_to_responses_usage(body.get("usage"))
    });

    if choice.get("finish_reason").and_then(Value::as_str) == Some("length") {
        response["incomplete_details"] = json!({ "reason": "max_output_tokens" });
    }

    Ok(response)
}

pub fn anthropic_message_to_response(body: Value) -> anyhow::Result<Value> {
    let response_id = response_id_from_chat_id(body.get("id").and_then(Value::as_str));
    let stop_reason = body.get("stop_reason").and_then(Value::as_str);
    let model = body.get("model").and_then(Value::as_str).unwrap_or("");
    let created_at = body
        .get("created_at")
        .and_then(Value::as_str)
        .and_then(parse_iso8601_timestamp)
        .unwrap_or(0);

    let mut output = Vec::new();
    let mut text_content = Vec::new();
    if let Some(content) = body.get("content").and_then(Value::as_array) {
        for (index, block) in content.iter().enumerate() {
            match block.get("type").and_then(Value::as_str).unwrap_or("") {
                "thinking" => {
                    if let Some(text) = block.get("thinking").and_then(Value::as_str)
                        && !text.is_empty()
                    {
                        output.push(json!({
                            "id": format!("rs_{response_id}_{index}"),
                            "type": "reasoning",
                            "summary": [{ "type": "summary_text", "text": text }]
                        }));
                    }
                }
                "text" => {
                    if let Some(text) = block.get("text").and_then(Value::as_str)
                        && !text.is_empty()
                    {
                        text_content.push(
                            json!({ "type": "output_text", "text": text, "annotations": [] }),
                        );
                    }
                }
                "tool_use" => {
                    let call_id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    output.push(json!({
                        "id": format!("fc_{call_id}"),
                        "type": "function_call",
                        "status": "completed",
                        "call_id": call_id,
                        "name": block.get("name").and_then(Value::as_str).unwrap_or(""),
                        "arguments": json_string(block.get("input").unwrap_or(&json!({})))
                    }));
                }
                _ => {}
            }
        }
    }

    if !text_content.is_empty() {
        output.insert(
            output
                .iter()
                .take_while(|item| item.get("type").and_then(Value::as_str) == Some("reasoning"))
                .count(),
            json!({
                "id": format!("{response_id}_msg"),
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": text_content
            }),
        );
    }

    let mut response = json!({
        "id": response_id,
        "object": "response",
        "created_at": created_at,
        "status": anthropic_stop_reason_to_response_status(stop_reason),
        "model": model,
        "output": output,
        "usage": anthropic_usage_to_responses_usage(body.get("usage"))
    });

    if stop_reason == Some("max_tokens") {
        response["incomplete_details"] = json!({ "reason": "max_output_tokens" });
    }

    Ok(response)
}

fn chat_reasoning_to_response_output_item(message: &Value, response_id: &str) -> Option<Value> {
    let reasoning = chat_reasoning_text(message)?;
    if reasoning.is_empty() {
        return None;
    }
    Some(json!({
        "id": format!("rs_{response_id}"),
        "type": "reasoning",
        "summary": [{ "type": "summary_text", "text": reasoning }]
    }))
}

fn chat_reasoning_text(message: &Value) -> Option<String> {
    for key in ["reasoning_content", "reasoning"] {
        if let Some(text) = message.get(key).and_then(Value::as_str) && !text.is_empty() {
            return Some(text.to_string());
        }
    }

    if let Some(reasoning) = message.get("reasoning") {
        for key in ["content", "text", "summary"] {
            if let Some(text) = reasoning.get(key).and_then(Value::as_str) && !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }

    if let Some(content) = message.get("content").and_then(Value::as_str)
        && let Some((reasoning, _answer)) = split_leading_think_block(content)
        && !reasoning.is_empty()
    {
        return Some(reasoning);
    }

    None
}

fn chat_message_to_response_output_item(message: &Value, response_id: &str) -> Option<Value> {
    let mut content = Vec::new();
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        let text = split_leading_think_block(text)
            .map(|(_reasoning, answer)| answer)
            .unwrap_or_else(|| text.to_string());
        if !text.is_empty() {
            content.push(json!({ "type": "output_text", "text": text, "annotations": [] }));
        }
    } else if let Some(parts) = message.get("content").and_then(Value::as_array) {
        for part in parts {
            match part.get("type").and_then(Value::as_str).unwrap_or("") {
                "text" | "output_text" => {
                    if let Some(text) = part.get("text").and_then(Value::as_str)
                        && !text.is_empty()
                    {
                        content.push(
                            json!({ "type": "output_text", "text": text, "annotations": [] }),
                        );
                    }
                }
                "refusal" => {
                    if let Some(refusal) = part.get("refusal").and_then(Value::as_str)
                        && !refusal.is_empty()
                    {
                        content.push(json!({ "type": "refusal", "refusal": refusal }));
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(refusal) = message.get("refusal").and_then(Value::as_str) && !refusal.is_empty() {
        content.push(json!({ "type": "refusal", "refusal": refusal }));
    }

    if content.is_empty() {
        return None;
    }

    Some(json!({
        "id": format!("{response_id}_msg"),
        "type": "message",
        "status": "completed",
        "role": "assistant",
        "content": content
    }))
}

fn chat_tool_calls_to_response_output_items(message: &Value) -> Vec<Value> {
    let mut output = Vec::new();
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, tool_call) in tool_calls.iter().enumerate() {
            output.push(chat_tool_call_to_response_item(tool_call, index));
        }
    }
    output
}

fn chat_tool_call_to_response_item(tool_call: &Value, index: usize) -> Value {
    let call_id = tool_call
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("call_{index}"));
    let function = tool_call.get("function").unwrap_or(&Value::Null);
    let name = function.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = json_string(function.get("arguments").unwrap_or(&json!({})));
    json!({
        "id": format!("fc_{call_id}"),
        "type": "function_call",
        "status": "completed",
        "call_id": call_id,
        "name": name,
        "arguments": arguments
    })
}
