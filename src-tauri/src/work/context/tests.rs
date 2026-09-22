use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use tempfile::TempDir;

use super::*;
use crate::agent::capability_resolver::{
    EffectiveCapabilities, EffectiveConnector, EffectiveMcpServer, EffectiveSkill,
    RuntimeProviderKind,
};
use crate::models::{StructuredTask, StructuredTaskStatus};
use crate::work::models::{
    AppMode, LibraryCategory, LibraryItemSummary, WorkArtifactCategory, WorkArtifactPreviewKind,
    WorkArtifactStatus, WorkArtifactSummary, WorkPolicy, WorkPreset, WorkRootKind, WorkTaskState,
    WorkWorkspace,
};

fn fake_capabilities(runtime: RuntimeProviderKind) -> EffectiveCapabilities {
    EffectiveCapabilities {
        app_mode: AppMode::Work,
        runtime,
        run_id: "run-test-1".to_string(),
        managed_home: PathBuf::from("/tmp/managed_home"),
        managed_runtime_dir: PathBuf::from("/tmp/managed_runtime_dir"),
        enabled_skills: vec![
            EffectiveSkill {
                id: "excel-spreadsheet".to_string(),
                owner: None,
                name: "excel-spreadsheet".to_string(),
                path: PathBuf::from("/skills/excel"),
                description: Some(
                    "Analyze, calculate, and format Excel spreadsheets and xlsx/csv files"
                        .to_string(),
                ),
            },
            EffectiveSkill {
                id: "gmail-manager".to_string(),
                owner: None,
                name: "gmail-manager".to_string(),
                path: PathBuf::from("/skills/gmail"),
                description: Some("Read, draft, and send emails via Gmail".to_string()),
            },
            EffectiveSkill {
                id: "github-tools".to_string(),
                owner: None,
                name: "github-tools".to_string(),
                path: PathBuf::from("/skills/github"),
                description: Some(
                    "Inspect repositories, create branches and PRs on GitHub".to_string(),
                ),
            },
        ],
        mcp_servers: vec![EffectiveMcpServer {
            id: "postgres-db".to_string(),
            transport: "stdio".to_string(),
            command: Some("npx".to_string()),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-postgres".to_string(),
            ],
            cwd: None,
            url: None,
            env: HashMap::new(),
            headers: HashMap::new(),
        }],
        connectors: vec![EffectiveConnector {
            id: "feishu-connector".to_string(),
            name: "Feishu / Lark".to_string(),
            package_path: PathBuf::from("/connectors/feishu"),
            entry_point: Some("index.js".to_string()),
        }],
        browser_enabled: true,
        browser_use_enabled: true,
        browser_config: None,
        allowed_tools: Vec::new(),
        disallowed_tools: Vec::new(),
        prohibited_discovery_paths: Vec::new(),
        detected_prohibited_paths: Vec::new(),
        diagnostics: Vec::new(),
        strict_mode: true,
    }
}

fn fake_workspace(id: &str, name: &str) -> WorkWorkspace {
    WorkWorkspace {
        id: id.to_string(),
        name: name.to_string(),
        root: format!("/workspaces/{id}"),
        input_dir: format!("/workspaces/{id}/input"),
        scratch_dir: format!("/workspaces/{id}/scratch"),
        output_dir: format!("/workspaces/{id}/output"),
        context_dir: format!("/workspaces/{id}/context"),
        created_at: "2026-08-15T00:00:00Z".to_string(),
        updated_at: "2026-08-15T00:00:00Z".to_string(),
        artifact_count: 0,
        archived: false,
        access_roots: Vec::new(),
        default_policy: WorkPolicy::default_for_workspace(),
        default_model: None,
        default_effort: None,
        root_kind: WorkRootKind::Managed,
        primary_work_root: None,
        working_root_valid: Some(true),
        artifact_storage_mode: crate::work::models::WorkArtifactStorageMode::Managed,
    }
}

// 1. deterministic assembly test
#[test]
fn test_deterministic_assembly() {
    let ws = fake_workspace("ws-1", "Finance");
    let input = WorkContextAssemblyInput {
        app_mode: AppMode::Work,
        runtime: RuntimeProviderKind::Pi,
        run_id: "run-deterministic-1".to_string(),
        session_id: Some("session-1".to_string()),
        workspace_id: Some("ws-1".to_string()),
        task_id: Some("task-1".to_string()),
        preset: Some(WorkPreset::Office),
        execution_context: None,
        prompt: Some("请分析 sales.xlsx 并生成报告".to_string()),
        workspace: Some(ws),
        task_state: Some(WorkTaskState {
            version: 1,
            revision: 1,
            goal: Some("财务分析".to_string()),
            goal_spec: None,
            plan: vec![StructuredTask {
                id: "step-1".to_string(),
                text: "读取表格".to_string(),
                status: StructuredTaskStatus::InProgress,
            }],
            checkpoint: None,
            pending_approval: None,
            updated_at: "2026-08-15T00:00:00Z".to_string(),
        }),
        task: None,
        capabilities: fake_capabilities(RuntimeProviderKind::Pi),
        artifacts: Vec::new(),
        library_items: Vec::new(),
        custom_rules: Some("Strict compliance with financial data standards.".to_string()),
    };

    let fixed_ts = "2026-08-15T00:00:00Z".to_string();
    let plan1 = WorkContextAssembler::assemble_with_timestamp(input.clone(), fixed_ts.clone());
    let plan2 = WorkContextAssembler::assemble_with_timestamp(input, fixed_ts);

    // Full equality assertion including created_at and segments
    assert_eq!(plan1, plan2);
}

// 2. workspace isolation test
#[test]
fn test_workspace_isolation() {
    let ws_a = fake_workspace("ws-a", "Workspace A");
    let input_a = WorkContextAssemblyInput {
        app_mode: AppMode::Work,
        runtime: RuntimeProviderKind::Claude,
        run_id: "run-a".to_string(),
        session_id: None,
        workspace_id: Some("ws-a".to_string()),
        task_id: None,
        preset: None,
        execution_context: None,
        prompt: Some("Hello".to_string()),
        workspace: Some(ws_a),
        task_state: None,
        task: None,
        capabilities: fake_capabilities(RuntimeProviderKind::Claude),
        artifacts: Vec::new(),
        library_items: Vec::new(),
        custom_rules: Some("Workspace A Confidential Rule".to_string()),
    };

    let plan_a = WorkContextAssembler::assemble(input_a);
    let rules_segment_a = plan_a
        .segments
        .iter()
        .find(|s| s.kind == WorkContextKind::WorkspaceRules);
    assert!(rules_segment_a.is_some());
    assert_eq!(
        rules_segment_a.unwrap().metadata.get("workspace_id"),
        Some(&"ws-a".to_string())
    );

    // Mismatched workspace_id vs workspace object (e.g. requested ws-requested but object is ws-different)
    let ws_different = fake_workspace("ws-different", "Different Workspace");
    let input_mismatch = WorkContextAssemblyInput {
        app_mode: AppMode::Work,
        runtime: RuntimeProviderKind::Claude,
        run_id: "run-mismatch".to_string(),
        session_id: None,
        workspace_id: Some("ws-requested".to_string()),
        task_id: None,
        preset: None,
        execution_context: None,
        prompt: Some("Hello".to_string()),
        workspace: Some(ws_different),
        task_state: None,
        task: None,
        capabilities: fake_capabilities(RuntimeProviderKind::Claude),
        artifacts: Vec::new(),
        library_items: Vec::new(),
        custom_rules: Some("Should not leak".to_string()),
    };
    let plan_mismatch = WorkContextAssembler::assemble(input_mismatch);
    // Must contain an explicit Rejected segment for workspace mismatch
    let rejected_ws = plan_mismatch
        .segments
        .iter()
        .find(|s| s.selection == WorkContextSelection::Rejected);
    assert!(
        rejected_ws.is_some(),
        "Mismatched workspace must yield Rejected segment"
    );
    assert!(rejected_ws
        .unwrap()
        .reason
        .as_deref()
        .unwrap_or("")
        .contains("Workspace ID mismatch"));
    // Must NOT contain Selected workspace rules or boundary
    assert!(!plan_mismatch.segments.iter().any(|s| {
        s.selection == WorkContextSelection::Selected
            && (s.kind == WorkContextKind::WorkspaceRules
                || s.kind == WorkContextKind::WorkspaceContext)
    }));

    // Standalone conversation (no workspace_id)
    let input_standalone = WorkContextAssemblyInput {
        app_mode: AppMode::Work,
        runtime: RuntimeProviderKind::Claude,
        run_id: "run-standalone".to_string(),
        session_id: None,
        workspace_id: None,
        task_id: None,
        preset: None,
        execution_context: None,
        prompt: Some("Hello".to_string()),
        workspace: None,
        task_state: None,
        task: None,
        capabilities: fake_capabilities(RuntimeProviderKind::Claude),
        artifacts: Vec::new(),
        library_items: Vec::new(),
        custom_rules: None,
    };
    let plan_standalone = WorkContextAssembler::assemble(input_standalone);
    assert!(!plan_standalone
        .segments
        .iter()
        .any(|s| s.kind == WorkContextKind::WorkspaceRules
            || s.kind == WorkContextKind::WorkspaceContext));
}

// 3. capability availability test
#[test]
fn test_capability_availability() {
    let mut caps = fake_capabilities(RuntimeProviderKind::Codex);
    // Disable browser and add detected prohibited path
    caps.browser_enabled = false;
    caps.detected_prohibited_paths = vec![PathBuf::from("/etc/shadow")];

    let input = WorkContextAssemblyInput {
        app_mode: AppMode::Work,
        runtime: RuntimeProviderKind::Codex,
        run_id: "run-caps".to_string(),
        session_id: None,
        workspace_id: None,
        task_id: None,
        preset: None,
        execution_context: None,
        prompt: Some("Check web page and search online".to_string()),
        workspace: None,
        task_state: None,
        task: None,
        capabilities: caps,
        artifacts: Vec::new(),
        library_items: Vec::new(),
        custom_rules: None,
    };

    let plan = WorkContextAssembler::assemble(input);
    // Disabled browser must appear as Unavailable
    let browser_seg = plan
        .segments
        .iter()
        .find(|s| s.id == "capability:browser_use")
        .expect("browser segment should exist");
    assert_eq!(browser_seg.selection, WorkContextSelection::Unavailable);

    // Prohibited path must appear as Rejected
    let prohibited_seg = plan
        .segments
        .iter()
        .find(|s| s.id == "security:prohibited:shadow")
        .expect("prohibited segment should exist");
    assert_eq!(prohibited_seg.selection, WorkContextSelection::Rejected);
}

// 4. deferred capability test
#[test]
fn test_deferred_capability() {
    let caps = fake_capabilities(RuntimeProviderKind::Pi);
    let input = WorkContextAssemblyInput {
        app_mode: AppMode::Work,
        runtime: RuntimeProviderKind::Pi,
        run_id: "run-deferred".to_string(),
        session_id: None,
        workspace_id: None,
        task_id: None,
        preset: None,
        execution_context: None,
        prompt: Some("编写一段 Python 脚本计算斐波那契数列".to_string()),
        workspace: None,
        task_state: None,
        task: None,
        capabilities: caps,
        artifacts: Vec::new(),
        library_items: Vec::new(),
        custom_rules: None,
    };

    let plan = WorkContextAssembler::assemble(input);
    // Gmail and Feishu are enabled in environment, but irrelevant to Python fibonacci calculation
    let gmail_seg = plan
        .segments
        .iter()
        .find(|s| s.id == "capability:skill:gmail-manager")
        .expect("gmail segment should exist");
    assert_eq!(gmail_seg.selection, WorkContextSelection::Deferred);

    let feishu_seg = plan
        .segments
        .iter()
        .find(|s| s.id == "capability:connector:feishu-connector")
        .expect("feishu connector segment should exist");
    assert_eq!(feishu_seg.selection, WorkContextSelection::Deferred);
}

// 5. simple conversation test
#[test]
fn test_simple_conversation() {
    let caps = fake_capabilities(RuntimeProviderKind::Pi);
    let input = WorkContextAssemblyInput {
        app_mode: AppMode::Work,
        runtime: RuntimeProviderKind::Pi,
        run_id: "run-simple".to_string(),
        session_id: None,
        workspace_id: None,
        task_id: None,
        preset: None,
        execution_context: None,
        prompt: Some("你好，在吗？".to_string()),
        workspace: None,
        task_state: None,
        task: None,
        capabilities: caps,
        artifacts: Vec::new(),
        library_items: Vec::new(),
        custom_rules: None,
    };

    let plan = WorkContextAssembler::assemble(input);
    // Required base policy should be selected
    let base_policy = plan
        .segments
        .iter()
        .find(|s| s.kind == WorkContextKind::BasePolicy)
        .expect("base policy must be selected");
    assert_eq!(base_policy.selection, WorkContextSelection::Selected);
    assert!(base_policy.required);

    // All operational capabilities should be Deferred
    for seg in &plan.segments {
        if matches!(
            seg.kind,
            WorkContextKind::Skill
                | WorkContextKind::Mcp
                | WorkContextKind::Connector
                | WorkContextKind::WebAccess
        ) {
            assert_eq!(
                seg.selection,
                WorkContextSelection::Deferred,
                "Capability {} should be Deferred in simple greeting conversation",
                seg.id
            );
        }
    }
}

// 6. operational task test
#[test]
fn test_operational_task() {
    let caps = fake_capabilities(RuntimeProviderKind::Pi);
    let input = WorkContextAssemblyInput {
        app_mode: AppMode::Work,
        runtime: RuntimeProviderKind::Pi,
        run_id: "run-operational".to_string(),
        session_id: None,
        workspace_id: None,
        task_id: None,
        preset: Some(WorkPreset::Office),
        execution_context: None,
        prompt: Some("分析 sales.xlsx 并生成报告".to_string()),
        workspace: None,
        task_state: None,
        task: None,
        capabilities: caps,
        artifacts: vec![WorkArtifactSummary {
            id: "art-1".to_string(),
            workspace_id: "ws-1".to_string(),
            run_id: Some("run-operational".to_string()),
            artifact_type: "spreadsheet".to_string(),
            category: WorkArtifactCategory::Spreadsheet,
            mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                .to_string(),
            title: "sales.xlsx".to_string(),
            path: "output/sales.xlsx".to_string(),
            status: WorkArtifactStatus::Ready,
            size: 2048,
            sha256: None,
            producer: None,
            evidence: None,
            validation_summary: Some("Generated 12 monthly sheets".to_string()),
            version: 1,
            preview_kind: WorkArtifactPreviewKind::Csv,
            can_preview: true,
            sources: Vec::new(),
            created_at: "2026-08-15T00:00:00Z".to_string(),
            updated_at: "2026-08-15T00:00:00Z".to_string(),
        }],
        library_items: vec![LibraryItemSummary {
            id: "lib-1".to_string(),
            workspace_id: Some("ws-1".to_string()),
            title: "Sales Report Template".to_string(),
            description: "Standard template for sales reporting and excel charts".to_string(),
            category: LibraryCategory::Template,
            tags: vec!["sales".to_string(), "report".to_string()],
            source_path: None,
            collection: None,
            metadata: BTreeMap::new(),
            citation_count: 0,
            content_preview: "Template instructions for quarterly sales".to_string(),
            source_artifact_id: None,
            updated_at: "2026-08-15T00:00:00Z".to_string(),
        }],
        custom_rules: None,
    };

    let plan = WorkContextAssembler::assemble(input);

    // Excel skill MUST be Selected
    let excel_skill = plan
        .segments
        .iter()
        .find(|s| s.id == "capability:skill:excel-spreadsheet")
        .expect("excel skill should exist");
    assert_eq!(excel_skill.selection, WorkContextSelection::Selected);
    assert!(excel_skill.content.is_some());

    // Gmail skill MUST remain Deferred
    let gmail_skill = plan
        .segments
        .iter()
        .find(|s| s.id == "capability:skill:gmail-manager")
        .expect("gmail skill should exist");
    assert_eq!(gmail_skill.selection, WorkContextSelection::Deferred);

    // Relevant artifact MUST be Selected
    let art_seg = plan
        .segments
        .iter()
        .find(|s| s.id == "artifact:art-1")
        .expect("sales.xlsx artifact segment should exist");
    assert_eq!(art_seg.selection, WorkContextSelection::Selected);
    assert!(art_seg.content.is_some());

    // Relevant library item MUST be Selected
    let lib_seg = plan
        .segments
        .iter()
        .find(|s| s.id == "library:lib-1")
        .expect("library item segment should exist");
    assert_eq!(lib_seg.selection, WorkContextSelection::Selected);
    assert!(lib_seg.content.is_some());
}

// 7. secrets test
#[test]
fn test_secrets_protection() {
    let plan = WorkContextPlan {
        version: 1,
        workspace_id: Some("ws-1".to_string()),
        task_id: Some("task-1".to_string()),
        run_id: "run-sec".to_string(),
        session_id: Some("session-sec".to_string()),
        runtime: RuntimeProviderKind::Pi,
        preset: Some(WorkPreset::Office),
        segments: vec![WorkContextSegment {
            id: "system:base_policy".to_string(),
            kind: WorkContextKind::BasePolicy,
            source: WorkContextSource::System,
            title: "Base Policy".to_string(),
            required: true,
            selection: WorkContextSelection::Selected,
            reason: None,
            render_priority: 100,
            budget_priority: 100,
            estimated_tokens: 50,
            ref_id: None,
            content: Some("Safe base policy content".to_string()),
            metadata: BTreeMap::new(),
        }],
        estimated_tokens: 50,
        created_at: "2026-08-15T00:00:00Z".to_string(),
    };

    let serialized = serde_json::to_string(&plan).unwrap();
    assert!(!serialized.contains("authorization:"));
    assert!(!serialized.contains("Bearer "));
    assert!(!serialized.contains("bridge_token"));
    assert!(!serialized.contains("api_key"));

    // Test secret detection assertion across various secret types and JSON structures
    assert!(assert_no_secrets_in_json(b"{\"key\": \"Bearer secret_token_123\"}").is_err());
    assert!(assert_no_secrets_in_json(b"{\"token\": \"sk-ant-api03-abcdefg\"}").is_err());
    assert!(assert_no_secrets_in_json(b"{\"key\": \"ghp_1234567890abcdef\"}").is_err());
    assert!(assert_no_secrets_in_json(b"{\"key\": \"xoxb-1234-5678-abcdef\"}").is_err());
    assert!(assert_no_secrets_in_json(b"{\"data\": \"-----BEGIN RSA PRIVATE KEY-----\"}").is_err());
    assert!(assert_no_secrets_in_json(b"{\"api_key\": \"my-secret-key\"}").is_err());
    assert!(assert_no_secrets_in_json(b"{\"password\": \"supersecret123\"}").is_err());
    assert!(assert_no_secrets_in_json(b"{\"access_token\": \"ya29.abcdef\"}").is_err());
    assert!(assert_no_secrets_in_json(b"{\"mcp\": [{\"token\": \"secret\"}]}").is_err());
    assert!(
        assert_no_secrets_in_json(b"{\"placeholder\": \"__AGENTCABIN_WORK_SECRET__\"}").is_ok()
    );
}

// 8. persistence round trip test
#[test]
fn test_persistence_round_trip() {
    let temp = TempDir::new().unwrap();
    let plan_path = temp.path().join("work-context-plan.json");

    let mut metadata = BTreeMap::new();
    metadata.insert("category".to_string(), "spreadsheet".to_string());
    let plan = WorkContextPlan {
        version: 1,
        workspace_id: Some("ws-persist".to_string()),
        task_id: Some("task-persist".to_string()),
        run_id: "run-persist-123".to_string(),
        session_id: Some("session-123".to_string()),
        runtime: RuntimeProviderKind::Claude,
        preset: Some(WorkPreset::Code),
        segments: vec![
            WorkContextSegment {
                id: "system:base_policy".to_string(),
                kind: WorkContextKind::BasePolicy,
                source: WorkContextSource::System,
                title: "Base Policy".to_string(),
                required: true,
                selection: WorkContextSelection::Selected,
                reason: Some("Essential safety boundary".to_string()),
                render_priority: 100,
                budget_priority: 100,
                estimated_tokens: 50,
                ref_id: None,
                content: Some("Base policy text".to_string()),
                metadata: BTreeMap::new(),
            },
            WorkContextSegment {
                id: "artifact:art-1".to_string(),
                kind: WorkContextKind::Artifact,
                source: WorkContextSource::ArtifactStore,
                title: "data.csv".to_string(),
                required: false,
                selection: WorkContextSelection::Selected,
                reason: Some("Data source".to_string()),
                render_priority: 40,
                budget_priority: 40,
                estimated_tokens: 20,
                ref_id: Some("art-1".to_string()),
                content: Some("Artifact data.csv".to_string()),
                metadata,
            },
        ],
        estimated_tokens: 70,
        created_at: "2026-08-15T00:00:00Z".to_string(),
    };

    save_to_path(&plan_path, &plan).unwrap();
    let loaded = load_from_path(&plan_path)
        .unwrap()
        .expect("plan should load");

    assert_eq!(loaded, plan);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&plan_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

// 9. runtime-neutral test
#[test]
fn test_runtime_neutrality() {
    let providers = [
        RuntimeProviderKind::Pi,
        RuntimeProviderKind::Claude,
        RuntimeProviderKind::Codex,
        RuntimeProviderKind::Grok,
    ];

    let mut plans = Vec::new();
    for provider in providers {
        let input = WorkContextAssemblyInput {
            app_mode: AppMode::Work,
            runtime: provider,
            run_id: format!("run-{:?}", provider),
            session_id: None,
            workspace_id: Some("ws-neutral".to_string()),
            task_id: None,
            preset: Some(WorkPreset::Office),
            execution_context: None,
            prompt: Some("整理会议纪要".to_string()),
            workspace: Some(fake_workspace("ws-neutral", "Neutral Workspace")),
            task_state: None,
            task: None,
            capabilities: fake_capabilities(provider),
            artifacts: Vec::new(),
            library_items: Vec::new(),
            custom_rules: None,
        };
        let plan = WorkContextAssembler::assemble(input);
        plans.push((provider, plan));
    }

    // All runtimes must produce the identical harness-level context semantic structure
    let base_segments_count = plans[0].1.segments.len();
    for (provider, plan) in &plans {
        assert_eq!(
            plan.segments.len(),
            base_segments_count,
            "Runtime {:?} produced differing segment count",
            provider
        );
        assert_eq!(plan.version, 1);
        // Base policy must be required and Selected across all runtimes
        let base_policy = plan
            .segments
            .iter()
            .find(|s| s.kind == WorkContextKind::BasePolicy)
            .unwrap();
        assert!(base_policy.required);
        assert_eq!(base_policy.selection, WorkContextSelection::Selected);
    }
}

// 10. runtime projection test
#[test]
fn test_runtime_projection_system_prompt() {
    let ws = fake_workspace("ws-proj", "Project Workspace");
    let mut caps = fake_capabilities(RuntimeProviderKind::Pi);
    caps.browser_enabled = false;
    caps.detected_prohibited_paths = vec![PathBuf::from("/etc/shadow")];

    let input = WorkContextAssemblyInput {
        app_mode: AppMode::Work,
        runtime: RuntimeProviderKind::Pi,
        run_id: "run-proj-1".to_string(),
        session_id: None,
        workspace_id: Some("ws-proj".to_string()),
        task_id: Some("task-proj".to_string()),
        preset: Some(WorkPreset::Office),
        execution_context: None,
        prompt: Some("请分析 sales.xlsx 表格".to_string()),
        workspace: Some(ws),
        task_state: Some(WorkTaskState {
            version: 1,
            revision: 1,
            goal: Some("生成财务月报".to_string()),
            goal_spec: None,
            plan: vec![StructuredTask {
                id: "step-1".to_string(),
                text: "读取并校验 sales.xlsx".to_string(),
                status: StructuredTaskStatus::InProgress,
            }],
            checkpoint: None,
            pending_approval: None,
            updated_at: "2026-08-15T00:00:00Z".to_string(),
        }),
        task: None,
        capabilities: caps,
        artifacts: Vec::new(),
        library_items: Vec::new(),
        custom_rules: Some("Strict privacy adherence".to_string()),
    };

    let plan = WorkContextAssembler::assemble(input);
    let prompt = plan.render_system_prompt();

    // Verify system prompt contains all materialized selected sections
    assert!(
        prompt.contains("Work Base Policy"),
        "Must contain base policy"
    );
    assert!(
        prompt.contains("Work Preset (office)"),
        "Must contain office preset"
    );
    assert!(
        prompt.contains("Workspace Boundary (Project Workspace)"),
        "Must contain workspace boundary"
    );
    assert!(
        prompt.contains("Workspace Rules (Project Workspace)"),
        "Must contain workspace rules"
    );
    assert!(
        prompt.contains("Active Goal: 生成财务月报"),
        "Must contain active goal"
    );
    assert!(
        prompt.contains("Active Step [step-1]"),
        "Must contain active step"
    );
    let excel_skill = plan
        .segments
        .iter()
        .find(|segment| segment.id == "capability:skill:excel-spreadsheet")
        .expect("selected excel skill should remain observable in the Plan");
    assert_eq!(excel_skill.selection, WorkContextSelection::Selected);
    assert!(
        !prompt.contains("Skill `excel-spreadsheet`"),
        "selected capability observations must not be rendered as synthetic context"
    );
    assert!(
        !prompt.contains("Available Session Capabilities (Deferred)"),
        "deferred capability observations must not be rendered"
    );
    assert!(!prompt.contains("gmail-manager"));
    // Verify rejected section
    assert!(
        prompt.contains("Policy Restrictions"),
        "Must contain policy restrictions section"
    );
    assert!(
        prompt.contains("Prohibited Path (shadow)"),
        "Must list prohibited shadow"
    );
    assert!(
        !prompt.contains("Unavailable Capabilities"),
        "unavailable capability observations must remain out of injected context"
    );
    assert!(!prompt.contains("Browser Control"));
}

#[test]
fn test_rendered_projection_excludes_user_prompt_and_counts_all_rendered_sections() {
    let user_prompt = "UNIQUE_USER_TURN_MARKER";
    let mut plan = WorkContextPlan {
        version: 1,
        workspace_id: None,
        task_id: None,
        run_id: "run-rendered-budget".to_string(),
        session_id: None,
        runtime: RuntimeProviderKind::Pi,
        preset: None,
        segments: vec![
            WorkContextSegment {
                id: "system:base_policy".to_string(),
                kind: WorkContextKind::BasePolicy,
                source: WorkContextSource::System,
                title: "Base Policy".to_string(),
                required: true,
                selection: WorkContextSelection::Selected,
                reason: None,
                render_priority: 100,
                budget_priority: 100,
                estimated_tokens: 1,
                ref_id: None,
                content: Some("Stable policy".to_string()),
                metadata: BTreeMap::new(),
            },
            WorkContextSegment {
                id: "conversation:user_prompt".to_string(),
                kind: WorkContextKind::Conversation,
                source: WorkContextSource::UserPrompt,
                title: "User Prompt".to_string(),
                required: true,
                selection: WorkContextSelection::Selected,
                reason: None,
                render_priority: 60,
                budget_priority: 60,
                estimated_tokens: 1,
                ref_id: None,
                content: Some(format!("User Prompt:\n{user_prompt}")),
                metadata: BTreeMap::new(),
            },
            WorkContextSegment {
                id: "capability:skill:deferred".to_string(),
                kind: WorkContextKind::Skill,
                source: WorkContextSource::CapabilityCenter,
                title: "Skill: deferred".to_string(),
                required: false,
                selection: WorkContextSelection::Deferred,
                reason: Some("Available on demand".to_string()),
                render_priority: 50,
                budget_priority: 50,
                estimated_tokens: 1,
                ref_id: Some("deferred".to_string()),
                content: None,
                metadata: BTreeMap::new(),
            },
        ],
        estimated_tokens: 0,
        created_at: "2026-08-15T00:00:00Z".to_string(),
    };

    plan.recalculate_tokens();
    let rendered = plan.render_system_prompt();

    assert!(!rendered.contains(user_prompt));
    assert!(!rendered.contains("Available Session Capabilities (Deferred)"));
    assert!(!rendered.contains("Skill: deferred"));
    assert_eq!(plan.estimated_tokens, estimate_tokens(&rendered));
    assert!(plan.estimated_tokens > estimate_tokens("Stable policy"));
}

#[test]
fn test_capability_observations_do_not_bloat_rendered_context() {
    let mut plan = WorkContextPlan {
        version: 1,
        workspace_id: None,
        task_id: None,
        run_id: "run-capability-observations".to_string(),
        session_id: None,
        runtime: RuntimeProviderKind::Pi,
        preset: None,
        segments: vec![WorkContextSegment {
            id: "system:base_policy".to_string(),
            kind: WorkContextKind::BasePolicy,
            source: WorkContextSource::System,
            title: "Base Policy".to_string(),
            required: true,
            selection: WorkContextSelection::Selected,
            reason: None,
            render_priority: 100,
            budget_priority: 100,
            estimated_tokens: 1,
            ref_id: None,
            content: Some("Stable policy".to_string()),
            metadata: BTreeMap::new(),
        }],
        estimated_tokens: 0,
        created_at: "2026-08-15T00:00:00Z".to_string(),
    };

    for index in 0..110 {
        plan.segments.push(WorkContextSegment {
            id: format!("capability:skill:skill-{index}"),
            kind: WorkContextKind::Skill,
            source: WorkContextSource::CapabilityCenter,
            title: format!("Skill: skill-{index}"),
            required: false,
            selection: if index == 0 {
                WorkContextSelection::Selected
            } else {
                WorkContextSelection::Deferred
            },
            reason: Some(
                "Available to this session; not currently highlighted by the Harness for this turn"
                    .to_string(),
            ),
            render_priority: 50,
            budget_priority: 50,
            estimated_tokens: 100,
            ref_id: Some(format!("skill-{index}")),
            content: Some(format!("Synthetic capability description {index}")),
            metadata: BTreeMap::new(),
        });
    }

    let mut without_observations = plan.clone();
    without_observations
        .segments
        .retain(|segment| !segment.kind.is_capability());

    plan.recalculate_tokens();
    without_observations.recalculate_tokens();

    assert_eq!(
        plan.estimated_tokens, without_observations.estimated_tokens,
        "capability observations must not contribute to injected context cost"
    );
    let rendered = plan.render_system_prompt();
    assert!(!rendered.contains("Available Session Capabilities (Deferred)"));
    assert!(!rendered.contains("Synthetic capability description"));
    assert!(WorkContextKind::Skill.is_capability());
    assert!(WorkContextKind::Mcp.is_capability());
    assert!(WorkContextKind::Connector.is_capability());
    assert!(WorkContextKind::AgentPlugin.is_capability());
    assert!(WorkContextKind::WebAccess.is_capability());
    assert!(WorkContextKind::BrowserUse.is_capability());
    assert!(!WorkContextKind::Artifact.is_capability());
}

#[test]
fn test_work_context_envelope_and_capability_snapshot() {
    let ws = fake_workspace("ws-envelope", "Envelope Workspace");
    let caps = fake_capabilities(RuntimeProviderKind::Pi);

    let input = WorkContextAssemblyInput {
        app_mode: AppMode::Work,
        runtime: RuntimeProviderKind::Pi,
        run_id: "run-envelope-1".to_string(),
        session_id: None,
        workspace_id: Some("ws-envelope".to_string()),
        task_id: Some("task-env".to_string()),
        preset: Some(WorkPreset::Code),
        execution_context: None,
        prompt: Some("重构工作子系统".to_string()),
        workspace: Some(ws),
        task_state: Some(WorkTaskState {
            version: 1,
            revision: 1,
            goal: Some("单权威收敛".to_string()),
            goal_spec: None,
            plan: vec![StructuredTask {
                id: "step-1".to_string(),
                text: "去除多重解释".to_string(),
                status: StructuredTaskStatus::InProgress,
            }],
            checkpoint: None,
            pending_approval: None,
            updated_at: "2026-09-05T00:00:00Z".to_string(),
        }),
        task: None,
        capabilities: caps.clone(),
        artifacts: Vec::new(),
        library_items: Vec::new(),
        custom_rules: Some("Maintain strict typing".to_string()),
    };

    let plan = WorkContextAssembler::assemble(input);
    let envelope = plan.envelope();
    assert!(envelope.base_policy.is_some());
    assert!(envelope
        .preset_guidance
        .as_deref()
        .unwrap_or("")
        .contains("Code preset"));
    assert!(envelope.workspace_boundary.is_some());
    assert!(envelope
        .goal_and_plan
        .as_deref()
        .unwrap_or("")
        .contains("单权威收敛"));
    assert!(envelope
        .workspace_rules
        .as_deref()
        .unwrap_or("")
        .contains("Maintain strict typing"));
    assert_eq!(envelope.rendered_prompt, plan.render_system_prompt());

    let snapshot = plan.capability_snapshot();
    assert!(!snapshot.skills.is_empty());
    assert!(!snapshot.mcp_servers.is_empty());

    let direct_snapshot = CapabilitySnapshot::from_capabilities(&caps);
    assert_eq!(direct_snapshot.skills.len(), caps.enabled_skills.len());
    assert_eq!(direct_snapshot.mcp_servers.len(), caps.mcp_servers.len());
}
