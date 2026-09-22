//! Runtime support for AgentCabin's built-in browser.
//!
//! Chromium is owned by Electron's `WebContentsView`. The Rust side only
//! stages the small Node Browser Worker and resolves Node.js; no Playwright
//! package, browser download, or headless Chromium cache is involved.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use crate::work::executor::runtime::RuntimeResolver;
use crate::work::models::WorkExecutionRuntime;
use crate::work::paths::WorkPaths;

const WORKER_SERVER: &str = include_str!("../../../browser-worker/server.mjs");
const WORKER_SNAPSHOT: &str = include_str!("../../../browser-worker/snapshot.mjs");
const WORKER_EMBEDDED_CDP: &str = include_str!("../../../browser-worker/embedded_cdp.mjs");
const WORKER_EMBEDDED_PAGE: &str = include_str!("../../../browser-worker/embedded_page.mjs");
const WORKER_PACKAGE: &str = include_str!("../../../browser-worker/package.json");

static PREPARE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserRuntimeStatus {
    pub node_available: bool,
    /// Kept in the wire model for compatibility with older clients. Always
    /// false now that Playwright is no longer part of the browser runtime.
    pub playwright_installed: bool,
    /// Kept in the wire model for compatibility. Chromium is owned by
    /// Electron, so it is not a separately prepared Rust-side cache.
    pub chromium_installed: bool,
    pub runtime_available: bool,
    pub managed: bool,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePreparationProgress {
    pub phase: String,
    pub status: String,
    pub progress: u8,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_phase: Option<String>,
}

struct ProgressEmitter<'a, F> {
    reporter: &'a F,
    started_at: Instant,
}

impl<F> ProgressEmitter<'_, F>
where
    F: Fn(BrowserRuntimePreparationProgress),
{
    fn emit(
        &self,
        phase: &str,
        status: &str,
        progress: u8,
        message: impl Into<String>,
        log: Option<String>,
    ) {
        (self.reporter)(BrowserRuntimePreparationProgress {
            phase: phase.to_string(),
            status: status.to_string(),
            progress,
            message: message.into(),
            log,
            elapsed_ms: self.started_at.elapsed().as_millis().min(u64::MAX as u128) as u64,
            error: None,
            failed_phase: None,
        });
    }

    fn failed(&self, phase: &str, progress: u8, error: String) {
        (self.reporter)(BrowserRuntimePreparationProgress {
            phase: "failed".to_string(),
            status: "failed".to_string(),
            progress,
            message: "内置浏览器运行环境准备失败".to_string(),
            log: None,
            elapsed_ms: self.started_at.elapsed().as_millis().min(u64::MAX as u128) as u64,
            error: Some(error),
            failed_phase: Some(phase.to_string()),
        });
    }
}

pub fn status_with_paths(_paths: &WorkPaths) -> BrowserRuntimeStatus {
    let node_available = RuntimeResolver::resolve_binary(WorkExecutionRuntime::Node).is_ok();
    BrowserRuntimeStatus {
        node_available,
        playwright_installed: false,
        chromium_installed: false,
        runtime_available: node_available,
        managed: false,
        message: if node_available {
            "内置 Chromium 由 Electron 持有，Browser Worker 通过原生 CDP 连接".to_string()
        } else {
            "缺少 Node.js 运行时，AgentCabin 无法启动 Browser Worker".to_string()
        },
    }
}

pub fn prepare_with_paths(paths: &WorkPaths) -> Result<BrowserRuntimeStatus, String> {
    prepare_with_progress(paths, |_| {})
}

pub fn prepare_with_progress<F>(
    paths: &WorkPaths,
    reporter: F,
) -> Result<BrowserRuntimeStatus, String>
where
    F: Fn(BrowserRuntimePreparationProgress) + Send + Sync,
{
    let started_at = Instant::now();
    let emitter = ProgressEmitter {
        reporter: &reporter,
        started_at,
    };
    let phase = Arc::new(Mutex::new("initializing".to_string()));

    let result = (|| {
        let lock = PREPARE_LOCK.get_or_init(|| Mutex::new(()));
        let _guard = lock
            .lock()
            .map_err(|_| "Browser Runtime preparation lock is poisoned".to_string())?;

        paths.ensure_layout()?;
        let worker_dir = paths.browser_worker_dir();
        fs::create_dir_all(&worker_dir).map_err(|error| {
            format!(
                "无法创建 Browser Worker 目录 {}: {error}",
                worker_dir.display()
            )
        })?;
        write_if_changed(&worker_dir.join("server.mjs"), WORKER_SERVER)?;
        write_if_changed(&worker_dir.join("snapshot.mjs"), WORKER_SNAPSHOT)?;
        write_if_changed(&worker_dir.join("embedded_cdp.mjs"), WORKER_EMBEDDED_CDP)?;
        write_if_changed(&worker_dir.join("embedded_page.mjs"), WORKER_EMBEDDED_PAGE)?;
        write_if_changed(&worker_dir.join("package.json"), WORKER_PACKAGE)?;
        emitter.emit(
            "initializing",
            "completed",
            45,
            "内置浏览器 Worker 已就绪",
            Some(worker_dir.display().to_string()),
        );

        let node = RuntimeResolver::resolve_binary(WorkExecutionRuntime::Node)
            .map_err(|error| format!("Browser Use 需要 Node.js 运行时: {error}"))?;
        if let Ok(mut current) = phase.lock() {
            *current = "verifying".to_string();
        }
        emitter.emit(
            "verifying",
            "running",
            70,
            "正在校验 Browser Worker 运行环境",
            Some(node.display().to_string()),
        );
        let status = status_with_paths(paths);
        if !status.runtime_available {
            return Err(status.message);
        }
        emitter.emit(
            "verifying",
            "completed",
            95,
            "原生 CDP 浏览器运行环境已就绪",
            None,
        );
        emitter.emit(
            "completed",
            "completed",
            100,
            "内置 Chromium 已准备完成",
            None,
        );
        Ok(status)
    })();

    if let Err(error) = &result {
        let failed_phase = phase
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| "initializing".to_string());
        emitter.failed(&failed_phase, 70, error.clone());
    }
    result
}

/// Resolve the worker script. The only runtime dependency is Node.js; the
/// browser process itself is the Electron WebContentsView.
pub fn resolve_worker_launch(paths: &WorkPaths) -> Result<(PathBuf, Option<PathBuf>), String> {
    paths.ensure_layout()?;
    if let Some(script) = development_worker_script() {
        return Ok((script, None));
    }
    prepare_with_paths(paths)?;
    Ok((paths.browser_worker_dir().join("server.mjs"), None))
}

fn development_worker_script() -> Option<PathBuf> {
    let candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("browser-worker")
        .join("server.mjs");
    candidate.is_file().then_some(candidate)
}

pub(crate) fn command_path(node: &Path) -> String {
    let mut parts = Vec::new();
    if let Some(parent) = node.parent() {
        parts.push(parent.to_path_buf());
    }
    let base = crate::agent::claude_stream::augmented_path();
    for entry in std::env::split_paths(&base) {
        if !parts.contains(&entry) {
            parts.push(entry);
        }
    }
    std::env::join_paths(parts)
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|_| {
            node.parent()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
}

fn write_if_changed(path: &Path, contents: &str) -> Result<(), String> {
    if fs::read_to_string(path).ok().as_deref() != Some(contents) {
        fs::write(path, contents)
            .map_err(|error| format!("无法写入 {}: {error}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn status_is_read_only_for_an_empty_runtime() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let _status = status_with_paths(&paths);
        assert!(!paths.browser_worker_dir().exists());
        assert!(!paths.browser_browsers_dir().exists());
    }

    #[test]
    fn preparation_progress_uses_native_cdp_phases() {
        let progress = BrowserRuntimePreparationProgress {
            phase: "completed".to_string(),
            status: "completed".to_string(),
            progress: 100,
            message: "内置 Chromium 已准备完成".to_string(),
            log: None,
            elapsed_ms: 1234,
            error: None,
            failed_phase: None,
        };
        let value = serde_json::to_value(progress).unwrap();
        assert_eq!(value["phase"], "completed");
        assert_eq!(value["elapsedMs"], 1234);
        assert!(value.get("error").is_none());
    }
}
