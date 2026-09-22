pub mod patrol;
pub mod position;
pub mod state;
pub mod window;

use tauri::{AppHandle, Listener, Manager};

pub const PET_WINDOW_LABEL: &str = "pet";
pub const PET_READY_EVENT: &str = "pet://ready";
pub const PET_STATE_EVENT: &str = "pet://state";
pub const PET_NOTIFICATION_EVENT: &str = "pet://notification";
pub const PET_SETTINGS_EVENT: &str = "pet://settings";

pub use state::{handle_bus_event, sync_to_window};
pub use window::*;

/// Register the one app-level listener used to replay settings/state after the
/// independent pet Webview has mounted. The listener intentionally lives in Rust
/// so it remains available even when the main window is on a non-chat route.
pub fn register_ready_listener(app: &AppHandle) {
    let handle = app.clone();
    app.listen(PET_READY_EVENT, move |_| {
        sync_to_window(&handle);
        if let Some(win) = handle.get_webview_window(PET_WINDOW_LABEL) {
            let settings = crate::storage::settings::load();
            if settings.user.pet_enabled {
                let _ = win.show();
            }
        }
    });
}
