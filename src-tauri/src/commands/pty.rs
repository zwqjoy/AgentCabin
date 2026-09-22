//! PTY session management for interactive terminal support.
//!
//! Each PTY session spawns a real shell process (zsh/bash/cmd) attached to a
//! pseudo-terminal. The frontend xterm.js sends keystrokes via `pty_write`,
//! and the backend reader thread streams output back via the `pty_data` event.

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, State};

/// A live PTY session backed by a real shell process.
pub struct PtySession {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    kill_flag: Arc<AtomicBool>,
}

/// Global registry: session_id -> active PTY session.
pub type PtyRegistry = Arc<Mutex<HashMap<String, PtySession>>>;

static GLOBAL_REGISTRY: OnceLock<PtyRegistry> = OnceLock::new();

pub fn global_registry() -> PtyRegistry {
    GLOBAL_REGISTRY
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

#[derive(Serialize, Clone)]
pub struct PtyDataEvent {
    pub session_id: String,
    pub data: String,
}

#[derive(Serialize, Clone)]
pub struct PtyExitEvent {
    pub session_id: String,
    pub exit_code: Option<i32>,
}

fn emit_pty_data(session_id: String, data: String, app: Option<&AppHandle>) {
    let payload = PtyDataEvent { session_id, data };
    if let Some(emitter) = crate::web_server::broadcaster::shared_emitter() {
        emitter.emit_realtime("pty_data", &payload, None);
    } else if let Some(app) = app {
        let _ = app.emit("pty_data", payload);
    }
}

fn emit_pty_exit(session_id: String, exit_code: Option<i32>, app: Option<&AppHandle>) {
    let payload = PtyExitEvent {
        session_id,
        exit_code,
    };
    if let Some(emitter) = crate::web_server::broadcaster::shared_emitter() {
        emitter.emit_realtime("pty_exit", &payload, None);
    } else if let Some(app) = app {
        let _ = app.emit("pty_exit", payload);
    }
}

/// Core implementation for creating an interactive PTY session.
pub fn pty_create_impl(
    session_id: String,
    cwd: String,
    cols: u16,
    rows: u16,
    app: Option<AppHandle>,
) -> Result<(), String> {
    let registry = global_registry();
    // Reject duplicate session_id
    if registry.lock().unwrap().contains_key(&session_id) {
        return Err(format!("PTY session already exists: {session_id}"));
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    // Resolve shell binary
    let shell = if cfg!(target_os = "windows") {
        "cmd.exe".to_string()
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "zsh".to_string())
    };

    let mut cmd = CommandBuilder::new(&shell);
    cmd.cwd(&cwd);
    cmd.env("TERM", "xterm-256color");

    // Augment PATH so nvm/homebrew-installed CLIs (claude, codex, etc.) are findable
    let path_env = crate::agent::claude_stream::augmented_path();
    cmd.env("PATH", &path_env);

    // Unix: start as login shell so .zshrc/.bash_profile are sourced
    #[cfg(unix)]
    {
        cmd.arg("-l");
    }

    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);

    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    let kill_flag = Arc::new(AtomicBool::new(false));
    let kill_flag_clone = kill_flag.clone();
    let session_id_clone = session_id.clone();
    let app_clone = app;

    // Reader thread: blocks on PTY read, emits `pty_data` events.
    // Exits naturally on EOF (shell closed) or when kill_flag is set.
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 8192];
        loop {
            if kill_flag_clone.load(Ordering::Relaxed) {
                break;
            }
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF — shell exited
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    emit_pty_data(session_id_clone.clone(), data, app_clone.as_ref());
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }

        // Notify that the shell has exited
        emit_pty_exit(session_id_clone, None, app_clone.as_ref());
    });

    let session = PtySession {
        writer: Arc::new(Mutex::new(writer)),
        master: Arc::new(Mutex::new(pair.master)),
        child: Arc::new(Mutex::new(child)),
        kill_flag,
    };

    registry.lock().unwrap().insert(session_id, session);
    Ok(())
}

/// Core implementation for writing to a PTY session.
pub fn pty_write_impl(session_id: String, data: String) -> Result<(), String> {
    let registry = global_registry();
    let writer = registry
        .lock()
        .unwrap()
        .get(&session_id)
        .map(|s| s.writer.clone())
        .ok_or("PTY session not found")?;

    let mut w = writer.lock().unwrap();
    w.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Core implementation for resizing a PTY session.
pub fn pty_resize_impl(session_id: String, cols: u16, rows: u16) -> Result<(), String> {
    let registry = global_registry();
    let master = registry
        .lock()
        .unwrap()
        .get(&session_id)
        .map(|s| s.master.clone())
        .ok_or("PTY session not found")?;

    let m = master.lock().unwrap();
    m.resize(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Core implementation for killing a PTY session.
pub fn pty_kill_impl(session_id: String, app: Option<&AppHandle>) -> Result<(), String> {
    let registry = global_registry();
    let session = registry
        .lock()
        .unwrap()
        .remove(&session_id)
        .ok_or("PTY session not found")?;

    session.kill_flag.store(true, Ordering::Relaxed);

    {
        let mut child = session.child.lock().unwrap();
        let _ = child.kill();
    }

    emit_pty_exit(session_id, None, app);
    Ok(())
}

/// Clean up all active PTY child processes (called during app/core shutdown).
pub fn cleanup_all_pty_sessions() {
    let registry = global_registry();
    let mut map = registry.lock().unwrap();
    for (_id, session) in map.drain() {
        session.kill_flag.store(true, Ordering::Relaxed);
        if let Ok(mut child) = session.child.lock() {
            let _ = child.kill();
        }
    }
}

/// Create a new interactive PTY session (Tauri command wrapper).
#[tauri::command]
pub async fn pty_create(
    app: AppHandle,
    _registry: State<'_, PtyRegistry>,
    session_id: String,
    cwd: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    pty_create_impl(session_id, cwd, cols, rows, Some(app))
}

/// Write raw bytes to the PTY master (Tauri command wrapper).
#[tauri::command]
pub async fn pty_write(
    _registry: State<'_, PtyRegistry>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    pty_write_impl(session_id, data)
}

/// Resize the PTY to match frontend dimensions (Tauri command wrapper).
#[tauri::command]
pub async fn pty_resize(
    _registry: State<'_, PtyRegistry>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    pty_resize_impl(session_id, cols, rows)
}

/// Kill a PTY session and remove it from the registry (Tauri command wrapper).
#[tauri::command]
pub async fn pty_kill(
    app: AppHandle,
    _registry: State<'_, PtyRegistry>,
    session_id: String,
) -> Result<(), String> {
    pty_kill_impl(session_id, Some(&app))
}
