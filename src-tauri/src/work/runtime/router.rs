//! Work Runtime Router.
//!
//! Routes a [`RuntimeProviderKind`] to its [`WorkRuntimeAdapter`].
//!
//! Rules:
//! - No silent fallback to Pi.
//! - No fake / empty adapters.
//! - Callers must attach provider failures to a durable WorkRun when one has
//!   already been created (for example, scheduled execution).
//! - Claude, Codex, Grok → explicit `UnsupportedRuntime` error.

use crate::agent::capability_resolver::RuntimeProviderKind;

use super::dsh::DshWorkRuntimeAdapter;
use super::pi::PiWorkRuntimeAdapter;
use super::{WorkRuntimeAdapter, WorkRuntimeError};

/// Route a provider kind to its concrete Work Runtime Adapter.
///
/// Returns `Err(WorkRuntimeError::UnsupportedRuntime)` for any provider that
/// is not yet implemented in Work mode. Callers must propagate this error
/// before creating any WorkRun or session state.
pub fn route(
    provider: RuntimeProviderKind,
) -> Result<Box<dyn WorkRuntimeAdapter>, WorkRuntimeError> {
    match provider {
        RuntimeProviderKind::Pi => Ok(Box::new(PiWorkRuntimeAdapter)),
        RuntimeProviderKind::Dsh => Ok(Box::new(DshWorkRuntimeAdapter)),
        other => Err(WorkRuntimeError::UnsupportedRuntime(other)),
    }
}

/// Route a raw agent identifier string to its concrete Work Runtime Adapter.
///
/// If the agent string is unrecognised, returns `Err(WorkRuntimeError::UnsupportedAgent)`.
/// Never silently defaults to Pi.
pub fn route_agent_str(agent: &str) -> Result<Box<dyn WorkRuntimeAdapter>, WorkRuntimeError> {
    let provider = RuntimeProviderKind::try_from_agent_str(agent)
        .map_err(|_| WorkRuntimeError::UnsupportedAgent(agent.to_string()))?;
    route(provider)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::capability_resolver::RuntimeProviderKind;

    #[test]
    fn routes_pi_to_pi_adapter() {
        let adapter = route(RuntimeProviderKind::Pi).expect("Pi should be supported");
        assert_eq!(adapter.provider(), RuntimeProviderKind::Pi);
    }

    #[test]
    fn rejects_claude_without_fallback() {
        let err =
            route(RuntimeProviderKind::Claude).expect_err("Claude Work is not yet implemented");
        assert!(
            matches!(
                err,
                WorkRuntimeError::UnsupportedRuntime(RuntimeProviderKind::Claude)
            ),
            "Expected UnsupportedRuntime(Claude), got: {err}"
        );
    }

    #[test]
    fn rejects_codex_without_fallback() {
        let err = route(RuntimeProviderKind::Codex).expect_err("Codex Work is not yet implemented");
        assert!(
            matches!(
                err,
                WorkRuntimeError::UnsupportedRuntime(RuntimeProviderKind::Codex)
            ),
            "Expected UnsupportedRuntime(Codex), got: {err}"
        );
    }

    #[test]
    fn rejects_grok_without_fallback() {
        let err = route(RuntimeProviderKind::Grok).expect_err("Grok Work is not yet implemented");
        assert!(
            matches!(
                err,
                WorkRuntimeError::UnsupportedRuntime(RuntimeProviderKind::Grok)
            ),
            "Expected UnsupportedRuntime(Grok), got: {err}"
        );
    }

    #[test]
    fn routes_dsh_to_dsh_adapter() {
        let adapter = route(RuntimeProviderKind::Dsh).expect("DSH should be supported");
        assert_eq!(adapter.provider(), RuntimeProviderKind::Dsh);
    }

    #[test]
    fn unsupported_error_message_names_provider() {
        let err = route(RuntimeProviderKind::Claude).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("claude"),
            "Error message should name the provider: {msg}"
        );
        assert!(
            msg.contains("No Work adapter"),
            "Error should explain that no Work adapter is registered: {msg}"
        );
    }

    #[test]
    fn rejects_unknown_agent_string_without_fallback() {
        let err = route_agent_str("unknown_agent_xyz")
            .expect_err("Unknown agent must be rejected without fallback");
        assert!(
            matches!(err, WorkRuntimeError::UnsupportedAgent(_)),
            "Expected UnsupportedAgent, got: {err}"
        );
    }
}
