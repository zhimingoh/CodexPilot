use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::protocol_proxy_conversion::{
    anthropic_stop_reason_to_response_status, anthropic_usage_to_responses_usage,
    default_responses_usage, parse_iso8601_timestamp, response_id_from_chat_id,
};
use crate::protocol_proxy_sse::AnthropicSseState;
use crate::protocol_proxy_sse_common::{ReasoningItemState, TextItemState, push_sse};

impl Default for AnthropicSseState {
    fn default() -> Self {
        Self {
            response_started: false,
            completed: false,
            response_id: "resp_codexpilot".to_string(),
            model: String::new(),
            created_at: 0,
            next_output_index: 0,
            text: TextItemState::default(),
            reasoning: ReasoningItemState::default(),
            active_blocks: BTreeMap::new(),
            output_items: Vec::new(),
            latest_usage: None,
            stop_reason: None,
        }
    }
}

impl AnthropicSseState {
    pub(crate) fn handle_anthropic_event_into(
        &mut self,
        event_name: Option<&str>,
        chunk: &Value,
        output: &mut String,
    ) {
        match event_name.unwrap_or_default() {
            "message_start" => {
                if let Some(message) = chunk.get("message") {
                    if let Some(id) = message.get("id").and_then(Value::as_str) {
                        self.response_id = response_id_from_chat_id(Some(id));
                    }
                    if let Some(model) = message.get("model").and_then(Value::as_str) {
                        self.model = model.to_string();
                    }
                    if let Some(created_at) = message.get("created_at").and_then(Value::as_str) {
                        self.created_at = parse_iso8601_timestamp(created_at).unwrap_or(0);
                    }
                    self.latest_usage =
                        Some(anthropic_usage_to_responses_usage(message.get("usage")));
                }
                self.ensure_response_started_into(output);
            }
            "content_block_start" => {
                self.ensure_response_started_into(output);
                let index = chunk.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let block = chunk.get("content_block").unwrap_or(&Value::Null);
                let block_type = block
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let block_id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let block_name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let needs_tool_add = {
                    let state = self.active_blocks.entry(index).or_default();
                    state.block_type = block_type.clone();
                    state.id = block_id.clone();
                    state.name = block_name.clone();
                    block_type == "tool_use" && !state.added
                };
                if needs_tool_add {
                    let output_index = self.next_output_index();
                    let call_id = if block_id.is_empty() {
                        format!("call_{index}")
                    } else {
                        block_id.clone()
                    };
                    let item_id = format!("fc_{call_id}");
                    if let Some(state) = self.active_blocks.get_mut(&index) {
                        state.output_index = Some(output_index);
                        state.added = true;
                        state.item_id = item_id.clone();
                    }
                    push_sse(
                        output,
                        "response.output_item.added",
                        json!({
                            "type": "response.output_item.added",
                            "output_index": output_index,
                            "item": {
                                "id": item_id,
                                "type": "function_call",
                                "status": "in_progress",
                                "call_id": call_id,
                                "name": if block_name.is_empty() { "unknown_tool" } else { &block_name },
                                "arguments": ""
                            }
                        }),
                    );
                }
            }
            "content_block_delta" => {
                self.ensure_response_started_into(output);
                let index = chunk.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let delta = chunk.get("delta").unwrap_or(&Value::Null);
                if let Some(state) = self.active_blocks.get_mut(&index) {
                    match delta.get("type").and_then(Value::as_str).unwrap_or("") {
                        "thinking_delta" => {
                            let text = delta.get("thinking").and_then(Value::as_str).unwrap_or("");
                            if !text.is_empty() {
                                self.push_reasoning_delta_into(text, output);
                            }
                        }
                        "text_delta" => {
                            let text = delta.get("text").and_then(Value::as_str).unwrap_or("");
                            if !text.is_empty() {
                                self.finalize_reasoning_into(output);
                                self.push_text_delta_into(text, output);
                            }
                        }
                        "input_json_delta" => {
                            let partial = delta
                                .get("partial_json")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            if !partial.is_empty() {
                                state.json_input.push_str(partial);
                                push_sse(
                                    output,
                                    "response.function_call_arguments.delta",
                                    json!({
                                        "type": "response.function_call_arguments.delta",
                                        "item_id": state.item_id,
                                        "output_index": state.output_index.unwrap_or(0),
                                        "delta": partial
                                    }),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
            "content_block_stop" => {
                let index = chunk.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(state) = self.active_blocks.get_mut(&index) {
                    match state.block_type.as_str() {
                        "thinking" => self.finalize_reasoning_into(output),
                        "text" => {}
                        "tool_use" => {
                            let output_index = state.output_index.unwrap_or(0);
                            let call_id = if state.id.is_empty() {
                                format!("call_{index}")
                            } else {
                                state.id.clone()
                            };
                            let item = json!({
                                "id": state.item_id,
                                "type": "function_call",
                                "status": "completed",
                                "call_id": call_id,
                                "name": state.name,
                                "arguments": state.json_input
                            });
                            self.output_items.push((output_index, item.clone()));
                            state.done = true;
                            push_sse(
                                output,
                                "response.output_item.done",
                                json!({
                                    "type": "response.output_item.done",
                                    "output_index": output_index,
                                    "item": item
                                }),
                            );
                        }
                        _ => {}
                    }
                }
            }
            "message_delta" => {
                if let Some(delta) = chunk.get("delta") {
                    self.stop_reason = delta
                        .get("stop_reason")
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                }
                if let Some(usage) = chunk.get("usage") {
                    self.latest_usage = Some(anthropic_usage_to_responses_usage(Some(usage)));
                }
            }
            "message_stop" => self.finalize_into(output),
            _ => {}
        }
    }

    fn ensure_response_started_into(&mut self, output: &mut String) {
        if self.response_started {
            return;
        }
        self.response_started = true;
        push_sse(
            output,
            "response.created",
            json!({
                "type": "response.created",
                "response": self.base_response("in_progress", Vec::new())
            }),
        );
        push_sse(
            output,
            "response.in_progress",
            json!({
                "type": "response.in_progress",
                "response": self.base_response("in_progress", Vec::new())
            }),
        );
    }

    fn push_reasoning_delta_into(&mut self, delta: &str, output: &mut String) {
        if !self.reasoning.added {
            let output_index = self.next_output_index();
            let item_id = format!("rs_{}", self.response_id);
            self.reasoning.output_index = Some(output_index);
            self.reasoning.item_id = item_id.clone();
            self.reasoning.added = true;
            push_sse(
                output,
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "output_index": output_index,
                    "item": {
                        "id": item_id,
                        "type": "reasoning",
                        "status": "in_progress",
                        "summary": []
                    }
                }),
            );
        }
        self.reasoning.text.push_str(delta);
        push_sse(
            output,
            "response.reasoning_summary_text.delta",
            json!({
                "type": "response.reasoning_summary_text.delta",
                "item_id": self.reasoning.item_id,
                "output_index": self.reasoning.output_index.unwrap_or(0),
                "summary_index": 0,
                "delta": delta
            }),
        );
    }

    fn push_text_delta_into(&mut self, delta: &str, output: &mut String) {
        if !self.text.added {
            let output_index = self.next_output_index();
            let item_id = format!("{}_msg", self.response_id);
            self.text.output_index = Some(output_index);
            self.text.item_id = item_id.clone();
            self.text.added = true;
            push_sse(
                output,
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "output_index": output_index,
                    "item": {
                        "id": item_id,
                        "type": "message",
                        "status": "in_progress",
                        "role": "assistant",
                        "content": []
                    }
                }),
            );
        }
        self.text.text.push_str(delta);
        push_sse(
            output,
            "response.output_text.delta",
            json!({
                "type": "response.output_text.delta",
                "item_id": self.text.item_id,
                "output_index": self.text.output_index.unwrap_or(0),
                "content_index": 0,
                "delta": delta
            }),
        );
    }

    fn finalize_reasoning_into(&mut self, output: &mut String) {
        if !self.reasoning.added || self.reasoning.done {
            return;
        }
        let output_index = self.reasoning.output_index.unwrap_or(0);
        let item = json!({
            "id": self.reasoning.item_id,
            "type": "reasoning",
            "summary": [{ "type": "summary_text", "text": self.reasoning.text }]
        });
        self.output_items.push((output_index, item.clone()));
        self.reasoning.done = true;
        push_sse(
            output,
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "output_index": output_index,
                "item": item
            }),
        );
    }

    fn finalize_text_into(&mut self, output: &mut String) {
        if !self.text.added || self.text.done {
            return;
        }
        let output_index = self.text.output_index.unwrap_or(0);
        let item = json!({
            "id": self.text.item_id,
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": self.text.text, "annotations": [] }]
        });
        self.output_items.push((output_index, item.clone()));
        self.text.done = true;
        push_sse(
            output,
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "output_index": output_index,
                "item": item
            }),
        );
    }

    pub(crate) fn finalize_into(&mut self, output: &mut String) {
        if self.completed {
            return;
        }
        self.ensure_response_started_into(output);
        self.finalize_reasoning_into(output);
        self.finalize_text_into(output);
        push_sse(
            output,
            "response.completed",
            json!({
                "type": "response.completed",
                "response": self.base_response(
                    anthropic_stop_reason_to_response_status(self.stop_reason.as_deref()),
                    self.completed_output_items()
                )
            }),
        );
        output.push_str("data: [DONE]\n\n");
        self.completed = true;
    }

    pub(crate) fn failed_into(&mut self, output: &mut String, message: String) {
        self.completed = true;
        push_sse(
            output,
            "response.failed",
            json!({
                "type": "response.failed",
                "response": {
                    "id": self.response_id,
                    "object": "response",
                    "created_at": self.created_at,
                    "status": "failed",
                    "model": self.model,
                    "output": self.completed_output_items(),
                    "usage": self.latest_usage.clone().unwrap_or_else(default_responses_usage),
                    "error": { "message": message }
                }
            }),
        );
    }

    fn completed_output_items(&self) -> Vec<Value> {
        let mut output_items = self.output_items.clone();
        output_items.sort_by_key(|(output_index, _)| *output_index);
        output_items.into_iter().map(|(_, item)| item).collect()
    }

    fn base_response(&self, status: &str, output: Vec<Value>) -> Value {
        json!({
            "id": self.response_id,
            "object": "response",
            "created_at": self.created_at,
            "status": status,
            "model": self.model,
            "output": output,
            "usage": self.latest_usage.clone().unwrap_or_else(default_responses_usage)
        })
    }

    fn next_output_index(&mut self) -> u32 {
        let index = self.next_output_index;
        self.next_output_index += 1;
        index
    }
}
