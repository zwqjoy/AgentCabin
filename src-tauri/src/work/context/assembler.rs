use chrono::Utc;
use std::collections::BTreeMap;

use crate::models::StructuredTaskStatus;
use crate::work::context::models::{
    estimate_tokens, WorkContextAssemblyInput, WorkContextKind, WorkContextPlan,
    WorkContextSegment, WorkContextSelection, WorkContextSource,
};
use crate::work::resources;

pub struct WorkContextAssembler;

impl WorkContextAssembler {
    /// Assemble a deterministic WorkContextPlan from the supplied assembly input using the current timestamp.
    pub fn assemble(input: WorkContextAssemblyInput) -> WorkContextPlan {
        Self::assemble_with_timestamp(input, Utc::now().to_rfc3339())
    }

    /// Assemble a deterministic WorkContextPlan with an explicit timestamp.
    pub fn assemble_with_timestamp(
        input: WorkContextAssemblyInput,
        created_at: String,
    ) -> WorkContextPlan {
        let mut segments = Vec::new();

        // 1. Required Base Policy
        let base_policy_text = "AgentCabin Work Mode Base Policy: Workspace is the default work boundary. Read source from input/, drafts in scratch/, deliverables in output/. Final deliverables must be registered as artifacts. Respect tool execution blockers.";
        let mut base_policy_meta = BTreeMap::new();
        base_policy_meta.insert("policy_version".to_string(), "1".to_string());
        segments.push(WorkContextSegment {
            id: "system:base_policy".to_string(),
            kind: WorkContextKind::BasePolicy,
            source: WorkContextSource::System,
            title: "Work Base Policy".to_string(),
            required: true,
            selection: WorkContextSelection::Selected,
            reason: Some("Required Work isolation and safety policy".to_string()),
            render_priority: 100,
            budget_priority: 100,
            estimated_tokens: estimate_tokens(base_policy_text),
            ref_id: None,
            content: Some(base_policy_text.to_string()),
            metadata: base_policy_meta,
        });

        // 2. Work Preset
        if let Some(preset) = input.preset {
            let mut preset_meta = BTreeMap::new();
            preset_meta.insert("preset".to_string(), preset.as_str().to_string());
            let preset_text = match preset {
                crate::work::models::WorkPreset::Office => {
                    "Office preset: Focus on structured documents, spreadsheets, presentations, and synthesis."
                }
                crate::work::models::WorkPreset::Code => {
                    "Code preset: Focus on repository analysis, minimal targeted code changes, and test validation."
                }
                crate::work::models::WorkPreset::Creative => {
                    "Creative preset: Focus on copywriting, visual structure, and creative delivery."
                }
            };
            segments.push(WorkContextSegment {
                id: format!("preset:{}", preset.as_str()),
                kind: WorkContextKind::WorkPreset,
                source: WorkContextSource::System,
                title: format!("Work Preset ({})", preset.as_str()),
                required: true,
                selection: WorkContextSelection::Selected,
                reason: Some(format!("Active Work preset guidance: {}", preset.as_str())),
                render_priority: 90,
                budget_priority: 90,
                estimated_tokens: estimate_tokens(preset_text),
                ref_id: None,
                content: Some(preset_text.to_string()),
                metadata: preset_meta,
            });
        }

        // 3. Runtime Instructions
        let runtime_text = format!(
            "Runtime Provider: {}. Standard execution and tool calling through AgentCabin harness.",
            input.runtime.as_str()
        );
        let mut runtime_meta = BTreeMap::new();
        runtime_meta.insert("provider".to_string(), input.runtime.as_str().to_string());
        segments.push(WorkContextSegment {
            id: format!("runtime:instructions:{}", input.runtime.as_str()),
            kind: WorkContextKind::RuntimeInstructions,
            source: WorkContextSource::Runtime,
            title: format!("Runtime Instructions ({})", input.runtime.as_str()),
            required: true,
            selection: WorkContextSelection::Selected,
            reason: Some("Runtime provider execution instructions".to_string()),
            render_priority: 85,
            budget_priority: 85,
            estimated_tokens: estimate_tokens(&runtime_text),
            ref_id: None,
            content: Some(runtime_text.clone()),
            metadata: runtime_meta,
        });

        // 4. Workspace Rules and Context Boundary (Workspace Isolation)
        if let Some(ws) = &input.workspace {
            let is_matched = input.workspace_id.as_deref() == Some(&ws.id);
            if !is_matched {
                // Explicit rejection due to workspace ID mismatch
                let reject_reason = format!(
                    "Workspace ID mismatch: input requested workspace '{:?}', but provided workspace object has ID '{}'",
                    input.workspace_id, ws.id
                );
                segments.push(WorkContextSegment {
                    id: format!("workspace:rejected:{}", ws.id),
                    kind: WorkContextKind::WorkspaceContext,
                    source: WorkContextSource::Workspace,
                    title: format!("Workspace Isolation Rejection ({})", ws.name),
                    required: false,
                    selection: WorkContextSelection::Rejected,
                    reason: Some(reject_reason.clone()),
                    render_priority: 80,
                    budget_priority: 80,
                    estimated_tokens: estimate_tokens(&reject_reason),
                    ref_id: Some(ws.id.clone()),
                    content: Some(format!(
                        "Access to workspace '{}' is denied due to workspace ID mismatch.",
                        ws.name
                    )),
                    metadata: BTreeMap::new(),
                });
            } else {
                let ws_id = &ws.id;
                // Workspace rules (e.g. custom_rules / AGENTS.md)
                if let Some(rules) = &input.custom_rules {
                    if !rules.trim().is_empty() {
                        let mut rules_meta = BTreeMap::new();
                        rules_meta.insert("workspace_id".to_string(), ws_id.clone());
                        segments.push(WorkContextSegment {
                            id: format!("workspace:rules:{}", ws_id),
                            kind: WorkContextKind::WorkspaceRules,
                            source: WorkContextSource::Workspace,
                            title: format!("Workspace Rules ({})", ws.name),
                            required: true,
                            selection: WorkContextSelection::Selected,
                            reason: Some(
                                "Workspace custom rules and AGENTS.md instructions".to_string(),
                            ),
                            render_priority: 80,
                            budget_priority: 80,
                            estimated_tokens: estimate_tokens(rules),
                            ref_id: Some(ws_id.clone()),
                            content: Some(rules.clone()),
                            metadata: rules_meta,
                        });
                    }
                }

                // Workspace boundary context
                let boundary_text = format!(
                    "Workspace: {} (root: {}). input: {}, scratch: {}, output: {}, context: {}",
                    ws.name, ws.root, ws.input_dir, ws.scratch_dir, ws.output_dir, ws.context_dir
                );
                let mut boundary_meta = BTreeMap::new();
                boundary_meta.insert("workspace_id".to_string(), ws_id.clone());
                boundary_meta.insert("workspace_name".to_string(), ws.name.clone());
                segments.push(WorkContextSegment {
                    id: format!("workspace:boundary:{}", ws_id),
                    kind: WorkContextKind::WorkspaceContext,
                    source: WorkContextSource::Workspace,
                    title: format!("Workspace Boundary ({})", ws.name),
                    required: true,
                    selection: WorkContextSelection::Selected,
                    reason: Some("Authorized workspace directories and boundaries".to_string()),
                    render_priority: 75,
                    budget_priority: 75,
                    estimated_tokens: estimate_tokens(&boundary_text),
                    ref_id: Some(ws_id.clone()),
                    content: Some(boundary_text),
                    metadata: boundary_meta,
                });
            }
        }

        // 5. Current Goal
        if let Some(state) = &input.task_state {
            if let Some(goal) = &state.goal {
                if !goal.trim().is_empty() {
                    let mut goal_meta = BTreeMap::new();
                    goal_meta.insert("goal".to_string(), goal.clone());
                    segments.push(WorkContextSegment {
                        id: "task_state:goal".to_string(),
                        kind: WorkContextKind::CurrentGoal,
                        source: WorkContextSource::TaskState,
                        title: "Current Goal".to_string(),
                        required: true,
                        selection: WorkContextSelection::Selected,
                        reason: Some("Active user goal".to_string()),
                        render_priority: 70,
                        budget_priority: 70,
                        estimated_tokens: estimate_tokens(goal),
                        ref_id: None,
                        content: Some(format!("Active Goal: {}", goal)),
                        metadata: goal_meta,
                    });
                }
            }

            // 6. Current Task / Active Step in Plan
            let active_step = state
                .plan
                .iter()
                .find(|t| t.status == StructuredTaskStatus::InProgress);
            if let Some(step) = active_step {
                let mut step_meta = BTreeMap::new();
                step_meta.insert("step_id".to_string(), step.id.clone());
                step_meta.insert("step_text".to_string(), step.text.clone());
                let step_content = format!("Active Step [{}]: {}", step.id, step.text);
                segments.push(WorkContextSegment {
                    id: format!("task_state:step:{}", step.id),
                    kind: WorkContextKind::CurrentTask,
                    source: WorkContextSource::TaskState,
                    title: format!("Active Step: {}", step.text),
                    required: true,
                    selection: WorkContextSelection::Selected,
                    reason: Some("Current in-progress task step".to_string()),
                    render_priority: 65,
                    budget_priority: 65,
                    estimated_tokens: estimate_tokens(&step.text),
                    ref_id: Some(step.id.clone()),
                    content: Some(step_content),
                    metadata: step_meta,
                });
            } else if let Some(task) = &input.task {
                let mut task_meta = BTreeMap::new();
                task_meta.insert("task_id".to_string(), task.id.clone());
                task_meta.insert("title".to_string(), task.title.clone());
                let task_text =
                    format!("Task: {}\nInstructions: {}", task.title, task.instructions);
                segments.push(WorkContextSegment {
                    id: format!("task:{}", task.id),
                    kind: WorkContextKind::CurrentTask,
                    source: WorkContextSource::TaskState,
                    title: format!("Task: {}", task.title),
                    required: true,
                    selection: WorkContextSelection::Selected,
                    reason: Some("Assigned work task".to_string()),
                    render_priority: 65,
                    budget_priority: 65,
                    estimated_tokens: estimate_tokens(&task_text),
                    ref_id: Some(task.id.clone()),
                    content: Some(task_text),
                    metadata: task_meta,
                });
            }
        }

        // 7. Conversation / User Prompt
        if let Some(prompt) = &input.prompt {
            if !prompt.trim().is_empty() {
                segments.push(WorkContextSegment {
                    id: "conversation:user_prompt".to_string(),
                    kind: WorkContextKind::Conversation,
                    source: WorkContextSource::UserPrompt,
                    title: "User Prompt".to_string(),
                    required: true,
                    selection: WorkContextSelection::Selected,
                    reason: Some("Current user message".to_string()),
                    render_priority: 60,
                    budget_priority: 60,
                    estimated_tokens: estimate_tokens(prompt),
                    ref_id: None,
                    content: Some(format!("User Prompt:\n{}", prompt)),
                    metadata: BTreeMap::new(),
                });
            }
        }

        // --- Collect Query Tokens for Capability & Resource Selection ---
        let mut query_text = String::new();
        if let Some(p) = &input.prompt {
            query_text.push_str(p);
            query_text.push(' ');
        }
        if let Some(state) = &input.task_state {
            if let Some(g) = &state.goal {
                query_text.push_str(g);
                query_text.push(' ');
            }
            for step in &state.plan {
                query_text.push_str(&step.text);
                query_text.push(' ');
            }
        }
        if let Some(t) = &input.task {
            query_text.push_str(&t.title);
            query_text.push(' ');
            query_text.push_str(&t.instructions);
            query_text.push(' ');
        }

        let tokens = resources::query_tokens(&query_text);
        let is_operational_query = resources::has_operational_intent(&tokens);

        // 8. Capabilities: Skills
        for skill in &input.capabilities.enabled_skills {
            let mut skill_meta = BTreeMap::new();
            skill_meta.insert("name".to_string(), skill.name.clone());
            skill_meta.insert(
                "path".to_string(),
                skill.path.to_string_lossy().into_owned(),
            );
            if let Some(desc) = &skill.description {
                skill_meta.insert("description".to_string(), desc.clone());
            }

            let (selection, reason, content) = if !is_operational_query {
                (
                    WorkContextSelection::Deferred,
                    Some(
                        "Available to this session; not currently highlighted by the Harness for this turn"
                            .to_string(),
                    ),
                    None,
                )
            } else {
                let score = resources::score_text_match(
                    &skill.name,
                    skill.description.as_deref().unwrap_or(""),
                    &tokens,
                );
                if score > 0 {
                    (
                        WorkContextSelection::Selected,
                        Some(format!(
                            "Harness considers this capability relevant to the current turn (score {score})"
                        )),
                        Some(format!(
                            "Skill `{}`: {}",
                            skill.name,
                            skill
                                .description
                                .as_deref()
                                .unwrap_or("Installed workspace skill.")
                        )),
                    )
                } else {
                    (
                        WorkContextSelection::Deferred,
                        Some(
                            "Available to this session; not currently highlighted by the Harness for this turn"
                                .to_string(),
                        ),
                        None,
                    )
                }
            };

            let approx_tokens = estimate_tokens(&format!(
                "Skill {}: {}",
                skill.name,
                skill.description.as_deref().unwrap_or("")
            ));

            segments.push(WorkContextSegment {
                id: format!("capability:skill:{}", skill.id),
                kind: WorkContextKind::Skill,
                source: WorkContextSource::CapabilityCenter,
                title: format!("Skill: {}", skill.name),
                required: false,
                selection,
                reason,
                render_priority: 50,
                budget_priority: 50,
                estimated_tokens: approx_tokens,
                ref_id: Some(skill.id.clone()),
                content,
                metadata: skill_meta,
            });
        }

        // 9. Capabilities: MCP Servers
        for server in &input.capabilities.mcp_servers {
            let mut mcp_meta = BTreeMap::new();
            mcp_meta.insert("transport".to_string(), server.transport.clone());
            if let Some(cmd) = &server.command {
                mcp_meta.insert("command".to_string(), cmd.clone());
            }

            let (selection, reason, content) = if !is_operational_query {
                (
                    WorkContextSelection::Deferred,
                    Some(
                        "Available to this session; not currently highlighted by the Harness for this turn"
                            .to_string(),
                    ),
                    None,
                )
            } else {
                let mcp_desc = format!(
                    "{} {}",
                    server.command.as_deref().unwrap_or(""),
                    server.args.join(" ")
                );
                let score = resources::score_text_match(&server.id, &mcp_desc, &tokens);
                if score > 0 {
                    (
                        WorkContextSelection::Selected,
                        Some(format!(
                            "Harness considers this capability relevant to the current turn (score {score})"
                        )),
                        Some(format!(
                            "MCP Server `{}` (transport: {})",
                            server.id, server.transport
                        )),
                    )
                } else {
                    (
                        WorkContextSelection::Deferred,
                        Some(
                            "Available to this session; not currently highlighted by the Harness for this turn"
                                .to_string(),
                        ),
                        None,
                    )
                }
            };

            segments.push(WorkContextSegment {
                id: format!("capability:mcp:{}", server.id),
                kind: WorkContextKind::Mcp,
                source: WorkContextSource::CapabilityCenter,
                title: format!("MCP Server: {}", server.id),
                required: false,
                selection,
                reason,
                render_priority: 50,
                budget_priority: 50,
                estimated_tokens: 30,
                ref_id: Some(server.id.clone()),
                content,
                metadata: mcp_meta,
            });
        }

        // 10. Capabilities: Connectors
        for conn in &input.capabilities.connectors {
            let mut conn_meta = BTreeMap::new();
            conn_meta.insert("name".to_string(), conn.name.clone());

            let (selection, reason, content) = if !is_operational_query {
                (
                    WorkContextSelection::Deferred,
                    Some(
                        "Available to this session; not currently highlighted by the Harness for this turn"
                            .to_string(),
                    ),
                    None,
                )
            } else {
                let score = resources::score_text_match(&conn.name, &conn.id, &tokens);
                if score > 0 {
                    (
                        WorkContextSelection::Selected,
                        Some(format!(
                            "Harness considers this capability relevant to the current turn (score {score})"
                        )),
                        Some(format!("Connector `{}`: {}", conn.name, conn.id)),
                    )
                } else {
                    (
                        WorkContextSelection::Deferred,
                        Some(
                            "Available to this session; not currently highlighted by the Harness for this turn"
                                .to_string(),
                        ),
                        None,
                    )
                }
            };

            segments.push(WorkContextSegment {
                id: format!("capability:connector:{}", conn.id),
                kind: WorkContextKind::Connector,
                source: WorkContextSource::CapabilityCenter,
                title: format!("Connector: {}", conn.name),
                required: false,
                selection,
                reason,
                render_priority: 50,
                budget_priority: 50,
                estimated_tokens: 30,
                ref_id: Some(conn.id.clone()),
                content,
                metadata: conn_meta,
            });
        }

        // 11. Capabilities: Web Access & Browser
        if input.capabilities.browser_enabled {
            let (selection, reason, content) = if !is_operational_query {
                (
                    WorkContextSelection::Deferred,
                    Some(
                        "Web access is available to the session; the Harness is not currently highlighting it for this turn"
                            .to_string(),
                    ),
                    None,
                )
            } else {
                let web_score = resources::score_text_match(
                    "web browser search internet url fetch",
                    "",
                    &tokens,
                );
                if web_score > 0 {
                    (
                        WorkContextSelection::Selected,
                        Some(
                            "Harness considers web access relevant to the current turn".to_string(),
                        ),
                        Some("Web search and URL fetch access is enabled.".to_string()),
                    )
                } else {
                    (
                        WorkContextSelection::Deferred,
                        Some(
                            "Web access is available to the session; the Harness is not currently highlighting it for this turn"
                                .to_string(),
                        ),
                        None,
                    )
                }
            };

            segments.push(WorkContextSegment {
                id: "capability:web_access".to_string(),
                kind: WorkContextKind::WebAccess,
                source: WorkContextSource::CapabilityCenter,
                title: "Web Access & Search".to_string(),
                required: false,
                selection,
                reason,
                render_priority: 50,
                budget_priority: 50,
                estimated_tokens: 25,
                ref_id: None,
                content,
                metadata: BTreeMap::new(),
            });
        } else {
            // Explicitly record browser capability as unavailable when disabled
            segments.push(WorkContextSegment {
                id: "capability:browser_use".to_string(),
                kind: WorkContextKind::BrowserUse,
                source: WorkContextSource::CapabilityCenter,
                title: "Browser Control".to_string(),
                required: false,
                selection: WorkContextSelection::Unavailable,
                reason: Some("Browser capability is disabled in environment settings".to_string()),
                render_priority: 50,
                budget_priority: 50,
                estimated_tokens: 15,
                ref_id: None,
                content: None,
                metadata: BTreeMap::new(),
            });
        }

        // 12. Security & Policy Violations (Rejected Paths)
        for prohibited in &input.capabilities.detected_prohibited_paths {
            let path_name = prohibited
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let reason = format!(
                "Security policy violation: prohibited path {}",
                prohibited.display()
            );
            segments.push(WorkContextSegment {
                id: format!("security:prohibited:{}", path_name),
                kind: WorkContextKind::Other,
                source: WorkContextSource::System,
                title: format!("Prohibited Path ({})", path_name),
                required: false,
                selection: WorkContextSelection::Rejected,
                reason: Some(reason.clone()),
                render_priority: 45,
                budget_priority: 45,
                estimated_tokens: estimate_tokens(&reason),
                ref_id: None,
                content: Some(format!(
                    "Access to prohibited path `{}` is blocked by policy.",
                    prohibited.display()
                )),
                metadata: BTreeMap::new(),
            });
        }

        // 13. Artifact References
        for art in &input.artifacts {
            let mut art_meta = BTreeMap::new();
            art_meta.insert("artifact_type".to_string(), art.artifact_type.clone());
            art_meta.insert("path".to_string(), art.path.clone());
            art_meta.insert("size".to_string(), art.size.to_string());
            art_meta.insert("status".to_string(), format!("{:?}", art.status));
            if let Some(val) = &art.validation_summary {
                art_meta.insert("validation_summary".to_string(), val.clone());
            }

            let (selection, reason, content) = if is_operational_query {
                let score = resources::score_text_match(&art.title, &art.path, &tokens);
                let created_in_run = art.run_id.as_deref() == Some(&input.run_id);
                if score > 0 || created_in_run {
                    (
                        WorkContextSelection::Selected,
                        Some("Relevant workspace deliverable artifact".to_string()),
                        Some(format!(
                            "Artifact `{}` (type: {}, path: {}, size: {} bytes, status: {:?})",
                            art.title, art.artifact_type, art.path, art.size, art.status
                        )),
                    )
                } else {
                    (
                        WorkContextSelection::Deferred,
                        Some("Existing artifact referenced in workspace".to_string()),
                        None,
                    )
                }
            } else {
                (
                    WorkContextSelection::Deferred,
                    Some("Existing artifact in workspace".to_string()),
                    None,
                )
            };

            segments.push(WorkContextSegment {
                id: format!("artifact:{}", art.id),
                kind: WorkContextKind::Artifact,
                source: WorkContextSource::ArtifactStore,
                title: format!("Artifact: {}", art.title),
                required: false,
                selection,
                reason,
                render_priority: 40,
                budget_priority: 40,
                estimated_tokens: 20,
                ref_id: Some(art.id.clone()),
                content,
                metadata: art_meta,
            });
        }

        // 14. Library References
        for lib in &input.library_items {
            let mut lib_meta = BTreeMap::new();
            lib_meta.insert("title".to_string(), lib.title.clone());
            lib_meta.insert("category".to_string(), format!("{:?}", lib.category));
            lib_meta.insert("preview".to_string(), lib.content_preview.clone());

            let (selection, reason, content) = if is_operational_query {
                let score = resources::score_text_match(&lib.title, &lib.description, &tokens);
                if score > 0 {
                    (
                        WorkContextSelection::Selected,
                        Some("Relevant workspace library reference item".to_string()),
                        Some(format!(
                            "Library Reference `{}` ({:?}): {}",
                            lib.title, lib.category, lib.content_preview
                        )),
                    )
                } else {
                    (
                        WorkContextSelection::Deferred,
                        Some("Library item available in workspace library".to_string()),
                        None,
                    )
                }
            } else {
                (
                    WorkContextSelection::Deferred,
                    Some("Library item in workspace library".to_string()),
                    None,
                )
            };

            segments.push(WorkContextSegment {
                id: format!("library:{}", lib.id),
                kind: WorkContextKind::Library,
                source: WorkContextSource::LibraryStore,
                title: format!("Library: {}", lib.title),
                required: false,
                selection,
                reason,
                render_priority: 30,
                budget_priority: 30,
                estimated_tokens: estimate_tokens(&lib.content_preview).max(15),
                ref_id: Some(lib.id.clone()),
                content,
                metadata: lib_meta,
            });
        }

        // 14. Deterministic Sorting:
        // 1st: render_priority descending
        // 2nd: kind ordinal
        // 3rd: id ascending
        segments.sort_by(|a, b| {
            b.render_priority
                .cmp(&a.render_priority)
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.id.cmp(&b.id))
        });

        let mut plan = WorkContextPlan {
            version: 1,
            workspace_id: input.workspace_id,
            task_id: input.task_id,
            run_id: input.run_id,
            session_id: input.session_id,
            runtime: input.runtime,
            preset: input.preset,
            segments,
            estimated_tokens: 0,
            created_at,
        };

        plan.recalculate_tokens();
        plan
    }
}
