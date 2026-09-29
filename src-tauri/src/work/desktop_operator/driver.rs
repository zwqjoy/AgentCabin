use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

use crate::process_ext::HideConsole;

const PROTOCOL_VERSION: i64 = 6;
const START_TIMEOUT: Duration = Duration::from_secs(8);
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const ARCHITECTURE_VERSION: i64 = 1;
const REQUIRED_INVARIANTS: [&str; 7] = [
    "state-scoped-observations",
    "bounded-observation-history",
    "multi-root-forest",
    "progressive-disclosure",
    "atomic-physical-input",
    "concurrent-requests",
    "transactional-batching",
];

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopOperatorStatus {
    pub enabled: bool,
    pub ready: bool,
    pub owner_run_id: Option<String>,
    pub command: String,
    pub message: String,
    pub accessibility: Option<bool>,
    pub screen_recording: Option<bool>,
}

#[derive(Debug, Clone)]
struct DesktopTarget {
    pid: i64,
    window_id: Option<i64>,
    root_ref: Option<String>,
    app_name: String,
    title: String,
}

type PendingRequests = Arc<
    std::sync::Mutex<
        std::collections::HashMap<u64, tokio::sync::oneshot::Sender<Result<Value, String>>>,
    >,
>;

struct UpstreamWorker {
    child: Child,
    stdin: tokio::process::ChildStdin,
    pending: PendingRequests,
}

#[derive(Clone)]
pub struct DesktopOperatorManager {
    helper_path: PathBuf,
    socket_path: PathBuf,
    child: Arc<Mutex<Option<Child>>>,
    startup_lock: Arc<Mutex<()>>,
    request_id: Arc<AtomicU64>,
    desktop_lease: Arc<Mutex<Option<String>>>,
    upstream_worker: Arc<Mutex<Option<UpstreamWorker>>>,
}

impl Default for DesktopOperatorManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopOperatorManager {
    pub fn new() -> Self {
        Self {
            helper_path: Self::resolve_helper_path(),
            socket_path: std::env::temp_dir().join(format!(
                "agentcabin-computer-use-{}.sock",
                std::process::id()
            )),
            child: Arc::new(Mutex::new(None)),
            startup_lock: Arc::new(Mutex::new(())),
            request_id: Arc::new(AtomicU64::new(1)),
            desktop_lease: Arc::new(Mutex::new(None)),
            upstream_worker: Arc::new(Mutex::new(None)),
        }
    }

    pub fn active_engine() -> &'static str {
        match std::env::var("AGENTCABIN_COMPUTER_USE_ENGINE")
            .as_deref()
            .map(str::trim)
        {
            Ok("legacy") => "legacy",
            _ => "upstream",
        }
    }

    fn resolve_upstream_runtime_script() -> Result<PathBuf, String> {
        if let Ok(path) = std::env::var("AGENTCABIN_PI_COMPUTER_USE_RUNTIME") {
            let p = PathBuf::from(path);
            if p.is_file() {
                return Ok(p);
            }
        }
        let dev_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/work/pi_computer_use_runtime.mjs");
        if dev_path.is_file() {
            return Ok(dev_path);
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let candidate1 = parent.join("../Resources/runtime/pi_computer_use_runtime.mjs");
                if candidate1.is_file() {
                    return Ok(candidate1);
                }
                let candidate2 = parent.join("runtime/pi_computer_use_runtime.mjs");
                if candidate2.is_file() {
                    return Ok(candidate2);
                }
            }
        }
        let cache_dir = std::env::temp_dir().join("agentcabin-computer-use-runtime");
        let _ = std::fs::create_dir_all(&cache_dir);
        let target = cache_dir.join("pi_computer_use_runtime.mjs");
        let shim_target = cache_dir.join("pi_coding_agent_shim.mjs");
        let _ = std::fs::write(&target, include_str!("../pi_computer_use_runtime.mjs"));
        let _ = std::fs::write(&shim_target, include_str!("../pi_coding_agent_shim.mjs"));
        if target.is_file() {
            return Ok(target);
        }
        Err("Failed to resolve pi_computer_use_runtime.mjs".to_string())
    }

    async fn ensure_upstream_worker(&self) -> Result<(), String> {
        let mut guard = self.upstream_worker.lock().await;
        if let Some(worker) = guard.as_mut() {
            match worker.child.try_wait() {
                Ok(None) => return Ok(()),
                _ => {
                    *guard = None;
                }
            }
        }

        let node_bin =
            crate::agent::runtime_locator::resolve_node().unwrap_or_else(|_| "node".to_string());
        let runtime_script = Self::resolve_upstream_runtime_script()?;

        // 1. Ensure AgentCabin native helper is running and verified on macOS
        if cfg!(target_os = "macos") {
            if !self.helper_available() {
                return Err(format!(
                    "agentcabin-computer-use macOS bridge is missing: {}",
                    self.helper_path.display()
                ));
            }
            self.ensure_started()
                .await
                .map_err(|e| format!("Failed to start AgentCabin native helper: {e}"))?;
        }

        let ext_dir = crate::agent::runtime_locator::extension_node_modules_dirs()
            .into_iter()
            .find(|d| d.join("@injaneity/pi-computer-use").is_dir())
            .or_else(|| {
                let cur = std::env::current_dir().ok()?;
                let candidate = cur.join("src-tauri/runtime/extensions/node_modules");
                if candidate.join("@injaneity/pi-computer-use").is_dir() {
                    Some(candidate)
                } else {
                    None
                }
            });

        let mut cmd = Command::new(&node_bin);
        cmd.arg(&runtime_script)
            .arg("--serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .hide_console();

        if let Some(ext) = ext_dir {
            cmd.env("AGENTCABIN_PI_EXTENSIONS_DIR", ext);
        }

        // 2. Inject AgentCabin native helper socket path so upstream never installs/launches com.injaneity.pi-computer-use
        cmd.env("PI_CU_SOCKET_PATH", &self.socket_path);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn upstream computer use runtime: {e}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to capture stdin for upstream runtime".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to capture stdout for upstream runtime".to_string())?;

        let pending: PendingRequests =
            Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let pending_clone = Arc::clone(&pending);

        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let reader = tokio::io::BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(resp) = serde_json::from_str::<Value>(line) {
                    if let Some(id) = resp.get("id").and_then(Value::as_u64) {
                        let tx = pending_clone.lock().unwrap().remove(&id);
                        if let Some(tx) = tx {
                            if resp.get("ok").and_then(Value::as_bool) == Some(true) {
                                let _ =
                                    tx.send(Ok(resp.get("result").cloned().unwrap_or(Value::Null)));
                            } else {
                                let err = resp
                                    .get("error")
                                    .and_then(Value::as_str)
                                    .unwrap_or("Upstream error");
                                let _ = tx.send(Err(err.to_string()));
                            }
                        }
                    }
                }
            }
            let mut map = pending_clone.lock().unwrap();
            for (_, tx) in map.drain() {
                let _ = tx.send(Err("Upstream worker process terminated".to_string()));
            }
        });

        *guard = Some(UpstreamWorker {
            child,
            stdin,
            pending,
        });

        Ok(())
    }

    pub async fn cancel_upstream_execution(&self, tool_call_id: &str) -> Result<(), String> {
        let mut guard = self.upstream_worker.lock().await;
        if let Some(worker) = guard.as_mut() {
            let req_id = self.request_id.fetch_add(1, Ordering::SeqCst);
            let payload = json!({
                "id": req_id,
                "method": "cancel",
                "toolCallId": tool_call_id,
            });
            let mut line = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
            line.push(b'\n');
            let _ = worker.stdin.write_all(&line).await;
            let _ = worker.stdin.flush().await;
        }
        Ok(())
    }

    pub async fn cancel_run_execution(&self, run_id: &str) -> Result<(), String> {
        let mut guard = self.upstream_worker.lock().await;
        if let Some(worker) = guard.as_mut() {
            let req_id = self.request_id.fetch_add(1, Ordering::SeqCst);
            let payload = json!({
                "id": req_id,
                "method": "cancel_run",
                "runId": run_id,
            });
            let mut line = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
            line.push(b'\n');
            let _ = worker.stdin.write_all(&line).await;
            let _ = worker.stdin.flush().await;
        }
        Ok(())
    }

    async fn send_upstream_request(&self, method: &str, mut args: Value) -> Result<Value, String> {
        self.ensure_upstream_worker().await?;
        let req_id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = tokio::sync::oneshot::channel();

        let tool_call_id = if method == "execute" {
            let id = args
                .get("toolCallId")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("cu-call-{}", uuid::Uuid::new_v4()));
            if let Value::Object(ref mut map) = args {
                map.insert("toolCallId".to_string(), json!(&id));
            }
            Some(id)
        } else {
            args.get("toolCallId")
                .and_then(Value::as_str)
                .map(str::to_string)
        };

        let mut guard = self.upstream_worker.lock().await;
        let worker = guard
            .as_mut()
            .ok_or_else(|| "Upstream worker unavailable".to_string())?;

        worker.pending.lock().unwrap().insert(req_id, tx);

        if let Value::Object(ref mut map) = args {
            map.insert("id".to_string(), json!(req_id));
            map.insert("method".to_string(), json!(method));
        } else {
            args = json!({
                "id": req_id,
                "method": method,
                "params": args,
            });
        }

        let mut line = serde_json::to_vec(&args).map_err(|e| e.to_string())?;
        line.push(b'\n');

        if let Err(e) = worker.stdin.write_all(&line).await {
            worker.pending.lock().unwrap().remove(&req_id);
            *guard = None;
            return Err(format!("Failed to write to upstream worker: {e}"));
        }
        let _ = worker.stdin.flush().await;
        drop(guard);

        match timeout(Duration::from_secs(60), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("Upstream worker channel dropped".to_string()),
            Err(_) => {
                if let Ok(guard) = self.upstream_worker.try_lock() {
                    if let Some(worker) = guard.as_ref() {
                        worker.pending.lock().unwrap().remove(&req_id);
                    }
                }
                if let Some(ref call_id) = tool_call_id {
                    let _ = self.cancel_upstream_execution(call_id).await;
                }
                Err(format!("Upstream request '{method}' timed out after 60s"))
            }
        }
    }

    fn resolve_helper_path() -> PathBuf {
        if let Some(path) = std::env::var("AGENTCABIN_COMPUTER_USE_BRIDGE")
            .ok()
            .map(PathBuf::from)
            .filter(|path| path.is_file())
        {
            return path;
        }
        let mut candidates = Vec::new();
        if let Ok(executable) = std::env::current_exe() {
            if let Some(parent) = executable.parent() {
                for relative in [
                    "../Resources/resources/agentcabin-computer-use/macos/agentcabin-computer-use.app/Contents/MacOS/bridge",
                    "../Resources/agentcabin-computer-use/macos/agentcabin-computer-use.app/Contents/MacOS/bridge",
                    "../../resources/agentcabin-computer-use/macos/agentcabin-computer-use.app/Contents/MacOS/bridge",
                    "../../../resources/agentcabin-computer-use/macos/agentcabin-computer-use.app/Contents/MacOS/bridge",
                    "../resources/agentcabin-computer-use/macos/agentcabin-computer-use.app/Contents/MacOS/bridge",
                    "../Resources/resources/agentcabin-computer-use/macos/bridge",
                    "../Resources/agentcabin-computer-use/macos/bridge",
                    "../../resources/agentcabin-computer-use/macos/bridge",
                    "../../../resources/agentcabin-computer-use/macos/bridge",
                    "../resources/agentcabin-computer-use/macos/bridge",
                ] {
                    candidates.push(parent.join(relative));
                }
            }
        }
        if let Ok(current_dir) = std::env::current_dir() {
            candidates.push(
                current_dir.join(
                    "src-tauri/resources/agentcabin-computer-use/macos/agentcabin-computer-use.app/Contents/MacOS/bridge",
                ),
            );
            candidates
                .push(current_dir.join("src-tauri/resources/agentcabin-computer-use/macos/bridge"));
            candidates.push(
                current_dir.join(
                    "resources/agentcabin-computer-use/macos/agentcabin-computer-use.app/Contents/MacOS/bridge",
                ),
            );
            candidates.push(current_dir.join("resources/agentcabin-computer-use/macos/bridge"));
        }
        candidates
            .into_iter()
            .find(|path| path.is_file())
            .unwrap_or_else(|| PathBuf::from("agentcabin-computer-use-bridge"))
    }

    pub fn helper_available(&self) -> bool {
        cfg!(target_os = "macos") && self.helper_path.is_file()
    }

    #[cfg(unix)]
    async fn request_once(&self, cmd: &str, args: Value, limit: Duration) -> Result<Value, String> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|error| format!("agentcabin-computer-use socket connect failed: {error}"))?;
        let id = format!(
            "agentcabin_{}",
            self.request_id.fetch_add(1, Ordering::Relaxed)
        );
        let mut request = match args {
            Value::Object(object) => object,
            _ => Map::new(),
        };
        request.insert("id".to_string(), json!(id));
        request.insert("cmd".to_string(), json!(cmd));
        let mut encoded = serde_json::to_vec(&request)
            .map_err(|error| format!("agentcabin-computer-use request encoding failed: {error}"))?;
        encoded.push(b'\n');
        timeout(limit, stream.write_all(&encoded))
            .await
            .map_err(|_| {
                format!("agentcabin-computer-use command '{cmd}' timed out while writing")
            })?
            .map_err(|error| format!("agentcabin-computer-use socket write failed: {error}"))?;

        let mut line = Vec::new();
        let deadline = tokio::time::Instant::now() + limit;
        let mut chunk = [0_u8; 8192];
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(format!("agentcabin-computer-use command '{cmd}' timed out"));
            }
            let read = timeout(remaining, stream.read(&mut chunk))
                .await
                .map_err(|_| format!("agentcabin-computer-use command '{cmd}' timed out"))?
                .map_err(|error| format!("agentcabin-computer-use socket read failed: {error}"))?;
            if read == 0 {
                return Err(
                    "agentcabin-computer-use socket closed before a response arrived".to_string(),
                );
            }
            line.extend_from_slice(&chunk[..read]);
            if line.len() > MAX_RESPONSE_BYTES {
                return Err(format!(
                    "agentcabin-computer-use response exceeded {MAX_RESPONSE_BYTES} bytes"
                ));
            }
            if let Some(newline) = line.iter().position(|byte| *byte == b'\n') {
                line.truncate(newline);
                break;
            }
        }
        let response: Value = serde_json::from_slice(&line)
            .map_err(|error| format!("agentcabin-computer-use returned invalid JSON: {error}"))?;
        if response.get("id").and_then(Value::as_str) != Some(id.as_str()) {
            return Err(
                "agentcabin-computer-use response id did not match the request".to_string(),
            );
        }
        if response.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(response.get("result").cloned().unwrap_or(Value::Null));
        }
        let error = response
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("native bridge command failed");
        let code = response
            .pointer("/error/code")
            .and_then(Value::as_str)
            .unwrap_or("native_error");
        Err(format!("agentcabin-computer-use {code}: {error}"))
    }

    #[cfg(not(unix))]
    async fn request_once(&self, cmd: &str, args: Value, limit: Duration) -> Result<Value, String> {
        let _ = (cmd, args, limit);
        Err("agentcabin-computer-use native bridge is unavailable on this platform".to_string())
    }

    async fn ensure_started(&self) -> Result<(), String> {
        if let Ok(diagnostics) = self
            .request_once("diagnostics", json!({}), Duration::from_secs(1))
            .await
        {
            return Self::validate_diagnostics(&diagnostics);
        }
        let _guard = self.startup_lock.lock().await;
        if let Ok(diagnostics) = self
            .request_once("diagnostics", json!({}), Duration::from_secs(1))
            .await
        {
            return Self::validate_diagnostics(&diagnostics);
        }
        if !self.helper_available() {
            return Err(format!(
                "agentcabin-computer-use macOS bridge is missing: {}",
                self.helper_path.display()
            ));
        }
        if let Some(mut old) = self.child.lock().await.take() {
            let _ = old.kill().await;
        }
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path).map_err(|error| {
                format!("failed to remove stale agentcabin-computer-use socket: {error}")
            })?;
        }
        let bundle = self
            .helper_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .filter(|p| p.extension().is_some_and(|ext| ext == "app"));
        // LaunchServices gives the helper its own TCC identity. Executing the
        // binary inside the bundle directly inherits the development host's
        // responsibility instead of using the grant for the helper app.
        let mut command = if let Some(bundle) = bundle {
            let mut command = Command::new("/usr/bin/open");
            command.arg("-n").arg("-g").arg(bundle).arg("--args");
            command
        } else {
            Command::new(&self.helper_path)
        };
        let child = command
            .arg("serve")
            .arg("--socket")
            .arg(&self.socket_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .hide_console()
            .spawn()
            .map_err(|error| format!("failed to start agentcabin-computer-use bridge: {error}"))?;
        *self.child.lock().await = Some(child);
        let deadline = tokio::time::Instant::now() + START_TIMEOUT;
        while tokio::time::Instant::now() < deadline {
            if let Ok(diagnostics) = self
                .request_once("diagnostics", json!({}), Duration::from_secs(1))
                .await
            {
                return Self::validate_diagnostics(&diagnostics);
            }
            sleep(Duration::from_millis(100)).await;
        }
        Err("agentcabin-computer-use macOS bridge did not become ready".to_string())
    }

    async fn request(&self, cmd: &str, args: Value) -> Result<Value, String> {
        self.ensure_started().await?;
        self.request_once(cmd, args, CALL_TIMEOUT).await
    }

    fn validate_diagnostics(diagnostics: &Value) -> Result<(), String> {
        let protocol = diagnostics
            .get("protocolVersion")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        if protocol != PROTOCOL_VERSION {
            return Err(format!(
                "agentcabin-computer-use protocol mismatch: expected {PROTOCOL_VERSION}, got {protocol}"
            ));
        }
        let architecture = diagnostics
            .get("architectureVersion")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        if architecture != ARCHITECTURE_VERSION {
            return Err(format!(
                "agentcabin-computer-use architecture mismatch: expected {ARCHITECTURE_VERSION}, got {architecture}"
            ));
        }
        let invariants = diagnostics
            .get("invariants")
            .and_then(Value::as_array)
            .ok_or_else(|| "agentcabin-computer-use diagnostics omitted invariants".to_string())?;
        let has_invariant = |required: &str| {
            invariants
                .iter()
                .any(|value| value.as_str() == Some(required))
        };
        if let Some(missing) = REQUIRED_INVARIANTS
            .iter()
            .find(|required| !has_invariant(required))
        {
            return Err(format!(
                "agentcabin-computer-use diagnostics omitted required invariant '{missing}'"
            ));
        }
        Ok(())
    }

    async fn restart_for_permission_refresh(&self) -> Result<(), String> {
        let lease = self.desktop_lease.lock().await;
        if lease.is_some() {
            return Err(
                "Desktop control is in use; refresh after the session finishes.".to_string(),
            );
        }
        let _guard = self.startup_lock.lock().await;

        // TCC permission decisions are cached by the client process. Ask the
        // current bridge to exit, then discard the tracked child and socket so
        // the next ensure_started launches a fresh TCC client.
        let _ = self
            .request_once("shutdown", json!({}), Duration::from_secs(1))
            .await;
        // `open -W` is the tracked child for bundles; wait for the helper's
        // asynchronous shutdown before allowing a replacement on this socket.
        #[cfg(unix)]
        {
            for _ in 0..20 {
                if UnixStream::connect(&self.socket_path).await.is_err() {
                    break;
                }
                sleep(Duration::from_millis(50)).await;
            }
        }
        #[cfg(not(unix))]
        return Err(
            "agentcabin-computer-use permission refresh is unavailable on this platform"
                .to_string(),
        );
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path).map_err(|error| {
                format!("failed to remove agentcabin-computer-use socket after restart: {error}")
            })?;
        }
        drop(_guard);
        self.ensure_started().await
    }

    async fn claim(&self, run_id: &str) -> Result<(), String> {
        let mut lease = self.desktop_lease.lock().await;
        if let Some(owner) = lease.as_deref() {
            if owner == run_id {
                return Ok(());
            }
            let is_active = crate::work::internal_bridge::is_run_active(owner).await
                || crate::desktop_runtime::is_run_active(owner).await;
            if !is_active {
                *lease = Some(run_id.to_string());
                return Ok(());
            }
            return Err(format!(
                "Desktop control belongs to session '{owner}'. Wait for it to finish."
            ));
        }
        *lease = Some(run_id.to_string());
        Ok(())
    }

    async fn require_owner(&self, run_id: &str) -> Result<(), String> {
        let mut lease = self.desktop_lease.lock().await;
        if let Some(owner) = lease.as_deref() {
            if owner == run_id {
                return Ok(());
            }
            let is_active = crate::work::internal_bridge::is_run_active(owner).await
                || crate::desktop_runtime::is_run_active(owner).await;
            if !is_active {
                *lease = Some(run_id.to_string());
                return Ok(());
            }
            return Err(format!("Desktop control belongs to session '{owner}'."));
        }
        *lease = Some(run_id.to_string());
        Ok(())
    }

    fn roots(result: &Value) -> Vec<Value> {
        result
            .get("roots")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    fn root_matches(root: &Value, name: &str, bundle_id: &str) -> bool {
        let app = root.get("appName").and_then(Value::as_str).unwrap_or("");
        let bundle = root.get("bundleId").and_then(Value::as_str).unwrap_or("");
        (!bundle_id.is_empty() && bundle.eq_ignore_ascii_case(bundle_id))
            || (!name.is_empty() && app.to_lowercase().contains(&name.to_lowercase()))
    }

    fn target_from_root(root: &Value) -> Option<DesktopTarget> {
        let pid = root.get("pid")?.as_i64()?;
        Some(DesktopTarget {
            pid,
            window_id: root.get("windowId").and_then(Value::as_i64),
            root_ref: root
                .get("rootRef")
                .or_else(|| root.get("windowRef"))
                .and_then(Value::as_str)
                .map(str::to_string),
            app_name: root
                .get("appName")
                .and_then(Value::as_str)
                .unwrap_or("Unknown App")
                .to_string(),
            title: root
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
    }

    fn disambiguate_candidates<'a>(candidates: &[&'a Value]) -> Option<&'a Value> {
        if candidates.is_empty() {
            return None;
        }
        if candidates.len() == 1 {
            return Some(candidates[0]);
        }

        // 1. Filter for visible (on-screen and not minimized)
        let visible: Vec<&'a Value> = candidates
            .iter()
            .copied()
            .filter(|r| {
                let on_screen = r.get("isOnscreen").and_then(Value::as_bool).unwrap_or(true);
                let minimized = r
                    .get("isMinimized")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                on_screen && !minimized
            })
            .collect();
        let pool: &[&'a Value] = if !visible.is_empty() {
            &visible
        } else {
            candidates
        };

        if pool.len() == 1 {
            return Some(pool[0]);
        }

        // 2. Focused window
        let focused: Vec<&'a Value> = pool
            .iter()
            .copied()
            .filter(|r| r.get("isFocused").and_then(Value::as_bool) == Some(true))
            .collect();
        if focused.len() == 1 {
            return Some(focused[0]);
        }

        // 3. Main window
        let main_windows: Vec<&'a Value> = pool
            .iter()
            .copied()
            .filter(|r| r.get("isMain").and_then(Value::as_bool) == Some(true))
            .collect();
        if main_windows.len() == 1 {
            return Some(main_windows[0]);
        }

        // 4. Topmost window by lowest zOrder
        let min_z = pool
            .iter()
            .filter_map(|r| r.get("zOrder").and_then(Value::as_i64))
            .min();
        if let Some(min_z_val) = min_z {
            let topmost: Vec<&'a Value> = pool
                .iter()
                .copied()
                .filter(|r| r.get("zOrder").and_then(Value::as_i64) == Some(min_z_val))
                .collect();
            if topmost.len() == 1 {
                return Some(topmost[0]);
            }
        }

        None
    }

    /// Resolve every observation or mutation from its explicit root identity.
    /// The manager deliberately keeps no process-wide "current target": two
    /// roots in one session must not silently redirect one another.
    async fn target_from_arguments(&self, args: &Value) -> Result<DesktopTarget, String> {
        let pid = args
            .get("pid")
            .and_then(Value::as_i64)
            .filter(|pid| *pid > 0)
            .ok_or_else(|| "Computer Use action requires an explicit target pid".to_string())?;
        let window_id = args
            .get("window_id")
            .or_else(|| args.get("windowId"))
            .and_then(Value::as_i64)
            .filter(|window_id| *window_id > 0);
        let root_ref = args
            .get("root_ref")
            .or_else(|| args.get("rootRef"))
            .and_then(Value::as_str)
            .filter(|root_ref| !root_ref.is_empty());
        let roots = self.list_roots(Some(pid)).await?;
        let pid_roots: Vec<&Value> = roots
            .iter()
            .filter(|root| root.get("pid").and_then(Value::as_i64) == Some(pid))
            .collect();

        // Pass 1: exact match on both window_id and root_ref
        let exact_matches: Vec<&Value> = pid_roots
            .iter()
            .copied()
            .filter(|root| {
                let same_window = window_id.is_none()
                    || root.get("windowId").and_then(Value::as_i64) == window_id;
                let same_root = root_ref.is_none()
                    || root
                        .get("rootRef")
                        .and_then(Value::as_str)
                        .is_some_and(|candidate| Some(candidate) == root_ref);
                same_window && same_root
            })
            .collect();

        let mut selected = Self::disambiguate_candidates(&exact_matches);

        // Pass 2: If root_ref became stale/re-keyed, try matching solely by window_id
        if selected.is_none() {
            if let Some(wid) = window_id {
                let window_matches: Vec<&Value> = pid_roots
                    .iter()
                    .copied()
                    .filter(|root| root.get("windowId").and_then(Value::as_i64) == Some(wid))
                    .collect();
                selected = Self::disambiguate_candidates(&window_matches);
            }
        }

        // Pass 3: If still unresolved (e.g. window_id was not set or changed), disambiguate across all pid_roots
        if selected.is_none() {
            selected = Self::disambiguate_candidates(&pid_roots);
        }

        selected.and_then(Self::target_from_root).ok_or_else(|| {
            format!("Target root is stale or ambiguous for pid {pid}; observe the exact root again")
        })
    }

    async fn list_roots(&self, pid: Option<i64>) -> Result<Vec<Value>, String> {
        let args = pid
            .map(|pid| json!({"pid": pid}))
            .unwrap_or_else(|| json!({}));
        Ok(Self::roots(&self.request("listRoots", args).await?))
    }

    async fn launch(&self, run_id: &str, args: &Value) -> Result<Value, String> {
        let name = args
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let bundle_id = args
            .get("bundle_id")
            .or_else(|| args.get("bundleId"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if name.is_empty() && bundle_id.is_empty() {
            return Err("launch_app requires name or bundleId".to_string());
        }
        let permissions = self.request("checkPermissions", json!({})).await?;
        if permissions.get("accessibility").and_then(Value::as_bool) != Some(true)
            || permissions.get("screenRecording").and_then(Value::as_bool) != Some(true)
        {
            return Err(format!(
                "desktop_permission_required: accessibility={}, screenRecording={}. Authorize the current helper in AgentCabin Computer Control settings. Stop desktop actions until permissions are restored; do not retry via shell, AppleScript, alternative apps or virtual machines.",
                permissions.get("accessibility").unwrap_or(&Value::Null),
                permissions.get("screenRecording").unwrap_or(&Value::Null),
            ));
        }
        // Resolve the installed app before launching: localized AX app names
        // (e.g. 计算器) need not match LaunchServices names (Calculator).
        let resolved;
        let bundle_id = if bundle_id.is_empty() {
            resolved = self
                .request("resolveApplication", json!({"name": name}))
                .await?;
            resolved
                .get("bundleId")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or_else(|| "Application resolution omitted bundleId".to_string())?
        } else {
            bundle_id
        };
        let new_instance = args
            .get("createsNewApplicationInstance")
            .or_else(|| args.get("creates_new_application_instance"))
            .or_else(|| args.get("new_instance"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.claim(run_id).await?;
        let mut command = Command::new("/usr/bin/open");
        if new_instance {
            command.arg("-n");
        }
        if !bundle_id.is_empty() {
            command.arg("-b").arg(bundle_id);
        } else {
            command.arg("-a").arg(name);
        }
        let output = command
            .output()
            .await
            .map_err(|error| format!("failed to launch application: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        for attempt in 0..30 {
            let roots = self.list_roots(None).await?;
            let matching_roots: Vec<&Value> = roots
                .iter()
                .filter(|root| Self::root_matches(root, name, bundle_id))
                .collect();

            if !matching_roots.is_empty() {
                let ready = matching_roots
                    .iter()
                    .copied()
                    .find(|root| {
                        let on_screen = root
                            .get("isOnscreen")
                            .and_then(Value::as_bool)
                            .unwrap_or(true);
                        let focused = root
                            .get("isFocused")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        let main = root.get("isMain").and_then(Value::as_bool).unwrap_or(false);
                        let w = root
                            .pointer("/framePoints/w")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0);
                        let h = root
                            .pointer("/framePoints/h")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0);
                        on_screen && (focused || main) && w >= 100.0 && h >= 80.0
                    })
                    .or_else(|| {
                        if attempt >= 10 {
                            matching_roots.iter().copied().find(|root| {
                                let on_screen = root
                                    .get("isOnscreen")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(true);
                                let w = root
                                    .pointer("/framePoints/w")
                                    .and_then(Value::as_f64)
                                    .unwrap_or(0.0);
                                let h = root
                                    .pointer("/framePoints/h")
                                    .and_then(Value::as_f64)
                                    .unwrap_or(0.0);
                                on_screen && w >= 100.0 && h >= 80.0
                            })
                        } else {
                            None
                        }
                    })
                    .or_else(|| {
                        if attempt >= 20 {
                            matching_roots.first().copied()
                        } else {
                            None
                        }
                    });

                if let Some(target) = ready.and_then(Self::target_from_root) {
                    let result = json!({
                        "backend": "agentcabin-computer-use",
                        "pid": target.pid,
                        "window_id": target.window_id,
                        "root_ref": target.root_ref,
                        "app_name": target.app_name,
                        "title": target.title,
                    });
                    return Ok(result);
                }
            }
            sleep(Duration::from_millis(100)).await;
        }
        Err(format!(
            "'{name}' launched but no controllable window appeared"
        ))
    }

    fn flatten_outline(node: &Value, output: &mut Vec<Value>) {
        if let Some(object) = node.as_object() {
            if object.get("ref").and_then(Value::as_str).is_some() {
                output.push(json!({
                    "ref": object.get("ref"),
                    "role": object.get("role"),
                    "subrole": object.get("subrole"),
                    "identifier": object.get("identifier"),
                    "title": object.get("title"),
                    "description": object.get("description"),
                    "value": object.get("value"),
                    "actions": object.get("actions"),
                    "canPress": object.get("canPress"),
                    "canFocus": object.get("canFocus"),
                    "canSetValue": object.get("canSetValue"),
                    "isTextInput": object.get("isTextInput"),
                    "rect": object.get("rect"),
                    "focused": object.get("focused"),
                }));
            }
            if let Some(children) = object.get("children").and_then(Value::as_array) {
                for child in children {
                    Self::flatten_outline(child, output);
                }
            }
        }
    }

    async fn observe_target(
        &self,
        target: &DesktopTarget,
    ) -> Result<(Value, Option<String>), String> {
        let look = self
            .request(
                "look",
                json!({
                    "windowId": target.window_id,
                    "windowRef": target.root_ref,
                    "readText": "auto",
                    "includeImage": true,
                    "maxDimension": 1800,
                }),
            )
            .await?;
        let look_id = look
            .get("lookId")
            .and_then(Value::as_str)
            .ok_or_else(|| "agentcabin-computer-use look response omitted lookId".to_string())?
            .to_string();
        let mut elements = Vec::new();
        Self::flatten_outline(look.get("outline").unwrap_or(&Value::Null), &mut elements);
        let structured = json!({
            "backend": "agentcabin-computer-use",
            "pid": target.pid,
            "window_id": look.pointer("/window/windowId").and_then(Value::as_i64).or(target.window_id),
            "root_ref": look.pointer("/window/rootRef").and_then(Value::as_str).or(target.root_ref.as_deref()),
            "observation_id": look_id,
            "app_name": target.app_name,
            "title": target.title,
            "elements": elements,
            "outline": look.get("outline"),
            "capture": look.get("window"),
        });
        let image = look
            .pointer("/image/jpegBase64")
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok((structured, image))
    }

    fn tool_result(text: &str, structured: Value, image: Option<String>) -> Value {
        let mut content = vec![json!({"type": "text", "text": text})];
        if let Some(data) = image {
            content.push(json!({"type": "image", "data": data, "mimeType": "image/jpeg"}));
        }
        json!({"content": content, "structuredContent": structured})
    }

    async fn observe(&self, run_id: &str, args: &Value) -> Result<Value, String> {
        self.claim(run_id).await?;
        let target = self.target_from_arguments(args).await?;
        let (structured, image) = self.observe_target(&target).await?;
        Ok(Self::tool_result("Observed native UI", structured, image))
    }

    fn action_request(
        tool_name: &str,
        args: &Value,
        target: &DesktopTarget,
    ) -> Result<Value, String> {
        let look_id = args
            .get("observation_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "Action requires a current observation.".to_string())?;
        let action_target =
            if let Some(reference) = args.get("element_token").and_then(Value::as_str) {
                json!({"ref": reference})
            } else if let (Some(x), Some(y)) = (
                args.get("x").and_then(Value::as_f64),
                args.get("y").and_then(Value::as_f64),
            ) {
                json!({"x": x, "y": y})
            } else {
                json!({})
            };
        let (action, params) = match tool_name {
            "desktop_click" => (
                args.get("native_action")
                    .and_then(Value::as_str)
                    .filter(|value| matches!(*value, "press" | "click"))
                    .unwrap_or("click"),
                json!({
                    "button": args.get("button").and_then(Value::as_str).unwrap_or("left"),
                    "clickCount": args.get("count").and_then(Value::as_i64).unwrap_or(1),
                }),
            ),
            "desktop_type" => (
                args.get("native_action")
                    .and_then(Value::as_str)
                    .filter(|value| matches!(*value, "setText" | "typeText"))
                    .unwrap_or("typeText"),
                json!({"text": args.get("text").and_then(Value::as_str).unwrap_or("")}),
            ),
            "desktop_key" => {
                let mut keys = args
                    .get("modifiers")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                keys.push(json!(args.get("key").and_then(Value::as_str).unwrap_or("")));
                ("keypress", json!({"keys": keys}))
            }
            "desktop_scroll" => {
                let direction = args
                    .get("direction")
                    .and_then(Value::as_str)
                    .unwrap_or("down");
                let amount = args.get("amount").and_then(Value::as_i64).unwrap_or(6) * 40;
                let (x, y) = match direction {
                    "up" => (0, -amount),
                    "left" => (-amount, 0),
                    "right" => (amount, 0),
                    _ => (0, amount),
                };
                ("scroll", json!({"scrollX": x, "scrollY": y}))
            }
            _ => return Err(format!("Unsupported native action: {tool_name}")),
        };
        Ok(json!({
            "lookId": look_id,
            "pid": target.pid,
            "target": action_target,
            "policy": "default",
            "action": action,
            "params": params,
            "cursorOverlay": true,
        }))
    }

    async fn act(&self, run_id: &str, tool_name: &str, args: &Value) -> Result<Value, String> {
        self.require_owner(run_id).await?;
        let target = self.target_from_arguments(args).await?;
        let request = Self::action_request(tool_name, args, &target)?;
        let action_result = self.request("act", request).await?;
        let (observation, image) = self.observe_target(&target).await?;
        let outcome = action_result
            .get("outcome")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let structured = json!({
            "backend": "agentcabin-computer-use",
            "verification": {"effect": if outcome == "worked" { "confirmed" } else { "unverifiable" }, "outcome": outcome},
            "execution": action_result,
            "observation": observation,
        });
        Ok(Self::tool_result(
            &format!("Native action outcome={outcome}"),
            structured,
            image,
        ))
    }

    async fn act_batch(&self, run_id: &str, args: &Value) -> Result<Value, String> {
        self.require_owner(run_id).await?;
        let target = self.target_from_arguments(args).await?;
        let actions = args
            .get("actions")
            .and_then(Value::as_array)
            .ok_or_else(|| "desktop_act_batch requires actions".to_string())?;
        if actions.is_empty() || actions.len() > 20 {
            return Err("desktop_act_batch requires 1-20 actions".to_string());
        }
        let mut native_actions = Vec::with_capacity(actions.len());
        for action in actions {
            let tool = action
                .get("tool")
                .and_then(Value::as_str)
                .ok_or_else(|| "desktop_act_batch action omitted tool".to_string())?;
            let params = action.get("params").cloned().unwrap_or_else(|| json!({}));
            native_actions.push(Self::action_request(tool, &params, &target)?);
        }
        let action_result = self
            .request("actBatch", json!({"actions": native_actions}))
            .await?;
        let (observation, image) = self.observe_target(&target).await?;
        let outcome = action_result
            .get("outcome")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let structured = json!({
            "backend": "agentcabin-computer-use",
            "verification": {"effect": if outcome == "worked" { "confirmed" } else { "unverifiable" }, "outcome": outcome},
            "execution": action_result,
            "observation": observation,
        });
        Ok(Self::tool_result(
            &format!("Native transaction outcome={outcome}"),
            structured,
            image,
        ))
    }

    pub async fn execute(
        &self,
        run_id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value, String> {
        if !cfg!(target_os = "macos") {
            return Err("Computer Use macOS backend is unavailable on this platform.".to_string());
        }
        if Self::active_engine() == "legacy" {
            return self.legacy_execute(run_id, tool_name, arguments).await;
        }

        match tool_name {
            "desktop_release" => {
                self.release_for_run(run_id).await;
                Ok(Self::tool_result(
                    "Desktop control released",
                    json!({"backend": "upstream", "released": true}),
                    None,
                ))
            }
            "find_roots" | "observe_ui" | "search_ui" | "expand_ui" | "inspect_ui" | "act_ui"
            | "read_text" | "wait_for" | "launch_browser" | "navigate_browser"
            | "evaluate_browser" => {
                let tool_call_id = arguments
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("cu-call-{}", uuid::Uuid::new_v4()));

                self.claim(run_id).await?;
                let res = self
                    .send_upstream_request(
                        "execute",
                        json!({
                            "runId": run_id,
                            "toolName": tool_name,
                            "toolCallId": tool_call_id,
                            "params": arguments,
                        }),
                    )
                    .await?;

                if res.get("success").and_then(Value::as_bool) == Some(true) {
                    let duration_ms = res.get("durationMs").and_then(Value::as_u64).unwrap_or(0);
                    let inner_result = res.get("result").cloned().unwrap_or(Value::Null);
                    Ok(json!({
                        "backend": "upstream",
                        "tool": tool_name,
                        "durationMs": duration_ms,
                        "result": inner_result,
                    }))
                } else {
                    let err_msg = res
                        .get("stderr")
                        .and_then(Value::as_str)
                        .unwrap_or("Upstream tool execution failed");
                    Err(err_msg.to_string())
                }
            }
            "launch_app" | "desktop_open_app" => {
                let launch_res = self.launch(run_id, &arguments).await?;
                let app_name = launch_res
                    .get("app_name")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let bundle_id = arguments
                    .get("bundle_id")
                    .or_else(|| arguments.get("bundleId"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let query_param = if !app_name.is_empty() {
                    json!({ "app": app_name })
                } else if !bundle_id.is_empty() {
                    json!({ "bundleId": bundle_id })
                } else {
                    json!({})
                };

                let mut observation = Value::Null;
                if let Ok(roots_val) = self
                    .send_upstream_request(
                        "execute",
                        json!({
                            "runId": run_id,
                            "toolName": "find_roots",
                            "params": query_param,
                        }),
                    )
                    .await
                {
                    let target_root = roots_val
                        .pointer("/result/details/windows")
                        .and_then(Value::as_array)
                        .and_then(|w| w.first())
                        .or_else(|| {
                            roots_val
                                .pointer("/result/windows")
                                .and_then(Value::as_array)
                                .and_then(|w| w.first())
                        });

                    if let Some(w) = target_root {
                        if let Some(r_ref) = w
                            .get("windowRef")
                            .or_else(|| w.get("root"))
                            .and_then(Value::as_str)
                        {
                            if let Ok(obs_val) = self
                                .send_upstream_request(
                                    "execute",
                                    json!({
                                        "runId": run_id,
                                        "toolName": "observe_ui",
                                        "params": { "root": r_ref },
                                    }),
                                )
                                .await
                            {
                                if let Some(res) = obs_val.get("result") {
                                    observation = res.clone();
                                }
                            }
                        }
                    }
                }

                let text_summary = format!("Application '{}' launched successfully.", app_name);
                let mut content = vec![json!({"type": "text", "text": text_summary})];
                if let Some(obs_content) = observation.get("content").and_then(Value::as_array) {
                    content.extend(obs_content.iter().cloned());
                }

                Ok(json!({
                    "backend": "upstream",
                    "tool": tool_name,
                    "durationMs": 0,
                    "result": {
                        "content": content,
                        "launch": launch_res,
                        "observation": observation,
                    },
                }))
            }
            "desktop_list_apps" | "desktop_observe" | "desktop_screenshot" | "desktop_click"
            | "desktop_type" | "desktop_key" | "desktop_scroll" | "desktop_act_batch" => {
                self.legacy_execute(run_id, tool_name, arguments).await
            }
            other => Err(format!("Unknown desktop tool: {other}")),
        }
    }

    pub async fn legacy_execute(
        &self,
        run_id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value, String> {
        match tool_name {
            "desktop_list_apps" => {
                let roots = self.list_roots(None).await?;
                let windows = roots
                    .iter()
                    .map(|root| {
                        json!({
                            "window_id": root.get("windowId"),
                            "pid": root.get("pid"),
                            "owner": root.get("appName"),
                            "title": root.get("title"),
                            "on_screen": root.get("isOnscreen"),
                            "x": root.pointer("/framePoints/x"),
                            "y": root.pointer("/framePoints/y"),
                            "width": root.pointer("/framePoints/w"),
                            "height": root.pointer("/framePoints/h"),
                            "root_ref": root.get("rootRef"),
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(Self::tool_result(
                    &format!("Found {} desktop roots", windows.len()),
                    json!({"backend": "agentcabin-computer-use", "windows": windows}),
                    None,
                ))
            }
            "desktop_open_app" => {
                let structured = self.launch(run_id, &arguments).await?;
                Ok(Self::tool_result("Application launched", structured, None))
            }
            "desktop_observe" | "desktop_screenshot" => self.observe(run_id, &arguments).await,
            "desktop_click" | "desktop_type" | "desktop_key" | "desktop_scroll" => {
                self.act(run_id, tool_name, &arguments).await
            }
            "desktop_act_batch" => self.act_batch(run_id, &arguments).await,
            "desktop_release" => {
                self.release_for_run(run_id).await;
                Ok(Self::tool_result(
                    "Desktop control released",
                    json!({"backend": "agentcabin-computer-use", "released": true}),
                    None,
                ))
            }
            other => Err(format!("Unknown native desktop method: {other}")),
        }
    }

    pub async fn release_for_run(&self, run_id: &str) {
        let mut lease = self.desktop_lease.lock().await;
        if lease.as_deref() == Some(run_id) {
            *lease = None;
        }
        drop(lease);
        if Self::active_engine() != "legacy" {
            let _ = self.cancel_run_execution(run_id).await;
            let _ = self
                .send_upstream_request("release", json!({"runId": run_id}))
                .await;
        }
    }

    pub async fn request_permissions(&self) -> Result<DesktopOperatorStatus, String> {
        if Self::active_engine() == "legacy" {
            self.request(
                "registerPermissions",
                json!({"promptAccessibility": true, "promptScreenRecording": true}),
            )
            .await?;
            return Ok(self.status().await);
        }
        let _ = self
            .send_upstream_request("request_permissions", json!({}))
            .await?;
        Ok(self.status().await)
    }

    pub async fn refresh_status(&self) -> DesktopOperatorStatus {
        if Self::active_engine() == "legacy" {
            let current = self.legacy_status().await;
            if current.ready
                && (current.screen_recording != Some(true) || current.accessibility != Some(true))
                && self.restart_for_permission_refresh().await.is_ok()
            {
                return self.legacy_status().await;
            }
            return current;
        }
        let current = self.status().await;
        if (current.screen_recording != Some(true) || current.accessibility != Some(true))
            && self.restart_for_permission_refresh().await.is_ok()
        {
            if let Ok(mut guard) = self.upstream_worker.try_lock() {
                if let Some(mut worker) = guard.take() {
                    let _ = worker.child.kill().await;
                }
            }
            return self.status().await;
        }
        current
    }

    pub async fn open_permission_pane(&self, kind: &str) -> Result<(), String> {
        if Self::active_engine() == "legacy" {
            let native_kind = match kind {
                "accessibility" => "accessibility",
                "screen-recording" | "screenRecording" => "screenRecording",
                _ => return Err(format!("Unknown desktop permission kind: {kind}")),
            };
            let _ = self
                .request(
                    "registerPermissions",
                    json!({"promptAccessibility": false, "promptScreenRecording": true}),
                )
                .await;
            self.request("openPermissionPane", json!({"kind": native_kind}))
                .await?;
            return Ok(());
        }
        self.send_upstream_request("open_permission_pane", json!({"kind": kind}))
            .await?;
        Ok(())
    }

    pub async fn status(&self) -> DesktopOperatorStatus {
        if Self::active_engine() == "legacy" {
            return self.legacy_status().await;
        }
        let default_cmd = self.helper_path.to_string_lossy().into_owned();
        match self.send_upstream_request("status", json!({})).await {
            Ok(res) => {
                let enabled = res.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                let ready = res.get("ready").and_then(Value::as_bool).unwrap_or(false);
                let command = res
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or(&default_cmd)
                    .to_string();
                let message = res
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let accessibility = res.get("accessibility").and_then(Value::as_bool);
                let screen_recording = res.get("screenRecording").and_then(Value::as_bool);
                DesktopOperatorStatus {
                    enabled: crate::work::desktop_operator::is_requested() && enabled,
                    ready,
                    owner_run_id: self.desktop_lease.lock().await.clone(),
                    command,
                    message,
                    accessibility,
                    screen_recording,
                }
            }
            Err(e) => DesktopOperatorStatus {
                enabled: crate::work::desktop_operator::is_requested(),
                ready: false,
                owner_run_id: self.desktop_lease.lock().await.clone(),
                command: default_cmd,
                message: format!("agentcabin-computer-use status error: {e}"),
                accessibility: Some(false),
                screen_recording: Some(false),
            },
        }
    }

    pub async fn legacy_status(&self) -> DesktopOperatorStatus {
        let diagnostics = if self.helper_available() && self.ensure_started().await.is_ok() {
            self.request("diagnostics", json!({})).await.ok()
        } else {
            None
        };
        let permissions = if diagnostics.is_some() {
            self.request("checkPermissions", json!({})).await.ok()
        } else {
            None
        };
        let ready = diagnostics
            .as_ref()
            .is_some_and(|value| Self::validate_diagnostics(value).is_ok());
        let accessibility = permissions
            .as_ref()
            .and_then(|value| value.get("accessibility"))
            .and_then(Value::as_bool)
            .or_else(|| {
                diagnostics
                    .as_ref()
                    .and_then(|value| value.get("accessibility"))
                    .and_then(Value::as_bool)
            });
        let screen_recording = permissions
            .as_ref()
            .and_then(|value| value.get("screenRecording"))
            .and_then(Value::as_bool)
            .or_else(|| {
                diagnostics
                    .as_ref()
                    .and_then(|value| value.get("screenRecording"))
                    .and_then(Value::as_bool)
            });
        DesktopOperatorStatus {
            enabled: crate::work::desktop_operator::is_requested(),
            ready,
            owner_run_id: self.desktop_lease.lock().await.clone(),
            command: self.helper_path.to_string_lossy().into_owned(),
            message: if ready {
                if accessibility == Some(true) && screen_recording == Some(true) {
                    "agentcabin-computer-use macOS bridge 与系统权限均已就绪。".to_string()
                } else {
                    "agentcabin-computer-use macOS bridge 已启动；请完成辅助功能与屏幕录制授权。"
                        .to_string()
                }
            } else {
                format!(
                    "agentcabin-computer-use macOS bridge 未就绪：{}",
                    self.helper_path.display()
                )
            },
            accessibility,
            screen_recording,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flattens_native_outline_without_changing_wire_refs() {
        let outline = json!({
            "ref": "ax:1",
            "role": "AXWindow",
            "children": [{"ref": "ax:2", "role": "AXButton", "title": "7", "children": []}]
        });
        let mut elements = Vec::new();
        DesktopOperatorManager::flatten_outline(&outline, &mut elements);
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[1].get("ref").and_then(Value::as_str), Some("ax:2"));
    }

    #[test]
    fn action_uses_state_owned_native_ref() {
        let target = DesktopTarget {
            pid: 42,
            window_id: Some(7),
            root_ref: Some("w1".to_string()),
            app_name: "Calculator".to_string(),
            title: String::new(),
        };
        let request = DesktopOperatorManager::action_request(
            "desktop_click",
            &json!({"observation_id": "look_1", "element_token": "ax:2"}),
            &target,
        )
        .unwrap();
        assert_eq!(
            request.pointer("/target/ref").and_then(Value::as_str),
            Some("ax:2")
        );
        assert_eq!(
            request.get("lookId").and_then(Value::as_str),
            Some("look_1")
        );
    }

    #[test]
    fn diagnostics_require_protocol_architecture_and_all_invariants() {
        let diagnostics = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "architectureVersion": ARCHITECTURE_VERSION,
            "invariants": REQUIRED_INVARIANTS,
        });
        assert!(DesktopOperatorManager::validate_diagnostics(&diagnostics).is_ok());

        let missing_invariant = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "architectureVersion": ARCHITECTURE_VERSION,
            "invariants": ["state-scoped-observations"],
        });
        assert!(
            DesktopOperatorManager::validate_diagnostics(&missing_invariant)
                .unwrap_err()
                .contains("bounded-observation-history")
        );
    }

    #[test]
    fn disambiguates_candidates_by_visibility_and_focus() {
        let background = json!({
            "pid": 94187,
            "windowId": 101,
            "rootRef": "w1",
            "isOnscreen": false,
            "isFocused": false,
            "isMain": false,
            "zOrder": 10
        });
        let visible_unfocused = json!({
            "pid": 94187,
            "windowId": 102,
            "rootRef": "w2",
            "isOnscreen": true,
            "isFocused": false,
            "isMain": false,
            "zOrder": 5
        });
        let active_tab = json!({
            "pid": 94187,
            "windowId": 103,
            "rootRef": "w3",
            "isOnscreen": true,
            "isFocused": true,
            "isMain": true,
            "zOrder": 0
        });

        // Multi-candidate with one focused active window selects the active window
        let candidates = vec![&background, &visible_unfocused, &active_tab];
        let selected = DesktopOperatorManager::disambiguate_candidates(&candidates);
        assert_eq!(
            selected.unwrap().get("windowId").and_then(Value::as_i64),
            Some(103)
        );

        // When none are focused, selects main window
        let active_unfocused = json!({
            "pid": 94187,
            "windowId": 103,
            "rootRef": "w3",
            "isOnscreen": true,
            "isFocused": false,
            "isMain": true,
            "zOrder": 0
        });
        let candidates2 = vec![&background, &visible_unfocused, &active_unfocused];
        let selected2 = DesktopOperatorManager::disambiguate_candidates(&candidates2);
        assert_eq!(
            selected2.unwrap().get("windowId").and_then(Value::as_i64),
            Some(103)
        );

        // When neither is main nor focused, selects lowest zOrder on screen
        let candidates3 = vec![&background, &visible_unfocused];
        let selected3 = DesktopOperatorManager::disambiguate_candidates(&candidates3);
        assert_eq!(
            selected3.unwrap().get("windowId").and_then(Value::as_i64),
            Some(102)
        );

        // Truly ambiguous (two identical visible windows with same zOrder) returns None
        let twin1 = json!({
            "pid": 94187,
            "windowId": 201,
            "isOnscreen": true,
            "isFocused": false,
            "isMain": false,
            "zOrder": 2
        });
        let twin2 = json!({
            "pid": 94187,
            "windowId": 202,
            "isOnscreen": true,
            "isFocused": false,
            "isMain": false,
            "zOrder": 2
        });
        let ambiguous_candidates = vec![&twin1, &twin2];
        let selected_ambiguous =
            DesktopOperatorManager::disambiguate_candidates(&ambiguous_candidates);
        assert!(selected_ambiguous.is_none());
    }

    #[test]
    fn upstream_engine_defaults_and_resolves_script() {
        assert_eq!(DesktopOperatorManager::active_engine(), "upstream");
        let script = DesktopOperatorManager::resolve_upstream_runtime_script();
        assert!(
            script.is_ok(),
            "resolve_upstream_runtime_script should succeed: {:?}",
            script
        );
        let path = script.unwrap();
        assert!(
            path.is_file(),
            "resolved script must exist: {}",
            path.display()
        );
    }

    #[test]
    fn helper_available_under_upstream_engine() {
        let manager = DesktopOperatorManager::new();
        if cfg!(target_os = "macos") {
            assert!(manager.helper_available());
        }
    }

    #[tokio::test]
    async fn live_upstream_status_and_find_roots() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let manager = DesktopOperatorManager::new();
        let status = manager.status().await;
        println!("Upstream status: {:?}", status);
        assert!(status.enabled);
        if status.ready {
            let roots = manager
                .execute("test-run", "find_roots", serde_json::json!({}))
                .await;
            println!("find_roots result: {:?}", roots);
            if status.accessibility == Some(true) && status.screen_recording == Some(true) {
                assert!(roots.is_ok());
            } else {
                assert!(roots.is_err());
                let err = roots.unwrap_err();
                assert!(
                    err.contains("Accessibility")
                        || err.contains("Screen Recording")
                        || err.contains("interactive")
                        || err.contains("helper")
                );
            }
        }
    }

    #[tokio::test]
    async fn fail_closed_when_helper_missing() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let mut manager = DesktopOperatorManager::new();
        manager.helper_path = PathBuf::from("/non/existent/path/to/agentcabin-computer-use");
        let result = manager.ensure_upstream_worker().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("agentcabin-computer-use macOS bridge is missing"),
            "Expected missing bridge error, got: {err}"
        );
    }

    #[test]
    fn protocol_mismatch_fails_closed() {
        let bad_proto = json!({
            "protocolVersion": 5,
            "architectureVersion": ARCHITECTURE_VERSION,
            "invariants": REQUIRED_INVARIANTS,
        });
        let err = DesktopOperatorManager::validate_diagnostics(&bad_proto).unwrap_err();
        assert!(err.contains("protocol mismatch: expected 6, got 5"));
    }

    #[test]
    fn architecture_mismatch_fails_closed() {
        let bad_arch = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "architectureVersion": 2,
            "invariants": REQUIRED_INVARIANTS,
        });
        let err = DesktopOperatorManager::validate_diagnostics(&bad_arch).unwrap_err();
        assert!(err.contains("architecture mismatch: expected 1, got 2"));
    }

    #[tokio::test]
    async fn desktop_lease_mutual_exclusion_and_release() {
        let manager = DesktopOperatorManager::new();
        let bridge_state = crate::work::internal_bridge::bridge_state();
        let token_a = format!("token-{}", uuid::Uuid::new_v4());
        bridge_state.tokens.write().await.insert(
            token_a.clone(),
            crate::work::internal_bridge::ProcessBridgeTokenInfo {
                token: token_a.clone(),
                run_id: "run_A".to_string(),
                task_id: Some("task_A".to_string()),
                workspace_id: "ws_A".to_string(),
                execution_context: crate::work::models::ExecutionContext::Attended,
                proxy_url: None,
            },
        );

        // Run A claims
        assert!(manager.claim("run_A").await.is_ok());

        // Run B claims -> denied while run A is active
        let denied = manager.claim("run_B").await;
        assert!(denied.is_err());
        assert!(denied.unwrap_err().contains("belongs to session 'run_A'"));

        // When run A becomes inactive, run B can take over
        bridge_state.tokens.write().await.remove(&token_a);
        assert!(manager.claim("run_B").await.is_ok());

        // Release run B
        manager.release_for_run("run_B").await;
        assert!(manager.desktop_lease.lock().await.is_none());
    }

    #[test]
    fn upstream_worker_uses_agentcabin_helper_socket() {
        let manager = DesktopOperatorManager::new();
        assert!(
            manager
                .socket_path
                .to_string_lossy()
                .contains("agentcabin-computer-use-"),
            "Socket path must be agentcabin-owned"
        );
    }
}
