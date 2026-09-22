use crate::models::{now_iso, BusEvent, ModelUsageSummary, RawRunUsage, RunEvent, RunEventType};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};

/// Event types the frontend reducer actually handles during replay.
/// "raw" events (CLI stream data) are 90%+ of the file but the frontend drops them,
/// so filtering here avoids serializing megabytes of unused data across IPC.
pub const REPLAY_TYPES: &[&str] = &[
    "session_init",
    "message_delta",
    "thinking_delta",
    "tool_input_delta",
    "message_complete",
    "user_message",
    "tool_start",
    "tool_end",
    "structured_task_state",
    "work_task_state",
    "run_state",
    "usage_update",
    "permission_denied",
    "permission_prompt",
    "compact_boundary",
    "agent_handoff",
    "system_status",
    "pi_context_usage",
    "auth_status",
    "hook_started",
    "hook_response",
    "control_cancelled",
    "task_notification",
    "tool_progress",
    "tool_use_summary",
    "command_output",
    "files_persisted",
    "hook_progress",
    "hook_callback",
    "elicitation_prompt",
    "pi_extension_ui",
    "rate_limit_event",
    "codex_hook_run",
    "turn_file_summary",
    "work_context_plan_updated",
];

/// Textual markers for the compact and pretty-printed JSON forms accepted by
/// the reader. Build them once; constructing these strings inside the file
/// loop was disproportionately expensive for large raw-event logs.
static REPLAY_TYPE_MARKERS: Lazy<Vec<String>> = Lazy::new(|| {
    REPLAY_TYPES
        .iter()
        .flat_map(|tag| {
            [
                format!("\"type\":\"{tag}\""),
                format!("\"type\": \"{tag}\""),
            ]
        })
        .collect()
});

/// Check if a BusEvent's serde tag is in REPLAY_TYPES.
pub fn is_replayable(event: &BusEvent) -> bool {
    let Ok(v) = serde_json::to_value(event) else {
        return false;
    };
    is_replayable_value(&v)
}

/// Check replayability for an already-serialized bus event.
///
/// The broadcaster already serializes every live event for transport. Reusing that
/// value avoids serializing large Pi session-entry snapshots a second time merely
/// to decide whether they belong in the durable replay log.
pub fn is_replayable_value(value: &serde_json::Value) -> bool {
    value
        .get("type")
        .and_then(|tag| tag.as_str())
        .is_some_and(|tag| REPLAY_TYPES.contains(&tag))
}

fn events_path(run_id: &str) -> std::path::PathBuf {
    super::run_dir(run_id).join("events.jsonl")
}

pub fn next_seq(run_id: &str) -> u64 {
    let path = events_path(run_id);
    let file_len = match fs::metadata(&path) {
        Ok(m) => m.len(),
        Err(_) => return 1,
    };
    if file_len == 0 {
        return 1;
    }

    // Fast path: scan only the last 4 KiB — recent (highest) seqs are at the end.
    if let Some(max) = max_seq_in_tail(&path, file_len) {
        return max + 1;
    }

    // Fallback: the tail window held no parseable seq line — e.g. the last event
    // line is itself larger than 4 KiB, so after dropping the partial first line
    // nothing parses. Seeding 1 here would collide with existing seqs, so do a
    // full scan to seed correctly. (audit #7: oversized-line seed reset)
    if let Ok(content) = fs::read_to_string(&path) {
        if let Some(max) = scan_max_seq(&content) {
            return max + 1;
        }
    }
    1
}

/// Max `seq` over a JSONL string's parseable lines (None if none parse).
fn scan_max_seq(content: &str) -> Option<u64> {
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v.get("seq").and_then(|s| s.as_u64()))
        .max()
}

/// Max `seq` from the last 4 KiB of `path`. Returns None when the window contains
/// no complete line (too small to hold the final event), signalling the caller to
/// fall back to a full scan instead of trusting a bogus 0 seed.
fn max_seq_in_tail(path: &std::path::Path, file_len: u64) -> Option<u64> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    if file_len > 4096 {
        reader.seek(SeekFrom::End(-4096)).ok()?;
    }
    // read_to_end + from_utf8_lossy tolerates a mid-character seek.
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).ok()?;
    let tail = String::from_utf8_lossy(&buf);
    // Drop the first (partial) line when we seeked into the middle. If there is no
    // newline at all, the whole window is one partial line → "" → None (full scan).
    let lines_str = if file_len > 4096 {
        tail.split_once('\n').map(|(_, rest)| rest).unwrap_or("")
    } else {
        &tail
    };
    scan_max_seq(lines_str)
}

/// Append a raw run-event (stdout/stderr/etc.) to events.jsonl.
///
/// Delegates to the process-wide [`EventWriter`] singleton so that seq allocation
/// and the file write happen under the SAME per-run lock as bus events. Previously
/// this computed seq via an unlocked file read, so concurrent writers (e.g. Codex
/// stdout + stderr tasks, or a bus-event write interleaving) could collide on seq
/// or interleave partial lines. (audit #1: append_event seq race)
pub fn append_event(
    run_id: &str,
    event_type: RunEventType,
    payload: serde_json::Value,
) -> Result<RunEvent, String> {
    log::trace!(
        "[storage/events] append_event: run_id={}, type={:?}",
        run_id,
        event_type
    );
    EVENT_WRITER.write_run_event(run_id, event_type, payload)
}

pub fn list_events(run_id: &str, since_seq: u64) -> Vec<RunEvent> {
    let path = events_path(run_id);
    if !path.exists() {
        return vec![];
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<RunEvent>(l).ok())
        .filter(|e| e.seq > since_seq)
        .collect()
}

// ── Bus event persistence ──

use std::sync::{Arc, Mutex};

/// Atomic seq allocation + file write under per-run locks.
/// Each run_id gets its own Mutex so different runs never block each other.
/// The outer Mutex is only held briefly to get/create the per-run Arc.
pub struct EventWriter {
    inner: Mutex<HashMap<String, Arc<Mutex<u64>>>>, // run_id → Arc<Mutex<next_seq>>
}

impl Default for EventWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl EventWriter {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Atomically assign seq + write to events.jsonl (both under the same per-run lock).
    /// Returns `Err` if any step fails (dir creation, serialization, file I/O).
    pub fn write_bus_event(&self, run_id: &str, event: &BusEvent) -> Result<(), String> {
        log::trace!("[storage/events] write_bus_event: run_id={}", run_id);

        // Get or create the per-run lock (brief global lock, then release)
        let run_lock = {
            let mut map = self.inner.lock().unwrap();
            // GC: drop entries whose per-run Arc has no other holders (session ended)
            if map.len() > 50 {
                map.retain(|_, v| Arc::strong_count(v) > 1);
            }
            map.entry(run_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(next_seq(run_id))))
                .clone()
        };
        // Global lock released here — other runs proceed in parallel

        // Per-run lock: seq allocation + file write are atomic
        let mut seq_guard = run_lock.lock().unwrap();
        let current = *seq_guard;
        *seq_guard = current + 1;

        let dir = super::run_dir(run_id);
        super::ensure_dir(&dir).map_err(|e| format!("ensure_dir failed: {}", e))?;

        let envelope = serde_json::json!({
            "_bus": true,
            "seq": current,
            "ts": now_iso(),
            "event": event,
        });
        let path = events_path(run_id);
        let line =
            serde_json::to_string(&envelope).map_err(|e| format!("serialize failed: {}", e))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("open {} failed: {}", path.display(), e))?;
        writeln!(file, "{}", line)
            .map_err(|e| format!("write to {} failed: {}", path.display(), e))?;
        invalidate_bus_events_cache(run_id);

        Ok(())
    }

    /// Like `write_bus_event` but uses a caller-supplied timestamp and returns the assigned seq.
    pub fn write_bus_event_with_ts(
        &self,
        run_id: &str,
        event: &BusEvent,
        ts: &str,
    ) -> Result<u64, String> {
        log::trace!(
            "[storage/events] write_bus_event_with_ts: run_id={}, ts={}",
            run_id,
            ts
        );

        let run_lock = {
            let mut map = self.inner.lock().unwrap();
            if map.len() > 50 {
                map.retain(|_, v| Arc::strong_count(v) > 1);
            }
            map.entry(run_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(next_seq(run_id))))
                .clone()
        };

        let mut seq_guard = run_lock.lock().unwrap();
        let current = *seq_guard;
        *seq_guard = current + 1;

        let dir = super::run_dir(run_id);
        super::ensure_dir(&dir).map_err(|e| format!("ensure_dir failed: {}", e))?;

        let envelope = serde_json::json!({
            "_bus": true,
            "seq": current,
            "ts": ts,
            "event": event,
        });
        let path = events_path(run_id);
        let line =
            serde_json::to_string(&envelope).map_err(|e| format!("serialize failed: {}", e))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("open {} failed: {}", path.display(), e))?;
        writeln!(file, "{}", line)
            .map_err(|e| format!("write to {} failed: {}", path.display(), e))?;
        invalidate_bus_events_cache(run_id);

        Ok(current)
    }

    /// Atomically assign seq + append a raw [`RunEvent`] (stdout/stderr/etc.) under
    /// the same per-run lock and seq counter as bus events, so the two write paths
    /// can't collide on seq or interleave partial lines into events.jsonl.
    pub fn write_run_event(
        &self,
        run_id: &str,
        event_type: RunEventType,
        payload: serde_json::Value,
    ) -> Result<RunEvent, String> {
        let run_lock = {
            let mut map = self.inner.lock().unwrap();
            if map.len() > 50 {
                map.retain(|_, v| Arc::strong_count(v) > 1);
            }
            map.entry(run_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(next_seq(run_id))))
                .clone()
        };

        let mut seq_guard = run_lock.lock().unwrap();
        let current = *seq_guard;
        *seq_guard = current + 1;

        let dir = super::run_dir(run_id);
        super::ensure_dir(&dir).map_err(|e| e.to_string())?;

        let event = RunEvent {
            id: uuid::Uuid::new_v4().to_string()[..12].to_string(),
            task_id: run_id.to_string(),
            seq: current,
            event_type,
            payload,
            timestamp: now_iso(),
        };
        let path = events_path(run_id);
        let line = serde_json::to_string(&event).map_err(|e| e.to_string())?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        writeln!(file, "{}", line).map_err(|e| e.to_string())?;

        Ok(event)
    }
}

/// Process-wide singleton EventWriter. Both bus events and raw run-events (via
/// `append_event`) write through this instance so all writes to a given run's
/// events.jsonl share one per-run lock + one monotonic seq source.
static EVENT_WRITER: Lazy<Arc<EventWriter>> = Lazy::new(|| Arc::new(EventWriter::new()));

/// Returns the process-wide [`EventWriter`] singleton. Register this as the Tauri
/// managed state so command handlers and `append_event` share the same locks/seq.
pub fn global_writer() -> Arc<EventWriter> {
    EVENT_WRITER.clone()
}

/// Thin wrapper for backward compatibility — delegates to EventWriter.
/// Returns `Err` if persistence failed.
pub fn persist_bus_event(
    writer: &EventWriter,
    run_id: &str,
    event: &BusEvent,
) -> Result<(), String> {
    writer.write_bus_event(run_id, event)
}

/// Copy content bus events from one run's events.jsonl to another.
/// Used by fork to preserve conversation history in the new run.
/// Lifecycle events (session_init, run_state, usage_update, permission_denied, raw)
/// are excluded — they belong to the parent session, not the fork.
/// Copied events get their `run_id` rewritten to `to_run_id` and `seq` renumbered
/// from 1 so the fork run's events.jsonl is fully self-consistent.
pub fn copy_bus_events(from_run_id: &str, to_run_id: &str) -> Result<(), String> {
    copy_bus_events_until(from_run_id, to_run_id, None)
}

/// Copy content events through an optional visible timeline anchor.
///
/// A continuation branch uses this instead of copying the whole source log. The anchor is
/// matched against the stable IDs rendered by the frontend (`message_id`, `uuid`, or
/// `tool_use_id`), and the matching event is included before copying stops.
pub fn copy_bus_events_until(
    from_run_id: &str,
    to_run_id: &str,
    anchor_id: Option<&str>,
) -> Result<(), String> {
    let src = events_path(from_run_id);
    if !src.exists() {
        log::debug!(
            "[storage/events] copy_bus_events: source {} has no events",
            from_run_id
        );
        return if anchor_id.is_some() {
            Err(format!(
                "Source run {} has no history for continuation",
                from_run_id
            ))
        } else {
            Ok(())
        };
    }
    let dst_dir = super::run_dir(to_run_id);
    super::ensure_dir(&dst_dir).map_err(|e| format!("ensure_dir failed: {}", e))?;
    let dst = events_path(to_run_id);

    let content =
        fs::read_to_string(&src).map_err(|e| format!("read source events failed: {}", e))?;

    let out = rewrite_content_events(&content, to_run_id, anchor_id)?;
    fs::write(&dst, &out).map_err(|e| format!("write fork events failed: {}", e))?;
    invalidate_bus_events_cache(to_run_id);
    log::debug!(
        "[storage/events] copy_bus_events: {} → {} (anchor={:?})",
        from_run_id,
        to_run_id,
        anchor_id
    );
    Ok(())
}

/// Rewrite a source event log into the content-only event log used by a fork.
/// Kept separate from filesystem access so the continuation boundary is regression-testable.
fn rewrite_content_events(
    content: &str,
    to_run_id: &str,
    anchor_id: Option<&str>,
) -> Result<String, String> {
    // Content event types to copy (conversation history).
    const CONTENT_TYPES: &[&str] = &[
        "message_delta",
        "message_complete",
        "tool_start",
        "tool_end",
        "user_message",
        "turn_file_summary",
    ];

    let mut out = String::new();
    let mut copied = 0u64;
    let mut skipped = 0u64;
    let mut anchor_found = anchor_id.is_none();
    // structured_task_state is a full-replacement snapshot; only the last one before the
    // anchor matters. Track it separately and append after all content events so the fork
    // starts with the correct final task state without replaying intermediate snapshots.
    let mut last_task_snapshot: Option<serde_json::Value> = None;
    let mut last_work_task_snapshot: Option<serde_json::Value> = None;

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(mut envelope) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        // Only process bus events
        if envelope.get("_bus").and_then(|b| b.as_bool()) != Some(true) {
            continue;
        }

        // Check inner event type
        let event_type = envelope
            .get("event")
            .and_then(|e| e.get("type"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        if CONTENT_TYPES.contains(&event_type.as_str()) {
            let event = envelope.get("event").unwrap_or(&serde_json::Value::Null);
            let is_anchor = anchor_id.is_some_and(|anchor| match event_type.as_str() {
                "user_message" => {
                    event
                        .get("uuid")
                        .and_then(|value| value.as_str())
                        .or_else(|| event.get("client_uuid").and_then(|value| value.as_str()))
                        == Some(anchor)
                }
                "message_complete" => {
                    event.get("message_id").and_then(|value| value.as_str()) == Some(anchor)
                }
                "tool_start" | "tool_end" => {
                    event.get("tool_use_id").and_then(|value| value.as_str()) == Some(anchor)
                }
                _ => false,
            });

            // Rewrite run_id in inner event to the fork run
            if let Some(event) = envelope.get_mut("event").and_then(|e| e.as_object_mut()) {
                event.insert(
                    "run_id".to_string(),
                    serde_json::Value::String(to_run_id.to_string()),
                );
            }
            // Renumber seq sequentially
            copied += 1;
            envelope["seq"] = serde_json::Value::Number(copied.into());

            let serialized =
                serde_json::to_string(&envelope).map_err(|e| format!("serialize failed: {}", e))?;
            out.push_str(&serialized);
            out.push('\n');

            if is_anchor {
                anchor_found = true;
                break;
            }
        } else if event_type == "structured_task_state" {
            // Rewrite run_id to the fork run; defer append until after content events.
            if let Some(event) = envelope.get_mut("event").and_then(|e| e.as_object_mut()) {
                event.insert(
                    "run_id".to_string(),
                    serde_json::Value::String(to_run_id.to_string()),
                );
            }
            last_task_snapshot = Some(envelope);
        } else if event_type == "work_task_state" {
            if let Some(event) = envelope.get_mut("event").and_then(|e| e.as_object_mut()) {
                event.insert(
                    "run_id".to_string(),
                    serde_json::Value::String(to_run_id.to_string()),
                );
            }
            last_work_task_snapshot = Some(envelope);
        } else {
            skipped += 1;
        }
    }

    if !anchor_found {
        return Err(format!(
            "Continuation anchor {:?} was not found in source history",
            anchor_id.unwrap_or_default()
        ));
    }

    // Append the last known Work state before its agent-neutral task projection.
    if let Some(mut snapshot) = last_work_task_snapshot {
        copied += 1;
        snapshot["seq"] = serde_json::Value::Number(copied.into());
        let serialized =
            serde_json::to_string(&snapshot).map_err(|e| format!("serialize failed: {}", e))?;
        out.push_str(&serialized);
        out.push('\n');
    }

    // Append the last known task snapshot so the fork inherits the final authoritative state.
    if let Some(mut snapshot) = last_task_snapshot {
        copied += 1;
        snapshot["seq"] = serde_json::Value::Number(copied.into());
        let serialized =
            serde_json::to_string(&snapshot).map_err(|e| format!("serialize failed: {}", e))?;
        out.push_str(&serialized);
        out.push('\n');
    }

    log::debug!(
        "[storage/events] rewrite fork events for {} (copied {} content events, skipped {} lifecycle, new max_seq={})",
        to_run_id, copied, skipped, copied
    );
    Ok(out)
}

/// Extract aggregated usage from bus-events for a single run.
///
/// Three modes:
/// - CLI imports (source=cli_import): per-turn cost+tokens, sum all
/// - Codex (agent=codex): per-turn tokens, sum all; cost estimated in stats.rs
/// - Claude native sessions: cumulative cost (peak-detect), cumulative tokens (take-last)
pub fn extract_run_usage(run_id: &str) -> Option<RawRunUsage> {
    let path = events_path(run_id);
    if !path.exists() {
        return None;
    }

    // Run-scoped detection: parse meta.json once for source + agent
    let (is_per_turn_cost, is_codex) = {
        let meta_path = super::run_dir(run_id).join("meta.json");
        let meta_val = meta_path
            .exists()
            .then(|| {
                fs::read_to_string(&meta_path)
                    .ok()
                    .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            })
            .flatten();
        let source = meta_val
            .as_ref()
            .and_then(|v| v.get("source").and_then(|s| s.as_str()).map(String::from));
        let agent = meta_val
            .as_ref()
            .and_then(|v| v.get("agent").and_then(|s| s.as_str()).map(String::from));
        (
            source == Some("cli_import".to_string()),
            agent == Some("codex".to_string()),
        )
    };
    // Codex turn.completed.usage is per-turn (same as CLI imports)
    let sum_usage = is_per_turn_cost || is_codex;

    let content = fs::read_to_string(&path).ok()?;

    let mut total_cost: f64 = 0.0;
    let mut prev_cost: f64 = 0.0;
    let mut peak_cost: f64 = 0.0;
    let mut total_duration_ms: u64 = 0;
    let mut found_any = false;

    // "Simpler v1": take values from the last usage_update event
    let mut last_input: u64 = 0;
    let mut last_output: u64 = 0;
    let mut last_cache_read: u64 = 0;
    let mut last_cache_write: u64 = 0;
    let mut last_num_turns: u64 = 0;
    let mut last_model_usage: HashMap<String, ModelUsageSummary> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Cheap pre-filter: skip ~99.6% of lines without JSON parsing
        if !line.contains("\"usage_update\"") {
            continue;
        }

        let Ok(envelope) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if envelope.get("_bus").and_then(|b| b.as_bool()) != Some(true) {
            continue;
        }
        let Some(event) = envelope.get("event") else {
            continue;
        };
        let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if event_type != "usage_update" {
            continue;
        }

        found_any = true;
        let cost = event
            .get("total_cost_usd")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        if sum_usage {
            // CLI imports + Codex: per-turn cost, sum directly
            total_cost += cost;
        } else {
            // Native Claude session: cumulative cost, peak-detect
            if cost < prev_cost * 0.9 && prev_cost > 0.0 {
                total_cost += peak_cost;
                peak_cost = 0.0;
            }
            if cost > peak_cost {
                peak_cost = cost;
            }
            prev_cost = cost;
        }

        // Tokens: for per-turn (CLI imports + Codex), sum; for cumulative, take last
        if sum_usage {
            last_input += event
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            last_output += event
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            last_cache_read += event
                .get("cache_read_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            last_cache_write += event
                .get("cache_write_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
        } else {
            last_input = event
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(last_input);
            last_output = event
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(last_output);
            last_cache_read = event
                .get("cache_read_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(last_cache_read);
            last_cache_write = event
                .get("cache_write_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(last_cache_write);
        }

        // num_turns: Claude sends num_turns, Codex sends turn_index (1-based)
        let event_num_turns = event.get("num_turns").and_then(|v| v.as_u64());
        let event_turn_index = event.get("turn_index").and_then(|v| v.as_u64());
        if let Some(nt) = event_num_turns {
            last_num_turns = nt;
        } else if let Some(ti) = event_turn_index {
            // Codex: turn_index is 1-based counter, use as num_turns
            if ti > last_num_turns {
                last_num_turns = ti;
            }
        }

        // Sum duration_ms across turns (per-turn value, not cumulative)
        if let Some(d) = event.get("duration_ms").and_then(|v| v.as_u64()) {
            total_duration_ms += d;
        }

        // Take last model_usage map
        if let Some(mu) = event.get("model_usage").and_then(|v| v.as_object()) {
            last_model_usage.clear();
            for (model, entry) in mu {
                last_model_usage.insert(
                    model.clone(),
                    ModelUsageSummary {
                        input_tokens: entry
                            .get("input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                        output_tokens: entry
                            .get("output_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                        cache_read_tokens: entry
                            .get("cache_read_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                        cache_write_tokens: entry
                            .get("cache_write_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                        cost_usd: entry
                            .get("cost_usd")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                    },
                );
            }
        }
    }

    if !found_any {
        return None;
    }

    // Add final segment's peak cost (only for cumulative mode)
    if !sum_usage {
        total_cost += peak_cost;
    }

    log::debug!(
        "[storage/events] extract_run_usage: run_id={}, cost={:.6}, tokens={}+{}, turns={}, models={}",
        run_id,
        total_cost,
        last_input,
        last_output,
        last_num_turns,
        last_model_usage.len()
    );

    Some(RawRunUsage {
        total_cost_usd: total_cost,
        input_tokens: last_input,
        output_tokens: last_output,
        cache_read_tokens: last_cache_read,
        cache_write_tokens: last_cache_write,
        duration_ms: total_duration_ms,
        num_turns: last_num_turns,
        model_usage: last_model_usage,
    })
}

/// Count user_message events in events.jsonl for resume baseline.
/// Returns (total_user_messages, normal_user_messages).
///
/// Compat: handles both wrapped `{"event": {"type": "user_message", ...}, ...}`
/// and direct `{"type": "user_message", ...}` JSONL formats.
/// Unparseable lines are skipped (debug-level count logged).
pub fn count_user_messages(run_id: &str) -> (u32, u32) {
    let path = events_path(run_id);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return (0, 0),
    };

    let mut total: u32 = 0;
    let mut normal: u32 = 0;
    let mut skipped: u32 = 0;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Fast pre-filter: skip lines that can't contain user_message
        if !line.contains("\"user_message\"") {
            continue;
        }
        let parsed = match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => v,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        // Compat: wrapped format takes .event, direct format takes self
        let event = parsed.get("event").unwrap_or(&parsed);
        let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if event_type == "user_message" {
            total += 1;
            let text = event.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if !text.trim_start().starts_with('/') {
                normal += 1;
            }
        }
    }

    if skipped > 0 {
        log::debug!(
            "[events] count_user_messages: skipped {} unparseable lines",
            skipped
        );
    }

    (total, normal)
}

/// In-memory cache for complete bus-event reads.
/// Avoids re-reading and re-parsing large events.jsonl files on repeated opens.
static BUS_EVENTS_CACHE: Lazy<Mutex<HashMap<String, Vec<serde_json::Value>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn list_bus_events(run_id: &str, since_seq: Option<u64>) -> Vec<serde_json::Value> {
    let t0 = std::time::Instant::now();
    log::debug!(
        "[storage/events] list_bus_events: run_id={}, since_seq={:?}",
        run_id,
        since_seq
    );

    // Check in-memory cache first (only for full reads without since_seq)
    if since_seq.is_none() || since_seq == Some(0) {
        if let Ok(cache) = BUS_EVENTS_CACHE.lock() {
            if let Some(cached) = cache.get(run_id) {
                log::debug!(
                    "[storage/events] list_bus_events: cache hit, {} events in {:?}",
                    cached.len(),
                    t0.elapsed()
                );
                return cached.clone();
            }
        }
    }

    let path = events_path(run_id);
    if !path.exists() {
        return vec![];
    }

    let result = parse_bus_events_streamed(&path, since_seq);

    log::debug!(
        "[storage/events] list_bus_events: parsed {} events in {:?}",
        result.len(),
        t0.elapsed()
    );

    // Cache the result for full reads (terminal sessions benefit from this)
    if since_seq.is_none() || since_seq == Some(0) {
        if let Ok(mut cache) = BUS_EVENTS_CACHE.lock() {
            // Limit cache size to prevent unbounded memory growth
            if cache.len() >= 50 {
                // Evict all — simple strategy, could be LRU if needed
                cache.clear();
            }
            cache.insert(run_id.to_string(), result.clone());
        }
    }

    result
}

/// Invalidate the bus events cache for a run (e.g., after new events are written).
pub fn invalidate_bus_events_cache(run_id: &str) {
    if let Ok(mut cache) = BUS_EVENTS_CACHE.lock() {
        cache.remove(run_id);
    }
}

/// Parse only durable, replayable bus events from an events file.
///
/// A run file can also contain raw stdout/stderr records and transient Pi state
/// snapshots. Those records are intentionally not part of the replay stream. The
/// cheap textual guards are important: older Pi runs may contain hundreds of MB
/// of raw RPC output, and parsing every discarded line made opening a short chat
/// take several seconds.
///
/// Uses BufReader for streaming line-by-line reads to avoid loading the entire
/// file into memory. The line buffer is reused between records so large raw
/// records do not create a fresh String on every iteration.
fn parse_bus_events_streamed(
    path: &std::path::Path,
    since_seq: Option<u64>,
) -> Vec<serde_json::Value> {
    use std::io::BufRead;

    let min_seq = since_seq.unwrap_or(0);

    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return vec![],
    };
    let mut reader = std::io::BufReader::with_capacity(256 * 1024, file); // 256KB buffer for large files

    let mut result = Vec::new();
    let mut line = String::with_capacity(16 * 1024);
    let mut last_session_init: Option<serde_json::Value> = None;

    loop {
        line.clear();
        let bytes_read = match reader.read_line(&mut line) {
            Ok(bytes_read) => bytes_read,
            Err(_) => break,
        };
        if bytes_read == 0 {
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // EventWriter emits compact JSON with this exact envelope marker
        if !line.contains("\"_bus\":true") && !line.contains("\"_bus\": true") {
            continue;
        }

        // Skip old raw bus records before allocating a JSON value. A nested
        // payload may contain a replay marker and will be parsed conservatively;
        // the typed event check below remains authoritative.
        if !REPLAY_TYPE_MARKERS
            .iter()
            .any(|marker| line.contains(marker.as_str()))
        {
            continue;
        }

        // Parse JSON
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Extract bus event
        if let Some(true) = v.get("_bus").and_then(|b| b.as_bool()) {
            if let Some(seq) = v.get("seq").and_then(|s| s.as_u64()) {
                if seq > min_seq {
                    let Some(event) = v.get("event") else {
                        continue;
                    };
                    let Some(etype) = event.get("type").and_then(|t| t.as_str()) else {
                        continue;
                    };
                    if !REPLAY_TYPES.contains(&etype) {
                        continue;
                    }

                    // Pi can emit the same full session snapshot after every state
                    // refresh. It is a state update, not a timeline event; returning
                    // thousands of identical 10–15 KB snapshots makes IPC dominate
                    // session switching. Keep state changes, drop exact repeats.
                    if etype == "session_init"
                        && last_session_init
                            .as_ref()
                            .is_some_and(|previous| previous == event)
                    {
                        continue;
                    }

                    let mut event = event.clone();
                    if etype == "session_init" {
                        last_session_init = Some(event.clone());
                    }
                    if let Some(obj) = event.as_object_mut() {
                        // Inject envelope timestamp
                        if let Some(ts) = v.get("ts") {
                            obj.insert("ts".to_string(), ts.clone());
                        }
                        // Inject _seq for WS subscribe checkpoint
                        obj.insert("_seq".to_string(), serde_json::Value::Number(seq.into()));
                    }
                    result.push(event);
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::{
        is_replayable_value, max_seq_in_tail, parse_bus_events_streamed, rewrite_content_events,
        scan_max_seq,
    };
    use std::io::Write as _;

    #[test]
    fn scan_max_seq_picks_highest_and_ignores_junk() {
        assert_eq!(
            scan_max_seq("{\"seq\":1}\n{\"seq\":5}\n{\"seq\":3}\n"),
            Some(5)
        );
        assert_eq!(scan_max_seq(""), None);
        assert_eq!(scan_max_seq("not json\n\n"), None);
    }

    #[test]
    fn max_seq_in_tail_small_file_reads_directly() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "{}", serde_json::json!({"seq": 7})).unwrap();
        f.flush().unwrap();
        let len = f.as_file().metadata().unwrap().len();
        assert_eq!(max_seq_in_tail(f.path(), len), Some(7));
    }

    #[test]
    fn max_seq_in_tail_returns_none_when_last_line_exceeds_window() {
        // audit #7: a final event line larger than the 4 KiB tail window leaves no
        // newline in the window, so the tail scan must report None (not a bogus 0)
        // to let next_seq fall back to a full scan instead of reseeding seq to 1.
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "{}", serde_json::json!({"seq": 1})).unwrap();
        let big = "x".repeat(8192);
        writeln!(f, "{}", serde_json::json!({"seq": 2, "blob": big})).unwrap();
        f.flush().unwrap();
        let len = f.as_file().metadata().unwrap().len();
        assert!(len > 4096);
        assert_eq!(max_seq_in_tail(f.path(), len), None);
        // The full-scan fallback path still recovers the true max.
        let content = std::fs::read_to_string(f.path()).unwrap();
        assert_eq!(scan_max_seq(&content), Some(2));
    }

    #[test]
    fn parse_bus_events_skips_raw_and_transient_pi_records() {
        let raw_stdout = serde_json::json!({
            "id": "raw-1",
            "task_id": "run-1",
            "seq": 1,
            "type": "stdout",
            "payload": { "text": "{\"type\":\"message_delta\"}" }
        });
        let transient_entries = serde_json::json!({
            "_bus": true,
            "seq": 2,
            "ts": "2026-01-01T00:00:00Z",
            "event": {
                "type": "pi_session_entries",
                "run_id": "run-1",
                "entries": [{ "id": "entry-1" }]
            }
        });
        let user_message = serde_json::json!({
            "_bus": true,
            "seq": 3,
            "ts": "2026-01-01T00:00:01Z",
            "event": {
                "type": "user_message",
                "run_id": "run-1",
                "text": "hello"
            }
        });

        let content = format!("{}\n{}\n{}\n", raw_stdout, transient_entries, user_message);
        // Write to a unique temp file since parse_bus_events_streamed takes a path.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &content).unwrap();
        let events = parse_bus_events_streamed(tmp.path(), None);

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].get("type").and_then(|v| v.as_str()),
            Some("user_message")
        );
        assert_eq!(events[0].get("_seq").and_then(|v| v.as_u64()), Some(3));
    }

    #[test]
    fn only_replayable_values_are_durable() {
        assert!(is_replayable_value(&serde_json::json!({
            "type": "message_delta"
        })));
        assert!(is_replayable_value(&serde_json::json!({
            "type": "turn_file_summary"
        })));
        assert!(!is_replayable_value(&serde_json::json!({
            "type": "pi_session_entries"
        })));
        assert!(!is_replayable_value(&serde_json::json!({
            "type": "raw"
        })));
    }

    #[test]
    fn parse_bus_events_deduplicates_repeated_session_init_snapshots() {
        let session_init = serde_json::json!({
            "type": "session_init",
            "run_id": "run-1",
            "session_id": "session-1",
            "model": "gpt-5",
            "tools": [],
            "cwd": "/workspace",
            "commands_loaded": true,
            "slash_commands": [{"name": "mcp", "description": "status"}]
        });
        let changed_session_init = serde_json::json!({
            "type": "session_init",
            "run_id": "run-1",
            "session_id": "session-1",
            "model": "gpt-5.1",
            "tools": [],
            "cwd": "/workspace",
            "commands_loaded": true,
            "slash_commands": [{"name": "mcp", "description": "status"}]
        });
        let user_message = serde_json::json!({
            "type": "user_message",
            "run_id": "run-1",
            "text": "hello"
        });
        let lines = [
            serde_json::json!({"_bus": true, "seq": 1, "ts": "t1", "event": session_init}),
            serde_json::json!({"_bus": true, "seq": 2, "ts": "t2", "event": session_init}),
            serde_json::json!({"_bus": true, "seq": 3, "ts": "t3", "event": changed_session_init}),
            serde_json::json!({"_bus": true, "seq": 4, "ts": "t4", "event": changed_session_init}),
            serde_json::json!({"_bus": true, "seq": 5, "ts": "t5", "event": user_message}),
        ];

        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(tmp, "{}", line).unwrap();
        }
        tmp.flush().unwrap();

        let events = parse_bus_events_streamed(tmp.path(), None);

        assert_eq!(events.len(), 3);
        assert_eq!(
            events
                .iter()
                .map(|event| event.get("_seq").and_then(|seq| seq.as_u64()))
                .collect::<Vec<_>>(),
            vec![Some(1), Some(3), Some(5)]
        );
    }

    #[test]
    fn continuation_copies_history_through_anchor_without_future_messages() {
        let lines = [
            serde_json::json!({
                "_bus": true,
                "seq": 1,
                "event": {"type": "session_init", "run_id": "source"}
            }),
            serde_json::json!({
                "_bus": true,
                "seq": 2,
                "event": {"type": "user_message", "run_id": "source", "uuid": "user-1", "text": "before"}
            }),
            serde_json::json!({
                "_bus": true,
                "seq": 3,
                "event": {"type": "message_complete", "run_id": "source", "message_id": "assistant-1", "text": "anchor"}
            }),
            serde_json::json!({
                "_bus": true,
                "seq": 4,
                "event": {"type": "user_message", "run_id": "source", "uuid": "user-2", "text": "after"}
            }),
        ];
        let content = lines
            .iter()
            .map(serde_json::Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        let rewritten = rewrite_content_events(&content, "target", Some("assistant-1")).unwrap();
        let copied = rewritten
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(copied.len(), 2);
        assert_eq!(copied[0]["event"]["text"], "before");
        assert_eq!(copied[1]["event"]["message_id"], "assistant-1");
        assert_eq!(copied[0]["event"]["run_id"], "target");
        assert_eq!(copied[1]["event"]["run_id"], "target");
        assert_eq!(copied[0]["seq"], 1);
        assert_eq!(copied[1]["seq"], 2);
        assert!(copied.iter().all(|event| event["event"]["text"] != "after"));
    }

    #[test]
    fn continuation_copies_the_last_work_checkpoint_before_the_anchor() {
        let lines = [
            serde_json::json!({
                "_bus": true,
                "seq": 1,
                "event": {
                    "type": "work_task_state",
                    "run_id": "source",
                    "state": {"version": 1, "revision": 1, "goal": "before", "plan": [], "checkpoint": null, "updatedAt": "2026-08-13T00:00:00Z"}
                }
            }),
            serde_json::json!({
                "_bus": true,
                "seq": 2,
                "event": {"type": "message_complete", "run_id": "source", "message_id": "assistant-1", "text": "anchor"}
            }),
            serde_json::json!({
                "_bus": true,
                "seq": 3,
                "event": {
                    "type": "work_task_state",
                    "run_id": "source",
                    "state": {"version": 1, "revision": 2, "goal": "after", "plan": [], "checkpoint": null, "updatedAt": "2026-08-13T00:01:00Z"}
                }
            }),
        ];
        let content = lines
            .iter()
            .map(serde_json::Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        let rewritten = rewrite_content_events(&content, "target", Some("assistant-1")).unwrap();
        let copied = rewritten
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(copied.len(), 2);
        assert_eq!(copied[0]["event"]["message_id"], "assistant-1");
        assert_eq!(copied[1]["event"]["type"], "work_task_state");
        assert_eq!(copied[1]["event"]["run_id"], "target");
        assert_eq!(copied[1]["event"]["state"]["revision"], 1);
        assert_eq!(copied[1]["event"]["state"]["goal"], "before");
    }

    #[test]
    fn continuation_requires_an_existing_anchor() {
        let event = serde_json::json!({
            "_bus": true,
            "seq": 1,
            "event": {"type": "message_complete", "run_id": "source", "message_id": "known", "text": "hello"}
        });
        let error = rewrite_content_events(&event.to_string(), "target", Some("missing"))
            .expect_err("missing anchor must fail instead of creating an empty branch");
        assert!(error.contains("Continuation anchor"));
    }

    #[test]
    fn continuation_keeps_completed_turn_file_summaries() {
        let summary = serde_json::json!({
            "_bus": true,
            "seq": 1,
            "event": {
                "type": "turn_file_summary",
                "run_id": "source",
                "summary_id": "summary-1",
                "cwd": "/repo",
                "diff": "diff --git a/a b/a"
            }
        });

        let rewritten = rewrite_content_events(&summary.to_string(), "target", None).unwrap();
        let copied: serde_json::Value = serde_json::from_str(rewritten.trim()).unwrap();

        assert_eq!(copied["event"]["type"], "turn_file_summary");
        assert_eq!(copied["event"]["summary_id"], "summary-1");
        assert_eq!(copied["event"]["run_id"], "target");
    }
}
