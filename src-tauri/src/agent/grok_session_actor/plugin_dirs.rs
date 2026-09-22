use std::path::{Path, PathBuf};

/// Resolve the user-selected Claude plugin roots that are safe to expose to
/// Grok for one session.
pub(super) fn resolve_grok_plugin_dirs(configured: Option<&[String]>, cwd: &Path) -> Vec<String> {
    let mut resolved = Vec::new();

    for configured_path in configured.into_iter().flatten() {
        match validate_grok_plugin_dir(configured_path, cwd) {
            Ok(path) => {
                if !resolved.iter().any(|value| value == &path) {
                    resolved.push(path);
                }
            }
            Err(error) => {
                log::warn!(
                    "[grok] skipping configured plugin dir {:?}: {}",
                    configured_path,
                    error
                );
            }
        }
    }

    resolved
}

/// Validate one plugin root against Claude's trusted plugin locations.
///
/// The configured path must be an existing directory below either the user's
/// `~/.claude/plugins` tree or the active project's `.claude/plugins` tree.
/// Canonicalizing both sides makes the prefix check reject symlink escapes.
pub(super) fn validate_grok_plugin_dir(path: &str, cwd: &Path) -> Result<String, String> {
    validate_grok_plugin_dir_with_root(&crate::storage::data_dir(), path, cwd)
}

pub(super) fn validate_grok_plugin_dir_with_root(
    root: &Path,
    path: &str,
    cwd: &Path,
) -> Result<String, String> {
    let requested = PathBuf::from(path.trim());
    if requested.as_os_str().is_empty() {
        return Err("path is empty".to_string());
    }
    if !requested.is_absolute() {
        return Err("path must be absolute".to_string());
    }

    let canonical = std::fs::canonicalize(&requested)
        .map_err(|error| format!("cannot resolve path: {error}"))?;
    if !canonical.is_dir() {
        return Err("path is not a directory".to_string());
    }

    let allowed = allowed_plugin_roots_with_root(root, cwd);
    let inside_allowed_root = allowed
        .iter()
        .filter_map(|root| canonicalize(root))
        .any(|root| canonical.starts_with(&root) && canonical != root);
    if !inside_allowed_root {
        return Err("path is outside Claude plugin directories".to_string());
    }

    if !looks_like_plugin_root(&canonical) {
        return Err("directory does not contain a recognized plugin component".to_string());
    }

    Ok(canonical.to_string_lossy().into_owned())
}

fn allowed_plugin_roots_with_root(root: &Path, _cwd: &Path) -> Vec<PathBuf> {
    vec![
        root.join("plugins"),
        root.join("skills"),
        root.join("runtime"),
    ]
}

fn canonicalize(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

fn looks_like_plugin_root(path: &Path) -> bool {
    [
        ".claude-plugin",
        ".grok-plugin",
        "plugin.json",
        "skills",
        "commands",
        "agents",
        "hooks",
        "hooks.json",
        ".mcp.json",
        ".lsp.json",
    ]
    .iter()
    .any(|name| path.join(name).exists())
}

#[cfg(test)]
mod tests {
    use super::validate_grok_plugin_dir_with_root;
    use std::fs;
    use std::path::Path;

    fn make_plugin(root: &Path, name: &str) -> std::path::PathBuf {
        let plugin = root.join(name);
        fs::create_dir_all(plugin.join("skills")).unwrap();
        plugin
    }

    #[test]
    fn accepts_central_plugin_and_returns_canonical_path() {
        let data_dir = tempfile::tempdir().unwrap();
        let central_plugins = data_dir.path().join("plugins");
        let plugin = make_plugin(&central_plugins, "test_central_grok_plugin");

        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("project");

        let resolved =
            validate_grok_plugin_dir_with_root(data_dir.path(), plugin.to_str().unwrap(), &cwd)
                .unwrap();
        assert_eq!(
            resolved,
            plugin
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        );
    }

    #[test]
    fn rejects_paths_outside_claude_plugin_roots() {
        let data_dir = tempfile::tempdir().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("project");
        let plugin = make_plugin(temp.path(), "outside");

        let error =
            validate_grok_plugin_dir_with_root(data_dir.path(), plugin.to_str().unwrap(), &cwd)
                .unwrap_err();
        assert!(error.contains("outside Claude plugin directories"));
    }

    #[test]
    fn rejects_unmanaged_project_plugin_roots() {
        let data_dir = tempfile::tempdir().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("project");
        let plugin = make_plugin(&cwd.join(".claude/plugins"), "demo");

        let error =
            validate_grok_plugin_dir_with_root(data_dir.path(), plugin.to_str().unwrap(), &cwd)
                .unwrap_err();
        assert!(error.contains("outside Claude plugin directories"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        let data_dir = tempfile::tempdir().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("project");
        let plugin_root = cwd.join(".claude/plugins");
        fs::create_dir_all(&plugin_root).unwrap();
        let outside = make_plugin(temp.path(), "outside");
        let link = plugin_root.join("escape");
        std::os::unix::fs::symlink(&outside, &link).unwrap();

        let error =
            validate_grok_plugin_dir_with_root(data_dir.path(), link.to_str().unwrap(), &cwd)
                .unwrap_err();
        assert!(error.contains("outside Claude plugin directories"));
    }
}
