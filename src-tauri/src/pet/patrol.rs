//! Optional idle free-roam behavior for the desktop pet.
//!
//! Uses a "wander" steering algorithm: the pet moves at a gentle pace and
//! gradually turns in a randomly-drifting direction.  It alternates between
//! walking and natural rest periods, so it does not constantly slide around
//! the desktop.  Near screen edges it steers back toward the centre so it
//! never gets stuck in a corner.

use super::position::{save_position_file, PetPosition};
use super::state::{current_state, PetMode};
use super::window::{monitor_bounds, normalize_pet_patrol_pause_min};
use once_cell::sync::Lazy;
use rand::Rng;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Monitor, PhysicalPosition, Position};

/// Animation tick in milliseconds.
const TICK_MS: u64 = 50;
/// Pixels per second — gentle strolling pace.
const SPEED_PX_PER_SECOND: f64 = 28.0;
/// Minimum and maximum duration of one walking stretch.
const MIN_WALK_SECS: u64 = 20;
const MAX_WALK_SECS: u64 = 55;
/// Short rests make the pet feel alive without making it appear frozen.
const MIN_SHORT_REST_SECS: u64 = 3;
const MAX_SHORT_REST_SECS: u64 = 9;
/// Occasionally take a longer rest; the user's pause setting caps its length.
const LONG_REST_CHANCE: f64 = 0.18;
const MIN_LONG_REST_SECS: u64 = 20;
const MAX_LONG_REST_SECS: u64 = 180;
/// Maximum angle the heading can rotate per second (radians).
/// ~40 °/s gives a lazy, drifting feel.
const MAX_TURN_RAD_PER_SEC: f64 = 0.70;
/// How far from the edge (px) we start steering back toward the centre.
const EDGE_REPEL_PX: f64 = 120.0;
/// Strength of the edge-repulsion steering (radians added per second).
const EDGE_REPEL_STRENGTH: f64 = 1.8;
/// Safety margin so the pet never overlaps the physical screen boundary.
const EDGE_MARGIN_PX: i32 = 8;

static PATROL_GENERATION: AtomicU64 = AtomicU64::new(0);
static PET_DRAGGING: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

#[derive(Clone, Copy)]
enum PatrolPhase {
    Walking { until: Instant },
    Resting { until: Instant },
}

impl PatrolPhase {
    fn is_expired(self, now: Instant) -> bool {
        self.until() <= now
    }

    fn is_resting(self) -> bool {
        matches!(self, Self::Resting { .. })
    }

    fn until(self) -> Instant {
        match self {
            Self::Walking { until } | Self::Resting { until } => until,
        }
    }
}

pub fn set_dragging(dragging: bool) {
    PET_DRAGGING.store(dragging, Ordering::Relaxed);
}

pub fn sync(app: &AppHandle, settings: &crate::models::UserSettings) {
    let generation = PATROL_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    if !settings.pet_enabled || !settings.pet_patrol_enabled {
        return;
    }

    let snap_to_edge = settings.pet_snap_to_edge;
    let pause_min = normalize_pet_patrol_pause_min(settings.pet_patrol_pause_min);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        patrol_loop(handle, generation, snap_to_edge, pause_min).await;
    });
}

async fn patrol_loop(app: AppHandle, generation: u64, snap_to_edge: bool, pause_min: u32) {
    // Wait until idle before we start moving at all.
    while is_current(generation)
        && (PET_DRAGGING.load(Ordering::Relaxed) || current_state().mode != PetMode::Idle)
    {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    if !is_current(generation) {
        return;
    }

    let Some(win) = app.get_webview_window(super::PET_WINDOW_LABEL) else {
        return;
    };

    // Start with a random heading.
    let mut heading: f64 = rand::thread_rng().gen_range(0.0..std::f64::consts::TAU);

    let dt = TICK_MS as f64 / 1000.0;
    let step = SPEED_PX_PER_SECOND * dt;
    let max_turn = MAX_TURN_RAD_PER_SEC * dt;

    // Initialise position from the current window position.
    let Ok(initial_pos) = win.outer_position() else {
        return;
    };
    let mut x = initial_pos.x as f64;
    let mut y = initial_pos.y as f64;
    let mut phase = PatrolPhase::Walking {
        until: Instant::now() + random_walk_duration(),
    };

    loop {
        tokio::time::sleep(Duration::from_millis(TICK_MS)).await;

        if !is_current(generation) {
            return;
        }

        // Pause while being dragged or while busy.
        if PET_DRAGGING.load(Ordering::Relaxed) || current_state().mode != PetMode::Idle {
            // Re-sync position after drag.
            if let Ok(pos) = win.outer_position() {
                x = pos.x as f64;
                y = pos.y as f64;
            }
            // Pick a fresh random heading so we don't immediately retrace our path.
            heading = rand::thread_rng().gen_range(0.0..std::f64::consts::TAU);
            phase = PatrolPhase::Walking {
                until: Instant::now() + random_walk_duration(),
            };
            continue;
        }

        let now = Instant::now();
        if phase.is_expired(now) {
            phase = match phase {
                PatrolPhase::Walking { .. } => PatrolPhase::Resting {
                    until: now + random_rest_duration(pause_min),
                },
                PatrolPhase::Resting { .. } => PatrolPhase::Walking {
                    until: now + random_walk_duration(),
                },
            };
        }

        // Keep checking the session/drag state at the normal tick interval,
        // but leave the window exactly where it is during a rest.
        if phase.is_resting() {
            continue;
        }

        let Some(monitor) = current_monitor(&win) else {
            continue;
        };
        let (mon_x, mon_y, mon_w, mon_h) = monitor_bounds(&monitor);
        let Ok(size) = win.inner_size() else {
            continue;
        };

        let edge_margin = if snap_to_edge { 0 } else { EDGE_MARGIN_PX };
        let left = (mon_x + edge_margin) as f64;
        let right = (mon_x + mon_w as i32 - size.width as i32 - edge_margin) as f64;
        let top = (mon_y + edge_margin) as f64;
        let bottom = (mon_y + mon_h as i32 - size.height as i32 - edge_margin) as f64;

        if right <= left || bottom <= top {
            continue;
        }

        // ── Edge-repulsion steering ──────────────────────────────────────────
        // Compute a "push" angle that points toward the screen centre and blend
        // it in when we get close to any edge.  This keeps the pet wandering
        // across the full screen without hard bouncing.
        let cx = (left + right) / 2.0;
        let cy = (top + bottom) / 2.0;
        let toward_centre = (cy - y).atan2(cx - x);

        // Distance to each edge, normalised to [0, 1] within EDGE_REPEL_PX.
        let dist_l = ((x - left) / EDGE_REPEL_PX).clamp(0.0, 1.0);
        let dist_r = ((right - x) / EDGE_REPEL_PX).clamp(0.0, 1.0);
        let dist_t = ((y - top) / EDGE_REPEL_PX).clamp(0.0, 1.0);
        let dist_b = ((bottom - y) / EDGE_REPEL_PX).clamp(0.0, 1.0);
        // Overall proximity: 1.0 = right on the edge, 0.0 = well away.
        let proximity = (1.0 - dist_l)
            .max(1.0 - dist_r)
            .max(1.0 - dist_t)
            .max(1.0 - dist_b);

        if proximity > 0.0 {
            // Smoothly blend heading toward "toward_centre".
            let diff = angle_diff(toward_centre, heading);
            let correction = (EDGE_REPEL_STRENGTH * dt * proximity).min(diff.abs()) * diff.signum();
            heading += correction;
        }

        // ── Random wander turn ───────────────────────────────────────────────
        // Add a small random nudge to the heading each tick.
        let turn = rand::thread_rng().gen_range(-max_turn..=max_turn);
        heading = (heading + turn).rem_euclid(std::f64::consts::TAU);

        // ── Move ─────────────────────────────────────────────────────────────
        x = (x + heading.cos() * step).clamp(left, right);
        y = (y + heading.sin() * step).clamp(top, bottom);

        let px = x.round() as i32;
        let py = y.round() as i32;
        let _ = win.set_position(Position::Physical(PhysicalPosition::new(px, py)));
        save_position_file(PetPosition { x: px, y: py }).ok();
    }
}

fn random_walk_duration() -> Duration {
    Duration::from_secs(rand::thread_rng().gen_range(MIN_WALK_SECS..=MAX_WALK_SECS))
}

fn random_rest_duration(pause_min: u32) -> Duration {
    let mut rng = rand::thread_rng();
    if rng.gen_bool(LONG_REST_CHANCE) {
        let configured_max = u64::from(pause_min).saturating_mul(60);
        let max_secs = configured_max.clamp(MIN_LONG_REST_SECS, MAX_LONG_REST_SECS);
        Duration::from_secs(rng.gen_range(MIN_LONG_REST_SECS..=max_secs))
    } else {
        Duration::from_secs(rng.gen_range(MIN_SHORT_REST_SECS..=MAX_SHORT_REST_SECS))
    }
}

/// Signed angular difference from `from` to `to` in [-π, π].
fn angle_diff(to: f64, from: f64) -> f64 {
    let diff = (to - from).rem_euclid(std::f64::consts::TAU);
    if diff > std::f64::consts::PI {
        diff - std::f64::consts::TAU
    } else {
        diff
    }
}

fn is_current(generation: u64) -> bool {
    PATROL_GENERATION.load(Ordering::Relaxed) == generation
}

fn current_monitor(win: &tauri::WebviewWindow) -> Option<Monitor> {
    win.current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten())
        .or_else(|| win.available_monitors().ok().and_then(|mut all| all.pop()))
}
