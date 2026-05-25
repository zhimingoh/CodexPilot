use serde_json::Value;

use crate::protocol_proxy_sse_common::{
    append_utf8_safe, strip_sse_field, take_sse_block, ReasoningItemState, TextItemState,
};

pub fn chat_sse_to_responses_sse(input: &str) -> String {
    let mut converter = ChatSseToResponsesConverter::default();
    let mut output = converter.push_bytes(input.as_bytes());
    output.extend(converter.finish());
    String::from_utf8(output).unwrap_or_default()
}

pub fn anthropic_sse_to_responses_sse(input: &str) -> String {
    let mut converter = AnthropicSseToResponsesConverter::default();
    let mut output = converter.push_bytes(input.as_bytes());
    output.extend(converter.finish());
    String::from_utf8(output).unwrap_or_default()
}

pub struct ChatSseToResponsesConverter {
    buffer: String,
    utf8_remainder: Vec<u8>,
    state: ChatSseState,
    failed: bool,
}

pub struct AnthropicSseToResponsesConverter {
    buffer: String,
    utf8_remainder: Vec<u8>,
    state: AnthropicSseState,
    failed: bool,
}

impl Default for ChatSseToResponsesConverter {
    fn default() -> Self {
        Self {
            buffer: String::new(),
            utf8_remainder: Vec::new(),
            state: ChatSseState::default(),
            failed: false,
        }
    }
}

impl Default for AnthropicSseToResponsesConverter {
    fn default() -> Self {
        Self {
            buffer: String::new(),
            utf8_remainder: Vec::new(),
            state: AnthropicSseState::default(),
            failed: false,
        }
    }
}

impl ChatSseToResponsesConverter {
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<u8> {
        append_utf8_safe(&mut self.buffer, &mut self.utf8_remainder, bytes);
        let mut output = String::new();
        while let Some(block) = take_sse_block(&mut self.buffer) {
            if block.trim().is_empty() {
                continue;
            }
            self.handle_block(&block, &mut output);
            if self.failed {
                break;
            }
        }
        output.into_bytes()
    }

    pub fn finish(&mut self) -> Vec<u8> {
        if !self.utf8_remainder.is_empty() {
            self.buffer
                .push_str(&String::from_utf8_lossy(&self.utf8_remainder));
            self.utf8_remainder.clear();
        }

        let mut output = String::new();
        if !self.failed {
            self.state.finalize_into(&mut output);
        }
        output.into_bytes()
    }

    pub fn fail_stream(&mut self, message: String) -> Vec<u8> {
        let mut failed = String::new();
        self.state.failed_into(&mut failed, message);
        failed.into_bytes()
    }

    fn handle_block(&mut self, block: &str, output: &mut String) {
        let mut event_name: Option<String> = None;
        let mut data_parts = Vec::new();
        for line in block.lines() {
            if let Some(event) = strip_sse_field(line, "event") {
                event_name = Some(event.trim().to_string());
            }
            if let Some(data) = strip_sse_field(line, "data") {
                data_parts.push(data.to_string());
            }
        }

        if data_parts.is_empty() {
            return;
        }
        let data = data_parts.join("\n");
        if data.trim() == "[DONE]" {
            self.state.finalize_into(output);
            return;
        }

        let Ok(chunk) = serde_json::from_str::<Value>(&data) else {
            return;
        };
        if event_name.as_deref() == Some("error") || chunk.get("error").is_some() {
            let message = chunk
                .get("error")
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("upstream stream error")
                .to_string();
            self.state.failed_into(output, message);
            self.failed = true;
            return;
        }
        self.state.handle_chat_chunk_into(&chunk, output);
    }
}

impl AnthropicSseToResponsesConverter {
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<u8> {
        append_utf8_safe(&mut self.buffer, &mut self.utf8_remainder, bytes);
        let mut output = String::new();
        while let Some(block) = take_sse_block(&mut self.buffer) {
            if block.trim().is_empty() {
                continue;
            }
            self.handle_block(&block, &mut output);
            if self.failed {
                break;
            }
        }
        output.into_bytes()
    }

    pub fn finish(&mut self) -> Vec<u8> {
        if !self.utf8_remainder.is_empty() {
            self.buffer
                .push_str(&String::from_utf8_lossy(&self.utf8_remainder));
            self.utf8_remainder.clear();
        }

        let mut output = String::new();
        if !self.failed {
            self.state.finalize_into(&mut output);
        }
        output.into_bytes()
    }

    pub fn fail_stream(&mut self, message: String) -> Vec<u8> {
        let mut failed = String::new();
        self.state.failed_into(&mut failed, message);
        failed.into_bytes()
    }

    fn handle_block(&mut self, block: &str, output: &mut String) {
        let mut event_name: Option<String> = None;
        let mut data_parts = Vec::new();
        for line in block.lines() {
            if let Some(event) = strip_sse_field(line, "event") {
                event_name = Some(event.trim().to_string());
            }
            if let Some(data) = strip_sse_field(line, "data") {
                data_parts.push(data.to_string());
            }
        }

        if data_parts.is_empty() {
            return;
        }
        let data = data_parts.join("\n");
        if data.trim() == "[DONE]" {
            self.state.finalize_into(output);
            return;
        }

        let Ok(chunk) = serde_json::from_str::<Value>(&data) else {
            return;
        };
        if event_name.as_deref() == Some("error") || chunk.get("error").is_some() {
            let message = chunk
                .get("error")
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("upstream stream error")
                .to_string();
            self.state.failed_into(output, message);
            self.failed = true;
            return;
        }

        self.state
            .handle_anthropic_event_into(event_name.as_deref(), &chunk, output);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum InlineThinkMode {
    #[default]
    Detecting,
    Reasoning,
    Text,
}

#[derive(Debug, Default)]
pub(crate) struct InlineThinkState {
    pub(crate) mode: InlineThinkMode,
    pub(crate) buffer: String,
}

#[derive(Debug, Default)]
pub(crate) struct ToolCallState {
    pub(crate) output_index: Option<u32>,
    pub(crate) item_id: String,
    pub(crate) call_id: String,
    pub(crate) name: String,
    pub(crate) arguments: String,
    pub(crate) added: bool,
    pub(crate) done: bool,
}

#[derive(Debug, Default)]
pub(crate) struct AnthropicContentBlockState {
    pub(crate) block_type: String,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) json_input: String,
    pub(crate) output_index: Option<u32>,
    pub(crate) item_id: String,
    pub(crate) added: bool,
    pub(crate) done: bool,
}

#[derive(Debug)]
pub(crate) struct ChatSseState {
    pub(crate) response_started: bool,
    pub(crate) completed: bool,
    pub(crate) response_id: String,
    pub(crate) model: String,
    pub(crate) created_at: u64,
    pub(crate) next_output_index: u32,
    pub(crate) text: TextItemState,
    pub(crate) reasoning: ReasoningItemState,
    pub(crate) inline_think: InlineThinkState,
    pub(crate) tools: std::collections::BTreeMap<usize, ToolCallState>,
    pub(crate) output_items: Vec<(u32, Value)>,
    pub(crate) latest_usage: Option<Value>,
    pub(crate) finish_reason: Option<String>,
}

#[derive(Debug)]
pub(crate) struct AnthropicSseState {
    pub(crate) response_started: bool,
    pub(crate) completed: bool,
    pub(crate) response_id: String,
    pub(crate) model: String,
    pub(crate) created_at: u64,
    pub(crate) next_output_index: u32,
    pub(crate) text: TextItemState,
    pub(crate) reasoning: ReasoningItemState,
    pub(crate) active_blocks: std::collections::BTreeMap<usize, AnthropicContentBlockState>,
    pub(crate) output_items: Vec<(u32, Value)>,
    pub(crate) latest_usage: Option<Value>,
    pub(crate) stop_reason: Option<String>,
}
