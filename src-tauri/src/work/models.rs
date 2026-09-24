use serde::{Deserialize, Serialize};

use crate::agent::capability_resolver::RuntimeProviderKind;
use crate::models::StructuredTask;

pub use crate::work::context::{
    WorkContextKind, WorkContextPlan, WorkContextSegment, WorkContextSelection, WorkContextSource,
};

/// Product-level mode. This is intentionally separate from `SessionMode`,
/// which describes a session lifecycle operation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AppMode {
    #[default]
    Code,
    Work,
}

impl AppMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Work => "work",
        }
    }
}

/// Work's user-facing task preset. Presets shape guidance and capability
/// defaults; they do not create a separate runtime or Harness.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkPreset {
    #[default]
    Office,
    Code,
    Creative,
}

impl WorkPreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Office => "office",
            Self::Code => "code",
            Self::Creative => "creative",
        }
    }
}

/// Serde helper for `WorkProfile.runtime`: maps canonical and legacy JSON values to `RuntimeProviderKind`.
///
/// Rules:
///   - Missing or empty field → defaults to `RuntimeProviderKind::Pi` (backward compatibility)
///   - "pi"                   → `RuntimeProviderKind::Pi`
///   - "claude" | "claude-code" → `RuntimeProviderKind::Claude`
///   - "codex"                → `RuntimeProviderKind::Codex`
///   - "grok"                 → `RuntimeProviderKind::Grok`
///   - "dsh"                  → `RuntimeProviderKind::Dsh`
///   - Unknown values ("abc") → Err (fail closed, never silently launch Pi)
pub fn work_runtime_provider_default() -> RuntimeProviderKind {
    RuntimeProviderKind::Pi
}

pub fn deserialize_work_runtime_provider<'de, D>(
    deserializer: D,
) -> Result<RuntimeProviderKind, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    let s = match opt {
        Some(s) if !s.trim().is_empty() => s,
        _ => return Ok(RuntimeProviderKind::Pi),
    };
    match s.trim().to_ascii_lowercase().as_str() {
        "pi" => Ok(RuntimeProviderKind::Pi),
        "claude-code" | "claude" => Ok(RuntimeProviderKind::Claude),
        "codex" => Ok(RuntimeProviderKind::Codex),
        "grok" => Ok(RuntimeProviderKind::Grok),
        "dsh" => Ok(RuntimeProviderKind::Dsh),
        other => Err(serde::de::Error::custom(format!(
            "Unsupported or unknown Work runtime provider: '{other}'"
        ))),
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkResourceKind {
    Skill,
    Capability,
    Connector,
    PiExtension,
    ArtifactTool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkArtifactStatus {
    Creating,
    Ready,
    Invalid,
    Failed,
    Validated,
    Delivered,
}

/// Normalized artifact category for the Artifact Hub. This is derived from the
/// file extension at read time and never persisted, so both Rust and the Node
/// runtime extension registry writers always agree on the classification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkArtifactCategory {
    Document,
    Spreadsheet,
    Presentation,
    Pdf,
    Image,
    Html,
    Code,
    Archive,
    #[default]
    Other,
}

/// How the frontend should present a preview for this artifact. Mirrors the
/// dispatch already implemented by ArtifactPreviewModal on the web side, so
/// the Hub can decide affordances ("Preview" vs "Open") from one authority.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkArtifactPreviewKind {
    Pdf,
    Image,
    Video,
    Markdown,
    Html,
    Csv,
    Office,
    Mermaid,
    Code,
    #[default]
    Binary,
}

/// How one file changed as the result of a successful Work tool execution.
/// Recorded as the durable `file_changed` ledger fact; the Run Receipt reads
/// it back to produce the Changes view without re-diffing the workspace.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkFileChangeKind {
    Created,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkResourceDiscovery {
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub verbs: Vec<String>,
    #[serde(default)]
    pub nouns: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub guidance: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkExecutionRuntime {
    #[default]
    Python,
    Node,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionNetworkPolicy {
    #[default]
    None,
    ConnectorOnly,
    Internet,
}

fn default_execution_timeout() -> u64 {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkExecutionManifest {
    #[serde(default)]
    pub trusted: bool,
    pub runtime: WorkExecutionRuntime,
    pub entry: String,
    #[serde(default)]
    pub actions: Vec<String>,
    /// Per-action risk overrides for capability executions (`work_execute`).
    /// Actions not listed here keep the tool-level classification (Exec, fail-closed).
    #[serde(default)]
    pub action_risk_classes: std::collections::HashMap<String, ToolRiskClass>,
    #[serde(default)]
    pub network: ExecutionNetworkPolicy,
    #[serde(default = "default_execution_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub readable_areas: Vec<String>,
    #[serde(default)]
    pub writable_areas: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResourceOrigin {
    #[default]
    Builtin,
    User,
    Community,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkResourceManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub kind: WorkResourceKind,
    #[serde(default)]
    pub origin: ResourceOrigin,
    #[serde(default)]
    pub modes: Vec<AppMode>,
    #[serde(default)]
    pub runtimes: Vec<RuntimeProviderKind>,
    #[serde(default)]
    pub entry: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default = "default_resource_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub discovery: WorkResourceDiscovery,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<WorkExecutionManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkResourceSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: WorkResourceKind,
    #[serde(default)]
    pub origin: ResourceOrigin,
    pub entry: String,
    pub permissions: Vec<String>,
    pub enabled: bool,
    pub active: bool,
    /// Whether this resource can be projected into the selected Work runtime.
    /// This is separate from `active`: a compatible resource may still be disabled.
    #[serde(default = "default_runtime_available")]
    pub runtime_available: bool,
    pub discovery: WorkResourceDiscovery,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<WorkExecutionManifest>,
}

fn default_runtime_available() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkCapabilityMatch {
    pub resource: WorkResourceSummary,
    pub score: u32,
    pub matched_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkConnectorSummary {
    pub name: String,
    pub transport: String,
    pub enabled: bool,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env_keys: Vec<String>,
    pub header_keys: Vec<String>,
    /// Legacy aggregate retained for older clients. It is true when either
    /// the Pi adapter or the DSH Work bridge can host this connector.
    pub runtime_available: bool,
    #[serde(default)]
    pub pi_runtime_available: bool,
    #[serde(default)]
    pub dsh_runtime_available: bool,
    /// Whether the legacy Pi MCP adapter package is installed.
    pub adapter_installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkConnectorHealthStatus {
    Disabled,
    Healthy,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkConnectorHealth {
    pub name: String,
    pub status: WorkConnectorHealthStatus,
    pub message: String,
    pub checked_at: String,
    pub latency_ms: u64,
    pub tool_count: u32,
    pub tool_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkBrowserConfig {
    pub enabled: bool,
    pub provider: String,
    pub max_results: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_url: Option<String>,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkBrowserHealthStatus {
    Disabled,
    Unconfigured,
    Healthy,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkBrowserHealth {
    pub status: WorkBrowserHealthStatus,
    pub provider: String,
    pub message: String,
    pub checked_at: String,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkBrowserSummary {
    pub enabled: bool,
    pub provider: String,
    pub configured: bool,
    pub adapter_installed: bool,
    #[serde(default)]
    pub pi_runtime_available: bool,
    #[serde(default)]
    pub dsh_runtime_available: bool,
    pub runtime_available: bool,
    pub browser_runtime_node_available: bool,
    pub browser_runtime_available: bool,
    pub playwright_installed: bool,
    pub chromium_installed: bool,
    pub browser_runtime_managed: bool,
    pub browser_runtime_message: String,
    pub max_results: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_url: Option<String>,
    #[serde(default)]
    pub auth_kind: String,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkFileSummary {
    pub path: String,
    pub name: String,
    pub area: String,
    pub size: u64,
    pub modified_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkScratchCleanupResult {
    pub retention_days: u32,
    pub cutoff_at: String,
    pub dry_run: bool,
    pub candidates: Vec<WorkFileSummary>,
    pub removed: Vec<WorkFileSummary>,
    pub removed_bytes: u64,
}

fn default_resource_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkProfile {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// The canonical runtime provider for this Work profile.
    ///
    /// Serializes as the provider's lowercase string ("pi", "claude", etc.).
    /// Old profile.json files used a `WorkRuntime` kebab-case enum; the
    /// custom deserializer maps legacy values so existing workspaces are
    /// never broken by this migration.
    #[serde(
        default = "crate::work::models::work_runtime_provider_default",
        deserialize_with = "crate::work::models::deserialize_work_runtime_provider"
    )]
    pub runtime: RuntimeProviderKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkRulesInfo {
    pub path: String,
    pub content: String,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkAccessRoot {
    pub path: String,
    #[serde(default)]
    pub writable: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkRootKind {
    #[default]
    Managed,
    LocalFolder,
}

/// Controls where the reserved `output/` area is physically stored.
///
/// The managed default keeps Work's internal state out of a user-selected
/// project. `PrimaryWorkRoot` is an explicit opt-in for users who want newly
/// generated deliverables to appear directly under that project's `output/`
/// directory.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkArtifactStorageMode {
    #[default]
    Managed,
    PrimaryWorkRoot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkWorkspace {
    pub id: String,
    pub name: String,
    pub root: String,
    pub input_dir: String,
    pub scratch_dir: String,
    pub output_dir: String,
    pub context_dir: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub artifact_count: u32,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub access_roots: Vec<WorkAccessRoot>,
    /// Workspace-level default policy. New tasks inherit this when no explicit
    /// policy is provided. It also acts as the autonomy ceiling: task-level
    /// execution_mode cannot be more permissive than this (clamp enforced on update).
    #[serde(default)]
    pub default_policy: WorkPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<String>,
    #[serde(default)]
    pub root_kind: WorkRootKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_work_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_root_valid: Option<bool>,
    #[serde(default)]
    pub artifact_storage_mode: WorkArtifactStorageMode,
}

impl WorkWorkspace {
    /// Return the raw configured primary work root path string, or managed root if Managed.
    pub fn configured_primary_work_root(&self) -> &str {
        match self.root_kind {
            WorkRootKind::Managed => &self.root,
            WorkRootKind::LocalFolder => self.primary_work_root.as_deref().unwrap_or(&self.root),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactProducer {
    // Pi's extension writes the on-disk registry using snake_case names.
    // Accept both spellings while Rust keeps its canonical camelCase shape.
    #[serde(alias = "run_id")]
    pub run_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "producer_tool_call_id"
    )]
    pub producer_tool_call_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "execution_id"
    )]
    pub execution_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactVerificationEvidence {
    #[serde(alias = "verified_at")]
    pub verified_at: String,
    #[serde(alias = "validator_version")]
    pub validator_version: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub checks: Vec<String>,
    #[serde(alias = "verification_status")]
    pub verification_status: WorkArtifactStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactSourceRef {
    #[serde(alias = "source_tool_call_id")]
    pub source_tool_call_id: String,
    #[serde(alias = "source_type")]
    pub source_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(alias = "result_digest")]
    pub result_digest: String,
    #[serde(alias = "captured_at")]
    pub captured_at: String,
    #[serde(default, alias = "is_claimed")]
    pub is_claimed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkArtifactSummary {
    pub id: String,
    pub workspace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub artifact_type: String,
    /// Normalized category derived from the extension (Artifact Hub grouping).
    #[serde(default)]
    pub category: WorkArtifactCategory,
    /// MIME type derived from the extension at read time.
    #[serde(default = "default_artifact_mime_type")]
    pub mime_type: String,
    pub title: String,
    pub path: String,
    pub status: WorkArtifactStatus,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<ArtifactProducer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<ArtifactVerificationEvidence>,
    /// Human-readable one-line summary of the latest validation outcome.
    /// Persisted on validate/deliver; older entries derive it from evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_summary: Option<String>,
    /// Content revision counter: 1 for the first validated content, bumped
    /// whenever the file at the same path is re-registered with new content.
    #[serde(default = "default_artifact_version")]
    pub version: u32,
    /// How the frontend should preview this artifact (unified preview provider).
    #[serde(default)]
    pub preview_kind: WorkArtifactPreviewKind,
    #[serde(default)]
    pub can_preview: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<ArtifactSourceRef>,
    pub created_at: String,
    pub updated_at: String,
}

fn default_artifact_version() -> u32 {
    1
}

fn default_artifact_mime_type() -> String {
    "application/octet-stream".to_string()
}

/// A user-declared artifact requirement. `path` is workspace-relative and is
/// deliberately kept simple so legacy `required_artifacts` entries can be
/// upgraded without changing the task storage format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkArtifactRequirement {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<String>,
    #[serde(default = "default_artifact_requirement_required")]
    pub required: bool,
}

fn default_artifact_requirement_required() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkArtifactCheckStatus {
    Satisfied,
    Missing,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkArtifactCheck {
    pub requirement: WorkArtifactRequirement,
    pub status: WorkArtifactCheckStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// The authoritative completion-gate result for a Work Run's declared files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkArtifactAcceptance {
    pub run_id: String,
    pub checks: Vec<WorkArtifactCheck>,
    pub required_count: usize,
    pub satisfied_count: usize,
    pub missing_count: usize,
    pub invalid_count: usize,
    pub satisfied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkTaskCheckpoint {
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_step_id: Option<String>,
    pub created_at: String,
}

/// The one human decision that currently blocks a Work Run.
///
/// Keep this deliberately small: the approval is part of the Run task state,
/// not a second Harness state machine. Persisting the proposed steps lets the
/// UI recover the exact decision after a reconnect or app restart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkPendingApproval {
    pub id: String,
    pub kind: String,
    pub summary: String,
    pub steps: Vec<StructuredTask>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    #[default]
    Pending,
    Checking,
    Passed,
    Failed,
    InsufficientEvidence,
    /// The task has no machine-verifiable acceptance contract (e.g. it demands
    /// no deliverables), so neither pass nor fail may be claimed for it.
    NotApplicable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CriterionStatus {
    #[default]
    Pending,
    Checking,
    Passed,
    Failed,
    InsufficientEvidence,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerifierType {
    #[default]
    Artifact,
    File,
    Structured,
    Machine,
    Llm,
    Composite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceCriterion {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub verifier_type: VerifierType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ref: Option<String>,
    #[serde(default)]
    pub status: CriterionStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GoalSpec {
    pub goal_id: String,
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub run_id: String,
    pub statement: String,
    #[serde(default)]
    pub criteria: Vec<AcceptanceCriterion>,
    #[serde(default)]
    pub status: GoalStatus,
    #[serde(default)]
    pub repair_round: u32,
    #[serde(default = "default_max_repair_rounds")]
    pub max_repair_rounds: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_instruction: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn default_max_repair_rounds() -> u32 {
    3
}

/// Authoritative Work Harness state for one Run. The file stored under the Run
/// directory and the `work_task_state` event use this exact schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkTaskState {
    pub version: u32,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_spec: Option<GoalSpec>,
    #[serde(default)]
    pub plan: Vec<StructuredTask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<WorkTaskCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_approval: Option<WorkPendingApproval>,
    pub updated_at: String,
}

impl Default for WorkTaskState {
    fn default() -> Self {
        Self {
            version: 1,
            revision: 0,
            goal: None,
            goal_spec: None,
            plan: Vec::new(),
            checkpoint: None,
            pending_approval: None,
            updated_at: crate::models::now_iso(),
        }
    }
}

// ============================================================================
// Work Mode V2 Data Models: WorkPolicy, WorkTask, WorkRun, InboxItem
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkExecutionMode {
    #[default]
    Direct,
    PlanFirst,
    Auto,
    FullAccess,
}

impl WorkExecutionMode {
    /// Parse the legacy Pi/AgentCabin permission-mode spellings used at
    /// session boundaries into the canonical Work policy mode.
    pub fn from_permission_mode(mode: &str) -> Option<Self> {
        match mode.trim().to_ascii_lowercase().as_str() {
            "plan" | "plan_first" | "planfirst" => Some(Self::PlanFirst),
            "direct" | "ask" | "default" => Some(Self::Direct),
            "auto" | "auto_all" => Some(Self::Auto),
            "full_access" | "fullaccess" | "bypass" | "bypasspermissions" => Some(Self::FullAccess),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolRiskClass {
    #[default]
    Read,
    WriteLocal,
    Exec,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum CommandRiskClassification {
    WorkspaceRead,
    WorkspaceWrite,
    TrustedHostCapability,
    SandboxedCommand,
    HostCommandFallback,
    DependencyInstall {
        package_manager: String,
        packages: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    ExternalPathAccess {
        path: String,
    },
    Destructive {
        reason: String,
    },
    CredentialOrSensitivePath {
        path: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskStandingRule {
    pub id: String,
    pub tool_name: String,
    pub target_pattern: String,
    pub risk_class: ToolRiskClass,
    pub granted_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunHealth {
    #[default]
    Healthy,
    Warning,
    Stalled,
    Degraded,
    NeedsAttention,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeLiveness {
    #[default]
    Alive,
    Dead,
    Disconnected,
    AliveButStalled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BudgetKind {
    WallTime,
    ToolCalls,
    SubagentSpawns,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GuardianAnomalyKind {
    Stall,
    DuplicateTool,
    ToolFailureStreak,
    BudgetExceeded,
    ProviderDegraded,
    SubagentHealth,
}

fn default_stall_after_ms() -> u64 {
    180_000
}

fn default_stall_warn_after_ms() -> u64 {
    90_000
}

fn default_max_duplicate_calls() -> u32 {
    3
}

fn default_max_failure_streak() -> u32 {
    3
}

fn default_max_steering_nudges() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GuardianConfig {
    #[serde(default = "default_stall_after_ms")]
    pub stall_after_ms: u64,
    #[serde(default = "default_stall_warn_after_ms")]
    pub stall_warn_after_ms: u64,
    #[serde(default = "default_max_duplicate_calls")]
    pub max_duplicate_calls: u32,
    #[serde(default = "default_max_failure_streak")]
    pub max_failure_streak: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_automated_steps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_subagent_spawns: Option<u32>,
    #[serde(default = "default_max_steering_nudges")]
    pub max_steering_nudges: u32,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            stall_after_ms: default_stall_after_ms(),
            stall_warn_after_ms: default_stall_warn_after_ms(),
            max_duplicate_calls: default_max_duplicate_calls(),
            max_failure_streak: default_max_failure_streak(),
            max_tokens: None,
            max_automated_steps: None,
            max_subagent_spawns: None,
            max_steering_nudges: default_max_steering_nudges(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkPolicy {
    #[serde(default)]
    pub execution_mode: WorkExecutionMode,
    #[serde(default = "default_max_automated_steps")]
    pub max_automated_steps: u32,
    #[serde(default)]
    pub allow_external_connectors: bool,
    #[serde(default)]
    pub standing_rules: Vec<TaskStandingRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardian_config: Option<GuardianConfig>,
}

fn default_max_automated_steps() -> u32 {
    50
}

impl Default for WorkPolicy {
    fn default() -> Self {
        Self {
            execution_mode: WorkExecutionMode::Direct,
            max_automated_steps: 50,
            allow_external_connectors: false,
            standing_rules: Vec::new(),
            guardian_config: None,
        }
    }
}

impl WorkPolicy {
    /// Default policy for a newly created Work workspace.
    ///
    /// Keep `Default` conservative for legacy deserialization and low-level
    /// callers; the product-level Work workspace default is attended,
    /// workspace-scoped automation.
    pub fn default_for_workspace() -> Self {
        Self {
            execution_mode: WorkExecutionMode::Auto,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkTaskStatus {
    #[default]
    Draft,
    Active,
    Scheduled,
    InRun,
    NeedsAttention,
    Completed,
    Archived,
}

/// How a schedule is defined. Defaults to `Cron` so that legacy data with
/// only a `cron_expression` field round-trips correctly.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkScheduleKind {
    #[default]
    Cron,
    Once,
    Daily,
    Weekly,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkScheduleConfig {
    /// Discriminator — defaults to Cron for backward compatibility.
    #[serde(default)]
    pub kind: WorkScheduleKind,
    #[serde(default)]
    pub enabled: bool,
    /// IANA timezone identifier, e.g. "Asia/Shanghai", "UTC".
    #[serde(default = "default_timezone")]
    pub timezone: String,
    /// Once: ISO-8601 datetime when to fire (stored as-is; scheduler converts to UTC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fire_at: Option<String>,
    /// Daily / Weekly: local time-of-day in 24 h "HH:MM" format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_of_day: Option<String>,
    /// Weekly only: 0 = Monday … 6 = Sunday.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day_of_week: Option<u8>,
    /// Cron kind (also kept for legacy data round-trip).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron_expression: Option<String>,
    /// UTC ISO-8601 — recomputed by the scheduler after every fire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_run_at: Option<String>,
    /// UTC ISO-8601 — set after each successful fire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<String>,
    /// Cap the total number of scheduled runs (None = unlimited).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_runs: Option<u32>,
    /// Legacy field — kept for backward compatibility.
    #[serde(default)]
    pub run_on_startup: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkTaskSource {
    #[default]
    Manual,
    Dialog,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkTask {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub status: WorkTaskStatus,
    #[serde(default)]
    pub source: WorkTaskSource,
    #[serde(default)]
    pub policy: WorkPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<WorkScheduleConfig>,
    /// Artifact ids that must reach the existing validated/delivered lifecycle
    /// before a task run can be considered complete. Empty means the task has
    /// no declared artifact requirement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_artifacts: Vec<String>,
    /// Structured requirements added after the original id-only field. Both
    /// fields remain readable and are evaluated together.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_requirements: Vec<WorkArtifactRequirement>,
    #[serde(default)]
    pub run_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkRunTrigger {
    #[default]
    Manual,
    Scheduled,
    EventTriggered,
}

/// Whether the run was initiated by a human in an active session or by the
/// scheduler while the user may be away.  This does **not** change the
/// permission level — it only affects how the run surfaces human-required
/// actions (current session UI vs. Inbox / suspend).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionContext {
    #[default]
    Attended,
    Unattended,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkRunStatus {
    Queued,
    #[default]
    Running,
    WaitingApproval,
    WaitingInput,
    /// The process stopped while the next action is still safe to choose.
    Recoverable,
    /// The agent finished its turn, but required files still need acceptance.
    WaitingDelivery,
    /// Run was skipped before starting (concurrent active run or disabled schedule).
    Skipped,
    Completed,
    Failed,
    Cancelled,
}

impl WorkRunStatus {
    /// Returns true when a run is active and should block a new scheduled fire.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            WorkRunStatus::Queued
                | WorkRunStatus::Running
                | WorkRunStatus::WaitingApproval
                | WorkRunStatus::WaitingInput
                | WorkRunStatus::Recoverable
                | WorkRunStatus::WaitingDelivery
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkRun {
    pub id: String,
    pub task_id: String,
    pub workspace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub trigger: WorkRunTrigger,
    #[serde(default)]
    pub status: WorkRunStatus,
    /// Execution context: whether a human is in the current session.
    #[serde(default)]
    pub execution_context: ExecutionContext,
    /// For scheduled runs: the exact UTC ISO-8601 fire time this run was
    /// created for. Acts as the persistent idempotency key:
    ///   (task_id, scheduled_for) must be unique across all runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_for: Option<String>,
    /// Reason a Skipped run was not started.
    /// E.g. "previous_run_active", "missed", "disabled", "max_runs_reached".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_reason: Option<String>,
    #[serde(default)]
    pub task_state: WorkTaskState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardian_config: Option<GuardianConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkAutomationStats {
    pub total_tasks: usize,
    pub enabled_schedules: usize,
    pub running_tasks: usize,
    pub needs_attention_tasks: usize,
    pub completed_runs_today: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkTaskRunSummary {
    pub run_id: String,
    pub task_id: String,
    pub workspace_id: String,
    pub trigger: WorkRunTrigger,
    pub status: WorkRunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub delivered_artifacts_count: usize,
    #[serde(default)]
    pub goal_passed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_status: Option<GoalStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkScope {
    pub run_id: String,
    pub workspace_id: Option<String>,
    pub automation_task_id: Option<String>,
}

impl WorkScope {
    pub fn new(
        run_id: impl Into<String>,
        workspace_id: Option<String>,
        automation_task_id: Option<String>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            workspace_id,
            automation_task_id,
        }
    }

    pub fn from_meta(meta: &crate::models::RunMeta) -> Self {
        Self {
            run_id: meta.id.clone(),
            workspace_id: meta.workspace_id.clone(),
            automation_task_id: meta.work_task_id.clone(),
        }
    }

    pub fn is_standalone(&self) -> bool {
        self.workspace_id.is_none()
    }

    pub fn is_automation(&self) -> bool {
        self.automation_task_id.is_some()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InboxItemType {
    PermissionRequest,
    QuestionElicitation,
    PlanApproval,
    ArtifactValidation,
    AccessRootRequest,
    AppConnectionRequest,
    ConnectorAuthRequest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum InboxItemStatus {
    #[default]
    Pending,
    Approved,
    Rejected,
    Answered,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StandingRuleProposal {
    pub tool_name: String,
    pub target_pattern: String,
    pub scope: String,
    /// Effective risk class resolved by the backend (action-level for
    /// capability executions); lets the UI record accurate rule metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_class: Option<ToolRiskClass>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct InboxItemPayload {
    /// Stable identifier of the underlying agent interaction.  Keeping this
    /// in the durable Inbox item lets a scheduled run resume after the UI or
    /// the app reconnects without guessing which prompt to answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_plan: Option<Vec<StructuredTask>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standing_rule_proposal: Option<StandingRuleProposal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_scopes: Option<Vec<String>>,
    /// Legacy field retained only so older Inbox files remain readable. New
    /// auth interactions deliberately never persist an OAuth URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_url: Option<String>,
    /// Recovery metadata is persisted in the same Inbox item as the original
    /// decision, so a restart never has to infer which run/tool it belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_effect_class: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_outputs: Vec<String>,
    /// Failure metadata for an automation run that needs a new explicit retry
    /// decision. This is separate from tool recovery metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InboxItem {
    pub id: String,
    pub task_id: String,
    pub run_id: String,
    pub workspace_id: String,
    pub item_type: InboxItemType,
    #[serde(default)]
    pub status: InboxItemStatus,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub payload: InboxItemPayload,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<serde_json::Value>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
}

// ============================================================================
// Work Run Progress Projection Models
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkRunProgressPhase {
    Queued,
    Planning,
    #[default]
    Running,
    Researching,
    Implementing,
    Reviewing,
    WaitingUser,
    Recoverable,
    AwaitingDelivery,
    Blocked,
    Completed,
    Failed,
    Cancelled,
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkCurrentActivity {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkProgressAttention {
    pub kind: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkProgressAgent {
    pub agent_id: String,
    pub child_index: u32,
    pub role: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkProgressToolSummary {
    pub proposed: usize,
    pub started: usize,
    pub completed: usize,
    pub failed: usize,
    pub running: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkRunBudgetView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls_used: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls_limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_spawns_used: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_spawns_limit: Option<u32>,
    #[serde(default)]
    pub is_any_warning: bool,
    #[serde(default)]
    pub is_any_exceeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkRunHealthView {
    pub status: RunHealth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub warning_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stalled_since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<WorkRunBudgetView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_meaningful_progress_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liveness: Option<RuntimeLiveness>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkRunProgressView {
    pub task_id: String,
    pub work_run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub run_status: WorkRunStatus,
    pub phase: WorkRunProgressPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_spec: Option<GoalSpec>,
    #[serde(default)]
    pub steps: Vec<StructuredTask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<WorkTaskCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_activity: Option<WorkCurrentActivity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<WorkProgressAttention>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<WorkRunHealthView>,
    #[serde(default)]
    pub agents: Vec<WorkProgressAgent>,
    #[serde(default)]
    pub tool_summary: WorkProgressToolSummary,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkAvailableActions {
    pub can_send: bool,
    pub can_stop: bool,
    pub can_resume: bool,
    pub can_recover: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkProjection {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automation_task_id: Option<String>,
    pub task_id: String,
    pub work_run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub runtime_status: crate::models::RunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automation_status: Option<WorkRunStatus>,
    pub run_status: WorkRunStatus,
    pub status: WorkRunStatus,
    pub phase: WorkRunProgressPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_spec: Option<GoalSpec>,
    #[serde(default)]
    pub plan: Vec<crate::models::StructuredTask>,
    #[serde(default)]
    pub steps: Vec<crate::models::StructuredTask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<WorkTaskCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_activity: Option<WorkCurrentActivity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<WorkProgressAttention>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<WorkRunHealthView>,
    #[serde(default)]
    pub agents: Vec<WorkProgressAgent>,
    #[serde(default)]
    pub tool_summary: WorkProgressToolSummary,
    #[serde(default)]
    pub artifacts: Vec<WorkArtifactSummary>,
    #[serde(default)]
    pub pending_interactions: Vec<PendingInteraction>,
    pub available_actions: WorkAvailableActions,
    pub updated_at: String,
}

impl WorkProjection {
    pub fn to_progress_view(&self) -> WorkRunProgressView {
        WorkRunProgressView {
            task_id: self.task_id.clone(),
            work_run_id: self.work_run_id.clone(),
            session_id: self.session_id.clone(),
            run_status: self.run_status,
            phase: self.phase,
            goal: self.goal.clone(),
            goal_spec: self.goal_spec.clone(),
            steps: self.steps.clone(),
            checkpoint: self.checkpoint.clone(),
            current_activity: self.current_activity.clone(),
            attention: self.attention.clone(),
            health: self.health.clone(),
            agents: self.agents.clone(),
            tool_summary: self.tool_summary.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkRecoveryAction {
    Continue,
    Retry,
    Verify,
    RetrySubagent,
    FromScratch,
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkRunRecovery {
    pub task_id: String,
    pub work_run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub status: WorkRunStatus,
    pub detected_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_effect_class: Option<String>,
    #[serde(default)]
    pub reused_existing_output: bool,
    pub reason: String,
    pub recommendation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_interaction_id: Option<String>,
    #[serde(default)]
    pub expected_outputs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance: Option<WorkArtifactAcceptance>,
    pub pending_interaction_count: usize,
    pub active_subagents: usize,
    pub interrupted_subagents: usize,
    #[serde(default)]
    pub interrupted_subagent_ids: Vec<String>,
    pub available_actions: Vec<WorkRecoveryAction>,
}

// ============================================================================
// Work Run Receipt Projection Models
//
// The Receipt is a pure read-only projection over existing authorities
// (WorkRun + runtime Ledger + Artifact registry + 网络访问 ledger + session
// meta). It never introduces a second source of truth: every source entry
// requires a durable successful event, and every file change requires a
// durable `file_changed` fact or ledger output.
// ============================================================================

/// Where a Run source came from, based on the durable event that proved it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkReceiptSourceKind {
    /// URL appeared in a successful web search result set.
    WebSearch,
    /// Page content was actually fetched through web_open / web_fetch.
    WebPage,
    /// Page was opened through the interactive browser operator.
    Browser,
    /// A durable Library entry was successfully read through the Host bridge.
    Library,
    #[default]
    Other,
}

/// One real web source for a Run. `surfaced_by_search` alone never marks a
/// source as accessed: access requires a successful fetch/open event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkReceiptSource {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// The URL appeared in at least one successful search result.
    #[serde(default)]
    pub surfaced_by_search: bool,
    /// The content was actually fetched / opened (durable success event).
    #[serde(default)]
    pub accessed: bool,
    #[serde(default)]
    pub access_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_seen_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_accessed_at: Option<String>,
    /// Which kinds of events proved this source (search, page, browser).
    #[serde(default)]
    pub kinds: Vec<WorkReceiptSourceKind>,
}

/// One successful web search performed during the Run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkReceiptSearchQuery {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default)]
    pub result_count: usize,
    pub timestamp: String,
}

/// One durable file change recorded during the Run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkReceiptFileChange {
    pub path: String,
    pub change_kind: WorkFileChangeKind,
}

/// The task receipt: what the Run produced, what it read, what it changed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkRunReceipt {
    pub task_id: String,
    pub work_run_id: String,
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Standalone Work chats have no WorkTask entity (task_id == run id).
    #[serde(default)]
    pub standalone: bool,
    pub status: WorkRunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub artifacts: Vec<WorkArtifactSummary>,
    pub sources: Vec<WorkReceiptSource>,
    pub search_queries: Vec<WorkReceiptSearchQuery>,
    pub input_files: Vec<String>,
    pub changed_files: Vec<WorkReceiptFileChange>,
    pub tool_summary: WorkProgressToolSummary,
    /// False when the runtime ledger file does not exist (e.g. a session that
    /// never dispatched a pipeline tool); the UI should say "暂无运行记录"
    /// rather than pretending an empty projection is complete history.
    #[serde(default)]
    pub ledger_available: bool,
    pub generated_at: String,
}

// ============================================================================
// Work Mode Harness Runtime Core Primitives
// ============================================================================

/// Collaboration mode describes how the agent is collaborating / reasoning.
/// Separated from ExecutionPolicy so Plan Mode is not itself the security boundary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationMode {
    #[default]
    Default,
    Plan,
}

/// Execution policy kind describes what effect classes are permitted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPolicyKind {
    ReadOnly,
    #[default]
    Ask,
    Auto,
    Custom(String),
}

/// Classification of side effects for deterministic recovery and policy checks.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectClass {
    #[default]
    Read,
    LocalVerifiable,
    ExternalMutating,
}

/// Concurrency class for tool scheduling.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolConcurrencyClass {
    #[default]
    Serial,
    ParallelSafe,
    Exclusive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PendingInteractionKind {
    Permission,
    UserInput,
    PlanApproval,
    ArtifactValidation,
    AccessRootRequest,
    AppConnectionRequest,
    ConnectorAuthRequest,
    Custom(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PendingInteractionState {
    #[default]
    Pending,
    Delivering,
    Resolved,
    Cancelled,
}

/// Durable PendingInteraction is the unified primitive for human approvals, inputs,
/// and reviews across both attended inline UI and unattended durable Inbox surfaces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingInteraction {
    pub interaction_id: String,
    pub task_id: String,
    pub work_run_id: String,
    pub session_id: Option<String>,
    pub runtime_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub kind: PendingInteractionKind,
    #[serde(default)]
    pub state: PendingInteractionState,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared_target_state: Option<PendingInteractionState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared_inbox_status: Option<InboxItemStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<serde_json::Value>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
}

/// Durable facts logged to the Work Runtime Ledger for deterministic recovery and audit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeFact {
    RunStarted {
        task_id: String,
        work_run_id: String,
        execution_context: ExecutionContext,
        collaboration_mode: CollaborationMode,
        timestamp: String,
    },
    TurnStarted {
        turn_id: String,
        prompt: Option<String>,
        timestamp: String,
    },
    StepStarted {
        step_id: String,
        title: Option<String>,
        timestamp: String,
    },
    ToolProposed {
        tool_call_id: String,
        tool_name: String,
        action: String,
        arguments_hash: String,
        #[serde(default)]
        expected_outputs: Vec<String>,
        side_effect_class: SideEffectClass,
        concurrency_class: ToolConcurrencyClass,
        timestamp: String,
    },
    ApprovalRequested {
        interaction_id: String,
        tool_call_id: Option<String>,
        kind: PendingInteractionKind,
        timestamp: String,
    },
    ApprovalResolved {
        interaction_id: String,
        resolution: serde_json::Value,
        approved: bool,
        timestamp: String,
    },
    GrantIssued {
        grant_id: String,
        interaction_id: String,
        tool_call_id: String,
        outcome: ApprovalOutcome,
        timestamp: String,
    },
    GrantConsumed {
        grant_id: String,
        tool_call_id: String,
        timestamp: String,
    },
    ToolStarted {
        tool_call_id: String,
        execution_id: String,
        timestamp: String,
    },
    ToolResult {
        tool_call_id: String,
        success: bool,
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        failure_kind: Option<crate::work::executor::ExecutionFailureKind>,
        exit_code: Option<i32>,
        error: Option<String>,
        outputs: Vec<String>,
        side_effect_class: SideEffectClass,
        timestamp: String,
    },
    /// A file inside the Workspace changed as a result of a successful,
    /// policy-approved file-writing tool. Emitted by the Tool Pipeline only
    /// after the write itself succeeded, so the Run Receipt can treat it as
    /// authoritative history rather than an agent claim.
    FileChanged {
        tool_call_id: String,
        /// Workspace-relative normalized path (input/…, scratch/…, output/…).
        path: String,
        change_kind: WorkFileChangeKind,
        timestamp: String,
    },
    RuntimeInterrupted {
        reason: String,
        step_id: Option<String>,
        tool_call_id: Option<String>,
        timestamp: String,
    },
    StepEnded {
        step_id: String,
        status: String,
        timestamp: String,
    },
    TurnEnded {
        turn_id: String,
        timestamp: String,
    },
    SteeringQueued {
        input_id: String,
        kind: RuntimeInputKind,
        instruction: String,
        timestamp: String,
    },
    SteeringClaimed {
        input_id: String,
        step_id: Option<String>,
        timestamp: String,
    },
    SteeringCancelled {
        input_id: String,
        reason: String,
        timestamp: String,
    },
    SubagentSpawned {
        agent_id: String,
        provider_run_id: String,
        child_index: u32,
        role: String,
        task_digest: String,
        launch_contract_digest: String,
        status: String,
        timestamp: String,
    },
    SubagentCompleted {
        agent_id: String,
        provider_run_id: String,
        child_index: u32,
        role: String,
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        timestamp: String,
    },
    SubagentFailed {
        agent_id: String,
        provider_run_id: String,
        child_index: u32,
        role: String,
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        timestamp: String,
    },
    SubagentStopped {
        agent_id: String,
        provider_run_id: String,
        child_index: u32,
        role: String,
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        timestamp: String,
    },
    SubagentInterrupted {
        agent_id: String,
        provider_run_id: String,
        child_index: u32,
        role: String,
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        timestamp: String,
    },
    RecoveryDetected {
        work_run_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        original_tool_call_id: Option<String>,
        recovery_action: String,
        side_effect_class: SideEffectClass,
        reused_existing_output: bool,
        result: Option<String>,
        error: Option<String>,
        timestamp: String,
    },
    RecoveryActionStarted {
        work_run_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        original_tool_call_id: Option<String>,
        recovery_action: String,
        side_effect_class: SideEffectClass,
        reused_existing_output: bool,
        result: Option<String>,
        error: Option<String>,
        timestamp: String,
    },
    RecoveryResolved {
        work_run_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        original_tool_call_id: Option<String>,
        recovery_action: String,
        side_effect_class: SideEffectClass,
        reused_existing_output: bool,
        result: Option<String>,
        error: Option<String>,
        timestamp: String,
    },
    RecoveryAbandoned {
        work_run_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        original_tool_call_id: Option<String>,
        recovery_action: String,
        side_effect_class: SideEffectClass,
        reused_existing_output: bool,
        result: Option<String>,
        error: Option<String>,
        timestamp: String,
    },
    GuardianAnomalyDetected {
        anomaly_kind: GuardianAnomalyKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        threshold: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        suggested_action: Option<String>,
        timestamp: String,
    },
    RunHealthChanged {
        previous: RunHealth,
        current: RunHealth,
        reason: String,
        timestamp: String,
    },
    RunStalled {
        idle_seconds: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_progress_fact: Option<String>,
        timestamp: String,
    },
    LoopDetected {
        fingerprint: String,
        repeat_count: u32,
        tool_name: String,
        timestamp: String,
    },
    ToolFailureStreakDetected {
        consecutive_failures: u32,
        last_tool_name: String,
        timestamp: String,
    },
    BudgetWarning {
        budget_kind: String,
        used: u64,
        limit: u64,
        timestamp: String,
    },
    BudgetExceeded {
        budget_kind: String,
        used: u64,
        limit: u64,
        timestamp: String,
    },
    RuntimeLivenessChanged {
        previous: String,
        current: String,
        reason: String,
        timestamp: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalOutcome {
    AllowedOnce,
    Rejected,
    Expired,
}

/// Host-issued, one-shot grant bound to a specific tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalGrant {
    pub grant_id: String,
    pub interaction_id: String,
    pub task_id: String,
    pub work_run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub tool_call_id: String,
    pub tool_name: String,
    pub action: String,
    pub arguments_hash: String,
    pub workspace_id: String,
    /// Optional execution lane selected by the Host for this one-shot grant.
    /// This is used by the Work default-permissions fallback when a command
    /// cannot run inside the OS sandbox and the user explicitly authorizes a
    /// single host execution. Older grants omit this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_lane: Option<String>,
    pub outcome: ApprovalOutcome,
    pub consumed: bool,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumed_at: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInputKind {
    Steer,
    FollowUp,
    Inject,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInputState {
    #[default]
    Queued,
    Claimed,
    Cancelled,
}

/// First-class user instruction / context items.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInputItem {
    pub id: String,
    pub task_id: String,
    pub work_run_id: String,
    pub kind: RuntimeInputKind,
    pub state: RuntimeInputState,
    pub content: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<String>,
}

/// Write file contents to a temp file in the same directory and atomically rename/replace it.
pub fn atomic_replace_file(target: &std::path::Path, content: &[u8]) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let parent_dir = target.parent().unwrap_or_else(|| std::path::Path::new("."));
    let temp_name = format!(
        ".{}.tmp.{}",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file"),
        uuid::Uuid::new_v4()
    );
    let temp_path = parent_dir.join(temp_name);

    let write_res = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temp_path)?;
        use std::io::Write;
        file.write_all(content)?;
        file.sync_data()?;
        Ok(())
    })();

    if let Err(e) = write_res {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!(
            "Failed writing temp file {}: {e}",
            temp_path.display()
        ));
    }

    #[cfg(windows)]
    {
        if target.exists() {
            let _ = std::fs::remove_file(target);
        }
    }

    if let Err(e) = std::fs::rename(&temp_path, target) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!(
            "Failed replacing target file {}: {e}",
            target.display()
        ));
    }

    Ok(())
}

pub fn atomic_write_json<T: Serialize>(target: &std::path::Path, val: &T) -> Result<(), String> {
    let json_bytes =
        serde_json::to_vec_pretty(val).map_err(|e| format!("Failed serializing json: {e}"))?;
    atomic_replace_file(target, &json_bytes)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LibraryCategory {
    #[default]
    Doc,
    Template,
    Rule,
    Dataset,
    Link,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct LibraryCitation {
    pub source_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    pub id: String,
    pub workspace_id: Option<String>,
    pub title: String,
    pub description: String,
    pub category: LibraryCategory,
    pub content: String,
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(default)]
    pub metadata: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub citations: Vec<LibraryCitation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_artifact_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItemSummary {
    pub id: String,
    pub workspace_id: Option<String>,
    pub title: String,
    pub description: String,
    pub category: LibraryCategory,
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(default)]
    pub metadata: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub citation_count: usize,
    #[serde(default)]
    pub content_preview: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_artifact_id: Option<String>,
    pub updated_at: String,
}

impl LibraryItem {
    pub fn summary(&self) -> LibraryItemSummary {
        let preview = if self.content.chars().count() > 150 {
            let mut s = String::new();
            for c in self.content.chars().take(150) {
                s.push(c);
            }
            s.push('…');
            s
        } else {
            self.content.clone()
        };
        LibraryItemSummary {
            id: self.id.clone(),
            workspace_id: self.workspace_id.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            category: self.category,
            tags: self.tags.clone(),
            source_path: self.source_path.clone(),
            collection: self.collection.clone(),
            metadata: self.metadata.clone(),
            citation_count: self.citations.len(),
            content_preview: preview,
            source_artifact_id: self.source_artifact_id.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Browser Use Phase 5 Data Models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BrowserActionType {
    #[default]
    Navigate,
    Snapshot,
    Screenshot,
    Click,
    Type,
    SelectOption,
    Scroll,
    WaitFor,
    Tabs,
    Close,
    Takeover,
    Custom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BrowserSessionStatus {
    #[default]
    Idle,
    Running,
    WaitingApproval,
    Paused,
    TakingOver,
    Completed,
    Failed,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTraceEntry {
    pub step_index: u32,
    pub action_type: BrowserActionType,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    pub status: String, // "started" | "success" | "failed" | "blocked" | "waiting_approval"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_data: Option<String>,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSession {
    pub session_id: String,
    pub run_id: String,
    pub mode: String, // "code" | "work"
    /// The browser is always the Electron WebContentsView surface.
    #[serde(default = "default_browser_surface")]
    pub surface: String,
    pub status: BrowserSessionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_screenshot: Option<String>,
    #[serde(default)]
    pub traces: Vec<BrowserTraceEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default)]
    pub is_taking_over: bool,
    pub created_at: String,
    pub updated_at: String,
}

fn default_browser_surface() -> String {
    "embedded".to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserEventType {
    SessionStarted,
    SessionUpdated,
    NavigationStarted,
    NavigationFinished,
    ReadStarted,
    ReadFinished,
    ActionRequested,
    ApprovalRequired,
    ActionStarted,
    ActionSucceeded,
    ActionFailed,
    ScreenshotCreated,
    TakeoverStarted,
    TakeoverFinished,
    SessionPaused,
    SessionResumed,
    SessionStopped,
    SessionClosed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserEvent {
    pub event_type: BrowserEventType,
    pub session_id: String,
    pub run_id: String,
    pub mode: String,
    pub payload: serde_json::Value,
    pub timestamp: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_workspace_policy_defaults_to_auto_execution() {
        assert_eq!(
            WorkPolicy::default_for_workspace().execution_mode,
            WorkExecutionMode::Auto
        );
    }

    #[test]
    fn app_mode_defaults_to_code() {
        assert_eq!(AppMode::default(), AppMode::Code);
    }

    #[test]
    fn app_mode_wire_names_are_stable() {
        assert_eq!(serde_json::to_string(&AppMode::Code).unwrap(), "\"code\"");
        assert_eq!(serde_json::to_string(&AppMode::Work).unwrap(), "\"work\"");
        // RuntimeProviderKind (the canonical runtime identity) serializes as lowercase.
        assert_eq!(
            serde_json::to_string(&RuntimeProviderKind::Pi).unwrap(),
            "\"pi\""
        );
        assert_eq!(
            serde_json::to_string(&RuntimeProviderKind::Claude).unwrap(),
            "\"claude\""
        );
        assert_eq!(
            serde_json::to_string(&WorkResourceKind::PiExtension).unwrap(),
            "\"pi_extension\""
        );
    }

    #[test]
    fn work_profile_deserialization_handles_compat_and_fails_on_unknown() {
        // Missing runtime field -> defaults to Pi
        let json_missing = r#"{"id":"work","name":"Work","enabled":true}"#;
        let profile: WorkProfile = serde_json::from_str(json_missing).unwrap();
        assert_eq!(profile.runtime, RuntimeProviderKind::Pi);

        // "pi" -> Pi
        let json_pi = r#"{"id":"work","name":"Work","enabled":true,"runtime":"pi"}"#;
        let profile: WorkProfile = serde_json::from_str(json_pi).unwrap();
        assert_eq!(profile.runtime, RuntimeProviderKind::Pi);

        // "claude-code" legacy -> Claude
        let json_claude_legacy =
            r#"{"id":"work","name":"Work","enabled":true,"runtime":"claude-code"}"#;
        let profile: WorkProfile = serde_json::from_str(json_claude_legacy).unwrap();
        assert_eq!(profile.runtime, RuntimeProviderKind::Claude);

        // "claude" -> Claude
        let json_claude = r#"{"id":"work","name":"Work","enabled":true,"runtime":"claude"}"#;
        let profile: WorkProfile = serde_json::from_str(json_claude).unwrap();
        assert_eq!(profile.runtime, RuntimeProviderKind::Claude);

        // "dsh" -> DSH
        let json_dsh = r#"{"id":"work","name":"Work","enabled":true,"runtime":"dsh"}"#;
        let profile: WorkProfile = serde_json::from_str(json_dsh).unwrap();
        assert_eq!(profile.runtime, RuntimeProviderKind::Dsh);

        // Unknown runtime string -> fails closed (Err)
        let json_unknown = r#"{"id":"work","name":"Work","enabled":true,"runtime":"mystery_bot"}"#;
        let res: Result<WorkProfile, _> = serde_json::from_str(json_unknown);
        assert!(res.is_err(), "Unknown runtime must fail deserialization");
    }

    #[test]
    fn work_task_state_uses_the_frontend_contract() {
        let state = WorkTaskState::default();
        let value = serde_json::to_value(state).unwrap();

        assert_eq!(value["version"], 1);
        assert_eq!(value["revision"], 0);
        assert_eq!(value["plan"], serde_json::json!([]));
        assert!(value.get("updatedAt").is_some());
        assert!(value.get("goal").is_none());
    }

    #[test]
    fn v2_enums_and_structs_serialize_camelcase_contract() {
        let task = WorkTask {
            id: "task-1".to_string(),
            workspace_id: "ws-1".to_string(),
            title: "Generate Report".to_string(),
            instructions: "Process output xlsx".to_string(),
            status: WorkTaskStatus::Active,
            policy: WorkPolicy {
                execution_mode: WorkExecutionMode::PlanFirst,
                max_automated_steps: 30,
                allow_external_connectors: true,
                standing_rules: vec![TaskStandingRule {
                    id: "rule-1".to_string(),
                    tool_name: "slack_send".to_string(),
                    target_pattern: "channel:#general".to_string(),
                    risk_class: ToolRiskClass::External,
                    granted_at: "2026-08-13T16:00:00Z".to_string(),
                }],
                guardian_config: None,
            },
            schedule: Some(WorkScheduleConfig {
                kind: WorkScheduleKind::Cron,
                enabled: true,
                timezone: "UTC".to_string(),
                fire_at: None,
                time_of_day: None,
                day_of_week: None,
                cron_expression: Some("0 9 * * 1".to_string()),
                next_run_at: None,
                last_run_at: None,
                run_on_startup: false,
                max_runs: Some(10),
            }),
            required_artifacts: Vec::new(),
            artifact_requirements: Vec::new(),
            run_count: 3,
            last_run_id: Some("run-3".to_string()),
            last_run_at: Some("2026-08-13T09:00:00Z".to_string()),
            created_at: "2026-08-13T08:00:00Z".to_string(),
            updated_at: "2026-08-13T09:00:00Z".to_string(),
            source: WorkTaskSource::Manual,
        };

        let val = serde_json::to_value(&task).unwrap();
        assert_eq!(val["id"], "task-1");
        assert_eq!(val["workspaceId"], "ws-1");
        assert_eq!(val["status"], "active");
        assert_eq!(val["policy"]["executionMode"], "plan_first");
        assert_eq!(val["policy"]["maxAutomatedSteps"], 30);
        assert_eq!(val["policy"]["standingRules"][0]["riskClass"], "external");
        assert_eq!(val["schedule"]["cronExpression"], "0 9 * * 1");
        assert_eq!(val["schedule"]["kind"], "cron");
        assert_eq!(
            serde_json::to_string(&WorkExecutionMode::FullAccess).unwrap(),
            "\"full_access\""
        );
    }

    #[test]
    fn work_execution_mode_parses_legacy_permission_spellings() {
        for spelling in ["full_access", "fullAccess", "bypass", "bypassPermissions"] {
            assert_eq!(
                WorkExecutionMode::from_permission_mode(spelling),
                Some(WorkExecutionMode::FullAccess),
                "{spelling} should select the canonical Work FullAccess policy"
            );
        }
        assert_eq!(
            WorkExecutionMode::from_permission_mode("auto"),
            Some(WorkExecutionMode::Auto)
        );
        assert_eq!(WorkExecutionMode::from_permission_mode("unknown"), None);
    }

    #[test]
    fn work_schedule_kind_defaults_to_cron_for_legacy_data() {
        // Legacy JSON without `kind` field must still deserialize and default to Cron.
        let legacy = r#"{"enabled": true, "cronExpression": "0 9 * * 1"}"#;
        let cfg: WorkScheduleConfig = serde_json::from_str(legacy).unwrap();
        assert_eq!(cfg.kind, WorkScheduleKind::Cron);
        assert_eq!(cfg.timezone, "UTC");
        assert_eq!(cfg.cron_expression, Some("0 9 * * 1".to_string()));
    }

    #[test]
    fn execution_context_defaults_to_attended() {
        assert_eq!(ExecutionContext::default(), ExecutionContext::Attended);
    }

    #[test]
    fn work_run_status_is_active_covers_waiting_states() {
        assert!(WorkRunStatus::Running.is_active());
        assert!(WorkRunStatus::WaitingApproval.is_active());
        assert!(WorkRunStatus::WaitingInput.is_active());
        assert!(WorkRunStatus::Recoverable.is_active());
        assert!(WorkRunStatus::WaitingDelivery.is_active());
        assert!(WorkRunStatus::Queued.is_active());
        assert!(!WorkRunStatus::Completed.is_active());
        assert!(!WorkRunStatus::Failed.is_active());
        assert!(!WorkRunStatus::Skipped.is_active());
        assert!(!WorkRunStatus::Cancelled.is_active());
    }

    #[test]
    fn work_run_with_scheduled_context_serializes_correctly() {
        use crate::work::models::{
            ExecutionContext, WorkRun, WorkRunStatus, WorkRunTrigger, WorkTaskState,
        };
        let run = WorkRun {
            id: "run-sched-1".to_string(),
            task_id: "task-1".to_string(),
            workspace_id: "ws-1".to_string(),
            session_id: None,
            trigger: WorkRunTrigger::Scheduled,
            status: WorkRunStatus::Running,
            execution_context: ExecutionContext::Unattended,
            scheduled_for: Some("2026-08-18T09:00:00Z".to_string()),
            skipped_reason: None,
            task_state: WorkTaskState::default(),
            error_message: None,
            started_at: "2026-08-18T09:00:01Z".to_string(),
            finished_at: None,
            duration_ms: None,
            guardian_config: None,
        };
        let val = serde_json::to_value(&run).unwrap();
        assert_eq!(val["executionContext"], "unattended");
        assert_eq!(val["scheduledFor"], "2026-08-18T09:00:00Z");
        assert_eq!(val["trigger"], "scheduled");
    }

    #[test]
    fn v2_inbox_item_deserializes_correctly() {
        let json_str = r#"{
            "id": "inbox-101",
            "taskId": "task-1",
            "runId": "run-3",
            "workspaceId": "ws-1",
            "itemType": "plan_approval",
            "status": "pending",
            "title": "Plan Approval Required",
            "description": "Agent proposed a 3-step plan",
            "payload": {
                "question": null,
                "proposedPlan": []
            },
            "createdAt": "2026-08-13T16:10:00Z"
        }"#;

        let item: InboxItem = serde_json::from_str(json_str).unwrap();
        assert_eq!(item.id, "inbox-101");
        assert_eq!(item.item_type, InboxItemType::PlanApproval);
        assert_eq!(item.status, InboxItemStatus::Pending);
        assert!(item.payload.proposed_plan.is_some());
    }

    #[test]
    fn guardian_config_defaults_and_serialization() {
        let config = GuardianConfig::default();
        assert_eq!(config.stall_after_ms, 180_000);
        assert_eq!(config.max_duplicate_calls, 3);
        assert_eq!(config.max_failure_streak, 3);

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: GuardianConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);

        // Test empty JSON deserializes with defaults
        let empty_json = "{}";
        let from_empty: GuardianConfig = serde_json::from_str(empty_json).unwrap();
        assert_eq!(from_empty.stall_after_ms, 180_000);
        // Older manifests may still contain the removed total-duration field;
        // serde ignores it so existing task data remains readable.
        let from_legacy: GuardianConfig =
            serde_json::from_str(r#"{"maxDurationMs":120000}"#).unwrap();
        assert_eq!(from_legacy, GuardianConfig::default());
    }

    #[test]
    fn guardian_anomaly_fact_round_trip() {
        let fact = RuntimeFact::GuardianAnomalyDetected {
            anomaly_kind: GuardianAnomalyKind::Stall,
            step_id: Some("step-1".to_string()),
            tool_call_id: None,
            reason: "No activity for 60s".to_string(),
            threshold: Some("60000ms".to_string()),
            suggested_action: Some("Check network connection".to_string()),
            timestamp: "2026-08-28T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&fact).unwrap();
        assert!(json.contains("\"type\":\"guardian_anomaly_detected\""));
        assert!(json.contains("\"anomaly_kind\":\"stall\""));

        let deserialized: RuntimeFact = serde_json::from_str(&json).unwrap();
        assert_eq!(fact, deserialized);
    }

    #[test]
    fn artifact_evidence_and_provenance_round_trip() {
        let artifact = WorkArtifactSummary {
            id: "art-1".to_string(),
            workspace_id: "ws-1".to_string(),
            run_id: Some("run-1".to_string()),
            artifact_type: "docx".to_string(),
            category: crate::work::models::WorkArtifactCategory::Document,
            mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                .to_string(),
            preview_kind: crate::work::models::WorkArtifactPreviewKind::Office,
            can_preview: true,
            version: 2,
            validation_summary: Some("格式有效 · SHA-256 已计算 · 路径受限".to_string()),
            title: "Report.docx".to_string(),
            path: "output/Report.docx".to_string(),
            status: WorkArtifactStatus::Delivered,
            size: 1024,
            sha256: Some("abcdef123456".to_string()),
            producer: Some(ArtifactProducer {
                run_id: "run-1".to_string(),
                producer_tool_call_id: Some("call-1".to_string()),
                execution_id: Some("exec-1".to_string()),
            }),
            evidence: Some(ArtifactVerificationEvidence {
                verified_at: "2026-08-28T12:00:00Z".to_string(),
                validator_version: "v1.0.0".to_string(),
                sha256: "abcdef123456".to_string(),
                size: 1024,
                checks: vec!["zip_valid".to_string()],
                verification_status: WorkArtifactStatus::Delivered,
            }),
            sources: vec![ArtifactSourceRef {
                source_tool_call_id: "call-search".to_string(),
                source_type: "web_search".to_string(),
                url: Some("https://example.com".to_string()),
                result_digest: "digest-1".to_string(),
                captured_at: "2026-08-28T12:00:00Z".to_string(),
                is_claimed: false,
            }],
            created_at: "2026-08-28T12:00:00Z".to_string(),
            updated_at: "2026-08-28T12:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&artifact).unwrap();
        let deserialized: WorkArtifactSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(artifact, deserialized);

        // Backward compatibility: old artifact JSON without new fields must parse
        let legacy_json = r#"{
            "id": "art-legacy",
            "workspaceId": "ws-1",
            "artifactType": "file",
            "title": "Legacy",
            "path": "output/legacy.txt",
            "status": "delivered",
            "size": 100,
            "createdAt": "2026-08-28T12:00:00Z",
            "updatedAt": "2026-08-28T12:00:00Z"
        }"#;
        let legacy: WorkArtifactSummary = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(legacy.id, "art-legacy");
        assert!(legacy.sha256.is_none());
        assert!(legacy.producer.is_none());
        assert!(legacy.evidence.is_none());
        assert!(legacy.sources.is_empty());
        // New fields must have safe defaults for historical records.
        assert_eq!(
            legacy.category,
            crate::work::models::WorkArtifactCategory::Other
        );
        assert_eq!(legacy.mime_type, "application/octet-stream");
        assert_eq!(
            legacy.preview_kind,
            crate::work::models::WorkArtifactPreviewKind::Binary
        );
        assert!(!legacy.can_preview);
        assert_eq!(legacy.version, 1);
        assert!(legacy.validation_summary.is_none());
    }

    #[test]
    fn artifact_hub_wire_names_are_stable() {
        assert_eq!(
            serde_json::to_string(&crate::work::models::WorkArtifactCategory::Spreadsheet).unwrap(),
            "\"spreadsheet\""
        );
        assert_eq!(
            serde_json::to_string(&crate::work::models::WorkArtifactCategory::Presentation)
                .unwrap(),
            "\"presentation\""
        );
        assert_eq!(
            serde_json::to_string(&crate::work::models::WorkArtifactPreviewKind::Markdown).unwrap(),
            "\"markdown\""
        );
        assert_eq!(
            serde_json::to_string(&crate::work::models::WorkFileChangeKind::Created).unwrap(),
            "\"created\""
        );
        assert_eq!(
            serde_json::to_string(&crate::work::models::WorkFileChangeKind::Modified).unwrap(),
            "\"modified\""
        );
        assert_eq!(
            serde_json::to_string(&crate::work::models::WorkFileChangeKind::Deleted).unwrap(),
            "\"deleted\""
        );
        assert_eq!(
            serde_json::to_string(&crate::work::models::WorkReceiptSourceKind::WebSearch).unwrap(),
            "\"web_search\""
        );
    }
}
