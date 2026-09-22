pub mod fake;
pub mod launcher;
pub mod macos;
pub mod policy;

#[cfg(test)]
pub mod tests;

use std::ffi::OsString;
use std::path::PathBuf;

pub use fake::FakeSandboxProvider;
pub use launcher::{NetworkPolicy, WorkSandboxLauncher, WorkSandboxPolicy};
pub(crate) use launcher::{WORK_NETWORK_POLICY_ENV, WORK_PROVIDER_NETWORK_POLICY};
pub use macos::MacosSeatbeltProvider;
pub use policy::{
    SandboxEnforcement, SandboxExecutionPolicy, SandboxMode, SandboxPolicyResolver,
    SandboxRequirement,
};

use crate::work::executor::ExecutionFailureKind;

#[derive(Debug, Clone)]
pub struct ExecutionCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub current_dir: PathBuf,
    pub envs: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct ConfinedCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub current_dir: PathBuf,
    pub envs: Vec<(String, String)>,
    pub enforcement: SandboxEnforcement,
    pub denial_signatures: Vec<String>,
    pub runner_failure_signatures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxError {
    Unavailable(String),
    PolicyUnsupported(String),
    RunnerFailed(String),
    EnforcementInsufficient {
        required: SandboxRequirement,
        actual: SandboxEnforcement,
    },
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxError::Unavailable(msg) => write!(f, "Sandbox provider unavailable: {}", msg),
            SandboxError::PolicyUnsupported(msg) => {
                write!(f, "Sandbox policy unsupported on this host: {}", msg)
            }
            SandboxError::RunnerFailed(msg) => {
                write!(f, "Sandbox runner failed to confine process: {}", msg)
            }
            SandboxError::EnforcementInsufficient { required, actual } => write!(
                f,
                "Sandbox enforcement requirement not met: required {:?}, got {:?}",
                required, actual
            ),
        }
    }
}

impl std::error::Error for SandboxError {}

pub trait NativeSandboxProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn probe(&self) -> Result<SandboxEnforcement, SandboxError>;
    fn confine(
        &self,
        command: &ExecutionCommand,
        policy: &SandboxExecutionPolicy,
    ) -> Result<ConfinedCommand, SandboxError>;
}

/// Fallback provider for unsupported OS platforms to strictly enforce Fail-Closed
pub struct UnavailableSandboxProvider {
    pub reason: String,
}

impl UnavailableSandboxProvider {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl NativeSandboxProvider for UnavailableSandboxProvider {
    fn name(&self) -> &'static str {
        "unavailable"
    }

    fn probe(&self) -> Result<SandboxEnforcement, SandboxError> {
        Err(SandboxError::Unavailable(self.reason.clone()))
    }

    fn confine(
        &self,
        _command: &ExecutionCommand,
        _policy: &SandboxExecutionPolicy,
    ) -> Result<ConfinedCommand, SandboxError> {
        Err(SandboxError::Unavailable(self.reason.clone()))
    }
}

/// Create the default native sandbox provider for the current operating system
pub fn create_default_sandbox_provider() -> Box<dyn NativeSandboxProvider> {
    #[cfg(target_os = "macos")]
    {
        Box::new(MacosSeatbeltProvider::new())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Box::new(UnavailableSandboxProvider::new(
            "Native Sandbox is not yet implemented on this operating system (fail-closed)",
        ))
    }
}

pub struct SandboxDenialClassifier;

impl SandboxDenialClassifier {
    /// Classifies an execution failure into SandboxInfrastructureFailure, SandboxDenied, or CapabilityFailure.
    pub fn classify_failure(
        stderr: &str,
        exit_code: Option<i32>,
        denial_signatures: &[String],
        runner_failure_signatures: &[String],
    ) -> ExecutionFailureKind {
        let err_lower = stderr.to_lowercase();

        // 1. Check if the sandbox runner itself failed (e.g. sandbox-exec syntax error or setup failure)
        for rule in runner_failure_signatures {
            if err_lower.contains(&rule.to_lowercase()) {
                return ExecutionFailureKind::SandboxInfrastructureFailure;
            }
        }

        // 2. Check backend-specific denial signatures
        for sig in denial_signatures {
            if err_lower.contains(&sig.to_lowercase()) {
                return ExecutionFailureKind::SandboxDenied;
            }
        }

        // 3. Fallback checks for common sandbox denial codes & messages
        if err_lower.contains("operation not permitted")
            || err_lower.contains("sandbox: denied")
            || err_lower.contains("deny file-write")
            || err_lower.contains("deny file-read")
            || err_lower.contains("deny process-exec")
            || err_lower.contains("eperm")
        {
            return ExecutionFailureKind::SandboxDenied;
        }

        if let Some(code) = exit_code {
            if code == 1 && (err_lower.contains("permissionerror") || err_lower.contains("eperm")) {
                return ExecutionFailureKind::SandboxDenied;
            }
        }

        ExecutionFailureKind::CapabilityFailure
    }
}
