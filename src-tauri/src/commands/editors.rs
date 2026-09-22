//! Open the current project in an installed code editor.
//!
//! The desktop app may be launched from Finder, so relying only on the shell's
//! PATH misses common macOS VS Code installations. We check both the CLI and
//! the standard application locations, while keeping the exposed surface to
//! the one editor AgentCabin currently supports.

use crate::process_ext::HideConsole;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
enum VscodeLauncher {
    Command(PathBuf),
    #[cfg(target_os = "macos")]
    MacApplication,
}

fn command_on_path(name: &str) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let finder = "where";
    #[cfg(not(target_os = "windows"))]
    let finder = "which";

    let output = Command::new(finder)
        .arg(name)
        .hide_console()
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .find(|path| path.exists())
}

fn first_existing(paths: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    paths.into_iter().find(|path| path.exists())
}

fn find_vscode() -> Option<VscodeLauncher> {
    if let Some(path) = command_on_path("code") {
        return Some(VscodeLauncher::Command(path));
    }

    #[cfg(target_os = "macos")]
    {
        let home_app = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Applications/Visual Studio Code.app"));
        let app = first_existing(
            [
                Some(PathBuf::from("/Applications/Visual Studio Code.app")),
                home_app,
            ]
            .into_iter()
            .flatten(),
        );
        if app.is_some() {
            return Some(VscodeLauncher::MacApplication);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let local_app = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("Programs/Microsoft VS Code/bin/code.cmd"));
        let program_files = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .map(|root| root.join("Microsoft VS Code/bin/code.cmd"));
        if let Some(path) = first_existing([local_app, program_files].into_iter().flatten()) {
            return Some(VscodeLauncher::Command(path));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(path) = first_existing([
            PathBuf::from("/usr/bin/code"),
            PathBuf::from("/usr/local/bin/code"),
            PathBuf::from("/snap/bin/code"),
            PathBuf::from("/var/lib/flatpak/exports/bin/com.visualstudio.code"),
        ]) {
            return Some(VscodeLauncher::Command(path));
        }
    }

    None
}

fn validate_project_directory(cwd: &str) -> Result<&Path, String> {
    let path = Path::new(cwd);
    if !path.is_absolute() {
        return Err("Project path must be absolute".to_string());
    }
    if !path.is_dir() {
        return Err(format!("Project directory does not exist: {cwd}"));
    }
    Ok(path)
}

#[tauri::command]
pub fn check_vscode_available() -> bool {
    let available = find_vscode().is_some();
    log::debug!("[editors] VS Code available: {available}");
    available
}

#[tauri::command]
pub fn open_project_in_vscode(cwd: String) -> Result<(), String> {
    let path = validate_project_directory(&cwd)?;
    let launcher = find_vscode().ok_or_else(|| "VS Code is not installed".to_string())?;

    let result = match launcher {
        VscodeLauncher::Command(program) => Command::new(program)
            .arg("--reuse-window")
            .arg(path)
            .hide_console()
            .spawn(),
        #[cfg(target_os = "macos")]
        VscodeLauncher::MacApplication => Command::new("open")
            .args(["-a", "Visual Studio Code"])
            .arg(&cwd)
            .hide_console()
            .spawn(),
    };

    result
        .map(|_| ())
        .map_err(|e| format!("Failed to open VS Code: {e}"))
}
