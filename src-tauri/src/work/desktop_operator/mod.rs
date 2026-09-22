//! Host-owned Computer Use V2 backend boundary.
//!
//! Work owns policy, Inbox and audit. Platform implementations live behind
//! this boundary so provider runtimes never execute native helpers directly.

mod driver;

pub use driver::{DesktopOperatorManager, DesktopOperatorStatus};

static MANAGER: std::sync::OnceLock<DesktopOperatorManager> = std::sync::OnceLock::new();

pub fn desktop_operator_manager() -> &'static DesktopOperatorManager {
    MANAGER.get_or_init(DesktopOperatorManager::new)
}

/// Desktop Use stays opt-in while the native backend is under development.
pub fn is_requested() -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    if let Some(enabled) = crate::storage::profile_bindings::is_desktop_use_enabled() {
        return enabled;
    }
    std::env::var("AGENTCABIN_DESKTOP_USE_ENABLED")
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
}

pub fn is_ready() -> bool {
    desktop_operator_manager().helper_available()
}

pub fn is_enabled() -> bool {
    is_requested() && is_ready()
}
