use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::models::BusEvent;
use crate::storage::events::EventWriter;
use tauri::Emitter;

/// Message envelope for broadcast channels.
#[derive(Debug, Clone)]
pub struct BroadcastMsg {
    /// Event name (e.g. "bus-event", "chat-delta", "hook-event")
    pub event_name: String,
    /// Serialized payload
    pub payload: Value,
    /// For A-class events: sequence number from EventWriter. None for B-class.
    pub seq: Option<u64>,
    /// Optional run_id for run-scoped event filtering
    pub run_id: Option<String>,
}

/// Dual-channel broadcaster: A-class (reliable, replayable) + B-class (lossy, realtime).
#[derive(Clone)]
pub struct EventBroadcaster {
    /// A-class channel: reliable, large capacity, for replayable bus-events
    a_tx: broadcast::Sender<BroadcastMsg>,
    /// B-class channel: lossy, medium capacity, for realtime streams (chat/run-event/hook)
    b_tx: broadcast::Sender<BroadcastMsg>,
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBroadcaster {
    pub fn new() -> Self {
        let (a_tx, _) = broadcast::channel(8192);
        let (b_tx, _) = broadcast::channel(1024);
        log::debug!("[broadcaster] created: A-channel=8192, B-channel=1024");
        Self { a_tx, b_tx }
    }

    /// Subscribe to A-class (replayable) events
    pub fn subscribe_a(&self) -> broadcast::Receiver<BroadcastMsg> {
        self.a_tx.subscribe()
    }

    /// Subscribe to B-class (realtime) events
    pub fn subscribe_b(&self) -> broadcast::Receiver<BroadcastMsg> {
        self.b_tx.subscribe()
    }

    /// Send an A-class event (bus-event with seq)
    pub fn send_a(&self, msg: BroadcastMsg) {
        let _ = self.a_tx.send(msg);
    }

    /// Send a B-class event (realtime, no seq)
    pub fn send_b(&self, msg: BroadcastMsg) {
        let _ = self.b_tx.send(msg);
    }
}

/// Unified emitter that replaces all direct `app.emit()` calls.
/// Handles: persist (A-class) + Tauri emit + broadcast to WS clients.
pub struct BroadcastEmitter {
    writer: Arc<EventWriter>,
    app: Option<tauri::AppHandle>,
    broadcaster: EventBroadcaster,
}

use std::sync::OnceLock;

static SHARED_EMITTER: OnceLock<Arc<BroadcastEmitter>> = OnceLock::new();

pub fn register_shared_emitter(emitter: Arc<BroadcastEmitter>) {
    let _ = SHARED_EMITTER.set(emitter);
}

pub fn shared_emitter() -> Option<Arc<BroadcastEmitter>> {
    SHARED_EMITTER.get().cloned()
}

impl BroadcastEmitter {
    pub fn new(
        writer: Arc<EventWriter>,
        app: tauri::AppHandle,
        broadcaster: EventBroadcaster,
    ) -> Self {
        log::debug!("[emitter] BroadcastEmitter created");
        Self {
            writer,
            app: Some(app),
            broadcaster,
        }
    }

    /// Headless constructor — no Tauri AppHandle; events flow through the
    /// broadcaster channels only (consumed by core-server SSE / WS clients).
    pub fn new_headless(writer: Arc<EventWriter>, broadcaster: EventBroadcaster) -> Self {
        log::debug!("[emitter] BroadcastEmitter created (headless)");
        Self {
            writer,
            app: None,
            broadcaster,
        }
    }

    #[cfg(test)]
    pub fn mock() -> Arc<Self> {
        let writer = Arc::new(crate::storage::events::EventWriter::new());
        let broadcaster = EventBroadcaster::new();
        Arc::new(Self {
            writer,
            app: None,
            broadcaster,
        })
    }

    /// Emit a bus event, persisting only the durable subset used for history replay.
    ///
    /// Transient Pi snapshots (for example `pi_session_entries`) can be tens of KB
    /// and are refreshed repeatedly. They still go to the live Tauri/WS channels,
    /// but must not inflate `events.jsonl` or consume the A-class replay log.
    pub fn persist_and_emit(&self, run_id: &str, event: &BusEvent) {
        let ts = crate::models::now_iso();
        let payload = match serde_json::to_value(event) {
            Ok(v) => v,
            Err(e) => {
                log::error!("[emitter] serialize bus-event failed: {}", e);
                return;
            }
        };

        let seq = if crate::storage::events::is_replayable_value(&payload) {
            match self.writer.write_bus_event_with_ts(run_id, event, &ts) {
                Ok(seq) => Some(seq),
                Err(e) => {
                    log::warn!("[emitter] persist failed for run_id={}: {}", run_id, e);
                    None
                }
            }
        } else {
            None
        };

        let mut live_payload = payload.clone();
        if let Some(seq) = seq {
            if let Some(obj) = live_payload.as_object_mut() {
                // Tauri live events must carry the same durable checkpoint as
                // WebSocket events so replay/live overlap can be deduplicated.
                obj.insert("_seq".to_string(), serde_json::Value::Number(seq.into()));
            }
            log::trace!(
                "[emitter] persist_and_emit: run_id={}, seq={}, type={:?}",
                run_id,
                seq,
                event_type_name(event)
            );
        }

        if let Some(app) = &self.app {
            let _ = app.emit("bus-event", &live_payload);
            // The pet consumes the same normalized AgentCabin run-state event. Keeping
            // this beside the canonical emitter means it works independently of the
            // currently mounted main-window route and of any specific agent protocol.
            crate::pet::handle_bus_event(app, event);
        }

        if let Some(seq) = seq {
            self.broadcaster.send_a(BroadcastMsg {
                event_name: "bus-event".to_string(),
                payload,
                seq: Some(seq),
                run_id: Some(run_id.to_string()),
            });
        } else {
            // Keep live WS clients updated for transient events or when durable
            // storage fails.
            self.broadcaster.send_b(BroadcastMsg {
                event_name: "bus-event".to_string(),
                payload,
                seq: None,
                run_id: Some(run_id.to_string()),
            });
        }
    }

    /// B-class: Tauri emit + broadcast (no persist, no seq).
    /// For realtime streams: chat-delta, chat-done, run-event, hook-event, etc.
    pub fn emit_realtime<T: Serialize + Clone>(
        &self,
        event_name: &str,
        payload: &T,
        run_id: Option<&str>,
    ) {
        log::trace!(
            "[emitter] emit_realtime: event={}, run_id={:?}",
            event_name,
            run_id
        );
        if let Some(app) = &self.app {
            let _ = app.emit(event_name, payload);
        }
        let value = match serde_json::to_value(payload) {
            Ok(v) => v,
            Err(e) => {
                log::error!("[emitter] serialize {} failed: {}", event_name, e);
                return;
            }
        };
        self.broadcaster.send_b(BroadcastMsg {
            event_name: event_name.to_string(),
            payload: value,
            seq: None,
            run_id: run_id.map(|s| s.to_string()),
        });
    }

    /// Get a reference to the inner EventWriter (for direct reads like list_bus_events)
    pub fn writer(&self) -> &EventWriter {
        &self.writer
    }

    /// Get a reference to the inner EventBroadcaster (for WS subscriptions)
    pub fn broadcaster(&self) -> &EventBroadcaster {
        &self.broadcaster
    }

    /// Get an optional reference to the AppHandle (None in mock/test mode)
    pub fn app_opt(&self) -> Option<&tauri::AppHandle> {
        self.app.as_ref()
    }

    /// Get a reference to the AppHandle
    pub fn app(&self) -> &tauri::AppHandle {
        self.app
            .as_ref()
            .expect("AppHandle is not available in mock/test mode")
    }
}

/// Extract event type name for logging
fn event_type_name(event: &BusEvent) -> &'static str {
    match event {
        BusEvent::SessionInit { .. } => "session_init",
        BusEvent::AgentModeUpdate { .. } => "agent_mode_update",
        BusEvent::MessageDelta { .. } => "message_delta",
        BusEvent::MessageComplete { .. } => "message_complete",
        BusEvent::UserMessage { .. } => "user_message",
        BusEvent::ToolStart { .. } => "tool_start",
        BusEvent::ToolEnd { .. } => "tool_end",
        BusEvent::StructuredTaskState { .. } => "structured_task_state",
        BusEvent::WorkTaskState { .. } => "work_task_state",
        BusEvent::WorkContextPlanUpdated { .. } => "work_context_plan_updated",
        BusEvent::WorkProjectionChanged { .. } => "work_projection_changed",
        BusEvent::RunState { .. } => "run_state",
        BusEvent::UsageUpdate { .. } => "usage_update",
        BusEvent::ThinkingDelta { .. } => "thinking_delta",
        BusEvent::ToolInputDelta { .. } => "tool_input_delta",
        BusEvent::PermissionDenied { .. } => "permission_denied",
        BusEvent::PermissionPrompt { .. } => "permission_prompt",
        BusEvent::CompactBoundary { .. } => "compact_boundary",
        BusEvent::AgentHandoff { .. } => "agent_handoff",
        BusEvent::SystemStatus { .. } => "system_status",
        BusEvent::PiExtensionUi { .. } => "pi_extension_ui",
        BusEvent::PiContextUsage { .. } => "pi_context_usage",
        BusEvent::PiExtensionUiRequestCreated { .. } => "pi_extension_ui_request_created",
        BusEvent::PiExtensionUiRequestResolved { .. } => "pi_extension_ui_request_resolved",
        BusEvent::PiExtensionNotice { .. } => "pi_extension_notice",
        BusEvent::PiExtensionStateSync { .. } => "pi_extension_state_sync",
        BusEvent::PiExtensionEditorAction { .. } => "pi_extension_editor_action",
        BusEvent::PiFeatureCapabilities { .. } => "pi_feature_capabilities",
        BusEvent::PiPlanState { .. } => "pi_plan_state",
        BusEvent::PiGoalState { .. } => "pi_goal_state",
        BusEvent::PiTodoState { .. } => "pi_todo_state",
        BusEvent::PiPermissionState { .. } => "pi_permission_state",
        BusEvent::PiQueueUpdated { .. } => "pi_queue_updated",
        BusEvent::PiSessionEntries { .. } => "pi_session_entries",
        BusEvent::PiSessionTree { .. } => "pi_session_tree",

        BusEvent::AuthStatus { .. } => "auth_status",
        BusEvent::HookStarted { .. } => "hook_started",
        BusEvent::HookProgress { .. } => "hook_progress",
        BusEvent::HookResponse { .. } => "hook_response",
        BusEvent::HookCallback { .. } => "hook_callback",
        BusEvent::TaskNotification { .. } => "task_notification",
        BusEvent::ToolProgress { .. } => "tool_progress",
        BusEvent::ToolOutputDelta { .. } => "tool_output_delta",
        BusEvent::GoalUpdate { .. } => "goal_update",
        BusEvent::ToolUseSummary { .. } => "tool_use_summary",
        BusEvent::FilesPersisted { .. } => "files_persisted",
        BusEvent::ControlCancelled { .. } => "control_cancelled",
        BusEvent::CommandOutput { .. } => "command_output",
        BusEvent::ElicitationPrompt { .. } => "elicitation_prompt",
        BusEvent::RateLimitEvent { .. } => "rate_limit_event",
        BusEvent::RalphStarted { .. } => "ralph_started",
        BusEvent::RalphIteration { .. } => "ralph_iteration",
        BusEvent::RalphComplete { .. } => "ralph_complete",
        BusEvent::CodexHookRun { .. } => "codex_hook_run",
        BusEvent::CodexMcpStatus { .. } => "codex_mcp_status",
        BusEvent::CodexTurnDiff { .. } => "codex_turn_diff",
        BusEvent::TurnFileSummary { .. } => "turn_file_summary",
        BusEvent::Raw { .. } => "raw",
    }
}
