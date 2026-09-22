//! CoreRuntime — shared application state without a Tauri AppHandle.
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::Arc;

use rand::Rng;
use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;

use crate::agent::adapter::{new_actor_session_map, ActorSessionMap};
use crate::agent::codex_control::CodexInfoCache;
use crate::agent::control::CliInfoCache;
use crate::agent::spawn_locks::SpawnLocks;
use crate::agent::stream::new_process_map;
use crate::storage::events::EventWriter;
use crate::web_server::broadcaster::{BroadcastEmitter, EventBroadcaster};
use crate::web_server::state::AppState;

/// Host-agnostic runtime: the same shared state Tauri `manage()`s, held directly.
pub struct CoreRuntime {
    pub process_map: crate::agent::stream::ProcessMap,
    pub sessions: ActorSessionMap,
    pub writer: Arc<EventWriter>,
    pub spawn_locks: SpawnLocks,
    pub cancel_token: CancellationToken,
    pub cli_info_cache: CliInfoCache,
    pub codex_info_cache: CodexInfoCache,
    pub broadcaster: EventBroadcaster,
    pub emitter: Arc<BroadcastEmitter>,
    pub token: Arc<RwLock<String>>,
    pub token_version: Arc<AtomicU64>,
    pub ws_shutdown: Arc<broadcast::Sender<()>>,
    pub effective_port: Arc<AtomicU16>,
}

impl Default for CoreRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreRuntime {
    /// Initialize shared state and run the same startup housekeeping as the Tauri path.
    pub fn new() -> Self {
        let cancel_token = CancellationToken::new();
        let writer = crate::storage::events::global_writer();
        let broadcaster = EventBroadcaster::new();
        let emitter = Arc::new(BroadcastEmitter::new_headless(
            writer.clone(),
            broadcaster.clone(),
        ));

        let token: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        let (ws_shutdown, _) = broadcast::channel(1);

        Self {
            process_map: new_process_map(),
            sessions: new_actor_session_map(),
            writer,
            spawn_locks: SpawnLocks::new(),
            cancel_token,
            cli_info_cache: CliInfoCache::new(),
            codex_info_cache: CodexInfoCache::new(),
            broadcaster,
            emitter,
            token: Arc::new(RwLock::new(token)),
            token_version: Arc::new(AtomicU64::new(0)),
            ws_shutdown: Arc::new(ws_shutdown),
            effective_port: Arc::new(AtomicU16::new(0)),
        }
    }

    /// Startup housekeeping shared with the Tauri `run()` path.
    pub fn startup(&self) {
        crate::process_ext::setup_job_kill_on_close();
        crate::storage::runs::reconcile_orphaned_runs();
        if let Err(error) =
            crate::work::tasks::reconcile_active_runs(&crate::work::paths::WorkPaths::app())
        {
            log::error!("[work/lifecycle] Startup reconciliation failed: {error}");
        }
        if let Err(error) =
            crate::storage::profile_bindings::migrate_legacy_pi_code_profile_if_needed()
        {
            log::warn!("[pi/profile] Legacy Code Pi profile migration skipped: {error}");
        }
        crate::storage::profile_bindings::migrate_legacy_skills_if_needed().ok();
        if let Err(error) = crate::work::twork_migration::sync_from_twork_if_present() {
            log::warn!("[twork/sync] Startup sync from T-Work skipped or partial: {error}");
        }
        crate::hooks::setup::cleanup_hook_bridge();
        crate::web_server::broadcaster::register_shared_emitter(self.emitter.clone());
        crate::work::browser_operator::browser_session_manager()
            .set_event_emitter(self.emitter.clone());
    }

    /// Spawn background bridges (work, browser, desktop, code connector).
    pub fn spawn_bridges(&self) {
        tokio::spawn(async {
            if let Err(e) = crate::work::internal_bridge::start_internal_bridge().await {
                log::error!("[work/bridge] Failed to start internal bridge: {e}");
            }
        });
        tokio::spawn(async {
            if let Err(e) = crate::browser_runtime::start_bridge().await {
                log::error!("[browser/bridge] Failed to start browser bridge: {e}");
            }
        });
        tokio::spawn(async {
            if let Err(e) = crate::desktop_runtime::start_bridge().await {
                log::error!("[desktop/bridge] Failed to start desktop bridge: {e}");
            }
        });
        tokio::spawn(async {
            if let Err(e) = crate::code_connector_runtime::start_bridge().await {
                log::error!("[code/connector-bridge] Failed to start Code Connector bridge: {e}");
            }
        });
    }

    /// Start the WorkScheduler (scheduled tasks).
    pub fn spawn_scheduler(&self) {
        let scheduler =
            crate::work::scheduler::WorkScheduler::new(crate::work::paths::WorkPaths::app());
        scheduler.spawn(
            self.emitter.clone(),
            self.sessions.clone(),
            self.spawn_locks.clone(),
            self.cancel_token.clone(),
        );
    }

    /// Prime the shell PATH cache off the hot path (same as Tauri setup).
    pub fn prime_path_cache(&self) {
        std::thread::spawn(crate::agent::claude_stream::prime_path_cache);
    }

    /// Build an `AppState` snapshot for dispatch / web-server handlers.
    pub fn app_state(
        &self,
        bind_addr: Arc<String>,
        allowed_origins: Option<Vec<String>>,
    ) -> AppState {
        AppState {
            process_map: self.process_map.clone(),
            sessions: self.sessions.clone(),
            writer: self.writer.clone(),
            spawn_locks: self.spawn_locks.clone(),
            cancel_token: self.cancel_token.clone(),
            cli_info_cache: self.cli_info_cache.clone(),
            emitter: self.emitter.clone(),
            broadcaster: self.broadcaster.clone(),
            token: self.token.clone(),
            token_version: self.token_version.clone(),
            ws_shutdown: self.ws_shutdown.clone(),
            http_sessions: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            effective_port: self.effective_port.clone(),
            bind_addr,
            allowed_origins,
            // The headless core server IS the desktop transport (Electron P4):
            // generic run methods must be able to load Work runs, matching the
            // Tauri IPC path. The remotely reachable web server keeps this false.
            allow_work: true,
        }
    }

    /// Dispatch a method call through the existing web-server command router.
    pub async fn invoke(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        // Work Mode commands are Tauri-IPC-only and never reach the legacy
        // web dispatcher; route them through the headless bridge instead so
        // the Electron renderer (P4) can drive Work via core-server /invoke.
        if method.starts_with("work_") {
            if let Some(result) =
                crate::web_server::dispatch_work::dispatch(method, &params, self).await
            {
                return result;
            }
        }

        let state = self.app_state(Arc::new("127.0.0.1".into()), None);
        crate::web_server::dispatch::dispatch_command(method, params, &state).await
    }

    /// Graceful shutdown: cancel token + drain actors + kill stream processes + kill PTYs.
    pub async fn shutdown(&self) {
        self.cancel_token.cancel();
        crate::commands::pty::cleanup_all_pty_sessions();
        self.graceful_shutdown_actors().await;
    }

    async fn graceful_shutdown_actors(&self) {
        use crate::agent::session_actor::ActorCommand;

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);

        loop {
            let count = self.sessions.lock().await.len();
            if count == 0 {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                log::warn!("[core] graceful shutdown: {count} actors still alive, force stopping");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        let remaining: Vec<_> = {
            let mut map = self.sessions.lock().await;
            map.drain().collect()
        };
        for (run_id, handle) in remaining {
            log::debug!("[core] force stopping actor: {run_id}");
            let (reply_tx, _reply_rx) = tokio::sync::oneshot::channel();
            let _ = handle.cmd_tx.try_send(ActorCommand::Stop {
                reason: crate::agent::session_actor::RuntimeStopReason::AppShutdown,
                reply: reply_tx,
            });
            let abort = handle.join_handle.abort_handle();
            match tokio::time::timeout(std::time::Duration::from_secs(2), handle.join_handle).await
            {
                Ok(Ok(())) => log::debug!("[core] actor {run_id} exited cleanly"),
                Ok(Err(e)) => log::warn!("[core] actor {run_id} join error: {e}"),
                Err(_) => {
                    log::warn!("[core] actor {run_id} did not exit in 2s, aborting");
                    abort.abort();
                }
            }
        }

        let to_kill = match tokio::time::timeout(std::time::Duration::from_secs(1), async {
            let mut map = self.process_map.lock().await;
            map.drain().collect::<Vec<_>>()
        })
        .await
        {
            Ok(vec) => vec,
            Err(_) => {
                log::warn!("[core] ProcessMap lock timeout during shutdown");
                Vec::new()
            }
        };
        for (run_id, mut child) in to_kill {
            log::debug!("[core] killing stream process {run_id}");
            let _ = child.kill().await;
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await;
        }

        log::info!("[core] graceful shutdown complete");
    }

    pub async fn token(&self) -> String {
        self.token.read().await.clone()
    }

    pub fn port(&self) -> u16 {
        self.effective_port.load(Ordering::Relaxed)
    }
}
