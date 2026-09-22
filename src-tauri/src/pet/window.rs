use super::position::{
    clamp_position_to_monitor, load_position_file, save_position_file, snap_position_to_edges,
    PetPosition,
};
use super::state::{emit_settings, sync_to_window};
use serde_json::Value;
use tauri::window::Color;
use tauri::LogicalSize;
use tauri::{AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, Position, Size};

const BASE_WIDTH: u32 = 160;
const BASE_HEIGHT: u32 = 176;
pub const MIN_SCALE: f64 = 0.3;
pub const MAX_SCALE: f64 = 2.0;
pub const MIN_PATROL_PAUSE_MIN: u32 = 1;
pub const MAX_PATROL_PAUSE_MIN: u32 = 30;
const DEFAULT_POSITION_MARGIN_PX: i32 = 20;
const EDGE_SNAP_THRESHOLD_PX: i32 = 24;

fn set_transparent_background(win: &tauri::WebviewWindow) {
    if let Err(error) = win.set_background_color(Some(Color(0, 0, 0, 0))) {
        // Transparency is a best-effort capability on some Linux compositors.
        // Keep the pet usable if the platform rejects the runtime color change.
        log::debug!(
            "[pet/window] Failed to set transparent WebView background: {}",
            error
        );
    }
}

pub fn normalize_pet_scale(scale: f64) -> f64 {
    if scale.is_finite() {
        scale.clamp(MIN_SCALE, MAX_SCALE)
    } else {
        1.0
    }
}

pub fn normalize_pet_patrol_pause_min(pause_min: u32) -> u32 {
    pause_min.clamp(MIN_PATROL_PAUSE_MIN, MAX_PATROL_PAUSE_MIN)
}

fn pet_logical_size(scale: f64) -> LogicalSize<f64> {
    let scale = normalize_pet_scale(scale);
    LogicalSize::new(BASE_WIDTH as f64 * scale, BASE_HEIGHT as f64 * scale)
}

fn pet_physical_size(scale: f64, scale_factor: f64) -> PhysicalSize<u32> {
    let logical = pet_logical_size(scale);
    PhysicalSize::new(
        (logical.width * scale_factor).round() as u32,
        (logical.height * scale_factor).round() as u32,
    )
}

fn current_pet_physical_size(win: &tauri::WebviewWindow, scale: f64) -> PhysicalSize<u32> {
    win.inner_size()
        .unwrap_or_else(|_| pet_physical_size(scale, win.scale_factor().unwrap_or(1.0)))
}

pub(crate) fn monitor_bounds(monitor: &Monitor) -> (i32, i32, u32, u32) {
    let work_area = monitor.work_area();
    (
        work_area.position.x,
        work_area.position.y,
        work_area.size.width,
        work_area.size.height,
    )
}

fn monitor_contains(monitor: &Monitor, x: i32, y: i32) -> bool {
    let (mon_x, mon_y, mon_w, mon_h) = monitor_bounds(monitor);
    x >= mon_x
        && y >= mon_y
        && x < mon_x.saturating_add(mon_w as i32)
        && y < mon_y.saturating_add(mon_h as i32)
}

fn monitor_for_saved_position(
    win: &tauri::WebviewWindow,
    saved: Option<PetPosition>,
) -> Option<Monitor> {
    let monitors = win.available_monitors().unwrap_or_default();
    if let Some(position) = saved {
        if let Some(monitor) = monitors
            .iter()
            .find(|monitor| monitor_contains(monitor, position.x, position.y))
        {
            return Some(monitor.clone());
        }
    }
    win.current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten())
        .or_else(|| monitors.into_iter().next())
}

pub fn init_pet_window(app: &AppHandle) {
    let settings = crate::storage::settings::load();
    super::patrol::sync(app, &settings.user);
    if let Some(win) = app.get_webview_window(super::PET_WINDOW_LABEL) {
        if settings.user.pet_enabled {
            apply_pet_window_config(app, &win, &settings.user);
            // Window is configured in background; shown when pet://ready event fires
        } else {
            let _ = win.hide();
        }
    }
}

pub fn apply_pet_window_config(
    _app: &AppHandle,
    win: &tauri::WebviewWindow,
    settings: &crate::models::UserSettings,
) {
    set_transparent_background(win);
    let _ = win.set_size(Size::Logical(pet_logical_size(settings.pet_scale)));
    let size = current_pet_physical_size(win, settings.pet_scale);

    let _ = win.set_always_on_top(settings.pet_always_on_top);

    let saved = load_position_file();
    let monitor = monitor_for_saved_position(win, saved);

    if let Some(mon) = monitor {
        let (mon_x, mon_y, mon_w, mon_h) = monitor_bounds(&mon);
        let margin = if settings.pet_snap_to_edge {
            0
        } else {
            DEFAULT_POSITION_MARGIN_PX
        };

        let (target_x, target_y) = if let Some(saved) = saved {
            (saved.x, saved.y)
        } else {
            let default_x = mon_x + mon_w as i32 - size.width as i32 - margin;
            let default_y = mon_y + mon_h as i32 - size.height as i32 - margin;
            (default_x, default_y)
        };
        let (target_x, target_y) = if settings.pet_snap_to_edge {
            snap_position_to_edges(
                target_x,
                target_y,
                size.width,
                size.height,
                mon_x,
                mon_y,
                mon_w,
                mon_h,
                EDGE_SNAP_THRESHOLD_PX,
            )
        } else {
            (target_x, target_y)
        };

        let (clamped_x, clamped_y) = clamp_position_to_monitor(
            target_x,
            target_y,
            size.width,
            size.height,
            mon_x,
            mon_y,
            mon_w,
            mon_h,
        );

        let _ = win.set_position(Position::Physical(PhysicalPosition::new(
            clamped_x, clamped_y,
        )));
    }
}

#[tauri::command]
pub fn save_pet_position(app: AppHandle, x: i32, y: i32) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(super::PET_WINDOW_LABEL) {
        let settings = crate::storage::settings::get_user_settings();
        let size = current_pet_physical_size(&win, settings.pet_scale);
        let monitor = win
            .current_monitor()
            .ok()
            .flatten()
            .or_else(|| win.primary_monitor().ok().flatten())
            .or_else(|| {
                win.available_monitors()
                    .ok()
                    .and_then(|all| all.into_iter().next())
            });
        let (clamped_x, clamped_y) = if let Some(mon) = monitor {
            let (mon_x, mon_y, mon_w, mon_h) = monitor_bounds(&mon);
            clamp_position_to_monitor(x, y, size.width, size.height, mon_x, mon_y, mon_w, mon_h)
        } else {
            (x, y)
        };

        save_position_file(PetPosition {
            x: clamped_x,
            y: clamped_y,
        })?;
    } else {
        save_position_file(PetPosition { x, y })?;
    }
    Ok(())
}

/// Snap a manually dragged pet to a nearby monitor edge when the setting is enabled.
/// The command is intentionally a no-op when disabled, so the renderer can safely
/// invoke it at the end of every drag without duplicating settings state.
#[tauri::command]
pub fn snap_pet_to_edge(app: AppHandle) -> Result<(), String> {
    let settings = crate::storage::settings::get_user_settings();
    if !settings.pet_snap_to_edge {
        return Ok(());
    }

    let Some(win) = app.get_webview_window(super::PET_WINDOW_LABEL) else {
        return Ok(());
    };
    let Ok(position) = win.outer_position() else {
        return Ok(());
    };
    let size = current_pet_physical_size(&win, settings.pet_scale);
    let monitor = win
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten())
        .or_else(|| {
            win.available_monitors()
                .ok()
                .and_then(|all| all.into_iter().next())
        });
    let Some(monitor) = monitor else {
        return Ok(());
    };

    let (mon_x, mon_y, mon_w, mon_h) = monitor_bounds(&monitor);
    let (x, y) = snap_position_to_edges(
        position.x,
        position.y,
        size.width,
        size.height,
        mon_x,
        mon_y,
        mon_w,
        mon_h,
        EDGE_SNAP_THRESHOLD_PX,
    );
    if (x, y) != (position.x, position.y) {
        let _ = win.set_position(Position::Physical(PhysicalPosition::new(x, y)));
    }
    save_position_file(PetPosition { x, y })?;
    Ok(())
}

#[tauri::command]
pub fn load_pet_position(_app: AppHandle) -> Result<Option<PetPosition>, String> {
    Ok(load_position_file())
}

#[tauri::command]
pub fn toggle_pet_window(app: AppHandle, enabled: bool) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(super::PET_WINDOW_LABEL) {
        if enabled {
            let settings = crate::storage::settings::load();
            apply_pet_window_config(&app, &win, &settings.user);
            let _ = win.show();
        } else {
            let _ = win.hide();
        }
    }
    sync_to_window(&app);
    Ok(())
}

#[tauri::command]
pub fn update_pet_always_on_top(app: AppHandle, always_on_top: bool) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(super::PET_WINDOW_LABEL) {
        if let Err(e) = win.set_always_on_top(always_on_top) {
            log::warn!(
                "[pet/window] Failed to set always_on_top (wayland fallback?): {}",
                e
            );
        }
    }
    Ok(())
}

#[tauri::command]
pub fn set_pet_dragging(dragging: bool) -> Result<(), String> {
    super::patrol::set_dragging(dragging);
    Ok(())
}

#[tauri::command]
pub fn update_pet_scale(app: AppHandle, scale: f64) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(super::PET_WINDOW_LABEL) {
        let _ = win.set_size(Size::Logical(pet_logical_size(scale)));
        let size = current_pet_physical_size(&win, scale);

        let monitor = win
            .current_monitor()
            .ok()
            .flatten()
            .or_else(|| win.primary_monitor().ok().flatten())
            .or_else(|| {
                win.available_monitors()
                    .ok()
                    .and_then(|all| all.into_iter().next())
            });
        if let Some(mon) = monitor {
            if let Ok(pos) = win.outer_position() {
                let (mon_x, mon_y, mon_w, mon_h) = monitor_bounds(&mon);
                let (cx, cy) = clamp_position_to_monitor(
                    pos.x,
                    pos.y,
                    size.width,
                    size.height,
                    mon_x,
                    mon_y,
                    mon_w,
                    mon_h,
                );
                let _ = win.set_position(Position::Physical(PhysicalPosition::new(cx, cy)));
                let _ = save_position_file(PetPosition { x: cx, y: cy });
            }
        }
    }
    Ok(())
}

/// Apply only the pet-related portion of a settings patch. General settings
/// writes must not reset the pet's position or resize its window.
pub fn apply_settings_change(
    app: &AppHandle,
    settings: &crate::models::UserSettings,
    patch: &Value,
) {
    let Some(object) = patch.as_object() else {
        return;
    };
    let has_pet_change = object.keys().any(|key| {
        matches!(
            key.as_str(),
            "pet_enabled"
                | "pet_scale"
                | "pet_always_on_top"
                | "pet_id"
                | "pet_patrol_enabled"
                | "pet_patrol_pause_min"
                | "pet_snap_to_edge"
                | "pet_click_interaction_enabled"
                | "pet_grokbot_color"
                | "pet_grokbot_shape"
                | "pet_grokbot_parts"
                | "pet_grokbot_accessories"
        )
    });
    if !has_pet_change {
        return;
    }

    if let Some(win) = app.get_webview_window(super::PET_WINDOW_LABEL) {
        if object.contains_key("pet_enabled") {
            if settings.pet_enabled {
                apply_pet_window_config(app, &win, settings);
                let _ = win.show();
            } else {
                let _ = win.hide();
            }
        }
        if object.contains_key("pet_scale") {
            let _ = update_pet_scale(app.clone(), settings.pet_scale);
        }
        if object.contains_key("pet_always_on_top") {
            let _ = win.set_always_on_top(settings.pet_always_on_top);
        }
        if object.contains_key("pet_snap_to_edge") && settings.pet_snap_to_edge {
            let _ = snap_pet_to_edge(app.clone());
        }
    }

    super::patrol::sync(app, settings);

    if settings.pet_enabled {
        sync_to_window(app);
    } else {
        emit_settings(app, settings);
    }
}
