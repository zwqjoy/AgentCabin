use super::attachments::prompt_content;
use super::capabilities::{apply_model_capabilities, from_initialize};
use super::commands::parse_available_commands;
use super::fork;
use super::mcp::parse_servers;
use super::models::parse_model_state;
use super::modes::{
    parse_available_modes_update, parse_current_mode_update, parse_session_modes, supports_plan,
};
use super::todos::parse_plan;
use crate::agent::adapter::ActorSessionMap;
use crate::agent::session_actor::{ActorCommand, AttachmentData};
use crate::models::{
    AgentCapabilities, AgentSessionMode, AttachmentMeta, BusEvent, CliModelInfo,
    GlobalProviderModel, McpServerInfo, RunStatus,
};
use crate::storage;
use crate::web_server::broadcaster::BroadcastEmitter;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct QueuedPrompt {
    text: String,
    attachments: Vec<AttachmentData>,
    reply: oneshot::Sender<Result<(), String>>,
}

struct PermissionRequest {
    wire_id: Value,
    options: Vec<Value>,
}

struct UserQuestionRequest {
    wire_id: Value,
    questions: Vec<Value>,
}

enum PendingRequest {
    Initialize,
    Authenticate,
    NewSession,
    LoadSession,
    Prompt,
    SetModel {
        model: String,
        reply: oneshot::Sender<Value>,
    },
    SetEffort {
        reply: oneshot::Sender<Value>,
    },
    SetMode {
        mode: String,
        reply: oneshot::Sender<Value>,
    },
    Fork {
        reply: oneshot::Sender<Value>,
    },
    Interject {
        text: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
}

struct Actor {
    emitter: Arc<BroadcastEmitter>,
    sessions: ActorSessionMap,
    run_id: String,
    cwd: String,
    grok_home: Option<PathBuf>,
    custom_provider_active: bool,
    custom_model_capabilities: Option<GlobalProviderModel>,
    model: Option<String>,
    permission_mode: String,
    resume_session_id: Option<String>,
    continuation_context: Option<String>,
    plugin_dirs: Vec<String>,
    capabilities: AgentCapabilities,
    slash_commands: Vec<Value>,
    commands_loaded: bool,
    current_mode_id: Option<String>,
    available_modes: Vec<AgentSessionMode>,
    model_options: Vec<CliModelInfo>,
    mcp_servers: Vec<McpServerInfo>,
    tag: Arc<()>,
    child: Child,
    stdin: ChildStdin,
    cmd_rx: mpsc::Receiver<ActorCommand>,
    cancel: CancellationToken,
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Code desktop bridge token owned by this actor process.
    desktop_runtime_token: Option<String>,
    next_id: u64,
    pending: HashMap<String, PendingRequest>,
    permissions: HashMap<String, PermissionRequest>,
    user_questions: HashMap<String, UserQuestionRequest>,
    tool_names: HashMap<String, String>,
    queue: VecDeque<QueuedPrompt>,
    session_id: Option<String>,
    ready: bool,
    prompt_active: bool,
    assistant_text: String,
    persisted_updates_offset: u64,
    usage_emitted_this_prompt: bool,
    explicitly_stopped: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_actor(
    emitter: Arc<BroadcastEmitter>,
    sessions: ActorSessionMap,
    run_id: String,
    cwd: String,
    grok_home: Option<PathBuf>,
    custom_provider_active: bool,
    custom_model_capabilities: Option<GlobalProviderModel>,
    model: Option<String>,
    permission_mode: String,
    resume_session_id: Option<String>,
    continuation_context: Option<String>,
    plugin_dirs: Vec<String>,
    tag: Arc<()>,
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
    cmd_rx: mpsc::Receiver<ActorCommand>,
    cancel: CancellationToken,
    shutdown_tx: oneshot::Sender<()>,
    desktop_runtime_token: Option<String>,
) {
    let mut actor = Actor {
        emitter,
        sessions,
        run_id,
        cwd,
        grok_home,
        custom_provider_active,
        custom_model_capabilities,
        model,
        permission_mode,
        resume_session_id,
        continuation_context,
        plugin_dirs,
        capabilities: AgentCapabilities::default(),
        slash_commands: Vec::new(),
        commands_loaded: false,
        current_mode_id: None,
        available_modes: Vec::new(),
        model_options: Vec::new(),
        mcp_servers: Vec::new(),
        tag,
        child,
        stdin,
        cmd_rx,
        cancel,
        shutdown_tx: Some(shutdown_tx),
        desktop_runtime_token,
        next_id: 1,
        pending: HashMap::new(),
        permissions: HashMap::new(),
        user_questions: HashMap::new(),
        tool_names: HashMap::new(),
        queue: VecDeque::new(),
        session_id: None,
        ready: false,
        prompt_active: false,
        assistant_text: String::new(),
        persisted_updates_offset: 0,
        usage_emitted_this_prompt: false,
        explicitly_stopped: false,
    };

    if let Err(error) = actor.initialize().await {
        actor.fail(error);
        let _ = actor.child.start_kill();
        let _ = actor.child.wait().await;
        actor.cleanup().await;
        return;
    }

    let cancel_signal = actor.cancel.clone();
    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();
    let mut stderr_open = true;

    loop {
        tokio::select! {
            command = actor.cmd_rx.recv() => {
                match command {
                    Some(command) => {
                        if actor.handle_command(command).await {
                            break;
                        }
                    }
                    None => {
                        actor.explicitly_stopped = true;
                        actor.shutdown().await;
                        break;
                    }
                }
            }
            result = stdout_lines.next_line() => {
                match result {
                    Ok(Some(line)) => actor.handle_line(&line).await,
                    Ok(None) => break,
                    Err(error) => {
                        actor.command_output(format!("[grok acp] stdout read failed: {error}"));
                        break;
                    }
                }
            }
            result = stderr_lines.next_line(), if stderr_open => {
                match result {
                    Ok(Some(line)) if !line.trim().is_empty() => {
                        if should_surface_stderr(&line, actor.custom_provider_active) {
                            actor.command_output(format!("[grok acp stderr] {line}"));
                        } else {
                            log::warn!("[grok acp stderr] {line}");
                        }
                    }
                    Ok(Some(_)) => {}
                    Ok(None) | Err(_) => stderr_open = false,
                }
            }
            _ = cancel_signal.cancelled() => {
                actor.explicitly_stopped = true;
                actor.shutdown().await;
                break;
            }
        }
    }

    if !actor.explicitly_stopped {
        let status = actor.child.wait().await.ok();
        let code = status.as_ref().and_then(|value| value.code());
        actor.fail(format!(
            "Grok Build ACP process exited unexpectedly (code {code:?})"
        ));
    }
    actor.reject_queue("Grok ACP session ended");
    actor.cleanup().await;
}

impl Actor {
    async fn initialize(&mut self) -> Result<(), String> {
        let id = self.request_id();
        self.pending.insert(key(&id), PendingRequest::Initialize);
        self.write(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": initialize_params()
        }))
        .await
    }

    async fn handle_command(&mut self, command: ActorCommand) -> bool {
        match command {
            ActorCommand::SendMessage {
                text,
                attachments,
                skills,
                work_context_plan,
                reply,
            } => {
                let _ = work_context_plan;
                if !skills.is_empty() {
                    let _ = reply.send(Err(
                        "Structured skill references are not supported by the Grok ACP MVP"
                            .to_string(),
                    ));
                } else {
                    self.queue.push_back(QueuedPrompt {
                        text,
                        attachments,
                        reply,
                    });
                    self.dispatch().await;
                }
            }
            ActorCommand::SteerMessage {
                text,
                attachments,
                reply,
            } => {
                self.interject(text, attachments, reply).await;
            }
            ActorCommand::CancelTurn { reply } => {
                let _ = reply.send(Err(
                    "Turn cancellation without ending the session is not available for Grok sessions"
                        .to_string(),
                ));
            }
            ActorCommand::SendControl { request, reply } => {
                self.control(request, reply).await;
            }
            ActorCommand::Stop { reason: _, reply } => {
                self.explicitly_stopped = true;
                self.shutdown().await;
                let _ = reply.send(Ok(()));
                return true;
            }
            ActorCommand::RespondPermission {
                request_id,
                response,
                reply,
            } => {
                let result = self.respond_permission(&request_id, &response).await;
                let _ = reply.send(result);
            }
            ActorCommand::RespondUserInput {
                request_id,
                response,
                reply,
            } => {
                let result = self.respond_user_question(&request_id, &response).await;
                let _ = reply.send(result);
            }
            ActorCommand::CancelControlRequest { reply, .. }
            | ActorCommand::RespondHookCallback { reply, .. }
            | ActorCommand::RespondElicitation { reply, .. } => {
                let _ = reply.send(Err(
                    "This interactive response type is not supported by the Grok ACP MVP"
                        .to_string(),
                ));
            }
            ActorCommand::StartRalphLoop { reply, .. } => {
                let _ = reply.send(Err(
                    "Ralph loop is not supported by the Grok ACP MVP".to_string()
                ));
            }
            ActorCommand::CancelRalphLoop { reply } => {
                let _ = reply.send(Err("No active Grok Ralph loop".to_string()));
            }
        }
        false
    }

    async fn control(
        &mut self,
        request: Value,
        reply: oneshot::Sender<Result<(String, oneshot::Receiver<Value>), String>>,
    ) {
        let subtype = request.get("subtype").and_then(Value::as_str).unwrap_or("");
        match subtype {
            "interrupt" | "abort" => {
                let result = self.cancel_turn().await;
                if result.is_ok() && self.prompt_active {
                    // ACP cancellation does not guarantee a follow-up response for the
                    // original session/prompt request. Settle that request locally so
                    // the next prompt is not blocked behind a stale active turn.
                    self.pending
                        .retain(|_, pending| !matches!(pending, PendingRequest::Prompt));
                    self.settle_prompt(None);
                }
                let (tx, rx) = oneshot::channel();
                if result.is_ok() {
                    let _ = tx.send(json!({"ok": true}));
                }
                let _ = reply.send(result.map(|_| ("grok_cancel".to_string(), rx)));
            }
            "set_model" => {
                if !self.capabilities.protocol.session_set_model {
                    let _ = reply.send(Err(
                        "Grok ACP did not advertise session/set_model support".to_string()
                    ));
                    return;
                }
                let Some(session_id) = self.session_id.clone() else {
                    let _ = reply.send(Err("Grok ACP session is not ready".to_string()));
                    return;
                };
                let Some(model) = request
                    .get("model")
                    .or_else(|| request.get("modelId"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    let _ = reply.send(Err("Grok set_model requires a model".to_string()));
                    return;
                };
                let model = model.to_string();
                let id = self.request_id();
                let request_id = key(&id);
                let (tx, rx) = oneshot::channel();
                self.pending.insert(
                    request_id.clone(),
                    PendingRequest::SetModel {
                        model: model.clone(),
                        reply: tx,
                    },
                );
                let frame = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": "session/set_model",
                    "params": {"sessionId": session_id, "modelId": model}
                });
                match self.write(&frame).await {
                    Ok(()) => {
                        let _ = reply.send(Ok((request_id, rx)));
                    }
                    Err(error) => {
                        self.pending.remove(&request_id);
                        let _ = reply.send(Err(error));
                    }
                }
            }
            "set_effort" => {
                if !self.capabilities.protocol.effort_control {
                    let _ = reply.send(Err(
                        "Grok ACP did not advertise reasoning effort support for this model"
                            .to_string(),
                    ));
                    return;
                }
                let Some(session_id) = self.session_id.clone() else {
                    let _ = reply.send(Err("Grok ACP session is not ready".to_string()));
                    return;
                };
                let effort = request
                    .get("effort")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !effort.is_empty() && !self.effort_is_supported(&effort) {
                    let _ = reply.send(Err(format!(
                        "Grok model '{}' does not advertise reasoning effort '{}'",
                        self.model.as_deref().unwrap_or("unknown"),
                        effort
                    )));
                    return;
                }
                let Some(model) = self.model.clone() else {
                    let _ =
                        reply.send(Err("Grok set_effort requires a selected model".to_string()));
                    return;
                };
                let id = self.request_id();
                let request_id = key(&id);
                let (tx, rx) = oneshot::channel();
                self.pending
                    .insert(request_id.clone(), PendingRequest::SetEffort { reply: tx });
                let frame = effort_request_frame(id, &session_id, &model, &effort);
                match self.write(&frame).await {
                    Ok(()) => {
                        let _ = reply.send(Ok((request_id, rx)));
                    }
                    Err(error) => {
                        self.pending.remove(&request_id);
                        let _ = reply.send(Err(error));
                    }
                }
            }
            "set_session_mode" => {
                let Some(session_id) = self.session_id.clone() else {
                    let _ = reply.send(Err("Grok ACP session is not ready".to_string()));
                    return;
                };
                let Some(mode) = request
                    .get("mode")
                    .or_else(|| request.get("modeId"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    let _ = reply.send(Err("Grok set_session_mode requires a mode".to_string()));
                    return;
                };
                if self.available_modes.is_empty() {
                    let _ = reply.send(Err("Grok ACP did not advertise session modes".to_string()));
                    return;
                }
                if !self
                    .available_modes
                    .iter()
                    .any(|candidate| candidate.id.as_str() == mode)
                {
                    let _ = reply.send(Err(format!(
                        "Grok ACP did not advertise session mode '{mode}'"
                    )));
                    return;
                }
                let mode = mode.to_string();
                let id = self.request_id();
                let request_id = key(&id);
                let (tx, rx) = oneshot::channel();
                self.pending.insert(
                    request_id.clone(),
                    PendingRequest::SetMode {
                        mode: mode.clone(),
                        reply: tx,
                    },
                );
                let frame = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": "session/set_mode",
                    "params": {"sessionId": session_id, "modeId": mode}
                });
                match self.write(&frame).await {
                    Ok(()) => {
                        let _ = reply.send(Ok((request_id, rx)));
                    }
                    Err(error) => {
                        self.pending.remove(&request_id);
                        let _ = reply.send(Err(error));
                    }
                }
            }
            "fork" => {
                if !self.capabilities.runtime.fork {
                    let _ = reply.send(Err(
                        "Grok ACP did not advertise the structured fork extension".to_string(),
                    ));
                    return;
                }
                if self.prompt_active {
                    let _ = reply.send(Err(
                        "Wait for the active Grok turn to finish before forking".to_string(),
                    ));
                    return;
                }
                let Some(session_id) = self.session_id.clone() else {
                    let _ = reply.send(Err("Grok ACP session is not ready".to_string()));
                    return;
                };
                let new_cwd = request
                    .get("new_cwd")
                    .or_else(|| request.get("newCwd"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(&self.cwd)
                    .to_string();
                let id = self.request_id();
                let request_id = key(&id);
                let (tx, rx) = oneshot::channel();
                self.pending
                    .insert(request_id.clone(), PendingRequest::Fork { reply: tx });
                let frame = fork::request_frame(id, &session_id, &self.cwd, &new_cwd);
                match self.write(&frame).await {
                    Ok(()) => {
                        let _ = reply.send(Ok((request_id, rx)));
                    }
                    Err(error) => {
                        self.pending.remove(&request_id);
                        let _ = reply.send(Err(error));
                    }
                }
            }
            "is_alive" => {
                let (tx, rx) = oneshot::channel();
                let _ = tx.send(json!({"ok": true, "alive": true}));
                let _ = reply.send(Ok(("grok_alive".to_string(), rx)));
            }
            _ => {
                let _ = reply.send(Err(format!(
                    "Unsupported Grok ACP control subtype '{subtype}'"
                )));
            }
        }
    }

    async fn interject(
        &mut self,
        text: String,
        attachments: Vec<AttachmentData>,
        reply: oneshot::Sender<Result<(), String>>,
    ) {
        if !self.capabilities.runtime.steer {
            let _ = reply.send(Err(
                "Grok ACP did not advertise the native interject extension".to_string(),
            ));
            return;
        }
        if !attachments.is_empty() {
            let _ = reply.send(Err(
                "Grok native interject accepts text only; queue attachments as a follow-up"
                    .to_string(),
            ));
            return;
        }
        if !self.prompt_active {
            let _ = reply.send(Err("No active Grok turn to steer".to_string()));
            return;
        }
        let Some(session_id) = self.session_id.clone() else {
            let _ = reply.send(Err("Grok ACP session is not ready".to_string()));
            return;
        };
        let text = text.trim().to_string();
        if text.is_empty() {
            let _ = reply.send(Err("Grok interject requires text".to_string()));
            return;
        }
        let id = self.request_id();
        let request_id = key(&id);
        self.pending.insert(
            request_id.clone(),
            PendingRequest::Interject {
                text: text.clone(),
                reply,
            },
        );
        if let Err(error) = self
            .write(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "_x.ai/interject",
                "params": {"sessionId": session_id, "text": text}
            }))
            .await
        {
            if let Some(PendingRequest::Interject { reply, .. }) = self.pending.remove(&request_id)
            {
                let _ = reply.send(Err(error));
            }
        }
    }

    fn effort_is_supported(&self, effort: &str) -> bool {
        self.model
            .as_deref()
            .and_then(|model| {
                self.model_options
                    .iter()
                    .find(|candidate| candidate.value == model)
            })
            .and_then(|model| model.supported_effort_levels.as_ref())
            .is_none_or(|levels| levels.iter().any(|level| level == effort))
    }

    async fn handle_line(&mut self, line: &str) {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            self.command_output(format!("[grok acp raw] {line}"));
            return;
        };
        if let Some(method) = message.get("method").and_then(Value::as_str) {
            if message.get("id").is_some() {
                self.server_request(method, &message).await;
            } else if is_session_update_method(method) {
                self.session_update(&message);
            } else if method == "_x.ai/mcp/servers_updated" || method == "x.ai/mcp/server_status" {
                self.apply_mcp_update(message.get("params"));
            } else if method == "x.ai/models/updated" {
                self.apply_model_update(message.get("params"));
            } else {
                self.raw("grok_acp", message);
            }
        } else if let Some(id) = message.get("id") {
            self.response(id, &message).await;
        }
    }

    async fn response(&mut self, id: &Value, message: &Value) {
        let Some(pending) = self.pending.remove(&key(id)) else {
            return;
        };
        if let Some(error) = message.get("error") {
            let error = rpc_error(error);
            match pending {
                PendingRequest::SetModel { reply, .. } => {
                    let _ = reply.send(json!({"error": error}));
                }
                PendingRequest::SetEffort { reply, .. } => {
                    let _ = reply.send(json!({"error": error}));
                }
                PendingRequest::SetMode { reply, .. } => {
                    let _ = reply.send(json!({"error": error}));
                }
                PendingRequest::Fork { reply } => {
                    let _ = reply.send(json!({"error": error}));
                }
                PendingRequest::Interject { reply, .. } => {
                    let _ = reply.send(Err(error));
                }
                PendingRequest::Prompt => self.finish_prompt(Some(error)).await,
                _ => self.fail_startup(error).await,
            }
            return;
        }

        let result = message.get("result").cloned().unwrap_or_else(|| json!({}));
        match pending {
            PendingRequest::Initialize => self.after_initialize(&result).await,
            PendingRequest::Authenticate => self.start_session().await,
            PendingRequest::NewSession | PendingRequest::LoadSession => {
                self.session_ready(&result).await
            }
            PendingRequest::Prompt => self.finish_prompt(None).await,
            PendingRequest::SetModel { model, reply } => {
                self.model = Some(model.clone());
                let _ = storage::runs::update_run_model(&self.run_id, &model);
                self.apply_model_capabilities();
                if self.ready {
                    self.emit_session_init();
                }
                let _ = reply.send(result);
            }
            PendingRequest::SetEffort { reply, .. } => {
                let _ = reply.send(result);
            }
            PendingRequest::SetMode { mode, reply } => {
                self.current_mode_id = Some(mode);
                self.emit_agent_mode_update();
                let _ = reply.send(result);
            }
            PendingRequest::Fork { reply } => {
                let _ = reply.send(result);
            }
            PendingRequest::Interject { text, reply } => {
                self.user_message(&text, &[]);
                let _ = reply.send(Ok(()));
            }
        }
    }

    async fn after_initialize(&mut self, result: &Value) {
        self.capabilities = from_initialize(result);
        let (current_model, model_options) = parse_model_state(result);
        if self.model.is_none() {
            self.model = current_model;
        }
        self.model_options = model_options;
        self.apply_model_capabilities();
        let methods = result
            .get("authMethods")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let has_key = std::env::var("XAI_API_KEY")
            .ok()
            .is_some_and(|value| !value.trim().is_empty());
        let selected = authentication_method(&methods, has_key).map(str::to_string);

        let Some(method_id) = selected else {
            self.start_session().await;
            return;
        };
        let id = self.request_id();
        self.pending.insert(key(&id), PendingRequest::Authenticate);
        if let Err(error) = self
            .write(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "authenticate",
                "params": {"methodId": method_id}
            }))
            .await
        {
            self.fail_startup(error).await;
        }
    }

    async fn start_session(&mut self) {
        let id = self.request_id();
        let (method, pending, mut params) = match self
            .resume_session_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(session_id) => (
                "session/load",
                PendingRequest::LoadSession,
                json!({
                    "sessionId": session_id,
                    "cwd": self.cwd.clone(),
                    "mcpServers": []
                }),
            ),
            None => (
                "session/new",
                PendingRequest::NewSession,
                json!({"cwd": self.cwd.clone(), "mcpServers": []}),
            ),
        };
        params["_meta"] = session_plugin_meta(
            &self.permission_mode,
            self.model.as_deref(),
            &self.plugin_dirs,
        );
        self.pending.insert(key(&id), pending);
        if let Err(error) = self
            .write(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params
            }))
            .await
        {
            self.fail_startup(error).await;
        }
    }

    async fn session_ready(&mut self, result: &Value) {
        let session_id = result
            .get("sessionId")
            .and_then(Value::as_str)
            .or(self.resume_session_id.as_deref())
            .map(str::to_string);
        let Some(session_id) = session_id else {
            self.fail_startup("Grok ACP session response did not include sessionId".to_string())
                .await;
            return;
        };
        self.session_id = Some(session_id.clone());
        let (current_model, model_options) = parse_model_state(result);
        if self.model.is_none() {
            self.model = current_model;
        }
        if !model_options.is_empty() {
            self.model_options = model_options;
        }
        self.apply_model_capabilities();
        let mcp_servers = parse_servers(
            result
                .get("mcpServers")
                .or_else(|| result.pointer("/_meta/mcpServers")),
        );
        if !mcp_servers.is_empty() {
            self.mcp_servers = mcp_servers;
        }
        self.apply_session_modes(result);
        self.ready = true;
        let _ = storage::runs::update_session_id(&self.run_id, &session_id);
        let _ = storage::runs::update_status(&self.run_id, RunStatus::Idle, None, None);
        self.emit_session_init();
        self.emit_agent_mode_update();
        self.state("idle", None);
        self.dispatch().await;
    }

    fn apply_session_modes(&mut self, result: &Value) {
        let Some(state) = parse_session_modes(result) else {
            return;
        };
        let plan_supported = supports_plan(&state.available_modes);
        self.current_mode_id = Some(state.current_mode_id);
        self.available_modes = state.available_modes;
        self.capabilities.protocol.session_mode_control = true;
        self.capabilities.protocol.plan_mode = plan_supported;
        self.capabilities.ui.plan_mode_toggle = plan_supported;
    }

    fn emit_agent_mode_update(&self) {
        let Some(current_mode_id) = self.current_mode_id.clone() else {
            return;
        };
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::AgentModeUpdate {
                run_id: self.run_id.clone(),
                current_mode_id,
                available_modes: self.available_modes.clone(),
            },
        );
    }

    fn emit_session_init(&self) {
        let Some(session_id) = self.session_id.clone() else {
            return;
        };
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::SessionInit {
                run_id: self.run_id.clone(),
                session_id: Some(session_id),
                model: self.model.clone(),
                model_options: self.model_options.clone(),
                tools: Vec::new(),
                cwd: self.cwd.clone(),
                slash_commands: self.slash_commands.clone(),
                commands_loaded: self.commands_loaded,
                mcp_servers: self.mcp_servers.clone(),
                permission_mode: Some(self.permission_mode.clone()),
                api_key_source: None,
                claude_code_version: None,
                output_style: None,
                agents: Vec::new(),
                skills: Vec::new(),
                plugins: Vec::new(),
                plugin_errors: Vec::new(),
                fast_mode_state: None,
                capabilities: Some(self.capabilities.clone()),
            },
        );
    }

    async fn dispatch(&mut self) {
        while self.ready && !self.prompt_active {
            let Some(prompt) = self.queue.pop_front() else {
                return;
            };
            let Some(session_id) = self.session_id.clone() else {
                let _ = prompt
                    .reply
                    .send(Err("Grok ACP session is not ready".to_string()));
                return;
            };
            if !prompt.attachments.is_empty() && !self.capabilities.runtime.attachments {
                let _ = prompt.reply.send(Err(
                    "Grok ACP did not advertise image prompt blocks; attachments are not supported by this session"
                        .to_string(),
                ));
                continue;
            }
            let id = self.request_id();
            let id_key = key(&id);
            let text = prompt.text;
            let prompt_text =
                compose_continuation_prompt(self.continuation_context.as_deref(), &text);
            let content = match prompt_content(&prompt_text, &prompt.attachments) {
                Ok(content) => content,
                Err(error) => {
                    let _ = prompt.reply.send(Err(error));
                    continue;
                }
            };
            let frame = json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "session/prompt",
                "params": {
                    "sessionId": session_id,
                    "prompt": content
                }
            });
            // Snapshot the append-only log before Grok can process this prompt.
            // Capturing after the stdin write has a race for very fast responses.
            let persisted_updates_offset = self
                .persisted_updates_path()
                .and_then(|path| std::fs::metadata(path).ok())
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            match self.write(&frame).await {
                Ok(()) => {
                    self.continuation_context = None;
                    self.pending.insert(id_key, PendingRequest::Prompt);
                    self.prompt_active = true;
                    self.assistant_text.clear();
                    self.persisted_updates_offset = persisted_updates_offset;
                    self.usage_emitted_this_prompt = false;
                    self.user_message(&text, &prompt.attachments);
                    let _ =
                        storage::runs::update_status(&self.run_id, RunStatus::Running, None, None);
                    self.state("running", None);
                    let _ = prompt.reply.send(Ok(()));
                    return;
                }
                Err(error) => {
                    let _ = prompt.reply.send(Err(error.clone()));
                    self.fail(error);
                    return;
                }
            }
        }
    }

    async fn finish_prompt(&mut self, error: Option<String>) {
        let should_restore_usage = error.is_none() && !self.usage_emitted_this_prompt;
        self.settle_prompt(error);
        if should_restore_usage {
            self.restore_persisted_usage().await;
        }
        self.dispatch().await;
    }

    fn persisted_updates_path(&self) -> Option<PathBuf> {
        let home = self.grok_home.as_ref()?;
        let session_id = self.session_id.as_deref()?;
        Some(grok_updates_path(home, &self.cwd, session_id))
    }

    async fn restore_persisted_usage(&mut self) {
        let Some(path) = self.persisted_updates_path() else {
            return;
        };
        let run_id = self.run_id.clone();
        let offset = self.persisted_updates_offset;

        // Grok writes `turn_completed` to updates.jsonl even when its ACP stdio
        // transport omits the corresponding session/update notification. The
        // prompt response normally follows that write, but allow a short flush
        // window for slower filesystems.
        for delay_ms in [0, 25, 100] {
            if delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
            if let Some(event) = persisted_turn_usage_since(&path, offset, &run_id) {
                self.emitter.persist_and_emit(&self.run_id, &event);
                self.usage_emitted_this_prompt = true;
                return;
            }
        }
        log::debug!(
            "[grok] no persisted turn usage found after prompt: run_id={}, path={}",
            self.run_id,
            path.display()
        );
    }

    fn settle_prompt(&mut self, error: Option<String>) {
        if !self.assistant_text.is_empty() {
            let text = std::mem::take(&mut self.assistant_text);
            self.emitter.persist_and_emit(
                &self.run_id,
                &assistant_message_complete_event(&self.run_id, text, self.model.clone()),
            );
        }
        self.prompt_active = false;
        let _ = storage::runs::update_status(&self.run_id, RunStatus::Idle, None, error.clone());
        self.state("idle", error);
    }

    fn session_update(&mut self, message: &Value) {
        let Some(update) = message.pointer("/params/update") else {
            return;
        };
        let kind = update
            .get("sessionUpdate")
            .or_else(|| update.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        match kind {
            "available_commands_update" => {
                self.slash_commands = parse_available_commands(update);
                self.commands_loaded = true;
                self.capabilities.protocol.slash_commands = true;
                self.capabilities.ui.slash_command_menu = !self.slash_commands.is_empty();
                if self.ready {
                    self.emit_session_init();
                }
            }
            "current_mode_update" => {
                let available_modes = parse_available_modes_update(update);
                if !available_modes.is_empty() {
                    self.available_modes = available_modes;
                    self.capabilities.protocol.session_mode_control = true;
                    self.capabilities.protocol.plan_mode = supports_plan(&self.available_modes);
                    self.capabilities.ui.plan_mode_toggle = self.capabilities.protocol.plan_mode;
                }
                if let Some(mode) = parse_current_mode_update(update) {
                    if !self.available_modes.is_empty()
                        && !self
                            .available_modes
                            .iter()
                            .any(|candidate| candidate.id == mode)
                    {
                        self.raw("grok_acp_update", update.clone());
                        return;
                    }
                    self.current_mode_id = Some(mode);
                    if self.ready {
                        self.emit_agent_mode_update();
                    }
                }
            }
            "mcp_server_status" | "mcp_servers_update" => {
                self.apply_mcp_update(Some(update));
            }
            "model_update" | "models_update" => {
                self.apply_model_update(Some(update));
            }
            "turn_completed" => {
                if let Some(event) = grok_turn_usage_event(&self.run_id, update) {
                    if !self.usage_emitted_this_prompt {
                        self.emitter.persist_and_emit(&self.run_id, &event);
                        self.usage_emitted_this_prompt = true;
                    }
                } else {
                    self.raw("grok_acp_update", update.clone());
                }
            }
            "plan" => {
                if let Some(tasks) = parse_plan(update) {
                    self.capabilities.protocol.structured_task_state = true;
                    self.emitter.persist_and_emit(
                        &self.run_id,
                        &BusEvent::StructuredTaskState {
                            run_id: self.run_id.clone(),
                            tasks,
                        },
                    );
                    if self.ready {
                        self.emit_session_init();
                    }
                } else {
                    self.raw("grok_acp_update", update.clone());
                }
            }
            "agent_message_chunk" => {
                if let Some(text) = content_text(update.get("content")) {
                    self.assistant_text.push_str(text);
                    self.emitter.persist_and_emit(
                        &self.run_id,
                        &BusEvent::MessageDelta {
                            run_id: self.run_id.clone(),
                            text: text.to_string(),
                            parent_tool_use_id: None,
                        },
                    );
                }
            }
            "agent_thought_chunk" => {
                if let Some(text) = content_text(update.get("content")) {
                    self.emitter.persist_and_emit(
                        &self.run_id,
                        &BusEvent::ThinkingDelta {
                            run_id: self.run_id.clone(),
                            text: text.to_string(),
                            parent_tool_use_id: None,
                        },
                    );
                }
            }
            "tool_call" => {
                let tool_id = update
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or("grok-tool")
                    .to_string();
                let tool_name = tool_name(update);
                self.tool_names.insert(tool_id.clone(), tool_name.clone());
                let input = update.get("rawInput").cloned().unwrap_or_else(|| json!({}));
                self.emitter.persist_and_emit(
                    &self.run_id,
                    &BusEvent::ToolStart {
                        run_id: self.run_id.clone(),
                        tool_use_id: tool_id,
                        tool_name,
                        input,
                        parent_tool_use_id: None,
                    },
                );
            }
            "tool_call_update" => {
                let status = update
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("in_progress");
                if matches!(status, "completed" | "failed" | "cancelled") {
                    let tool_id = update
                        .get("toolCallId")
                        .and_then(Value::as_str)
                        .unwrap_or("grok-tool")
                        .to_string();
                    let completed_name = self
                        .tool_names
                        .remove(&tool_id)
                        .unwrap_or_else(|| tool_name(update));
                    self.emitter.persist_and_emit(
                        &self.run_id,
                        &BusEvent::ToolEnd {
                            run_id: self.run_id.clone(),
                            tool_use_id: tool_id,
                            tool_name: completed_name,
                            output: update
                                .get("rawOutput")
                                .cloned()
                                .unwrap_or_else(|| update.clone()),
                            status: status.to_string(),
                            duration_ms: None,
                            parent_tool_use_id: None,
                            tool_use_result: Some(update.clone()),
                        },
                    );
                }
            }
            _ => self.raw("grok_acp_update", update.clone()),
        }
    }

    fn apply_model_update(&mut self, value: Option<&Value>) {
        let Some(value) = value else { return };
        let (current_model, model_options) = parse_model_state(value);
        let model_changed = current_model.is_some();
        if let Some(current_model) = current_model {
            self.model = Some(current_model);
            if let Some(model) = self.model.as_deref() {
                let _ = storage::runs::update_run_model(&self.run_id, model);
            }
        }
        if !model_options.is_empty() {
            self.model_options = model_options;
        }
        self.apply_model_capabilities();
        if self.ready && (model_changed || !self.model_options.is_empty()) {
            self.emit_session_init();
        }
    }

    fn apply_model_capabilities(&mut self) {
        apply_model_capabilities(
            &mut self.capabilities,
            &mut self.model_options,
            self.model.as_deref(),
            self.custom_provider_active,
            self.custom_model_capabilities.as_ref(),
        );
    }

    fn apply_mcp_update(&mut self, value: Option<&Value>) {
        let servers = parse_servers(value);
        if servers.is_empty() {
            return;
        }
        for server in servers {
            if let Some(existing) = self
                .mcp_servers
                .iter_mut()
                .find(|existing| existing.name == server.name)
            {
                *existing = server;
            } else {
                self.mcp_servers.push(server);
            }
        }
        if self.ready {
            self.emit_session_init();
        }
    }

    async fn server_request(&mut self, method: &str, message: &Value) {
        if is_user_question_method(method) {
            self.user_question_request(message).await;
            return;
        }
        if method != "session/request_permission" {
            if let Some(id) = message.get("id") {
                let _ = self
                    .write(&json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": format!("AgentCabin does not implement ACP method {method}")
                        }
                    }))
                    .await;
            }
            return;
        }

        let Some(wire_id) = message.get("id").cloned() else {
            return;
        };
        let request_id = key(&wire_id);
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        let options = params
            .get("options")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let tool = params.get("toolCall").cloned().unwrap_or_else(|| json!({}));
        self.permissions.insert(
            request_id.clone(),
            PermissionRequest {
                wire_id,
                options: options.clone(),
            },
        );
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::PermissionPrompt {
                run_id: self.run_id.clone(),
                request_id,
                tool_name: tool_name(&tool),
                tool_use_id: tool
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or("grok-tool")
                    .to_string(),
                tool_input: tool.get("rawInput").cloned().unwrap_or_else(|| json!({})),
                decision_reason: "Grok Build requested permission through ACP".to_string(),
                parent_tool_use_id: None,
                suggestions: Vec::new(),
            },
        );
    }

    async fn user_question_request(&mut self, message: &Value) {
        let Some(wire_id) = message.get("id").cloned() else {
            return;
        };
        let request_id = key(&wire_id);
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        let Some(questions) = params.get("questions").and_then(Value::as_array) else {
            let _ = self
                .write(&json!({
                    "jsonrpc": "2.0",
                    "id": wire_id,
                    "error": {
                        "code": -32602,
                        "message": "Grok ask_user_question requires a questions array"
                    }
                }))
                .await;
            return;
        };
        if questions.is_empty() {
            let _ = self
                .write(&json!({
                    "jsonrpc": "2.0",
                    "id": wire_id,
                    "error": {
                        "code": -32602,
                        "message": "Grok ask_user_question requires at least one question"
                    }
                }))
                .await;
            return;
        }

        let questions = questions.clone();
        let input = grok_question_input(&questions);
        self.user_questions.insert(
            request_id.clone(),
            UserQuestionRequest { wire_id, questions },
        );
        // ACP can stream the assistant's introduction before asking for input. Flush that
        // message first so the shared timeline renders the introduction before the question
        // card instead of appending the card ahead of the still-live streaming text.
        let assistant_text =
            (!self.assistant_text.is_empty()).then(|| std::mem::take(&mut self.assistant_text));
        for event in grok_question_events(
            &self.run_id,
            &request_id,
            input,
            assistant_text,
            self.model.clone(),
        ) {
            self.emitter.persist_and_emit(&self.run_id, &event);
        }
    }

    async fn respond_user_question(
        &mut self,
        request_id: &str,
        response: &Value,
    ) -> Result<(), String> {
        let pending = self.user_questions.remove(request_id).ok_or_else(|| {
            format!("Grok user question request '{request_id}' is no longer pending")
        })?;
        let result = grok_question_response(&pending.questions, response);
        self.write(&json!({
            "jsonrpc": "2.0",
            "id": pending.wire_id,
            "result": result
        }))
        .await
    }

    async fn respond_permission(
        &mut self,
        request_id: &str,
        response: &Value,
    ) -> Result<(), String> {
        let pending = self.permissions.remove(request_id).ok_or_else(|| {
            format!("Grok permission request '{request_id}' is no longer pending")
        })?;
        let allow = response
            .get("behavior")
            .or_else(|| response.get("decision"))
            .and_then(Value::as_str)
            .is_some_and(|value| value == "allow");
        let result = match permission_option(&pending.options, allow) {
            Some(option_id) => json!({
                "outcome": {"outcome": "selected", "optionId": option_id}
            }),
            None => json!({"outcome": {"outcome": "cancelled"}}),
        };
        self.write(&json!({
            "jsonrpc": "2.0",
            "id": pending.wire_id,
            "result": result
        }))
        .await
    }

    async fn cancel_turn(&mut self) -> Result<(), String> {
        let Some(session_id) = self.session_id.clone() else {
            return Ok(());
        };
        self.write(&json!({
            "jsonrpc": "2.0",
            "method": "session/cancel",
            "params": {"sessionId": session_id}
        }))
        .await
    }

    async fn fail_startup(&mut self, error: String) {
        self.ready = false;
        self.prompt_active = false;
        self.fail(error);
        self.reject_queue("Grok ACP startup failed");
        // Suppress the generic unexpected-exit error so the original ACP failure remains visible.
        self.explicitly_stopped = true;
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }

    async fn shutdown(&mut self) {
        let _ = self.cancel_turn().await;
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        let _ = storage::runs::update_status(&self.run_id, RunStatus::Stopped, None, None);
        self.state("stopped", None);
    }

    fn fail(&self, error: String) {
        let _ = storage::runs::update_status(
            &self.run_id,
            RunStatus::Failed,
            None,
            Some(error.clone()),
        );
        self.state("failed", Some(error));
    }

    fn state(&self, state: &str, error: Option<String>) {
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

    fn user_message(&self, text: &str, attachments: &[AttachmentData]) {
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::UserMessage {
                run_id: self.run_id.clone(),
                text: text.to_string(),
                uuid: None,
                client_uuid: None,
                attachments: attachments
                    .iter()
                    .map(|attachment| AttachmentMeta {
                        name: attachment.filename.clone(),
                        mime_type: attachment.media_type.clone(),
                        size: attachment.content_base64.len() as u64,
                    })
                    .collect(),
            },
        );
    }

    fn command_output(&self, content: String) {
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::CommandOutput {
                run_id: self.run_id.clone(),
                content,
            },
        );
    }

    fn raw(&self, source: &str, data: Value) {
        self.emitter.persist_and_emit(
            &self.run_id,
            &BusEvent::Raw {
                run_id: self.run_id.clone(),
                source: source.to_string(),
                data,
            },
        );
    }

    fn reject_queue(&mut self, error: &str) {
        for prompt in self.queue.drain(..) {
            let _ = prompt.reply.send(Err(error.to_string()));
        }
    }

    async fn write(&mut self, message: &Value) -> Result<(), String> {
        let mut line = serde_json::to_string(message).map_err(|error| error.to_string())?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|error| format!("Grok ACP stdin write failed: {error}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|error| format!("Grok ACP stdin flush failed: {error}"))
    }

    fn request_id(&mut self) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        json!(id)
    }

    async fn cleanup(&mut self) {
        if let Some(token) = self.desktop_runtime_token.take() {
            crate::desktop_runtime::revoke_token(&token).await;
        }
        let mut sessions = self.sessions.lock().await;
        let remove = sessions
            .get(&self.run_id)
            .is_some_and(|handle| Arc::ptr_eq(&handle.tag, &self.tag));
        if remove {
            sessions.remove(&self.run_id);
        }
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
    }
}

pub(super) fn initialize_params() -> Value {
    json!({
        "protocolVersion": 1,
        "clientCapabilities": {
            "fs": {"readTextFile": false, "writeTextFile": false},
            "terminal": false
        },
        "clientInfo": {
            "name": "AgentCabin",
            "version": env!("CARGO_PKG_VERSION")
        },
        "_meta": {
            "clientType": "agentcabin",
            "clientVersion": env!("CARGO_PKG_VERSION")
        }
    })
}

pub(super) fn authentication_method(methods: &[Value], has_external_api_key: bool) -> Option<&str> {
    // Grok already discovers XAI_API_KEY during initialize. Calling authenticate
    // again switches its ACP transport into a non-interactive permission policy,
    // so use the injected credential directly for the session.
    if has_external_api_key {
        return None;
    }
    ["cached_token", "xai.api_key"]
        .iter()
        .find_map(|wanted| {
            methods
                .iter()
                .find(|method| auth_id(method) == Some(*wanted))
                .and_then(auth_id)
        })
        .or_else(|| methods.iter().find_map(auth_id))
}

pub(super) fn permission_session_meta(permission_mode: &str, model: Option<&str>) -> Value {
    let mut meta = json!({
        // ACP agent transports default to a non-interactive tool policy unless
        // the client explicitly opts out of yolo mode. Always send the value so
        // AgentCabin's "ask" mode cannot be shadowed by CLI/config defaults.
        "yoloMode": permission_mode == "bypassPermissions",
    });
    if permission_mode == "auto" {
        meta["autoMode"] = json!(true);
    }
    if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
        meta["modelId"] = json!(model);
    }
    meta
}

pub(super) fn session_plugin_meta(
    permission_mode: &str,
    model: Option<&str>,
    plugin_dirs: &[String],
) -> Value {
    let mut meta = permission_session_meta(permission_mode, model);
    if !plugin_dirs.is_empty() {
        meta["pluginDirs"] = json!(plugin_dirs);
    }
    meta
}

/// Grok's ACP effort control is the normal `session/set_model` request with a
/// `reasoningEffort` value in the request metadata. The CLI's `/effort` command
/// uses this same model-switch path.
pub(super) fn effort_request_frame(
    id: Value,
    session_id: &str,
    model_id: &str,
    effort: &str,
) -> Value {
    let mut params = json!({
        "sessionId": session_id,
        "modelId": model_id,
    });
    if !effort.trim().is_empty() {
        params["_meta"] = json!({"reasoningEffort": effort.trim()});
    }
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/set_model",
        "params": params,
    })
}

#[cfg(test)]
mod effort_control_tests {
    use super::effort_request_frame;
    use serde_json::json;

    #[test]
    fn sends_reasoning_effort_through_session_set_model_meta() {
        let frame = effort_request_frame(json!(7), "session-1", "grok-4.5", "low");
        assert_eq!(frame["method"], "session/set_model");
        assert_eq!(frame["params"]["sessionId"], "session-1");
        assert_eq!(frame["params"]["modelId"], "grok-4.5");
        assert_eq!(frame["params"]["_meta"]["reasoningEffort"], "low");
    }

    #[test]
    fn empty_effort_removes_the_override() {
        let frame = effort_request_frame(json!(8), "session-1", "grok-4.5", "");
        assert!(frame["params"].get("_meta").is_none());
    }
}

pub(super) fn compose_continuation_prompt(context: Option<&str>, text: &str) -> String {
    let Some(context) = context else {
        return text.to_string();
    };
    format!(
        "{context}\n\nThe user's next request is below. Use the historical context as background and answer only this request.\n\n<current-request>\n{text}\n</current-request>"
    )
}

fn key(id: &Value) -> String {
    id.as_str()
        .map(str::to_string)
        .unwrap_or_else(|| id.to_string())
}

pub(super) fn is_user_question_method(method: &str) -> bool {
    matches!(method, "x.ai/ask_user_question" | "_x.ai/ask_user_question")
}

pub(super) fn grok_question_input(questions: &[Value]) -> Value {
    let normalized = questions
        .iter()
        .enumerate()
        .map(|(index, question)| {
            let text = question
                .get("question")
                .and_then(Value::as_str)
                .unwrap_or("");
            let id = question
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or(if text.is_empty() {
                    "grok-question"
                } else {
                    text
                });
            let options = question
                .get("options")
                .and_then(Value::as_array)
                .map(|options| {
                    options
                        .iter()
                        .map(|option| {
                            let mut normalized = json!({
                                "label": option
                                    .get("label")
                                    .and_then(Value::as_str)
                                    .unwrap_or(""),
                                "description": option
                                    .get("description")
                                    .and_then(Value::as_str)
                                    .unwrap_or(""),
                            });
                            if let Some(preview) = option.get("preview") {
                                normalized["preview"] = preview.clone();
                            }
                            normalized
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            json!({
                "id": if id == "grok-question" {
                    format!("grok-question-{index}")
                } else {
                    id.to_string()
                },
                "question": text,
                "options": options,
                "multiSelect": question
                    .get("multiSelect")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect::<Vec<_>>();
    json!({"questions": normalized})
}

pub(super) fn grok_question_response(questions: &[Value], response: &Value) -> Value {
    let incoming = response
        .get("answers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut answers = serde_json::Map::new();
    for (index, question) in questions.iter().enumerate() {
        let text = question
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or("");
        let fallback_id = if text.is_empty() {
            format!("grok-question-{index}")
        } else {
            text.to_string()
        };
        let id = question
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(&fallback_id);
        let answer = incoming
            .get(id)
            .or_else(|| incoming.get(text))
            .and_then(question_answer_text)
            .unwrap_or_default();
        answers.insert(text.to_string(), json!(answer));
    }
    json!({
        "outcome": "accepted",
        "answers": Value::Object(answers),
        "annotations": {}
    })
}

fn assistant_message_complete_event(run_id: &str, text: String, model: Option<String>) -> BusEvent {
    BusEvent::MessageComplete {
        run_id: run_id.to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        text,
        parent_tool_use_id: None,
        model,
        stop_reason: None,
        message_usage: None,
    }
}

pub(super) fn grok_question_events(
    run_id: &str,
    request_id: &str,
    input: Value,
    assistant_text: Option<String>,
    model: Option<String>,
) -> Vec<BusEvent> {
    let mut events = Vec::with_capacity(if assistant_text.is_some() { 3 } else { 2 });
    if let Some(text) = assistant_text.filter(|text| !text.is_empty()) {
        events.push(assistant_message_complete_event(run_id, text, model));
    }
    events.push(BusEvent::ToolStart {
        run_id: run_id.to_string(),
        tool_use_id: request_id.to_string(),
        tool_name: "AskUserQuestion".to_string(),
        input: input.clone(),
        parent_tool_use_id: None,
    });
    // The shared store uses an error ToolEnd to enter its ask_pending state. The actual Grok
    // request remains pending in user_questions until the answer arrives.
    events.push(BusEvent::ToolEnd {
        run_id: run_id.to_string(),
        tool_use_id: request_id.to_string(),
        tool_name: "AskUserQuestion".to_string(),
        output: input,
        status: "error".to_string(),
        duration_ms: None,
        parent_tool_use_id: None,
        tool_use_result: None,
    });
    events
}

fn question_answer_text(value: &Value) -> Option<String> {
    if let Some(values) = value.as_array() {
        return Some(
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    value.as_str().map(str::to_string)
}

/// Decide whether a Grok child diagnostic belongs in the task timeline.
///
/// Grok's MCP worker reports an OAuth discovery/authorization failure on stderr
/// as a fatal transport message even though the ACP session continues and the
/// failed MCP is represented in `mcp_server_status`. Custom-provider sessions
/// also make Grok's built-in `grok-*` path see the provider key through the
/// compatibility `XAI_API_KEY` variable. That native x.ai request is unrelated
/// to the active custom model and must stay in the application log instead of
/// looking like the user's request failed.
pub(super) fn should_surface_stderr(line: &str, custom_provider_active: bool) -> bool {
    let lower = line.to_ascii_lowercase();
    let is_mcp_auth_diagnostic = lower.contains("worker quit with fatal")
        && (lower.contains("authrequired") || lower.contains("auth required"))
        && lower.contains("resource_metadata");
    let is_native_grok_auth_diagnostic = custom_provider_active
        && lower.contains("responses api error")
        && lower.contains("incorrect api key provided")
        && lower.contains("console.x.ai")
        && lower.contains("model_id")
        && lower.contains("grok-");

    !(is_mcp_auth_diagnostic || is_native_grok_auth_diagnostic)
}

fn auth_id(method: &Value) -> Option<&str> {
    method
        .as_str()
        .or_else(|| method.get("id").and_then(Value::as_str))
}

fn content_text(content: Option<&Value>) -> Option<&str> {
    let content = content?;
    content
        .get("text")
        .and_then(Value::as_str)
        .or_else(|| content.as_str())
}

pub(super) fn tool_name(value: &Value) -> String {
    value
        .get("_meta")
        .and_then(|meta| meta.get("x.ai/tool"))
        .and_then(|tool| tool.get("label"))
        .or_else(|| value.get("title"))
        .or_else(|| value.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("Grok Tool")
        .to_string()
}

pub(super) fn permission_option(options: &[Value], allow: bool) -> Option<&str> {
    let matched = options.iter().find(|option| {
        let kind = option
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let name = option
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        if allow {
            kind == "allow_once"
                || kind == "allowonce"
                || kind.starts_with("allow")
                || name.contains("allow")
        } else {
            kind.contains("reject")
                || kind.contains("deny")
                || name.contains("reject")
                || name.contains("deny")
        }
    });
    matched.and_then(|option| {
        option
            .get("optionId")
            .or_else(|| option.get("id"))
            .and_then(Value::as_str)
    })
}

fn rpc_error(error: &Value) -> String {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Grok ACP request failed");
    match error.get("data") {
        Some(data) => format!("{message}: {data}"),
        None => message.to_string(),
    }
}

/// Convert Grok's `_x.ai/session/update` turn summary into AgentCabin's shared usage event.
/// Grok reports cached tokens as a subset of `inputTokens`; the shared event model expects
/// uncached input and cache buckets separately, so subtract the cache subsets exactly once.
pub(super) fn grok_turn_usage_event(run_id: &str, update: &Value) -> Option<BusEvent> {
    let usage = update.get("usage")?;
    let total_input = usage_u64(usage, &["inputTokens", "input_tokens"]);
    let output_tokens = usage_u64(usage, &["outputTokens", "output_tokens"]);
    let cache_read = usage_u64(
        usage,
        &["cachedReadTokens", "cacheReadTokens", "cache_read_tokens"],
    );
    let cache_write = usage_u64(
        usage,
        &[
            "cacheCreationTokens",
            "cacheWriteTokens",
            "cache_write_tokens",
        ],
    );
    if total_input == 0 && output_tokens == 0 && cache_read == 0 && cache_write == 0 {
        return None;
    }
    let input_tokens = total_input.saturating_sub(cache_read.saturating_add(cache_write));
    let total_cost_usd = usage
        .get("cost")
        .and_then(|cost| cost.get("total"))
        .and_then(Value::as_f64)
        .or_else(|| usage.get("costUsd").and_then(Value::as_f64))
        .or_else(|| usage.get("costUSD").and_then(Value::as_f64))
        .unwrap_or(0.0);

    Some(BusEvent::UsageUpdate {
        run_id: run_id.to_string(),
        input_tokens,
        output_tokens,
        cache_read_tokens: Some(cache_read),
        cache_write_tokens: Some(cache_write),
        total_cost_usd,
        turn_index: None,
        context_tokens: None,
        context_window: None,
        model_usage: None,
        duration_api_ms: ["apiDurationMs", "durationApiMs", "duration_api_ms"]
            .into_iter()
            .find_map(|key| usage.get(key).and_then(Value::as_u64)),
        duration_ms: None,
        num_turns: ["numTurns", "num_turns"]
            .into_iter()
            .find_map(|key| usage.get(key).and_then(Value::as_u64)),
        stop_reason: update
            .get("stop_reason")
            .or_else(|| update.get("stopReason"))
            .and_then(Value::as_str)
            .map(str::to_string),
        service_tier: None,
        speed: None,
        web_fetch_requests: None,
        cache_creation_5m: None,
        cache_creation_1h: None,
    })
}

fn grok_updates_path(grok_home: &Path, cwd: &str, session_id: &str) -> PathBuf {
    grok_home
        .join("sessions")
        .join(urlencoding::encode(cwd).as_ref())
        .join(session_id)
        .join("updates.jsonl")
}

fn persisted_turn_usage_since(path: &Path, offset: u64, run_id: &str) -> Option<BusEvent> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let safe_offset = if offset <= len { offset } else { 0 };
    file.seek(SeekFrom::Start(safe_offset)).ok()?;

    let mut latest = None;
    for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(update) = record
            .get("update")
            .or_else(|| record.pointer("/params/update"))
        else {
            continue;
        };
        let kind = update
            .get("sessionUpdate")
            .or_else(|| update.get("type"))
            .and_then(Value::as_str);
        if kind == Some("turn_completed") {
            latest = grok_turn_usage_event(run_id, update);
        }
    }
    latest
}

#[cfg(test)]
pub(super) fn last_persisted_turn_usage(
    grok_home: &Path,
    cwd: &str,
    session_id: &str,
    run_id: &str,
) -> Option<BusEvent> {
    persisted_turn_usage_since(&grok_updates_path(grok_home, cwd, session_id), 0, run_id)
}

#[cfg(test)]
mod stderr_tests {
    use super::should_surface_stderr;

    const NATIVE_GROK_AUTH_DIAGNOSTIC: &str =
        "ERROR responses API error status=400 Bad Request error_message=invalid-argument: Incorrect API key provided. You can obtain an API key from https://console.x.ai. model_id=grok-4.5";
    const NATIVE_GROK_4_6_AUTH_DIAGNOSTIC: &str =
        "[grok acp stderr] 2026-08-15T02:33:38.065829Z ERROR responses API error *status*=400 Bad Request *error_message*=invalid-argument: Incorrect API key provided. You can obtain an API key from https://console.x.ai. *body_preview*={\"code\":\"invalid-argument\",\"error\":\"Incorrect API key provided\"} *model_id*=grok-4.6";

    #[test]
    fn hides_native_grok_auth_diagnostic_for_custom_provider_sessions() {
        assert!(!should_surface_stderr(NATIVE_GROK_AUTH_DIAGNOSTIC, true));
    }

    #[test]
    fn hides_newer_native_grok_auth_diagnostic_for_custom_provider_sessions() {
        assert!(!should_surface_stderr(
            NATIVE_GROK_4_6_AUTH_DIAGNOSTIC,
            true
        ));
    }

    #[test]
    fn keeps_native_grok_auth_diagnostic_visible_for_cli_sessions() {
        assert!(should_surface_stderr(NATIVE_GROK_AUTH_DIAGNOSTIC, false));
    }

    #[test]
    fn keeps_real_custom_responses_errors_visible() {
        assert!(should_surface_stderr(
            "ERROR responses API error status=401 model_id=deepseek/deepseek-v4-flash",
            true
        ));
    }
}

pub(super) fn is_session_update_method(method: &str) -> bool {
    matches!(
        method,
        "session/update" | "x.ai/session/update" | "_x.ai/session/update"
    )
}

fn usage_u64(usage: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| usage.get(*key).and_then(Value::as_u64))
        .unwrap_or(0)
}
