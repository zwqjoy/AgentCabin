//! Browser Operator Manager for AgentCabin Host.
//!
//! Owns the Node.js Browser Worker child process, routes tool calls by `run_id`,
//! and ensures that each WorkRun receives an isolated ephemeral BrowserContext.

use base64::Engine;
use std::collections::HashMap;
use std::fs;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex, RwLock};
use tokio::time::timeout;

use super::embedded::ResolvedEmbeddedTarget;
use super::ipc::{WorkerRequest, WorkerResponse};
use super::security::{validate_browser_eval_fixture_url, validate_browser_url_with_allowed_hosts};
use super::session::BrowserActionCompletion;
use crate::process_ext::HideConsole;
use crate::work::executor::runtime::RuntimeResolver;
use crate::work::models::WorkExecutionRuntime;
use crate::work::paths::WorkPaths;

const CALL_TIMEOUT: Duration = Duration::from_secs(60);
const EMBEDDED_ATTACH_TIMEOUT: Duration = Duration::from_secs(5);

pub struct BrowserWorkerHandle {
    _child: Child,
    stdin: ChildStdin,
    pending: Arc<RwLock<HashMap<u64, oneshot::Sender<WorkerResponse>>>>,
    browsers_path: Option<std::path::PathBuf>,
}

#[derive(Clone)]
pub struct BrowserOperatorManager {
    worker: Arc<Mutex<Option<BrowserWorkerHandle>>>,
    req_counter: Arc<AtomicU64>,
    run_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

static MANAGER: std::sync::OnceLock<BrowserOperatorManager> = std::sync::OnceLock::new();

pub fn browser_operator_manager() -> &'static BrowserOperatorManager {
    MANAGER.get_or_init(BrowserOperatorManager::new)
}

/// Merge the Electron relay coordinates into a browser call.
/// The built-in browser is the only supported surface.
fn with_embedded_target(target: ResolvedEmbeddedTarget, params: Value) -> Value {
    let mut object = match params {
        Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("input".to_string(), other);
            map
        }
    };
    object.insert("cdpEndpoint".to_string(), Value::String(target.endpoint));
    object.insert("cdpToken".to_string(), Value::String(target.token));
    object.insert("cdpTargetId".to_string(), Value::String(target.target_id));
    Value::Object(object)
}

/// The renderer mounts the native view asynchronously. Wait briefly for its
/// registration instead of letting the first browser tool call fall through
/// to the worker's unsupported standalone-session path.
async fn wait_for_embedded_surface(run_id: &str) -> Result<ResolvedEmbeddedTarget, String> {
    let deadline = tokio::time::Instant::now() + EMBEDDED_ATTACH_TIMEOUT;
    loop {
        if let Some(target) = crate::work::browser_operator::embedded_registry().resolve(run_id) {
            return Ok(target);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "Embedded browser relay was not registered for run '{run_id}' within {:?}; ensure the browser panel is mounted.",
                EMBEDDED_ATTACH_TIMEOUT
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

impl Default for BrowserOperatorManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserOperatorManager {
    pub fn new() -> Self {
        Self {
            worker: Arc::new(Mutex::new(None)),
            req_counter: Arc::new(AtomicU64::new(1)),
            run_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn run_lock(&self, run_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.run_locks.lock().await;
        locks
            .entry(run_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Spawn or ensure Browser Worker child process is alive.
    async fn ensure_worker(&self) -> Result<(), String> {
        let app_paths = WorkPaths::app();
        let (script_path, browsers_path) = super::runtime::resolve_worker_launch(&app_paths)?;
        let node = RuntimeResolver::resolve_binary(WorkExecutionRuntime::Node)?;

        let mut guard = self.worker.lock().await;
        if guard
            .as_ref()
            .is_some_and(|worker| worker.browsers_path == browsers_path)
        {
            return Ok(());
        }

        // Drop a worker whose launch configuration no longer matches. The
        // current worker has no browser cache or external runtime to inherit.
        drop(guard.take());

        if !script_path.exists() {
            return Err(format!(
                "Browser worker server script not found at {:?}",
                script_path
            ));
        }

        let mut cmd = Command::new(&node);
        cmd.arg(&script_path)
            .current_dir(
                script_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(".")),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .hide_console()
            .kill_on_drop(true);
        cmd.env("PATH", super::runtime::command_path(&node));
        for (key, val) in crate::work::proxy::work_node_proxy_env() {
            cmd.env(key, val);
        }
        if let Ok(config) = crate::work::browser::get_config_with_paths(&app_paths) {
            if !config.allowed_hosts.is_empty() {
                cmd.env(
                    "AGENTCABIN_BROWSER_ALLOWED_HOSTS",
                    config.allowed_hosts.join(","),
                );
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn Node.js Browser Worker: {e}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to capture stdin of Browser Worker".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to capture stdout of Browser Worker".to_string())?;

        let pending: Arc<RwLock<HashMap<u64, oneshot::Sender<WorkerResponse>>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let pending_clone = pending.clone();

        let worker_ref = self.worker.clone();
        // Background reader for worker stdout
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(resp) = serde_json::from_str::<WorkerResponse>(trimmed) {
                    if let Some(id) = resp.id {
                        let mut map = pending_clone.write().await;
                        if let Some(tx) = map.remove(&id) {
                            let _ = tx.send(resp);
                        }
                    }
                }
            }

            // Reader exited: worker crashed or EOF reached
            let mut map = pending_clone.write().await;
            for (_id, tx) in map.drain() {
                let _ = tx.send(WorkerResponse {
                    id: None,
                    ok: false,
                    result: None,
                    error: Some("Browser Worker terminated unexpectedly".into()),
                });
            }
            let mut guard = worker_ref.lock().await;
            *guard = None;
        });

        *guard = Some(BrowserWorkerHandle {
            _child: child,
            stdin,
            pending,
            browsers_path,
        });

        Ok(())
    }

    /// Execute a tool method on the Browser Worker.
    pub async fn call(&self, run_id: &str, method: &str, params: Value) -> Result<Value, String> {
        let run_lock = self.run_lock(run_id).await;
        let _run_guard = run_lock.lock().await;
        let params = if method == "ping" {
            params
        } else {
            let target = wait_for_embedded_surface(run_id).await?;
            with_embedded_target(target, params)
        };
        self.ensure_worker().await?;

        let req_id = self.req_counter.fetch_add(1, Ordering::Relaxed);
        let req = WorkerRequest {
            id: req_id,
            run_id: run_id.to_string(),
            method: method.to_string(),
            params,
        };

        let json_line = serde_json::to_string(&req).map_err(|e| e.to_string())? + "\n";
        let (tx, rx) = oneshot::channel();

        {
            let mut guard = self.worker.lock().await;
            if let Some(handle) = guard.as_mut() {
                handle.pending.write().await.insert(req_id, tx);
                if let Err(e) = handle.stdin.write_all(json_line.as_bytes()).await {
                    // Worker stdin broken; kill and clear
                    *guard = None;
                    return Err(format!("Failed to write to Browser Worker: {e}"));
                }
                let _ = handle.stdin.flush().await;
            } else {
                return Err("Browser worker unavailable".into());
            }
        }

        let resp = match timeout(CALL_TIMEOUT, rx).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => {
                // Sender dropped, worker probably died
                let mut guard = self.worker.lock().await;
                *guard = None;
                return Err("Browser Worker disconnected unexpectedly".into());
            }
            Err(_) => {
                // Timeout: remove from pending map so it does not leak
                let guard = self.worker.lock().await;
                if let Some(handle) = guard.as_ref() {
                    handle.pending.write().await.remove(&req_id);
                }
                return Err(format!(
                    "Browser call '{method}' timed out after {:?}",
                    CALL_TIMEOUT
                ));
            }
        };

        if resp.ok {
            Ok(resp.result.unwrap_or(Value::Null))
        } else {
            Err(resp
                .error
                .unwrap_or_else(|| "Unknown error in Browser Worker".into()))
        }
    }

    /// Execute the public, mode-neutral browser protocol.
    ///
    /// Work routes this same protocol through ToolPipeline, while Code reaches
    /// it through the authenticated browser bridge. Calls target the embedded
    /// Electron CDP surface.
    pub async fn execute(
        &self,
        run_id: &str,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        let session_mgr = super::session::browser_session_manager();
        session_mgr.get_or_create_session(run_id, "work").await;
        session_mgr.wait_until_actionable(run_id).await?;
        let (action_type, desc, target_url, selector) = parse_action_meta(method, &params);
        let step_index = session_mgr
            .record_action_start(
                run_id,
                action_type,
                &desc,
                target_url.clone(),
                selector.clone(),
            )
            .await;
        let start_time = std::time::Instant::now();

        let result = self.execute_internal(run_id, method, params).await;
        let duration_ms = start_time.elapsed().as_millis() as u64;

        match &result {
            Ok(val) => {
                let current_url = val
                    .get("url")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
                    .or(target_url);
                let page_title = val
                    .get("title")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                let screenshot = screenshot_data_from_result(val);
                let verification_failed = val
                    .get("timedOut")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let verification_error = verification_failed.then(|| {
                    val.get("failures")
                        .and_then(Value::as_array)
                        .map(|failures| {
                            failures
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join("; ")
                        })
                        .filter(|message| !message.is_empty())
                        .unwrap_or_else(|| "Expected browser page condition was not met".into())
                });

                session_mgr
                    .record_action_end(
                        run_id,
                        step_index,
                        BrowserActionCompletion {
                            status: if verification_failed {
                                "failed".to_string()
                            } else {
                                "success".to_string()
                            },
                            error: verification_error,
                            screenshot,
                            page_title,
                            current_url,
                            duration_ms,
                        },
                    )
                    .await;
            }
            Err(err) => {
                session_mgr
                    .record_action_end(
                        run_id,
                        step_index,
                        BrowserActionCompletion {
                            status: "failed".to_string(),
                            error: Some(err.clone()),
                            screenshot: None,
                            page_title: None,
                            current_url: None,
                            duration_ms,
                        },
                    )
                    .await;
            }
        }

        result
    }

    async fn execute_internal(
        &self,
        run_id: &str,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        match method {
            "browser_navigate" => {
                let url = param_string(&params, &["url"])
                    .ok_or_else(|| "browser_navigate requires url".to_string())?;
                self.navigate(run_id, &url, param_u64(&params, &["wait_ms", "waitMs"]))
                    .await
            }
            "browser_eval_navigate" => {
                let url = param_string(&params, &["url"])
                    .ok_or_else(|| "browser_eval_navigate requires url".to_string())?;
                let allowed_origin = param_string(&params, &["allow_origin", "allowOrigin"])
                    .ok_or_else(|| "browser_eval_navigate requires allowOrigin".to_string())?;
                self.navigate_for_eval(
                    run_id,
                    &url,
                    &allowed_origin,
                    param_u64(&params, &["wait_ms", "waitMs"]),
                )
                .await
            }
            "browser_eval_tabs" => {
                let action =
                    param_string(&params, &["action"]).unwrap_or_else(|| "list".to_string());
                let index = param_u64(&params, &["index"]).map(|value| value as usize);
                let url = param_string(&params, &["url"]);
                let allowed_origin = param_string(&params, &["allow_origin", "allowOrigin"])
                    .ok_or_else(|| "browser_eval_tabs requires allowOrigin".to_string())?;
                self.tabs_for_eval(run_id, &action, index, url, &allowed_origin)
                    .await
            }
            "browser_cdp_observe" | "browser_cdp_act" => self.call(run_id, method, params).await,
            "browser_snapshot" => self.snapshot(run_id).await,
            "browser_take_screenshot" => {
                self.screenshot(
                    run_id,
                    param_string(&params, &["filename", "outputPath"]),
                    param_bool(&params, &["full_page", "fullPage"]),
                )
                .await
            }
            "browser_wait_for" => self.wait(run_id, params).await,
            "browser_tabs" => {
                let target_id = param_string(&params, &["target_id", "targetId", "id"]);
                self.tabs_with_target(
                    run_id,
                    &param_string(&params, &["action"]).unwrap_or_else(|| "list".to_string()),
                    param_u64(&params, &["index"]).map(|value| value as usize),
                    param_string(&params, &["url"]),
                    target_id,
                )
                .await
            }
            "browser_close" => self.close_context(run_id).await,
            "browser_click" => {
                self.click(
                    run_id,
                    param_string(&params, &["ref"]),
                    param_string(&params, &["selector"]),
                    param_string(&params, &["button"]),
                    param_bool(&params, &["double_click", "doubleClick"]),
                )
                .await
            }
            "browser_type" => {
                self.type_text(
                    run_id,
                    param_string(&params, &["ref"]),
                    param_string(&params, &["text"]).unwrap_or_default(),
                    param_string(&params, &["selector"]),
                    param_bool(&params, &["clear"]),
                    param_bool(&params, &["press_enter", "pressEnter"]),
                )
                .await
            }
            "browser_select_option" => {
                self.select_option(
                    run_id,
                    param_string(&params, &["ref"]),
                    param_string(&params, &["value"]).unwrap_or_default(),
                    param_string(&params, &["selector"]),
                )
                .await
            }
            "browser_scroll" => {
                self.scroll(
                    run_id,
                    param_string(&params, &["direction"]),
                    param_u64(&params, &["amount"]),
                    param_string(&params, &["ref"]),
                )
                .await
            }
            other => Err(format!("Unknown browser tool: {other}")),
        }
    }

    /// Invalidate active worker process so the next command respawns with updated environment.
    pub fn reload_worker(&self) {
        if let Ok(mut guard) = self.worker.try_lock() {
            drop(guard.take());
        } else if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let worker_arc = self.worker.clone();
            handle.spawn(async move {
                let mut guard = worker_arc.lock().await;
                drop(guard.take());
            });
        }
    }

    /// `browser_navigate`: validate URL against SSRF policy and navigate.
    pub async fn navigate(
        &self,
        run_id: &str,
        url: &str,
        wait_ms: Option<u64>,
    ) -> Result<Value, String> {
        let app_paths = WorkPaths::app();
        let allowed_hosts = crate::work::browser::get_config_with_paths(&app_paths)
            .map(|c| c.allowed_hosts)
            .unwrap_or_default();
        let output_root = app_paths.standalone_output_dir(run_id).ok();
        let valid_url =
            validate_browser_url_with_allowed_hosts(url, &allowed_hosts, output_root.as_deref())
                .map_err(|e| format!("SSRF validation blocked URL: {e}"))?;
        let params = json!({
            "url": valid_url.to_string(),
            "waitMs": wait_ms,
            "allowFileRoot": output_root.map(|path| path.to_string_lossy().into_owned()),
        });
        self.call(run_id, "browser_navigate", params).await
    }

    /// Navigate an evaluation fixture only when its URL matches the exact
    /// temporary loopback origin registered by the live eval runner.
    async fn navigate_for_eval(
        &self,
        run_id: &str,
        url: &str,
        allowed_origin: &str,
        wait_ms: Option<u64>,
    ) -> Result<Value, String> {
        let valid_url = validate_browser_eval_fixture_url(url, allowed_origin)
            .map_err(|error| format!("Eval fixture URL blocked: {error}"))?;
        self.call(
            run_id,
            "browser_navigate",
            json!({
                "url": valid_url.to_string(),
                "waitMs": wait_ms,
                "allowOrigin": allowed_origin,
            }),
        )
        .await
    }

    async fn tabs_for_eval(
        &self,
        run_id: &str,
        action: &str,
        index: Option<usize>,
        url: Option<String>,
        allowed_origin: &str,
    ) -> Result<Value, String> {
        let valid_url = url
            .as_deref()
            .map(|value| validate_browser_eval_fixture_url(value, allowed_origin))
            .transpose()
            .map_err(|error| format!("Eval fixture URL blocked: {error}"))?;
        self.call(
            run_id,
            "browser_tabs",
            json!({
                "action": action,
                "index": index,
                "url": valid_url.map(|value| value.to_string()),
                "allowOrigin": allowed_origin,
            }),
        )
        .await
    }

    /// `browser_snapshot`: produce semantic ref accessibility snapshot.
    pub async fn snapshot(&self, run_id: &str) -> Result<Value, String> {
        self.call(run_id, "browser_snapshot", json!({})).await
    }

    /// `browser_take_screenshot`: capture page screenshot.
    pub async fn screenshot(
        &self,
        run_id: &str,
        output_path: Option<String>,
        full_page: Option<bool>,
    ) -> Result<Value, String> {
        let params = json!({
            "outputPath": output_path,
            "fullPage": full_page,
        });
        self.call(run_id, "browser_take_screenshot", params).await
    }

    /// `browser_wait_for`: wait for a duration and/or verify a page condition.
    pub async fn wait(&self, run_id: &str, params: Value) -> Result<Value, String> {
        self.call(run_id, "browser_wait_for", params).await
    }

    /// `browser_tabs`: list, new, switch, or close tabs.
    pub async fn tabs(
        &self,
        run_id: &str,
        action: &str,
        index: Option<usize>,
        url: Option<String>,
    ) -> Result<Value, String> {
        self.tabs_with_target(run_id, action, index, url, None)
            .await
    }

    /// `browser_tabs` with optional target_id for stable tab addressing.
    pub async fn tabs_with_target(
        &self,
        run_id: &str,
        action: &str,
        index: Option<usize>,
        url: Option<String>,
        target_id: Option<String>,
    ) -> Result<Value, String> {
        let mut params = json!({
            "action": action,
            "index": index,
        });
        if let Some(target_url) = &url {
            let app_paths = WorkPaths::app();
            let allowed_hosts = crate::work::browser::get_config_with_paths(&app_paths)
                .map(|c| c.allowed_hosts)
                .unwrap_or_default();
            let valid = validate_browser_url_with_allowed_hosts(target_url, &allowed_hosts, None)
                .map_err(|e| format!("SSRF validation blocked URL: {e}"))?;
            params["url"] = json!(valid.to_string());
        }
        if let Some(tid) = target_id {
            params["targetId"] = json!(tid);
            params["id"] = json!(tid);
        }
        self.call(run_id, "browser_tabs", params).await
    }

    /// `browser_close`: explicitly close browser context for this WorkRun.
    pub async fn close_context(&self, run_id: &str) -> Result<Value, String> {
        let is_running = self.worker.lock().await.is_some();
        if !is_running
            || crate::work::browser_operator::embedded_registry()
                .resolve(run_id)
                .is_none()
        {
            let _ = crate::work::browser_operator::browser_session_manager()
                .close_session(run_id)
                .await;
            return Ok(json!({ "ok": true, "runId": run_id, "skipped": true }));
        }
        let session_mgr = crate::work::browser_operator::browser_session_manager();
        let res = self.call(run_id, "browser_close", json!({})).await?;
        let _ = session_mgr.close_session(run_id).await;
        Ok(res)
    }

    /// Bring the active page to the front for a desktop takeover.
    /// This is intentionally not exposed as an agent tool.
    pub async fn focus_page(&self, run_id: &str) -> Result<Value, String> {
        self.call(run_id, "browser_focus", json!({})).await
    }

    /// Direct user interaction from the embedded browser panel (click, scroll, navigate, etc.).
    pub async fn user_interact(
        &self,
        run_id: &str,
        action: &str,
        params: Value,
    ) -> Result<crate::work::models::BrowserSession, String> {
        let mut interact_params = params;
        if let Some(obj) = interact_params.as_object_mut() {
            obj.insert("action".to_string(), json!(action));
        } else {
            interact_params = json!({ "action": action });
        }

        let result = self
            .call(run_id, "browser_interact", interact_params)
            .await?;

        let session_mgr = super::session::browser_session_manager();
        session_mgr.get_or_create_session(run_id, "code").await;

        let current_url = result
            .get("url")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let page_title = result
            .get("title")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let screenshot = screenshot_data_from_result(&result);

        session_mgr
            .update_session_snapshot(run_id, page_title, current_url, screenshot)
            .await
    }

    /// `browser_click`: click on an element by semantic ref or selector.
    pub async fn click(
        &self,
        run_id: &str,
        ref_id: Option<String>,
        selector: Option<String>,
        button: Option<String>,
        double_click: Option<bool>,
    ) -> Result<Value, String> {
        let params = json!({
            "ref": ref_id,
            "selector": selector,
            "button": button,
            "doubleClick": double_click,
        });
        self.call(run_id, "browser_click", params).await
    }

    /// `browser_type`: type text into an input element.
    pub async fn type_text(
        &self,
        run_id: &str,
        ref_id: Option<String>,
        text: String,
        selector: Option<String>,
        clear: Option<bool>,
        press_enter: Option<bool>,
    ) -> Result<Value, String> {
        let params = json!({
            "ref": ref_id,
            "text": text,
            "selector": selector,
            "clear": clear,
            "pressEnter": press_enter,
        });
        self.call(run_id, "browser_type", params).await
    }

    /// `browser_select_option`: select an option from a dropdown.
    pub async fn select_option(
        &self,
        run_id: &str,
        ref_id: Option<String>,
        value: String,
        selector: Option<String>,
    ) -> Result<Value, String> {
        let params = json!({
            "ref": ref_id,
            "value": value,
            "selector": selector,
        });
        self.call(run_id, "browser_select_option", params).await
    }

    /// `browser_scroll`: scroll the page or a specific container.
    pub async fn scroll(
        &self,
        run_id: &str,
        direction: Option<String>,
        amount: Option<u64>,
        ref_id: Option<String>,
    ) -> Result<Value, String> {
        let params = json!({
            "direction": direction,
            "amount": amount,
            "ref": ref_id,
        });
        self.call(run_id, "browser_scroll", params).await
    }
}

fn param_value<'a>(params: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| params.get(*name))
}

fn param_string(params: &Value, names: &[&str]) -> Option<String> {
    param_value(params, names)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn param_u64(params: &Value, names: &[&str]) -> Option<u64> {
    param_value(params, names).and_then(Value::as_u64)
}

fn param_bool(params: &Value, names: &[&str]) -> Option<bool> {
    param_value(params, names).and_then(Value::as_bool)
}

const MAX_INLINE_SCREENSHOT_BYTES: u64 = 16 * 1024 * 1024;

fn screenshot_data_from_result(value: &Value) -> Option<String> {
    if let Some(screenshot) = value.get("screenshot").and_then(Value::as_str) {
        if screenshot.starts_with("data:") {
            return Some(screenshot.to_string());
        }
    }

    if let Some(base64) = value
        .get("base64")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        if base64.len() as u64 > (MAX_INLINE_SCREENSHOT_BYTES * 4 / 3) {
            log::warn!(
                "[browser] inline screenshot is too large to keep in the inspector: {} bytes",
                base64.len()
            );
            return None;
        }
        let mime_type = value
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or("image/png");
        return Some(format!("data:{mime_type};base64,{base64}"));
    }

    let path = value
        .get("path")
        .or_else(|| value.get("outputPath"))
        .and_then(Value::as_str)?;
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > MAX_INLINE_SCREENSHOT_BYTES {
        log::warn!(
            "[browser] screenshot is too large to inline for the inspector: {} bytes",
            metadata.len()
        );
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let mime_type = match std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    Some(format!(
        "data:{mime_type};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn parse_action_meta(
    method: &str,
    params: &Value,
) -> (
    crate::work::models::BrowserActionType,
    String,
    Option<String>,
    Option<String>,
) {
    use crate::work::models::BrowserActionType;
    match method {
        "browser_navigate" => {
            let url = param_string(params, &["url"]);
            let desc = format!("导航至 {}", url.as_deref().unwrap_or(""));
            (BrowserActionType::Navigate, desc, url, None)
        }
        "browser_eval_navigate" => {
            let url = param_string(params, &["url"]);
            let desc = format!("导航至评测 fixture {}", url.as_deref().unwrap_or(""));
            (BrowserActionType::Navigate, desc, url, None)
        }
        "browser_eval_tabs" => (
            BrowserActionType::Tabs,
            "评测 fixture 标签页操作".to_string(),
            None,
            None,
        ),
        "browser_cdp_observe" => (
            BrowserActionType::Snapshot,
            "读取外部 Electron 页面快照".to_string(),
            None,
            None,
        ),
        "browser_cdp_act" => (
            BrowserActionType::Custom,
            "操作外部 Electron 页面".to_string(),
            None,
            None,
        ),
        "browser_snapshot" => (
            BrowserActionType::Snapshot,
            "读取网页可访问性快照与语义树".to_string(),
            None,
            None,
        ),
        "browser_take_screenshot" => (
            BrowserActionType::Screenshot,
            "捕获当前页面截图".to_string(),
            None,
            None,
        ),
        "browser_wait_for" => {
            let expect = params.get("expect").filter(|value| value.is_object());
            let label = expect
                .and_then(|value| value.get("text_contains"))
                .and_then(Value::as_str)
                .map(|text| format!("等待页面出现「{}」", text))
                .or_else(|| {
                    expect
                        .and_then(|value| value.get("url_contains"))
                        .and_then(Value::as_str)
                        .map(|url| format!("验证页面地址包含 {}", url))
                })
                .unwrap_or_else(|| {
                    format!(
                        "等待页面加载或延时 ({}ms)",
                        param_u64(params, &["ms"]).unwrap_or(0)
                    )
                });
            (BrowserActionType::WaitFor, label, None, None)
        }
        "browser_tabs" => {
            let action = param_string(params, &["action"]).unwrap_or_else(|| "list".to_string());
            (
                BrowserActionType::Tabs,
                format!("标签页操作: {}", action),
                None,
                None,
            )
        }
        "browser_close" => (
            BrowserActionType::Close,
            "关闭浏览器会话".to_string(),
            None,
            None,
        ),
        "browser_click" => {
            let sel =
                param_string(params, &["selector"]).or_else(|| param_string(params, &["ref"]));
            let label = param_string(params, &["target_label"]);
            let desc = label
                .map(|label| format!("点击「{}」", label))
                .unwrap_or_else(|| format!("点击元素: {}", sel.as_deref().unwrap_or("")));
            (BrowserActionType::Click, desc, None, sel)
        }
        "browser_type" => {
            let sel =
                param_string(params, &["selector"]).or_else(|| param_string(params, &["ref"]));
            let label = param_string(params, &["target_label"]);
            let desc = label
                .map(|label| format!("在「{}」中输入文本", label))
                .unwrap_or_else(|| format!("在元素 {} 中输入文本", sel.as_deref().unwrap_or("")));
            (BrowserActionType::Type, desc, None, sel)
        }
        "browser_select_option" => {
            let sel =
                param_string(params, &["selector"]).or_else(|| param_string(params, &["ref"]));
            (
                BrowserActionType::SelectOption,
                format!("在元素 {} 中选择选项", sel.as_deref().unwrap_or("")),
                None,
                sel,
            )
        }
        "browser_scroll" => {
            let dir = param_string(params, &["direction"]).unwrap_or_else(|| "down".to_string());
            (
                BrowserActionType::Scroll,
                format!("页面滚动: {}", dir),
                None,
                None,
            )
        }
        other => (
            BrowserActionType::Custom,
            format!("执行浏览器操作: {}", other),
            None,
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_params_are_merged_into_worker_calls() {
        crate::work::browser_operator::embedded_registry().register(
            crate::work::browser_operator::EmbeddedRegistration {
                endpoint: "127.0.0.1:59999".to_string(),
                token: "tok_x".to_string(),
                targets: vec![crate::work::browser_operator::EmbeddedTarget {
                    target_id: "T9".to_string(),
                    run_id: Some("run-embedded".to_string()),
                    url: "https://example.com".to_string(),
                }],
            },
        );

        let target = crate::work::browser_operator::embedded_registry()
            .resolve("run-embedded")
            .expect("registered target should resolve");
        let merged = with_embedded_target(target, json!({ "url": "https://a.test" }));
        assert_eq!(
            merged.get("cdpEndpoint").and_then(Value::as_str),
            Some("127.0.0.1:59999")
        );
        assert_eq!(
            merged.get("cdpToken").and_then(Value::as_str),
            Some("tok_x")
        );
        assert_eq!(
            merged.get("cdpTargetId").and_then(Value::as_str),
            Some("T9")
        );
        assert_eq!(
            merged.get("url").and_then(Value::as_str),
            Some("https://a.test")
        );
    }

    #[test]
    fn screenshot_worker_result_is_normalized_for_the_inspector() {
        let result = screenshot_data_from_result(&json!({
            "base64": "aGVsbG8=",
            "mimeType": "image/png",
        }))
        .expect("inline screenshot should be normalized");
        assert_eq!(result, "data:image/png;base64,aGVsbG8=");
    }

    #[tokio::test]
    async fn test_browser_worker_ping_and_lifecycle() {
        let mgr = BrowserOperatorManager::new();
        // Ping should succeed without starting a browser surface.
        let pong = mgr.call("test-run-1", "ping", json!({})).await;
        assert!(pong.is_ok(), "Ping failed: {:?}", pong.err());

        // Browser operations without an Electron relay fail closed.
        let result = mgr.tabs("test-run-1", "list", None, None).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Embedded browser relay was not registered"));
        mgr.reload_worker();
    }

    #[tokio::test]
    async fn test_browser_worker_run_isolation_and_tab_operations() {
        let mgr = BrowserOperatorManager::new();

        // 1. Session A operations
        let run_a = "run-isolation-a";
        let tabs_a = mgr
            .execute(run_a, "browser_tabs", json!({"action": "list"}))
            .await;

        let err_msg = tabs_a.expect_err("standalone browser operations must fail closed");
        assert!(
            err_msg.contains("Embedded browser relay was not registered"),
            "Unexpected error: {err_msg}"
        );
        mgr.reload_worker();
    }

    #[tokio::test]
    async fn execute_records_a_trace_for_work_browser_calls() {
        let mgr = BrowserOperatorManager::new();
        let run_id = "run-trace-recording";
        let result = mgr
            .execute(run_id, "browser_tabs", json!({ "action": "list" }))
            .await;

        if result.is_ok() {
            let session = super::super::session::browser_session_manager()
                .get_session(run_id)
                .await
                .expect("execute should create a browser session");
            assert_eq!(session.traces.len(), 1);
            assert_eq!(session.traces[0].status, "success");
        }

        let _ = mgr.close_context(run_id).await;
        let _ = super::super::session::browser_session_manager()
            .close_session(run_id)
            .await;
    }

    #[tokio::test]
    async fn test_legacy_browser_tool_names_are_rejected() {
        let mgr = BrowserOperatorManager::new();
        for name in [
            "work_browser_navigate",
            "work_browser_snapshot",
            "work_browser_screenshot",
            "work_browser_wait",
            "work_browser_select",
            "browser_screenshot",
            "browser_wait",
            "browser_select",
        ] {
            let error = mgr
                .execute("legacy-name-test", name, json!({}))
                .await
                .expect_err(name);
            assert_eq!(error, format!("Unknown browser tool: {name}"));
        }
    }

    #[tokio::test]
    async fn test_browser_worker_ssrf_rejection_at_manager_level() {
        let mgr = BrowserOperatorManager::new();
        let blocked = mgr
            .navigate("test-run-ssrf", "http://127.0.0.1:8080", None)
            .await;
        assert!(blocked.is_err(), "Should reject SSRF target: {:?}", blocked);

        let blocked_file = mgr
            .navigate("test-run-ssrf", "file:///etc/passwd", None)
            .await;
        assert!(
            blocked_file.is_err(),
            "Should reject file:// scheme: {:?}",
            blocked_file
        );
    }
}
