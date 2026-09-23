pub mod claude;
pub mod codex;
pub mod grok;
pub mod pi;

use crate::agent::capability_resolver::{
    EffectiveCapabilities, EffectiveMcpServer, EffectiveSkill, RuntimeProviderKind,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RuntimeSpawnConfig {
    pub binary: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub cwd: PathBuf,
    pub managed_home: PathBuf,
}

pub trait RuntimeProviderAdapter: Send + Sync {
    fn kind(&self) -> RuntimeProviderKind;
    fn prepare_runtime(&self, caps: &EffectiveCapabilities) -> Result<RuntimeSpawnConfig, String>;
    fn cleanup_runtime(&self, caps: &EffectiveCapabilities) -> Result<(), String>;
}

pub(crate) fn add_code_desktop_use_env(
    _env: &mut HashMap<String, String>,
    _app_mode: crate::work::models::AppMode,
) {
}

/// Build the provider-compatible MCP projection for Code desktop/browser use.
/// Pi Code loads the native extension directly, while Claude/Codex/Grok
/// consume this same Computer Use V2 contract through their standard MCP tool
/// surface. Browser-only sessions use the CDP backend inside the adapter and
/// do not require the native desktop helper.
pub(crate) fn code_desktop_mcp_server(
    caps: &EffectiveCapabilities,
) -> Result<Option<EffectiveMcpServer>, String> {
    let browser_capability_enabled = caps.browser_enabled || caps.browser_use_enabled;
    if caps.app_mode != crate::work::models::AppMode::Code
        || caps.runtime == RuntimeProviderKind::Pi
        || (!crate::work::desktop_operator::is_enabled() && !browser_capability_enabled)
    {
        return Ok(None);
    }

    const SERVER_ID: &str = "agentcabin_desktop";
    if caps.mcp_servers.iter().any(|server| server.id == SERVER_ID) {
        return Err(format!(
            "MCP server id '{}' is reserved by AgentCabin's built-in desktop capability",
            SERVER_ID
        ));
    }

    let adapter_path = caps.managed_runtime_dir.join("agentcabin_desktop_mcp.mjs");
    write_managed_file(
        &adapter_path,
        include_str!("../../work/desktop_mcp_adapter.mjs"),
        "Code desktop MCP adapter",
    )?;
    write_managed_file(
        &caps.managed_runtime_dir.join("computer_use_v2_runtime.mjs"),
        include_str!("../../work/computer_use_v2_runtime.mjs"),
        "Code Computer Use V2 runtime",
    )?;
    write_managed_file(
        &caps.managed_runtime_dir.join("computer_use_v3_models.mjs"),
        include_str!("../../work/computer_use_v3_models.mjs"),
        "Code Computer Use V3 models",
    )?;
    write_managed_file(
        &caps
            .managed_runtime_dir
            .join("desktop_computer_use_backend.mjs"),
        include_str!("../../work/desktop_computer_use_backend.mjs"),
        "Code Desktop backend",
    )?;
    write_managed_file(
        &caps
            .managed_runtime_dir
            .join("cdp_computer_use_backend.mjs"),
        include_str!("../../work/cdp_computer_use_backend.mjs"),
        "Code CDP backend",
    )?;
    write_managed_file(
        &caps
            .managed_runtime_dir
            .join("visual_grounding_backend.mjs"),
        include_str!("../../work/visual_grounding_backend.mjs"),
        "Code Visual Grounding backend",
    )?;
    let node = crate::agent::runtime_locator::resolve_node().or_else(|err| {
        if crate::agent::runtime_locator::packaged() {
            Err(err)
        } else {
            crate::agent::claude_stream::which_binary("node")
                .ok_or_else(|| "node binary not found".to_string())
        }
    })?;
    Ok(Some(EffectiveMcpServer {
        id: SERVER_ID.to_string(),
        transport: "stdio".to_string(),
        command: Some(node),
        args: vec![adapter_path.to_string_lossy().into_owned()],
        cwd: None,
        url: None,
        env: HashMap::new(),
        headers: HashMap::new(),
    }))
}

pub fn get_adapter(kind: RuntimeProviderKind) -> Box<dyn RuntimeProviderAdapter> {
    match kind {
        RuntimeProviderKind::Claude => Box::new(claude::ClaudeRuntimeAdapter),
        RuntimeProviderKind::Codex => Box::new(codex::CodexRuntimeAdapter),
        RuntimeProviderKind::Grok => Box::new(grok::GrokRuntimeAdapter),
        RuntimeProviderKind::Pi => Box::new(pi::PiRuntimeAdapter),
        RuntimeProviderKind::Dsh => Box::new(UnsupportedDshRuntimeAdapter),
    }
}

pub struct UnsupportedDshRuntimeAdapter;

impl RuntimeProviderAdapter for UnsupportedDshRuntimeAdapter {
    fn kind(&self) -> RuntimeProviderKind {
        RuntimeProviderKind::Dsh
    }

    fn prepare_runtime(&self, _caps: &EffectiveCapabilities) -> Result<RuntimeSpawnConfig, String> {
        Err("DeepSeek Harness (DSH) runtime has been removed".to_string())
    }

    fn cleanup_runtime(&self, caps: &EffectiveCapabilities) -> Result<(), String> {
        cleanup_managed_directory(&caps.managed_runtime_dir, "DSH runtime")
    }
}

/// Ensure a managed runtime directory is a real directory, never a symlink.
/// Provider configuration must stay inside AgentCabin's per-run projection.
pub(crate) fn ensure_real_directory(path: &Path, label: &str) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err(format!("Managed {label} path cannot be empty."));
    }

    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    if is_allowed_system_alias(&current) {
                        continue;
                    }
                    return Err(format!(
                        "Strict isolation error: managed {label} '{}' is not a real directory.",
                        current.display()
                    ));
                }
                if !metadata.is_dir() {
                    return Err(format!(
                        "Strict isolation error: managed {label} '{}' is not a real directory.",
                        current.display()
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|error| {
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        format!(
                            "Managed {label} {} was created concurrently; retry the operation.",
                            current.display()
                        )
                    } else {
                        format!(
                            "Failed to create managed {label} {}: {error}",
                            current.display()
                        )
                    }
                })?;
                let metadata = fs::symlink_metadata(&current).map_err(|error| {
                    format!(
                        "Failed to inspect managed {label} {} after creation: {error}",
                        current.display()
                    )
                })?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(format!(
                        "Strict isolation error: managed {label} '{}' is not a real directory.",
                        current.display()
                    ));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&current, fs::Permissions::from_mode(0o700));
                }
            }
            Err(error) => {
                return Err(format!(
                    "Failed to inspect managed {label} {}: {error}",
                    current.display()
                ));
            }
        }
    }
    Ok(())
}

/// Write a file in a managed runtime after rejecting a pre-existing symlink.
/// The parent is validated as a real directory so provider config cannot be
/// redirected outside `~/.agentcabin/runtime` by a stale path.
pub(crate) fn write_managed_file(
    path: &Path,
    contents: impl AsRef<[u8]>,
    label: &str,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Managed {label} path has no parent: {}", path.display()))?;
    ensure_real_directory(parent, &format!("{label} parent"))?;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "Strict isolation error: managed {label} '{}' is not a regular file.",
                path.display()
            ));
        }
    }
    if !path_has_no_symlink_components(path) {
        return Err(format!(
            "Strict isolation error: managed {label} path '{}' contains a symlink.",
            path.display()
        ));
    }

    fs::write(path, contents)
        .map_err(|e| format!("Failed to write managed {label} {}: {e}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

/// Remove a per-run managed directory without ever following a symlink.
///
/// Runtime cleanup is normally called for a path AgentCabin created itself, but
/// the path may have been replaced while the provider was running. Treat that
/// as an isolation violation instead of allowing `remove_dir_all` to operate on
/// an attacker-selected target.
pub(crate) fn cleanup_managed_directory(path: &Path, label: &str) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Failed to inspect managed {label} {} before cleanup: {error}",
                path.display()
            ));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "Strict isolation error: refusing to remove managed {label} '{}' because it is not a real directory.",
            path.display()
        ));
    }
    if !path_has_no_symlink_components(path) {
        return Err(format!(
            "Strict isolation error: refusing to remove managed {label} '{}' through a symlinked path.",
            path.display()
        ));
    }

    fs::remove_dir_all(path).map_err(|error| {
        format!(
            "Failed to remove managed {label} {}: {error}",
            path.display()
        )
    })
}

fn path_has_no_symlink_components(path: &Path) -> bool {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata)
                if metadata.file_type().is_symlink() && !is_allowed_system_alias(&current) =>
            {
                return false;
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => return false,
        }
    }
    true
}

fn is_allowed_system_alias(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        if path == Path::new("/var") || path == Path::new("/tmp") {
            return fs::canonicalize(path)
                .map(|canonical| {
                    (path == Path::new("/var") && canonical == Path::new("/private/var"))
                        || (path == Path::new("/tmp") && canonical == Path::new("/private/tmp"))
                })
                .unwrap_or(false);
        }
    }
    false
}

/// Serialize the already-resolved MCP projection into a TOML table. Provider
/// adapters must use the TOML serializer instead of interpolating values into
/// source text; MCP args, URLs, headers, and injected secrets may contain
/// quotes or control characters.
pub(crate) fn mcp_servers_toml_table(
    servers: &[EffectiveMcpServer],
    headers_key: &str,
) -> toml::value::Table {
    let mut mcp_table = toml::value::Table::new();
    for server in servers {
        let mut server_table = toml::value::Table::new();
        if let Some(command) = &server.command {
            server_table.insert("command".into(), toml::Value::String(command.clone()));
        }
        if !server.args.is_empty() {
            server_table.insert(
                "args".into(),
                toml::Value::Array(
                    server
                        .args
                        .iter()
                        .cloned()
                        .map(toml::Value::String)
                        .collect(),
                ),
            );
        }
        if let Some(cwd) = &server.cwd {
            server_table.insert(
                "cwd".into(),
                toml::Value::String(cwd.to_string_lossy().into_owned()),
            );
        }
        if let Some(url) = &server.url {
            server_table.insert("url".into(), toml::Value::String(url.clone()));
        }
        if !server.env.is_empty() {
            let env = server
                .env
                .iter()
                .map(|(key, value)| (key.clone(), toml::Value::String(value.clone())))
                .collect();
            server_table.insert("env".into(), toml::Value::Table(env));
        }
        if !server.headers.is_empty() {
            let headers = server
                .headers
                .iter()
                .map(|(key, value)| (key.clone(), toml::Value::String(value.clone())))
                .collect();
            server_table.insert(headers_key.to_string(), toml::Value::Table(headers));
        }
        mcp_table.insert(server.id.clone(), toml::Value::Table(server_table));
    }
    mcp_table
}

/// Safely project enabled skills into an isolated directory for a specific run.
/// Never creates symlinks back to native directories.
pub fn project_skills(
    target_skills_dir: &Path,
    enabled_skills: &[EffectiveSkill],
) -> Result<(), String> {
    let parent_dir = target_skills_dir
        .parent()
        .ok_or_else(|| "Target skills directory has no parent".to_string())?;
    ensure_real_directory(parent_dir, "skills parent")?;

    let tmp_staging = parent_dir.join(format!(
        ".skills_staging_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));

    ensure_real_directory(&tmp_staging, "skills staging directory")?;

    let stage_result = (|| {
        for skill in enabled_skills {
            validate_projection_name(&skill.name)?;
            let top_meta = fs::symlink_metadata(&skill.path).map_err(|e| {
                format!("Failed to inspect skill path {}: {e}", skill.path.display())
            })?;
            if top_meta.file_type().is_symlink() {
                return Err(format!(
                    "Strict isolation error: Skill path '{}' is a symlink. Symlinks in capabilities are rejected.",
                    skill.path.display()
                ));
            }
            if !top_meta.is_dir() && !top_meta.is_file() {
                return Err(format!(
                    "Strict isolation error: Skill path '{}' is not a regular file or directory.",
                    skill.path.display()
                ));
            }

            let skill_target = tmp_staging.join(&skill.name);
            ensure_real_directory(&skill_target, "skill staging directory")?;

            if top_meta.is_dir() {
                copy_dir_recursive_secure(&skill.path, &skill_target)?;
            } else {
                let file_name = skill.path.file_name().ok_or_else(|| {
                    format!("Skill file path has no file name: {}", skill.path.display())
                })?;
                let dest = skill_target.join(file_name);
                fs::copy(&skill.path, &dest).map_err(|e| {
                    format!(
                        "Failed to copy skill file {} to {}: {e}",
                        skill.path.display(),
                        dest.display()
                    )
                })?;
            }
        }
        Ok::<(), String>(())
    })();
    if let Err(error) = stage_result {
        let _ = remove_path(&tmp_staging);
        return Err(error);
    }

    // Replace the target only after the complete staging tree has been built.
    // Both paths live under the same parent, so each rename is atomic and a
    // failed commit can restore the previous tree.
    let old_backup = parent_dir.join(format!(
        ".skills_old_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));

    let existing_meta = match fs::symlink_metadata(target_skills_dir) {
        Ok(meta) => Some(meta),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            let _ = remove_path(&tmp_staging);
            return Err(format!(
                "Failed to inspect existing skills target {}: {error}",
                target_skills_dir.display()
            ));
        }
    };
    if existing_meta
        .as_ref()
        .is_some_and(|meta| meta.file_type().is_symlink())
    {
        let _ = remove_path(&tmp_staging);
        return Err(format!(
            "Strict isolation error: Skills target '{}' is a symlink.",
            target_skills_dir.display()
        ));
    }
    if existing_meta.as_ref().is_some_and(|meta| !meta.is_dir()) {
        let _ = remove_path(&tmp_staging);
        return Err(format!(
            "Strict isolation error: Skills target '{}' is not a directory.",
            target_skills_dir.display()
        ));
    }
    let has_existing = existing_meta.is_some();
    if has_existing {
        fs::rename(target_skills_dir, &old_backup).map_err(|e| {
            let _ = remove_path(&tmp_staging);
            format!(
                "Failed to stage existing target skills dir for replacement from {} to {}: {e}",
                target_skills_dir.display(),
                old_backup.display()
            )
        })?;
    }

    if let Err(e) = fs::rename(&tmp_staging, target_skills_dir) {
        // Attempt rollback if rename fails
        if has_existing {
            if let Err(rollback_error) = fs::rename(&old_backup, target_skills_dir) {
                return Err(format!(
                    "Failed to commit skills to {}: {e}; rollback also failed: {rollback_error}",
                    target_skills_dir.display()
                ));
            }
        }
        let _ = remove_path(&tmp_staging);
        return Err(format!(
            "Failed to atomically commit skills from {} to {}: {e}",
            tmp_staging.display(),
            target_skills_dir.display()
        ));
    }

    if has_existing {
        remove_path(&old_backup).map_err(|error| {
            format!(
                "Skills committed to {}, but failed to remove backup {}: {error}",
                target_skills_dir.display(),
                old_backup.display()
            )
        })?;
    }

    Ok(())
}

fn validate_projection_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(format!(
            "Strict isolation error: Invalid projected skill name '{name}'."
        ));
    }
    Ok(())
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        fs::remove_file(path)
    } else {
        fs::remove_dir_all(path)
    }
}

fn copy_dir_recursive_secure(src: &Path, dst: &Path) -> Result<(), String> {
    ensure_real_directory(dst, "skill projection directory")?;
    let entries =
        fs::read_dir(src).map_err(|e| format!("Failed to read dir {}: {e}", src.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to enumerate {}: {e}", src.display()))?;
        let path = entry.path();
        let name = entry.file_name();
        let target = dst.join(name);

        let meta = fs::symlink_metadata(&path)
            .map_err(|e| format!("Failed to get metadata for {}: {e}", path.display()))?;

        if meta.file_type().is_symlink() {
            return Err(format!(
                "Strict isolation error: Found symlink '{}' in capability package. Symlinks are rejected for projection.",
                path.display()
            ));
        }

        if meta.is_dir() {
            copy_dir_recursive_secure(&path, &target)?;
        } else if meta.is_file() {
            if let Ok(target_meta) = fs::symlink_metadata(&target) {
                if target_meta.file_type().is_symlink() || !target_meta.is_file() {
                    return Err(format!(
                        "Strict isolation error: refusing to overwrite non-regular projection target '{}'.",
                        target.display()
                    ));
                }
            }
            fs::copy(&path, &target).map_err(|e| {
                format!(
                    "Failed to copy file from {} to {}: {e}",
                    path.display(),
                    target.display()
                )
            })?;
        } else {
            return Err(format!(
                "Strict isolation error: Unsupported file type '{}' in capability package.",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Copy a managed directory tree into another managed runtime without
/// following symlinks. This is used by Claude fork to move only session state
/// from the source run's HOME into the new run's HOME; provider settings,
/// skills, and MCP files are regenerated by the target adapter.
pub(crate) fn copy_managed_directory(
    source: &Path,
    target: &Path,
    label: &str,
) -> Result<(), String> {
    let source_metadata = fs::symlink_metadata(source).map_err(|error| {
        format!(
            "Failed to inspect source managed {label} {}: {error}",
            source.display()
        )
    })?;
    if source_metadata.file_type().is_symlink()
        || !source_metadata.is_dir()
        || !path_has_no_symlink_components(source)
    {
        return Err(format!(
            "Strict isolation error: source managed {label} '{}' is not a real directory.",
            source.display()
        ));
    }
    ensure_real_directory(target, &format!("{label} target"))?;
    copy_dir_recursive_secure(source, target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn mcp_toml_projection_round_trips_special_characters() {
        let mut env = HashMap::new();
        env.insert(
            "TOKEN".to_string(),
            "value with \"quotes\"\nand newline".to_string(),
        );
        let mut headers = HashMap::new();
        headers.insert("X-Header".to_string(), "header = \"value\"".to_string());
        let servers = vec![EffectiveMcpServer {
            id: "server.with.dots".to_string(),
            transport: "stdio".to_string(),
            command: Some("/opt/bin/mcp \"server\"".to_string()),
            args: vec!["--query=a\"b".to_string(), "line\nvalue".to_string()],
            cwd: None,
            url: Some("https://example.test/path?q=\"quoted\"".to_string()),
            env,
            headers,
        }];

        let mut root = toml::value::Table::new();
        root.insert(
            "mcp_servers".to_string(),
            toml::Value::Table(mcp_servers_toml_table(&servers, "http_headers")),
        );
        let serialized = toml::to_string(&toml::Value::Table(root)).unwrap();
        let parsed: toml::Value = toml::from_str(&serialized).unwrap();
        let server = &parsed["mcp_servers"]["server.with.dots"];

        assert_eq!(server["command"].as_str(), Some("/opt/bin/mcp \"server\""));
        assert_eq!(server["args"][0].as_str(), Some("--query=a\"b"));
        assert_eq!(server["args"][1].as_str(), Some("line\nvalue"));
        assert_eq!(
            server["url"].as_str(),
            Some("https://example.test/path?q=\"quoted\"")
        );
        assert_eq!(
            server["env"]["TOKEN"].as_str(),
            Some("value with \"quotes\"\nand newline")
        );
        assert_eq!(
            server["http_headers"]["X-Header"].as_str(),
            Some("header = \"value\"")
        );
    }

    fn effective_skill(name: &str, path: PathBuf) -> EffectiveSkill {
        EffectiveSkill {
            owner: None,
            id: name.to_string(),
            name: name.to_string(),
            path,
            description: None,
        }
    }

    #[test]
    fn empty_projection_replaces_stale_skill_tree() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("runtime").join("skills");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("stale.txt"), "stale").unwrap();

        project_skills(&target, &[]).unwrap();

        assert!(target.is_dir());
        assert!(!target.join("stale.txt").exists());
    }

    #[test]
    fn invalid_projection_name_is_rejected_without_touching_target() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "skill").unwrap();
        let target = temp.path().join("runtime").join("skills");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("keep.txt"), "keep").unwrap();

        let error = project_skills(&target, &[effective_skill("../escape", source)]).unwrap_err();

        assert!(error.contains("Invalid projected skill name"));
        assert_eq!(fs::read_to_string(target.join("keep.txt")).unwrap(), "keep");
    }

    #[cfg(unix)]
    #[test]
    fn top_level_and_nested_symlinks_are_rejected() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let real = temp.path().join("real");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("SKILL.md"), "skill").unwrap();

        let top_level_link = temp.path().join("top-level-link");
        symlink(&real, &top_level_link).unwrap();
        let top_target = temp.path().join("top-runtime").join("skills");
        let top_error =
            project_skills(&top_target, &[effective_skill("linked", top_level_link)]).unwrap_err();
        assert!(top_error.contains("is a symlink"));
        assert!(!top_target.exists());

        let nested_real = temp.path().join("nested-real");
        fs::create_dir_all(&nested_real).unwrap();
        fs::write(nested_real.join("SKILL.md"), "skill").unwrap();
        symlink(&real, nested_real.join("escape")).unwrap();
        let nested_target = temp.path().join("nested-runtime").join("skills");
        let nested_error =
            project_skills(&nested_target, &[effective_skill("nested", nested_real)]).unwrap_err();
        assert!(nested_error.contains("Found symlink"));
        assert!(!nested_target.exists());
    }

    #[cfg(unix)]
    #[test]
    fn managed_file_writer_rejects_symlink_targets() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let parent = temp.path().join("runtime");
        fs::create_dir_all(&parent).unwrap();
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, "original").unwrap();
        let managed_path = parent.join("config.toml");
        symlink(&outside, &managed_path).unwrap();

        let error = write_managed_file(&managed_path, "overwritten", "test config").unwrap_err();

        assert!(error.contains("not a regular file"));
        assert_eq!(fs::read_to_string(outside).unwrap(), "original");
    }

    #[cfg(unix)]
    #[test]
    fn managed_cleanup_rejects_symlinked_runtime_dir() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.txt"), "keep").unwrap();
        let managed = temp.path().join("managed");
        symlink(&outside, &managed).unwrap();

        let error = cleanup_managed_directory(&managed, "test runtime").unwrap_err();

        assert!(error.contains("not a real directory"));
        assert!(outside.join("keep.txt").is_file());
        assert!(fs::symlink_metadata(&managed)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn managed_copy_rejects_nested_symlink() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "secret").unwrap();
        symlink(&outside, source.join("nested-link")).unwrap();
        let target = temp.path().join("target");

        let error = copy_managed_directory(&source, &target, "test session state").unwrap_err();

        assert!(error.contains("Found symlink"));
        assert!(!target.join("secret.txt").exists());
    }
}
