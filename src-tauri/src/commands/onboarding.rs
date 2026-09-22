use crate::agent::claude_stream;
use crate::models::{AuthCheckResult, AuthOverview, InstallMethod, PiAuthResult};
use crate::process_ext::HideConsole;
use crate::storage;
use serde_json::Value;
use tokio::process::Command;

const PI_CODEX_PROVIDER: &str = "openai-codex";
const MINIMUM_PI_NODE_VERSION: &str = "22.19.0";

/// Check whether the user has an active OAuth session or API key configured.
#[tauri::command]
pub async fn check_auth_status() -> Result<AuthCheckResult, String> {
    log::debug!("[onboarding] check_auth_status");

    // Check API key from app settings + CLI sources (settings.json, env vars, shell configs)
    let user_settings = storage::settings::get_user_settings();
    let has_app_key = user_settings
        .anthropic_api_key
        .as_ref()
        .is_some_and(|k| !k.is_empty());
    let cli_config = storage::cli_config::load_cli_config();
    let (cli_key, cli_key_source) = detect_cli_api_key(&cli_config);
    let has_api_key = has_app_key || cli_key.is_some();

    // Check OAuth via shared helper
    let (has_oauth, oauth_account) = check_cli_oauth().await;

    log::debug!(
        "[onboarding] auth check result: has_oauth={}, has_api_key={} (app={}, cli={:?}), account={:?}",
        has_oauth,
        has_api_key,
        has_app_key,
        cli_key_source,
        oauth_account
    );

    Ok(AuthCheckResult {
        has_oauth,
        has_api_key,
        oauth_account,
    })
}

/// Detect which CLI installation methods are available for the given agent.
/// Unknown agent values fall back to Claude (preserves old behavior for callers
/// that haven't been updated yet).
#[tauri::command]
pub async fn detect_install_methods(agent: String) -> Result<Vec<InstallMethod>, String> {
    log::debug!("[onboarding] detect_install_methods: agent={}", agent);
    let methods = match agent.as_str() {
        "codex" => detect_codex_install_methods().await,
        "pi" => detect_pi_install_methods().await,
        "grok" => detect_grok_install_methods().await,
        "dsh" => detect_dsh_install_methods().await,
        _ => detect_claude_install_methods().await,
    };
    log::debug!(
        "[onboarding] install methods ({}): {:?}",
        agent,
        methods
            .iter()
            .map(|m| format!("{}={}", m.id, m.available))
            .collect::<Vec<_>>()
    );
    Ok(methods)
}

async fn detect_claude_install_methods() -> Vec<InstallMethod> {
    let mut methods = Vec::new();

    // 1. Homebrew — macOS/Linux only (not relevant on Windows)
    #[cfg(not(windows))]
    {
        let has_brew = which_binary("brew");
        methods.push(InstallMethod {
            id: "brew".into(),
            name: "Homebrew".into(),
            command: "brew install claude-code".into(),
            available: has_brew,
            unavailable_reason: if has_brew {
                None
            } else {
                Some("Homebrew not installed".into())
            },
            note: None,
        });
    }

    // 2. Windows-native install methods (PowerShell, WinGet, CMD curl)
    #[cfg(windows)]
    {
        // PowerShell installer (recommended on Windows)
        // Prefer "powershell" (Windows PowerShell 5.1), fall back to "pwsh" (PowerShell 7+)
        let ps_bin = if which_binary("powershell") {
            Some("powershell")
        } else if which_binary("pwsh") {
            Some("pwsh")
        } else {
            None
        };
        methods.push(InstallMethod {
            id: "powershell".into(),
            name: "PowerShell".into(),
            command: "irm https://claude.ai/install.ps1 | iex".into(),
            available: ps_bin.is_some(),
            unavailable_reason: if ps_bin.is_some() {
                None
            } else {
                Some("PowerShell not found".into())
            },
            note: Some("Auto-updates".into()),
        });

        // WinGet (Win11 built-in, Win10 may need install)
        let has_winget = which_binary("winget");
        methods.push(InstallMethod {
            id: "winget".into(),
            name: "WinGet".into(),
            command: "winget install --id Anthropic.ClaudeCode -e --source winget".into(),
            available: has_winget,
            unavailable_reason: if has_winget {
                None
            } else {
                Some("WinGet not found".into())
            },
            note: Some("Manual update: winget upgrade Anthropic.ClaudeCode".into()),
        });

        // CMD curl (fallback — Win10 1803+ has curl built-in)
        let has_curl = which_binary("curl");
        methods.push(InstallMethod {
            id: "cmd".into(),
            name: "CMD (curl)".into(),
            command: "cmd /d /c \"curl -fsSL https://claude.ai/install.cmd -o install.cmd && install.cmd && del /f /q install.cmd\"".into(),
            available: has_curl,
            unavailable_reason: if has_curl {
                None
            } else {
                Some("curl not found".into())
            },
            note: None,
        });
    }

    // 3. npm — deprecated, requires Node.js 18+
    let has_npm = check_npm_available().await;
    methods.push(InstallMethod {
        id: "npm".into(),
        name: "npm (Node.js)".into(),
        command: "npm install -g @anthropic-ai/claude-code".into(),
        available: has_npm,
        unavailable_reason: if has_npm {
            None
        } else {
            Some("Requires Node.js 18+".into())
        },
        note: Some("Deprecated — use native installer instead".into()),
    });

    // 4. Native install (curl script) — Unix only (curl | bash)
    #[cfg(not(windows))]
    {
        let has_curl = which_binary("curl");
        methods.push(InstallMethod {
            id: "native".into(),
            name: "Native Install (curl)".into(),
            command: "curl -fsSL https://claude.ai/install.sh | bash".into(),
            available: has_curl,
            unavailable_reason: if has_curl {
                None
            } else {
                Some("curl not found".into())
            },
            note: None,
        });
    }

    methods
}

async fn detect_codex_install_methods() -> Vec<InstallMethod> {
    let mut methods = Vec::new();

    // Homebrew — macOS/Linux only
    #[cfg(not(windows))]
    {
        let has_brew = which_binary("brew");
        methods.push(InstallMethod {
            id: "brew".into(),
            name: "Homebrew".into(),
            command: "brew install codex".into(),
            available: has_brew,
            unavailable_reason: if has_brew {
                None
            } else {
                Some("Homebrew not installed".into())
            },
            note: None,
        });
    }

    // npm — official cross-platform install path for Codex CLI
    let has_npm = check_npm_available().await;
    methods.push(InstallMethod {
        id: "npm".into(),
        name: "npm (Node.js)".into(),
        command: "npm install -g @openai/codex".into(),
        available: has_npm,
        unavailable_reason: if has_npm {
            None
        } else {
            Some("Requires Node.js 18+".into())
        },
        note: None,
    });

    methods
}

async fn detect_pi_install_methods() -> Vec<InstallMethod> {
    let mut methods = Vec::new();

    let has_npm = check_npm_available_for(MINIMUM_PI_NODE_VERSION).await;
    methods.push(InstallMethod {
        id: "npm".into(),
        name: "npm (Node.js)".into(),
        command: "npm run prepare:runtimes".into(),
        available: has_npm,
        unavailable_reason: if has_npm {
            None
        } else {
            Some(format!("Requires Node.js {MINIMUM_PI_NODE_VERSION}+"))
        },
        note: None,
    });

    methods
}

async fn detect_grok_install_methods() -> Vec<InstallMethod> {
    let mut methods = Vec::new();

    // Official Grok Build installer for macOS, Linux and WSL.
    #[cfg(not(windows))]
    {
        let has_curl = which_binary("curl");
        methods.push(InstallMethod {
            id: "native".into(),
            name: "Grok Build installer".into(),
            command: "curl -fsSL https://x.ai/cli/install.sh | bash".into(),
            available: has_curl,
            unavailable_reason: if has_curl {
                None
            } else {
                Some("curl not found".into())
            },
            note: Some("Official macOS / Linux / WSL installer".into()),
        });
    }

    // Official Windows PowerShell installer.
    #[cfg(windows)]
    {
        let has_powershell = which_binary("powershell") || which_binary("pwsh");
        methods.push(InstallMethod {
            id: "powershell".into(),
            name: "PowerShell".into(),
            command: "irm https://x.ai/cli/install.ps1 | iex".into(),
            available: has_powershell,
            unavailable_reason: if has_powershell {
                None
            } else {
                Some("PowerShell not found".into())
            },
            note: Some("Official Windows installer".into()),
        });
    }

    // xAI also distributes Grok Build through npm, useful on managed machines where
    // executing a bootstrap script is undesirable.
    let has_npm = check_npm_available().await;
    methods.push(InstallMethod {
        id: "npm".into(),
        name: "npm (Node.js)".into(),
        command: "npm install -g @xai-official/grok".into(),
        available: has_npm,
        unavailable_reason: if has_npm {
            None
        } else {
            Some("Requires Node.js 18+".into())
        },
        note: Some("Alternative Grok Build distribution".into()),
    });

    methods
}

async fn detect_dsh_install_methods() -> Vec<InstallMethod> {
    let mut methods = Vec::new();
    let has_npm = check_npm_available().await;
    methods.push(InstallMethod {
        id: "npm".into(),
        name: "npm (Node.js)".into(),
        command: "npm run prepare:runtimes".into(),
        available: has_npm,
        unavailable_reason: if has_npm {
            None
        } else {
            Some("Requires Node.js 18+".into())
        },
        note: Some("Official DeepSeek Harness distribution".into()),
    });
    methods
}

/// Headless-compatible impl: run `claude login` (OAuth flow, CLI opens a browser).
pub(crate) async fn run_claude_login_impl(
    emitter: std::sync::Arc<crate::web_server::broadcaster::BroadcastEmitter>,
) -> Result<bool, String> {
    log::debug!("[onboarding] run_claude_login");

    let claude_bin = claude_stream::resolve_claude_path();
    let path_env = claude_stream::augmented_path();

    let mut child = Command::new(&claude_bin)
        .arg("login")
        .env("PATH", &path_env)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .hide_console()
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn claude login: {}", e))?;

    if let Some(stdout) = child.stdout.take() {
        let em = emitter.clone();
        tokio::spawn(async move { stream_pipe_to_events(stdout, em, "login stdout").await });
    }
    if let Some(stderr) = child.stderr.take() {
        let em = emitter.clone();
        tokio::spawn(async move { stream_pipe_to_events(stderr, em, "login stderr").await });
    }

    // Wait for exit (3 min timeout — user needs to complete browser auth)
    let status = tokio::time::timeout(std::time::Duration::from_secs(180), child.wait())
        .await
        .map_err(|_| "Login timed out after 3 minutes".to_string())?
        .map_err(|e| format!("Login process error: {}", e))?;

    let success = status.success();
    log::debug!(
        "[onboarding] run_claude_login: exit={:?}, success={}",
        status.code(),
        success
    );

    Ok(success)
}

/// Run `claude login` to start the OAuth flow. The CLI opens a browser automatically.
#[tauri::command]
pub async fn run_claude_login(
    emitter: tauri::State<'_, std::sync::Arc<crate::web_server::broadcaster::BroadcastEmitter>>,
) -> Result<bool, String> {
    run_claude_login_impl(emitter.inner().clone()).await
}

/// Resolve the `codex` CLI binary path. Shared between `run_codex_login` and
/// `run_codex_logout` so both commands use the same resolution strategy
/// (falling back to a bare `"codex"` literal when `which_binary` fails so the
/// OS PATH lookup still has a chance).
fn resolve_codex_binary() -> String {
    // Candidate-list resolver (npm %APPDATA%\npm\codex.cmd etc.) so Windows login works even
    // when `where codex` doesn't surface the .cmd shim.
    claude_stream::resolve_codex_path()
}

/// Headless-compatible impl: run `codex login` (OAuth flow, CLI opens a browser).
pub(crate) async fn run_codex_login_impl(
    emitter: std::sync::Arc<crate::web_server::broadcaster::BroadcastEmitter>,
) -> Result<bool, String> {
    let aug_path = claude_stream::augmented_path();
    let codex_bin = resolve_codex_binary();
    log::debug!("[onboarding] run_codex_login: binary={}", codex_bin);

    let mut child = Command::new(&codex_bin)
        .arg("login")
        .env("PATH", &aug_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .hide_console()
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn codex login: {}", e))?;

    log::debug!("[onboarding] run_codex_login: spawned");

    if let Some(stdout) = child.stdout.take() {
        let em = emitter.clone();
        tokio::spawn(async move { stream_pipe_to_events(stdout, em, "codex login stdout").await });
    }
    if let Some(stderr) = child.stderr.take() {
        let em = emitter.clone();
        tokio::spawn(async move { stream_pipe_to_events(stderr, em, "codex login stderr").await });
    }

    let status = tokio::time::timeout(std::time::Duration::from_secs(180), child.wait())
        .await
        .map_err(|_| "Codex login timed out after 3 minutes".to_string())?
        .map_err(|e| format!("Codex login process error: {}", e))?;

    log::debug!(
        "[onboarding] run_codex_login: exit={:?}, success={}",
        status.code(),
        status.success()
    );

    if !status.success() {
        return Err(format!(
            "Codex login exited with code {}",
            status
                .code()
                .map_or("unknown".into(), |c: i32| c.to_string())
        ));
    }

    Ok(true)
}

/// Run `codex login` to start the OAuth flow. The CLI opens a browser automatically.
#[tauri::command]
pub async fn run_codex_login(
    emitter: tauri::State<'_, std::sync::Arc<crate::web_server::broadcaster::BroadcastEmitter>>,
) -> Result<bool, String> {
    run_codex_login_impl(emitter.inner().clone()).await
}

/// Sign in to the AgentCabin-wide ChatGPT subscription Provider without invoking the Codex CLI.
/// This OAuth session is intentionally separate from both Pi's auth file and native Codex CLI
/// credentials, so the resulting Provider can be selected by Grok, Codex, and Pi Agent alike.
#[tauri::command]
pub async fn run_codex_subscription_login() -> Result<bool, String> {
    crate::agent::codex_subscription_bridge::sign_in().await
}

/// Reopen the browser page for an in-progress AgentCabin subscription login.
#[tauri::command]
pub async fn reopen_codex_subscription_login() -> Result<bool, String> {
    crate::agent::codex_subscription_bridge::reopen_sign_in().await
}

/// Headless-compatible impl: run `grok login` (same resolved binary as the ACP actor).
pub(crate) async fn run_grok_login_impl(
    emitter: std::sync::Arc<crate::web_server::broadcaster::BroadcastEmitter>,
) -> Result<bool, String> {
    let aug_path = claude_stream::augmented_path();
    let grok_bin = crate::agent::grok_session_actor::process::resolve_grok_path();
    log::debug!("[onboarding] run_grok_login: binary={}", grok_bin);

    let mut child = Command::new(&grok_bin)
        .arg("login")
        .env("PATH", &aug_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .hide_console()
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn grok login: {}", e))?;

    if let Some(stdout) = child.stdout.take() {
        let em = emitter.clone();
        tokio::spawn(async move { stream_pipe_to_events(stdout, em, "grok login stdout").await });
    }
    if let Some(stderr) = child.stderr.take() {
        let em = emitter.clone();
        tokio::spawn(async move { stream_pipe_to_events(stderr, em, "grok login stderr").await });
    }

    let status = tokio::time::timeout(std::time::Duration::from_secs(180), child.wait())
        .await
        .map_err(|_| "Grok login timed out after 3 minutes".to_string())?
        .map_err(|e| format!("Grok login process error: {}", e))?;

    log::debug!(
        "[onboarding] run_grok_login: exit={:?}, success={}",
        status.code(),
        status.success()
    );

    if !status.success() {
        return Err(format!(
            "Grok login exited with code {}",
            status
                .code()
                .map_or("unknown".into(), |code: i32| code.to_string())
        ));
    }

    Ok(true)
}

/// Run `grok login` using the same resolved binary as the ACP actor. Grok Build owns its
/// credentials; AgentCabin only launches the native login flow and never reads secret tokens.
#[tauri::command]
pub async fn run_grok_login(
    emitter: tauri::State<'_, std::sync::Arc<crate::web_server::broadcaster::BroadcastEmitter>>,
) -> Result<bool, String> {
    run_grok_login_impl(emitter.inner().clone()).await
}

/// Run `codex logout` to clear stored credentials. Non-interactive, returns
/// quickly. Mirrors `run_codex_login`'s binary resolution.
#[tauri::command]
pub async fn run_codex_logout() -> Result<bool, String> {
    let aug_path = claude_stream::augmented_path();
    let codex_bin = resolve_codex_binary();
    log::debug!("[onboarding] run_codex_logout: binary={}", codex_bin);

    let output = Command::new(&codex_bin)
        .arg("logout")
        .env("PATH", &aug_path)
        .hide_console()
        .output()
        .await
        .map_err(|e| format!("Failed to spawn codex logout: {}", e))?;

    log::debug!(
        "[onboarding] run_codex_logout: exit={:?}, success={}",
        output.status.code(),
        output.status.success()
    );

    if output.status.success() {
        Ok(true)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!(
                "Codex logout exited with code {}",
                output
                    .status
                    .code()
                    .map_or("unknown".into(), |c: i32| c.to_string())
            )
        } else {
            stderr
        })
    }
}

/// Log out only the AgentCabin-wide ChatGPT subscription Provider.
#[tauri::command]
pub fn run_codex_subscription_logout() -> Result<bool, String> {
    crate::agent::codex_subscription_bridge::clear_subscription_auth()
}

/// Read Pi's native ChatGPT/Codex OAuth status without exposing credentials.
/// Pi stores provider credentials in `~/.pi/agent/auth.json` and refreshes them
/// itself when they are used by the RPC actor.
#[tauri::command]
pub async fn check_pi_auth() -> Result<PiAuthResult, String> {
    let Some(home) = storage::home_dir() else {
        return Ok(PiAuthResult {
            logged_in: false,
            provider: None,
            auth_type: None,
            status_text: Some("Home directory unavailable".to_string()),
        });
    };

    let auth_path = std::path::PathBuf::from(home)
        .join(".pi")
        .join("agent")
        .join("auth.json");
    let raw = match std::fs::read_to_string(&auth_path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PiAuthResult {
                logged_in: false,
                provider: None,
                auth_type: None,
                status_text: None,
            });
        }
        Err(error) => return Err(format!("Failed to read Pi auth status: {}", error)),
    };

    let root: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("Pi auth.json is invalid JSON: {}", error))?;
    let credential = root.get(PI_CODEX_PROVIDER);
    let auth_type = credential
        .and_then(|entry| entry.get("type"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let logged_in = auth_type.as_deref() == Some("oauth");

    Ok(PiAuthResult {
        logged_in,
        provider: logged_in.then(|| PI_CODEX_PROVIDER.to_string()),
        auth_type,
        status_text: logged_in.then(|| "ChatGPT Plus/Pro (Codex)".to_string()),
    })
}

/// Clear Pi's native ChatGPT/Codex credential, equivalent to `/logout` for
/// that provider. Other Pi provider credentials are preserved.
#[tauri::command]
pub async fn run_pi_logout() -> Result<bool, String> {
    let Some(home) = storage::home_dir() else {
        return Ok(false);
    };

    let auth_path = std::path::PathBuf::from(home)
        .join(".pi")
        .join("agent")
        .join("auth.json");
    let raw = match std::fs::read_to_string(&auth_path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("Failed to read Pi auth file: {}", error)),
    };

    let mut root: serde_json::Map<String, Value> = serde_json::from_str(&raw)
        .map_err(|error| format!("Pi auth.json is invalid JSON: {}", error))?;
    if root.remove(PI_CODEX_PROVIDER).is_none() {
        return Ok(false);
    }

    let next = serde_json::to_string_pretty(&root)
        .map_err(|error| format!("Failed to serialize Pi auth file: {}", error))?;
    std::fs::write(&auth_path, format!("{}\n", next))
        .map_err(|error| format!("Failed to update Pi auth file: {}", error))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&auth_path)
            .map_err(|error| format!("Failed to inspect Pi auth file: {}", error))?
            .permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(&auth_path, permissions)
            .map_err(|error| format!("Failed to secure Pi auth file: {}", error))?;
    }

    Ok(true)
}

/// Get an overview of all authentication sources (configuration state only).
#[tauri::command]
pub async fn get_auth_overview() -> Result<AuthOverview, String> {
    log::debug!("[onboarding] get_auth_overview");

    // 1. Read user settings → auth_mode, platform_credentials, active_platform_id
    let user_settings = storage::settings::get_user_settings();
    let auth_mode = user_settings.auth_mode.clone();

    // 2. CLI OAuth login — check via subprocess (same as onboarding wizard).
    let (cli_login_available, cli_login_account) = check_cli_oauth().await;

    // 3. Check CLI API Key from multiple sources (first non-empty wins):
    //    a) ~/.claude/settings.json "apiKey"
    //    b) ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN process env var
    //    c) Same vars in shell config files (.zshrc, .bashrc, etc.)
    let cli_config = storage::cli_config::load_cli_config();
    let (cli_api_key_str, cli_api_key_source) = detect_cli_api_key(&cli_config);
    let cli_has_api_key = cli_api_key_str.is_some();
    let cli_api_key_hint = cli_api_key_str.as_ref().map(|k| {
        let suffix: String = k
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("...{}", suffix)
    });

    // 4. Check App platform credentials
    let active_pid = user_settings.active_platform_id.clone();
    let app_has_credentials = active_pid.as_ref().is_some_and(|pid| {
        user_settings
            .platform_credentials
            .iter()
            .any(|c| &c.platform_id == pid && c.api_key.as_ref().is_some_and(|k| !k.is_empty()))
    });

    // Platform name: use credential name, fallback to preset name, fallback to pid
    let app_platform_name = active_pid.as_ref().map(|pid| {
        // Try credential name first
        let cred_name = user_settings
            .platform_credentials
            .iter()
            .find(|c| &c.platform_id == pid)
            .and_then(|c| c.name.clone());
        if let Some(name) = cred_name {
            if !name.is_empty() {
                return name;
            }
        }
        // Fallback to preset name
        preset_name(pid)
    });

    log::debug!(
        "[onboarding] auth overview: mode={}, cli_login={}, cli_key={} (source={:?}), app_cred={}",
        auth_mode,
        cli_login_available,
        cli_has_api_key,
        cli_api_key_source,
        app_has_credentials
    );

    Ok(AuthOverview {
        auth_mode,
        cli_login_available,
        cli_login_account,
        cli_has_api_key,
        cli_api_key_hint,
        cli_api_key_source,
        app_has_credentials,
        app_platform_id: active_pid,
        app_platform_name,
    })
}

/// Set API key in CLI config (~/.claude/settings.json).
#[tauri::command]
pub async fn set_cli_api_key(key: String) -> Result<(), String> {
    log::debug!("[onboarding] set_cli_api_key");
    let trimmed = key.trim().to_string();
    if trimmed.is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    storage::cli_config::update_cli_config(serde_json::json!({ "apiKey": trimmed }))?;
    Ok(())
}

/// Remove API key from CLI config (~/.claude/settings.json).
#[tauri::command]
pub async fn remove_cli_api_key() -> Result<(), String> {
    log::debug!("[onboarding] remove_cli_api_key");
    storage::cli_config::update_cli_config(serde_json::json!({ "apiKey": null }))?;
    Ok(())
}

// ── Helpers ──

/// Env var names that Claude CLI recognizes for API key authentication.
const CLI_KEY_ENV_VARS: &[&str] = &["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

/// Detect CLI API key from settings file, process env vars, and shell config files.
/// Returns (key_value, source_label).
pub(crate) fn detect_cli_api_key(
    cli_config: &serde_json::Value,
) -> (Option<String>, Option<String>) {
    // a) ~/.claude/settings.json "apiKey"
    let settings_key = cli_config
        .get("apiKey")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string());
    if let Some(k) = settings_key {
        return (Some(k), Some("settings".to_string()));
    }

    // b) Process env vars
    for var in CLI_KEY_ENV_VARS {
        if let Ok(val) = std::env::var(var) {
            if !val.trim().is_empty() {
                return (Some(val), Some("env".to_string()));
            }
        }
    }

    // c) Shell config files
    for var in CLI_KEY_ENV_VARS {
        if let Some((val, path)) = read_env_from_shell_config(var) {
            return (Some(val), Some(format!("shell_config:{}", path)));
        }
    }

    (None, None)
}

/// Parse shell config files to find `export VAR_NAME=value`.
/// Handles: `export VAR=val`, `export VAR="val"`, `export VAR='val'`.
/// Skips commented lines. Returns (value, file_path) of the first match.
#[cfg(unix)]
pub(crate) fn read_env_from_shell_config(var_name: &str) -> Option<(String, String)> {
    let home = crate::storage::home_dir()?;
    let config_files = [
        format!("{}/.zshrc", home),
        format!("{}/.zprofile", home),
        format!("{}/.bashrc", home),
        format!("{}/.bash_profile", home),
        format!("{}/.profile", home),
    ];
    let pattern = format!("{}=", var_name);
    for path in &config_files {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                continue;
            }
            // Match "export VAR_NAME=..." or "VAR_NAME=..."
            let after_export = trimmed.strip_prefix("export ").unwrap_or(trimmed);
            if let Some(rest) = after_export.strip_prefix(&pattern) {
                let val = rest.trim().trim_matches('"').trim_matches('\'');
                if !val.is_empty() {
                    log::debug!("[onboarding] found {} in shell config: {}", var_name, path);
                    return Some((val.to_string(), path.clone()));
                }
            }
        }
    }
    None
}

#[cfg(windows)]
pub(crate) fn read_env_from_shell_config(_var_name: &str) -> Option<(String, String)> {
    None
}

/// Check CLI OAuth status via subprocess. Used by onboarding wizard (slower but gets account email).
pub(crate) async fn check_cli_oauth() -> (bool, Option<String>) {
    let claude_bin = claude_stream::resolve_claude_path();
    if claude_bin != "claude" || which_binary("claude") {
        match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            Command::new(&claude_bin)
                .arg("auth")
                .arg("status")
                .env("PATH", claude_stream::augmented_path())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .hide_console()
                .kill_on_drop(true)
                .output(),
        )
        .await
        {
            Ok(Ok(output)) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let account = serde_json::from_str::<serde_json::Value>(&stdout)
                    .ok()
                    .and_then(|v| v.get("email")?.as_str().map(|s| s.to_string()));
                (true, account)
            }
            _ => (false, None),
        }
    } else {
        (false, None)
    }
}

/// Get display name for a platform preset ID.
pub(crate) fn preset_name(pid: &str) -> String {
    // Known preset names (mirrors frontend PLATFORM_PRESETS)
    match pid {
        "anthropic" => "Anthropic",
        "deepseek" => "DeepSeek",
        "kimi" => "Kimi (Moonshot)",
        "kimi-coding" => "Kimi For Coding",
        "zhipu" => "Zhipu (智谱)",
        "zhipu-intl" => "Zhipu (智谱 Intl)",
        "bailian" => "Bailian (Coding Plan)",
        "bailian-api" => "Bailian (百炼 API)",
        "doubao" => "DouBao (豆包)",
        "minimax" => "MiniMax",
        "minimax-cn" => "MiniMax (China)",
        "mimo" => "Xiaomi MiMo (小米)",
        "mimo-tp" => "Xiaomi MiMo (Token Plan)",
        "hunyuan" => "Tencent Hunyuan (混元)",
        "vercel" => "Vercel AI Gateway",
        "openrouter" => "OpenRouter",
        "siliconflow" => "SiliconFlow (硅基流动)",
        "aihubmix" => "AiHubMix",
        "ollama" => "Ollama",
        "ccswitch" => "CC Switch",
        "ccr" => "Claude Code Router",
        "zenmux" => "ZenMux",
        "custom" => "Custom",
        _ => return pid.to_string(),
    }
    .to_string()
}

/// Stream a pipe (stdout or stderr) to the frontend via setup-progress events.
/// Reads in chunks and splits on both `\r` and `\n`:
///   - `\n`-terminated lines → `setup-progress` event (frontend appends)
///   - `\r`-terminated segments → `setup-progress-replace` event (frontend replaces last line)
///     Throttled to 300ms to avoid flooding with rapid progress bar updates.
///
/// Events flow through the BroadcastEmitter, covering both the Tauri webview
/// and the headless core-server SSE channel (Electron).
async fn stream_pipe_to_events(
    pipe: impl tokio::io::AsyncRead + Unpin,
    emitter: std::sync::Arc<crate::web_server::broadcaster::BroadcastEmitter>,
    label: &'static str,
) {
    use tokio::io::AsyncReadExt;

    let mut reader = tokio::io::BufReader::new(pipe);
    let mut buf = [0u8; 4096];
    let mut pending = String::new();
    let mut last_replace = std::time::Instant::now();
    let throttle = std::time::Duration::from_millis(300);

    loop {
        let n = match reader.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };

        pending.push_str(&String::from_utf8_lossy(&buf[..n]));

        // Process complete segments (terminated by \r or \n)
        loop {
            let cr_pos = pending.find('\r');
            let lf_pos = pending.find('\n');

            let pos = match (cr_pos, lf_pos) {
                (Some(cr), Some(lf)) => cr.min(lf),
                (Some(cr), None) => cr,
                (None, Some(lf)) => lf,
                (None, None) => break,
            };

            let segment = pending[..pos].to_string();
            let is_cr = pending.as_bytes()[pos] == b'\r';
            pending = pending[pos + 1..].to_string();

            if segment.trim().is_empty() {
                continue;
            }

            if let Some(cleaned) = sanitize_progress_line(&segment) {
                if is_cr {
                    // \r = progress bar update → replace last line (throttled)
                    if last_replace.elapsed() >= throttle {
                        log::trace!("[onboarding] {} (replace): {}", label, cleaned);
                        emitter.emit_realtime("setup-progress-replace", &cleaned, None);
                        last_replace = std::time::Instant::now();
                    }
                } else {
                    // \n = new line → append
                    log::trace!("[onboarding] {} (append): {}", label, cleaned);
                    emitter.emit_realtime("setup-progress", &cleaned, None);
                }
            }
        }
    }

    // Emit any remaining content
    let remaining = pending.trim().to_string();
    if !remaining.is_empty() {
        if let Some(cleaned) = sanitize_progress_line(&remaining) {
            log::trace!("[onboarding] {} (final): {}", label, cleaned);
            emitter.emit_realtime("setup-progress", &cleaned, None);
        }
    }
}

/// Sanitize a progress line by handling ANSI cursor movement and stripping
/// escape sequences.  Cursor-forward (`\x1b[nC`) is replaced with *n* spaces
/// so that "Checking\x1b[1Cinstallation" becomes "Checking installation".
/// All other CSI / private-mode sequences are silently dropped.
/// Returns `None` when the sanitized result is empty (pure control line).
fn sanitize_progress_line(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // CSI sequence: \x1b[ [?] [digits;]* <letter>
            i += 2; // skip \x1b[

            let is_private = i < bytes.len() && bytes[i] == b'?';
            if is_private {
                i += 1;
            }

            // Parse numeric parameter (only last segment matters for CUF)
            let mut num = 0u32;
            let mut has_num = false;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b';') {
                if bytes[i].is_ascii_digit() {
                    num = num * 10 + (bytes[i] - b'0') as u32;
                    has_num = true;
                } else {
                    num = 0;
                    has_num = false;
                }
                i += 1;
            }
            if !has_num {
                num = 1;
            }

            if i < bytes.len() {
                if !is_private && bytes[i] == b'C' {
                    // CUF — Cursor Forward: replace with spaces
                    result.extend(std::iter::repeat_n(b' ', num.min(20) as usize));
                }
                // All other sequences are dropped
                i += 1;
            }
        } else if bytes[i] == 0x1b {
            // Non-CSI escape (e.g. \x1bM) — skip 2 bytes
            i += 2;
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }

    let cleaned = String::from_utf8_lossy(&result).trim().to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Check if a binary is available on PATH (cross-platform).
fn which_binary(name: &str) -> bool {
    claude_stream::which_binary(name).is_some()
}

fn node_version_satisfies(raw: &str, min_version: &str) -> bool {
    let min = match semver::Version::parse(min_version) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let cleaned = raw.trim().trim_start_matches('v');
    let token = cleaned.split_whitespace().next().unwrap_or(cleaned);
    semver::Version::parse(token).is_ok_and(|v| v >= min)
}

/// Check if npm is available and Node.js version satisfies min_version.
async fn check_npm_available_for(min_version: &str) -> bool {
    if !which_binary("npm") {
        return false;
    }
    let output = Command::new("node")
        .arg("--version")
        .env("PATH", claude_stream::augmented_path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .hide_console()
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => {
            let version = String::from_utf8_lossy(&o.stdout);
            node_version_satisfies(&version, min_version)
        }
        _ => false,
    }
}

/// Check if npm is available and Node.js version >= 18.
async fn check_npm_available() -> bool {
    check_npm_available_for("18.0.0").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_version_satisfies() {
        assert!(node_version_satisfies("v22.19.0", "22.19.0"));
        assert!(node_version_satisfies("v22.19.1", "22.19.0"));
        assert!(node_version_satisfies("v26.7.0", "22.19.0"));
        assert!(!node_version_satisfies("v22.18.9", "22.19.0"));
        assert!(!node_version_satisfies("v20.18.0", "22.19.0"));
        assert!(!node_version_satisfies("invalid", "22.19.0"));
    }

    #[test]
    fn preset_name_key_platforms() {
        // Guard against drift from frontend platform-presets.ts
        assert_eq!(preset_name("ccswitch"), "CC Switch");
        assert_eq!(preset_name("ccr"), "Claude Code Router");
        assert_eq!(preset_name("zhipu-intl"), "Zhipu (智谱 Intl)");
        assert_eq!(preset_name("minimax-cn"), "MiniMax (China)");
        assert_eq!(preset_name("zenmux"), "ZenMux");
        // Existing mappings
        assert_eq!(preset_name("anthropic"), "Anthropic");
        assert_eq!(preset_name("ollama"), "Ollama");
        // Unknown falls back to pid
        assert_eq!(preset_name("unknown-xyz"), "unknown-xyz");
    }

    #[tokio::test]
    async fn detect_install_methods_codex_includes_npm() {
        // npm install path is cross-platform and must always be listed for Codex.
        let methods = detect_install_methods("codex".into()).await.unwrap();
        let npm = methods
            .iter()
            .find(|m| m.id == "npm")
            .expect("codex npm method");
        assert_eq!(npm.command, "npm install -g @openai/codex");
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn detect_install_methods_codex_includes_brew_on_unix() {
        let methods = detect_install_methods("codex".into()).await.unwrap();
        let brew = methods
            .iter()
            .find(|m| m.id == "brew")
            .expect("codex brew method");
        assert_eq!(brew.command, "brew install codex");
    }

    #[tokio::test]
    async fn detect_install_methods_claude_default() {
        // Claude path keeps its existing commands (regression guard).
        let methods = detect_install_methods("claude".into()).await.unwrap();
        let npm = methods
            .iter()
            .find(|m| m.id == "npm")
            .expect("claude npm method");
        assert_eq!(npm.command, "npm install -g @anthropic-ai/claude-code");
    }

    #[tokio::test]
    async fn detect_install_methods_invalid_falls_back_to_claude() {
        // Unknown agent values fall back to Claude to preserve old callers.
        let methods = detect_install_methods("nonsense".into()).await.unwrap();
        let npm = methods.iter().find(|m| m.id == "npm").expect("npm method");
        assert!(npm.command.contains("@anthropic-ai/claude-code"));
    }
}
