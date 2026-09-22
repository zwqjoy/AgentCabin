use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::agent::adapter::ActorSessionMap;
use crate::agent::control::CliInfoCache;
use crate::agent::spawn_locks::SpawnLocks;
use crate::agent::stream::ProcessMap;
use crate::storage::events::EventWriter;
use crate::web_server::broadcaster::{BroadcastEmitter, EventBroadcaster};
use tokio_util::sync::CancellationToken;

/// Session entry for cookie-based HTTP authentication
#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub id: String,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub token_version: u64,
}

/// Aggregated application state shared between Tauri IPC and axum web server.
/// All fields are Arc-wrapped for cheap cloning.
#[derive(Clone)]
pub struct AppState {
    pub process_map: ProcessMap,
    pub sessions: ActorSessionMap,
    pub writer: Arc<EventWriter>,
    pub spawn_locks: SpawnLocks,
    pub cancel_token: CancellationToken,
    pub cli_info_cache: CliInfoCache,
    pub emitter: Arc<BroadcastEmitter>,
    pub broadcaster: EventBroadcaster,

    /// Authentication token for web server access (hot-swappable for rotation)
    pub token: Arc<tokio::sync::RwLock<String>>,
    /// Token version — incremented on token rotation to invalidate sessions
    pub token_version: Arc<std::sync::atomic::AtomicU64>,
    /// WS shutdown broadcast — token rotation triggers disconnect of all WS clients
    pub ws_shutdown: Arc<tokio::sync::broadcast::Sender<()>>,
    /// HTTP session store (cookie → session entry)
    pub http_sessions: Arc<Mutex<HashMap<String, SessionEntry>>>,
    /// Effective port after binding (may differ from config if port was busy)
    pub effective_port: Arc<std::sync::atomic::AtomicU16>,
    /// Bind address
    pub bind_addr: Arc<String>,
    /// Allowed origins for CORS
    pub allowed_origins: Option<Vec<String>>,
    /// Whether generic run methods may touch Work-mode runs. The desktop
    /// transports (Tauri IPC, Electron via the headless core server) allow it;
    /// the remotely reachable web server rejects Work runs
    /// (`dispatch_legacy::reject_work_runs_on_web`).
    pub allow_work: bool,
}
