use crate::models::RunStatus;
use crate::process_ext::HideConsole;
use crate::storage;
use serde::Serialize;
use serde_json::json;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

const AGENTCABIN_MARKER: &str = ".agentcabin-managed";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitProjectInfo {
    pub project_root: String,
    pub checkout_root: String,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub is_top_level: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWorktreeInfo {
    pub path: String,
    pub branch: Option<String>,
    pub is_main: bool,
    pub is_current: bool,
    pub is_prunable: bool,
    pub is_dirty: bool,
    pub locked_reason: Option<String>,
}

fn git(cwd: &str, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("git")
        .current_dir(cwd)
        .args(args)
        .hide_console()
        .output()
        .map_err(|error| format!("failed to run git: {error}"))
}

fn git_checked(cwd: &str, args: &[&str]) -> Result<String, String> {
    let output = git(cwd, args)?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn canonical_for_comparison(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn is_top_level_checkout(cwd: &str, checkout_root: &str) -> bool {
    canonical_for_comparison(Path::new(cwd)) == canonical_for_comparison(Path::new(checkout_root))
}

fn path_is_within(root: &Path, path: &Path) -> bool {
    canonical_for_comparison(path).starts_with(canonical_for_comparison(root))
}

fn same_path(left: &Path, right: &Path) -> bool {
    canonical_for_comparison(left) == canonical_for_comparison(right)
}

fn worktree_git_dir(path: &str) -> Result<PathBuf, String> {
    let git_dir = git_checked(path, &["rev-parse", "--git-dir"])?;
    let git_path = PathBuf::from(&git_dir);
    Ok(if git_path.is_absolute() {
        git_path
    } else {
        Path::new(path).join(git_path)
    })
}

fn mark_agentcabin_worktree(path: &str) -> Result<(), String> {
    let git_dir = worktree_git_dir(path)?;
    std::fs::write(git_dir.join(AGENTCABIN_MARKER), b"AgentCabin\n")
        .map_err(|error| format!("mark AgentCabin worktree: {error}"))
}

fn is_agentcabin_worktree(path: &str) -> bool {
    worktree_git_dir(path)
        .map(|git_dir| git_dir.join(AGENTCABIN_MARKER).is_file())
        .unwrap_or(false)
}

fn eligible_for_auto_cleanup(
    item: &GitWorktreeInfo,
    managed: &HashSet<PathBuf>,
    active: &HashSet<PathBuf>,
    marked: bool,
) -> bool {
    !item.is_main
        && !item.is_current
        && !item.is_dirty
        && marked
        && managed
            .iter()
            .any(|path| same_path(path, Path::new(&item.path)))
        && !active
            .iter()
            .any(|run_cwd| path_is_within(Path::new(&item.path), run_cwd))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn configured_worktree_root(project_root: &str) -> PathBuf {
    let settings = storage::settings::get_user_settings();
    if let Some(root) = settings.worktree_root.as_deref() {
        let root = expand_home(root);
        let root = if root.is_absolute() {
            root
        } else if let Some(home) = home_dir() {
            home.join(root)
        } else {
            root
        };
        let project_name = Path::new(project_root)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "project".to_string());
        return root.join(project_name);
    }
    PathBuf::from(format!("{}-worktrees", project_root))
}

fn managed_path_set(settings: &crate::models::UserSettings) -> HashSet<PathBuf> {
    settings
        .worktree_managed_paths
        .iter()
        .map(|path| canonical_for_comparison(Path::new(path)))
        .collect()
}

fn active_worktree_paths() -> HashSet<PathBuf> {
    storage::runs::list_runs()
        .into_iter()
        .filter(|run| {
            matches!(
                run.status,
                RunStatus::Pending | RunStatus::Running | RunStatus::Idle
            )
        })
        .map(|run| canonical_for_comparison(Path::new(&run.cwd)))
        .collect()
}

fn persist_managed_paths(paths: Vec<String>) -> Result<(), String> {
    storage::settings::update_user_settings(json!({
        "worktree_managed_paths": paths,
    }))
    .map(|_| ())
}

fn register_managed_path(path: &Path) -> Result<(), String> {
    let mut settings = storage::settings::get_user_settings();
    let path = canonical_for_comparison(path).to_string_lossy().to_string();
    if !settings
        .worktree_managed_paths
        .iter()
        .any(|item| same_path(Path::new(item), Path::new(&path)))
    {
        settings.worktree_managed_paths.push(path);
        persist_managed_paths(settings.worktree_managed_paths)?;
    }
    Ok(())
}

fn unregister_managed_path(path: &Path) -> Result<(), String> {
    let settings = storage::settings::get_user_settings();
    let remaining: Vec<String> = settings
        .worktree_managed_paths
        .into_iter()
        .filter(|item| !same_path(Path::new(item), path))
        .collect();
    persist_managed_paths(remaining)
}

fn remove_worktree_path(cwd: &str, path: &str) -> Result<(), String> {
    let output = git(cwd, &["worktree", "remove", path])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Remove only old, clean worktrees that this app created. Main, active, current, dirty, and
/// unregistered worktrees are deliberately excluded even when the retention limit is exceeded.
fn cleanup_managed_worktrees(cwd: &str) -> Result<(), String> {
    let settings = storage::settings::get_user_settings();
    if !settings.worktree_auto_cleanup {
        return Ok(());
    }

    let managed = managed_path_set(&settings);
    let active = active_worktree_paths();
    let worktrees = parse_worktrees(cwd)?;
    let managed_count = worktrees
        .iter()
        .filter(|item| {
            managed
                .iter()
                .any(|path| same_path(path, Path::new(&item.path)))
        })
        .count();
    let limit = settings.worktree_cleanup_limit.max(1) as usize;
    if managed_count <= limit {
        return Ok(());
    }

    let mut candidates: Vec<_> = worktrees
        .into_iter()
        .filter(|item| {
            eligible_for_auto_cleanup(item, &managed, &active, is_agentcabin_worktree(&item.path))
        })
        .collect();
    candidates.sort_by_key(|item| {
        std::fs::metadata(&item.path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });

    let mut remaining = managed_count;
    for item in candidates {
        if remaining <= limit {
            break;
        }
        match remove_worktree_path(cwd, &item.path) {
            Ok(()) => {
                remaining -= 1;
                let _ = unregister_managed_path(Path::new(&item.path));
                log::info!(
                    "[worktrees] auto-removed old managed worktree: {}",
                    item.path
                );
            }
            Err(error) => {
                log::warn!(
                    "[worktrees] could not auto-remove managed worktree {}: {}",
                    item.path,
                    error
                );
            }
        }
    }
    Ok(())
}

fn project_info(cwd: &str) -> Result<GitProjectInfo, String> {
    let checkout_root = git_checked(cwd, &["rev-parse", "--show-toplevel"])?;
    let common_dir = git_checked(cwd, &["rev-parse", "--git-common-dir"])?;
    let git_dir = git_checked(cwd, &["rev-parse", "--git-dir"])?;
    let common_path = if Path::new(&common_dir).is_absolute() {
        PathBuf::from(common_dir)
    } else {
        Path::new(cwd).join(common_dir)
    };
    let project_root = common_path
        .parent()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| checkout_root.clone());
    let branch = git_checked(cwd, &["branch", "--show-current"])
        .ok()
        .filter(|value| !value.is_empty());
    let common_absolute = std::fs::canonicalize(&common_path).unwrap_or(common_path);
    let git_path = Path::new(&git_dir);
    let git_absolute = if git_path.is_absolute() {
        git_path.to_path_buf()
    } else {
        Path::new(cwd).join(git_path)
    };
    let is_worktree = std::fs::canonicalize(git_absolute)
        .map(|path| path != common_absolute)
        .unwrap_or(true);
    let is_top_level = is_top_level_checkout(cwd, &checkout_root);
    Ok(GitProjectInfo {
        project_root,
        checkout_root,
        branch,
        is_worktree,
        is_top_level,
    })
}

#[tauri::command]
pub async fn get_git_project(cwd: String) -> Result<GitProjectInfo, String> {
    project_info(&cwd)
}

fn parse_worktrees(cwd: &str) -> Result<Vec<GitWorktreeInfo>, String> {
    let current = std::fs::canonicalize(cwd).unwrap_or_else(|_| PathBuf::from(cwd));
    let output = git_checked(cwd, &["worktree", "list", "--porcelain"])?;
    let mut result = Vec::new();
    let mut path = None;
    let mut branch = None;
    let mut is_main = false;
    let mut is_prunable = false;
    let mut locked_reason = None;
    let flush = |result: &mut Vec<GitWorktreeInfo>,
                 path: &mut Option<String>,
                 branch: &mut Option<String>,
                 is_main: &mut bool,
                 is_prunable: &mut bool,
                 locked_reason: &mut Option<String>| {
        let Some(path_value) = path.take() else {
            return;
        };
        let path_buf = PathBuf::from(&path_value);
        let canonical = std::fs::canonicalize(&path_buf).unwrap_or(path_buf.clone());
        let is_dirty = git_checked(
            &path_value,
            &["status", "--porcelain", "--untracked-files=normal"],
        )
        .map(|status| !status.is_empty())
        .unwrap_or(false);
        result.push(GitWorktreeInfo {
            is_current: canonical == current,
            path: path_value,
            branch: branch.take(),
            is_main: *is_main,
            is_prunable: *is_prunable,
            is_dirty,
            locked_reason: locked_reason.take(),
        });
        *is_main = false;
        *is_prunable = false;
    };
    for line in output.lines() {
        if line.is_empty() {
            flush(
                &mut result,
                &mut path,
                &mut branch,
                &mut is_main,
                &mut is_prunable,
                &mut locked_reason,
            );
        } else if let Some(value) = line.strip_prefix("worktree ") {
            flush(
                &mut result,
                &mut path,
                &mut branch,
                &mut is_main,
                &mut is_prunable,
                &mut locked_reason,
            );
            path = Some(value.to_string());
            is_main = result.is_empty();
        } else if let Some(value) = line.strip_prefix("branch refs/heads/") {
            branch = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("locked") {
            locked_reason = Some(value.trim().to_string());
        } else if line.starts_with("prunable") {
            is_prunable = true;
        }
    }
    flush(
        &mut result,
        &mut path,
        &mut branch,
        &mut is_main,
        &mut is_prunable,
        &mut locked_reason,
    );
    Ok(result)
}

#[tauri::command]
pub async fn list_git_worktrees(cwd: String) -> Result<Vec<GitWorktreeInfo>, String> {
    parse_worktrees(&cwd)
}

fn safe_branch(branch: &str) -> Result<(), String> {
    if branch.trim().is_empty() {
        return Err("branch must not be empty".to_string());
    }
    let output = Command::new("git")
        .args(["check-ref-format", "--branch", branch])
        .hide_console()
        .output()
        .map_err(|error| format!("failed to validate branch: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "invalid branch: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[tauri::command]
pub async fn create_git_worktree(
    cwd: String,
    branch: String,
    path: Option<String>,
) -> Result<GitWorktreeInfo, String> {
    safe_branch(&branch)?;
    let info = project_info(&cwd)?;
    let target = path.map(PathBuf::from).unwrap_or_else(|| {
        configured_worktree_root(&info.project_root).join(branch.replace('/', "-"))
    });
    if !target.is_absolute() {
        return Err("worktree path must be absolute".to_string());
    }
    if target.exists() {
        return Err(format!(
            "worktree path already exists: {}",
            target.display()
        ));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create worktree parent: {error}"))?;
    }
    let branch_exists = git(
        cwd.as_str(),
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .map(|output| output.status.success())
    .unwrap_or(false);
    let target_string = target.to_string_lossy().to_string();
    let args = if branch_exists {
        vec!["worktree", "add", target_string.as_str(), branch.as_str()]
    } else {
        vec![
            "worktree",
            "add",
            "-b",
            branch.as_str(),
            target_string.as_str(),
        ]
    };
    let output = git(&cwd, &args)?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let target_canonical = std::fs::canonicalize(&target).unwrap_or(target.clone());
    let created = parse_worktrees(&cwd)?
        .into_iter()
        .find(|item| {
            item.path == target_string
                || std::fs::canonicalize(&item.path)
                    .map(|path| path == target_canonical)
                    .unwrap_or(false)
        })
        .ok_or_else(|| "git created the worktree but it could not be listed".to_string())?;
    if let Err(error) = mark_agentcabin_worktree(&created.path) {
        log::warn!(
            "[worktrees] could not mark worktree as AgentCabin-managed: {}: {}",
            created.path,
            error
        );
    } else {
        register_managed_path(Path::new(&created.path))?;
    }
    let _ = cleanup_managed_worktrees(&cwd);
    Ok(created)
}

#[tauri::command]
pub async fn remove_git_worktree(cwd: String, path: String, force: bool) -> Result<(), String> {
    let target = std::fs::canonicalize(&path).unwrap_or_else(|_| PathBuf::from(&path));
    let worktree = parse_worktrees(&cwd)?
        .into_iter()
        .find(|item| Path::new(&item.path) == target.as_path() || item.path == path)
        .ok_or_else(|| "worktree is not registered in this repository".to_string())?;
    if worktree.is_main {
        return Err("the main checkout cannot be removed".to_string());
    }
    if !force && worktree.is_dirty {
        return Err("worktree has uncommitted changes; pass force to remove it".to_string());
    }
    let worktree_root = canonical_for_comparison(Path::new(&worktree.path));
    let active = storage::runs::list_runs().into_iter().any(|run| {
        path_is_within(&worktree_root, Path::new(&run.cwd))
            && matches!(
                run.status,
                RunStatus::Pending | RunStatus::Running | RunStatus::Idle
            )
    });
    if active {
        return Err("worktree is used by an active AgentCabin run".to_string());
    }
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(worktree.path.as_str());
    let output = git(&cwd, &args)?;
    if output.status.success() {
        let _ = unregister_managed_path(&target);
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn run(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .expect("git is installed for worktree tests");
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-b", "main"]);
        run(dir.path(), &["config", "user.email", "test@example.com"]);
        run(dir.path(), &["config", "user.name", "AgentCabin Test"]);
        fs::write(dir.path().join("README"), "test\n").unwrap();
        run(dir.path(), &["add", "README"]);
        run(dir.path(), &["commit", "-m", "initial"]);
        dir
    }

    #[test]
    fn identifies_top_level_and_lists_created_worktree() {
        let dir = repo();
        let cwd = dir.path().to_string_lossy().to_string();
        let project = project_info(&cwd).unwrap();
        assert!(project.is_top_level);
        assert!(!project.is_worktree);

        let nested = dir.path().join("nested");
        fs::create_dir_all(&nested).unwrap();
        let nested_project = project_info(&nested.to_string_lossy()).unwrap();
        assert!(!nested_project.is_top_level);
        assert!(!nested_project.is_worktree);

        let target = dir.path().join("agent-worktree");
        let target_string = target.to_string_lossy().to_string();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let created = runtime
            .block_on(create_git_worktree(
                cwd.clone(),
                "agent/test".to_string(),
                Some(target_string.clone()),
            ))
            .unwrap();
        assert_eq!(created.branch.as_deref(), Some("agent/test"));
        assert!(is_agentcabin_worktree(&target_string));
        assert_eq!(list_git_worktrees_sync(&cwd).len(), 2);

        let linked_project = project_info(&target_string).unwrap();
        assert!(linked_project.is_top_level);
        assert!(linked_project.is_worktree);

        let target_canonical = fs::canonicalize(&target).unwrap();
        let listed = list_git_worktrees_sync(&cwd)
            .into_iter()
            .find(|item| fs::canonicalize(&item.path).ok() == Some(target_canonical.clone()))
            .unwrap();
        assert!(!listed.is_dirty);
        runtime
            .block_on(remove_git_worktree(cwd, target_string, false))
            .unwrap();
        assert!(!target.exists());
    }

    fn list_git_worktrees_sync(cwd: &str) -> Vec<GitWorktreeInfo> {
        parse_worktrees(cwd).unwrap()
    }

    #[test]
    fn active_run_scope_uses_path_components_not_string_prefixes() {
        let dir = repo();
        let main = dir.path().join("main");
        let linked = dir.path().join("main-agent");
        let similar = dir.path().join("main-agent-copy");
        fs::create_dir_all(main.join("src")).unwrap();
        fs::create_dir_all(linked.join("src")).unwrap();
        fs::create_dir_all(&similar).unwrap();

        assert!(path_is_within(&main, &main));
        assert!(path_is_within(&main, &main.join("src")));
        assert!(path_is_within(&linked, &linked.join("src")));
        assert!(!path_is_within(&linked, &similar));
    }

    #[test]
    fn cleanup_candidate_protects_unmanaged_dirty_current_and_active_worktrees() {
        let managed_path = PathBuf::from("/tmp/agentcabin-clean");
        let active_path = PathBuf::from("/tmp/agentcabin-active");
        let managed = HashSet::from([managed_path.clone(), active_path.clone()]);
        let active = HashSet::from([active_path.clone()]);
        let base = GitWorktreeInfo {
            path: managed_path.to_string_lossy().to_string(),
            branch: Some("agentcabin/test".to_string()),
            is_main: false,
            is_current: false,
            is_prunable: false,
            is_dirty: false,
            locked_reason: None,
        };

        assert!(eligible_for_auto_cleanup(&base, &managed, &active, true));
        assert!(!eligible_for_auto_cleanup(&base, &managed, &active, false));

        let mut dirty = base.clone();
        dirty.is_dirty = true;
        assert!(!eligible_for_auto_cleanup(&dirty, &managed, &active, true));

        let mut current = base.clone();
        current.is_current = true;
        assert!(!eligible_for_auto_cleanup(
            &current, &managed, &active, true
        ));

        let mut main = base.clone();
        main.is_main = true;
        assert!(!eligible_for_auto_cleanup(&main, &managed, &active, true));

        let mut active_item = base;
        active_item.path = active_path.to_string_lossy().to_string();
        assert!(!eligible_for_auto_cleanup(
            &active_item,
            &managed,
            &active,
            true
        ));

        let unmanaged = GitWorktreeInfo {
            path: "/tmp/user-created-worktree".to_string(),
            branch: Some("user/branch".to_string()),
            is_main: false,
            is_current: false,
            is_prunable: false,
            is_dirty: false,
            locked_reason: None,
        };
        assert!(!eligible_for_auto_cleanup(
            &unmanaged, &managed, &active, true
        ));
    }
}
