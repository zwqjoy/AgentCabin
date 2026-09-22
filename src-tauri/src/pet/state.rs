use crate::models::{BusEvent, UserSettings};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

use super::{PET_SETTINGS_EVENT, PET_STATE_EVENT, PET_WINDOW_LABEL};

pub const PET_TRANSITION_HOLD_MS: u64 = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PetMode {
    #[default]
    Idle,
    Waiting,
    Running,
    Review,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PetAggregateState {
    pub mode: PetMode,
    pub running_count: usize,
    pub error_count: usize,
    pub active_run_id: Option<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PetNotification {
    #[serde(rename = "type")]
    pub notif_type: String,
    pub text: String,
    pub agent_id: Option<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PetSettingsPayload {
    pub pet_enabled: bool,
    pub pet_scale: f64,
    pub pet_always_on_top: bool,
    pub pet_id: String,
    pub pet_patrol_enabled: bool,
    pub pet_patrol_pause_min: u32,
    pub pet_snap_to_edge: bool,
    pub pet_click_interaction_enabled: bool,
    pub pet_grokbot_color: String,
    pub pet_grokbot_shape: String,
    pub pet_grokbot_parts: Vec<String>,
    pub pet_grokbot_accessories: Vec<String>,
}

#[derive(Debug, Clone)]
struct RunPetState {
    mode: PetMode,
    generation: u64,
}

#[derive(Default)]
struct PetRuntime {
    runs: Mutex<HashMap<String, RunPetState>>,
    next_generation: AtomicU64,
    last_emitted: Mutex<Option<PetAggregateState>>,
}

static RUNTIME: Lazy<Arc<PetRuntime>> = Lazy::new(|| Arc::new(PetRuntime::default()));

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn mode_priority(mode: PetMode) -> u8 {
    match mode {
        PetMode::Failed => 5,
        PetMode::Running => 4,
        PetMode::Waiting => 3,
        PetMode::Review => 2,
        PetMode::Idle => 1,
    }
}

/// Map the backend's normalized run-state string to the pet's public mode.
/// This consumes AgentCabin's BusEvent vocabulary, not an agent protocol.
pub fn map_run_state_to_pet_mode(state: &str) -> PetMode {
    match state {
        "spawning" | "loading" => PetMode::Waiting,
        "running" => PetMode::Running,
        "completed" => PetMode::Review,
        "failed" => PetMode::Failed,
        "idle" | "stopped" | "ready" | "empty" => PetMode::Idle,
        _ => PetMode::Idle,
    }
}

fn aggregate_runs(runs: &HashMap<String, RunPetState>) -> PetAggregateState {
    let mut running_count = 0;
    let mut error_count = 0;
    let mut selected: Option<(&String, &RunPetState)> = None;

    for (run_id, run) in runs {
        match run.mode {
            PetMode::Running => running_count += 1,
            PetMode::Failed => error_count += 1,
            _ => {}
        }

        let replace = selected.is_none_or(|(_, current)| {
            mode_priority(run.mode) > mode_priority(current.mode)
                || (run.mode == current.mode && run.generation > current.generation)
        });
        if replace && run.mode != PetMode::Idle {
            selected = Some((run_id, run));
        }
    }

    PetAggregateState {
        mode: selected.map(|(_, run)| run.mode).unwrap_or(PetMode::Idle),
        running_count,
        error_count,
        active_run_id: selected.map(|(run_id, _)| run_id.clone()),
        timestamp: now_millis(),
    }
}

fn same_visible_state(left: &PetAggregateState, right: &PetAggregateState) -> bool {
    left.mode == right.mode
        && left.running_count == right.running_count
        && left.error_count == right.error_count
        && left.active_run_id == right.active_run_id
}

fn should_emit_state(settings: &UserSettings) -> bool {
    settings.pet_enabled
}

fn emit_state_if_changed(app: &AppHandle, state: &PetAggregateState, force: bool) {
    if !force && !should_emit_state(&crate::storage::settings::get_user_settings()) {
        return;
    }

    let should_emit = {
        let mut last = RUNTIME
            .last_emitted
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let changed = force
            || last
                .as_ref()
                .is_none_or(|previous| !same_visible_state(previous, state));
        if changed {
            *last = Some(state.clone());
        }
        changed
    };

    if should_emit {
        if let Err(error) = app.emit_to(PET_WINDOW_LABEL, PET_STATE_EVENT, state) {
            log::debug!("[pet/state] state emit skipped: {}", error);
        }
    }
}

pub fn settings_payload(settings: &UserSettings) -> PetSettingsPayload {
    PetSettingsPayload {
        pet_enabled: settings.pet_enabled,
        pet_scale: super::window::normalize_pet_scale(settings.pet_scale),
        pet_always_on_top: settings.pet_always_on_top,
        pet_id: settings.pet_id.clone(),
        pet_patrol_enabled: settings.pet_patrol_enabled,
        pet_patrol_pause_min: super::window::normalize_pet_patrol_pause_min(
            settings.pet_patrol_pause_min,
        ),
        pet_snap_to_edge: settings.pet_snap_to_edge,
        pet_click_interaction_enabled: settings.pet_click_interaction_enabled,
        pet_grokbot_color: settings.pet_grokbot_color.clone(),
        pet_grokbot_shape: settings.pet_grokbot_shape.clone(),
        pet_grokbot_parts: settings.pet_grokbot_parts.clone(),
        pet_grokbot_accessories: settings.pet_grokbot_accessories.clone(),
    }
}

pub fn emit_settings(app: &AppHandle, settings: &UserSettings) {
    let payload = settings_payload(settings);
    if let Err(error) = app.emit_to(PET_WINDOW_LABEL, PET_SETTINGS_EVENT, payload) {
        log::debug!("[pet/state] settings emit skipped: {}", error);
    }
}

pub fn current_state() -> PetAggregateState {
    let runs = RUNTIME.runs.lock().unwrap_or_else(|e| e.into_inner());
    aggregate_runs(&runs)
}

/// Synchronize settings and the latest aggregate state after the pet window signals readiness.
pub fn sync_to_window(app: &AppHandle) {
    let settings = crate::storage::settings::get_user_settings();
    emit_settings(app, &settings);
    if settings.pet_enabled {
        emit_state_if_changed(app, &current_state(), true);
    }
}

fn expire_run(app: &AppHandle, run_id: String, generation: u64, mode: PetMode) {
    let next = {
        let mut runs = RUNTIME.runs.lock().unwrap_or_else(|e| e.into_inner());
        let matches = runs
            .get(&run_id)
            .is_some_and(|run| run.generation == generation && run.mode == mode);
        if matches {
            runs.remove(&run_id);
            Some(aggregate_runs(&runs))
        } else {
            None
        }
    };

    if let Some(state) = next {
        emit_state_if_changed(app, &state, false);
    }
}

fn apply_run_state(app: &AppHandle, run_id: &str, raw_state: &str) {
    let raw_mode = map_run_state_to_pet_mode(raw_state);
    let mut hold: Option<(u64, PetMode)> = None;
    let aggregate = {
        let mut runs = RUNTIME.runs.lock().unwrap_or_else(|e| e.into_inner());
        let previous = runs.get(run_id).cloned();

        // An idle event closes a turn. Preserve a visible review/failed hold and
        // convert running → review before returning to idle after the hold.
        let mode = if matches!(raw_state, "empty" | "ready" | "idle" | "stopped") {
            match previous.as_ref().map(|run| run.mode) {
                Some(PetMode::Running) => PetMode::Review,
                Some(mode @ (PetMode::Review | PetMode::Failed)) => mode,
                _ => PetMode::Idle,
            }
        } else {
            raw_mode
        };

        if previous.as_ref().is_some_and(|run| run.mode == mode) {
            return;
        }

        if mode == PetMode::Idle {
            runs.remove(run_id);
        } else {
            let generation = RUNTIME.next_generation.fetch_add(1, Ordering::Relaxed) + 1;
            runs.insert(run_id.to_string(), RunPetState { mode, generation });
            if matches!(mode, PetMode::Review | PetMode::Failed) {
                hold = Some((generation, mode));
            }
        }
        aggregate_runs(&runs)
    };

    emit_state_if_changed(app, &aggregate, false);

    if let Some((generation, mode)) = hold {
        let handle = app.clone();
        let run_id = run_id.to_string();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(PET_TRANSITION_HOLD_MS)).await;
            expire_run(&handle, run_id, generation, mode);
        });
    }
}

/// Consume normalized AgentCabin run-state events and update the pet aggregate.
pub fn handle_bus_event(app: &AppHandle, event: &BusEvent) {
    if let BusEvent::RunState { run_id, state, .. } = event {
        apply_run_state(app, run_id, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pet_mode_serialization() {
        let mode = PetMode::Waiting;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"waiting\"");

        let de: PetMode = serde_json::from_str("\"review\"").unwrap();
        assert_eq!(de, PetMode::Review);
    }

    #[test]
    fn test_pet_aggregate_state_serialization() {
        let state = PetAggregateState {
            mode: PetMode::Running,
            running_count: 2,
            error_count: 0,
            active_run_id: Some("run-123".into()),
            timestamp: 1600000000,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"runningCount\":2"));
        assert!(json.contains("\"activeRunId\":\"run-123\""));
    }

    #[test]
    fn maps_normalized_run_states() {
        assert_eq!(map_run_state_to_pet_mode("spawning"), PetMode::Waiting);
        assert_eq!(map_run_state_to_pet_mode("running"), PetMode::Running);
        assert_eq!(map_run_state_to_pet_mode("completed"), PetMode::Review);
        assert_eq!(map_run_state_to_pet_mode("failed"), PetMode::Failed);
        assert_eq!(map_run_state_to_pet_mode("stopped"), PetMode::Idle);
    }

    #[test]
    fn aggregate_priority_is_failed_then_running_then_waiting_then_review() {
        let runs = HashMap::from([
            (
                "review".to_string(),
                RunPetState {
                    mode: PetMode::Review,
                    generation: 1,
                },
            ),
            (
                "waiting".to_string(),
                RunPetState {
                    mode: PetMode::Waiting,
                    generation: 2,
                },
            ),
            (
                "running".to_string(),
                RunPetState {
                    mode: PetMode::Running,
                    generation: 3,
                },
            ),
            (
                "failed".to_string(),
                RunPetState {
                    mode: PetMode::Failed,
                    generation: 4,
                },
            ),
        ]);

        let state = aggregate_runs(&runs);
        assert_eq!(state.mode, PetMode::Failed);
        assert_eq!(state.running_count, 1);
        assert_eq!(state.error_count, 1);
        assert_eq!(state.active_run_id.as_deref(), Some("failed"));
    }

    #[test]
    fn settings_payload_uses_camel_case_and_safe_scale() {
        let settings = UserSettings {
            pet_enabled: true,
            pet_scale: 99.0,
            ..UserSettings::default()
        };
        let json = serde_json::to_value(settings_payload(&settings)).unwrap();
        assert_eq!(json["petEnabled"], true);
        assert_eq!(json["petScale"], 2.0);
        assert_eq!(json["petAlwaysOnTop"], true);
        assert_eq!(json["petId"], "agentcabin-bot");
        assert_eq!(json["petPatrolEnabled"], true);
        assert_eq!(json["petPatrolPauseMin"], 3);
        assert_eq!(json["petSnapToEdge"], false);
        assert_eq!(json["petClickInteractionEnabled"], true);
        assert_eq!(json["petGrokbotColor"], "blue");
        assert_eq!(json["petGrokbotShape"], "blob");
        assert_eq!(json["petGrokbotParts"], serde_json::json!([]));
        assert_eq!(json["petGrokbotAccessories"], serde_json::json!([]));
    }

    #[test]
    fn user_settings_have_safe_pet_defaults() {
        let settings = UserSettings::default();
        assert!(!settings.pet_enabled);
        assert_eq!(settings.pet_scale, 1.0);
        assert!(settings.pet_always_on_top);
        assert_eq!(settings.pet_id, "agentcabin-bot");
        assert!(settings.pet_patrol_enabled);
        assert_eq!(settings.pet_patrol_pause_min, 3);
        assert!(!settings.pet_snap_to_edge);
        assert!(settings.pet_click_interaction_enabled);
        assert_eq!(settings.pet_grokbot_color, "blue");
        assert_eq!(settings.pet_grokbot_shape, "blob");
        assert!(settings.pet_grokbot_parts.is_empty());
        assert!(settings.pet_grokbot_accessories.is_empty());
    }
}
