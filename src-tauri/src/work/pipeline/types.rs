use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::work::executor::ExecutionFailureKind;
use crate::work::models::{ExecutionContext, SideEffectClass};

/// Common structured ToolIntent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolIntent {
    pub task_id: String,
    pub work_run_id: String,
    pub session_id: Option<String>,
    pub workspace_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub action: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub input_paths: Vec<String>,
    #[serde(default)]
    pub expected_outputs: Vec<String>,
    #[serde(default)]
    pub execution_context: ExecutionContext,
    #[serde(default)]
    pub policy_revision: Option<u64>,
}

impl ToolIntent {
    pub fn compute_arguments_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(
            serde_json::to_string(&self.arguments)
                .unwrap_or_default()
                .as_bytes(),
        );
        format!("{:x}", hasher.finalize())
    }

    pub fn target_string(&self) -> String {
        if let Some(target) = self.arguments.get("target").and_then(|v| v.as_str()) {
            target.to_string()
        } else if let Some(path) = self.arguments.get("path").and_then(|v| v.as_str()) {
            path.to_string()
        } else if self.tool_name.starts_with("browser_") {
            self.arguments
                .get("url")
                .or_else(|| self.arguments.get("selector"))
                .or_else(|| self.arguments.get("ref"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
                .unwrap_or_else(|| {
                    if !self.action.is_empty() {
                        format!("{}.{}", self.tool_name, self.action)
                    } else {
                        self.tool_name.clone()
                    }
                })
        } else if self.tool_name.starts_with("desktop_") {
            self.arguments
                .get("app")
                .or_else(|| self.arguments.get("name"))
                .or_else(|| self.arguments.get("bundle_id"))
                .or_else(|| self.arguments.get("element_token"))
                .or_else(|| self.arguments.get("key"))
                .or_else(|| self.arguments.get("text"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
                .unwrap_or_else(|| {
                    if !self.action.is_empty() {
                        format!("{}.{}", self.tool_name, self.action)
                    } else {
                        self.tool_name.clone()
                    }
                })
        } else if let Some(res_id) = self
            .arguments
            .get("resource_id")
            .or_else(|| self.arguments.get("resourceId"))
            .and_then(|v| v.as_str())
        {
            if !self.action.is_empty() {
                format!("{}.{}", res_id, self.action)
            } else {
                res_id.to_string()
            }
        } else if !self.action.is_empty() {
            format!("{}.{}", self.tool_name, self.action)
        } else {
            self.tool_name.clone()
        }
    }
}

/// Authoritative execution outcome from the Harness Tool Pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub action: String,
    pub success: bool,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<ExecutionFailureKind>,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub outputs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
    pub side_effect_class: SideEffectClass,
    pub started_at: String,
    pub finished_at: String,
}

impl ToolResult {
    pub fn synthetic_interrupted(
        tool_call_id: &str,
        tool_name: &str,
        action: &str,
        reason: &str,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            tool_call_id: tool_call_id.to_string(),
            tool_name: tool_name.to_string(),
            action: action.to_string(),
            success: false,
            status: "interrupted".to_string(),
            failure_kind: None,
            exit_code: Some(-1),
            stdout: String::new(),
            stderr: format!("Tool call interrupted before/during dispatch: {reason}"),
            outputs: Vec::new(),
            interaction_id: None,
            side_effect_class: SideEffectClass::Read,
            started_at: now.clone(),
            finished_at: now,
        }
    }

    pub fn denied(tool_call_id: &str, tool_name: &str, action: &str, reason: &str) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            tool_call_id: tool_call_id.to_string(),
            tool_name: tool_name.to_string(),
            action: action.to_string(),
            success: false,
            status: "denied".to_string(),
            failure_kind: None,
            exit_code: Some(-1),
            stdout: String::new(),
            stderr: reason.to_string(),
            outputs: Vec::new(),
            interaction_id: None,
            side_effect_class: SideEffectClass::Read,
            started_at: now.clone(),
            finished_at: now,
        }
    }

    pub fn error(tool_call_id: &str, tool_name: &str, action: &str, reason: &str) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            tool_call_id: tool_call_id.to_string(),
            tool_name: tool_name.to_string(),
            action: action.to_string(),
            success: false,
            status: "error".to_string(),
            failure_kind: None,
            exit_code: Some(-1),
            stdout: String::new(),
            stderr: reason.to_string(),
            outputs: Vec::new(),
            interaction_id: None,
            side_effect_class: SideEffectClass::Read,
            started_at: now.clone(),
            finished_at: now,
        }
    }
}
