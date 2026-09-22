use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkExecutionRequest {
    pub workspace_id: String,
    pub resource_id: String,
    pub action: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub input_paths: Vec<String>,
    #[serde(default)]
    pub expected_outputs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_run_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkExecutionStatus {
    #[default]
    Success,
    Failed,
    TimedOut,
    Cancelled,
    Denied,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionFailureKind {
    CapabilityFailure,
    SandboxDenied,
    SandboxInfrastructureFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkExecutionResult {
    pub execution_id: String,
    pub resource_id: String,
    pub action: String,
    pub status: WorkExecutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<ExecutionFailureKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub outputs: Vec<String>,
    pub started_at: String,
    pub finished_at: String,
}
