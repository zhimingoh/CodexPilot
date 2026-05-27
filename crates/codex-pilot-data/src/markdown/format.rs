use super::MessageBlock;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn exported_at_label() -> String {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return "unknown time".to_string();
    };
    format_unix_utc(duration.as_secs())
}

pub(super) fn format_unix_utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = month_index + if month_index < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month as u32, day as u32)
}

pub(super) fn text_blocks(value: String) -> Vec<MessageBlock> {
    let cleaned = normalize_newlines(&value);
    if cleaned.trim().is_empty() {
        Vec::new()
    } else {
        vec![MessageBlock::Text(cleaned)]
    }
}

pub(super) fn strip_image_tags(value: &str) -> String {
    let mut output = value.to_string();
    loop {
        let Some(start) = output.find("<image>") else {
            break;
        };
        let end = output[start..]
            .find("</image>")
            .map(|offset| start + offset + "</image>".len())
            .unwrap_or(start + "<image>".len());
        output.replace_range(start..end, "");
    }
    output.replace("</image>", "")
}

pub(super) fn extract_image_src(block: &Value) -> Option<String> {
    ["image_url", "url", "path", "file_path", "data_url", "image"]
        .iter()
        .find_map(|key| block.get(*key).and_then(Value::as_str))
        .or_else(|| {
            block
                .get("source")
                .and_then(|source| source.get("url").or_else(|| source.get("path")))
                .and_then(Value::as_str)
        })
        .and_then(|value| {
            let trimmed = value.trim();
            (trimmed.starts_with("data:image/")
                || trimmed.starts_with("file://")
                || trimmed.starts_with('/')
                || trimmed.starts_with("http://")
                || trimmed.starts_with("https://"))
            .then_some(trimmed.to_string())
        })
}

#[derive(Debug)]
pub(super) enum TextPart {
    Plain(String),
    Code(String),
}

pub(super) fn split_fenced_code(value: &str) -> Vec<TextPart> {
    let mut parts = Vec::new();
    let mut plain = Vec::new();
    let mut code = Vec::new();
    let mut in_code = false;

    for line in normalize_newlines(value).lines() {
        if line.trim_start().starts_with("```") {
            if in_code {
                parts.push(TextPart::Code(code.join("\n")));
                code.clear();
                in_code = false;
            } else {
                if !plain.join("\n").trim().is_empty() {
                    parts.push(TextPart::Plain(plain.join("\n")));
                }
                plain.clear();
                in_code = true;
            }
            continue;
        }
        if in_code {
            code.push(line.to_string());
        } else {
            plain.push(line.to_string());
        }
    }

    if in_code {
        plain.push("```".to_string());
        plain.extend(code);
    }
    if !plain.join("\n").trim().is_empty() {
        parts.push(TextPart::Plain(plain.join("\n")));
    }
    parts
}

pub(super) fn parse_time_hhmm(value: &str) -> Option<String> {
    let time_part = value
        .split('T')
        .nth(1)
        .or_else(|| value.split_whitespace().nth(1))?;
    let mut pieces = time_part.split(':');
    let hour = pieces.next()?;
    let minute = pieces.next()?;
    if hour.len() == 2 && minute.len() == 2 {
        Some(format!("{hour}:{minute}"))
    } else {
        None
    }
}

pub(super) fn normalize_newlines(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

pub(super) fn replace_filename_chars(value: &str, replacement: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || ch.is_control() {
            output.push_str(replacement);
        } else {
            output.push(ch);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::format_unix_utc;

    #[test]
    fn formats_unix_time_as_readable_utc() {
        assert_eq!(format_unix_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_unix_utc(1_767_225_600), "2026-01-01 00:00 UTC");
    }
}
