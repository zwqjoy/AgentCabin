//! Pi Coding Agent RPC protocol driver.
//!
//! Pi RPC mode is a long-lived, bidirectional JSONL transport. Commands are
//! written to stdin and responses/events are read asynchronously from stdout.
//! This module keeps that wire format out of the session execution layer and
//! reuses the existing Pi event parser for message/tool/usage normalization.

use crate::agent::pi_parser::PiStdoutParser;
use crate::agent::pipe_parser::PipeStdoutParser;
use crate::agent::session_protocol::{
    CodexSkillRef, CodexTurnOverrides, LifecycleSignal, ParsedLine, PendingKind, SessionProtocol,
    StartupCtx,
};
use crate::models::{
    BusEvent, McpServerInfo, PiExtensionHostSnapshot, PiExtensionUiMethod, PiExtensionUiRequest,
    PiExtensionWidget, PiFeatureCapabilities, PiGoalState, PiMessageQueueState, PiPermissionState,
    PiPlanState, PiSessionTreeNode, PiTodoState,
};
use serde_json::{json, Value};
use std::collections::HashMap;

const STATE_REQUEST_ID: &str = "agentcabin-pi-state";
const COMMANDS_REQUEST_ID: &str = "agentcabin-pi-commands";
const MODELS_REQUEST_ID: &str = "agentcabin-pi-models";
const PI_EXTENSION_SOURCE: &str = "Pi extension UI";
const AGENTCABIN_CONTEXT_USAGE_STATUS_KEY: &str = "agentcabin-context-usage";

#[derive(Debug, Clone, Default)]
pub struct PiExtensionHost {
    pending_requests: Vec<PiExtensionUiRequest>,
    statuses: HashMap<String, String>,
    widgets: HashMap<String, PiExtensionWidget>,
    title: Option<String>,
}

impl PiExtensionHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> PiExtensionHostSnapshot {
        PiExtensionHostSnapshot {
            pending_requests: self.pending_requests.clone(),
            statuses: self.statuses.clone(),
            widgets: self.widgets.clone(),
            title: self.title.clone(),
        }
    }

    pub fn add_pending_request(&mut self, request: PiExtensionUiRequest) {
        if let Some(pos) = self
            .pending_requests
            .iter()
            .position(|r| r.id == request.id)
        {
            self.pending_requests[pos] = request;
        } else {
            self.pending_requests.push(request);
        }
    }

    pub fn remove_pending_request(&mut self, id: &str) -> Option<PiExtensionUiRequest> {
        if let Some(pos) = self.pending_requests.iter().position(|r| r.id == id) {
            Some(self.pending_requests.remove(pos))
        } else {
            None
        }
    }

    pub fn pending_requests(&self) -> &[PiExtensionUiRequest] {
        &self.pending_requests
    }

    pub fn set_status(&mut self, key: String, text: Option<String>) {
        if let Some(text) = text {
            if text.trim().is_empty() {
                self.statuses.remove(&key);
            } else {
                self.statuses.insert(key, text);
            }
        } else {
            self.statuses.remove(&key);
        }
    }

    pub fn set_widget(&mut self, key: String, lines: Vec<String>, placement: Option<String>) {
        if lines.is_empty() {
            self.widgets.remove(&key);
        } else {
            self.widgets.insert(
                key.clone(),
                PiExtensionWidget {
                    key,
                    lines,
                    placement,
                },
            );
        }
    }

    pub fn set_title(&mut self, title: Option<String>) {
        self.title = title;
    }

    pub fn clear(&mut self) {
        self.pending_requests.clear();
        self.statuses.clear();
        self.widgets.clear();
        self.title = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiStreamingBehavior {
    Steer,
    FollowUp,
}

impl PiStreamingBehavior {
    fn as_wire(self) -> &'static str {
        match self {
            Self::Steer => "steer",
            Self::FollowUp => "followUp",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiRpcImage {
    pub data: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkRetryState {
    None,
    WaitingForRetryStart(std::time::Instant),
    Retrying(std::time::Instant),
}

pub struct PiRpc {
    pub(crate) work_retry_state: WorkRetryState,
    parser: PiStdoutParser,
    next_request_id: u64,
    cwd: String,
    slash_commands: Vec<Value>,
    commands_loaded: bool,
    available_models: Vec<Value>,
    last_state: Option<Value>,
    host: PiExtensionHost,
    capabilities: PiFeatureCapabilities,
    plan_enabled: bool,
    goal_enabled: bool,
    permission_enabled: bool,
    permission_mode: String,
    last_control_response: Option<(String, Value)>,
    feature_state_refresh_requested: bool,
    last_session_init: Option<Value>,
    /// stop_reason of the most recent turn's usage update (e.g. "length").
    /// Cleared on agent_start so every turn starts fresh; populated from the
    /// turn's UsageUpdate events, which are emitted even when the assistant
    /// produced no visible text (thinking-only turns truncated by length).
    last_turn_stop_reason: Option<String>,
}

impl Default for PiRpc {
    fn default() -> Self {
        Self {
            work_retry_state: WorkRetryState::None,
            parser: PiStdoutParser::new(),
            next_request_id: 1,
            cwd: String::new(),
            slash_commands: Vec::new(),
            commands_loaded: false,
            available_models: Vec::new(),
            last_state: None,
            host: PiExtensionHost::new(),
            capabilities: PiFeatureCapabilities::default(),
            plan_enabled: false,
            goal_enabled: false,
            permission_enabled: false,
            permission_mode: "accept_edits".to_string(),
            last_control_response: None,
            feature_state_refresh_requested: false,
            last_session_init: None,
            last_turn_stop_reason: None,
        }
    }
}

impl PiRpc {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn has_pending_provider_error(&self) -> bool {
        self.parser.has_pending_error()
    }

    pub fn host_snapshot(&self) -> PiExtensionHostSnapshot {
        self.host.snapshot()
    }

    pub fn host(&self) -> &PiExtensionHost {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut PiExtensionHost {
        &mut self.host
    }

    pub fn slash_commands(&self) -> &[Value] {
        &self.slash_commands
    }

    pub fn commands_loaded(&self) -> bool {
        self.commands_loaded
    }

    pub fn configure_features(
        &mut self,
        plan_enabled: bool,
        goal_enabled: bool,
        permission_enabled: bool,
    ) {
        self.plan_enabled = plan_enabled;
        self.goal_enabled = goal_enabled;
        self.permission_enabled = permission_enabled;
    }

    pub fn configure_permission_mode(&mut self, mode: Option<&str>) {
        self.permission_mode =
            crate::agent::pi_permission::normalize_permission_mode(mode.unwrap_or("accept_edits"))
                .to_string();
    }

    pub fn take_control_response(&mut self) -> Option<(String, Value)> {
        self.last_control_response.take()
    }

    /// The actor consumes this flag and schedules a debounced `get_entries`.
    /// Keeping the request marker in the protocol makes startup discovery and
    /// later state handling use the same structured refresh path.
    pub fn take_feature_state_refresh_request(&mut self) -> bool {
        std::mem::take(&mut self.feature_state_refresh_requested)
    }

    /// stop_reason reported by the most recent turn's usage update.
    /// Returns `Some("length")` when the model's output was truncated by the
    /// output-length limit, even on thinking-only turns with no visible text.
    pub fn last_turn_stop_reason(&self) -> Option<&str> {
        self.last_turn_stop_reason.as_deref()
    }

    /// Clear the recorded stop_reason. Called at agent_start so every turn
    /// starts fresh and stale values never bleed into the next turn.
    pub fn reset_turn_stop_reason(&mut self) {
        self.last_turn_stop_reason = None;
    }

    /// Build cancellation responses without clearing the host. The actor sends these
    /// responses before stopping Pi, then clears the host after the process is gone.
    pub fn pending_cancellation_frames(&self) -> (Vec<Value>, Vec<String>) {
        let mut frames = Vec::new();
        let mut cancelled_ids = Vec::new();
        for req in self.host.pending_requests() {
            frames.push(json!({
                "type": "extension_ui_response",
                "id": req.id,
                "cancelled": true,
            }));
            cancelled_ids.push(req.id.clone());
        }
        (frames, cancelled_ids)
    }

    pub fn clear_host(&mut self) {
        self.host.clear();
    }

    pub fn cancel_all_pending(&mut self) -> (Vec<Value>, Vec<String>) {
        let (frames, cancelled_ids) = self.pending_cancellation_frames();
        self.clear_host();
        (frames, cancelled_ids)
    }

    fn next_id(&mut self, prefix: &str) -> String {
        let id = format!("agentcabin-pi-{}-{}", prefix, self.next_request_id);
        self.next_request_id += 1;
        id
    }

    fn image_values(images: &[PiRpcImage]) -> Value {
        Value::Array(
            images
                .iter()
                .map(|image| {
                    json!({
                        "type": "image",
                        "data": image.data,
                        "mimeType": image.mime_type,
                    })
                })
                .collect(),
        )
    }

    pub fn frame_prompt(
        &mut self,
        text: &str,
        images: &[PiRpcImage],
        streaming_behavior: Option<PiStreamingBehavior>,
    ) -> Value {
        let mut command = serde_json::Map::new();
        command.insert("id".into(), json!(self.next_id("prompt")));
        command.insert("type".into(), json!("prompt"));
        command.insert("message".into(), json!(text));
        if !images.is_empty() {
            command.insert("images".into(), Self::image_values(images));
        }
        if let Some(behavior) = streaming_behavior {
            command.insert("streamingBehavior".into(), json!(behavior.as_wire()));
        }
        Value::Object(command)
    }

    pub fn frame_follow_up(&mut self, text: &str, images: &[PiRpcImage]) -> Value {
        self.frame_queued_command("follow_up", text, images)
    }

    pub fn frame_clear_queue(&mut self) -> Value {
        json!({
            "id": self.next_id("clear-queue"),
            "type": "clear_queue",
        })
    }

    pub fn frame_compact(&mut self) -> Value {
        json!({
            "id": self.next_id("compact"),
            "type": "compact",
        })
    }

    pub fn frame_new_session(&mut self) -> Value {
        json!({"id": self.next_id("new-session"), "type": "new_session"})
    }

    pub fn frame_clone(&mut self) -> Value {
        json!({"id": self.next_id("clone"), "type": "clone"})
    }

    pub fn frame_get_state(&mut self) -> Value {
        json!({"id": self.next_id("state"), "type": "get_state"})
    }

    pub fn frame_get_available_models(&mut self) -> Value {
        json!({
            "id": self.next_id("models"),
            "type": "get_available_models",
        })
    }

    pub fn frame_get_entries(&mut self, since: Option<&str>) -> Value {
        let mut frame = serde_json::Map::new();
        frame.insert("id".into(), json!(self.next_id("entries")));
        frame.insert("type".into(), json!("get_entries"));
        if let Some(since) = since.filter(|value| !value.trim().is_empty()) {
            frame.insert("since".into(), json!(since));
        }
        Value::Object(frame)
    }

    pub fn frame_get_tree(&mut self) -> Value {
        json!({"id": self.next_id("tree"), "type": "get_tree"})
    }

    pub fn frame_get_fork_messages(&mut self) -> Value {
        json!({"id": self.next_id("fork-messages"), "type": "get_fork_messages"})
    }

    pub fn frame_fork(&mut self, entry_id: &str) -> Value {
        json!({"id": self.next_id("fork"), "type": "fork", "entryId": entry_id})
    }

    pub fn frame_switch_session(&mut self, session_path: &str) -> Value {
        json!({
            "id": self.next_id("switch-session"),
            "type": "switch_session",
            "sessionPath": session_path,
        })
    }

    pub fn available_models(&self) -> &[Value] {
        &self.available_models
    }

    pub fn mcp_servers(&self) -> Vec<McpServerInfo> {
        self.last_state
            .as_ref()
            .map(Self::mcp_servers_from_state)
            .unwrap_or_default()
    }

    fn frame_queued_command(
        &mut self,
        command_type: &str,
        text: &str,
        images: &[PiRpcImage],
    ) -> Value {
        let mut command = serde_json::Map::new();
        command.insert("id".into(), json!(self.next_id(command_type)));
        command.insert("type".into(), json!(command_type));
        command.insert("message".into(), json!(text));
        if !images.is_empty() {
            command.insert("images".into(), Self::image_values(images));
        }
        Value::Object(command)
    }

    fn normalize_mcp_status(value: Option<&Value>) -> String {
        match value
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "connected" | "ready" | "running" | "started" | "ok" => "connected",
            "failed" | "error" | "disconnected" | "stopped" => "failed",
            "disabled" => "disabled",
            _ => "pending",
        }
        .to_string()
    }

    /// Pi's current RPC `get_state` does not expose MCP, but extensions/future
    /// versions may add either an array or a name-keyed object. Parse both
    /// shapes without treating absent data as a configured server.
    fn mcp_servers_from_state(state: &Value) -> Vec<McpServerInfo> {
        let Some(raw) = state.get("mcpServers").or_else(|| state.get("mcp_servers")) else {
            return Vec::new();
        };

        let mut servers = Vec::new();
        let mut push_server = |fallback_name: Option<&str>, item: &Value| {
            let object = item.as_object();
            let name = object
                .and_then(|v| v.get("name").or_else(|| v.get("serverName")))
                .and_then(Value::as_str)
                .or(fallback_name)
                .unwrap_or("unknown")
                .trim();
            if name.is_empty() {
                return;
            }
            let status = Self::normalize_mcp_status(
                object.and_then(|v| v.get("status").or_else(|| v.get("state"))),
            );
            let server_type = object
                .and_then(|v| v.get("type").or_else(|| v.get("transport")))
                .and_then(Value::as_str)
                .map(str::to_string);
            let error = object
                .and_then(|v| v.get("error").or_else(|| v.get("errorMessage")))
                .and_then(Value::as_str)
                .map(str::to_string);
            servers.push(McpServerInfo {
                name: name.to_string(),
                status,
                server_type,
                error,
            });
        };

        match raw {
            Value::Array(items) => {
                for item in items {
                    push_server(None, item);
                }
            }
            Value::Object(items) => {
                for (name, item) in items {
                    push_server(Some(name), item);
                }
            }
            _ => {}
        }
        servers.sort_by(|left, right| left.name.cmp(&right.name));
        servers
    }

    fn session_init_from_state(
        &self,
        run_id: &str,
        state: &Value,
        commands_loaded: bool,
    ) -> BusEvent {
        let model = state.get("model").and_then(|model_val| {
            let id = model_val
                .get("id")
                .or_else(|| model_val.get("name"))
                .and_then(Value::as_str)?;
            let provider = model_val.get("provider").and_then(Value::as_str);
            match provider {
                Some(p) if !p.is_empty() && p != "agentcabin" => {
                    if id.starts_with(&format!("{}/", p)) {
                        Some(id.to_string())
                    } else {
                        Some(format!("{}/{}", p, id))
                    }
                }
                _ => Some(id.to_string()),
            }
        });
        let cwd = state
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or(&self.cwd)
            .to_string();

        BusEvent::SessionInit {
            run_id: run_id.to_string(),
            session_id: state
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string),
            model,
            model_options: vec![],
            tools: vec![],
            cwd,
            slash_commands: self.slash_commands.clone(),
            commands_loaded,
            mcp_servers: Self::mcp_servers_from_state(state),
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
        }
    }

    fn push_session_init_if_changed(&mut self, out: &mut ParsedLine, event: BusEvent) {
        let Ok(snapshot) = serde_json::to_value(&event) else {
            out.events.push(event);
            return;
        };
        if self.last_session_init.as_ref() == Some(&snapshot) {
            return;
        }
        self.last_session_init = Some(snapshot);
        out.events.push(event);
    }

    fn parse_response(&mut self, run_id: &str, msg: &Value) -> ParsedLine {
        if let Some(id) = msg.get("id").and_then(Value::as_str) {
            self.last_control_response = Some((id.to_string(), msg.clone()));
        }
        let mut out = ParsedLine::default();
        let success = msg.get("success").and_then(Value::as_bool).unwrap_or(false);
        let command = msg.get("command").and_then(Value::as_str).unwrap_or("");

        if !success {
            let error = msg
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Pi RPC command failed")
                .to_string();
            out.events.push(BusEvent::CommandOutput {
                run_id: run_id.to_string(),
                content: format!("[pi rpc] {}: {}", command, error),
            });
            if matches!(command, "prompt" | "steer" | "follow_up") {
                out.lifecycle = Some(LifecycleSignal::TurnFailed(Some(error)));
            }
            if let Some(id) = msg.get("id").and_then(Value::as_str) {
                out.control_response = Some((id.to_string(), msg.clone()));
            }
            return out;
        }

        if let Some(id) = msg.get("id").and_then(Value::as_str) {
            out.control_response = Some((id.to_string(), msg.clone()));
        }

        let is_state_response = command == "get_state"
            || msg.get("id").and_then(Value::as_str) == Some(STATE_REQUEST_ID);
        if is_state_response {
            if let Some(state) = msg.get("data") {
                self.last_state = Some(state.clone());
                out.thread_id = state
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let event = self.session_init_from_state(run_id, state, self.commands_loaded);
                self.push_session_init_if_changed(&mut out, event);
            }
        } else if command == "get_commands"
            || msg.get("id").and_then(Value::as_str) == Some(COMMANDS_REQUEST_ID)
        {
            self.slash_commands = msg
                .get("data")
                .and_then(|data| data.get("commands"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            self.commands_loaded = true;
            self.feature_state_refresh_requested = true;
            let names = self.slash_commands.iter().filter_map(|command| {
                command.as_str().map(str::to_string).or_else(|| {
                    command
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
            });
            self.capabilities = PiFeatureCapabilities::from_commands(
                true,
                names,
                self.plan_enabled,
                self.goal_enabled,
                self.permission_enabled,
            );
            if let Some(state) = self.last_state.as_ref() {
                let event = self.session_init_from_state(run_id, state, true);
                self.push_session_init_if_changed(&mut out, event);
            }
            out.events.push(BusEvent::PiFeatureCapabilities {
                run_id: run_id.to_string(),
                capabilities: self.capabilities.clone(),
            });
            out.events.push(BusEvent::PiPermissionState {
                run_id: run_id.to_string(),
                state: PiPermissionState {
                    available: self.capabilities.permission_available,
                    mode: self.permission_mode.clone(),
                    pending_request_count: self.host.pending_requests().len(),
                    restart_required: false,
                },
            });
        } else if command == "get_entries" {
            if let Some(data) = msg.get("data") {
                let entries = data
                    .get("entries")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let leaf_id = data
                    .get("leafId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let last_entry_id = entries
                    .last()
                    .and_then(|entry| entry.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                out.events.push(BusEvent::PiSessionEntries {
                    run_id: run_id.to_string(),
                    entries: entries.clone(),
                    leaf_id,
                    last_entry_id,
                });
                self.append_feature_states(run_id, &entries, &mut out);
            }
        } else if command == "get_tree" {
            if let Some(data) = msg.get("data") {
                let roots = data
                    .get("tree")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|node| serde_json::from_value::<PiSessionTreeNode>(node).ok())
                    .collect();
                out.events.push(BusEvent::PiSessionTree {
                    run_id: run_id.to_string(),
                    roots,
                    leaf_id: data
                        .get("leafId")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                });
            }
        } else if command == "get_available_models"
            || msg.get("id").and_then(Value::as_str) == Some(MODELS_REQUEST_ID)
        {
            self.available_models = msg
                .get("data")
                .and_then(|data| data.get("models"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
        }

        out
    }

    fn append_feature_states(&self, run_id: &str, entries: &[Value], out: &mut ParsedLine) {
        for entry in entries.iter().rev() {
            let custom_type = entry
                .get("customType")
                .and_then(Value::as_str)
                .unwrap_or("");
            let data = entry.get("data").cloned().unwrap_or(Value::Null);
            if custom_type == "plan-mode-state" {
                let enabled = data
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                out.events.push(BusEvent::PiPlanState {
                    run_id: run_id.to_string(),
                    state: PiPlanState {
                        phase: if enabled { "active" } else { "inactive" }.to_string(),
                        updated_at: entry
                            .get("timestamp")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        detail: None,
                    },
                });
                break;
            }
        }
        let mut goal_state_found = false;
        for entry in entries.iter().rev() {
            if entry.get("customType").and_then(Value::as_str) != Some("goal-state") {
                continue;
            }
            goal_state_found = true;
            let goal = entry
                .get("data")
                .and_then(|value| value.get("goal"))
                .cloned()
                .unwrap_or(Value::Null);
            let status = goal
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("inactive");
            let phase = match status {
                "active" => "active",
                "paused" => "paused",
                "budget_limited" => "budget_limited",
                "complete" => "complete",
                _ => "inactive",
            };
            out.events.push(BusEvent::PiGoalState {
                run_id: run_id.to_string(),
                state: PiGoalState {
                    phase: phase.to_string(),
                    id: goal.get("id").and_then(Value::as_str).map(str::to_string),
                    objective: goal.get("text").and_then(Value::as_str).map(str::to_string),
                    iteration: goal.get("iteration").and_then(Value::as_u64),
                    token_budget: goal.get("tokenBudget").and_then(Value::as_u64),
                    tokens_used: goal.get("tokensUsed").and_then(Value::as_u64),
                    time_used_seconds: goal.get("timeUsedSeconds").and_then(Value::as_u64),
                    updated_at: entry
                        .get("timestamp")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                },
            });
            break;
        }
        // A freshly created session has no goal-state entry yet. That means the
        // Goal extension is available but currently inactive; it must not remain
        // in the UI's initial "unavailable" state (which also disables the
        // button). Only report unavailable when command discovery did not find
        // the extension at all.
        if !goal_state_found && self.capabilities.goal_available {
            out.events.push(BusEvent::PiGoalState {
                run_id: run_id.to_string(),
                state: PiGoalState {
                    phase: "inactive".to_string(),
                    id: None,
                    objective: None,
                    iteration: None,
                    token_budget: None,
                    tokens_used: None,
                    time_used_seconds: None,
                    updated_at: None,
                },
            });
        }

        let todo_state = entries.iter().rev().find_map(Self::todo_state_from_entry);
        out.events.push(BusEvent::PiTodoState {
            run_id: run_id.to_string(),
            state: todo_state.unwrap_or_default(),
        });
    }

    fn todo_state_from_entry(entry: &Value) -> Option<PiTodoState> {
        if let Some(message) = entry.get("message") {
            if message.get("role").and_then(Value::as_str) == Some("toolResult")
                && message.get("toolName").and_then(Value::as_str) == Some("todo")
                && message.get("isError").and_then(Value::as_bool) != Some(true)
            {
                if let Some(state) = message
                    .get("details")
                    .and_then(|details| details.get("state"))
                    .and_then(|state| serde_json::from_value::<PiTodoState>(state.clone()).ok())
                {
                    return Some(state);
                }
            }
        }

        if entry.get("customType").and_then(Value::as_str) != Some("pi-deck-todo") {
            return None;
        }
        let todos = entry.get("data")?.get("todos")?.as_array()?;
        let tasks = todos
            .iter()
            .filter_map(|todo| {
                Some(crate::models::PiTodoTask {
                    name: todo.get("text")?.as_str()?.to_string(),
                    description: String::new(),
                    status: if todo.get("done").and_then(Value::as_bool).unwrap_or(false) {
                        "completed".to_string()
                    } else {
                        "pending".to_string()
                    },
                })
            })
            .collect();
        Some(PiTodoState {
            phases: vec![crate::models::PiTodoPhase {
                name: "Todo".to_string(),
                tasks,
            }],
            working_on: None,
        })
    }

    fn parse_extension_ui_request(&mut self, run_id: &str, msg: &Value, out: &mut ParsedLine) {
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let req_id = msg.get("id").and_then(Value::as_str);

        match method {
            "select" | "confirm" | "input" | "editor" => {
                let id = req_id.unwrap_or("pi-ext-req").to_string();
                let ui_method = match method {
                    "select" => PiExtensionUiMethod::Select,
                    "confirm" => PiExtensionUiMethod::Confirm,
                    "input" => PiExtensionUiMethod::Input,
                    _ => PiExtensionUiMethod::Editor,
                };
                let title = msg.get("title").and_then(Value::as_str).map(str::to_string);
                let message = msg
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let options: Vec<String> = msg
                    .get("options")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                let placeholder = msg
                    .get("placeholder")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let prefill = msg
                    .get("prefill")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let expires_at = msg
                    .get("expiresAt")
                    .or_else(|| msg.get("expires_at"))
                    .and_then(Value::as_str)
                    .map(str::to_string);

                let req = PiExtensionUiRequest {
                    id: id.clone(),
                    run_id: run_id.to_string(),
                    method: ui_method,
                    title: title.clone(),
                    message: message.clone(),
                    options: options.clone(),
                    placeholder: placeholder.clone(),
                    prefill: prefill.clone(),
                    created_at: crate::models::now_iso(),
                    expires_at,
                };

                self.host.add_pending_request(req.clone());

                out.events.push(BusEvent::PiExtensionUiRequestCreated {
                    run_id: run_id.to_string(),
                    request: req,
                });
                out.events.push(BusEvent::PiExtensionStateSync {
                    run_id: run_id.to_string(),
                    snapshot: self.host.snapshot(),
                });

                let requested_schema = match ui_method {
                    PiExtensionUiMethod::Select => Some(json!({
                        "type": "object",
                        "properties": {
                            "value": {
                                "type": "string",
                                "title": "Selection",
                                "enum": options,
                            }
                        },
                        "required": ["value"]
                    })),
                    PiExtensionUiMethod::Input => Some(json!({
                        "type": "object",
                        "properties": {
                            "value": {
                                "type": "string",
                                "title": placeholder.as_deref().unwrap_or("Value")
                            }
                        },
                        "required": ["value"]
                    })),
                    PiExtensionUiMethod::Editor => Some(json!({
                        "type": "object",
                        "properties": {
                            "value": {
                                "type": "string",
                                "title": "Text",
                                "default": prefill.clone().unwrap_or_default()
                            }
                        },
                        "required": ["value"]
                    })),
                    PiExtensionUiMethod::Confirm => None,
                };

                let detail = message.as_deref().unwrap_or("");
                let title_str = title.as_deref().unwrap_or("Pi extension request");
                let msg_text = if detail.is_empty() {
                    title_str.to_string()
                } else if title_str.is_empty() {
                    detail.to_string()
                } else {
                    format!("{}\n{}", title_str, detail)
                };

                out.events.push(BusEvent::ElicitationPrompt {
                    run_id: run_id.to_string(),
                    request_id: id.clone(),
                    mcp_server_name: PI_EXTENSION_SOURCE.to_string(),
                    message: msg_text,
                    elicitation_id: Some(id),
                    mode: Some(format!("pi_extension_{}", method)),
                    url: None,
                    requested_schema,
                });
            }
            "notify" => {
                let notice_type = msg
                    .get("notifyType")
                    .or_else(|| msg.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("info")
                    .to_string();
                let message = msg
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Extension notification")
                    .to_string();

                out.events.push(BusEvent::PiExtensionNotice {
                    run_id: run_id.to_string(),
                    notice_type,
                    message,
                });
            }
            "setStatus" => {
                let key = msg
                    .get("statusKey")
                    .or_else(|| msg.get("key"))
                    .and_then(Value::as_str)
                    .unwrap_or("default")
                    .to_string();
                let text = msg
                    .get("statusText")
                    .or_else(|| msg.get("text"))
                    .or_else(|| msg.get("message"))
                    .and_then(Value::as_str)
                    .map(str::to_string);

                if key == AGENTCABIN_CONTEXT_USAGE_STATUS_KEY {
                    if let Some(payload) = text
                        .as_deref()
                        .and_then(|value| serde_json::from_str::<Value>(value).ok())
                    {
                        out.events.push(BusEvent::PiContextUsage {
                            run_id: run_id.to_string(),
                            snapshot: payload,
                        });
                    }
                    return;
                }

                self.host.set_status(key, text);
                out.events.push(BusEvent::PiExtensionStateSync {
                    run_id: run_id.to_string(),
                    snapshot: self.host.snapshot(),
                });
            }
            "setWidget" => {
                let key = msg
                    .get("widgetKey")
                    .or_else(|| msg.get("key"))
                    .and_then(Value::as_str)
                    .unwrap_or("default")
                    .to_string();
                let lines = msg
                    .get("widgetLines")
                    .or_else(|| msg.get("lines"))
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                let placement = msg
                    .get("placement")
                    .or_else(|| msg.get("widgetPlacement"))
                    .and_then(Value::as_str)
                    .map(str::to_string);

                self.host.set_widget(key, lines, placement);
                out.events.push(BusEvent::PiExtensionStateSync {
                    run_id: run_id.to_string(),
                    snapshot: self.host.snapshot(),
                });
            }
            "setTitle" => {
                let title = msg.get("title").and_then(Value::as_str).map(str::to_string);
                self.host.set_title(title);
                out.events.push(BusEvent::PiExtensionStateSync {
                    run_id: run_id.to_string(),
                    snapshot: self.host.snapshot(),
                });
            }
            "set_editor_text" => {
                let text = msg
                    .get("text")
                    .or_else(|| msg.get("editorText"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                out.events.push(BusEvent::PiExtensionEditorAction {
                    run_id: run_id.to_string(),
                    text,
                });
            }
            "custom" => {
                log::warn!("[pi_rpc] custom TUI requested by Pi extension: {:?}", msg);
                out.events.push(BusEvent::PiExtensionNotice {
                    run_id: run_id.to_string(),
                    notice_type: "warning".to_string(),
                    message: "此 Pi 扩展请求了当前 AgentCabin 尚未支持的自定义界面。".to_string(),
                });
                if let Some(id) = req_id {
                    out.immediate_frames.push(json!({
                        "type": "extension_ui_response",
                        "id": id,
                        "cancelled": true,
                    }));
                }
            }
            _ => {
                log::debug!(
                    "[pi_rpc] unknown extension_ui_request method '{}': {:?}",
                    method,
                    msg
                );
                if let Some(id) = req_id {
                    out.immediate_frames.push(json!({
                        "type": "extension_ui_response",
                        "id": id,
                        "cancelled": true,
                    }));
                }
            }
        }
    }

    fn normalize_extension_response(request_id: &str, response: &Value) -> Value {
        if response.get("cancelled").and_then(Value::as_bool) == Some(true) {
            return json!({
                "type": "extension_ui_response",
                "id": request_id,
                "cancelled": true,
            });
        }

        if let Some(action) = response.get("action").and_then(Value::as_str) {
            if matches!(action, "decline" | "cancel") {
                return json!({
                    "type": "extension_ui_response",
                    "id": request_id,
                    "cancelled": true,
                });
            }
            if action == "accept" {
                if let Some(content) = response.get("content").and_then(Value::as_object) {
                    if let Some(confirmed) = content.get("confirmed").and_then(Value::as_bool) {
                        return json!({
                            "type": "extension_ui_response",
                            "id": request_id,
                            "confirmed": confirmed,
                        });
                    }
                    if let Some(value) = content.get("value") {
                        return json!({
                            "type": "extension_ui_response",
                            "id": request_id,
                            "value": value,
                        });
                    }
                    if content.len() == 1 {
                        if let Some(value) = content.values().next() {
                            return json!({
                                "type": "extension_ui_response",
                                "id": request_id,
                                "value": value,
                            });
                        }
                    }
                }
            }
        }

        if let Some(confirmed) = response.get("confirmed").and_then(Value::as_bool) {
            return json!({
                "type": "extension_ui_response",
                "id": request_id,
                "confirmed": confirmed,
            });
        }
        if let Some(value) = response.get("value") {
            return json!({
                "type": "extension_ui_response",
                "id": request_id,
                "value": value,
            });
        }
        if let Some(behavior) = response.get("behavior").and_then(Value::as_str) {
            return json!({
                "type": "extension_ui_response",
                "id": request_id,
                "confirmed": behavior == "allow",
            });
        }

        json!({
            "type": "extension_ui_response",
            "id": request_id,
            "cancelled": true,
        })
    }
}

impl SessionProtocol for PiRpc {
    fn startup_messages(&mut self, ctx: &StartupCtx) -> Vec<Value> {
        self.cwd.clone_from(&ctx.cwd);
        vec![
            json!({
                "id": STATE_REQUEST_ID,
                "type": "get_state",
            }),
            json!({
                "id": COMMANDS_REQUEST_ID,
                "type": "get_commands",
            }),
            json!({
                "id": MODELS_REQUEST_ID,
                "type": "get_available_models",
            }),
        ]
    }

    fn frame_user_turn(
        &mut self,
        text: &str,
        image_paths: &[String],
        _skills: &[CodexSkillRef],
        _overrides: &CodexTurnOverrides,
    ) -> Vec<Value> {
        let augmented = if image_paths.is_empty() {
            text.to_string()
        } else {
            format!(
                "{}\n\nAttached image paths:\n{}",
                text,
                image_paths.join("\n")
            )
        };
        vec![self.frame_prompt(&augmented, &[], None)]
    }

    fn frame_interrupt(&mut self) -> Vec<Value> {
        vec![
            self.frame_clear_queue(),
            json!({
                "id": self.next_id("abort"),
                "type": "abort",
            }),
        ]
    }

    fn frame_steer(&mut self, text: &str) -> Vec<Value> {
        vec![json!({
            "id": self.next_id("steer"),
            "type": "steer",
            "message": text,
        })]
    }

    fn parse_line(&mut self, run_id: &str, line: &str) -> ParsedLine {
        let mut out = ParsedLine::default();
        let line = line.trim();
        if line.is_empty() {
            return out;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => return out,
        };
        let event_type = msg.get("type").and_then(Value::as_str).unwrap_or("");

        if event_type == "response" {
            return self.parse_response(run_id, &msg);
        }

        match event_type {
            "agent_start" => {
                // Every turn starts fresh: clear any stale stop_reason so it
                // never bleeds into the next turn's continuation decision.
                self.last_turn_stop_reason = None;
                out.lifecycle = Some(LifecycleSignal::TurnStarted);
            }
            // Current Pi RPC emits agent_end after an abort or a completed turn.
            // Keep agent_settled for older Pi versions that used that name.
            "agent_end" | "agent_settled" => {
                out.lifecycle = Some(LifecycleSignal::TurnCompleted);
            }
            "extension_ui_request" => {
                self.parse_extension_ui_request(run_id, &msg, &mut out);
                return out;
            }
            "queue_update" => {
                let steering: Vec<String> = msg
                    .get("steering")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let follow_up: Vec<String> = msg
                    .get("followUp")
                    .or_else(|| msg.get("follow_up"))
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let state = PiMessageQueueState {
                    pending_count: steering.len() + follow_up.len(),
                    steering,
                    follow_up,
                    details_known: true,
                };
                out.events.push(BusEvent::PiQueueUpdated {
                    run_id: run_id.to_string(),
                    state,
                });
            }
            _ => {}
        }

        out.events.extend(self.parser.parse_line(run_id, &msg));

        // Capture the turn's stop_reason from the UsageUpdate event. The
        // UsageUpdate is emitted at message_end even when the assistant
        // produced no visible text (thinking-only turns), so this is the only
        // reliable source of "length" for auto-continuation decisions.
        for event in &out.events {
            if let crate::models::BusEvent::UsageUpdate {
                stop_reason: Some(reason),
                ..
            } = event
            {
                self.last_turn_stop_reason = Some(reason.clone());
            }
        }

        out
    }

    fn frame_response(
        &mut self,
        _kind: PendingKind,
        request_id: &str,
        response: Value,
    ) -> Vec<Value> {
        vec![Self::normalize_extension_response(request_id, &response)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_requests_state_and_commands() {
        let mut rpc = PiRpc::new();
        let messages = rpc.startup_messages(&StartupCtx {
            cwd: "/workspace".to_string(),
            ..StartupCtx::default()
        });
        assert_eq!(
            messages,
            vec![
                json!({"id": STATE_REQUEST_ID, "type": "get_state"}),
                json!({"id": COMMANDS_REQUEST_ID, "type": "get_commands"}),
                json!({"id": MODELS_REQUEST_ID, "type": "get_available_models"}),
            ]
        );
    }

    #[test]
    fn repeated_identical_state_responses_emit_one_session_init() {
        let mut rpc = PiRpc::new();
        let first = r#"{"type":"response","command":"get_state","success":true,"data":{"sessionId":"s-1","model":{"id":"gpt-5"},"cwd":"/workspace"}}"#;

        let first_result = rpc.parse_line("run-1", first);
        assert_eq!(
            first_result
                .events
                .iter()
                .filter(|event| matches!(event, BusEvent::SessionInit { .. }))
                .count(),
            1
        );

        let repeated_result = rpc.parse_line("run-1", first);
        assert!(repeated_result.events.is_empty());

        let commands = r#"{"type":"response","command":"get_commands","success":true,"data":{"commands":[{"name":"mcp","description":"status"}]}}"#;
        let commands_result = rpc.parse_line("run-1", commands);
        assert_eq!(
            commands_result
                .events
                .iter()
                .filter(|event| matches!(event, BusEvent::SessionInit { .. }))
                .count(),
            1
        );
        let repeated_commands_result = rpc.parse_line("run-1", commands);
        assert_eq!(
            repeated_commands_result
                .events
                .iter()
                .filter(|event| matches!(event, BusEvent::SessionInit { .. }))
                .count(),
            0
        );

        let changed = r#"{"type":"response","command":"get_state","success":true,"data":{"sessionId":"s-1","model":{"id":"gpt-5.1"},"cwd":"/workspace"}}"#;
        let changed_result = rpc.parse_line("run-1", changed);
        assert_eq!(
            changed_result
                .events
                .iter()
                .filter(|event| matches!(event, BusEvent::SessionInit { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn prompt_supports_images_and_queue_behavior() {
        let mut rpc = PiRpc::new();
        let command = rpc.frame_prompt(
            "look",
            &[PiRpcImage {
                data: "AAAA".into(),
                mime_type: "image/png".into(),
            }],
            Some(PiStreamingBehavior::Steer),
        );
        assert_eq!(command.get("type").and_then(Value::as_str), Some("prompt"));
        assert_eq!(
            command.get("streamingBehavior").and_then(Value::as_str),
            Some("steer")
        );
        assert_eq!(
            command
                .get("images")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn steer_follow_up_and_abort_use_rpc_commands() {
        let mut rpc = PiRpc::new();
        assert_eq!(
            rpc.frame_steer("change")[0]
                .get("type")
                .and_then(Value::as_str),
            Some("steer")
        );
        assert_eq!(
            rpc.frame_follow_up("later", &[])
                .get("type")
                .and_then(Value::as_str),
            Some("follow_up")
        );
        let interrupt = rpc.frame_interrupt();
        assert_eq!(
            interrupt[0].get("type").and_then(Value::as_str),
            Some("clear_queue")
        );
        assert_eq!(
            interrupt[1].get("type").and_then(Value::as_str),
            Some("abort")
        );
        assert_eq!(
            rpc.frame_clear_queue().get("type").and_then(Value::as_str),
            Some("clear_queue")
        );
        assert_eq!(
            rpc.frame_compact().get("type").and_then(Value::as_str),
            Some("compact")
        );
    }

    #[test]
    fn interrupt_frames_clear_queue_before_abort() {
        let mut rpc = PiRpc::new();
        let frames = rpc.frame_interrupt();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0]["type"], "clear_queue");
        assert_eq!(frames[1]["type"], "abort");
    }

    #[test]
    fn session_management_frames_match_pi_rpc_commands() {
        let mut rpc = PiRpc::new();
        assert_eq!(rpc.frame_new_session()["type"], "new_session");
        assert_eq!(rpc.frame_clone()["type"], "clone");
        assert_eq!(rpc.frame_fork("entry-1")["entryId"], "entry-1");
        assert_eq!(
            rpc.frame_switch_session("/tmp/session.jsonl")["sessionPath"],
            "/tmp/session.jsonl"
        );
    }

    #[test]
    fn state_response_emits_session_init_and_resume_id() {
        let mut rpc = PiRpc::new();
        rpc.startup_messages(&StartupCtx {
            cwd: "/workspace".to_string(),
            ..StartupCtx::default()
        });
        let parsed = rpc.parse_line(
            "run-1",
            &json!({
                "id": STATE_REQUEST_ID,
                "type": "response",
                "command": "get_state",
                "success": true,
                "data": {
                    "sessionId": "session-123",
                    "cwd": "/workspace",
                    "model": {"id": "model-a"}
                }
            })
            .to_string(),
        );
        assert_eq!(parsed.thread_id.as_deref(), Some("session-123"));
        assert!(matches!(
            parsed.events.first(),
            Some(BusEvent::SessionInit {
                session_id: Some(id),
                cwd,
                commands_loaded: false,
                ..
            }) if id == "session-123" && cwd == "/workspace"
        ));
    }

    #[test]
    fn state_response_qualifies_model_with_provider() {
        let mut rpc = PiRpc::new();
        rpc.startup_messages(&StartupCtx {
            cwd: "/workspace".to_string(),
            ..StartupCtx::default()
        });
        let parsed = rpc.parse_line(
            "run-1",
            &json!({
                "id": STATE_REQUEST_ID,
                "type": "response",
                "command": "get_state",
                "success": true,
                "data": {
                    "sessionId": "session-123",
                    "cwd": "/workspace",
                    "model": {
                        "id": "zhanlu/kimi-k3",
                        "provider": "provider-1788769121841"
                    }
                }
            })
            .to_string(),
        );
        match parsed.events.first() {
            Some(BusEvent::SessionInit { model, .. }) => {
                assert_eq!(
                    model.as_deref(),
                    Some("provider-1788769121841/zhanlu/kimi-k3")
                );
            }
            other => panic!("expected SessionInit with qualified model, got {:?}", other),
        }
    }

    #[test]
    fn commands_response_refreshes_session_commands() {
        let mut rpc = PiRpc::new();
        let _ = rpc.parse_line(
            "run-1",
            &json!({
                "id": STATE_REQUEST_ID,
                "type": "response",
                "command": "get_state",
                "success": true,
                "data": {
                    "sessionId": "session-123",
                    "model": {"id": "model-a"}
                }
            })
            .to_string(),
        );
        let parsed = rpc.parse_line(
            "run-1",
            &json!({
                "id": COMMANDS_REQUEST_ID,
                "type": "response",
                "command": "get_commands",
                "success": true,
                "data": {
                    "commands": [
                        {
                            "name": "fix-tests",
                            "description": "Fix failing tests",
                            "source": "prompt"
                        },
                        {
                            "name": "skill:research",
                            "description": "Research",
                            "source": "skill"
                        }
                    ]
                }
            })
            .to_string(),
        );
        assert!(matches!(
            parsed.events.first(),
            Some(BusEvent::SessionInit {
                slash_commands,
                session_id: Some(session_id),
                commands_loaded: true,
                ..
            }) if session_id == "session-123"
                && slash_commands.len() == 2
                && slash_commands[0]["name"] == "fix-tests"
        ));
        assert!(rpc.take_feature_state_refresh_request());
        assert!(!rpc.take_feature_state_refresh_request());
    }

    #[test]
    fn empty_commands_response_is_marked_loaded() {
        let mut rpc = PiRpc::new();
        assert!(!rpc.commands_loaded());
        let _ = rpc.parse_line(
            "run-1",
            &json!({
                "id": STATE_REQUEST_ID,
                "type": "response",
                "command": "get_state",
                "success": true,
                "data": {"sessionId": "session-123", "model": {"id": "model-a"}}
            })
            .to_string(),
        );
        let parsed = rpc.parse_line(
            "run-1",
            &json!({
                "id": COMMANDS_REQUEST_ID,
                "type": "response",
                "command": "get_commands",
                "success": true,
                "data": {"commands": []}
            })
            .to_string(),
        );
        assert!(matches!(
            parsed.events.first(),
            Some(BusEvent::SessionInit {
                slash_commands,
                commands_loaded: true,
                ..
            }) if slash_commands.is_empty()
        ));
        assert!(rpc.commands_loaded());
    }

    #[test]
    fn entries_response_updates_plan_and_goal_from_structured_state() {
        let mut rpc = PiRpc::new();
        let parsed = rpc.parse_line(
            "run-1",
            &json!({
                "id": "entries-1",
                "type": "response",
                "command": "get_entries",
                "success": true,
                "data": {
                    "leafId": "goal-1",
                    "entries": [
                        {"id": "plan-1", "type": "custom", "customType": "plan-mode-state", "data": {"enabled": true}},
                        {"id": "goal-1", "type": "custom", "customType": "goal-state", "data": {"goal": {"status": "paused", "text": "ship it"}}},
                        {"id": "todo-1", "type": "message", "message": {"role": "toolResult", "toolName": "todo", "details": {"action": "transition", "state": {"phases": [{"name": "Build", "tasks": [{"name": "Wire state", "description": "Connect the session state", "status": "in_progress"}]}], "workingOn": "Wire state"}}}}
                    ]
                }
            })
            .to_string(),
        );
        assert!(matches!(
            parsed.events.iter().find(|event| matches!(event, BusEvent::PiPlanState { .. })),
            Some(BusEvent::PiPlanState { state, .. }) if state.phase == "active"
        ));
        assert!(matches!(
            parsed.events.iter().find(|event| matches!(event, BusEvent::PiGoalState { .. })),
            Some(BusEvent::PiGoalState { state, .. }) if state.phase == "paused" && state.objective.as_deref() == Some("ship it")
        ));
        assert!(matches!(
            parsed.events.iter().find(|event| matches!(event, BusEvent::PiTodoState { .. })),
            Some(BusEvent::PiTodoState { state, .. })
                if state.working_on.as_deref() == Some("Wire state")
                    && state.phases[0].tasks[0].status == "in_progress"
        ));
    }

    #[test]
    fn entries_without_goal_state_report_inactive_when_goal_extension_is_available() {
        let mut rpc = PiRpc::new();
        rpc.configure_features(false, true, false);
        let _ = rpc.parse_line(
            "run-1",
            &json!({
                "id": COMMANDS_REQUEST_ID,
                "type": "response",
                "command": "get_commands",
                "success": true,
                "data": {"commands": [{"name": "goal"}]}
            })
            .to_string(),
        );

        let parsed = rpc.parse_line(
            "run-1",
            &json!({
                "id": "entries-1",
                "type": "response",
                "command": "get_entries",
                "success": true,
                "data": {"entries": []}
            })
            .to_string(),
        );

        assert!(matches!(
            parsed.events.iter().find(|event| matches!(event, BusEvent::PiGoalState { .. })),
            Some(BusEvent::PiGoalState { state, .. }) if state.phase == "inactive"
        ));
    }

    #[test]
    fn available_models_response_updates_live_catalog() {
        let mut rpc = PiRpc::new();
        let parsed = rpc.parse_line(
            "run-1",
            &json!({
                "id": MODELS_REQUEST_ID,
                "type": "response",
                "command": "get_available_models",
                "success": true,
                "data": {
                    "models": [
                        {
                            "provider": "agentcabin",
                            "id": "qwen3-coder",
                            "name": "Qwen 3 Coder",
                            "reasoning": true,
                            "contextWindow": 131072
                        }
                    ]
                }
            })
            .to_string(),
        );
        assert!(parsed.events.is_empty());
        assert_eq!(rpc.available_models().len(), 1);
        assert_eq!(rpc.available_models()[0]["id"], "qwen3-coder");
    }

    #[test]
    fn available_models_can_be_refreshed_with_a_correlatable_request() {
        let mut rpc = PiRpc::new();
        let frame = rpc.frame_get_available_models();
        let request_id = frame["id"].as_str().expect("model request id");

        assert_eq!(frame["type"], "get_available_models");
        assert!(request_id.starts_with("agentcabin-pi-models-"));

        let response = json!({
            "id": request_id,
            "type": "response",
            "command": "get_available_models",
            "success": true,
            "data": {"models": [{"provider": "openai-codex", "id": "gpt-5.6-sol"}]}
        });
        let parsed = rpc.parse_line("run-1", &response.to_string());

        assert!(parsed.events.is_empty());
        assert_eq!(rpc.available_models().len(), 1);
        assert_eq!(
            rpc.take_control_response(),
            Some((request_id.to_string(), response))
        );
    }

    #[test]
    fn state_response_accepts_optional_pi_mcp_shapes() {
        let mut rpc = PiRpc::new();
        let parsed = rpc.parse_line(
            "run-1",
            &json!({
                "id": STATE_REQUEST_ID,
                "type": "response",
                "command": "get_state",
                "success": true,
                "data": {
                    "sessionId": "session-123",
                    "mcpServers": {
                        "filesystem": {"status": "ready", "transport": "stdio"},
                        "broken": {"state": "error", "errorMessage": "connection refused"}
                    }
                }
            })
            .to_string(),
        );
        let BusEvent::SessionInit { mcp_servers, .. } = &parsed.events[0] else {
            panic!("expected session init");
        };
        assert_eq!(mcp_servers.len(), 2);
        let filesystem = mcp_servers
            .iter()
            .find(|server| server.name == "filesystem")
            .unwrap();
        assert_eq!(filesystem.status, "connected");
        assert_eq!(filesystem.server_type.as_deref(), Some("stdio"));
        let broken = mcp_servers
            .iter()
            .find(|server| server.name == "broken")
            .unwrap();
        assert_eq!(broken.status, "failed");
        assert_eq!(broken.error.as_deref(), Some("connection refused"));
    }

    #[test]
    fn rejected_prompt_releases_running_state() {
        let mut rpc = PiRpc::new();
        let parsed = rpc.parse_line(
            "run-1",
            r#"{"type":"response","command":"prompt","success":false,"error":"busy"}"#,
        );
        assert_eq!(
            parsed.lifecycle,
            Some(LifecycleSignal::TurnFailed(Some("busy".to_string())))
        );
    }

    #[test]
    fn agent_settled_is_turn_completion_boundary() {
        let mut rpc = PiRpc::new();
        let parsed = rpc.parse_line("run-1", r#"{"type":"agent_settled"}"#);
        assert_eq!(parsed.lifecycle, Some(LifecycleSignal::TurnCompleted));
    }

    #[test]
    fn agent_end_is_turn_completion_boundary() {
        let mut rpc = PiRpc::new();
        let parsed = rpc.parse_line("run-1", r#"{"type":"agent_end"}"#);
        assert_eq!(parsed.lifecycle, Some(LifecycleSignal::TurnCompleted));
    }

    #[test]
    fn message_events_reuse_pi_parser() {
        let mut rpc = PiRpc::new();
        let parsed = rpc.parse_line(
            "run-1",
            r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"hello"}}"#,
        );
        assert!(matches!(
            parsed.events.first(),
            Some(BusEvent::MessageDelta { text, .. }) if text == "hello"
        ));
    }

    #[test]
    fn extension_select_registers_pending_and_emits_events() {
        let mut rpc = PiRpc::new();
        let parsed = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","id":"ui-1","method":"select","title":"Pick","options":["A","B"]}"#,
        );
        assert!(matches!(
            parsed.events.first(),
            Some(BusEvent::PiExtensionUiRequestCreated { request, .. })
                if request.id == "ui-1"
                    && request.method == PiExtensionUiMethod::Select
                    && request.options == vec!["A", "B"]
        ));
        assert_eq!(rpc.host().pending_requests().len(), 1);
        assert_eq!(rpc.host().pending_requests()[0].id, "ui-1");
    }

    #[test]
    fn extension_confirm_registers_pending() {
        let mut rpc = PiRpc::new();
        let parsed = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","id":"ui-2","method":"confirm","title":"Continue?","message":"Proceed"}"#,
        );
        assert!(matches!(
            parsed.events.first(),
            Some(BusEvent::PiExtensionUiRequestCreated { request, .. })
                if request.id == "ui-2"
                    && request.method == PiExtensionUiMethod::Confirm
                    && request.title.as_deref() == Some("Continue?")
        ));
        assert_eq!(rpc.host().pending_requests().len(), 1);
    }

    #[test]
    fn extension_input_and_editor_register_pending() {
        let mut rpc = PiRpc::new();
        let _ = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","id":"ui-input","method":"input","title":"Name","placeholder":"Enter name"}"#,
        );
        assert_eq!(rpc.host().pending_requests().len(), 1);
        assert_eq!(
            rpc.host().pending_requests()[0].placeholder.as_deref(),
            Some("Enter name")
        );

        let _ = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","id":"ui-editor","method":"editor","title":"Code","prefill":"fn main() {}"}"#,
        );
        assert_eq!(rpc.host().pending_requests().len(), 2);
    }

    #[test]
    fn extension_duplicate_request_id_updates_in_place() {
        let mut rpc = PiRpc::new();
        let _ = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","id":"ui-dup","method":"input","title":"V1"}"#,
        );
        assert_eq!(rpc.host().pending_requests().len(), 1);

        let _ = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","id":"ui-dup","method":"input","title":"V2"}"#,
        );
        assert_eq!(rpc.host().pending_requests().len(), 1);
        assert_eq!(
            rpc.host().pending_requests()[0].title.as_deref(),
            Some("V2")
        );
    }

    #[test]
    fn extension_notify_emits_notice_event_without_command_output() {
        let mut rpc = PiRpc::new();
        let parsed = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","method":"notify","notifyType":"warning","message":"Disk space low"}"#,
        );
        assert_eq!(parsed.events.len(), 1);
        assert!(matches!(
            &parsed.events[0],
            BusEvent::PiExtensionNotice { notice_type, message, .. }
                if notice_type == "warning" && message == "Disk space low"
        ));
        assert!(parsed
            .events
            .iter()
            .all(|e| !matches!(e, BusEvent::CommandOutput { .. })));
    }

    #[test]
    fn extension_state_updates_manage_host_snapshot() {
        let mut rpc = PiRpc::new();
        let _ = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","method":"setStatus","statusKey":"task","statusText":"Indexing files"}"#,
        );
        let snap = rpc.host_snapshot();
        assert_eq!(
            snap.statuses.get("task").map(String::as_str),
            Some("Indexing files")
        );

        // Clear status when text is empty
        let _ = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","method":"setStatus","statusKey":"task","statusText":""}"#,
        );
        assert!(!rpc.host_snapshot().statuses.contains_key("task"));

        // Set widget
        let _ = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","method":"setWidget","widgetKey":"stats","widgetLines":["line1","line2"]}"#,
        );
        let snap2 = rpc.host_snapshot();
        assert_eq!(snap2.widgets.get("stats").map(|w| w.lines.len()), Some(2));

        // Clear widget when lines empty
        let _ = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","method":"setWidget","widgetKey":"stats","widgetLines":[]}"#,
        );
        assert!(!rpc.host_snapshot().widgets.contains_key("stats"));

        // Set title
        let _ = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","method":"setTitle","title":"My Custom Title"}"#,
        );
        assert_eq!(
            rpc.host_snapshot().title.as_deref(),
            Some("My Custom Title")
        );

        // Set editor text (one-off action)
        let parsed = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","method":"set_editor_text","text":"hello world"}"#,
        );
        assert!(matches!(
            parsed.events.first(),
            Some(BusEvent::PiExtensionEditorAction { text, .. }) if text == "hello world"
        ));
    }

    #[test]
    fn context_usage_status_becomes_structured_event_without_polluting_extension_ui() {
        let mut rpc = PiRpc::new();
        let parsed = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","method":"setStatus","statusKey":"agentcabin-context-usage","statusText":"{\"version\":1,\"usedTokens\":42,\"contextWindow\":128}"}"#,
        );
        assert!(matches!(
            parsed.events.first(),
            Some(BusEvent::PiContextUsage { snapshot, .. })
                if snapshot.get("usedTokens") == Some(&json!(42))
        ));
        assert!(rpc.host_snapshot().statuses.is_empty());
    }

    #[test]
    fn extension_custom_and_unknown_methods_safely_cancelled() {
        let mut rpc = PiRpc::new();
        let parsed = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","id":"custom-1","method":"custom","data":{"foo":"bar"}}"#,
        );
        assert_eq!(parsed.immediate_frames.len(), 1);
        assert_eq!(parsed.immediate_frames[0]["id"], "custom-1");
        assert_eq!(parsed.immediate_frames[0]["cancelled"], true);

        let parsed_unknown = rpc.parse_line(
            "run-1",
            r#"{"type":"extension_ui_request","id":"unknown-1","method":"unknown_tui_method"}"#,
        );
        assert_eq!(parsed_unknown.immediate_frames.len(), 1);
        assert_eq!(parsed_unknown.immediate_frames[0]["id"], "unknown-1");
        assert_eq!(parsed_unknown.immediate_frames[0]["cancelled"], true);
    }

    #[test]
    fn extension_response_unwraps_existing_elicitation_envelope() {
        let mut rpc = PiRpc::new();
        let value = rpc.frame_response(
            PendingKind::Elicitation,
            "ui-3",
            json!({"action":"accept","content":{"value":"A"}}),
        );
        assert_eq!(
            value,
            vec![json!({"type":"extension_ui_response","id":"ui-3","value":"A"})]
        );

        let session_approval = rpc.frame_response(
            PendingKind::Elicitation,
            "ui-session",
            json!({
                "action":"accept",
                "content":{"value":"Yes, for this session"}
            }),
        );
        assert_eq!(
            session_approval,
            vec![json!({
                "type":"extension_ui_response",
                "id":"ui-session",
                "value":"Yes, for this session"
            })]
        );

        let confirmed = rpc.frame_response(
            PendingKind::Elicitation,
            "ui-4",
            json!({"action":"accept","content":{"confirmed":true}}),
        );
        assert_eq!(
            confirmed,
            vec![json!({"type":"extension_ui_response","id":"ui-4","confirmed":true})]
        );

        let declined = rpc.frame_response(
            PendingKind::Elicitation,
            "ui-5",
            json!({"action":"decline"}),
        );
        assert_eq!(
            declined,
            vec![json!({"type":"extension_ui_response","id":"ui-5","cancelled":true})]
        );

        let direct_val = rpc.frame_response(
            PendingKind::Elicitation,
            "ui-6",
            json!({"id":"ui-6","value":"direct content"}),
        );
        assert_eq!(
            direct_val,
            vec![json!({"type":"extension_ui_response","id":"ui-6","value":"direct content"})]
        );
    }

    #[test]
    fn fake_pi_rpc_full_lifecycle_sequence() {
        let mut rpc = PiRpc::new();
        let run_id = "test-run-fake-rpc";

        // 1. Initial agent state frame (response to get_state)
        let state_line = r#"{"type":"response","command":"get_state","success":true,"data":{"sessionId":"s-100","model":{"id":"claude-3-7-sonnet"},"mcpServers":[{"name":"git","status":"connected"}]}}"#;
        let p1 = rpc.parse_line(run_id, state_line);
        assert_eq!(p1.thread_id.as_deref(), Some("s-100"));

        // 2. Turn start (agent_start)
        let turn_start = r#"{"type":"agent_start"}"#;
        let p2 = rpc.parse_line(run_id, turn_start);
        assert!(matches!(
            p2.lifecycle,
            Some(crate::agent::session_protocol::LifecycleSignal::TurnStarted)
        ));

        // 3. Thinking block & Text delta
        let thinking_line = r#"{"type":"thinking","text":"Analyzing codebase..."}"#;
        let p3 = rpc.parse_line(run_id, thinking_line);
        assert!(!p3.events.is_empty());

        let text_line = r#"{"type":"text","text":"Hello world"}"#;
        let p4 = rpc.parse_line(run_id, text_line);
        assert!(!p4.events.is_empty());

        // 4. Tool execution
        let tool_start = r#"{"type":"tool_call_start","id":"call-1","name":"read_file","input":{"path":"main.rs"}}"#;
        let p5 = rpc.parse_line(run_id, tool_start);
        assert!(!p5.events.is_empty());

        // 5. Turn completed (agent_settled)
        let turn_end = r#"{"type":"agent_settled"}"#;
        let p6 = rpc.parse_line(run_id, turn_end);
        assert!(matches!(
            p6.lifecycle,
            Some(crate::agent::session_protocol::LifecycleSignal::TurnCompleted)
        ));

        // 6. Frame generation tests
        let compact_frame = rpc.frame_compact();
        assert_eq!(compact_frame["type"], "compact");

        let clone_frame = rpc.frame_clone();
        assert_eq!(clone_frame["type"], "clone");

        let fork_frame = rpc.frame_fork("msg-entry-1");
        assert_eq!(fork_frame["type"], "fork");
        assert_eq!(fork_frame["entryId"], "msg-entry-1");
    }

    #[test]
    fn frame_response_retains_pending_request_until_explicitly_removed() {
        let mut rpc = PiRpc::new();
        let run_id = "test-run-pending-retained";
        let req_line = r#"{"type":"extension_ui_request","id":"ui-pending-1","method":"confirm","title":"Delete file?"}"#;
        rpc.parse_line(run_id, req_line);
        assert_eq!(rpc.host().pending_requests().len(), 1);

        // Framing response frame should not delete from pending store
        let frames = rpc.frame_response(
            PendingKind::Elicitation,
            "ui-pending-1",
            json!({"answers":{"confirmed":"true"}}),
        );
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["type"], "extension_ui_response");
        assert_eq!(rpc.host().pending_requests().len(), 1);

        // Explicit removal (which actor loop executes after successful stdin write) removes it
        rpc.host_mut().remove_pending_request("ui-pending-1");
        assert_eq!(rpc.host().pending_requests().len(), 0);
    }

    #[test]
    fn host_snapshot_captures_all_components() {
        let mut rpc = PiRpc::new();
        let run_id = "test-run-snapshot";
        rpc.parse_line(
            run_id,
            r#"{"type":"extension_ui_request","method":"setTitle","title":"My Project"}"#,
        );
        rpc.parse_line(run_id, r#"{"type":"extension_ui_request","method":"setStatus","statusKey":"build","statusText":"Passing"}"#);
        rpc.parse_line(run_id, r#"{"type":"extension_ui_request","method":"setWidget","widgetKey":"summary","widgetLines":["All clean"]}"#);
        rpc.parse_line(run_id, r#"{"type":"extension_ui_request","id":"req-snap-1","method":"input","message":"Enter API key"}"#);

        let snap = rpc.host_snapshot();
        assert_eq!(snap.title.as_deref(), Some("My Project"));
        assert_eq!(
            snap.statuses.get("build").map(String::as_str),
            Some("Passing")
        );
        assert_eq!(
            snap.widgets.get("summary").map(|w| w.lines.clone()),
            Some(vec!["All clean".to_string()])
        );
        assert_eq!(snap.pending_requests.len(), 1);
        assert_eq!(snap.pending_requests[0].id, "req-snap-1");
    }

    #[test]
    fn slash_command_produces_prompt_type_message() {
        let mut rpc = PiRpc::new();
        let frames = rpc.frame_user_turn("/fix-tests args1", &[], &[], &Default::default());
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["type"], "prompt");
        assert_eq!(frames[0]["message"], "/fix-tests args1");
    }

    #[test]
    fn stop_reason_length_is_captured_from_usage_update_even_without_text() {
        let mut rpc = PiRpc::new();

        // agent_start clears any stale stop_reason
        rpc.parse_line("run-1", r#"{"type":"agent_start"}"#);
        assert_eq!(rpc.last_turn_stop_reason(), None);

        // A thinking-only turn truncated by length: no visible text, but the
        // usage update still carries stop_reason="length".
        let message_end = r#"{"type":"message_end","message":{"role":"assistant","content":[],"stopReason":"length","usage":{"input":72798,"output":16384}}}"#;
        rpc.parse_line("run-1", message_end);
        assert_eq!(rpc.last_turn_stop_reason(), Some("length"));
    }

    #[test]
    fn stop_reason_is_reset_on_next_agent_start() {
        let mut rpc = PiRpc::new();
        let message_end = r#"{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"hi"}],"stopReason":"length","usage":{"input":100,"output":16384}}}"#;
        rpc.parse_line("run-1", message_end);
        assert_eq!(rpc.last_turn_stop_reason(), Some("length"));

        // Next turn starts: stale stop_reason must be cleared.
        rpc.parse_line("run-1", r#"{"type":"agent_start"}"#);
        assert_eq!(rpc.last_turn_stop_reason(), None);
    }

    #[test]
    fn stop_reason_end_turn_does_not_trigger_length_continuation() {
        let mut rpc = PiRpc::new();
        let message_end = r#"{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"done"}],"stopReason":"end_turn","usage":{"input":100,"output":50}}}"#;
        rpc.parse_line("run-1", message_end);
        assert_eq!(rpc.last_turn_stop_reason(), Some("end_turn"));
    }
}
