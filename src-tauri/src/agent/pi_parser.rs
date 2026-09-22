use crate::agent::pipe_parser::PipeStdoutParser;
use crate::models::{BusEvent, ModelUsageEntry};
use serde_json::Value;
use std::collections::HashMap;

use crate::work::models::WorkTaskState;

/// Stateful parser for Pi's JSON message payloads.
///
/// Pi emits one JSON object per line. The parser normalizes the useful subset
/// into AgentCabin's agent-neutral BusEvent model while preserving unknown
/// payloads as Raw events for forward compatibility.
pub struct PiStdoutParser {
    message_counter: u64,
    partial_tool_output: HashMap<String, String>,
    pending_error: Option<String>,
}

impl PiStdoutParser {
    pub fn new() -> Self {
        Self {
            message_counter: 0,
            partial_tool_output: HashMap::new(),
            pending_error: None,
        }
    }

    pub(crate) fn has_pending_error(&self) -> bool {
        self.pending_error.is_some()
    }

    fn next_message_id(&mut self, message: &Value) -> String {
        if let Some(id) = message
            .get("responseId")
            .or_else(|| message.get("id"))
            .and_then(Value::as_str)
        {
            return id.to_string();
        }
        self.message_counter += 1;
        format!("pi-message-{}", self.message_counter)
    }

    fn message_complete(&mut self, run_id: &str, message: &Value) -> Vec<BusEvent> {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            return vec![];
        }

        let text = {
            let extracted = extract_text_content(message.get("content"));
            if extracted.is_empty() {
                message
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            } else {
                extracted
            }
        };
        let model = message
            .get("responseModel")
            .or_else(|| message.get("model"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let stop_reason = message
            .get("stopReason")
            .and_then(Value::as_str)
            .map(str::to_string);
        let usage = message.get("usage").cloned();
        let context_window = usage
            .as_ref()
            .and_then(|value| {
                value
                    .get("contextWindow")
                    .or_else(|| value.get("context_window"))
                    .and_then(Value::as_u64)
            })
            .or_else(|| {
                message
                    .get("contextWindow")
                    .or_else(|| message.get("context_window"))
                    .and_then(Value::as_u64)
            });

        let mut events = Vec::new();
        if !text.is_empty() {
            events.push(BusEvent::MessageComplete {
                run_id: run_id.to_string(),
                message_id: self.next_message_id(message),
                text,
                parent_tool_use_id: None,
                model: model.clone(),
                stop_reason,
                message_usage: usage.clone(),
            });
        }

        if let Some(usage) = usage.as_ref() {
            let model_usage = context_window.map(|context_window| {
                let model_name = model.clone().unwrap_or_else(|| "pi".to_string());
                HashMap::from([(
                    model_name,
                    ModelUsageEntry {
                        input_tokens: u64_field(usage, "input"),
                        output_tokens: u64_field(usage, "output"),
                        cache_read_tokens: u64_field(usage, "cacheRead"),
                        cache_write_tokens: u64_field(usage, "cacheWrite"),
                        web_search_requests: 0,
                        cost_usd: usage
                            .get("cost")
                            .and_then(|value| value.get("total"))
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                        context_window: Some(context_window),
                        max_output_tokens: None,
                    },
                )])
            });
            events.push(BusEvent::UsageUpdate {
                run_id: run_id.to_string(),
                input_tokens: u64_field(usage, "input"),
                output_tokens: u64_field(usage, "output"),
                cache_read_tokens: Some(u64_field(usage, "cacheRead")),
                cache_write_tokens: Some(u64_field(usage, "cacheWrite")),
                total_cost_usd: usage
                    .get("cost")
                    .and_then(|v| v.get("total"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                turn_index: None,
                context_tokens: None,
                context_window: None,
                model_usage,
                duration_api_ms: None,
                duration_ms: None,
                num_turns: None,
                stop_reason: message
                    .get("stopReason")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                service_tier: None,
                speed: None,
                web_fetch_requests: None,
                cache_creation_5m: None,
                cache_creation_1h: None,
            });
        }

        if let Some(error) = message
            .get("errorMessage")
            .and_then(Value::as_str)
            .filter(|error| !error.is_empty())
        {
            self.pending_error = Some(error.to_string());
        } else {
            self.pending_error = None;
        }

        events
    }

    fn tool_update(&mut self, run_id: &str, raw: &Value) -> Vec<BusEvent> {
        let tool_use_id = raw
            .get("toolCallId")
            .and_then(Value::as_str)
            .unwrap_or("pi-tool-unknown")
            .to_string();
        let accumulated = extract_result_text(raw.get("partialResult"));
        if accumulated.is_empty() {
            return vec![];
        }

        let previous = self
            .partial_tool_output
            .insert(tool_use_id.clone(), accumulated.clone())
            .unwrap_or_default();
        let delta = accumulated
            .strip_prefix(&previous)
            .unwrap_or(&accumulated)
            .to_string();
        if delta.is_empty() {
            return vec![];
        }

        vec![BusEvent::ToolOutputDelta {
            run_id: run_id.to_string(),
            tool_use_id,
            delta,
            parent_tool_use_id: None,
        }]
    }

    fn work_task_state_from_result(result: &Value) -> Option<WorkTaskState> {
        result
            .get("details")
            .and_then(|details| details.get("work_task_state"))
            .cloned()
            .and_then(|state| serde_json::from_value(state).ok())
    }
}

impl Default for PiStdoutParser {
    fn default() -> Self {
        Self::new()
    }
}

impl PipeStdoutParser for PiStdoutParser {
    fn parse_line(&mut self, run_id: &str, raw: &Value) -> Vec<BusEvent> {
        let event_type = raw.get("type").and_then(Value::as_str).unwrap_or("");
        match event_type {
            "session" => vec![BusEvent::SessionInit {
                run_id: run_id.to_string(),
                session_id: raw.get("id").and_then(Value::as_str).map(str::to_string),
                model: None,
                model_options: vec![],
                tools: vec![],
                cwd: raw
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                slash_commands: vec![],
                commands_loaded: true,
                mcp_servers: vec![],
                permission_mode: None,
                api_key_source: None,
                claude_code_version: None,
                output_style: None,
                agents: vec![],
                skills: vec![],
                plugins: vec![],
                plugin_errors: vec![],
                fast_mode_state: None,
                capabilities: None,
            }],
            "message_update" => {
                let update = raw.get("assistantMessageEvent").unwrap_or(&Value::Null);
                match update.get("type").and_then(Value::as_str).unwrap_or("") {
                    "text_delta" => {
                        self.pending_error = None;
                        update
                            .get("delta")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(|text| {
                                vec![BusEvent::MessageDelta {
                                    run_id: run_id.to_string(),
                                    text: text.to_string(),
                                    parent_tool_use_id: None,
                                }]
                            })
                            .unwrap_or_default()
                    }
                    "thinking_delta" => {
                        self.pending_error = None;
                        update
                            .get("delta")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(|text| {
                                vec![BusEvent::ThinkingDelta {
                                    run_id: run_id.to_string(),
                                    text: text.to_string(),
                                    parent_tool_use_id: None,
                                }]
                            })
                            .unwrap_or_default()
                    }
                    "error" => {
                        let message = update
                            .get("error")
                            .and_then(|v| v.get("errorMessage"))
                            .or_else(|| update.get("errorMessage"))
                            .and_then(Value::as_str)
                            .unwrap_or("Pi message stream failed");
                        if is_expected_abort_error(message) {
                            // Pi reports a user initiated abort as a message error. It
                            // is a normal stop boundary, not an actionable failure.
                            self.pending_error = None;
                        } else {
                            self.pending_error = Some(message.to_string());
                        }
                        vec![]
                    }
                    _ => vec![],
                }
            }
            "message_end" => raw
                .get("message")
                .map(|message| self.message_complete(run_id, message))
                .unwrap_or_default(),
            "tool_execution_start" => {
                self.pending_error = None;
                let tool_use_id = raw
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or("pi-tool-unknown")
                    .to_string();
                self.partial_tool_output.remove(&tool_use_id);
                vec![BusEvent::ToolStart {
                    run_id: run_id.to_string(),
                    tool_use_id,
                    tool_name: normalize_tool_name(
                        raw.get("toolName")
                            .and_then(Value::as_str)
                            .unwrap_or("Tool"),
                    ),
                    input: raw
                        .get("args")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({})),
                    parent_tool_use_id: None,
                }]
            }
            "tool_execution_update" => self.tool_update(run_id, raw),
            "tool_execution_end" => {
                let tool_use_id = raw
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or("pi-tool-unknown")
                    .to_string();
                self.partial_tool_output.remove(&tool_use_id);
                let is_error = raw.get("isError").and_then(Value::as_bool).unwrap_or(false);
                let result = raw.get("result").cloned().unwrap_or(Value::Null);
                let work_task_state = (!is_error)
                    .then(|| Self::work_task_state_from_result(&result))
                    .flatten();
                let mut events = vec![BusEvent::ToolEnd {
                    run_id: run_id.to_string(),
                    tool_use_id,
                    tool_name: normalize_tool_name(
                        raw.get("toolName")
                            .and_then(Value::as_str)
                            .unwrap_or("Tool"),
                    ),
                    output: result.clone(),
                    status: if is_error { "failed" } else { "completed" }.to_string(),
                    duration_ms: None,
                    parent_tool_use_id: None,
                    tool_use_result: Some(result),
                }];
                if let Some(state) = work_task_state {
                    events.extend(crate::work::task_state::bus_events(run_id, &state));
                }
                events
            }
            "compaction_start" => vec![BusEvent::SystemStatus {
                run_id: run_id.to_string(),
                status: Some("compacting".to_string()),
                data: raw.clone(),
            }],
            "compaction_end" => {
                let mut events = vec![BusEvent::SystemStatus {
                    run_id: run_id.to_string(),
                    status: None,
                    data: raw.clone(),
                }];
                if !raw.get("aborted").and_then(Value::as_bool).unwrap_or(false) {
                    events.push(BusEvent::CompactBoundary {
                        run_id: run_id.to_string(),
                        trigger: raw
                            .get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or("pi")
                            .to_string(),
                        pre_tokens: raw
                            .get("result")
                            .and_then(|v| v.get("tokensBefore"))
                            .and_then(Value::as_u64),
                    });
                }
                events
            }
            "auto_retry_start" => vec![BusEvent::SystemStatus {
                run_id: run_id.to_string(),
                status: Some("retrying".to_string()),
                data: raw.clone(),
            }],
            "auto_retry_end" => {
                let mut events = vec![BusEvent::SystemStatus {
                    run_id: run_id.to_string(),
                    status: None,
                    data: raw.clone(),
                }];
                if raw.get("success").and_then(Value::as_bool) == Some(true) {
                    self.pending_error = None;
                } else if let Some(error) = raw
                    .get("finalError")
                    .and_then(Value::as_str)
                    .filter(|error| !error.is_empty())
                    .map(str::to_string)
                    .or_else(|| self.pending_error.take())
                {
                    events.push(BusEvent::CommandOutput {
                        run_id: run_id.to_string(),
                        content: format!("[pi error] {}", error),
                    });
                    self.pending_error = None;
                }
                events
            }
            // Current Pi emits agent_end before auto_retry_start for every failed
            // attempt. Keep the error pending until auto_retry_end tells us whether
            // the retry recovered or was exhausted. Older Pi versions used
            // agent_settled as the final boundary, so retain its fallback behavior.
            "agent_end" => vec![],
            "agent_settled" => self
                .pending_error
                .take()
                .map(|error| {
                    vec![BusEvent::CommandOutput {
                        run_id: run_id.to_string(),
                        content: format!("[pi error] {}", error),
                    }]
                })
                .unwrap_or_default(),
            "extension_error" => vec![BusEvent::CommandOutput {
                run_id: run_id.to_string(),
                content: format!(
                    "[pi extension error] {}",
                    raw.get("error")
                        .or_else(|| raw.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or("unknown extension error")
                ),
            }],
            // Lifecycle events are represented by process/run state elsewhere.
            "agent_start" | "turn_start" | "turn_end" | "message_start" => {
                if event_type == "turn_start" || event_type == "message_start" {
                    self.pending_error = None;
                }
                vec![]
            }
            // Preserve newer Pi events so they are inspectable before a dedicated mapping exists.
            _ if !event_type.is_empty() => vec![BusEvent::Raw {
                run_id: run_id.to_string(),
                source: "pi".to_string(),
                data: raw.clone(),
            }],
            _ => vec![],
        }
    }
}

fn normalize_tool_name(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "read" => "Read".to_string(),
        "bash" => "Bash".to_string(),
        "edit" => "Edit".to_string(),
        "write" => "Write".to_string(),
        "grep" => "Grep".to_string(),
        "find" => "Glob".to_string(),
        "ls" => "Glob".to_string(),
        _ => name.to_string(),
    }
}

fn is_expected_abort_error(message: &str) -> bool {
    let message = message.trim().to_ascii_lowercase();
    message.contains("request aborted") || message.contains("request was aborted")
}

fn extract_text_content(content: Option<&Value>) -> String {
    let Some(content) = content else {
        return String::new();
    };
    if let Some(text) = content.as_str() {
        return text.to_string();
    }
    if let Some(items) = content.as_array() {
        return items
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(Value::as_str) == Some("text") {
                    item.get("text").and_then(Value::as_str)
                } else if let Some(text) = item.get("text").and_then(Value::as_str) {
                    Some(text)
                } else {
                    item.as_str()
                }
            })
            .collect::<Vec<_>>()
            .join("");
    }
    String::new()
}

fn extract_result_text(result: Option<&Value>) -> String {
    let Some(result) = result else {
        return String::new();
    };
    if let Some(text) = result.as_str() {
        return text.to_string();
    }
    if let Some(content) = result.get("content").and_then(Value::as_array) {
        return content
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| item.as_str())
            })
            .collect::<Vec<_>>()
            .join("");
    }
    result
        .get("output")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn u64_field(value: &Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|n| n.max(0.0) as u64)))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_and_thinking_deltas() {
        let mut parser = PiStdoutParser::new();
        let text = serde_json::json!({
            "type": "message_update",
            "assistantMessageEvent": {"type": "text_delta", "delta": "Hello"}
        });
        assert!(matches!(
            parser.parse_line("run", &text).as_slice(),
            [BusEvent::MessageDelta { text, .. }] if text == "Hello"
        ));

        let thinking = serde_json::json!({
            "type": "message_update",
            "assistantMessageEvent": {"type": "thinking_delta", "delta": "Reasoning"}
        });
        assert!(matches!(
            parser.parse_line("run", &thinking).as_slice(),
            [BusEvent::ThinkingDelta { text, .. }] if text == "Reasoning"
        ));
    }

    #[test]
    fn parses_assistant_message_and_usage() {
        let mut parser = PiStdoutParser::new();
        let event = serde_json::json!({
            "type": "message_end",
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Done"}],
                "model": "gpt-5.6",
                "stopReason": "stop",
                "usage": {
                    "input": 10,
                    "output": 4,
                    "cacheRead": 2,
                    "cacheWrite": 1,
                    "cost": {"total": 0.01}
                }
            }
        });
        let events = parser.parse_line("run", &event);
        assert!(matches!(
            &events[0],
            BusEvent::MessageComplete { text, model, .. }
                if text == "Done" && model.as_deref() == Some("gpt-5.6")
        ));
        assert!(matches!(
            &events[1],
            BusEvent::UsageUpdate {
                input_tokens: 10,
                output_tokens: 4,
                ..
            }
        ));
    }

    #[test]
    fn parses_context_window_into_model_usage() {
        let mut parser = PiStdoutParser::new();
        let event = serde_json::json!({
            "type": "message_end",
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Done"}],
                "model": "glm-5.2",
                "usage": {
                    "input": 100,
                    "output": 20,
                    "contextWindow": 1048576,
                    "cost": {"total": 0.01}
                }
            }
        });

        let events = parser.parse_line("run", &event);
        assert!(matches!(
            &events[1],
            BusEvent::UsageUpdate { model_usage: Some(model_usage), .. }
                if model_usage.get("glm-5.2").and_then(|entry| entry.context_window) == Some(1048576)
        ));
    }

    #[test]
    fn tool_updates_emit_only_new_suffix() {
        let mut parser = PiStdoutParser::new();
        let first = serde_json::json!({
            "type": "tool_execution_update",
            "toolCallId": "call-1",
            "partialResult": {"content": [{"type": "text", "text": "abc"}]}
        });
        let second = serde_json::json!({
            "type": "tool_execution_update",
            "toolCallId": "call-1",
            "partialResult": {"content": [{"type": "text", "text": "abcdef"}]}
        });
        assert!(matches!(
            parser.parse_line("run", &first).as_slice(),
            [BusEvent::ToolOutputDelta { delta, .. }] if delta == "abc"
        ));
        assert!(matches!(
            parser.parse_line("run", &second).as_slice(),
            [BusEvent::ToolOutputDelta { delta, .. }] if delta == "def"
        ));
    }

    #[test]
    fn work_harness_tool_results_emit_authoritative_and_structured_task_state() {
        let mut parser = PiStdoutParser::new();
        let event = serde_json::json!({
            "type": "tool_execution_end",
            "toolCallId": "work-plan-1",
            "toolName": "work_replace_plan",
            "isError": false,
            "result": {
                "content": [{"type": "text", "text": "updated"}],
                "details": {
                    "ok": true,
                    "work_task_state": {
                        "version": 1,
                        "revision": 2,
                        "goal": "Ship the report",
                        "plan": [
                            {"id": "research", "text": "Research", "status": "in_progress"}
                        ],
                        "checkpoint": null,
                        "updatedAt": "2026-08-13T00:00:00Z"
                    }
                }
            }
        });

        let events = parser.parse_line("run", &event);
        assert!(
            matches!(&events[0], BusEvent::ToolEnd { tool_name, .. } if tool_name == "work_replace_plan")
        );
        assert!(matches!(
            &events[1],
            BusEvent::WorkTaskState { state, .. }
                if state.goal.as_deref() == Some("Ship the report") && state.revision == 2
        ));
        assert!(matches!(
            &events[2],
            BusEvent::StructuredTaskState { tasks, .. }
                if tasks.len() == 1 && tasks[0].id == "research"
        ));
    }

    #[test]
    fn collapses_retry_errors_until_agent_settles() {
        let mut parser = PiStdoutParser::new();
        for request_id in ["req-1", "req-2", "req-3"] {
            let event = serde_json::json!({
                "type": "message_end",
                "message": {
                    "role": "assistant",
                    "content": [],
                    "errorMessage": format!("429 rate_limit_error request_id={request_id}")
                }
            });
            let events = parser.parse_line("run", &event);
            assert!(events
                .iter()
                .all(|event| !matches!(event, BusEvent::CommandOutput { .. })));
        }

        let settled = parser.parse_line("run", &serde_json::json!({ "type": "agent_settled" }));
        assert!(matches!(
            settled.as_slice(),
            [BusEvent::CommandOutput { content, .. }] if content.contains("429 rate_limit_error")
        ));
    }

    #[test]
    fn recovers_from_transient_upstream_error_when_retry_succeeds() {
        let mut parser = PiStdoutParser::new();
        // 1. Thinking delta comes in
        let thinking = serde_json::json!({
            "type": "message_update",
            "assistantMessageEvent": {"type": "thinking_delta", "delta": "Thinking..."}
        });
        parser.parse_line("run", &thinking);

        // 2. Upstream 500 error occurs
        let error_event = serde_json::json!({
            "type": "message_update",
            "assistantMessageEvent": {
                "type": "error",
                "errorMessage": "500: {\"message\":\"upstream error: do request failed\",\"code\":\"do_request_failed\"}"
            }
        });
        assert!(parser.parse_line("run", &error_event).is_empty());

        // Current Pi closes each failed attempt before announcing the retry.
        // That boundary must not leak a transient error into the conversation.
        assert!(parser
            .parse_line("run", &serde_json::json!({ "type": "agent_end" }))
            .is_empty());

        // 3. Auto retry starts and succeeds, emitting text deltas
        let retry_start = serde_json::json!({ "type": "auto_retry_start" });
        parser.parse_line("run", &retry_start);

        let text_event = serde_json::json!({
            "type": "message_update",
            "assistantMessageEvent": {"type": "text_delta", "delta": "Hello world"}
        });
        let text_deltas = parser.parse_line("run", &text_event);
        assert_eq!(text_deltas.len(), 1);

        // 4. Successful message_end
        let msg_end = serde_json::json!({
            "type": "message_end",
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Hello world"}],
                "stopReason": "stop"
            }
        });
        parser.parse_line("run", &msg_end);

        // 5. Agent finishes turn - NO error should be emitted
        let retry_end = parser.parse_line(
            "run",
            &serde_json::json!({ "type": "auto_retry_end", "attempt": 1, "success": true }),
        );
        assert!(
            retry_end
                .iter()
                .all(|event| !matches!(event, BusEvent::CommandOutput { .. })),
            "Transient error should not be emitted after successful turn"
        );
        assert!(parser
            .parse_line("run", &serde_json::json!({ "type": "agent_end" }))
            .is_empty());
    }

    #[test]
    fn surfaces_only_the_final_error_when_automatic_retries_are_exhausted() {
        let mut parser = PiStdoutParser::new();

        for attempt in 1..=3 {
            let message_end = serde_json::json!({
                "type": "message_end",
                "message": {
                    "role": "assistant",
                    "content": [],
                    "errorMessage": format!("transient error {attempt}")
                }
            });
            parser.parse_line("run", &message_end);
            assert!(parser
                .parse_line("run", &serde_json::json!({ "type": "agent_end" }))
                .is_empty());
            parser.parse_line(
                "run",
                &serde_json::json!({
                    "type": "auto_retry_start",
                    "attempt": attempt,
                    "maxAttempts": 3
                }),
            );
        }

        parser.parse_line(
            "run",
            &serde_json::json!({
                "type": "message_end",
                "message": {
                    "role": "assistant",
                    "content": [],
                    "errorMessage": "final retry error"
                }
            }),
        );
        assert!(parser
            .parse_line("run", &serde_json::json!({ "type": "agent_end" }))
            .is_empty());

        let exhausted = parser.parse_line(
            "run",
            &serde_json::json!({
                "type": "auto_retry_end",
                "attempt": 3,
                "success": false,
                "finalError": "final retry error"
            }),
        );
        assert_eq!(
            exhausted
                .iter()
                .filter(|event| matches!(event, BusEvent::CommandOutput { .. }))
                .count(),
            1
        );
        assert!(exhausted.iter().any(
            |event| matches!(event, BusEvent::CommandOutput { content, .. } if content == "[pi error] final retry error")
        ));
    }

    #[test]
    fn does_not_surface_user_abort_as_an_error() {
        for message in ["Request aborted", "Request was aborted"] {
            let mut parser = PiStdoutParser::new();
            let error = serde_json::json!({
                "type": "message_update",
                "assistantMessageEvent": {
                    "type": "error",
                    "errorMessage": message
                }
            });

            assert!(parser.parse_line("run", &error).is_empty());
            assert!(parser
                .parse_line("run", &serde_json::json!({ "type": "agent_end" }))
                .is_empty());
        }
    }

    #[test]
    fn parses_session_header() {
        let mut parser = PiStdoutParser::new();
        let event = serde_json::json!({
            "type": "session",
            "id": "session-123",
            "cwd": "/tmp/project"
        });
        assert!(matches!(
            parser.parse_line("run", &event).as_slice(),
            [BusEvent::SessionInit { session_id, cwd, .. }]
                if session_id.as_deref() == Some("session-123") && cwd == "/tmp/project"
        ));
    }

    #[test]
    fn parses_assistant_message_with_string_content() {
        let mut parser = PiStdoutParser::new();
        let event = serde_json::json!({
            "type": "message_end",
            "message": {
                "role": "assistant",
                "content": "Deepseek text output directly as string",
                "model": "deepseek-v4-flash",
                "stopReason": "end_turn"
            }
        });
        let events = parser.parse_line("run-1", &event);
        assert!(matches!(
            &events[0],
            BusEvent::MessageComplete { text, model, .. }
                if text == "Deepseek text output directly as string"
                    && model.as_deref() == Some("deepseek-v4-flash")
        ));
    }

    #[test]
    fn parses_assistant_message_with_direct_text_fallback() {
        let mut parser = PiStdoutParser::new();
        let event = serde_json::json!({
            "type": "message_end",
            "message": {
                "role": "assistant",
                "content": [],
                "text": "Fallback text from message.text",
                "model": "deepseek-v4-flash",
                "stopReason": "end_turn"
            }
        });
        let events = parser.parse_line("run-1", &event);
        assert!(matches!(
            &events[0],
            BusEvent::MessageComplete { text, .. }
                if text == "Fallback text from message.text"
        ));
    }
}
