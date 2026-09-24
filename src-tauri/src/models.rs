pub use crate::agent::pi_features::capabilities::PiFeatureCapabilities;
pub use crate::agent::pi_features::state::{
    PiGoalState, PiMessageQueueState, PiPermissionState, PiPlanState, PiSessionEntry,
    PiSessionTreeNode, PiTodoPhase, PiTodoState, PiTodoTask,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::work::models::AppMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentTarget {
    #[serde(rename = "native:codex")]
    NativeCodex,
    #[serde(rename = "native:claude")]
    NativeClaude,
    #[serde(rename = "native:grok")]
    NativeGrok,
    #[serde(rename = "pi:code")]
    PiCode,
    /// Runtime-neutral Work product target. The old `pi:work` value remains a
    /// deserialization alias for persisted runs created before Runtime
    /// became independent from the product mode.
    #[serde(rename = "work", alias = "pi:work")]
    Work,
}

impl AgentTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NativeCodex => "native:codex",
            Self::NativeClaude => "native:claude",
            Self::NativeGrok => "native:grok",
            Self::PiCode => "pi:code",
            Self::Work => "work",
        }
    }

    pub fn from_legacy(app_mode: AppMode, agent: &str) -> Self {
        if app_mode == AppMode::Work {
            Self::Work
        } else {
            match agent {
                "pi" => Self::PiCode,
                "codex" => Self::NativeCodex,
                "grok" => Self::NativeGrok,
                _ => Self::NativeClaude,
            }
        }
    }

    pub fn is_native(&self) -> bool {
        matches!(
            self,
            Self::NativeCodex | Self::NativeClaude | Self::NativeGrok
        )
    }

    pub fn is_pi(&self) -> bool {
        matches!(self, Self::PiCode)
    }

    pub fn agent_name(&self) -> &'static str {
        match self {
            Self::NativeCodex => "codex",
            Self::NativeClaude => "claude",
            Self::NativeGrok => "grok",
            Self::PiCode => "pi",
            Self::Work => "work",
        }
    }

    pub fn app_mode(&self) -> AppMode {
        match self {
            Self::Work => AppMode::Work,
            _ => AppMode::Code,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PiExtensionUiMethod {
    Select,
    Confirm,
    Input,
    Editor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PiExtensionUiRequest {
    pub id: String,
    pub run_id: String,
    pub method: PiExtensionUiMethod,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefill: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PiExtensionWidget {
    pub key: String,
    pub lines: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PiExtensionHostSnapshot {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_requests: Vec<PiExtensionUiRequest>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub statuses: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub widgets: HashMap<String, PiExtensionWidget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LocalProxyStatus {
    pub proxy_id: String,
    pub running: bool,
    pub needs_auth: bool,
    pub base_url: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiTestResult {
    pub success: bool,
    pub latency_ms: u64,
    pub reply: Option<String>,
    pub error: Option<String>,
    /// True when auth+connectivity OK but probe model was rejected (no user model configured).
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryFileCandidate {
    pub path: String,
    pub label: String,
    pub scope: String, // "project" | "global" | "memory"
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Pending,
    Running,
    /// Turn complete, waiting for user input. Session is still alive.
    Idle,
    Completed,
    Failed,
    Stopped,
}

impl std::fmt::Display for RunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunStatus::Pending => write!(f, "pending"),
            RunStatus::Running => write!(f, "running"),
            RunStatus::Idle => write!(f, "idle"),
            RunStatus::Completed => write!(f, "completed"),
            RunStatus::Failed => write!(f, "failed"),
            RunStatus::Stopped => write!(f, "stopped"),
        }
    }
}

/// Run source — how this run was created.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunSource {
    Native,    // app-created run
    CliImport, // imported from CLI transcript
}

/// Import watermark for incremental CLI session sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportWatermark {
    pub offset: u64,
    pub mtime_ns: u128,
    pub file_size: u64,
    pub last_uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunEventType {
    System,
    Stdout,
    Stderr,
    Command,
    User,
    Assistant,
}

impl std::fmt::Display for RunEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunEventType::System => write!(f, "system"),
            RunEventType::Stdout => write!(f, "stdout"),
            RunEventType::Stderr => write!(f, "stderr"),
            RunEventType::Command => write!(f, "command"),
            RunEventType::User => write!(f, "user"),
            RunEventType::Assistant => write!(f, "assistant"),
        }
    }
}

/// Attachment metadata (name/type/size — no content blob).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMeta {
    pub name: String,
    pub mime_type: String,
    pub size: u64,
}

/// App-internal execution path — which backend subsystem handles this run.
/// NOT a protocol description; a single agent may support multiple paths.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPath {
    /// Long-lived process with bidirectional control protocol (Claude stream-json via session_actor)
    SessionActor,
    /// Single-shot process, stdout only (Codex exec, Claude --print via stream.rs)
    PipeExec,
}

/// Unified resume/fork identity across agents.
/// Claude = session_id from system/init; Codex = thread_id from thread.started.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "id")]
pub enum ConversationRef {
    /// Claude Code session ID (from system/init event)
    #[serde(rename = "claude_session")]
    ClaudeSession(String),
    /// Codex thread ID (from thread.started event)
    #[serde(rename = "codex_thread")]
    CodexThread(String),
    /// Pi session ID
    #[serde(rename = "pi_session")]
    PiSession(String),
    /// Grok Build ACP session ID
    #[serde(rename = "grok_session")]
    GrokSession(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRun {
    pub id: String,
    pub prompt: String,
    pub cwd: String,
    pub agent: String,
    /// Code run created without a project, using its own AgentCabin-managed cwd.
    #[serde(default)]
    pub code_standalone_task: bool,
    #[serde(default)]
    pub app_mode: AppMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_target: Option<AgentTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_execution_context: Option<crate::work::models::ExecutionContext>,
    /// Work user-facing preset. Legacy runs default to Office when consumed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_preset: Option<crate::work::models::WorkPreset>,
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,
    pub status: RunStatus,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_subtype: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The reasoning effort selected for this conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    /// The run_id this session was forked from (None if not a fork).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
    /// User-assigned display name (None = use prompt as label).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Remote host name (if this run is on a remote machine).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_host_name: Option<String>,
    /// Snapshot of remote working directory at run creation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_cwd: Option<String>,
    /// Snapshot of active_platform_id at run creation time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_id: Option<String>,
    /// Snapshot of anthropic_base_url at run creation time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_base_url: Option<String>,
    /// Run source (native or cli_import).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<RunSource>,
    /// CLI import watermark for incremental sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_import_watermark: Option<ImportWatermark>,
    /// Absolute path to CLI session JSONL file (read-only reference).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_session_path: Option<String>,
    /// True when CLI import couldn't reconstruct complete usage data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_usage_incomplete: Option<bool>,
    /// Snapshot of no_session_persistence at run creation time.
    #[serde(default)]
    pub no_session_persistence: bool,
    /// Resolved execution path (materialized from RunMeta, never None in API output).
    pub execution_path: ExecutionPath,
    /// Resolved conversation identity (None = not resumable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_ref: Option<ConversationRef>,
    /// Pinned to top of sidebar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    /// Archived (hidden from default sidebar view).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
    /// Unread badge indicator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unread: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    pub id: String,
    pub task_id: String,
    pub seq: u64,
    #[serde(rename = "type")]
    pub event_type: RunEventType,
    pub payload: serde_json::Value,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunArtifact {
    pub task_id: String,
    pub files_changed: Vec<String>,
    pub diff_summary: String,
    pub commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_estimate: Option<f64>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectModelPreference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    #[serde(default = "default_agent_pi")]
    pub default_agent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    pub allowed_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    pub provider_mode: String,
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_base_url: Option<String>,
    /// Which env var to inject: "ANTHROPIC_API_KEY" or "ANTHROPIC_AUTH_TOKEN".
    /// Set by the selected platform preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_env_var: Option<String>,
    #[serde(default = "default_permission_mode")]
    pub permission_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_budget_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_model: Option<String>,
    #[serde(default)]
    pub keybinding_overrides: Vec<KeyBindingOverride>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remote_hosts: Vec<RemoteHost>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platform_credentials: Vec<PlatformCredential>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_platform_id: Option<String>,
    /// Canonical provider profiles shared by Claude, Codex, Pi, Grok and DSH. The legacy fields below
    /// remain serialized so older versions and the runtime can continue to read them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub global_providers: Vec<GlobalProviderCredential>,
    /// Per-agent choice of native CLI auth or a compatible global provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_provider_bindings: Option<AgentProviderBindings>,
    /// Active Codex third-party provider (OpenAI Responses API). None = plain `codex login`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_provider: Option<CodexProviderCredential>,
    /// AgentCabin-managed Pi provider. None = use Pi CLI's own config and environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pi_provider: Option<PiProviderCredential>,
    /// Codex session transport: "app_server" (bidirectional JSON-RPC — interactive tools:
    /// approvals, requestUserInput, MCP elicitation) or "exec" (one-way NDJSON, no interactivity).
    /// None / "exec" = legacy exec path. Drives the default execution_path for new Codex runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_transport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_zoom: Option<f64>,
    #[serde(default)]
    pub onboarding_completed: bool,
    /// Beta kill-switch for the entire Work product surface. Missing legacy
    /// settings default to enabled so existing users keep their Work data.
    #[serde(default = "default_work_mode_enabled")]
    pub work_mode_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_server_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_server_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_server_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_server_bind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_server_allowed_origins: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_server_tunnel_url: Option<String>,
    /// Custom path/program to launch the Claude CLI, instead of auto-detecting `claude`.
    /// Lets users point at a non-standard install or a transparent wrapper (e.g. a
    /// claude-tap script). Empty/None = auto-detect. (#155)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_path: Option<String>,
    /// Custom path/program to launch the Codex CLI, instead of auto-detecting `codex`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_path: Option<String>,
    /// Custom path/program to launch the Pi CLI, instead of auto-detecting `pi`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pi_path: Option<String>,
    /// Custom path/program to launch Grok Build, instead of auto-detecting `grok`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grok_path: Option<String>,
    /// Custom path/program to launch DeepSeek Harness (DSH), instead of auto-detecting `dsh`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dsh_path: Option<String>,
    /// Enabled agents list ("pi", "codex", "claude", "grok", "dsh"). None defaults to ["pi"].
    #[serde(
        default = "default_enabled_agents",
        skip_serializing_if = "Option::is_none"
    )]
    pub enabled_agents: Option<Vec<String>>,
    /// Root directory for automatically created worktrees. None keeps the project-local default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_default_runtime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_default_runtime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_root: Option<String>,
    /// Prefix used for branches generated by AgentCabin continuation flows.
    #[serde(default = "default_worktree_branch_prefix")]
    pub worktree_branch_prefix: String,
    /// Whether AgentCabin may remove old, clean managed worktrees.
    #[serde(default = "default_worktree_auto_cleanup")]
    pub worktree_auto_cleanup: bool,
    /// Maximum managed worktrees retained per repository.
    #[serde(default = "default_worktree_cleanup_limit")]
    pub worktree_cleanup_limit: u32,
    /// Internal provenance registry. User-created worktrees are never eligible for cleanup.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub worktree_managed_paths: Vec<String>,
    #[serde(default = "default_pet_enabled")]
    pub pet_enabled: bool,
    #[serde(default = "default_pet_scale")]
    pub pet_scale: f64,
    #[serde(default = "default_pet_always_on_top")]
    pub pet_always_on_top: bool,
    #[serde(default = "default_pet_id")]
    pub pet_id: String,
    #[serde(default = "default_pet_patrol_enabled")]
    pub pet_patrol_enabled: bool,
    #[serde(default = "default_pet_patrol_pause_min")]
    pub pet_patrol_pause_min: u32,
    #[serde(default = "default_pet_snap_to_edge")]
    pub pet_snap_to_edge: bool,
    #[serde(default = "default_pet_click_interaction_enabled")]
    pub pet_click_interaction_enabled: bool,
    #[serde(default = "default_pet_grokbot_color")]
    pub pet_grokbot_color: String,
    #[serde(default = "default_pet_grokbot_shape")]
    pub pet_grokbot_shape: String,
    #[serde(default)]
    pub pet_grokbot_parts: Vec<String>,
    #[serde(default)]
    pub pet_grokbot_accessories: Vec<String>,
    pub updated_at: String,
}

fn default_pet_enabled() -> bool {
    false
}

fn default_pet_scale() -> f64 {
    1.0
}

fn default_pet_always_on_top() -> bool {
    true
}

fn default_pet_id() -> String {
    "agentcabin-bot".to_string()
}

fn default_pet_patrol_enabled() -> bool {
    true
}

fn default_pet_patrol_pause_min() -> u32 {
    3
}

fn default_pet_snap_to_edge() -> bool {
    false
}

fn default_pet_click_interaction_enabled() -> bool {
    true
}

fn default_pet_grokbot_color() -> String {
    "blue".to_string()
}

fn default_pet_grokbot_shape() -> String {
    "blob".to_string()
}

fn default_worktree_branch_prefix() -> String {
    "agentcabin/".to_string()
}

fn default_worktree_auto_cleanup() -> bool {
    true
}

fn default_worktree_cleanup_limit() -> u32 {
    15
}

fn default_agent_pi() -> String {
    "pi".to_string()
}

fn default_enabled_agents() -> Option<Vec<String>> {
    Some(vec!["pi".to_string()])
}

fn default_auth_mode() -> String {
    "cli".to_string()
}

fn default_work_mode_enabled() -> bool {
    true
}

fn default_ssh_port() -> u16 {
    22
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteHost {
    pub name: String,
    pub host: String,
    pub user: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_claude_path: Option<String>,
    #[serde(default)]
    pub forward_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTestResult {
    pub ssh_ok: bool,
    pub cli_found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn default_permission_mode() -> String {
    "auto_read".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformCredential {
    pub platform_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_env_var: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_env: Option<HashMap<String, String>>,
}

/// Canonical provider profile shared across agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalProviderCredential {
    pub id: String,
    pub name: String,
    /// `anthropic-messages`, `openai-responses`, or `openai-completions`.
    pub protocol: String,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_env_var: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_key: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_global_models",
        skip_serializing_if = "Option::is_none"
    )]
    pub models: Option<Vec<GlobalProviderModel>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_developer_role: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_reasoning_effort: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_env: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyless: Option<bool>,
}

/// A model configured inside a global Provider. Keeping capabilities next to the model lets
/// agent pickers and future request adapters make an informed choice without another catalog.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GlobalProviderModel {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_reasoning: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_xhigh: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_effort_levels: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_images: Option<bool>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum GlobalProviderModelValue {
    Detailed(GlobalProviderModel),
    Id(String),
}

fn deserialize_global_models<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<GlobalProviderModel>>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Option::<Vec<GlobalProviderModelValue>>::deserialize(deserializer)?;
    Ok(values.map(|items| {
        items
            .into_iter()
            .map(|item| match item {
                GlobalProviderModelValue::Detailed(model) => model,
                GlobalProviderModelValue::Id(id) => GlobalProviderModel {
                    id,
                    name: None,
                    context_window: None,
                    max_tokens: None,
                    supports_reasoning: None,
                    supports_xhigh: None,
                    supported_effort_levels: None,
                    supports_images: None,
                },
            })
            .collect()
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProviderBinding {
    /// `cli` uses the agent's native auth/config; `custom` uses provider_id below.
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Models enabled for this agent. Missing means a legacy single-model binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl Default for AgentProviderBinding {
    fn default() -> Self {
        Self {
            mode: "cli".to_string(),
            provider_id: None,
            models: None,
            model: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProviderBindings {
    pub claude: AgentProviderBinding,
    pub codex: AgentProviderBinding,
    pub pi: AgentProviderBinding,
    #[serde(default)]
    pub grok: AgentProviderBinding,
    #[serde(default)]
    pub dsh: AgentProviderBinding,
}

/// A Codex third-party provider. Chat Completions providers are routed through AgentCabin's
/// loopback Responses bridge before Codex is spawned. Unlike Claude's Anthropic-shaped
/// PlatformCredential, Codex providers are injected via `codex exec -c model_providers.<id>.*`
/// overrides + an env var (env_key → api_key), not ANTHROPIC_* env.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexProviderCredential {
    /// Stable provider id used as the `model_providers.<id>` table key (e.g. "vercel", "custom").
    pub id: String,
    pub name: String,
    pub base_url: String,
    /// Name of the env var Codex reads the API key from (e.g. "OPENAI_API_KEY").
    pub env_key: String,
    /// Persisted upstream shape: "responses" or "chat". The runtime bridge always presents
    /// "responses" to Codex.
    #[serde(default = "default_wire_api")]
    pub wire_api: String,
    /// Provider-side model id (OpenAI-format, e.g. "gpt-5.5"). Empty → Codex default.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// API key. Optional for keyless local providers (e.g. Ollama).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Whether the Chat Completions upstream accepts the `reasoning_effort` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_reasoning_effort: Option<bool>,
    /// Optional Codex transport override. The loopback Chat bridge sets this to `false`
    /// because it intentionally exposes HTTPS Responses only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_websockets: Option<bool>,
}

/// Provider configuration consumed by AgentCabin's explicit Pi Provider Bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiProviderCredential {
    pub id: String,
    pub name: String,
    pub base_url: String,
    /// Pi API adapter: `openai-completions`, `openai-responses`, or `anthropic-messages`.
    pub api: String,
    pub model: String,
    /// All models enabled for this provider. The selected `model` is kept separately for
    /// compatibility with Pi's current-model flag, while this list keeps Pi's model registry
    /// complete when the user switches models during a running session.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<GlobalProviderModel>,
    /// Configured context window for the selected model, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

fn default_wire_api() -> String {
    "responses".to_string()
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            default_agent: "pi".to_string(),
            default_model: None,
            allowed_tools: vec![],
            working_directory: None,
            provider_mode: "local".to_string(),
            auth_mode: "cli".to_string(),
            anthropic_api_key: None,
            anthropic_base_url: None,
            auth_env_var: None,
            permission_mode: "auto_read".to_string(),
            max_budget_usd: None,
            fallback_model: None,
            keybinding_overrides: vec![],
            remote_hosts: vec![],
            platform_credentials: vec![],
            active_platform_id: None,
            global_providers: vec![],
            agent_provider_bindings: None,
            codex_provider: None,
            pi_provider: None,
            codex_transport: None,
            ui_zoom: None,
            onboarding_completed: false,
            work_mode_enabled: true,
            web_server_enabled: None,
            web_server_token: None,
            web_server_port: None,
            web_server_bind: None,
            web_server_allowed_origins: None,
            web_server_tunnel_url: None,
            claude_path: None,
            codex_path: None,
            pi_path: None,
            grok_path: None,
            dsh_path: None,
            enabled_agents: Some(vec!["pi".to_string()]),
            code_default_runtime: None,
            work_default_runtime: None,
            worktree_root: None,
            worktree_branch_prefix: default_worktree_branch_prefix(),
            worktree_auto_cleanup: default_worktree_auto_cleanup(),
            worktree_cleanup_limit: default_worktree_cleanup_limit(),
            worktree_managed_paths: vec![],
            pet_enabled: false,
            pet_scale: 1.0,
            pet_always_on_top: true,
            pet_id: "agentcabin-bot".to_string(),
            pet_patrol_enabled: true,
            pet_patrol_pause_min: 3,
            pet_snap_to_edge: false,
            pet_click_interaction_enabled: true,
            pet_grokbot_color: default_pet_grokbot_color(),
            pet_grokbot_shape: default_pet_grokbot_shape(),
            pet_grokbot_parts: vec![],
            pet_grokbot_accessories: vec![],
            updated_at: now_iso(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    pub agent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub allowed_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_mode: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disallowed_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append_system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_budget_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_set: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_dirs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_partial_messages: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_debug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_session_persistence: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
    /// Custom agent definitions JSON string (passed to --agents flag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents_json: Option<String>,
    /// Agent-scoped permission mode override (app names: "ask", "auto_all", "plan", etc.).
    /// Takes priority over plan_mode and user.permission_mode in adapter resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    /// Claude plugin roots explicitly enabled for Grok Build ACP sessions.
    ///
    /// Grok receives these as session-scoped `_meta.pluginDirs`; keeping the
    /// selection in AgentCabin settings avoids silently loading every Claude
    /// plugin (some plugins also contain hooks, MCP servers, or LSP servers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grok_plugin_dirs: Option<Vec<String>>,
    /// Codex `--ephemeral` — disable on-disk session persistence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<bool>,
    /// Codex `--profile <name>` — select a profile from ~/.codex/config.toml.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Codex `--ignore-user-config` — skip ~/.codex/config.toml.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_user_config: Option<bool>,
    /// Codex `--ignore-rules` — skip execpolicy .rules files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_rules: Option<bool>,
    /// Codex `--search` — enable the native web_search tool (new sessions only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_search: Option<bool>,
    /// Pi `-e npm:@narumitw/pi-plan-mode` — load the Plan extension independently.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pi_plan_mode_enabled: Option<bool>,
    /// Pi `-e npm:@narumitw/pi-goal` — load the Goal extension independently.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pi_goal_enabled: Option<bool>,
    /// Pi `-e npm:pi-context-prune` — load optional context pruning. Disabled by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pi_context_prune_enabled: Option<bool>,
    /// Legacy compatibility field. Pi Code always loads the system-managed multi-edit extension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pi_multi_edit_enabled: Option<bool>,
    /// Pi `-e npm:@narumitw/pi-lsp` — load Language Server Protocol (LSP) code intelligence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pi_lsp_enabled: Option<bool>,
    pub updated_at: String,
}

impl AgentSettings {
    pub fn default_for(agent: &str) -> Self {
        Self {
            agent: agent.to_string(),
            model: None,
            allowed_tools: vec![],
            working_directory: None,
            plan_mode: None,
            disallowed_tools: None,
            append_system_prompt: None,
            max_budget_usd: None,
            fallback_model: None,
            system_prompt: None,
            tool_set: None,
            add_dirs: None,
            json_schema: None,
            include_partial_messages: None,
            cli_debug: None,
            no_session_persistence: None,
            max_turns: None,
            effort: None,
            betas: None,
            agents_json: None,
            permission_mode: None,
            grok_plugin_dirs: None,
            ephemeral: None,
            profile: None,
            ignore_user_config: None,
            ignore_rules: None,
            web_search: None,
            pi_plan_mode_enabled: None,
            pi_goal_enabled: None,
            pi_context_prune_enabled: None,
            pi_multi_edit_enabled: None,
            pi_lsp_enabled: None,
            updated_at: now_iso(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllSettings {
    pub user: UserSettings,
    pub agents: std::collections::HashMap<String, AgentSettings>,
}

impl Default for AllSettings {
    fn default() -> Self {
        let mut agents = std::collections::HashMap::new();
        agents.insert("claude".to_string(), AgentSettings::default_for("claude"));
        agents.insert("codex".to_string(), AgentSettings::default_for("codex"));
        agents.insert("pi".to_string(), AgentSettings::default_for("pi"));
        agents.insert("dsh".to_string(), AgentSettings::default_for("dsh"));
        Self {
            user: UserSettings::default(),
            agents,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMeta {
    pub id: String,
    pub prompt: String,
    pub cwd: String,
    pub agent: String,
    /// Code run created without a project, using its own AgentCabin-managed cwd.
    #[serde(default)]
    pub code_standalone_task: bool,
    /// Product-level mode that owns this run. Missing legacy values default to Code.
    #[serde(default)]
    pub app_mode: AppMode,
    /// Explicit execution runtime target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_target: Option<AgentTarget>,
    /// Work Workspace that owns this run. None for Code runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_execution_context: Option<crate::work::models::ExecutionContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_preset: Option<crate::work::models::WorkPreset>,
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,
    pub status: RunStatus,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_subtype: Option<String>,
    /// The model used in this run (updated on hot-switch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The reasoning effort selected for this conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    /// The run_id this session was forked from (None if not a fork).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
    /// Historical context supplied to a fresh continuation session as a system prompt.
    /// Unlike a user prompt, this does not create an automatic model turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_context: Option<String>,
    /// User-assigned display name (None = use prompt as label).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Remote host name (references UserSettings.remote_hosts by name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_host_name: Option<String>,
    /// Snapshot of remote_cwd at run creation time (stable — not affected by later settings changes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_cwd: Option<String>,
    /// Full snapshot of RemoteHost config at run creation time.
    /// Used to restore remote sessions even if the host is renamed/deleted from settings.
    /// Falls back to name-based lookup for old runs that don't have this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_host_snapshot: Option<RemoteHost>,
    /// Snapshot of active_platform_id at run creation time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_id: Option<String>,
    /// Snapshot of anthropic_base_url at run creation time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_base_url: Option<String>,
    /// Run source (native or cli_import).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<RunSource>,
    /// CLI import watermark for incremental sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_import_watermark: Option<ImportWatermark>,
    /// Absolute path to CLI session JSONL file (read-only reference).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_session_path: Option<String>,
    /// True when CLI import couldn't reconstruct complete usage data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_usage_incomplete: Option<bool>,
    /// Legacy soft-delete timestamp (ISO 8601). New deletions remove the run directory;
    /// this field is retained only so older metadata remains hidden and migratable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    /// Snapshot of no_session_persistence at run creation time (metadata only — runtime
    /// resume gate uses current agent settings, not this snapshot).
    #[serde(default)]
    pub no_session_persistence: bool,
    /// App execution path for this run. Option on disk (backward compat); resolved via agent heuristic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_path: Option<ExecutionPath>,
    /// Unified resume identity. None = not resumable. Written by runtime events (session_init / thread.started).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_ref: Option<ConversationRef>,
    /// Codex process invocation counter. Incremented each run_agent() call for the same run.
    /// Used to scope item IDs across resume processes. None = not a Codex run or legacy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_process_seq: Option<u32>,
    /// Codex CLI import: list of rollout files imported into this run.
    /// Used by `sync_cli_session` to detect newly produced rollout files for the
    /// same thread (Codex resume produces a new rollout file rather than appending).
    /// None for Claude or non-imported runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_imported_rollouts: Option<Vec<CodexImportedRollout>>,
    /// Pinned to top of sidebar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    /// Archived (hidden from default sidebar view).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
    /// Unread badge indicator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unread: Option<bool>,
}

/// Codex rollout file that has been imported into a run.
///
/// `mtime_ns` is a stringified u128 (nanoseconds since UNIX epoch) — JS can't
/// safely represent u128 as a number, so we serialize as decimal string.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexImportedRollout {
    pub path: String,
    pub size: u64,
    pub mtime_ns: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_event_ts: Option<String>,
}

impl RunMeta {
    /// Resolve execution_path for old runs that don't have it on disk.
    /// Migration assumption: historically, Claude runs used session_actor,
    /// Codex runs used pipe_exec. Based on shipped product paths, not a protocol guarantee.
    pub fn resolved_execution_path(&self) -> ExecutionPath {
        self.execution_path.clone().unwrap_or_else(|| {
            if self.agent == "claude" {
                ExecutionPath::SessionActor
            } else {
                ExecutionPath::PipeExec
            }
        })
    }

    /// Resolve conversation_ref for old runs. Falls back to session_id using the owning agent.
    pub fn resolved_conversation_ref(&self) -> Option<ConversationRef> {
        self.conversation_ref.clone().or_else(|| {
            self.session_id.as_ref().map(|sid| {
                if self.agent == "codex" {
                    ConversationRef::CodexThread(sid.clone())
                } else if self.agent == "pi" {
                    ConversationRef::PiSession(sid.clone())
                } else if self.agent == "grok" {
                    ConversationRef::GrokSession(sid.clone())
                } else {
                    ConversationRef::ClaudeSession(sid.clone())
                }
            })
        })
    }

    pub fn is_work(&self) -> bool {
        self.app_mode == AppMode::Work
    }

    pub fn runtime_provider(
        &self,
    ) -> Result<crate::agent::capability_resolver::RuntimeProviderKind, String> {
        crate::agent::capability_resolver::RuntimeProviderKind::try_from_agent_str(&self.agent)
    }

    pub fn to_task_run(
        &self,
        last_activity_at: Option<String>,
        message_count: Option<u32>,
        last_message_preview: Option<String>,
    ) -> TaskRun {
        TaskRun {
            id: self.id.clone(),
            prompt: self.prompt.clone(),
            cwd: self.cwd.clone(),
            agent: self.agent.clone(),
            code_standalone_task: self.code_standalone_task,
            app_mode: self.app_mode,
            agent_target: Some(
                self.agent_target
                    .unwrap_or_else(|| AgentTarget::from_legacy(self.app_mode, &self.agent)),
            ),
            workspace_id: self.workspace_id.clone(),
            work_task_id: self.work_task_id.clone(),
            work_run_id: self.work_run_id.clone(),
            work_execution_context: self.work_execution_context,
            work_preset: self.work_preset,
            auth_mode: self.auth_mode.clone(),
            status: self.status.clone(),
            started_at: self.started_at.clone(),
            ended_at: self.ended_at.clone(),
            exit_code: self.exit_code,
            error_message: self.error_message.clone(),
            last_activity_at,
            message_count,
            last_message_preview,
            session_id: self.session_id.clone(),
            result_subtype: self.result_subtype.clone(),
            model: self.model.clone(),
            effort: self.effort.clone(),
            permission_mode: self.permission_mode.clone(),
            parent_run_id: self.parent_run_id.clone(),
            name: self.name.clone(),
            remote_host_name: self.remote_host_name.clone(),
            remote_cwd: self.remote_cwd.clone(),
            platform_id: self.platform_id.clone(),
            platform_base_url: self.platform_base_url.clone(),
            source: self.source.clone(),
            cli_import_watermark: self.cli_import_watermark.clone(),
            cli_session_path: self.cli_session_path.clone(),
            cli_usage_incomplete: self.cli_usage_incomplete,
            no_session_persistence: self.no_session_persistence,
            execution_path: self.resolved_execution_path(),
            conversation_ref: self.resolved_conversation_ref(),
            pinned: self.pinned,
            archived: self.archived,
            unread: self.unread,
        }
    }
}

impl TaskRun {
    pub fn is_work(&self) -> bool {
        self.app_mode == AppMode::Work
    }

    pub fn runtime_provider(
        &self,
    ) -> Result<crate::agent::capability_resolver::RuntimeProviderKind, String> {
        crate::agent::capability_resolver::RuntimeProviderKind::try_from_agent_str(&self.agent)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirListing {
    pub path: String,
    pub entries: Vec<DirEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatDelta {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatDone {
    pub ok: bool,
    pub code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    #[serde(rename = "type")]
    pub mime_type: String,
    pub size: u64,
    #[serde(rename = "contentBase64")]
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliCheckResult {
    pub agent: String,
    pub found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_supported: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRateLimitWindow {
    pub used_percent: f64,
    pub window_duration_mins: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRateLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<SubscriptionRateLimitWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<SubscriptionRateLimitWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_credits: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexAuthResult {
    pub installed: bool,
    pub version: Option<String>,
    pub logged_in: bool,
    pub auth_method: Option<String>,
    pub status_text: Option<String>,
    /// AgentCabin's own ChatGPT subscription OAuth state. This is deliberately
    /// separate from the Codex CLI status because the global Provider does not
    /// reuse Pi credentials or depend on the CLI keychain.
    #[serde(default)]
    pub subscription_logged_in: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription_account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription_rate_limits: Option<SubscriptionRateLimits>,
}

/// Metadata-only view of Pi's native OAuth credentials.
///
/// The credential values in `~/.pi/agent/auth.json` are intentionally never
/// returned to the frontend. This is only used to show whether Pi has a
/// ChatGPT/Codex login and which provider it belongs to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiAuthResult {
    pub logged_in: bool,
    pub provider: Option<String>,
    pub auth_type: Option<String>,
    pub status_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliDistTags {
    pub latest: Option<String>,
    pub stable: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInitStatus {
    pub cwd: String,
    pub has_claude_md: bool,
    #[serde(default)]
    pub has_agents_md: bool,
}

// ── Diagnostics report (run_diagnostics command) ──

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsReport {
    pub cli: CliDiagnostics,
    pub auth: AuthDiagnostics,
    pub project: ProjectDiagnostics,
    pub configs: ConfigDiagnostics,
    pub services: ServicesDiagnostics,
    pub system: SystemDiagnostics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codex: Option<CodexAuthResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliDiagnostics {
    pub found: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub latest: Option<String>,
    pub stable: Option<String>,
    pub auto_update_channel: Option<String>,
    pub ripgrep_available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthDiagnostics {
    pub has_oauth: bool,
    pub oauth_account: Option<String>,
    pub has_api_key: bool,
    pub api_key_hint: Option<String>,
    pub api_key_source: Option<String>,
    pub app_has_credentials: bool,
    pub app_platform_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectDiagnostics {
    pub cwd: String,
    pub has_claude_md: bool,
    pub claude_md_files: Vec<ClaudeMdInfo>,
    pub has_agents_md: bool,
    pub agents_md_files: Vec<AgentsMdInfo>,
    pub skipped_project_scope: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaudeMdInfo {
    pub path: String,
    pub size_chars: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentsMdInfo {
    pub path: String,
    pub size_chars: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigDiagnostics {
    pub settings_issues: Vec<ConfigIssue>,
    pub keybinding_issues: Vec<ConfigIssue>,
    pub mcp_issues: Vec<ConfigIssue>,
    pub env_var_issues: Vec<ConfigIssue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigIssue {
    pub scope: String,
    pub file: String,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServicesDiagnostics {
    pub community_registry: Option<bool>,
    pub mcp_registry: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemDiagnostics {
    pub sandbox_available: Option<bool>,
    pub lock_files: Vec<String>,
}

/// Raw usage data extracted from a run's events.jsonl (no RunMeta fields).
#[derive(Debug, Clone, Default)]
pub struct RawRunUsage {
    pub total_cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub duration_ms: u64,
    pub num_turns: u64,
    pub model_usage: HashMap<String, ModelUsageSummary>,
}

/// Per-run usage summary (RunMeta + usage data), returned by IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunUsageSummary {
    pub run_id: String,
    pub name: String,
    pub agent: String,
    pub model: Option<String>,
    pub status: RunStatus,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub total_cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub duration_ms: u64,
    pub num_turns: u64,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub model_usage: HashMap<String, ModelUsageSummary>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cost_estimated: bool,
}

/// Per-model token and cost summary.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsageSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cost_usd: f64,
}

/// Aggregated usage overview across all runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageOverview {
    pub total_cost_usd: f64,
    pub total_tokens: u64,
    pub total_runs: u32,
    pub avg_cost_per_run: f64,
    pub by_model: Vec<ModelAggregate>,
    pub daily: Vec<DailyAggregate>,
    pub runs: Vec<RunUsageSummary>,
    /// How the data was produced: "memory", "disk", "incremental", "full".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_mode: Option<String>,
    /// Number of days with activity.
    #[serde(default)]
    pub active_days: u32,
    /// Current consecutive active days (including today).
    #[serde(default)]
    pub current_streak: u32,
    /// Longest consecutive active days ever.
    #[serde(default)]
    pub longest_streak: u32,
}

/// Per-model aggregate stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelAggregate {
    pub model: String,
    pub runs: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cost_usd: f64,
    pub pct: f64,
}

/// Daily aggregate stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyAggregate {
    pub date: String,
    pub cost_usd: f64,
    pub runs: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Message count (Global mode — from Claude Code stats-cache).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_count: Option<u32>,
    /// Session count (Global mode — from Claude Code stats-cache).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_count: Option<u32>,
    /// Tool call count (Global mode — from Claude Code stats-cache).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_count: Option<u32>,
    /// Per-model token breakdown (populated for last 30 daily entries only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_breakdown: Option<std::collections::HashMap<String, ModelTokens>>,
}

/// Per-model token counts for a single day (stacked chart).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelTokens {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

// ── CLI Control Protocol types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliModelInfo {
    pub value: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(
        default,
        rename = "contextWindow",
        skip_serializing_if = "Option::is_none"
    )]
    pub context_window: Option<u64>,
    #[serde(
        default,
        rename = "supportsEffort",
        skip_serializing_if = "Option::is_none"
    )]
    pub supports_effort: Option<bool>,
    #[serde(
        default,
        rename = "supportedEffortLevels",
        skip_serializing_if = "Option::is_none"
    )]
    pub supported_effort_levels: Option<Vec<String>>,
    #[serde(
        default,
        rename = "supportsAdaptiveThinking",
        skip_serializing_if = "Option::is_none"
    )]
    pub supports_adaptive_thinking: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliCommand {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliAccount {
    #[serde(default, rename = "tokenSource")]
    pub token_source: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliInfo {
    pub models: Vec<CliModelInfo>,
    pub commands: Vec<CliCommand>,
    #[serde(default)]
    pub available_output_styles: Vec<String>,
    pub account: Option<CliAccount>,
    /// The model currently selected in Claude Code (from ~/.claude/settings.json)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_model: Option<String>,
    pub fetched_at: String,
}

/// Codex model catalog fetched live from `codex app-server` (model/list).
/// Unlike Claude (see CliInfo), Codex has no control protocol on the exec path,
/// so models are pulled via the experimental app-server JSON-RPC instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexModelList {
    pub models: Vec<CliModelInfo>,
    /// The model marked `isDefault` in the catalog — used when the user hasn't picked one.
    #[serde(rename = "defaultModel", skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliInfoError {
    pub code: String,
    pub message: String,
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

// ── Attachment limits ──
// Images: no app-side limit — CLI compresses via sharp (→ ≤3.75MB + ≤2000px).
pub const MAX_TEXT_SIZE: u64 = 10 * 1024 * 1024; // 10MB — text files
pub const MAX_PDF_BINARY_SIZE: u64 = 20 * 1024 * 1024; // 20MB — PDF binary inline (CLI dj6)
pub const ALLOWED_IMAGE_TYPES: &[&str] = &["image/png", "image/jpeg", "image/webp", "image/gif"];
pub const ALLOWED_DOC_TYPES: &[&str] = &["application/pdf"];

/// Max size for attachment by MIME type. Images: no limit, PDF: 20MB, text: 10MB.
pub fn max_attachment_size(mime: &str) -> u64 {
    if ALLOWED_IMAGE_TYPES.iter().any(|t| mime.starts_with(t)) {
        u64::MAX // CLI handles compression
    } else if ALLOWED_DOC_TYPES.contains(&mime) {
        MAX_PDF_BINARY_SIZE // 20MB for PDF (CLI dj6)
    } else {
        MAX_TEXT_SIZE // 10MB for text
    }
}

// ── Per-model usage breakdown ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsageEntry {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
    #[serde(default)]
    pub web_search_requests: u64,
    pub cost_usd: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    /// Maximum output tokens for this model (e.g. 32000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
}

// ── MCP server info ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ── Ralph Loop types ──

/// Reason a Ralph loop ended. Serializes to snake_case for frontend union matching.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RalphCompleteReason {
    MaxIterations,
    CompletionPromise,
    Cancelled,
    FailStopped,
}

/// Agent-neutral capability contract carried by session initialization events.
///
/// This is intentionally separate from protocol-specific feature state (for example
/// PiFeatureCapabilities). Protocol adapters may populate it once runtime negotiation
/// is available; until then SessionInit.capabilities remains None.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    #[serde(default)]
    pub protocol: AgentProtocolCapabilities,
    #[serde(default)]
    pub runtime: AgentRuntimeCapabilities,
    #[serde(default)]
    pub ui: AgentUiCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentProtocolCapabilities {
    #[serde(default)]
    pub session_load: bool,
    #[serde(default)]
    pub session_set_model: bool,
    #[serde(default)]
    pub session_mode_control: bool,
    #[serde(default)]
    pub permission_request: bool,
    #[serde(default)]
    pub permission_mode_control: bool,
    #[serde(default)]
    pub slash_commands: bool,
    #[serde(default)]
    pub plan_mode: bool,
    #[serde(default)]
    pub effort_control: bool,
    #[serde(default)]
    pub goal_state: bool,
    /// Whether the session exposes an authoritative structured task/Todo snapshot.
    #[serde(default)]
    pub structured_task_state: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeCapabilities {
    #[serde(default)]
    pub attachments: bool,
    #[serde(default)]
    pub remote: bool,
    #[serde(default)]
    pub fork: bool,
    #[serde(default)]
    pub steer: bool,
    /// Whether AgentCabin can safely queue a distinct follow-up turn while one is running.
    #[serde(default)]
    pub follow_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentUiCapabilities {
    #[serde(default)]
    pub slash_command_menu: bool,
    #[serde(default)]
    pub plan_mode_toggle: bool,
    #[serde(default)]
    pub goal_panel: bool,
    #[serde(default)]
    pub effort_selector: bool,
    #[serde(default)]
    pub permission_mode_switch: bool,
    #[serde(default)]
    pub add_dir_action: bool,
}

// ── Event Bus types ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionMode {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Agent-neutral structured task snapshot. Unlike tool-rendered TodoWrite events, this state is
/// authoritative for the session and each update replaces the previous snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StructuredTask {
    pub id: String,
    pub text: String,
    pub status: StructuredTaskStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StructuredTaskStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum BusEvent {
    SessionInit {
        run_id: String,
        session_id: Option<String>,
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        model_options: Vec<CliModelInfo>,
        tools: Vec<String>,
        cwd: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        slash_commands: Vec<Value>,
        /// Pi emits SessionInit once for get_state and once after get_commands.
        /// This distinguishes command discovery from general session readiness,
        /// including the valid empty-command-list case.
        #[serde(default)]
        commands_loaded: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mcp_servers: Vec<McpServerInfo>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "permissionMode"
        )]
        permission_mode: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "apiKeySource"
        )]
        api_key_source: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        claude_code_version: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_style: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        agents: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skills: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        plugins: Vec<Value>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        plugin_errors: Vec<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fast_mode_state: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capabilities: Option<AgentCapabilities>,
    },
    AgentModeUpdate {
        run_id: String,
        current_mode_id: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        available_modes: Vec<AgentSessionMode>,
    },
    StructuredTaskState {
        run_id: String,
        /// Full replacement snapshot. An empty vector authoritatively clears prior tasks.
        tasks: Vec<StructuredTask>,
    },
    WorkTaskState {
        run_id: String,
        /// Full replacement snapshot owned by the Work Harness.
        state: crate::work::models::WorkTaskState,
    },
    WorkContextPlanUpdated {
        run_id: String,
        plan: crate::work::context::WorkContextPlan,
    },
    WorkProjectionChanged {
        run_id: String,
        projection: crate::work::models::WorkProjection,
    },
    MessageDelta {
        run_id: String,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
    },
    MessageComplete {
        run_id: String,
        message_id: String,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
        /// Actual model used for this message (e.g. "claude-opus-4-6").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        /// Stop reason (v2.1.41: usually null; future: "end_turn", "tool_use").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stop_reason: Option<String>,
        /// Per-message token usage (raw JSON — result event has aggregated totals).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_usage: Option<Value>,
    },
    ToolStart {
        run_id: String,
        tool_use_id: String,
        tool_name: String,
        input: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
    },
    ToolEnd {
        run_id: String,
        tool_use_id: String,
        tool_name: String,
        output: Value,
        status: String,
        duration_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
        /// Structured tool result metadata from CLI verbose mode (e.g. file info for Read)
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_result: Option<Value>,
    },
    UserMessage {
        run_id: String,
        text: String,
        /// CLI-assigned UUID (Claude actor path).
        #[serde(skip_serializing_if = "Option::is_none")]
        uuid: Option<String>,
        /// Frontend-generated UUID for optimistic dedup (Codex pipe path).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_uuid: Option<String>,
        /// Attachment metadata (names/types/sizes — no content).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<AttachmentMeta>,
    },
    RunState {
        run_id: String,
        state: String,
        exit_code: Option<i32>,
        error: Option<String>,
    },
    UsageUpdate {
        run_id: String,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: Option<u64>,
        cache_write_tokens: Option<u64>,
        total_cost_usd: f64,
        /// Backend-authoritative turn index (1-based). Injected by session_actor for user turns.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_index: Option<u32>,
        /// Current prompt context size when the agent reports it separately from cumulative usage.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_tokens: Option<u64>,
        /// Context window reported by the agent for this usage snapshot.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_window: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model_usage: Option<HashMap<String, ModelUsageEntry>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_api_ms: Option<u64>,
        /// Total duration including hooks/overhead (from result event).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
        /// Number of turns in this session (from result event).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        num_turns: Option<u64>,
        /// Stop reason from result event (v2.1.41: usually null).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stop_reason: Option<String>,
        /// Service tier (e.g. "standard").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        service_tier: Option<String>,
        /// Speed tier (e.g. "standard").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        speed: Option<String>,
        /// Web fetch request count (from usage.server_tool_use).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        web_fetch_requests: Option<u64>,
        /// 5-minute ephemeral cache creation tokens.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_creation_5m: Option<u64>,
        /// 1-hour ephemeral cache creation tokens.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_creation_1h: Option<u64>,
    },
    Raw {
        run_id: String,
        source: String,
        data: Value,
    },
    PermissionDenied {
        run_id: String,
        tool_name: String,
        tool_use_id: String,
        tool_input: Value,
    },
    /// Thinking/reasoning text delta (from extended thinking).
    ThinkingDelta {
        run_id: String,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
    },
    /// Partial JSON input for a tool being invoked (real-time streaming).
    ToolInputDelta {
        run_id: String,
        tool_use_id: String,
        partial_json: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
    },
    /// Inline permission prompt from `--permission-prompt-tool stdio`.
    /// CLI is waiting for a control_response with allow/deny.
    PermissionPrompt {
        run_id: String,
        request_id: String,
        tool_name: String,
        tool_use_id: String,
        tool_input: Value,
        decision_reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        suggestions: Vec<Value>,
    },
    /// Context compaction boundary — CLI auto-compressed the conversation context.
    CompactBoundary {
        run_id: String,
        trigger: String,
        pre_tokens: Option<u64>,
    },
    /// A visible boundary between the source agent's transcript and a new agent branch.
    AgentHandoff {
        run_id: String,
        source_agent: String,
        target_agent: String,
        source_run_id: String,
    },
    /// System status change (e.g. "compacting").
    SystemStatus {
        run_id: String,
        /// CLI status string, e.g. "compacting", null for cleared
        status: Option<String>,
        data: Value,
    },
    /// Structured UI state emitted by a Pi extension. Unlike command output, these values are
    /// retained as the current extension surface (status/widget/title/editor text) and replaced
    /// by subsequent updates.
    PiExtensionUi {
        run_id: String,
        method: String,
        data: Value,
    },
    /// Structured context composition emitted by AgentCabin's built-in Pi extension.
    /// It contains counts and category ids only; prompt/tool contents never leave Pi.
    PiContextUsage {
        run_id: String,
        snapshot: Value,
    },
    PiExtensionUiRequestCreated {
        run_id: String,
        request: PiExtensionUiRequest,
    },
    PiExtensionUiRequestResolved {
        run_id: String,
        request_id: String,
    },
    PiExtensionNotice {
        run_id: String,
        notice_type: String,
        message: String,
    },
    PiExtensionStateSync {
        run_id: String,
        snapshot: PiExtensionHostSnapshot,
    },
    PiExtensionEditorAction {
        run_id: String,
        text: String,
    },
    PiFeatureCapabilities {
        run_id: String,
        capabilities: PiFeatureCapabilities,
    },
    PiPlanState {
        run_id: String,
        state: PiPlanState,
    },
    PiGoalState {
        run_id: String,
        state: PiGoalState,
    },
    PiTodoState {
        run_id: String,
        state: PiTodoState,
    },
    PiPermissionState {
        run_id: String,
        state: PiPermissionState,
    },
    PiQueueUpdated {
        run_id: String,
        state: PiMessageQueueState,
    },
    PiSessionEntries {
        run_id: String,
        entries: Vec<Value>,
        leaf_id: Option<String>,
        last_entry_id: Option<String>,
    },
    PiSessionTree {
        run_id: String,
        roots: Vec<PiSessionTreeNode>,
        leaf_id: Option<String>,
    },

    /// Hook execution started.
    HookStarted {
        run_id: String,
        hook_event: String,
        hook_id: String,
        data: Value,
        /// Hook name (e.g. "SessionStart:startup").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hook_name: Option<String>,
    },
    /// Hook execution progress.
    HookProgress {
        run_id: String,
        hook_id: String,
        data: Value,
    },
    /// Hook execution completed with result.
    HookResponse {
        run_id: String,
        hook_id: String,
        hook_event: String,
        outcome: String,
        data: Value,
        /// Hook name (e.g. "SessionStart:startup").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hook_name: Option<String>,
        /// Hook stdout.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stdout: Option<String>,
        /// Hook stderr.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stderr: Option<String>,
        /// Hook process exit code.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },
    /// Background task notification (file indexing, MCP init, etc.).
    TaskNotification {
        run_id: String,
        task_id: String,
        status: String,
        data: Value,
    },
    /// Files persisted notification.
    FilesPersisted {
        run_id: String,
        files: Value,
        data: Value,
    },
    /// Tool progress update (real-time elapsed time).
    /// Top-level event type "tool_progress" (not a content_block_delta subtype).
    ToolProgress {
        run_id: String,
        tool_use_id: String,
        elapsed_time_seconds: Option<f64>,
        data: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
    },
    /// Incremental tool output chunk — top-level event type "tool_output_delta".
    /// Appends to an open tool card's output (keyed by `tool_use_id`) so command
    /// output streams live instead of only appearing at completion. Codex source:
    /// `item/commandExecution/outputDelta`.
    ToolOutputDelta {
        run_id: String,
        tool_use_id: String,
        delta: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
    },
    /// Codex thread goal update — top-level event type "goal_update". Carries the `ThreadGoal`
    /// object verbatim (`{threadId, objective, status, tokenBudget?, tokensUsed,
    /// timeUsedSeconds, createdAt, updatedAt}`) from the `thread/goal/updated` notification, or
    /// `Value::Null` when the goal was cleared (`thread/goal/cleared`). The GoalPanel renders it.
    GoalUpdate {
        run_id: String,
        goal: Value,
    },
    /// Codex hook lifecycle (`hook/started` → status "running", `hook/completed` → terminal
    /// HookRunStatus). `hook_id` (= run.id) is stable across the pair so the frontend upserts a
    /// single timeline card. `event_name` is the camelCase HookEventName (e.g. "preToolUse").
    CodexHookRun {
        run_id: String,
        hook_id: String,
        event_name: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        status_message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    /// Codex MCP server startup-state change (`mcpServer/startupStatus/updated`). Live push so the
    /// MCP status panel updates without a manual refresh. `status` is the raw Codex
    /// McpServerStartupState ("starting"|"ready"|"failed"|"cancelled"); the frontend maps it to the
    /// panel vocabulary. Not replayed — ephemeral session state, like goal updates.
    CodexMcpStatus {
        run_id: String,
        name: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Codex turn-level aggregated unified diff (`turn/diff/updated`). `diff` is the cumulative
    /// diff across all file changes in the turn; later pushes supersede earlier ones (latest
    /// wins). Not replayed — ephemeral live state cleared at the next turn, like goal/mcp status.
    CodexTurnDiff {
        run_id: String,
        turn_id: String,
        diff: String,
    },
    /// Durable, agent-neutral file patch captured after one completed conversation turn.
    /// It is replayed into the timeline so every turn retains its own summary card.
    TurnFileSummary {
        run_id: String,
        summary_id: String,
        cwd: String,
        diff: String,
    },
    /// Tool use summary — top-level event type "tool_use_summary".
    ToolUseSummary {
        run_id: String,
        tool_use_id: String,
        summary: String,
        preceding_tool_use_ids: Vec<String>,
        data: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
    },
    /// Authentication status update.
    AuthStatus {
        run_id: String,
        is_authenticating: bool,
        output: Vec<String>,
        data: Value,
    },
    /// Hook callback control_request — CLI requests hook execution/approval.
    /// Analogous to PermissionPrompt (needs a control_response).
    HookCallback {
        run_id: String,
        request_id: String,
        hook_event: String,
        hook_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        hook_name: Option<String>,
        data: Value,
    },
    /// CLI cancelled a pending control_request (e.g. cancelled permission prompt).
    ControlCancelled {
        run_id: String,
        request_id: String,
    },
    /// Output from a CLI slash command (e.g. /context, /cost).
    /// Extracted from `<local-command-stdout>` tags in user messages.
    CommandOutput {
        run_id: String,
        content: String,
    },
    /// MCP elicitation: CLI requests user input for MCP server authentication/configuration.
    ElicitationPrompt {
        run_id: String,
        request_id: String,
        mcp_server_name: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        elicitation_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_schema: Option<Value>,
    },
    /// Rate limit event — emitted when API rate limit status changes.
    RateLimitEvent {
        run_id: String,
        /// Rate limit status: "allowed", "allowed_warning", "rejected"
        status: String,
        /// When the rate limit window resets (epoch seconds).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resets_at: Option<f64>,
        /// Which limit: "five_hour", "seven_day", etc.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rate_limit_type: Option<String>,
        /// Utilization percentage (0.0-1.0).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        utilization: Option<f64>,
        data: Value,
    },
    /// Ralph loop started — carries full config for replay.
    RalphStarted {
        run_id: String,
        prompt: String,
        max_iterations: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        completion_promise: Option<String>,
        started_at: String,
    },
    /// Ralph loop iteration completed (not the final one).
    RalphIteration {
        run_id: String,
        iteration: u32,
        max_iterations: u32,
    },
    /// Ralph loop ended.
    RalphComplete {
        run_id: String,
        reason: RalphCompleteReason,
        iteration: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    #[default]
    New,
    Resume,
    Continue,
    Fork,
}

// ── Agent Team Mode types ──
// Read from ~/.claude/teams/ and ~/.claude/tasks/ (Claude Code team collaboration)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "createdAt", default)]
    pub created_at: u64,
    #[serde(rename = "leadAgentId", default)]
    pub lead_agent_id: String,
    #[serde(rename = "leadSessionId", default)]
    pub lead_session_id: String,
    #[serde(default)]
    pub members: Vec<TeamMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub name: String,
    #[serde(rename = "agentType", default)]
    pub agent_type: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub color: String,
    #[serde(rename = "planModeRequired", default)]
    pub plan_mode_required: bool,
    #[serde(rename = "joinedAt", default)]
    pub joined_at: u64,
    #[serde(rename = "tmuxPaneId", default)]
    pub tmux_pane_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub subscriptions: Vec<String>,
    #[serde(rename = "backendType", default)]
    pub backend_type: String,
    /// The prompt given to spawned teammates (not present on leader)
    #[serde(default)]
    pub prompt: String,
    /// Runtime active status (set by setMemberActive in Claude Code SDK)
    #[serde(rename = "isActive", default)]
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamInboxMessage {
    #[serde(default)]
    pub from: String,
    pub text: String,
    #[serde(default)]
    pub summary: String,
    pub timestamp: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub read: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamTask {
    pub id: String,
    pub subject: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "activeForm", default)]
    pub active_form: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub blocks: Vec<String>,
    #[serde(rename = "blockedBy", default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamSummary {
    pub name: String,
    pub description: String,
    pub member_count: usize,
    pub task_count: usize,
    pub created_at: u64,
}

// ── Plugin types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplacePlugin {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub author: Option<PluginAuthor>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    /// Raw source — string for local ("./plugins/name"), object for external
    #[serde(default)]
    pub source: Option<serde_json::Value>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub strict: Option<bool>,
    #[serde(default, rename = "lspServers")]
    pub lsp_servers: Option<serde_json::Value>,
    // ── Fields enriched by our code (not from marketplace.json) ──
    #[serde(default)]
    pub marketplace_name: Option<String>,
    #[serde(default)]
    pub install_count: Option<u64>,
    /// Components discovered by scanning plugin subdirectories
    #[serde(default)]
    pub components: PluginComponents,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAuthor {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginComponents {
    pub skills: Vec<String>,
    pub commands: Vec<String>,
    pub agents: Vec<String>,
    pub hooks: bool,
    pub mcp_servers: Vec<String>,
    pub lsp_servers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceInfo {
    pub name: String,
    pub source: serde_json::Value,
    pub install_location: String,
    pub last_updated: Option<String>,
    pub plugin_count: usize,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// Source kind for Codex skills (mirrors loader.rs scan order).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SkillSourceKind {
    User,          // $HOME/.agents/skills/
    ProjectAgents, // {layer}/.agents/skills/
    ProjectCodex,  // {layer}/.codex/skills/
    Legacy,        // $CODEX_HOME/skills/ (non .system/)
    Bundled,       // $CODEX_HOME/skills/.system/
}

/// How a Codex skill was disabled.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SkillDisabledBy {
    Path,    // path rule match
    Name,    // name rule match
    Bundled, // [skills.bundled] enabled=false
}

fn default_agent_claude() -> String {
    "claude".to_string()
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandaloneSkill {
    pub name: String,
    pub description: String,
    pub path: String,
    /// "user" or "project" or "system"
    #[serde(default)]
    pub scope: String,
    #[serde(default = "default_agent_claude")]
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<SkillSourceKind>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_by: Option<SkillDisabledBy>,
    #[serde(default = "default_true")]
    pub can_edit: bool,
    #[serde(default = "default_true")]
    pub can_delete: bool,
    #[serde(default = "default_true")]
    pub can_toggle: bool,
}

impl Default for StandaloneSkill {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            path: String::new(),
            scope: String::new(),
            agent: "claude".into(),
            source_kind: None,
            enabled: true,
            disabled_by: None,
            can_edit: true,
            can_delete: true,
            can_toggle: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstalledPlugin {
    #[serde(default, alias = "id")]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub marketplace: Option<String>,
    #[serde(default, rename = "pluginId")]
    pub plugin_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Project directory this plugin was installed in (project/local scope only).
    #[serde(
        default,
        rename = "projectPath",
        skip_serializing_if = "Option::is_none"
    )]
    pub project_path: Option<String>,
    /// Catch-all for unknown fields
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// A plugin exposed by a Codex marketplace. Unlike `InstalledPlugin`, this
/// also represents entries that are available but not installed yet.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CodexPluginInfo {
    pub plugin_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub marketplace_name: Option<String>,
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub install_policy: Option<String>,
    #[serde(default)]
    pub auth_policy: Option<String>,
    #[serde(default)]
    pub source: Option<serde_json::Value>,
    #[serde(default)]
    pub marketplace_source: Option<serde_json::Value>,
}

/// A marketplace known to Codex CLI.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CodexMarketplace {
    pub name: String,
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub marketplace_source: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginOperationResult {
    pub success: bool,
    pub message: String,
}

// ── Community skill types ──

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CommunitySkillResult {
    pub id: String,
    pub name: String,
    pub skill_id: String,
    pub installs: u64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CommunitySkillDetail {
    pub id: String,
    pub name: String,
    pub description: String,
    pub installs: u64,
    pub source: String,
    pub content: Option<String>,
    pub raw_url: Option<String>,
    pub skills_sh_url: Option<String>,
    pub github_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub available: bool,
    pub reason: Option<String>,
}

// ── Pi Package Catalog types ──

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PiPackageItem {
    pub name: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub downloads_formatted: String,
    pub time_ago: String,
    pub date: u64,
    pub npm_url: String,
    pub repo_url: String,
    pub package_path: String,
    pub install_source: String,
    pub types: Vec<String>,
    pub is_recent: bool,
}

// ── MCP Registry API response types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRegistrySearchResult {
    pub servers: Vec<McpRegistryServer>,
    pub next_cursor: Option<String>,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRegistryServer {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub packages: Vec<McpRegistryPackage>,
    #[serde(default)]
    pub remotes: Vec<McpRegistryRemote>,
    #[serde(default)]
    pub repository: Option<McpRegistryRepository>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRegistryPackage {
    #[serde(default)]
    pub registry_type: String,
    #[serde(default)]
    pub identifier: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub environment_variables: Vec<McpRegistryEnvVar>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRegistryRemote {
    #[serde(rename = "type", default)]
    pub remote_type: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: Vec<McpRegistryHeader>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRegistryEnvVar {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_required: Option<bool>,
    #[serde(default)]
    pub is_secret: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRegistryHeader {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub is_required: Option<bool>,
    #[serde(default)]
    pub is_secret: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRegistryRepository {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

// ── Configured MCP server ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfiguredMcpServer {
    pub name: String,
    pub server_type: String,
    pub scope: String,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub env_keys: Vec<String>,
    #[serde(default)]
    pub header_keys: Vec<String>,
    #[serde(default = "default_agent_claude")]
    pub agent: String,
    /// Absolute path of the native config file this entry was read from.
    #[serde(default)]
    pub source_path: Option<String>,
}

impl Default for ConfiguredMcpServer {
    fn default() -> Self {
        Self {
            name: String::new(),
            server_type: String::new(),
            scope: String::new(),
            command: None,
            args: vec![],
            url: None,
            env_keys: vec![],
            header_keys: vec![],
            agent: "claude".into(),
            source_path: None,
        }
    }
}

// ── Keybinding types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindingOverride {
    pub command: String,
    pub key: String,
}

// ── Onboarding types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshKeyInfo {
    pub key_path: String,
    pub key_path_expanded: String,
    pub pub_key_path: String,
    pub key_type: String,
    pub exists: bool,
    pub pub_exists: bool,
    pub ssh_copy_id_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthCheckResult {
    pub has_oauth: bool,
    pub has_api_key: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_account: Option<String>,
}

/// Overview of all three authentication sources (configuration state only — no effective_source inference).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthOverview {
    /// User-configured mode: "cli" or "api"
    pub auth_mode: String,
    /// CLI Login (OAuth) available via `claude auth status`
    pub cli_login_available: bool,
    /// CLI Login account email (if logged in)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_login_account: Option<String>,
    /// CLI API Key detected from settings/env/shell config
    pub cli_has_api_key: bool,
    /// Hint of the CLI API key (last 4 chars)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_api_key_hint: Option<String>,
    /// Source of CLI API key: "settings", "env", "shell_config", or None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_api_key_source: Option<String>,
    /// App has platform credentials configured
    pub app_has_credentials: bool,
    /// Active platform ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_platform_id: Option<String>,
    /// Active platform display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_platform_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallMethod {
    pub id: String,
    pub name: String,
    pub command: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ── Prompt search & favorites ──

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptSearchResult {
    pub run_id: String,
    pub run_name: Option<String>,
    pub run_prompt: String,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_target: Option<AgentTarget>,
    pub model: Option<String>,
    pub status: RunStatus,
    pub started_at: String,
    pub matched_text: String,
    pub matched_seq: u64,
    pub matched_ts: String,
    /// Stable event ID: uuid (user_message) or message_id (message_complete).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_event_id: Option<String>,
    pub is_favorite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptFavorite {
    pub run_id: String,
    pub seq: u64,
    pub text: String,
    pub tags: Vec<String>,
    pub note: String,
    pub created_at: String,
}

// ── History search ──

/// History 页面搜索过滤条件
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSearchFilters {
    pub query: Option<String>,
    pub projects: Option<Vec<String>>,
    pub tools: Option<Vec<String>>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub cost_min: Option<f64>,
    pub cost_max: Option<f64>,
    pub statuses: Option<Vec<RunStatus>>,
    pub has_errors: Option<bool>,
    pub agents: Option<Vec<String>>,
    pub agent_target: Option<String>,
    pub realm: Option<String>,
    pub sort_by: Option<String>,
    pub sort_asc: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// History 搜索结果条目
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSearchResult {
    pub run_id: String,
    pub cwd: String,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_target: Option<AgentTarget>,
    pub model: Option<String>,
    pub status: RunStatus,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub name: Option<String>,
    pub prompt_preview: String,
    pub tools_used: Vec<String>,
    pub tool_call_count: u32,
    pub files_touched_count: u32,
    pub total_cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub duration_ms: u64,
    pub num_turns: u64,
    pub has_errors: bool,
    pub error_summary: Option<String>,
}

/// Facet 统计（用于 filter UI 下拉选项）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FacetCount {
    pub value: String,
    pub count: usize,
}

/// History 页面 facets
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSearchFacets {
    pub projects: Vec<FacetCount>,
    pub tools: Vec<FacetCount>,
    pub agents: Vec<FacetCount>,
    pub cost_range: [f64; 2],
    pub date_range: [String; 2],
    pub total_runs: usize,
    pub total_cost: f64,
}

/// History 搜索响应（结果 + facets + 总数）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSearchResponse {
    pub results: Vec<RunSearchResult>,
    pub facets: RunSearchFacets,
    pub total_matching: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_target_legacy_mapping() {
        assert_eq!(
            AgentTarget::from_legacy(AppMode::Work, "pi"),
            AgentTarget::Work
        );
        assert_eq!(
            AgentTarget::from_legacy(AppMode::Code, "pi"),
            AgentTarget::PiCode
        );
        assert_eq!(
            AgentTarget::from_legacy(AppMode::Code, "codex"),
            AgentTarget::NativeCodex
        );
        assert_eq!(
            AgentTarget::from_legacy(AppMode::Code, "grok"),
            AgentTarget::NativeGrok
        );
        assert_eq!(
            AgentTarget::from_legacy(AppMode::Code, "claude"),
            AgentTarget::NativeClaude
        );
    }

    #[test]
    fn test_agent_target_serde() {
        let json = serde_json::to_string(&AgentTarget::NativeCodex).unwrap();
        assert_eq!(json, "\"native:codex\"");
        let deserialized: AgentTarget = serde_json::from_str("\"pi:code\"").unwrap();
        assert_eq!(deserialized, AgentTarget::PiCode);
        let legacy_work: AgentTarget = serde_json::from_str("\"pi:work\"").unwrap();
        assert_eq!(legacy_work, AgentTarget::Work);
        assert_eq!(
            serde_json::to_string(&AgentTarget::Work).unwrap(),
            "\"work\""
        );
    }

    #[test]
    fn test_agent_target_realm_checks() {
        assert!(AgentTarget::NativeCodex.is_native());
        assert!(AgentTarget::NativeClaude.is_native());
        assert!(AgentTarget::NativeGrok.is_native());
        assert!(!AgentTarget::PiCode.is_native());
        assert!(!AgentTarget::Work.is_native());

        assert!(AgentTarget::PiCode.is_pi());
        assert!(!AgentTarget::Work.is_pi());
        assert!(!AgentTarget::NativeCodex.is_pi());
    }
}
