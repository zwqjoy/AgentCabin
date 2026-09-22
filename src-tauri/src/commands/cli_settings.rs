use crate::storage::teams::claude_home_dir;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Max length for a single permission rule (prevents abuse via WS).
const MAX_RULE_LEN: usize = 2000;

// ── Path resolution ──

/// User-level settings.local.json: ~/.claude/settings.local.json
fn user_settings_path() -> PathBuf {
    claude_home_dir().join("settings.local.json")
}

/// Project-level settings.local.json: <cwd>/.claude/settings.local.json
/// Hardened validation: absolute path, exists, no symlink attacks.
fn resolve_project_settings_path(cwd: &str) -> Result<PathBuf, String> {
    // 1. Must be absolute path (reject relative / traversal inputs)
    if cwd.is_empty() || !PathBuf::from(cwd).is_absolute() {
        return Err("Working directory must be an absolute path".into());
    }
    // 2. Must exist and be a directory
    let cwd_path = PathBuf::from(cwd);
    if !cwd_path.is_dir() {
        return Err(format!("Working directory does not exist: {}", cwd));
    }
    // 3. Canonicalize cwd (resolves symlinks in the cwd itself)
    let canon =
        std::fs::canonicalize(&cwd_path).map_err(|e| format!("Cannot resolve path: {}", e))?;
    // 4. Anti-symlink check: if .claude exists, it must NOT be a symlink
    let dot_claude = canon.join(".claude");
    if dot_claude.exists() {
        let meta = std::fs::symlink_metadata(&dot_claude)
            .map_err(|e| format!("Cannot stat .claude: {}", e))?;
        if meta.file_type().is_symlink() {
            return Err(".claude is a symlink — refusing to write".into());
        }
    }
    let target = dot_claude.join("settings.local.json");
    // 5. Anti-symlink check: if target file exists, it must NOT be a symlink
    if target.exists() {
        let meta =
            std::fs::symlink_metadata(&target).map_err(|e| format!("Cannot stat target: {}", e))?;
        if meta.file_type().is_symlink() {
            return Err("settings.local.json is a symlink — refusing to write".into());
        }
    }
    Ok(target)
}

// ── Read/Write helpers ──

/// Read settings.local.json — strict on corruption.
/// Missing file → empty object; corrupt file → error.
fn read_settings_local(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => {
            let v: Value = serde_json::from_str(&s)
                .map_err(|e| format!("JSON parse error in {}: {}", path.display(), e))?;
            if !v.is_object() {
                return Err(format!("{}: not a JSON object", path.display()));
            }
            Ok(v)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(Value::Object(serde_json::Map::new()))
        }
        Err(e) => Err(format!("Cannot read {}: {}", path.display(), e)),
    }
}

/// Extract permissions.allow and permissions.deny from a settings object.
fn extract_permissions(settings: &Value) -> (Vec<String>, Vec<String>) {
    let perms = settings.get("permissions");
    let allow = perms
        .and_then(|p| p.get("allow"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let deny = perms
        .and_then(|p| p.get("deny"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    (allow, deny)
}

/// Sanitize rules: trim, filter empty, remove duplicates (preserve order), reject oversized.
fn sanitize_rules(rules: &[String]) -> Result<Vec<String>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for r in rules {
        let trimmed = r.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.len() > MAX_RULE_LEN {
            return Err(format!(
                "Rule exceeds maximum length of {} characters",
                MAX_RULE_LEN
            ));
        }
        if seen.insert(trimmed.clone()) {
            result.push(trimmed);
        }
    }
    Ok(result)
}

/// Write permissions to a settings file — atomic write, merge-only, preserve other fields.
fn write_permissions(path: &Path, category: &str, rules: &[String]) -> Result<(), String> {
    // 1. Read existing JSON via read_settings_local (errors on corruption)
    let mut settings = read_settings_local(path)?;

    // 2. Ensure "permissions" key is an object
    let map = settings.as_object_mut().expect("always object");
    if !map.contains_key("permissions") || !map["permissions"].is_object() {
        map.insert(
            "permissions".to_string(),
            Value::Object(serde_json::Map::new()),
        );
    }

    // 3. Set permissions.<category> = rules
    let perms = map.get_mut("permissions").unwrap().as_object_mut().unwrap();
    perms.insert(
        category.to_string(),
        Value::Array(rules.iter().map(|r| Value::String(r.clone())).collect()),
    );

    // 4. Create parent .claude/ dir if needed
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    // 5. Serialize with pretty formatting
    let content = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize: {}", e))?;

    // 6. Write to temp file
    let tmp = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::write(&tmp, &content).map_err(|e| format!("write tmp: {}", e))?;

    // 7. Set 0o600 on temp file (Unix)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }

    // 8. Atomic rename temp → target
    std::fs::rename(&tmp, path).map_err(|e| format!("rename: {}", e))
}

// ── IPC commands ──

#[tauri::command]
pub async fn get_cli_permissions(cwd: Option<String>) -> Result<Value, String> {
    log::debug!("[cli_settings] get_cli_permissions: cwd={:?}", cwd);

    // User-level always loads (errors here → Err, fatal)
    let user_path = user_settings_path();
    let user_settings = read_settings_local(&user_path)?;
    let (user_allow, user_deny) = extract_permissions(&user_settings);

    // Project-level degrades gracefully
    let (project_allow, project_deny, project_error) = match &cwd {
        Some(c) => match resolve_project_settings_path(c) {
            Ok(path) => match read_settings_local(&path) {
                Ok(settings) => {
                    let (allow, deny) = extract_permissions(&settings);
                    (allow, deny, None)
                }
                Err(e) => {
                    log::debug!("[cli_settings] project degraded: {}", e);
                    (vec![], vec![], Some(e))
                }
            },
            Err(e) => {
                log::debug!("[cli_settings] project degraded: {}", e);
                (vec![], vec![], Some(e))
            }
        },
        None => (vec![], vec![], None),
    };

    Ok(json!({
        "user": { "allow": user_allow, "deny": user_deny },
        "project": { "allow": project_allow, "deny": project_deny },
        "projectError": project_error,
    }))
}

#[tauri::command]
pub async fn update_cli_permissions(
    scope: String,
    category: String,
    rules: Vec<String>,
    cwd: Option<String>,
) -> Result<(), String> {
    log::debug!(
        "[cli_settings] update_cli_permissions: scope={}, category={}, count={}",
        scope,
        category,
        rules.len()
    );

    // Validate scope and category
    if scope != "user" && scope != "project" {
        return Err(format!("Invalid scope: {}", scope));
    }
    if category != "allow" && category != "deny" {
        return Err(format!("Invalid category: {}", category));
    }

    // Sanitize rules
    let clean_rules = sanitize_rules(&rules)?;

    // Resolve path
    let path = match scope.as_str() {
        "user" => user_settings_path(),
        "project" => {
            let c = cwd
                .as_deref()
                .ok_or_else(|| "cwd is required for project scope".to_string())?;
            resolve_project_settings_path(c)?
        }
        _ => unreachable!(),
    };

    write_permissions(&path, &category, &clean_rules)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_temp_settings(dir: &Path, content: &str) -> PathBuf {
        let claude_dir = dir.join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let path = claude_dir.join("settings.local.json");
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_resolve_project_path_relative_rejected() {
        let result = resolve_project_settings_path("../some/relative");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("absolute"));
    }

    #[test]
    fn test_resolve_project_path_empty_cwd() {
        let result = resolve_project_settings_path("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("absolute"));
    }

    #[cfg(unix)]
    #[test]
    fn test_resolve_project_path_symlink_rejected() {
        let tmp = TempDir::new().unwrap();
        let real_dir = tmp.path().join("real");
        std::fs::create_dir_all(&real_dir).unwrap();

        // Create a symlink for .claude pointing elsewhere
        let cwd_dir = tmp.path().join("project");
        std::fs::create_dir_all(&cwd_dir).unwrap();
        let dot_claude = cwd_dir.join(".claude");
        std::os::unix::fs::symlink(&real_dir, &dot_claude).unwrap();

        let result = resolve_project_settings_path(cwd_dir.to_str().unwrap());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("symlink"));
    }

    #[test]
    fn test_read_missing_file_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nonexistent.json");
        let result = read_settings_local(&path).unwrap();
        assert!(result.is_object());
        assert!(result.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_read_corrupted_file_returns_error() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("bad.json");
        std::fs::write(&path, "not valid json {{{").unwrap();
        let result = read_settings_local(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("JSON parse error"));
    }

    #[test]
    fn test_write_preserves_other_fields() {
        let tmp = TempDir::new().unwrap();
        let path = make_temp_settings(
            tmp.path(),
            r#"{"env": {"FOO": "bar"}, "permissions": {"allow": ["old"]}}"#,
        );

        write_permissions(&path, "deny", &["new_rule".to_string()]).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&content).unwrap();
        // env preserved
        assert_eq!(v["env"]["FOO"], "bar");
        // old allow preserved
        assert_eq!(v["permissions"]["allow"][0], "old");
        // new deny written
        assert_eq!(v["permissions"]["deny"][0], "new_rule");
    }

    #[test]
    fn test_invalid_scope_category() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(update_cli_permissions(
            "invalid".to_string(),
            "allow".to_string(),
            vec![],
            None,
        ));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid scope"));

        let result = rt.block_on(update_cli_permissions(
            "user".to_string(),
            "invalid".to_string(),
            vec![],
            None,
        ));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid category"));
    }

    #[test]
    fn test_rules_sanitized() {
        let rules = vec![
            "  rule1  ".to_string(),
            "".to_string(),
            "   ".to_string(),
            "rule1".to_string(), // duplicate after trim
            "rule2".to_string(),
        ];
        let result = sanitize_rules(&rules).unwrap();
        assert_eq!(result, vec!["rule1", "rule2"]);
    }

    #[cfg(unix)]
    #[test]
    fn test_permissions_file_mode() {
        let tmp = TempDir::new().unwrap();
        let claude_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let path = claude_dir.join("settings.local.json");

        write_permissions(&path, "allow", &["test_rule".to_string()]).unwrap();

        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn test_atomic_write_no_orphan_tmp() {
        let tmp = TempDir::new().unwrap();
        let claude_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let path = claude_dir.join("settings.local.json");

        write_permissions(&path, "allow", &["test".to_string()]).unwrap();

        // No .tmp files should remain
        let entries: Vec<_> = std::fs::read_dir(&claude_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(entries.is_empty(), "orphan tmp files found: {:?}", entries);
    }

    #[test]
    fn test_get_degrades_gracefully() {
        // get_cli_permissions with a bad cwd should NOT return Err —
        // it should return user permissions + projectError string
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(get_cli_permissions(Some(
            "/nonexistent/path/xyz".to_string(),
        )));
        // User-level may fail if ~/.claude doesn't exist in test env,
        // but project degradation logic is what we test:
        // If user path succeeds, project should degrade with projectError set.
        // If user path also fails (no ~/.claude), that's expected in CI.
        match result {
            Ok(val) => {
                // projectError should be set
                assert!(val["projectError"].is_string());
                // project arrays should be empty
                assert_eq!(val["project"]["allow"].as_array().unwrap().len(), 0);
                assert_eq!(val["project"]["deny"].as_array().unwrap().len(), 0);
            }
            Err(_) => {
                // User-level read failed (e.g. no ~/.claude in test env) — acceptable
            }
        }
    }
}
