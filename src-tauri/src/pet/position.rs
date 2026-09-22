use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PetPosition {
    pub x: i32,
    pub y: i32,
}

/// Snap a pet to any nearby edge of the monitor work area.
///
/// Window positions are physical pixels, so the threshold is also expressed in
/// physical pixels. Keeping this as a pure function makes edge snapping safe to
/// test without creating a native window.
#[allow(clippy::too_many_arguments)]
pub fn snap_position_to_edges(
    x: i32,
    y: i32,
    win_w: u32,
    win_h: u32,
    mon_x: i32,
    mon_y: i32,
    mon_w: u32,
    mon_h: u32,
    threshold: i32,
) -> (i32, i32) {
    let max_x = if mon_w as i32 > win_w as i32 {
        mon_x + mon_w as i32 - win_w as i32
    } else {
        mon_x
    };
    let max_y = if mon_h as i32 > win_h as i32 {
        mon_y + mon_h as i32 - win_h as i32
    } else {
        mon_y
    };
    let threshold = i64::from(threshold.max(0));

    let snap_axis = |value: i32, min: i32, max: i32| {
        let clamped = value.clamp(min, max);
        let distance_to_min = (i64::from(clamped) - i64::from(min)).abs();
        let distance_to_max = (i64::from(max) - i64::from(clamped)).abs();
        if distance_to_min <= threshold {
            min
        } else if distance_to_max <= threshold {
            max
        } else {
            clamped
        }
    };

    (snap_axis(x, mon_x, max_x), snap_axis(y, mon_y, max_y))
}

fn position_file_path() -> PathBuf {
    crate::storage::data_dir().join("pet_position.json")
}

#[allow(clippy::too_many_arguments)]
pub fn clamp_position_to_monitor(
    x: i32,
    y: i32,
    win_w: u32,
    win_h: u32,
    mon_x: i32,
    mon_y: i32,
    mon_w: u32,
    mon_h: u32,
) -> (i32, i32) {
    let win_w_i32 = win_w as i32;
    let win_h_i32 = win_h as i32;

    let min_x = mon_x;
    let max_x = if mon_w as i32 > win_w_i32 {
        mon_x + mon_w as i32 - win_w_i32
    } else {
        mon_x
    };

    let min_y = mon_y;
    let max_y = if mon_h as i32 > win_h_i32 {
        mon_y + mon_h as i32 - win_h_i32
    } else {
        mon_y
    };

    let clamped_x = x.clamp(min_x, max_x);
    let clamped_y = y.clamp(min_y, max_y);

    (clamped_x, clamped_y)
}

pub fn load_position_file() -> Option<PetPosition> {
    let path = position_file_path();
    if !path.exists() {
        return None;
    }
    match fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(pos) => Some(pos),
            Err(e) => {
                log::warn!("[pet/position] Failed to parse pet_position.json: {}", e);
                None
            }
        },
        Err(e) => {
            log::warn!("[pet/position] Failed to read pet_position.json: {}", e);
            None
        }
    }
}

pub fn save_position_file(pos: PetPosition) -> Result<(), String> {
    let path = position_file_path();
    if let Some(parent) = path.parent() {
        let _ = crate::storage::ensure_dir(parent);
    }
    let json = serde_json::to_string_pretty(&pos)
        .map_err(|e| format!("Failed to serialize pet position: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write pet position file: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_inside_monitor() {
        let (cx, cy) = clamp_position_to_monitor(500, 500, 160, 176, 0, 0, 1920, 1080);
        assert_eq!(cx, 500);
        assert_eq!(cy, 500);
    }

    #[test]
    fn test_clamp_outside_left_top() {
        let (cx, cy) = clamp_position_to_monitor(-50, -100, 160, 176, 0, 0, 1920, 1080);
        assert_eq!(cx, 0);
        assert_eq!(cy, 0);
    }

    #[test]
    fn test_clamp_outside_right_bottom() {
        let (cx, cy) = clamp_position_to_monitor(2000, 1200, 160, 176, 0, 0, 1920, 1080);
        assert_eq!(cx, 1920 - 160);
        assert_eq!(cy, 1080 - 176);
    }

    #[test]
    fn test_clamp_secondary_monitor() {
        // Secondary monitor starting at x=1920, y=0, width 1920, height 1080
        let (cx, cy) = clamp_position_to_monitor(3900, 1100, 160, 176, 1920, 0, 1920, 1080);
        assert_eq!(cx, 1920 + 1920 - 160);
        assert_eq!(cy, 1080 - 176);
    }

    #[test]
    fn test_snap_near_left_and_bottom_edges() {
        let (x, y) = snap_position_to_edges(12, 895, 160, 176, 0, 0, 1920, 1080, 24);
        assert_eq!(x, 0);
        assert_eq!(y, 1080 - 176);
    }

    #[test]
    fn test_snap_keeps_position_away_from_edges() {
        let (x, y) = snap_position_to_edges(500, 500, 160, 176, 0, 0, 1920, 1080, 24);
        assert_eq!((x, y), (500, 500));
    }
}
