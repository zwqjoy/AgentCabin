use serde::{Deserialize, Serialize};

/// Capabilities are facts about the running Pi process, not settings toggles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PiFeatureCapabilities {
    pub commands_ready: bool,
    pub plan_available: bool,
    pub goal_available: bool,
    pub permission_available: bool,
    pub session_tree_available: bool,
    pub fork_available: bool,
    pub clone_available: bool,
    pub steer_available: bool,
    pub follow_up_available: bool,
    pub clear_queue_available: bool,
    pub navigate_tree_available: bool,
}

impl PiFeatureCapabilities {
    pub fn from_commands(
        commands_ready: bool,
        command_names: impl Iterator<Item = String>,
        plan_enabled: bool,
        goal_enabled: bool,
        permission_enabled: bool,
    ) -> Self {
        let names: std::collections::HashSet<String> = command_names
            .map(|name| canonical_command_name(&name))
            .collect();
        Self {
            commands_ready,
            plan_available: plan_enabled && names.contains("plan"),
            goal_available: goal_enabled && names.contains("goal"),
            permission_available: permission_enabled && names.contains("permission-system"),
            session_tree_available: true,
            fork_available: true,
            clone_available: true,
            steer_available: true,
            follow_up_available: true,
            clear_queue_available: true,
            // Pi has no navigate_tree RPC.
            navigate_tree_available: false,
        }
    }
}

/// Pi appends `:1`, `:2`, ... when multiple extensions register the same command.
/// Capability detection should treat those conflict-qualified names as the same
/// command so a duplicate package does not hide an otherwise available feature.
fn canonical_command_name(name: &str) -> String {
    let normalized = name.trim_start_matches('/').to_ascii_lowercase();
    let Some((base, suffix)) = normalized.rsplit_once(':') else {
        return normalized;
    };
    if !base.is_empty() && !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        base.to_string()
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::PiFeatureCapabilities;

    #[test]
    fn recognizes_conflict_suffixed_extension_commands() {
        let capabilities = PiFeatureCapabilities::from_commands(
            true,
            ["plan:2".to_string(), "permission-system:1".to_string()].into_iter(),
            true,
            false,
            true,
        );

        assert!(capabilities.plan_available);
        assert!(capabilities.permission_available);
        assert!(capabilities.clear_queue_available);
        assert!(!capabilities.navigate_tree_available);
    }

    #[test]
    fn keeps_non_numeric_command_suffixes_intact() {
        let capabilities = PiFeatureCapabilities::from_commands(
            true,
            ["permission-system:debug".to_string()].into_iter(),
            false,
            false,
            true,
        );

        assert!(!capabilities.permission_available);
        assert!(capabilities.clear_queue_available);
        assert!(!capabilities.navigate_tree_available);
    }
}
