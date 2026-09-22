use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::agent::capability_resolver::{EffectiveCapabilities, RuntimeProviderKind};
use crate::work::models::{
    AppMode, ExecutionContext, LibraryItemSummary, WorkArtifactSummary, WorkPreset, WorkTask,
    WorkTaskState, WorkWorkspace,
};

/// Product-level context kind classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkContextKind {
    BasePolicy,
    RuntimeInstructions,
    WorkPreset,
    WorkspaceRules,
    WorkspaceContext,
    CurrentGoal,
    CurrentTask,
    Conversation,
    Skill,
    Mcp,
    Connector,
    AgentPlugin,
    WebAccess,
    BrowserUse,
    Library,
    Artifact,
    Memory,
    Other,
}

impl WorkContextKind {
    /// Whether this segment describes a capability whose availability and discovery are owned by
    /// CapabilityResolver and the selected Runtime Adapter rather than by ContextPlan.
    pub fn is_capability(&self) -> bool {
        matches!(
            self,
            Self::Skill
                | Self::Mcp
                | Self::Connector
                | Self::AgentPlugin
                | Self::WebAccess
                | Self::BrowserUse
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BasePolicy => "base_policy",
            Self::RuntimeInstructions => "runtime_instructions",
            Self::WorkPreset => "work_preset",
            Self::WorkspaceRules => "workspace_rules",
            Self::WorkspaceContext => "workspace_context",
            Self::CurrentGoal => "current_goal",
            Self::CurrentTask => "current_task",
            Self::Conversation => "conversation",
            Self::Skill => "skill",
            Self::Mcp => "mcp",
            Self::Connector => "connector",
            Self::AgentPlugin => "agent_plugin",
            Self::WebAccess => "web_access",
            Self::BrowserUse => "browser_use",
            Self::Library => "library",
            Self::Artifact => "artifact",
            Self::Memory => "memory",
            Self::Other => "other",
        }
    }
}

/// Source origin of a context segment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkContextSource {
    System,
    Workspace,
    CapabilityCenter,
    TaskState,
    ArtifactStore,
    LibraryStore,
    UserPrompt,
    Runtime,
    Custom(String),
}

/// Selection status of a context segment in the current turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkContextSelection {
    /// Harness considers this information or capability relevant to the current turn.
    /// For capability segments, this does not mean that ContextPlan controls runtime loading.
    Selected,
    /// Available to the session/runtime, but not currently highlighted by the Harness as relevant.
    /// The runtime may still discover and use the capability through its native mechanism.
    Deferred,
    /// Explicitly rejected by an authoritative policy or security boundary. Enforcement remains
    /// in the Policy, Capability, or Runtime layer; ContextPlan records the observation.
    Rejected,
    /// Not currently available according to authoritative CapabilityResolver/runtime state.
    Unavailable,
}

/// One individual context segment within the WorkContextPlan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkContextSegment {
    pub id: String,
    pub kind: WorkContextKind,
    pub source: WorkContextSource,
    pub title: String,
    pub required: bool,
    pub selection: WorkContextSelection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub render_priority: i32,
    pub budget_priority: i32,
    pub estimated_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

/// Runtime-neutral Context Plan representing the full context assembly for a Work run/turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkContextPlan {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub runtime: RuntimeProviderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<WorkPreset>,
    pub segments: Vec<WorkContextSegment>,
    pub estimated_tokens: u64,
    pub created_at: String,
}

impl WorkContextPlan {
    pub fn selected_segments(&self) -> impl Iterator<Item = &WorkContextSegment> {
        self.segments
            .iter()
            .filter(|s| s.selection == WorkContextSelection::Selected)
    }

    pub fn deferred_segments(&self) -> impl Iterator<Item = &WorkContextSegment> {
        self.segments
            .iter()
            .filter(|s| s.selection == WorkContextSelection::Deferred)
    }

    pub fn rejected_segments(&self) -> impl Iterator<Item = &WorkContextSegment> {
        self.segments
            .iter()
            .filter(|s| s.selection == WorkContextSelection::Rejected)
    }

    pub fn unavailable_segments(&self) -> impl Iterator<Item = &WorkContextSegment> {
        self.segments
            .iter()
            .filter(|s| s.selection == WorkContextSelection::Unavailable)
    }

    pub fn required_segments(&self) -> impl Iterator<Item = &WorkContextSegment> {
        self.segments.iter().filter(|s| s.required)
    }

    pub fn recalculate_tokens(&mut self) {
        // Estimate exactly what the runtime projection renders. Capability observation segments
        // are intentionally excluded from that projection and therefore from this cost.
        self.estimated_tokens = estimate_tokens(&self.render_system_prompt());
    }

    /// Return capability IDs for Harness relevance observability only. These helpers must not be
    /// used to filter runtime skill projection, MCP servers, connectors, or tool availability.
    pub fn selected_skill_ids(&self) -> Vec<String> {
        self.selected_segments()
            .filter(|s| s.kind == WorkContextKind::Skill)
            .filter_map(|s| s.ref_id.clone())
            .collect()
    }

    /// Return deferred capability IDs for observability only; runtime discovery remains intact.
    pub fn deferred_skill_ids(&self) -> Vec<String> {
        self.deferred_segments()
            .filter(|s| s.kind == WorkContextKind::Skill)
            .filter_map(|s| s.ref_id.clone())
            .collect()
    }

    /// Renders injected Work context from ordinary Selected segments. Capability segments are
    /// observations owned by the Harness and are intentionally not rendered: the Runtime Adapter
    /// keeps the complete session capability set and the runtime owns progressive disclosure.
    /// The Conversation segment is also excluded because the original user message is sent
    /// separately as the current turn prompt. Non-capability policy boundaries remain visible.
    pub fn render_system_prompt(&self) -> String {
        let mut sections = Vec::new();

        // 1. Render all Selected segments in order of appearance (which are sorted by render_priority)
        for segment in self.selected_segments().filter(|segment| {
            segment.kind != WorkContextKind::Conversation && !segment.kind.is_capability()
        }) {
            if let Some(content) = &segment.content {
                if !content.trim().is_empty() {
                    sections.push(format!("### {}\n{}", segment.title, content.trim()));
                }
            }
        }

        // 2. Render non-capability policy/security boundaries. Capability rejection remains
        // observable in the Plan but does not become a second capability visibility mechanism.
        let rejected: Vec<_> = self
            .rejected_segments()
            .filter(|segment| !segment.kind.is_capability())
            .collect();
        if !rejected.is_empty() {
            let mut rej_text =
                String::from("The following resources or actions are restricted by policy:\n");
            for s in rejected {
                rej_text.push_str(&format!(
                    "- {}: {}\n",
                    s.title,
                    s.reason.as_deref().unwrap_or("Policy restriction")
                ));
            }
            sections.push(format!("### Policy Restrictions\n{}", rej_text.trim()));
        }

        sections.join("\n\n")
    }

    pub fn envelope(&self) -> WorkContextEnvelope {
        WorkContextEnvelope::from_plan(self)
    }

    pub fn capability_snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::from_plan(self)
    }
}

/// Structured Work Context Envelope for direct prompt injection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkContextEnvelope {
    pub base_policy: Option<String>,
    pub preset_guidance: Option<String>,
    pub runtime_instructions: Option<String>,
    pub workspace_boundary: Option<String>,
    pub workspace_rules: Option<String>,
    pub goal_and_plan: Option<String>,
    pub artifacts_summary: Option<String>,
    pub policy_restrictions: Option<String>,
    pub custom_rules: Option<String>,
    pub rendered_prompt: String,
}

impl WorkContextEnvelope {
    pub fn from_plan(plan: &WorkContextPlan) -> Self {
        let mut base_policy: Option<String> = None;
        let mut preset_guidance: Option<String> = None;
        let mut runtime_instructions: Option<String> = None;
        let mut workspace_boundary: Option<String> = None;
        let mut workspace_rules: Option<String> = None;
        let mut goal_and_plan: Option<String> = None;
        let mut artifacts_summary: Option<String> = None;
        let mut custom_rules: Option<String> = None;

        for seg in plan.selected_segments() {
            match seg.kind {
                WorkContextKind::BasePolicy => base_policy = seg.content.clone(),
                WorkContextKind::WorkPreset => preset_guidance = seg.content.clone(),
                WorkContextKind::RuntimeInstructions => runtime_instructions = seg.content.clone(),
                WorkContextKind::WorkspaceContext => workspace_boundary = seg.content.clone(),
                WorkContextKind::WorkspaceRules => {
                    if seg.id.contains("custom") {
                        custom_rules = seg.content.clone();
                    } else {
                        workspace_rules = seg.content.clone();
                    }
                }
                WorkContextKind::CurrentGoal | WorkContextKind::CurrentTask => {
                    if let Some(c) = &seg.content {
                        if let Some(existing) = &mut goal_and_plan {
                            existing.push_str("\n\n");
                            existing.push_str(c);
                        } else {
                            goal_and_plan = Some(c.clone());
                        }
                    }
                }
                WorkContextKind::Artifact => {
                    if let Some(c) = &seg.content {
                        if let Some(existing) = &mut artifacts_summary {
                            existing.push('\n');
                            existing.push_str(c);
                        } else {
                            artifacts_summary = Some(c.clone());
                        }
                    }
                }
                _ => {}
            }
        }

        let rendered_prompt = plan.render_system_prompt();
        let policy_restrictions = {
            let rej: Vec<_> = plan
                .rejected_segments()
                .filter(|s| !s.kind.is_capability())
                .collect();
            if rej.is_empty() {
                None
            } else {
                let mut text =
                    String::from("The following resources or actions are restricted by policy:\n");
                for s in rej {
                    text.push_str(&format!(
                        "- {}: {}\n",
                        s.title,
                        s.reason.as_deref().unwrap_or("Policy restriction")
                    ));
                }
                Some(text.trim().to_string())
            }
        };

        Self {
            base_policy,
            preset_guidance,
            runtime_instructions,
            workspace_boundary,
            workspace_rules,
            goal_and_plan,
            artifacts_summary,
            policy_restrictions,
            custom_rules,
            rendered_prompt,
        }
    }
}

/// Clean snapshot of available session capabilities for UI inspector without token heuristics.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySnapshot {
    pub skills: Vec<String>,
    pub mcp_servers: Vec<String>,
    pub connectors: Vec<String>,
    pub web_access: bool,
    pub browser_use: bool,
}

impl CapabilitySnapshot {
    pub fn from_capabilities(capabilities: &EffectiveCapabilities) -> Self {
        let skills = capabilities
            .enabled_skills
            .iter()
            .map(|s| s.name.clone())
            .collect();
        let mcp_servers = capabilities
            .mcp_servers
            .iter()
            .map(|s| s.id.clone())
            .collect();
        let connectors = capabilities
            .connectors
            .iter()
            .map(|s| s.name.clone())
            .collect();
        let web_access = capabilities.browser_enabled;
        let browser_use = capabilities.browser_use_enabled;

        Self {
            skills,
            mcp_servers,
            connectors,
            web_access,
            browser_use,
        }
    }

    pub fn from_plan(plan: &WorkContextPlan) -> Self {
        let skills = plan
            .segments
            .iter()
            .filter(|s| s.kind == WorkContextKind::Skill)
            .map(|s| s.title.clone())
            .collect();
        let mcp_servers = plan
            .segments
            .iter()
            .filter(|s| s.kind == WorkContextKind::Mcp)
            .map(|s| s.title.clone())
            .collect();
        let connectors = plan
            .segments
            .iter()
            .filter(|s| s.kind == WorkContextKind::Connector)
            .map(|s| s.title.clone())
            .collect();
        let web_access = plan.segments.iter().any(|s| {
            s.kind == WorkContextKind::WebAccess && s.selection == WorkContextSelection::Selected
        });
        let browser_use = plan.segments.iter().any(|s| {
            s.kind == WorkContextKind::BrowserUse && s.selection == WorkContextSelection::Selected
        });

        Self {
            skills,
            mcp_servers,
            connectors,
            web_access,
            browser_use,
        }
    }
}

/// Input payload passed to WorkContextAssembler to assemble a WorkContextPlan.
#[derive(Debug, Clone)]
pub struct WorkContextAssemblyInput {
    pub app_mode: AppMode,
    pub runtime: RuntimeProviderKind,
    pub run_id: String,
    pub session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub task_id: Option<String>,
    pub preset: Option<WorkPreset>,
    pub execution_context: Option<ExecutionContext>,
    pub prompt: Option<String>,
    pub workspace: Option<WorkWorkspace>,
    pub task_state: Option<WorkTaskState>,
    pub task: Option<WorkTask>,
    pub capabilities: EffectiveCapabilities,
    pub artifacts: Vec<WorkArtifactSummary>,
    pub library_items: Vec<LibraryItemSummary>,
    pub custom_rules: Option<String>,
}

/// Heuristic deterministic token count estimator.
/// - ASCII words / whitespace: ~4 chars per token
/// - Non-ASCII / CJK: ~1.5 chars per token
pub fn estimate_tokens(text: &str) -> u64 {
    if text.is_empty() {
        return 0;
    }
    let mut ascii_chars = 0u64;
    let mut non_ascii_chars = 0u64;

    for c in text.chars() {
        if c.is_ascii() {
            ascii_chars += 1;
        } else {
            non_ascii_chars += 1;
        }
    }

    let ascii_tokens = ascii_chars.div_ceil(4);
    let non_ascii_tokens = (non_ascii_chars * 2).div_ceil(3);
    let total = ascii_tokens + non_ascii_tokens;
    total.max(1)
}
