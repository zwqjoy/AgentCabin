//! Work Runtime Adapter abstraction layer.
//!
//! This module defines the boundary between the Work Harness and the
//! provider-specific runtime. The Work Harness drives any runtime through
//! the [`WorkRuntimeAdapter`] contract. Provider-specific setup (CLI,
//! extension paths, env vars, etc.) stays inside the concrete adapter.
//!
//! Current registrations:
//!   - Pi     ✅  (PiWorkRuntimeAdapter)
//!   - DSH    ❌  (UnsupportedWorkRuntime)
//!   - Claude  ❌  (UnsupportedWorkRuntime)
//!   - Codex   ❌  (UnsupportedWorkRuntime)
//!   - Grok    ❌  (UnsupportedWorkRuntime)

pub mod pi;

pub use pi::PiWorkRuntimeAdapter;

pub fn get_pi_work_runtime(agent: &str) -> Result<&'static PiWorkRuntimeAdapter, WorkRuntimeError> {
    let normalized = agent.trim().to_lowercase();
    if normalized.is_empty() || normalized == "pi" {
        Ok(&PiWorkRuntimeAdapter)
    } else {
        Err(WorkRuntimeError::UnsupportedAgent(agent.to_string()))
    }
}

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::agent::adapter::ActorSessionMap;
use crate::agent::capability_resolver::{EffectiveCapabilities, RuntimeProviderKind};
use crate::agent::session_actor::ActorCommand;
use crate::web_server::broadcaster::BroadcastEmitter;
use crate::work::paths::WorkPaths;

/// Capabilities that a Work Runtime Adapter declares it supports.
#[derive(Debug, Clone, Default)]
pub struct WorkRuntimeCapabilities {
    pub supports_resume: bool,
    pub supports_continuation: bool,
    pub supports_follow_up: bool,
    pub supports_subagents: bool,
}

/// Runtime-neutral input for provider-specific preparation.
#[derive(Debug, Clone)]
pub struct WorkRuntimePrepareContext {
    pub paths: WorkPaths,
    pub run: crate::models::RunMeta,
    pub capabilities: EffectiveCapabilities,
    pub context_plan: Option<crate::work::context::WorkContextPlan>,
}

/// Optional per-run launch overrides (e.g. for smoke testing without modifying user settings).
#[derive(Debug, Clone, Default)]
pub struct WorkLaunchOverrides {
    pub runtime: Option<RuntimeProviderKind>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub forbid_model_fallback: bool,
}

/// Runtime-neutral input for starting a Work session actor.
///
/// The concrete adapter owns the provider settings it derives from this
/// request. Work Core only supplies the run identity, lifecycle operation,
/// resolved capabilities, and host-provided environment.
#[derive(Debug)]
pub struct WorkRuntimeLaunchRequest {
    pub run: crate::models::RunMeta,
    pub session_mode: crate::models::SessionMode,
    pub session_id: Option<String>,
    pub permission_mode_override: Option<String>,
    pub capabilities: EffectiveCapabilities,
    /// Shared per-run Computer Use gate used by every Work provider.
    pub desktop_use_enabled: bool,
    pub bridge: WorkBridgeLaunchInfo,
    pub extra_env: HashMap<String, String>,
    pub context_plan: crate::work::context::WorkContextPlan,
    pub launch_overrides: Option<WorkLaunchOverrides>,
}

/// Observable stages of the shared Work launch protocol.
///
/// The sequence is deliberately owned by Work Core rather than a provider
/// adapter: provider code may prepare and spawn its own transport, but it
/// cannot bypass bridge registration or reorder the authority boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkRuntimeLaunchStage {
    Prepared,
    BridgeRegistered,
    ActorSpawned,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkRuntimeLaunchTrace {
    stages: Vec<WorkRuntimeLaunchStage>,
}

impl WorkRuntimeLaunchTrace {
    pub fn record(&mut self, stage: WorkRuntimeLaunchStage) -> Result<(), WorkRuntimeError> {
        let expected = match self.stages.as_slice() {
            [] => WorkRuntimeLaunchStage::Prepared,
            [WorkRuntimeLaunchStage::Prepared] => WorkRuntimeLaunchStage::BridgeRegistered,
            [WorkRuntimeLaunchStage::Prepared, WorkRuntimeLaunchStage::BridgeRegistered] => {
                WorkRuntimeLaunchStage::ActorSpawned
            }
            _ => {
                return Err(WorkRuntimeError::LaunchFailed(
                    "Work Runtime launch trace is already complete".to_string(),
                ));
            }
        };
        if stage != expected {
            return Err(WorkRuntimeError::LaunchFailed(format!(
                "invalid Work Runtime launch stage: expected {:?}, got {:?}",
                expected, stage
            )));
        }
        self.stages.push(stage);
        Ok(())
    }

    pub fn stages(&self) -> &[WorkRuntimeLaunchStage] {
        &self.stages
    }

    pub fn is_complete(&self) -> bool {
        self.stages.len() == 3
    }
}

/// Runtime-neutral bridge identity for one Work launch.
///
/// The bridge is owned by Work Core, so provider adapters receive its canonical
/// identity and host-selected proxy instead of deriving workspace/task state
/// themselves. Standalone Work chats use the run as their durable WorkRun
/// identity; workspace-backed runs must carry the full Task/WorkRun tuple and
/// fail closed when it is incomplete. The adapter may project the identity into
/// its own transport, but cannot create a second Work authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkBridgeLaunchInfo {
    pub work_run_id: String,
    pub workspace_id: Option<String>,
    pub task_id: Option<String>,
    pub execution_context: crate::work::models::ExecutionContext,
    pub proxy_url: Option<String>,
}

impl WorkBridgeLaunchInfo {
    pub fn from_run(
        run: &crate::models::RunMeta,
        proxy_url: Option<String>,
    ) -> Result<Self, String> {
        let work_run_id = run.work_run_id.clone().unwrap_or_else(|| run.id.clone());
        let execution_context = run
            .work_execution_context
            .unwrap_or(crate::work::models::ExecutionContext::Attended);
        // If an automation task id exists, use it; otherwise use run.id as the stable namespace
        let task_id = run.work_task_id.clone().or_else(|| Some(run.id.clone()));

        Ok(Self {
            work_run_id,
            workspace_id: run.workspace_id.clone(),
            task_id,
            execution_context,
            proxy_url,
        })
    }
}

/// Errors emitted by the Work Runtime layer.
#[derive(Debug)]
pub enum WorkRuntimeError {
    /// The provider is not yet supported in Work mode.
    UnsupportedRuntime(RuntimeProviderKind),
    /// The agent identifier is unknown or not supported.
    UnsupportedAgent(String),
    /// The provider is recognised but currently unavailable.
    RuntimeUnavailable(RuntimeProviderKind, String),
    /// The provider-specific launch preparation failed.
    LaunchFailed(String),
}

impl std::fmt::Display for WorkRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedRuntime(p) => write!(
                f,
                "Work Runtime does not support provider '{}'. \
                 No Work adapter is registered for this provider.",
                p.as_str()
            ),
            Self::UnsupportedAgent(a) => write!(
                f,
                "Work Runtime does not recognize agent '{}'. \
                 No Work adapter is registered for this agent.",
                a
            ),
            Self::RuntimeUnavailable(p, msg) => {
                write!(f, "Work Runtime '{}' unavailable: {}", p.as_str(), msg)
            }
            Self::LaunchFailed(msg) => write!(f, "Work Runtime launch failed: {}", msg),
        }
    }
}

impl From<WorkRuntimeError> for String {
    fn from(e: WorkRuntimeError) -> Self {
        e.to_string()
    }
}

/// The contract that every Work Runtime Adapter must satisfy.
///
/// An adapter is responsible for the Work-harness → runtime bridge:
/// - declaring provider identity + capabilities
/// - provisioning provider-specific packages / extensions (`prepare_runtime`)
/// - spawning the session actor transport (`spawn_session_actor`)
///
/// It receives the canonical [`EffectiveCapabilities`] from the resolver
/// (ensuring single source of truth for runtime directories and homes).
///
/// It must **not** own: Workspace, Task, WorkRun, Artifact, Permission,
/// Recovery, Browser Host, MCP Host, Connector Host, Sandbox, Scheduler.
#[async_trait]
pub trait WorkRuntimeAdapter: std::fmt::Debug + Send + Sync {
    fn provider(&self) -> RuntimeProviderKind;
    fn capabilities(&self) -> WorkRuntimeCapabilities;

    /// Report whether the provider currently persists sessions for this run.
    /// The provider keeps its settings representation private.
    fn session_persistence_enabled(
        &self,
        run: &crate::models::RunMeta,
    ) -> Result<bool, WorkRuntimeError>;

    /// Provider-specific runtime package/extension provisioning.
    async fn prepare_runtime(
        &self,
        context: &WorkRuntimePrepareContext,
    ) -> Result<(), WorkRuntimeError>;

    /// Provider-specific transport spawn boundary.
    async fn spawn_session_actor(
        &self,
        emitter: Arc<BroadcastEmitter>,
        sessions: ActorSessionMap,
        request: WorkRuntimeLaunchRequest,
        cancel_token: CancellationToken,
    ) -> Result<Sender<ActorCommand>, String>;
}

#[cfg(test)]
mod tests {
    use super::{WorkRuntimeLaunchStage, WorkRuntimeLaunchTrace};

    #[test]
    fn launch_trace_requires_prepare_bridge_then_spawn() {
        let mut trace = WorkRuntimeLaunchTrace::default();
        assert!(trace.record(WorkRuntimeLaunchStage::Prepared).is_ok());
        assert!(trace
            .record(WorkRuntimeLaunchStage::BridgeRegistered)
            .is_ok());
        assert!(trace.record(WorkRuntimeLaunchStage::ActorSpawned).is_ok());
        assert!(trace.is_complete());
        assert_eq!(trace.stages().len(), 3);
    }

    #[test]
    fn launch_trace_rejects_spawn_before_bridge() {
        let mut trace = WorkRuntimeLaunchTrace::default();
        let error = trace
            .record(WorkRuntimeLaunchStage::ActorSpawned)
            .expect_err("spawn must not bypass bridge registration");
        assert!(error.to_string().contains("Prepared"));
        assert!(trace.stages().is_empty());
    }
}
