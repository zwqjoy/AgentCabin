//! Explicit capability seams for AgentCabin Work Mode.
//!
//! Consumers depend on capability interfaces rather than concrete implementations.
//! Pi Extension is only a Runtime Adapter and does not own authoritative state.

// Keep the historical module path available while exposing the one canonical
// launch contract. The former local WorkRuntimeAdapter duplicated the runtime
// adapter API and made a future DSH implementation ambiguous.
pub use crate::work::runtime::WorkRuntimeAdapter;

use crate::work::models::{
    PendingInteraction, PendingInteractionState, WorkExecutionManifest, WorkPolicy,
};
use crate::work::paths::WorkPaths;
use async_trait::async_trait;

#[async_trait]
pub trait WorkHumanLoop: Send + Sync {
    /// Surface a pending interaction (either inline UI or durable Inbox).
    async fn present_interaction(&self, interaction: &PendingInteraction) -> Result<(), String>;

    /// Resolve a pending interaction.
    async fn resolve_interaction(
        &self,
        interaction_id: &str,
        state: PendingInteractionState,
        resolution: Option<serde_json::Value>,
    ) -> Result<PendingInteraction, String>;
}

#[async_trait]
pub trait WorkExecutorBackend: Send + Sync {
    /// Execute a verified capability or command in the restricted host environment.
    async fn execute(
        &self,
        paths: &WorkPaths,
        manifest: &WorkExecutionManifest,
        directory: &std::path::Path,
        request: &crate::work::executor::WorkExecutionRequest,
        is_builtin: bool,
    ) -> Result<crate::work::executor::WorkExecutionResult, String>;
}

pub trait WorkPolicyEvaluator: Send + Sync {
    /// Evaluate decision (Allow, Ask, Deny) for a tool intent.
    fn evaluate(
        &self,
        policy: &WorkPolicy,
        tool_name: &str,
        target: &str,
        context: crate::work::models::ExecutionContext,
    ) -> crate::work::policy::WorkPolicyDecision;
}

pub trait ArtifactValidator: Send + Sync {
    /// Validate that an artifact exists, matches schema, and is uncorrupted.
    fn validate(&self, artifact_path: &std::path::Path, artifact_type: &str) -> Result<(), String>;
}
