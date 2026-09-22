//! Goal / Acceptance Contract Verifier & Repair Loop
//!
//! Replaces "Agent claims it completed" with "System proves it completed".
//! Evaluates Acceptance Criteria using machine-first verifiers, associates durable
//! evidence, and manages the bounded (max 3 rounds) auto-repair loop.

use chrono::Utc;
use uuid::Uuid;

use crate::work::artifacts;
use crate::work::models::{
    AcceptanceCriterion, CriterionStatus, GoalSpec, GoalStatus, RuntimeFact, VerifierType,
    WorkArtifactStatus, WorkArtifactSummary, WorkTask,
};
use crate::work::paths::WorkPaths;

pub const DEFAULT_MAX_REPAIR_ROUNDS: u32 = 3;

/// GoalBuilder: constructs Acceptance Criteria only from explicit deliverables.
///
/// Ordinary instructions may mention reports, spreadsheets, research, or
/// presentations without asking Work to produce a file. Keyword inference
/// therefore creates false acceptance obligations and is intentionally not
/// part of the Work contract.
pub struct GoalBuilder;

impl GoalBuilder {
    pub fn build(
        workspace_id: &str,
        run_id: &str,
        statement: &str,
        task: Option<&WorkTask>,
    ) -> GoalSpec {
        let now = Utc::now().to_rfc3339();
        let goal_id = format!("goal-{}", Uuid::new_v4());
        let statement = statement.trim();
        let statement = if statement.is_empty() {
            task.map(|t| t.title.as_str()).unwrap_or("执行工作任务")
        } else {
            statement
        };

        let mut criteria = Vec::new();

        // 1. Artifact criteria from required artifacts
        if let Some(task) = task {
            let mut added_paths = std::collections::HashSet::new();
            for req in &task.artifact_requirements {
                if req.required
                    && !req.path.trim().is_empty()
                    && added_paths.insert(req.path.clone())
                {
                    criteria.push(AcceptanceCriterion {
                        id: format!("crit-art-{}", criteria.len() + 1),
                        description: format!(
                            "生成交付物 {} 并通过格式与完整性验证",
                            req.title.as_deref().unwrap_or(&req.path)
                        ),
                        verifier_type: VerifierType::Artifact,
                        target_ref: Some(req.path.clone()),
                        status: CriterionStatus::Pending,
                        evidence_refs: Vec::new(),
                        failure_reason: None,
                    });
                }
            }
            for path in &task.required_artifacts {
                if !path.trim().is_empty() && added_paths.insert(path.clone()) {
                    criteria.push(AcceptanceCriterion {
                        id: format!("crit-art-{}", criteria.len() + 1),
                        description: format!("输出文件 {path} 已交付且校验有效"),
                        verifier_type: VerifierType::Artifact,
                        target_ref: Some(path.clone()),
                        status: CriterionStatus::Pending,
                        evidence_refs: Vec::new(),
                        failure_reason: None,
                    });
                }
            }
        }

        // An explicit deliverable contract still needs a small amount of
        // execution evidence. These criteria are system-level; they do not
        // claim semantic correctness that the machine verifier cannot prove.
        if !criteria.is_empty() && criteria.len() < 3 {
            criteria.push(AcceptanceCriterion {
                id: "crit-execution-1".to_string(),
                description: "WorkRun 至少包含一个成功完成的执行步骤证据".to_string(),
                verifier_type: VerifierType::Machine,
                target_ref: None,
                status: CriterionStatus::Pending,
                evidence_refs: Vec::new(),
                failure_reason: None,
            });
        }
        if !criteria.is_empty() && criteria.len() < 3 {
            criteria.push(AcceptanceCriterion {
                id: "crit-evidence-1".to_string(),
                description: "运行结果具备可追溯的工具执行证据".to_string(),
                verifier_type: VerifierType::Structured,
                target_ref: None,
                status: CriterionStatus::Pending,
                evidence_refs: Vec::new(),
                failure_reason: None,
            });
        }

        // Cap at 6 criteria to keep verification focused and robust.
        criteria.truncate(6);
        let status = if criteria.is_empty() {
            GoalStatus::NotApplicable
        } else {
            GoalStatus::Pending
        };

        GoalSpec {
            goal_id,
            workspace_id: workspace_id.to_string(),
            run_id: run_id.to_string(),
            statement: statement.to_string(),
            criteria,
            status,
            repair_round: 0,
            max_repair_rounds: DEFAULT_MAX_REPAIR_ROUNDS,
            repair_instruction: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

/// GoalVerifier: executes machine-first verifications on a GoalSpec.
pub struct GoalVerifier<'a> {
    paths: &'a WorkPaths,
    workspace_id: &'a str,
    run_id: &'a str,
    artifacts: Vec<WorkArtifactSummary>,
    facts: Vec<RuntimeFact>,
}

impl<'a> GoalVerifier<'a> {
    pub fn new(
        paths: &'a WorkPaths,
        workspace_id: &'a str,
        task_id: &str,
        run_id: &'a str,
    ) -> Self {
        let artifacts = if workspace_id.trim().is_empty() {
            artifacts::list_standalone_with_paths(paths, run_id).unwrap_or_default()
        } else {
            artifacts::list_with_paths(paths, workspace_id, Some(run_id)).unwrap_or_default()
        };

        let facts = {
            if let Ok(ledger) = crate::work::ledger::WorkRuntimeLedger::open(paths, task_id, run_id)
            {
                ledger.list_facts().unwrap_or_default()
            } else {
                Vec::new()
            }
        };

        Self {
            paths,
            workspace_id,
            run_id,
            artifacts,
            facts,
        }
    }

    /// Verifies the given GoalSpec against durable system state.
    pub fn verify(&self, mut goal: GoalSpec) -> GoalSpec {
        let now = Utc::now().to_rfc3339();
        let mut all_passed = true;
        let mut has_failed = false;
        let mut has_insufficient = false;

        for criterion in &mut goal.criteria {
            criterion.status = CriterionStatus::Checking;
            criterion.evidence_refs.clear();
            criterion.failure_reason = None;

            match criterion.verifier_type {
                VerifierType::Artifact => {
                    self.verify_artifact_criterion(criterion);
                }
                VerifierType::File => {
                    self.verify_file_criterion(criterion);
                }
                VerifierType::Structured | VerifierType::Machine => {
                    self.verify_machine_criterion(criterion);
                }
                VerifierType::Composite | VerifierType::Llm => {
                    self.verify_composite_criterion(criterion);
                }
            }

            match criterion.status {
                CriterionStatus::Passed => {}
                CriterionStatus::Failed => {
                    all_passed = false;
                    has_failed = true;
                }
                CriterionStatus::InsufficientEvidence
                | CriterionStatus::Checking
                | CriterionStatus::Pending => {
                    all_passed = false;
                    has_insufficient = true;
                }
            }
        }

        goal.status = if goal.criteria.is_empty() {
            // Nothing was machine-provable, so claiming a pass here would
            // fabricate acceptance the system never performed.
            GoalStatus::NotApplicable
        } else if all_passed {
            GoalStatus::Passed
        } else if has_failed {
            GoalStatus::Failed
        } else if has_insufficient {
            GoalStatus::InsufficientEvidence
        } else {
            GoalStatus::Pending
        };

        goal.updated_at = now;
        goal
    }

    fn verify_artifact_criterion(&self, criterion: &mut AcceptanceCriterion) {
        if let Some(target_path) = criterion.target_ref.as_deref() {
            let normalized = target_path.trim().trim_start_matches("./");
            let found = self.artifacts.iter().find(|art| {
                art.path == normalized
                    || art.path.ends_with(normalized)
                    || normalized.ends_with(&art.path)
            });

            if let Some(art) = found {
                if art.status == WorkArtifactStatus::Delivered
                    || art.status == WorkArtifactStatus::Validated
                {
                    criterion.status = CriterionStatus::Passed;
                    criterion.evidence_refs.push(format!("artifact:{}", art.id));
                    if let Some(sha256) = art.sha256.as_deref() {
                        criterion.evidence_refs.push(format!("sha256:{sha256}"));
                    }
                } else if art.status == WorkArtifactStatus::Invalid {
                    criterion.status = CriterionStatus::Failed;
                    criterion.failure_reason = Some("交付物格式损坏或未通过合法性检查".to_string());
                } else {
                    criterion.status = CriterionStatus::InsufficientEvidence;
                    criterion.failure_reason =
                        Some("交付物正在生成中或尚未完成验收交付".to_string());
                }
            } else {
                criterion.status = CriterionStatus::Failed;
                criterion.failure_reason = Some(format!("未在输出目录找到目标交付物 {normalized}"));
            }
        } else {
            // General artifact check: at least one validated artifact delivered
            let delivered = self
                .artifacts
                .iter()
                .filter(|a| {
                    a.status == WorkArtifactStatus::Delivered
                        || a.status == WorkArtifactStatus::Validated
                })
                .collect::<Vec<_>>();

            if !delivered.is_empty() {
                criterion.status = CriterionStatus::Passed;
                for a in delivered {
                    criterion.evidence_refs.push(format!("artifact:{}", a.id));
                }
            } else if !self.artifacts.is_empty() {
                criterion.status = CriterionStatus::InsufficientEvidence;
                criterion.failure_reason =
                    Some("输出文件已创建但尚未完成格式验证或交付".to_string());
            } else {
                criterion.status = CriterionStatus::Failed;
                criterion.failure_reason = Some("尚未生成任何交付物文件".to_string());
            }
        }
    }

    fn verify_file_criterion(&self, criterion: &mut AcceptanceCriterion) {
        if let Some(target) = criterion.target_ref.as_deref() {
            let relative = target.trim().trim_start_matches("./");
            let exists = if !self.workspace_id.trim().is_empty() {
                self.paths
                    .resolve_workspace_path(
                        self.workspace_id,
                        std::path::Path::new(relative),
                        false,
                    )
                    .map(|p| {
                        p.is_file() && std::fs::metadata(&p).map(|m| m.len() > 0).unwrap_or(false)
                    })
                    .unwrap_or(false)
            } else if let Ok(dir) = self.paths.standalone_output_dir(self.run_id) {
                let candidate = dir.join(relative.trim_start_matches("output/"));
                candidate.is_file()
                    && std::fs::metadata(&candidate)
                        .map(|m| m.len() > 0)
                        .unwrap_or(false)
            } else {
                false
            };

            if exists {
                criterion.status = CriterionStatus::Passed;
                criterion.evidence_refs.push(format!("file:{relative}"));
            } else {
                criterion.status = CriterionStatus::Failed;
                criterion.failure_reason = Some(format!("文件 {relative} 不存在或内容为空"));
            }
        } else {
            criterion.status = CriterionStatus::InsufficientEvidence;
            criterion.failure_reason = Some("文件验收标准缺少明确目标路径".to_string());
        }
    }

    fn verify_machine_criterion(&self, criterion: &mut AcceptanceCriterion) {
        // Machine checks must be backed by an authoritative successful
        // ToolResult. Merely having an artifact record is not execution proof.
        let successful_tools = self
            .facts
            .iter()
            .filter(|f| {
                matches!(
                    f,
                    RuntimeFact::ToolResult {
                        success: true,
                        status,
                        ..
                    } if status != "interrupted" && status != "denied"
                )
            })
            .count();
        let failed_tools = self
            .facts
            .iter()
            .filter(|f| matches!(f, RuntimeFact::ToolResult { success: false, .. }))
            .count();
        let delivered_artifacts = self
            .artifacts
            .iter()
            .filter(|a| {
                a.status == WorkArtifactStatus::Delivered
                    || a.status == WorkArtifactStatus::Validated
            })
            .count();

        if successful_tools > 0 {
            criterion.status = CriterionStatus::Passed;
            criterion
                .evidence_refs
                .push(format!("tool_successes:{successful_tools}"));
            if delivered_artifacts > 0 {
                criterion
                    .evidence_refs
                    .push(format!("artifacts_count:{delivered_artifacts}"));
            }
        } else if failed_tools > 0 {
            criterion.status = CriterionStatus::Failed;
            criterion.failure_reason = Some("执行步骤均未产生成功的工具结果".to_string());
        } else if !self.artifacts.is_empty() {
            criterion.status = CriterionStatus::InsufficientEvidence;
            criterion.failure_reason = Some("存在成果记录，但缺少成功的工具执行证据".to_string());
        } else {
            criterion.status = CriterionStatus::InsufficientEvidence;
            criterion.failure_reason = Some("未检测到可验证的工具执行结果".to_string());
        }
    }

    fn verify_composite_criterion(&self, criterion: &mut AcceptanceCriterion) {
        // A proposal is intent, not evidence. Join each proposed tool call to
        // its own successful ToolResult so search-only, failed, or abandoned
        // calls cannot make a source criterion pass.
        let proposed_tools = self
            .facts
            .iter()
            .filter_map(|fact| match fact {
                RuntimeFact::ToolProposed {
                    tool_call_id,
                    tool_name,
                    ..
                } => Some((tool_call_id.as_str(), tool_name.as_str())),
                _ => None,
            })
            .collect::<std::collections::HashMap<_, _>>();
        let source_tools = [
            "web_open",
            "web_fetch",
            "browser_navigate",
            "browser_snapshot",
            "browser_take_screenshot",
            "library_read",
            "work_mcp_call",
            "work_call_app",
            "work_run_connector_cli",
        ];
        let successful_sources = self
            .facts
            .iter()
            .filter_map(|fact| match fact {
                RuntimeFact::ToolResult {
                    tool_call_id,
                    success: true,
                    ..
                } => proposed_tools
                    .get(tool_call_id.as_str())
                    .filter(|tool_name| source_tools.iter().any(|name| name == *tool_name))
                    .map(|tool_name| (tool_call_id, *tool_name)),
                _ => None,
            })
            .collect::<Vec<_>>();

        if !successful_sources.is_empty() {
            criterion.status = CriterionStatus::Passed;
            for (tool_call_id, tool_name) in successful_sources {
                criterion
                    .evidence_refs
                    .push(format!("ledger:source_access:{tool_name}:{tool_call_id}"));
            }
        } else {
            criterion.status = CriterionStatus::InsufficientEvidence;
            criterion.failure_reason = Some("缺少必要的数据调研或来源证据记录".to_string());
        }
    }
}

/// GoalRepairLoop: generates targeted repair instructions and handles max repair rounds.
pub struct GoalRepairLoop;

impl GoalRepairLoop {
    /// Generates structured repair instruction text for unpassed criteria.
    pub fn build_repair_instruction(goal: &GoalSpec) -> Option<String> {
        let unpassed = goal
            .criteria
            .iter()
            .filter(|c| c.status != CriterionStatus::Passed)
            .collect::<Vec<_>>();

        if unpassed.is_empty() {
            return None;
        }

        let mut lines = Vec::new();
        let round = goal.repair_round.max(1);
        lines.push(format!(
            "【目标自动修复 - 第 {}/{} 轮】任务目标尚未完全达成：",
            round, goal.max_repair_rounds
        ));

        for (index, crit) in unpassed.iter().enumerate() {
            let reason = crit
                .failure_reason
                .as_deref()
                .unwrap_or("未满足验收标准要求");
            lines.push(format!(
                "{}. {} (未通过原因: {})",
                index + 1,
                crit.description,
                reason
            ));
        }

        lines.push(
            "请针对上述未通过项进行补充执行、修复文件或补充来源，完成后系统将重新进行目标验收。"
                .to_string(),
        );
        Some(lines.join("\n"))
    }

    /// Evaluates if repair should continue or escalate to Human Inbox.
    pub fn advance_repair(mut goal: GoalSpec) -> (GoalSpec, bool) {
        if matches!(goal.status, GoalStatus::Passed | GoalStatus::NotApplicable) {
            goal.repair_instruction = None;
            return (goal, false);
        }

        if goal.repair_round < goal.max_repair_rounds {
            goal.repair_round += 1;
            goal.repair_instruction = Self::build_repair_instruction(&goal);
            goal.updated_at = Utc::now().to_rfc3339();
            (goal, true) // Can continue repair
        } else {
            goal.status = GoalStatus::Failed;
            goal.repair_instruction = Some(format!(
                "已达到最大自动修复轮次 ({} 轮)，仍有部分验收标准未达成，已转入待我处理。",
                goal.max_repair_rounds
            ));
            goal.updated_at = Utc::now().to_rfc3339();
            (goal, false) // Exceeded, escalate
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::models::WorkArtifactRequirement;
    use tempfile::tempdir;

    #[test]
    fn test_goal_builder_creates_criteria_from_task() {
        let task = WorkTask {
            id: "task-1".to_string(),
            workspace_id: "ws-1".to_string(),
            title: "分析 Q2 销售下滑并输出管理层汇报".to_string(),
            instructions: "生成 Excel 分析表与 PPT 汇报材料".to_string(),
            status: crate::work::models::WorkTaskStatus::Active,
            source: crate::work::models::WorkTaskSource::Manual,
            policy: crate::work::models::WorkPolicy::default(),
            schedule: None,
            required_artifacts: vec!["output/q2-analysis.xlsx".to_string()],
            artifact_requirements: vec![WorkArtifactRequirement {
                path: "output/management-review.pptx".to_string(),
                title: Some("管理层汇报 PPT".to_string()),
                artifact_type: Some("pptx".to_string()),
                required: true,
            }],
            run_count: 0,
            last_run_id: None,
            last_run_at: None,
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };

        let goal = GoalBuilder::build("ws-1", "run-1", "", Some(&task));
        assert_eq!(goal.statement, "分析 Q2 销售下滑并输出管理层汇报");
        assert!(goal.criteria.len() >= 2);
        assert_eq!(goal.status, GoalStatus::Pending);
        assert_eq!(goal.max_repair_rounds, 3);
        assert_eq!(goal.repair_round, 0);

        // Verify artifact criteria were included
        assert!(goal
            .criteria
            .iter()
            .any(|c| c.target_ref.as_deref() == Some("output/management-review.pptx")));
        assert!(goal
            .criteria
            .iter()
            .any(|c| c.target_ref.as_deref() == Some("output/q2-analysis.xlsx")));
    }

    #[test]
    fn test_goal_builder_ignores_keyword_only_statements() {
        let task = WorkTask {
            id: "task-plain".to_string(),
            workspace_id: "ws-plain".to_string(),
            title: "分析 Q2 销售趋势".to_string(),
            instructions: "直接在对话中总结结论，不生成文件".to_string(),
            status: crate::work::models::WorkTaskStatus::Active,
            source: crate::work::models::WorkTaskSource::Manual,
            policy: crate::work::models::WorkPolicy::default(),
            schedule: None,
            required_artifacts: Vec::new(),
            artifact_requirements: Vec::new(),
            run_count: 0,
            last_run_id: None,
            last_run_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let goal = GoalBuilder::build("ws-plain", "run-plain", "", Some(&task));
        assert!(goal.criteria.is_empty());
        assert_eq!(goal.status, GoalStatus::NotApplicable);
    }

    #[test]
    fn test_goal_repair_loop_bounds_and_instruction() {
        let goal = GoalSpec {
            goal_id: "g-1".to_string(),
            workspace_id: "ws-1".to_string(),
            run_id: "run-1".to_string(),
            statement: "测试目标".to_string(),
            criteria: vec![AcceptanceCriterion {
                id: "c-1".to_string(),
                description: "分析报告生成".to_string(),
                verifier_type: VerifierType::Artifact,
                target_ref: Some("output/report.pdf".to_string()),
                status: CriterionStatus::Failed,
                evidence_refs: Vec::new(),
                failure_reason: Some("未找到目标文件".to_string()),
            }],
            status: GoalStatus::Failed,
            repair_round: 0,
            max_repair_rounds: 3,
            repair_instruction: None,
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };

        // Round 1
        let (g1, can_continue1) = GoalRepairLoop::advance_repair(goal);
        assert!(can_continue1);
        assert_eq!(g1.repair_round, 1);
        assert!(g1
            .repair_instruction
            .as_ref()
            .unwrap()
            .contains("第 1/3 轮"));

        // Round 2
        let (g2, can_continue2) = GoalRepairLoop::advance_repair(g1);
        assert!(can_continue2);
        assert_eq!(g2.repair_round, 2);

        // Round 3
        let (g3, can_continue3) = GoalRepairLoop::advance_repair(g2);
        assert!(can_continue3);
        assert_eq!(g3.repair_round, 3);

        // Round 4 (exceeded)
        let (g4, can_continue4) = GoalRepairLoop::advance_repair(g3);
        assert!(!can_continue4);
        assert_eq!(g4.status, GoalStatus::Failed);
        assert!(g4
            .repair_instruction
            .as_ref()
            .unwrap()
            .contains("已达到最大自动修复轮次"));
    }

    #[test]
    fn test_goal_verifier_evaluates_delivered_artifact() {
        let temp = tempdir().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        let ws = crate::work::workspace::WorkspaceManager::new(paths.clone())
            .create("Test WS")
            .unwrap();

        // Create a valid artifact file
        let output_dir = paths.workspace_dir(&ws.id).unwrap().join("output");
        std::fs::create_dir_all(&output_dir).unwrap();
        let file_path = output_dir.join("report.md");
        std::fs::write(&file_path, "# Report\nContent OK").unwrap();

        artifacts::register_with_paths(
            &paths,
            &ws.id,
            "output/report.md",
            "Report",
            Some("md"),
            Some("run-test"),
        )
        .unwrap();

        let goal = GoalSpec {
            goal_id: "g-test".to_string(),
            workspace_id: ws.id.clone(),
            run_id: "run-test".to_string(),
            statement: "生成报告".to_string(),
            criteria: vec![AcceptanceCriterion {
                id: "c-1".to_string(),
                description: "报告文件生成".to_string(),
                verifier_type: VerifierType::Artifact,
                target_ref: Some("output/report.md".to_string()),
                status: CriterionStatus::Pending,
                evidence_refs: Vec::new(),
                failure_reason: None,
            }],
            status: GoalStatus::Pending,
            repair_round: 0,
            max_repair_rounds: 3,
            repair_instruction: None,
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };

        let verifier = GoalVerifier::new(&paths, &ws.id, "task-test", "run-test");
        let verified = verifier.verify(goal);

        assert_eq!(verified.status, GoalStatus::Passed);
        assert_eq!(verified.criteria[0].status, CriterionStatus::Passed);
        assert!(!verified.criteria[0].evidence_refs.is_empty());
    }

    #[test]
    fn test_goal_verifier_reads_ledger_facts_by_task_id() {
        let temp = tempdir().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        let ws = crate::work::workspace::WorkspaceManager::new(paths.clone())
            .create("Ledger WS")
            .unwrap();
        let task_id = "task-ledger-facts";
        let run_id = "run-ledger-facts";

        let ledger = crate::work::ledger::WorkRuntimeLedger::open(&paths, task_id, run_id).unwrap();
        ledger
            .record(&RuntimeFact::ToolProposed {
                tool_call_id: "tc-1".to_string(),
                tool_name: "web_open".to_string(),
                action: "open".to_string(),
                arguments_hash: "hash".to_string(),
                expected_outputs: Vec::new(),
                side_effect_class: crate::work::models::SideEffectClass::Read,
                concurrency_class: crate::work::models::ToolConcurrencyClass::ParallelSafe,
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolResult {
                tool_call_id: "tc-1".to_string(),
                success: true,
                status: "ok".to_string(),
                failure_kind: None,
                exit_code: Some(0),
                error: None,
                outputs: vec!["https://example.com/a".to_string()],
                side_effect_class: crate::work::models::SideEffectClass::Read,
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();

        let goal = GoalSpec {
            goal_id: "g-ledger".to_string(),
            workspace_id: ws.id.clone(),
            run_id: run_id.to_string(),
            statement: "调研并核对来源".to_string(),
            criteria: vec![AcceptanceCriterion {
                id: "c-source".to_string(),
                description: "关键结论具备真实访问依据".to_string(),
                verifier_type: VerifierType::Composite,
                target_ref: None,
                status: CriterionStatus::Pending,
                evidence_refs: Vec::new(),
                failure_reason: None,
            }],
            status: GoalStatus::Pending,
            repair_round: 0,
            max_repair_rounds: 3,
            repair_instruction: None,
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };

        // With the real task_id the verifier must see the recorded source-access facts.
        let verified = GoalVerifier::new(&paths, &ws.id, task_id, run_id).verify(goal.clone());
        assert_eq!(verified.status, GoalStatus::Passed);
        assert!(verified.criteria[0]
            .evidence_refs
            .iter()
            .any(|evidence| evidence.starts_with("ledger:source_access:web_open:")));

        // A wrong task id loads no facts, proving facts are keyed by the real task.
        let unverified = GoalVerifier::new(&paths, &ws.id, "some-other-task", run_id).verify(goal);
        assert_eq!(
            unverified.status,
            GoalStatus::InsufficientEvidence,
            "verifier must not fabricate source evidence from a ledger it cannot read"
        );
    }

    #[test]
    fn test_verify_without_criteria_is_not_applicable_not_passed() {
        let temp = tempdir().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());

        let goal = GoalSpec {
            goal_id: "g-empty".to_string(),
            workspace_id: String::new(),
            run_id: "run-empty".to_string(),
            statement: "口头答复即可的任务".to_string(),
            criteria: Vec::new(),
            status: GoalStatus::Pending,
            repair_round: 0,
            max_repair_rounds: 3,
            repair_instruction: None,
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };

        let verifier = GoalVerifier::new(&paths, "", "task-empty", "run-empty");
        let verified = verifier.verify(goal);
        assert_eq!(
            verified.status,
            GoalStatus::NotApplicable,
            "a goal with zero acceptance criteria proves nothing and must not report Passed"
        );
    }

    #[test]
    fn test_not_applicable_goal_never_enters_repair_loop() {
        let goal = GoalSpec {
            goal_id: "g-na".to_string(),
            workspace_id: "ws-1".to_string(),
            run_id: "run-1".to_string(),
            statement: "无交付物要求的任务".to_string(),
            criteria: Vec::new(),
            status: GoalStatus::NotApplicable,
            repair_round: 0,
            max_repair_rounds: 3,
            repair_instruction: None,
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };

        let (same, can_continue) = GoalRepairLoop::advance_repair(goal);
        assert!(
            !can_continue,
            "not-applicable goals must not trigger repair"
        );
        assert_eq!(same.status, GoalStatus::NotApplicable);
        assert_eq!(same.repair_round, 0);
        assert!(same.repair_instruction.is_none());
    }

    #[test]
    fn search_only_does_not_prove_source_access() {
        let temp = tempdir().unwrap();
        let paths = WorkPaths::new(temp.path().to_path_buf());
        let task_id = "task-search-only";
        let run_id = "run-search-only";
        let ledger = crate::work::ledger::WorkRuntimeLedger::open(&paths, task_id, run_id).unwrap();
        ledger
            .record(&RuntimeFact::ToolProposed {
                tool_call_id: "search-call".to_string(),
                tool_name: "web_search".to_string(),
                action: "search".to_string(),
                arguments_hash: "hash".to_string(),
                expected_outputs: Vec::new(),
                side_effect_class: crate::work::models::SideEffectClass::Read,
                concurrency_class: crate::work::models::ToolConcurrencyClass::ParallelSafe,
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();
        ledger
            .record(&RuntimeFact::ToolResult {
                tool_call_id: "search-call".to_string(),
                success: true,
                status: "ok".to_string(),
                failure_kind: None,
                exit_code: Some(0),
                error: None,
                outputs: vec!["https://example.com/search-result".to_string()],
                side_effect_class: crate::work::models::SideEffectClass::Read,
                timestamp: Utc::now().to_rfc3339(),
            })
            .unwrap();

        let goal = GoalSpec {
            goal_id: "search-only-goal".to_string(),
            workspace_id: String::new(),
            run_id: run_id.to_string(),
            statement: "调研来源".to_string(),
            criteria: vec![AcceptanceCriterion {
                id: "source".to_string(),
                description: "真实访问来源".to_string(),
                verifier_type: VerifierType::Composite,
                target_ref: None,
                status: CriterionStatus::Pending,
                evidence_refs: Vec::new(),
                failure_reason: None,
            }],
            status: GoalStatus::Pending,
            repair_round: 0,
            max_repair_rounds: 3,
            repair_instruction: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let verified = GoalVerifier::new(&paths, "", task_id, run_id).verify(goal);
        assert_eq!(verified.status, GoalStatus::InsufficientEvidence);
        assert_eq!(
            verified.criteria[0].status,
            CriterionStatus::InsufficientEvidence
        );
    }
}
