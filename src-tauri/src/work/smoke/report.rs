use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::verifier::{
    ApprovalStats, ArtifactReport, LedgerStats, RunVerificationStats, SemanticAssertion,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub requested: String,
    pub effective: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub requested: String,
    pub run_meta: String,
    pub runtime_reported: Option<String>,
    pub runtime_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffortInfo {
    pub requested: String,
    pub effective_launch: Option<String>,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmokeFailure {
    pub failure_kind: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmokeReport {
    pub version: u32,
    pub scenario: String,
    pub verdict: String,
    pub started_at: String,
    pub ended_at: String,
    pub duration_ms: u64,
    pub runtime: RuntimeInfo,
    pub model: ModelInfo,
    pub effort: EffortInfo,
    pub run: Option<RunVerificationStats>,
    pub approvals: ApprovalStats,
    pub ledger: LedgerStats,
    pub artifacts: Vec<ArtifactReport>,
    pub semantic_assertions: Vec<SemanticAssertion>,
    pub failure: Option<SmokeFailure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserved_workspace_path: Option<String>,
}

impl SmokeReport {
    pub fn to_markdown(&self) -> String {
        let duration_secs = self.duration_ms / 1000;
        let mins = duration_secs / 60;
        let secs = duration_secs % 60;
        let duration_str = format!("{mins}m {secs}s");

        let mut md = String::new();
        md.push_str("# AgentCabin Work Real Agent Smoke\n\n");
        md.push_str(&format!("Scenario: {}\n", self.scenario));
        md.push_str(&format!("Verdict: {}\n\n", self.verdict));

        md.push_str("## Runtime\n");
        md.push_str(&format!("- Requested: {}\n", self.runtime.requested));
        md.push_str(&format!("- Effective: {}\n\n", self.runtime.effective));

        md.push_str("## Model\n");
        md.push_str(&format!("- Requested: {}\n", self.model.requested));
        md.push_str(&format!("- RunMeta: {}\n", self.model.run_meta));
        if let Some(ref reported) = self.model.runtime_reported {
            md.push_str(&format!("- Runtime reported: {}\n", reported));
        } else {
            md.push_str("- Runtime reported: none (runtime does not return responseModel)\n");
        }
        md.push_str(&format!(
            "- Runtime verified: {}\n\n",
            self.model.runtime_verified
        ));

        md.push_str("## Effort\n");
        md.push_str(&format!("- Requested: {}\n", self.effort.requested));
        if let Some(ref eff) = self.effort.effective_launch {
            md.push_str(&format!("- Effective launch: {}\n", eff));
        } else {
            md.push_str("- Effective launch: unknown (not captured)\n");
        }
        md.push_str(&format!("- Verified: {}\n\n", self.effort.verified));

        md.push_str("## Duration\n");
        md.push_str(&format!("- {duration_str} ({} ms)\n\n", self.duration_ms));

        if let Some(run) = &self.run {
            md.push_str("## Run\n");
            md.push_str(&format!("- Workspace ID: {}\n", run.workspace_id));
            md.push_str(&format!("- Task ID: {}\n", run.task_id));
            md.push_str(&format!("- Run ID: {}\n", run.run_id));
            md.push_str(&format!("- Final: {}\n", run.final_status));
            md.push_str(&format!("- Projection: {}\n\n", run.projection_status));
        }

        md.push_str("## Approvals\n");
        md.push_str(&format!("- Requested: {}\n", self.approvals.requested));
        md.push_str(&format!("- Approved: {}\n", self.approvals.approved));
        md.push_str(&format!("- Unexpected: {}\n\n", self.approvals.unexpected));

        md.push_str("## Ledger\n");
        md.push_str(&format!("- ToolProposed: {}\n", self.ledger.tool_proposed));
        md.push_str(&format!("- ToolStarted: {}\n", self.ledger.tool_started));
        md.push_str(&format!("- ToolResult: {}\n", self.ledger.tool_result));
        md.push_str(&format!("- Failures: {}\n", self.ledger.tool_failures));
        md.push_str(&format!(
            "- Duplicate ToolStarted: {}\n\n",
            self.ledger.duplicate_tool_started
        ));

        md.push_str("## Artifacts\n");
        if self.artifacts.is_empty() {
            md.push_str("- None\n\n");
        } else {
            for art in &self.artifacts {
                let status = if art.valid { "PASS" } else { "FAIL" };
                let reg_sha = art.sha256.as_deref().unwrap_or("missing");
                let act_sha = art.actual_sha256.as_deref().unwrap_or("missing");
                md.push_str(&format!(
                    "- {} {} (registry sha256: {}, actual sha256: {})\n",
                    status, art.path, reg_sha, act_sha
                ));
            }
            md.push('\n');
        }

        md.push_str("## Semantic Assertions\n");
        if self.semantic_assertions.is_empty() {
            md.push_str("- None\n\n");
        } else {
            for sa in &self.semantic_assertions {
                let status = if sa.passed { "PASS" } else { "FAIL" };
                md.push_str(&format!(
                    "- {} {}: expected {}, actual {}\n",
                    status, sa.name, sa.expected, sa.actual
                ));
            }
            md.push('\n');
        }

        if let Some(failure) = &self.failure {
            md.push_str("## Failure\n");
            md.push_str(&format!("- Kind: {}\n", failure.failure_kind));
            md.push_str(&format!("- Reason: {}\n", failure.reason));
            if let Some(ref details) = failure.details {
                md.push_str(&format!("- Details: {}\n", details));
            }
            md.push('\n');
        }

        if let Some(ref ws_path) = self.preserved_workspace_path {
            md.push_str(&format!("Preserved Workspace Path: {}\n\n", ws_path));
        }

        md.push_str("## Result\n");
        md.push_str(&format!("{}\n", self.verdict));

        redact_secrets(&md)
    }

    pub fn write_to_dir(&self, report_dir: &Path) -> Result<(PathBuf, PathBuf), String> {
        fs::create_dir_all(report_dir).map_err(|e| {
            format!(
                "Failed to create report directory {}: {e}",
                report_dir.display()
            )
        })?;

        let timestamp = Utc::now().format("%Y%m%d-%H%M%S").to_string();
        let sanitized_model = self.model.requested.replace(['/', ':', '\\', ' '], "_");
        let base_name = format!("{}-{}-{}", timestamp, self.scenario, sanitized_model);

        let json_path = report_dir.join(format!("{base_name}.json"));
        let md_path = report_dir.join(format!("{base_name}.md"));

        let json_content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize report JSON: {e}"))?;
        let sanitized_json = redact_secrets(&json_content);
        let sanitized_md = self.to_markdown();

        fs::write(&json_path, sanitized_json).map_err(|e| {
            format!(
                "Failed to write report JSON to {}: {e}",
                json_path.display()
            )
        })?;
        fs::write(&md_path, sanitized_md)
            .map_err(|e| format!("Failed to write report MD to {}: {e}", md_path.display()))?;

        Ok((json_path, md_path))
    }
}

pub fn redact_sk_tokens(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("sk-") {
        if pos > 0 {
            let prev = rest[..pos].chars().last().unwrap();
            if prev.is_ascii_alphanumeric() {
                // Not a standalone token prefix (e.g. "task-", "mask-")
                result.push_str(&rest[..pos + 3]);
                rest = &rest[pos + 3..];
                continue;
            }
        }
        result.push_str(&rest[..pos]);
        result.push_str("sk-***REDACTED***");
        let token_start = pos + 3;
        let token_end = rest[token_start..]
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
            .map(|offset| token_start + offset)
            .unwrap_or(rest.len());
        rest = &rest[token_end..];
    }
    result.push_str(rest);
    result
}

pub fn redact_bearer_tokens(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.to_ascii_lowercase().find("bearer ") {
        result.push_str(&rest[..pos]);
        result.push_str("Bearer ***REDACTED***");
        let token_start = pos + 7;
        let token_val_start = rest[token_start..]
            .char_indices()
            .find(|(_, c)| !c.is_whitespace())
            .map(|(i, _)| token_start + i)
            .unwrap_or(rest.len());
        let token_end = rest[token_val_start..]
            .find(|c: char| {
                c.is_whitespace()
                    || c == '"'
                    || c == '\''
                    || c == ','
                    || c == ';'
                    || c == '`'
                    || c == '\n'
            })
            .map(|offset| token_val_start + offset)
            .unwrap_or(rest.len());
        rest = &rest[token_end..];
    }
    result.push_str(rest);
    result
}

pub fn redact_key_values(text: &str) -> String {
    let sensitive_keys = [
        "authorization",
        "api_key",
        "api-key",
        "apikey",
        "access_token",
        "refresh_token",
        "password",
        "secret",
        "token",
    ];

    let mut output_lines = Vec::new();
    for line in text.lines() {
        let mut result_line = String::with_capacity(line.len());
        let mut rest = line;

        while !rest.is_empty() {
            let mut earliest: Option<(usize, &str)> = None;
            let rest_lower = rest.to_ascii_lowercase();

            for &key in &sensitive_keys {
                if let Some(pos) = rest_lower.find(key) {
                    if pos > 0 {
                        let prev = rest[..pos].chars().last().unwrap();
                        if prev.is_ascii_alphanumeric() || prev == '_' {
                            continue;
                        }
                    }
                    if earliest.map_or(true, |(earliest_pos, _)| pos < earliest_pos) {
                        earliest = Some((pos, key));
                    }
                }
            }

            let Some((key_pos, key)) = earliest else {
                result_line.push_str(rest);
                break;
            };

            let after_key_pos = key_pos + key.len();
            let after_key = &rest[after_key_pos..];

            let mut sep_pos = None;
            for (c_idx, c) in after_key.char_indices() {
                if c == ':' || c == '=' {
                    sep_pos = Some(c_idx);
                    break;
                } else if c != '"' && c != '\'' && !c.is_whitespace() {
                    break;
                }
            }

            let Some(sep_idx) = sep_pos else {
                result_line.push_str(&rest[..after_key_pos]);
                rest = &rest[after_key_pos..];
                continue;
            };

            let after_sep_pos = after_key_pos + sep_idx + 1;
            let after_sep = &rest[after_sep_pos..];

            let mut val_start_offset = 0;
            let mut quote_char = None;
            for (c_idx, c) in after_sep.char_indices() {
                if quote_char.is_none() && (c == '"' || c == '\'') {
                    quote_char = Some(c);
                    val_start_offset = c_idx + 1;
                    break;
                } else if !c.is_whitespace() {
                    val_start_offset = c_idx;
                    break;
                }
            }

            let val_slice = &after_sep[val_start_offset..];
            let (val_end_offset, trailing_suffix) = if let Some(q) = quote_char {
                let end = val_slice.find(q).unwrap_or(val_slice.len());
                (end, "")
            } else {
                let trimmed = val_slice
                    .trim_end_matches(|c: char| c == ',' || c == ';' || c == '}' || c == ')');
                let trailing = &val_slice[trimmed.len()..];
                (trimmed.len(), trailing)
            };

            let prefix_to_keep = &rest[..after_sep_pos + val_start_offset];
            result_line.push_str(prefix_to_keep);
            result_line.push_str("***REDACTED***");
            result_line.push_str(trailing_suffix);

            let resume_pos =
                after_sep_pos + val_start_offset + val_end_offset + trailing_suffix.len();
            rest = &rest[resume_pos..];
        }

        output_lines.push(result_line);
    }

    let mut result = output_lines.join("\n");
    if text.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

pub fn redact_secrets(input: &str) -> String {
    let bearers = redact_bearer_tokens(input);
    let sks = redact_sk_tokens(&bearers);
    redact_key_values(&sks)
}
