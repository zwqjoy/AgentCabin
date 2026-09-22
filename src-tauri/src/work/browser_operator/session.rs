//! Browser Session Manager for Code and Work modes.
//!
//! Tracks live and historical browser state per run_id, emits unified
//! BrowserEvents, and manages takeover, pause, resume, and teardown.

use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock as StdRwLock};
use tokio::sync::{Notify, RwLock};

use crate::web_server::broadcaster::BroadcastEmitter;
use crate::work::models::{
    BrowserActionType, BrowserEvent, BrowserEventType, BrowserSession, BrowserSessionStatus,
    BrowserTraceEntry,
};

#[derive(Clone)]
pub struct BrowserSessionManager {
    sessions: Arc<RwLock<HashMap<String, BrowserSession>>>,
    control_notify: Arc<Notify>,
    emitter: Arc<StdRwLock<Option<Arc<BroadcastEmitter>>>>,
}

/// Data written when a browser action finishes. Keeping this as one value
/// avoids a wide positional argument list at every worker call site.
#[derive(Debug, Clone)]
pub struct BrowserActionCompletion {
    pub status: String,
    pub error: Option<String>,
    pub screenshot: Option<String>,
    pub page_title: Option<String>,
    pub current_url: Option<String>,
    pub duration_ms: u64,
}

static SESSION_MANAGER: OnceLock<BrowserSessionManager> = OnceLock::new();

pub fn browser_session_manager() -> &'static BrowserSessionManager {
    SESSION_MANAGER.get_or_init(BrowserSessionManager::new)
}

impl Default for BrowserSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            control_notify: Arc::new(Notify::new()),
            emitter: Arc::new(StdRwLock::new(None)),
        }
    }

    /// Attach the desktop/web event emitter after the Tauri app is initialized.
    /// Browser sessions can be created before setup completes, so the emitter
    /// is optional and installed exactly once for the process lifetime.
    pub fn set_event_emitter(&self, emitter: Arc<BroadcastEmitter>) {
        if let Ok(mut slot) = self.emitter.write() {
            *slot = Some(emitter);
        }
    }

    /// Get or register a new BrowserSession for a Code or Work run.
    pub async fn get_or_create_session(&self, run_id: &str, mode: &str) -> BrowserSession {
        let mut map = self.sessions.write().await;
        if let Some(existing) = map.get(run_id) {
            let mut current = existing.clone();
            if crate::work::browser_operator::embedded_registry()
                .resolve(run_id)
                .is_some()
                && current.surface != "embedded"
            {
                current.surface = "embedded".to_string();
                current.updated_at = Utc::now().to_rfc3339();
                map.insert(run_id.to_string(), current.clone());
            }
            return current;
        }

        let now = Utc::now().to_rfc3339();
        let surface = "embedded";
        let session = BrowserSession {
            session_id: format!("bsess-{}", run_id),
            run_id: run_id.to_string(),
            mode: mode.to_string(),
            surface: surface.to_string(),
            status: BrowserSessionStatus::Idle,
            current_url: None,
            page_title: None,
            current_action: None,
            last_screenshot: None,
            traces: Vec::new(),
            last_error: None,
            is_taking_over: false,
            created_at: now.clone(),
            updated_at: now,
        };

        map.insert(run_id.to_string(), session.clone());
        self.emit_event(
            BrowserEventType::SessionStarted,
            &session.session_id,
            run_id,
            mode,
            json!({ "status": session.status }),
        );
        session
    }

    /// Get existing session if available.
    pub async fn get_session(&self, run_id: &str) -> Option<BrowserSession> {
        let map = self.sessions.read().await;
        map.get(run_id).cloned()
    }

    /// Wait until a browser action is allowed to run for this session.
    /// Pausing or taking over deliberately holds the agent's next browser
    /// tool call instead of merely changing a UI label.
    pub async fn wait_until_actionable(&self, run_id: &str) -> Result<(), String> {
        loop {
            let notified = self.control_notify.notified();
            let status = self.get_session(run_id).await.map(|session| session.status);
            match status {
                Some(BrowserSessionStatus::Paused) | Some(BrowserSessionStatus::TakingOver) => {
                    notified.await
                }
                Some(BrowserSessionStatus::Closed) => {
                    return Err(format!("Browser session {run_id} is closed"));
                }
                _ => return Ok(()),
            }
        }
    }

    /// List all active or recent browser sessions.
    pub async fn list_sessions(&self) -> Vec<BrowserSession> {
        let map = self.sessions.read().await;
        let mut list: Vec<_> = map.values().cloned().collect();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list
    }

    /// Record the start of a browser action. Returns step_index.
    pub async fn record_action_start(
        &self,
        run_id: &str,
        action_type: BrowserActionType,
        description: &str,
        target_url: Option<String>,
        selector: Option<String>,
    ) -> u32 {
        let mut map = self.sessions.write().await;
        let session = map.entry(run_id.to_string()).or_insert_with(|| {
            let now = Utc::now().to_rfc3339();
            BrowserSession {
                session_id: format!("bsess-{}", run_id),
                run_id: run_id.to_string(),
                mode: "work".to_string(),
                surface: "embedded".to_string(),
                status: BrowserSessionStatus::Running,
                current_url: target_url.clone(),
                page_title: None,
                current_action: Some(description.to_string()),
                last_screenshot: None,
                traces: Vec::new(),
                last_error: None,
                is_taking_over: false,
                created_at: now.clone(),
                updated_at: now,
            }
        });

        session.status = BrowserSessionStatus::Running;
        session.current_action = Some(description.to_string());
        if let Some(ref url) = target_url {
            session.current_url = Some(url.clone());
        }
        let now = Utc::now().to_rfc3339();
        session.updated_at = now.clone();

        let step_index = session.traces.len() as u32 + 1;
        let trace = BrowserTraceEntry {
            step_index,
            action_type,
            description: description.to_string(),
            target_url: target_url.clone(),
            selector: selector.clone(),
            status: "started".to_string(),
            screenshot_data: None,
            duration_ms: 0,
            error: None,
            timestamp: now,
        };
        session.traces.push(trace);

        let event_type = match action_type {
            BrowserActionType::Navigate => BrowserEventType::NavigationStarted,
            BrowserActionType::Snapshot => BrowserEventType::ReadStarted,
            _ => BrowserEventType::ActionStarted,
        };

        self.emit_event(
            event_type,
            &session.session_id,
            run_id,
            &session.mode,
            json!({
                "stepIndex": step_index,
                "actionType": action_type,
                "description": description,
                "targetUrl": target_url,
                "selector": selector,
            }),
        );

        step_index
    }

    /// Record the completion or failure of a browser action.
    pub async fn record_action_end(
        &self,
        run_id: &str,
        step_index: u32,
        completion: BrowserActionCompletion,
    ) {
        let mut map = self.sessions.write().await;
        if let Some(session) = map.get_mut(run_id) {
            let now = Utc::now().to_rfc3339();
            session.updated_at = now.clone();
            if let Some(title) = completion.page_title {
                session.page_title = Some(title);
            }
            if let Some(url) = completion.current_url {
                session.current_url = Some(url);
            }
            if let Some(ref sc) = completion.screenshot {
                session.last_screenshot = Some(sc.clone());
            }

            if let Some(trace) = session
                .traces
                .iter_mut()
                .find(|t| t.step_index == step_index)
            {
                trace.status = completion.status.clone();
                trace.duration_ms = completion.duration_ms;
                trace.error = completion.error.clone();
                if let Some(sc) = completion.screenshot.clone() {
                    trace.screenshot_data = Some(sc);
                }
            }

            // A browser session can try multiple sources in one Work run.
            // Keep the failed action in the trace, but do not surface the
            // whole session as failed when another action in the same session
            // already succeeded (for example, wttr.in fails and
            // weather.com.cn supplies the evidence). Only a session with no
            // successful action remains failed after an error.
            if let Some(err) = completion.error.clone() {
                session.last_error = Some(err);
                if session.status != BrowserSessionStatus::Closed {
                    let has_successful_action =
                        session.traces.iter().any(|trace| trace.status == "success");
                    session.status = if has_successful_action {
                        BrowserSessionStatus::Idle
                    } else {
                        BrowserSessionStatus::Failed
                    };
                }
            } else if session.status != BrowserSessionStatus::Closed {
                session.status = BrowserSessionStatus::Idle;
                session.last_error = None;
            }

            let event_type = if completion.error.is_some() {
                BrowserEventType::ActionFailed
            } else {
                BrowserEventType::ActionSucceeded
            };

            self.emit_event(
                event_type,
                &session.session_id,
                run_id,
                &session.mode,
                json!({
                    "stepIndex": step_index,
                    "status": completion.status,
                    "error": completion.error,
                    "hasScreenshot": completion.screenshot.is_some(),
                    "durationMs": completion.duration_ms,
                }),
            );

            if completion.screenshot.is_some() {
                self.emit_event(
                    BrowserEventType::ScreenshotCreated,
                    &session.session_id,
                    run_id,
                    &session.mode,
                    json!({ "stepIndex": step_index }),
                );
            }
        }
    }

    /// Directly update live session snapshot (title, url, screenshot) from user interactions.
    pub async fn update_session_snapshot(
        &self,
        run_id: &str,
        page_title: Option<String>,
        current_url: Option<String>,
        screenshot: Option<String>,
    ) -> Result<BrowserSession, String> {
        let mut map = self.sessions.write().await;
        let session = map.entry(run_id.to_string()).or_insert_with(|| {
            let now = Utc::now().to_rfc3339();
            BrowserSession {
                session_id: format!("bsess-{}", run_id),
                run_id: run_id.to_string(),
                mode: "code".to_string(),
                surface: if crate::work::browser_operator::embedded_registry()
                    .resolve(run_id)
                    .is_some()
                {
                    "embedded".to_string()
                } else {
                    "embedded".to_string()
                },
                status: BrowserSessionStatus::Idle,
                current_url: current_url.clone(),
                page_title: page_title.clone(),
                current_action: None,
                last_screenshot: screenshot.clone(),
                traces: Vec::new(),
                last_error: None,
                is_taking_over: false,
                created_at: now.clone(),
                updated_at: now,
            }
        });

        let now = Utc::now().to_rfc3339();
        session.updated_at = now.clone();
        if let Some(title) = page_title {
            session.page_title = Some(title);
        }
        if let Some(url) = current_url {
            session.current_url = Some(url);
        }
        if let Some(sc) = screenshot {
            session.last_screenshot = Some(sc);
        }
        let updated = session.clone();
        drop(map);

        self.emit_event(
            BrowserEventType::SessionUpdated,
            &updated.session_id,
            run_id,
            &updated.mode,
            json!({
                "status": updated.status,
                "currentUrl": updated.current_url,
                "pageTitle": updated.page_title,
                "lastScreenshot": updated.last_screenshot,
            }),
        );

        Ok(updated)
    }

    /// User takeover control.
    pub async fn start_takeover(&self, run_id: &str) -> Result<BrowserSession, String> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(run_id)
            .ok_or_else(|| format!("Browser session not found for run {}", run_id))?;
        if session.status == BrowserSessionStatus::Closed {
            return Err(format!("Browser session {run_id} is already closed"));
        }
        session.is_taking_over = true;
        session.status = BrowserSessionStatus::TakingOver;
        session.updated_at = Utc::now().to_rfc3339();

        self.emit_event(
            BrowserEventType::TakeoverStarted,
            &session.session_id,
            run_id,
            &session.mode,
            json!({ "status": session.status }),
        );
        Ok(session.clone())
    }

    pub async fn finish_takeover(&self, run_id: &str) -> Result<BrowserSession, String> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(run_id)
            .ok_or_else(|| format!("Browser session not found for run {}", run_id))?;
        if session.status == BrowserSessionStatus::Closed {
            return Err(format!("Browser session {run_id} is already closed"));
        }
        session.is_taking_over = false;
        session.status = BrowserSessionStatus::Idle;
        session.updated_at = Utc::now().to_rfc3339();

        self.control_notify.notify_waiters();

        self.emit_event(
            BrowserEventType::TakeoverFinished,
            &session.session_id,
            run_id,
            &session.mode,
            json!({ "status": session.status }),
        );
        Ok(session.clone())
    }

    /// Pause session.
    pub async fn pause_session(&self, run_id: &str) -> Result<BrowserSession, String> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(run_id)
            .ok_or_else(|| format!("Browser session not found for run {}", run_id))?;
        if session.status == BrowserSessionStatus::Closed {
            return Err(format!("Browser session {run_id} is already closed"));
        }
        session.status = BrowserSessionStatus::Paused;
        session.updated_at = Utc::now().to_rfc3339();

        self.emit_event(
            BrowserEventType::SessionPaused,
            &session.session_id,
            run_id,
            &session.mode,
            json!({ "status": session.status }),
        );
        Ok(session.clone())
    }

    /// Resume session.
    pub async fn resume_session(&self, run_id: &str) -> Result<BrowserSession, String> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(run_id)
            .ok_or_else(|| format!("Browser session not found for run {}", run_id))?;
        if session.status == BrowserSessionStatus::Closed {
            return Err(format!("Browser session {run_id} is already closed"));
        }
        session.status = BrowserSessionStatus::Idle;
        session.updated_at = Utc::now().to_rfc3339();

        self.control_notify.notify_waiters();

        self.emit_event(
            BrowserEventType::SessionResumed,
            &session.session_id,
            run_id,
            &session.mode,
            json!({ "status": session.status }),
        );
        Ok(session.clone())
    }

    /// Stop session.
    pub async fn stop_session(&self, run_id: &str) -> Result<BrowserSession, String> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(run_id)
            .ok_or_else(|| format!("Browser session not found for run {}", run_id))?;
        session.status = BrowserSessionStatus::Closed;
        session.updated_at = Utc::now().to_rfc3339();

        self.control_notify.notify_waiters();

        self.emit_event(
            BrowserEventType::SessionStopped,
            &session.session_id,
            run_id,
            &session.mode,
            json!({ "status": session.status }),
        );
        Ok(session.clone())
    }

    /// Mark a session closed while retaining its trace/screenshot history.
    ///
    /// Browser sessions are a run-scoped inspection record, not only a worker
    /// handle. Removing them here would make the final trace disappear as
    /// soon as the browser context is closed or the actor is stopped.
    pub async fn close_session(&self, run_id: &str) -> Result<(), String> {
        let mut map = self.sessions.write().await;
        if let Some(session) = map.get_mut(run_id) {
            if session.status != BrowserSessionStatus::Closed {
                session.status = BrowserSessionStatus::Closed;
                session.updated_at = Utc::now().to_rfc3339();
            }
            self.emit_event(
                BrowserEventType::SessionClosed,
                &session.session_id,
                run_id,
                &session.mode,
                json!({ "status": "closed" }),
            );
        }
        self.control_notify.notify_waiters();
        Ok(())
    }

    fn emit_event(
        &self,
        event_type: BrowserEventType,
        session_id: &str,
        run_id: &str,
        mode: &str,
        payload: serde_json::Value,
    ) {
        let event = BrowserEvent {
            event_type,
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            mode: mode.to_string(),
            payload,
            timestamp: Utc::now().to_rfc3339(),
        };

        if let Ok(emitter) = self.emitter.read() {
            if let Some(emitter) = emitter.as_ref() {
                emitter.emit_realtime("browser-event", &event, Some(run_id));
            }
        }

        log::debug!(
            "[browser/event] [{:?}] run_id={} session_id={}",
            event.event_type,
            event.run_id,
            event.session_id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_browser_session_lifecycle() {
        let manager = BrowserSessionManager::new();
        let session = manager.get_or_create_session("run-123", "work").await;
        assert_eq!(session.run_id, "run-123");
        assert_eq!(session.mode, "work");
        assert_eq!(session.status, BrowserSessionStatus::Idle);

        let step = manager
            .record_action_start(
                "run-123",
                BrowserActionType::Navigate,
                "导航至 https://example.com",
                Some("https://example.com".to_string()),
                None,
            )
            .await;
        assert_eq!(step, 1);

        let s1 = manager.get_session("run-123").await.unwrap();
        assert_eq!(s1.status, BrowserSessionStatus::Running);
        assert_eq!(s1.current_url.as_deref(), Some("https://example.com"));

        manager
            .record_action_end(
                "run-123",
                step,
                BrowserActionCompletion {
                    status: "success".to_string(),
                    error: None,
                    screenshot: Some("data:image/png;base64,abc".to_string()),
                    page_title: Some("Example Domain".to_string()),
                    current_url: Some("https://example.com".to_string()),
                    duration_ms: 120,
                },
            )
            .await;

        let s2 = manager.get_session("run-123").await.unwrap();
        assert_eq!(s2.status, BrowserSessionStatus::Idle);
        assert_eq!(s2.page_title.as_deref(), Some("Example Domain"));
        assert_eq!(s2.traces.len(), 1);
        assert_eq!(s2.traces[0].status, "success");
        assert_eq!(s2.traces[0].duration_ms, 120);

        manager.start_takeover("run-123").await.unwrap();
        let s3 = manager.get_session("run-123").await.unwrap();
        assert!(s3.is_taking_over);
        assert_eq!(s3.status, BrowserSessionStatus::TakingOver);

        manager.finish_takeover("run-123").await.unwrap();
        let s4 = manager.get_session("run-123").await.unwrap();
        assert!(!s4.is_taking_over);

        manager.close_session("run-123").await.unwrap();
        let closed = manager.get_session("run-123").await.unwrap();
        assert_eq!(closed.status, BrowserSessionStatus::Closed);
        assert_eq!(closed.traces.len(), 1);
    }

    #[tokio::test]
    async fn paused_session_blocks_browser_actions_until_resumed() {
        let manager = BrowserSessionManager::new();
        manager.get_or_create_session("run-paused", "work").await;
        manager.pause_session("run-paused").await.unwrap();

        let manager_for_wait = manager.clone();
        let mut wait_task =
            tokio::spawn(async move { manager_for_wait.wait_until_actionable("run-paused").await });

        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut wait_task)
                .await
                .is_err()
        );

        manager.resume_session("run-paused").await.unwrap();
        assert!(wait_task.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn recovered_browser_session_does_not_remain_failed_after_later_source_error() {
        let manager = BrowserSessionManager::new();
        let run_id = "run-browser-fallback";
        manager.get_or_create_session(run_id, "work").await;

        let successful_step = manager
            .record_action_start(
                run_id,
                BrowserActionType::Navigate,
                "导航至 https://weather.example.com",
                Some("https://weather.example.com".to_string()),
                None,
            )
            .await;
        manager
            .record_action_end(
                run_id,
                successful_step,
                BrowserActionCompletion {
                    status: "success".to_string(),
                    error: None,
                    screenshot: None,
                    page_title: Some("Weather".to_string()),
                    current_url: Some("https://weather.example.com".to_string()),
                    duration_ms: 10,
                },
            )
            .await;

        let failed_candidate = manager
            .record_action_start(
                run_id,
                BrowserActionType::Navigate,
                "导航至 https://unavailable.example.com",
                Some("https://unavailable.example.com".to_string()),
                None,
            )
            .await;
        manager
            .record_action_end(
                run_id,
                failed_candidate,
                BrowserActionCompletion {
                    status: "failed".to_string(),
                    error: Some("连接被关闭".to_string()),
                    screenshot: None,
                    page_title: None,
                    current_url: None,
                    duration_ms: 20,
                },
            )
            .await;

        let session = manager.get_session(run_id).await.unwrap();
        assert_eq!(session.status, BrowserSessionStatus::Idle);
        assert_eq!(session.traces[0].status, "success");
        assert_eq!(session.traces[1].status, "failed");
        assert_eq!(session.last_error.as_deref(), Some("连接被关闭"));
    }

    #[tokio::test]
    async fn browser_session_remains_failed_when_every_action_fails() {
        let manager = BrowserSessionManager::new();
        let run_id = "run-browser-all-failed";
        manager.get_or_create_session(run_id, "work").await;

        let step = manager
            .record_action_start(
                run_id,
                BrowserActionType::Navigate,
                "导航至 https://unavailable.example.com",
                Some("https://unavailable.example.com".to_string()),
                None,
            )
            .await;
        manager
            .record_action_end(
                run_id,
                step,
                BrowserActionCompletion {
                    status: "failed".to_string(),
                    error: Some("连接被关闭".to_string()),
                    screenshot: None,
                    page_title: None,
                    current_url: None,
                    duration_ms: 20,
                },
            )
            .await;

        let session = manager.get_session(run_id).await.unwrap();
        assert_eq!(session.status, BrowserSessionStatus::Failed);
    }
}
