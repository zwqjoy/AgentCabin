use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::work::models::{InboxItemStatus, RuntimeFact, WorkRunStatus};
use crate::work::paths::WorkPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactReport {
    pub path: String,
    pub sha256: Option<String>,
    pub actual_sha256: Option<String>,
    pub valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticAssertion {
    pub name: String,
    pub expected: Value,
    pub actual: Value,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerStats {
    pub tool_proposed: usize,
    pub tool_started: usize,
    pub tool_result: usize,
    pub tool_failures: usize,
    pub duplicate_tool_started: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalStats {
    pub requested: usize,
    pub approved: usize,
    pub unexpected: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunVerificationStats {
    pub workspace_id: String,
    pub task_id: String,
    pub run_id: String,
    pub session_id: Option<String>,
    pub final_status: String,
    pub projection_status: String,
}

pub fn compute_file_sha256(path: &Path) -> Result<String, std::io::Error> {
    use sha2::{Digest, Sha256};
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn verify_effort_gate(
    requested: &str,
    effective: Option<&str>,
) -> Result<bool, (String, String)> {
    match effective {
        Some(eff) if eff.eq_ignore_ascii_case(requested) => Ok(true),
        Some(eff) => Err((
            "effort_mismatch".to_string(),
            format!(
                "Effective launch effort '{eff}' does not match requested effort '{requested}'"
            ),
        )),
        None => Err((
            "effort_mismatch".to_string(),
            format!("Effective launch effort was not captured, expected '{requested}'"),
        )),
    }
}

pub fn count_duplicate_tool_started(facts: &[RuntimeFact]) -> usize {
    let mut started_counts: HashMap<String, usize> = HashMap::new();
    for fact in facts {
        if let RuntimeFact::ToolStarted { tool_call_id, .. } = fact {
            *started_counts.entry(tool_call_id.clone()).or_default() += 1;
        }
    }
    started_counts
        .values()
        .filter(|&&count| count > 1)
        .map(|count| count - 1)
        .sum()
}

pub fn analyze_ledger_facts(facts: &[RuntimeFact]) -> LedgerStats {
    let mut tool_proposed = 0;
    let mut tool_started = 0;
    let mut tool_result = 0;
    let mut tool_failures = 0;

    for fact in facts {
        match fact {
            RuntimeFact::ToolProposed { .. } => tool_proposed += 1,
            RuntimeFact::ToolStarted { .. } => tool_started += 1,
            RuntimeFact::ToolResult {
                success, status, ..
            } => {
                tool_result += 1;
                if !success && status != "waiting_approval" && status != "waiting_input" {
                    tool_failures += 1;
                }
            }
            _ => {}
        }
    }

    let duplicate_tool_started = count_duplicate_tool_started(facts);

    LedgerStats {
        tool_proposed,
        tool_started,
        tool_result,
        tool_failures,
        duplicate_tool_started,
    }
}

pub struct VerifierInput<'a> {
    pub paths: &'a WorkPaths,
    pub workspace_id: &'a str,
    pub task_id: &'a str,
    pub run_id: &'a str,
    pub requested_model: &'a str,
    pub fixture_contracts_dir: &'a Path,
    pub approvals_requested: usize,
    pub approvals_approved: usize,
    pub unexpected_approvals: usize,
}

pub struct VerificationResult {
    pub run_stats: RunVerificationStats,
    pub approval_stats: ApprovalStats,
    pub ledger_stats: LedgerStats,
    pub artifacts: Vec<ArtifactReport>,
    pub semantic_assertions: Vec<SemanticAssertion>,
}

impl VerificationResult {
    pub fn is_all_passed(&self) -> bool {
        self.first_failure_kind_and_reason().is_none()
    }

    pub fn first_failure_kind_and_reason(&self) -> Option<(&'static str, String)> {
        if self.approval_stats.unexpected > 0 {
            return Some((
                "unexpected_approval",
                format!(
                    "Received {} unexpected approval(s)",
                    self.approval_stats.unexpected
                ),
            ));
        }
        if self.run_stats.final_status != "completed" {
            return Some((
                "runtime_start_failed",
                format!(
                    "Run did not complete; final status: {}",
                    self.run_stats.final_status
                ),
            ));
        }
        if self.run_stats.projection_status != "completed" {
            return Some((
                "projection_not_completed",
                format!(
                    "Projection status is '{}', expected 'completed'",
                    self.run_stats.projection_status
                ),
            ));
        }
        if self.approval_stats.requested != 1 {
            return Some((
                "approval_count_mismatch",
                format!(
                    "Expected exactly 1 requested approval, got {}",
                    self.approval_stats.requested
                ),
            ));
        }
        if self.approval_stats.approved != 1 {
            return Some((
                "approval_count_mismatch",
                format!(
                    "Expected exactly 1 approved approval, got {}",
                    self.approval_stats.approved
                ),
            ));
        }
        if self.ledger_stats.tool_proposed == 0 {
            return Some((
                "ledger_empty",
                "Ledger recorded 0 ToolProposed facts".to_string(),
            ));
        }
        if self.ledger_stats.tool_started == 0 {
            return Some((
                "ledger_empty",
                "Ledger recorded 0 ToolStarted facts".to_string(),
            ));
        }
        if self.ledger_stats.tool_result == 0 {
            return Some((
                "ledger_empty",
                "Ledger recorded 0 ToolResult facts".to_string(),
            ));
        }
        if self.ledger_stats.tool_failures > 0 {
            return Some((
                "tool_failure",
                format!(
                    "Ledger recorded {} failed tool execution(s)",
                    self.ledger_stats.tool_failures
                ),
            ));
        }
        if self.ledger_stats.duplicate_tool_started > 0 {
            return Some((
                "duplicate_execution",
                format!(
                    "Ledger recorded {} duplicate tool started execution(s)",
                    self.ledger_stats.duplicate_tool_started
                ),
            ));
        }
        if let Some(failed_artifact) = self.artifacts.iter().find(|a| !a.valid) {
            let detail = match (&failed_artifact.sha256, &failed_artifact.actual_sha256) {
                (Some(reg), Some(act)) if reg != act => {
                    format!("registry sha256 '{reg}' does not match actual file sha256 '{act}'")
                }
                (None, _) => "missing registry sha256".to_string(),
                (_, None) => "actual file missing or unreadable".to_string(),
                _ => "invalid".to_string(),
            };
            return Some((
                "artifact_invalid",
                format!("Artifact '{}' is invalid: {detail}", failed_artifact.path),
            ));
        }
        if let Some(failed_assertion) = self.semantic_assertions.iter().find(|s| !s.passed) {
            return Some((
                "semantic_assertion_failed",
                format!(
                    "Semantic assertion '{}' failed: expected {}, got {}",
                    failed_assertion.name, failed_assertion.expected, failed_assertion.actual
                ),
            ));
        }
        None
    }
}

pub fn verify_core_contract_smoke(input: &VerifierInput) -> Result<VerificationResult, String> {
    let tm = crate::work::tasks::TaskManager::new(input.paths.clone());
    let run = tm.get_run(input.task_id, input.run_id)?;

    let meta = crate::storage::runs::get_run(input.run_id)
        .ok_or_else(|| format!("RunMeta not found for {}", input.run_id))?;

    // Layer A: Run
    if meta.agent != "pi" {
        return Err(format!("RunMeta agent is '{}', expected 'pi'", meta.agent));
    }
    if meta.model.as_deref() != Some(input.requested_model) {
        return Err(format!(
            "RunMeta model is {:?}, expected '{}'",
            meta.model, input.requested_model
        ));
    }

    let projection = crate::work::projection::project_work_projection(input.paths, input.run_id)
        .map_err(|e| format!("Failed to read projection: {e}"))?;

    let final_status_str = match run.status {
        WorkRunStatus::Completed => "completed".to_string(),
        WorkRunStatus::Failed => "failed".to_string(),
        WorkRunStatus::Running => "running".to_string(),
        WorkRunStatus::WaitingApproval => "waiting_approval".to_string(),
        WorkRunStatus::WaitingDelivery => "waiting_delivery".to_string(),
        WorkRunStatus::WaitingInput => "waiting_input".to_string(),
        WorkRunStatus::Cancelled => "cancelled".to_string(),
        WorkRunStatus::Recoverable => "recoverable".to_string(),
        WorkRunStatus::Queued => "queued".to_string(),
        WorkRunStatus::Skipped => "skipped".to_string(),
    };

    let proj_status_str = format!("{:?}", projection.status).to_ascii_lowercase();

    // Check inbox and interactions are 0 pending
    let inbox_mgr = crate::work::inbox::InboxManager::new(input.paths.clone());
    let pending_inbox = inbox_mgr
        .list_items(false, Some(input.task_id))?
        .into_iter()
        .filter(|item| item.run_id == input.run_id && item.status == InboxItemStatus::Pending)
        .count();
    if pending_inbox > 0 {
        return Err(format!("Pending inbox items remain: {pending_inbox}"));
    }

    let interaction_mgr = crate::work::interaction::InteractionManager::new(input.paths.clone());
    let pending_interactions = interaction_mgr
        .list_pending(Some(input.task_id), Some(input.run_id))?
        .len();
    if pending_interactions > 0 {
        return Err(format!(
            "Pending interactions remain: {pending_interactions}"
        ));
    }

    let run_stats = RunVerificationStats {
        workspace_id: input.workspace_id.to_string(),
        task_id: input.task_id.to_string(),
        run_id: input.run_id.to_string(),
        session_id: run.session_id.clone(),
        final_status: final_status_str,
        projection_status: proj_status_str,
    };

    // Layer B: Approvals
    let approval_stats = ApprovalStats {
        requested: input.approvals_requested,
        approved: input.approvals_approved,
        unexpected: input.unexpected_approvals,
    };

    // Layer C: Ledger
    let ledger =
        crate::work::ledger::WorkRuntimeLedger::open(input.paths, input.task_id, input.run_id)?;
    let facts = ledger.list_facts()?;
    let ledger_stats = analyze_ledger_facts(&facts);

    // Layer D: Artifact
    let ws_mgr = crate::work::workspace::WorkspaceManager::new(input.paths.clone());
    let ws = ws_mgr.get(input.workspace_id)?;
    let artifacts_list = crate::work::artifacts::list_with_paths(
        input.paths,
        input.workspace_id,
        Some(input.run_id),
    )?;

    let target_artifact = artifacts_list.iter().find(|a| {
        a.path == "output/archive_manifest.json" || a.path.ends_with("archive_manifest.json")
    });

    let output_dir_path = Path::new(&ws.output_dir);
    let manifest_file_path = output_dir_path.join("archive_manifest.json");
    let physical_manifest_exists = manifest_file_path.is_file();

    let actual_file_sha256 = if physical_manifest_exists {
        compute_file_sha256(&manifest_file_path).ok()
    } else {
        None
    };

    let mut artifact_valid = false;
    let mut registry_sha256 = None;

    if let Some(artifact) = target_artifact {
        if artifact.run_id.as_deref() == Some(input.run_id) {
            registry_sha256 = artifact.sha256.clone();
            if let (Some(ref reg_sha), Some(ref act_sha)) = (&registry_sha256, &actual_file_sha256)
            {
                if reg_sha == act_sha {
                    artifact_valid = true;
                }
            }
        }
    }

    let artifacts = vec![ArtifactReport {
        path: "output/archive_manifest.json".to_string(),
        sha256: registry_sha256,
        actual_sha256: actual_file_sha256,
        valid: artifact_valid,
    }];

    // Layer E: Business Semantic
    let mut semantic_assertions = Vec::new();

    if physical_manifest_exists {
        match fs::read_to_string(&manifest_file_path) {
            Ok(content) => match serde_json::from_str::<Value>(&content) {
                Ok(json) => {
                    let total_archived = json
                        .get("totalArchived")
                        .and_then(Value::as_i64)
                        .unwrap_or(-1);
                    semantic_assertions.push(SemanticAssertion {
                        name: "totalArchived".to_string(),
                        expected: Value::from(3),
                        actual: Value::from(total_archived),
                        passed: total_archived == 3,
                    });

                    let count_a = json
                        .get("customers")
                        .and_then(|c| c.get("A"))
                        .and_then(Value::as_i64)
                        .unwrap_or(-1);
                    semantic_assertions.push(SemanticAssertion {
                        name: "customers.A".to_string(),
                        expected: Value::from(1),
                        actual: Value::from(count_a),
                        passed: count_a == 1,
                    });

                    let count_b = json
                        .get("customers")
                        .and_then(|c| c.get("B"))
                        .and_then(Value::as_i64)
                        .unwrap_or(-1);
                    semantic_assertions.push(SemanticAssertion {
                        name: "customers.B".to_string(),
                        expected: Value::from(2),
                        actual: Value::from(count_b),
                        passed: count_b == 2,
                    });

                    let files_count = json
                        .get("files")
                        .and_then(Value::as_array)
                        .map(|a| a.len())
                        .unwrap_or(0);
                    semantic_assertions.push(SemanticAssertion {
                        name: "manifest.files.count".to_string(),
                        expected: Value::from(3),
                        actual: Value::from(files_count),
                        passed: files_count == 3,
                    });
                }
                Err(err) => {
                    semantic_assertions.push(SemanticAssertion {
                        name: "manifest_json_parse".to_string(),
                        expected: Value::from(true),
                        actual: Value::from(format!("JSON parse error: {err}")),
                        passed: false,
                    });
                }
            },
            Err(err) => {
                semantic_assertions.push(SemanticAssertion {
                    name: "manifest_read".to_string(),
                    expected: Value::from(true),
                    actual: Value::from(format!("Read error: {err}")),
                    passed: false,
                });
            }
        }
    } else {
        semantic_assertions.push(SemanticAssertion {
            name: "manifest_exists".to_string(),
            expected: Value::from(true),
            actual: Value::from(false),
            passed: false,
        });
    }

    // Check archived files on disk and verify content against fixture
    let file_a1_dest = output_dir_path.join("archive").join("A").join("A-001.txt");
    let file_b1_dest = output_dir_path.join("archive").join("B").join("B-001.txt");
    let file_b2_dest = output_dir_path.join("archive").join("B").join("B-002.txt");

    let file_a1_src = input.fixture_contracts_dir.join("A-001.txt");
    let file_b1_src = input.fixture_contracts_dir.join("B-001.txt");
    let file_b2_src = input.fixture_contracts_dir.join("B-002.txt");

    let check_file_match = |name: &str, src: &Path, dest: &Path| -> SemanticAssertion {
        if !dest.is_file() {
            return SemanticAssertion {
                name: format!("file_exists:{name}"),
                expected: Value::from(true),
                actual: Value::from(false),
                passed: false,
            };
        }
        let src_bytes = fs::read(src).unwrap_or_default();
        let dest_bytes = fs::read(dest).unwrap_or_default();
        let matched = !src_bytes.is_empty() && src_bytes == dest_bytes;
        SemanticAssertion {
            name: format!("content_match:{name}"),
            expected: Value::from(true),
            actual: Value::from(matched),
            passed: matched,
        }
    };

    semantic_assertions.push(check_file_match("A/A-001.txt", &file_a1_src, &file_a1_dest));
    semantic_assertions.push(check_file_match("B/B-001.txt", &file_b1_src, &file_b1_dest));
    semantic_assertions.push(check_file_match("B/B-002.txt", &file_b2_src, &file_b2_dest));

    Ok(VerificationResult {
        run_stats,
        approval_stats,
        ledger_stats,
        artifacts,
        semantic_assertions,
    })
}
