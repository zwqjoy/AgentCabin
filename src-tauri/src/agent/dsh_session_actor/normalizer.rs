//! Normalizes DSH JSON-RPC events into standard AgentCabin BusEvents.

use crate::models::{BusEvent, RunStatus, StructuredTask, StructuredTaskStatus};
use crate::web_server::broadcaster::BroadcastEmitter;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct DshEventNormalizer {
    emitter: Arc<BroadcastEmitter>,
    run_id: String,
    cwd: PathBuf,
    current_turn_index: AtomicUsize,
}

impl DshEventNormalizer {
    pub fn new(emitter: Arc<BroadcastEmitter>, run_id: String, cwd: impl Into<PathBuf>) -> Self {
        Self {
            emitter,
            run_id,
            cwd: cwd.into(),
            current_turn_index: AtomicUsize::new(0),
        }
    }

    /// Mark the next user prompt as a new Work turn. DSH notifications do not
    /// carry AgentCabin's backend turn index, so the normalizer attaches the
    /// index at the point where the prompt is admitted.
    pub fn begin_turn(&self) {
        self.current_turn_index.fetch_add(1, Ordering::Relaxed);
    }

    pub fn emit_run_state(&self, state: &str, error: Option<String>) {
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::RunState {
                run_id: self.run_id.clone(),
                state: state.to_string(),
                exit_code: None,
                error,
            },
        );
    }

    pub(crate) fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn fail_run(&self, error: String) {
        let _ = crate::storage::runs::update_status(
            &self.run_id,
            RunStatus::Failed,
            None,
            Some(error.clone()),
        );
        self.emit_run_state("failed", Some(error));
    }

    pub fn emit_user_message(&self, text: &str) {
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::UserMessage {
                run_id: self.run_id.clone(),
                text: text.to_string(),
                uuid: None,
                client_uuid: None,
                attachments: vec![],
            },
        );
    }

    pub fn emit_assistant_delta(&self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::MessageDelta {
                run_id: self.run_id.clone(),
                text: delta.to_string(),
                parent_tool_use_id: None,
            },
        );
    }

    pub fn emit_reasoning_delta(&self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::ThinkingDelta {
                run_id: self.run_id.clone(),
                text: delta.to_string(),
                parent_tool_use_id: None,
            },
        );
    }

    pub fn emit_assistant_complete(&self, message_id: &str, text: &str, model: Option<String>) {
        let text = self.attach_html_preview_if_file_reference(text);
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::MessageComplete {
                run_id: self.run_id.clone(),
                message_id: message_id.to_string(),
                text,
                parent_tool_use_id: None,
                model,
                stop_reason: None,
                message_usage: None,
            },
        );
    }

    /// DSH may complete a visualization as a file-oriented task. Recover that
    /// result into the same explicit conversation preview contract used by Pi.
    /// Only files below the run cwd are eligible for this fallback read.
    fn attach_html_preview_if_file_reference(&self, text: &str) -> String {
        if text.contains("```html-preview") || text.contains("```html") {
            return text.to_string();
        }
        let Some(path) = text
            .split_whitespace()
            .map(|token| token.trim_matches(|ch: char| "`'\"()[]{}<>.,;".contains(ch)))
            .filter(|token| {
                let lower = token.to_ascii_lowercase();
                lower.ends_with(".html") || lower.ends_with(".htm")
            })
            .find_map(|token| self.resolve_html_reference(token))
        else {
            return text.to_string();
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            return text.to_string();
        };
        if content.is_empty() || content.len() > 5 * 1024 * 1024 {
            return text.to_string();
        }
        format!("{text}\n\n```html-preview\n{content}\n```")
    }

    fn resolve_html_reference(&self, reference: &str) -> Option<PathBuf> {
        let candidate = Path::new(reference);
        let path = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.cwd.join(candidate)
        };
        let canonical = std::fs::canonicalize(path).ok()?;
        let cwd = std::fs::canonicalize(&self.cwd).ok()?;
        if !canonical.starts_with(cwd) {
            return None;
        }
        Some(canonical)
    }

    pub fn emit_usage_update(
        &self,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: Option<u64>,
        cache_write_tokens: Option<u64>,
    ) {
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::UsageUpdate {
                run_id: self.run_id.clone(),
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
                total_cost_usd: 0.0,
                turn_index: Some(self.current_turn_index.load(Ordering::Relaxed) as u32),
                context_tokens: None,
                context_window: None,
                model_usage: None,
                duration_api_ms: None,
                duration_ms: None,
                num_turns: None,
                stop_reason: None,
                service_tier: None,
                speed: None,
                web_fetch_requests: None,
                cache_creation_5m: None,
                cache_creation_1h: None,
            },
        );
    }

    pub fn emit_tool_start(&self, call_id: &str, tool_name: &str, input: Value) {
        let tool_name = canonical_tool_name(tool_name);
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::ToolStart {
                run_id: self.run_id.clone(),
                tool_use_id: call_id.to_string(),
                tool_name,
                input,
                parent_tool_use_id: None,
            },
        );
    }

    pub fn emit_tool_end(&self, call_id: &str, tool_name: &str, output: Value, is_error: bool) {
        let tool_name = canonical_tool_name(tool_name);
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::ToolEnd {
                run_id: self.run_id.clone(),
                tool_use_id: call_id.to_string(),
                tool_name,
                output,
                status: if is_error {
                    "error".to_string()
                } else {
                    "success".to_string()
                },
                duration_ms: None,
                parent_tool_use_id: None,
                tool_use_result: None,
            },
        );
    }

    /// Convert a DSH-native user-question request into the agent-neutral
    /// AskUserQuestion timeline pair. The actor keeps the JSON-RPC request
    /// open until the frontend answers it through respond_user_input.
    pub fn emit_user_question(&self, request_id: &str, params: &Value) {
        let questions = params
            .get("questions")
            .cloned()
            .or_else(|| {
                params
                    .get("input")
                    .and_then(|input| input.get("questions"))
                    .cloned()
            })
            .unwrap_or_else(|| json!([]));
        let input = json!({ "questions": questions });
        self.emit_tool_start(request_id, "AskUserQuestion", input.clone());
        self.emit_tool_end(request_id, "AskUserQuestion", input, true);
    }

    pub fn handle_notification(&self, method: &str, params: Option<&Value>) {
        match method {
            "session.event" => {
                let Some(event) = params.and_then(|value| value.get("event")) else {
                    return;
                };
                self.handle_session_event(event);
            }
            "session/messageDelta" | "message/delta" => {
                if let Some(delta) = params.and_then(|p| p.get("delta")).and_then(Value::as_str) {
                    self.emit_assistant_delta(delta);
                } else if let Some(delta) =
                    params.and_then(|p| p.get("text")).and_then(Value::as_str)
                {
                    self.emit_assistant_delta(delta);
                }
            }
            "session/reasoningDelta" | "reasoning/delta" => {
                if let Some(delta) = params.and_then(|p| p.get("delta")).and_then(Value::as_str) {
                    self.emit_reasoning_delta(delta);
                } else if let Some(delta) =
                    params.and_then(|p| p.get("text")).and_then(Value::as_str)
                {
                    self.emit_reasoning_delta(delta);
                }
            }
            "session/update" => {
                self.handle_acp_update(params.and_then(|value| value.get("update")));
            }
            "session/toolUse" | "tool/call" => {
                if let Some(p) = params {
                    let call_id = p.get("id").and_then(Value::as_str).unwrap_or("call_dsh");
                    let name = p.get("name").and_then(Value::as_str).unwrap_or("unknown");
                    let input = p.get("input").cloned().unwrap_or(Value::Null);
                    if is_todo_tool(name) {
                        self.emit_todo_snapshot(&input);
                    }
                    self.emit_tool_start(call_id, name, input);
                }
            }
            "session/toolResult" | "tool/result" => {
                if let Some(p) = params {
                    let call_id = p.get("id").and_then(Value::as_str).unwrap_or("call_dsh");
                    let name = p.get("name").and_then(Value::as_str).unwrap_or("unknown");
                    let output = p.get("content").cloned().unwrap_or(Value::Null);
                    let is_error = p.get("isError").and_then(Value::as_bool).unwrap_or(false);
                    self.emit_tool_end(call_id, name, output, is_error);
                }
            }
            "session/status" => {
                if let Some(status) = params.and_then(|p| p.get("status")).and_then(Value::as_str) {
                    match status {
                        "idle" | "completed" => {
                            let _ = crate::storage::runs::update_status(
                                &self.run_id,
                                RunStatus::Idle,
                                None,
                                None,
                            );
                            self.emit_run_state("idle", None);
                        }
                        "running" => {
                            let _ = crate::storage::runs::update_status(
                                &self.run_id,
                                RunStatus::Running,
                                None,
                                None,
                            );
                            self.emit_run_state("running", None);
                        }
                        "failed" | "error" => {
                            let err_msg = params
                                .and_then(|p| p.get("error"))
                                .and_then(Value::as_str)
                                .map(ToString::to_string);
                            let _ = crate::storage::runs::update_status(
                                &self.run_id,
                                RunStatus::Failed,
                                None,
                                err_msg.clone(),
                            );
                            self.emit_run_state("failed", err_msg);
                        }
                        _ => {}
                    }
                }
            }
            "session/usage" | "session.usage" => {
                if let Some(p) = params {
                    let u = p.get("usage").unwrap_or(p);
                    let input = u
                        .get("inputTokens")
                        .or_else(|| u.get("input_tokens"))
                        .or_else(|| u.get("prompt_tokens"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let output = u
                        .get("outputTokens")
                        .or_else(|| u.get("output_tokens"))
                        .or_else(|| u.get("completion_tokens"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let cache_read = u
                        .get("cacheReadTokens")
                        .or_else(|| u.get("cache_read_tokens"))
                        .and_then(Value::as_u64);
                    let cache_write = u
                        .get("cacheWriteTokens")
                        .or_else(|| u.get("cache_write_tokens"))
                        .and_then(Value::as_u64);
                    if input > 0 || output > 0 {
                        self.emit_usage_update(input, output, cache_read, cache_write);
                    }
                }
            }
            _ => {
                log::debug!("[dsh_normalizer] unhandled notification {method}: {params:?}");
            }
        }
    }

    fn handle_session_event(&self, event: &Value) {
        let Some(event_type) = event.get("type").and_then(Value::as_str) else {
            return;
        };
        let data = event.get("data").unwrap_or(&Value::Null);
        match event_type {
            "turn/end" => {
                let reason = data
                    .get("reason")
                    .and_then(|value| value.get("kind"))
                    .and_then(Value::as_str)
                    .unwrap_or("completed");
                if reason == "error" {
                    let error = data
                        .pointer("/reason/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("DSH turn failed")
                        .to_string();
                    self.fail_run(error);
                } else {
                    // DSH keeps the SDK process alive for the next prompt, so the
                    // child stdout does not close at the end of a turn. `turn/end`
                    // is the durable completion boundary for the current AgentCabin
                    // run and must settle the UI back to idle.
                    let _ = crate::storage::runs::update_status(
                        &self.run_id,
                        RunStatus::Idle,
                        None,
                        None,
                    );
                    self.emit_run_state("idle", None);
                }
            }
            "assistant/message" => {
                let Some(message) = data.get("message") else {
                    return;
                };
                if let Some(stream) = data.get("stream").and_then(Value::as_array) {
                    for record in stream {
                        self.emit_stream_record(record);
                    }
                }
                let text = content_text(message.get("content"), "text");
                let message_id = message
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("dsh-assistant");
                let model = message
                    .pointer("/source/model")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                self.emit_assistant_complete(message_id, &text, model);

                // Extract token usage from assistant/message (data.usage or data.message.usage)
                let usage_val = data.get("usage").or_else(|| message.get("usage"));
                if let Some(u) = usage_val {
                    let input = u
                        .get("inputTokens")
                        .or_else(|| u.get("input_tokens"))
                        .or_else(|| u.get("prompt_tokens"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let output = u
                        .get("outputTokens")
                        .or_else(|| u.get("output_tokens"))
                        .or_else(|| u.get("completion_tokens"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    let cache_read = u
                        .get("cacheReadTokens")
                        .or_else(|| u.get("cache_read_tokens"))
                        .and_then(Value::as_u64);
                    let cache_write = u
                        .get("cacheWriteTokens")
                        .or_else(|| u.get("cache_write_tokens"))
                        .and_then(Value::as_u64);
                    if input > 0 || output > 0 {
                        self.emit_usage_update(input, output, cache_read, cache_write);
                    }
                }
            }
            "tool/call" => {
                let call_id = data
                    .get("callId")
                    .and_then(Value::as_str)
                    .unwrap_or("call_dsh");
                let name = data
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let input = data
                    .get("arguments")
                    .and_then(Value::as_str)
                    .and_then(|value| serde_json::from_str(value).ok())
                    .unwrap_or(Value::Null);
                if is_todo_tool(name) {
                    self.emit_todo_snapshot(&input);
                }
                self.emit_tool_start(call_id, name, input);
            }
            "tool/result" => {
                let message = data.get("message").unwrap_or(&Value::Null);
                let call_id = message
                    .pointer("/source/callId")
                    .and_then(Value::as_str)
                    .unwrap_or("call_dsh");
                let block = message
                    .get("content")
                    .and_then(Value::as_array)
                    .and_then(|blocks| blocks.first())
                    .cloned()
                    .unwrap_or(Value::Null);
                let is_error = block
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.emit_tool_end(call_id, "dsh", block, is_error);
            }
            _ => {}
        }
    }

    fn emit_stream_record(&self, record: &Value) {
        match record.get("type").and_then(Value::as_str) {
            Some("text-chunks") => {
                if let Some(texts) = record.get("texts").and_then(Value::as_array) {
                    for text in texts.iter().filter_map(Value::as_str) {
                        self.emit_assistant_delta(text);
                    }
                }
            }
            Some("reasoning-chunks") => {
                if let Some(texts) = record.get("texts").and_then(Value::as_array) {
                    for text in texts.iter().filter_map(Value::as_str) {
                        self.emit_reasoning_delta(text);
                    }
                }
            }
            Some("chunk") => {
                let chunk = record.get("chunk").unwrap_or(&Value::Null);
                let text = chunk.get("text").and_then(Value::as_str);
                match chunk.get("type").and_then(Value::as_str) {
                    Some("text-delta") => {
                        if let Some(text) = text {
                            self.emit_assistant_delta(text);
                        }
                    }
                    Some("reasoning-delta") => {
                        if let Some(text) = text {
                            self.emit_reasoning_delta(text);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn handle_acp_update(&self, update: Option<&Value>) {
        let Some(update) = update else {
            return;
        };
        match update.get("sessionUpdate").and_then(Value::as_str) {
            Some("plan") => {
                if let Some(tasks) = parse_acp_plan(update) {
                    self.emitter.persist_and_emit(
                        &self.run_id,
                        &BusEvent::StructuredTaskState {
                            run_id: self.run_id.clone(),
                            tasks,
                        },
                    );
                }
            }
            Some("agent_message_chunk") => {
                if let Some(text) = acp_text_block(update.get("content")) {
                    self.emit_assistant_delta(&text);
                }
            }
            Some("agent_thought_chunk") => {
                if let Some(text) = acp_text_block(update.get("content")) {
                    self.emit_reasoning_delta(&text);
                }
            }
            Some("usage_update") => {
                let Some(usage) = update.get("usage") else {
                    return;
                };
                let input = usage
                    .get("inputTokens")
                    .or_else(|| usage.get("input_tokens"))
                    .or_else(|| usage.get("prompt_tokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let output = usage
                    .get("outputTokens")
                    .or_else(|| usage.get("output_tokens"))
                    .or_else(|| usage.get("completion_tokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let cache_read = usage
                    .get("cacheReadTokens")
                    .or_else(|| usage.get("cache_read_tokens"))
                    .and_then(Value::as_u64);
                let cache_write = usage
                    .get("cacheWriteTokens")
                    .or_else(|| usage.get("cache_write_tokens"))
                    .and_then(Value::as_u64);
                if input > 0 || output > 0 {
                    self.emit_usage_update(input, output, cache_read, cache_write);
                }
            }
            Some("tool_call") => {
                let call_id = update
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or("call_dsh");
                let name = update
                    .get("name")
                    .and_then(Value::as_str)
                    .or_else(|| update.get("title").and_then(Value::as_str))
                    .unwrap_or("unknown");
                let input = update.get("rawInput").cloned().unwrap_or(Value::Null);
                if is_todo_tool(name) {
                    self.emit_todo_snapshot(&input);
                }
                self.emit_tool_start(call_id, name, input);
            }
            Some("tool_call_update") => {
                let status = update
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !matches!(status, "completed" | "failed" | "cancelled") {
                    return;
                }
                let call_id = update
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or("call_dsh");
                let name = update.get("title").and_then(Value::as_str).unwrap_or("dsh");
                let output = update
                    .get("rawOutput")
                    .cloned()
                    .or_else(|| update.get("content").cloned())
                    .unwrap_or(Value::Null);
                self.emit_tool_end(call_id, name, output, status != "completed");
            }
            _ => {}
        }
    }

    fn emit_todo_snapshot(&self, input: &Value) {
        let Some(tasks) = parse_todo_snapshot(input) else {
            return;
        };
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::StructuredTaskState {
                run_id: self.run_id.clone(),
                tasks,
            },
        );
    }
}

fn parse_todo_snapshot(input: &Value) -> Option<Vec<StructuredTask>> {
    let todos = input.get("todos")?.as_array()?;
    Some(
        todos
            .iter()
            .enumerate()
            .filter_map(|(index, todo)| {
                let text = todo
                    .get("content")
                    .or_else(|| todo.get("text"))
                    .and_then(Value::as_str)?
                    .trim();
                if text.is_empty() {
                    return None;
                }
                let status = match todo.get("status").and_then(Value::as_str) {
                    Some("in_progress") | Some("in-progress") => StructuredTaskStatus::InProgress,
                    Some("completed") | Some("done") => StructuredTaskStatus::Completed,
                    _ => StructuredTaskStatus::Pending,
                };
                Some(StructuredTask {
                    id: todo
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| index.to_string()),
                    text: text.to_string(),
                    status,
                })
            })
            .collect(),
    )
}

fn is_todo_tool(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "todo_write" | "todowrite"
    )
}

fn parse_acp_plan(update: &Value) -> Option<Vec<StructuredTask>> {
    let entries = update.get("entries")?.as_array()?;
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let text = entry.get("content")?.as_str()?.trim();
            if text.is_empty() {
                return None;
            }
            let status = match entry.get("status")?.as_str()? {
                "pending" => StructuredTaskStatus::Pending,
                "in_progress" => StructuredTaskStatus::InProgress,
                "completed" => StructuredTaskStatus::Completed,
                _ => return None,
            };
            Some(StructuredTask {
                id: entry
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| index.to_string()),
                text: text.to_string(),
                status,
            })
        })
        .collect()
}

/// DSH emits its built-in question tool in snake_case, while AgentCabin's shared
/// inline questionnaire contract uses the canonical `AskUserQuestion` name.
fn canonical_tool_name(tool_name: &str) -> String {
    if tool_name.eq_ignore_ascii_case("ask_user_question") {
        "AskUserQuestion".to_string()
    } else {
        tool_name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{canonical_tool_name, is_todo_tool, parse_acp_plan, parse_todo_snapshot};
    use crate::models::StructuredTaskStatus;
    use serde_json::json;

    #[test]
    fn canonicalizes_dsh_question_tool_name() {
        assert_eq!(canonical_tool_name("ask_user_question"), "AskUserQuestion");
        assert_eq!(canonical_tool_name("Ask_User_Question"), "AskUserQuestion");
        assert_eq!(canonical_tool_name("bash"), "bash");
    }

    #[test]
    fn parses_dsh_todo_snapshots_and_clears_empty_lists() {
        let tasks = parse_todo_snapshot(&json!({
            "todos": [
                {"id": "one", "content": " First step ", "status": "in_progress"},
                {"text": "Second step", "status": "completed"},
                {"content": "  ", "status": "pending"}
            ]
        }))
        .unwrap();

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "one");
        assert_eq!(tasks[0].text, "First step");
        assert_eq!(tasks[0].status, StructuredTaskStatus::InProgress);
        assert_eq!(tasks[1].status, StructuredTaskStatus::Completed);
        assert!(parse_todo_snapshot(&json!({"todos": []}))
            .unwrap()
            .is_empty());
        assert!(parse_todo_snapshot(&json!({})).is_none());
        assert!(is_todo_tool("todo_write"));
        assert!(is_todo_tool("TodoWrite"));
    }

    #[test]
    fn parses_acp_plan_replacement_snapshots() {
        let tasks = parse_acp_plan(&json!({
            "entries": [
                {"id": "one", "content": "Plan first", "status": "in_progress"},
                {"content": "Plan second", "status": "completed"}
            ]
        }))
        .unwrap();

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].status, StructuredTaskStatus::InProgress);
        assert_eq!(tasks[1].id, "1");
        assert_eq!(tasks[1].status, StructuredTaskStatus::Completed);
        assert!(parse_acp_plan(&json!({"entries": []})).unwrap().is_empty());
    }
}

pub(crate) fn acp_assistant_message_chunk(params: Option<&Value>) -> Option<(String, String)> {
    let update = params?.get("update")?;
    if update.get("sessionUpdate").and_then(Value::as_str) != Some("agent_message_chunk") {
        return None;
    }
    let text = acp_text_block(update.get("content"))?;
    let message_id = update
        .get("messageId")
        .and_then(Value::as_str)
        .unwrap_or("dsh-acp-assistant")
        .to_string();
    Some((message_id, text))
}

fn acp_text_block(content: Option<&Value>) -> Option<String> {
    let content = content?;
    if content.get("type").and_then(Value::as_str) != Some("text") {
        return None;
    }
    content
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn content_text(content: Option<&Value>, block_type: &str) -> String {
    content
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some(block_type))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect()
}
