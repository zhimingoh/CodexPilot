use serde_json::{Value, json};

const THINK_OPEN_TAG: &str = "<think>";
const THINK_CLOSE_TAG: &str = "</think>";

pub(crate) fn response_id_from_chat_id(id: Option<&str>) -> String {
    id.map(|value| {
        if value.starts_with("resp_") {
            value.to_string()
        } else {
            format!("resp_{value}")
        }
    })
    .unwrap_or_else(|| "resp_codexpilot".to_string())
}

pub(crate) fn chat_delta_reasoning_text(delta: &Value) -> Option<String> {
    for key in ["reasoning_content", "reasoning"] {
        if let Some(text) = delta.get(key).and_then(Value::as_str) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    let reasoning = delta.get("reasoning")?;
    for key in ["content", "text", "summary"] {
        if let Some(text) = reasoning.get(key).and_then(Value::as_str) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

pub(crate) enum ThinkPrefixDecision {
    NeedMore,
    Reasoning,
    Text,
}

pub(crate) fn leading_think_prefix_decision(buffer: &str) -> ThinkPrefixDecision {
    let trimmed = buffer.trim_start();
    if trimmed.is_empty() {
        return ThinkPrefixDecision::NeedMore;
    }
    if trimmed.starts_with(THINK_OPEN_TAG) {
        return ThinkPrefixDecision::Reasoning;
    }
    if THINK_OPEN_TAG.starts_with(trimmed) {
        return ThinkPrefixDecision::NeedMore;
    }
    ThinkPrefixDecision::Text
}

pub(crate) fn split_leading_think_block(text: &str) -> Option<(String, String)> {
    let leading_ws_len = text.len() - text.trim_start().len();
    let after_ws = &text[leading_ws_len..];
    if !after_ws.starts_with(THINK_OPEN_TAG) {
        return None;
    }
    let body_start = leading_ws_len + THINK_OPEN_TAG.len();
    let close_relative = text[body_start..].find(THINK_CLOSE_TAG)?;
    let close_start = body_start + close_relative;
    let answer_start = close_start + THINK_CLOSE_TAG.len();
    Some((
        text[body_start..close_start].trim().to_string(),
        strip_think_answer_separator(&text[answer_start..]).to_string(),
    ))
}

pub(crate) fn strip_leading_think_open_tag(text: &str) -> Option<String> {
    let leading_ws_len = text.len() - text.trim_start().len();
    let after_ws = &text[leading_ws_len..];
    after_ws
        .strip_prefix(THINK_OPEN_TAG)
        .map(|value| value.trim().to_string())
}

pub(crate) fn default_responses_usage() -> Value {
    json!({ "input_tokens": 0, "output_tokens": 0, "total_tokens": 0 })
}

pub(crate) fn chat_usage_to_responses_usage(usage: Option<&Value>) -> Value {
    let Some(usage) = usage.filter(|value| value.is_object() && !value.is_null()) else {
        return default_responses_usage();
    };
    let input_tokens = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "total_tokens": usage
            .get("total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(input_tokens + output_tokens)
    })
}

pub(crate) fn anthropic_usage_to_responses_usage(usage: Option<&Value>) -> Value {
    let Some(usage) = usage.filter(|value| value.is_object() && !value.is_null()) else {
        return default_responses_usage();
    };
    let input_tokens = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "total_tokens": input_tokens + output_tokens
    })
}

pub(crate) fn response_status(finish_reason: Option<&str>) -> &'static str {
    match finish_reason {
        Some("length") => "incomplete",
        _ => "completed",
    }
}

pub(crate) fn anthropic_stop_reason_to_response_status(stop_reason: Option<&str>) -> &'static str {
    match stop_reason {
        Some("max_tokens") => "incomplete",
        _ => "completed",
    }
}

pub(crate) fn parse_iso8601_timestamp(value: &str) -> Option<u64> {
    let text = value.trim();
    let date_time = text.strip_suffix('Z').unwrap_or(text);
    let (date, time) = date_time.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i32 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    let time = time.split('.').next().unwrap_or(time);
    let mut time_parts = time.split(':');
    let hour: u32 = time_parts.next()?.parse().ok()?;
    let minute: u32 = time_parts.next()?.parse().ok()?;
    let second: u32 = time_parts.next()?.parse().ok()?;
    unix_timestamp_utc(year, month, day, hour, minute, second)
}

pub(crate) fn supports_max_completion_tokens(model: &str) -> bool {
    model.starts_with("gpt-5") || model.contains("reasoner")
}

pub(crate) fn supports_reasoning_effort(model: &str) -> bool {
    supports_max_completion_tokens(model)
}

pub(crate) fn response_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(response_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(object) => object
            .get("text")
            .or_else(|| object.get("content"))
            .map(response_text)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

pub(crate) fn json_string(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        text.to_string()
    } else {
        serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
    }
}

pub(crate) fn parse_json_or_string(value: &Value) -> Value {
    if let Some(text) = value.as_str() {
        serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!(text))
    } else {
        value.clone()
    }
}

fn strip_think_answer_separator(text: &str) -> &str {
    text.trim_start_matches(['\r', '\n', '\t', ' '])
}

fn unix_timestamp_utc(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<u64> {
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }

    let month_i = month as i64;
    let day_i = day as i64;
    let year_i = year as i64;
    let adjusted_year = year_i - ((14 - month_i) / 12);
    let adjusted_month = month_i + 12 * ((14 - month_i) / 12) - 3;
    let julian_day =
        day_i + ((153 * adjusted_month + 2) / 5) + 365 * adjusted_year + adjusted_year / 4
            - adjusted_year / 100
            + adjusted_year / 400
            - 719_469;
    if julian_day < 0 {
        return None;
    }
    Some(
        (julian_day as u64) * 86_400 + (hour as u64) * 3_600 + (minute as u64) * 60 + second as u64,
    )
}
