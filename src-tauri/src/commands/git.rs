use crate::process_ext::HideConsole;
use crate::web_server::broadcaster::BroadcastEmitter;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tauri::State;

#[derive(Serialize)]
pub struct GitFileStat {
    pub path: String,
    pub status: String,
    pub insertions: u32,
    pub deletions: u32,
}

#[derive(Serialize)]
pub struct GitSummary {
    pub branch: String,
    pub files: Vec<GitFileStat>,
    pub total_files: u32,
    pub total_insertions: u32,
    pub total_deletions: u32,
}

#[tauri::command]
pub async fn get_git_summary(cwd: String) -> Result<GitSummary, String> {
    log::debug!("[git] get_git_summary: cwd={}", cwd);

    // Branch name
    let branch = Command::new("git")
        .current_dir(&cwd)
        .args(["branch", "--show-current"])
        .hide_console()
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();

    // Per-file numstat (staged + unstaged vs HEAD)
    let numstat_output = Command::new("git")
        .current_dir(&cwd)
        .args(["diff", "--numstat", "HEAD"])
        .hide_console()
        .output()
        .map_err(|e| format!("Failed to run git diff --numstat: {}", e))?;

    // Status for file status codes (M/A/D/R/?)
    let status_output = Command::new("git")
        .current_dir(&cwd)
        .args(["status", "--short"])
        .hide_console()
        .output()
        .map_err(|e| format!("Failed to run git status: {}", e))?;

    // Parse status codes into a map: path → status char
    let status_str = String::from_utf8_lossy(&status_output.stdout);
    let mut status_map = std::collections::HashMap::new();
    for line in status_str.lines() {
        if line.len() < 4 {
            continue;
        }
        let xy = &line[..2];
        let path = line[3..].trim();
        // Pick the most relevant status: index (X) or worktree (Y)
        let code = if xy.starts_with('?') {
            "?"
        } else if xy.starts_with('A') || xy.ends_with('A') {
            "A"
        } else if xy.starts_with('D') || xy.ends_with('D') {
            "D"
        } else if xy.starts_with('R') || xy.ends_with('R') {
            "R"
        } else {
            "M"
        };
        // Handle renames: "R  old -> new"
        let actual_path = if let Some(arrow) = path.find(" -> ") {
            &path[arrow + 4..]
        } else {
            path
        };
        status_map.insert(actual_path.to_string(), code.to_string());
    }

    // Parse numstat: "insertions\tdeletions\tpath"
    let numstat_str = String::from_utf8_lossy(&numstat_output.stdout);
    let mut files = Vec::new();
    let mut total_ins: u32 = 0;
    let mut total_del: u32 = 0;

    for line in numstat_str.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        // Binary files show "-" for insertions/deletions
        let ins = parts[0].parse::<u32>().unwrap_or(0);
        let del = parts[1].parse::<u32>().unwrap_or(0);
        let path = parts[2].to_string();
        let status = status_map
            .get(&path)
            .cloned()
            .unwrap_or_else(|| "M".to_string());
        total_ins += ins;
        total_del += del;
        files.push(GitFileStat {
            path,
            status,
            insertions: ins,
            deletions: del,
        });
    }

    // Also add untracked files from status (not in numstat)
    for (path, code) in &status_map {
        if code == "?" && !files.iter().any(|f| &f.path == path) {
            files.push(GitFileStat {
                path: path.clone(),
                status: "?".to_string(),
                insertions: 0,
                deletions: 0,
            });
        }
    }

    let total_files = files.len() as u32;

    Ok(GitSummary {
        branch,
        files,
        total_files,
        total_insertions: total_ins,
        total_deletions: total_del,
    })
}

#[tauri::command]
pub async fn get_git_branch(cwd: String) -> Result<String, String> {
    log::debug!("[git] get_git_branch: cwd={}", cwd);

    // Step 1: structured probe (rev-parse plumbing, exit code semantics are well-defined)
    let check = match Command::new("git")
        .current_dir(&cwd)
        .args(["rev-parse", "--is-inside-work-tree"])
        .hide_console()
        .output()
    {
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                // git executable not installed → normal state, no branch badge
                log::debug!("[git] get_git_branch: git not installed, cwd={}", cwd);
                return Ok(String::new());
            }
            log::warn!("[git] get_git_branch I/O error: cwd={}, err={}", cwd, e);
            return Err(e.to_string());
        }
        Ok(o) => o,
    };

    if check.status.success() {
        let stdout = String::from_utf8_lossy(&check.stdout).trim().to_string();
        if stdout != "true" {
            // "false" → bare repo / inside .git dir → no branch badge
            log::debug!("[git] get_git_branch: not a work tree, cwd={}", cwd);
            return Ok(String::new());
        }
    } else {
        let code = check.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&check.stderr);
        let stderr_trimmed = stderr.trim();
        if code == 128 && stderr_trimmed.contains("not a git repository") {
            // Genuinely not a git directory → Ok("") (normal state, not an error)
            log::debug!("[git] get_git_branch: not a git repo, cwd={}", cwd);
            return Ok(String::new());
        }
        // Other failures (safe.directory, corruption, permissions, etc) → Err
        log::warn!(
            "[git] get_git_branch: rev-parse error, cwd={}, code={}",
            cwd,
            code
        );
        return Err(format!(
            "git rev-parse failed (code {}): {}",
            code, stderr_trimmed
        ));
    }

    // Step 2: get branch name
    let output = Command::new("git")
        .current_dir(&cwd)
        .args(["branch", "--show-current"])
        .hide_console()
        .output()
        .map_err(|e| {
            log::warn!("[git] get_git_branch: branch cmd I/O error: {}", e);
            e.to_string()
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::warn!(
            "[git] get_git_branch: branch cmd failed: code={}",
            output.status.code().unwrap_or(-1)
        );
        return Err(format!("git branch failed: {}", stderr.trim()));
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Step 3: detached HEAD → fallback to short SHA
    if branch.is_empty() {
        let sha = Command::new("git")
            .current_dir(&cwd)
            .args(["rev-parse", "--short", "HEAD"])
            .hide_console()
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        log::debug!(
            "[git] get_git_branch: detached HEAD, sha={}, cwd={}",
            sha,
            cwd
        );
        return Ok(sha);
    }

    Ok(branch)
}

#[tauri::command]
pub async fn get_git_diff(
    cwd: String,
    staged: bool,
    file: Option<String>,
) -> Result<String, String> {
    log::debug!(
        "[git] get_git_diff: cwd={}, staged={}, file={:?}",
        cwd,
        staged,
        file
    );
    let mut cmd = Command::new("git");
    cmd.current_dir(&cwd);
    cmd.arg("diff");
    if staged {
        cmd.arg("--cached");
    } else if file.is_some() {
        // Per-file diff: compare working tree against HEAD (staged + unstaged)
        cmd.arg("HEAD");
    }
    if let Some(ref f) = file {
        cmd.arg("--").arg(f);
    }
    let output = cmd
        .hide_console()
        .output()
        .map_err(|e| format!("Failed to run git diff: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git diff failed: {}", stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub async fn get_git_status(cwd: String) -> Result<String, String> {
    log::debug!("[git] get_git_status: cwd={}", cwd);
    let output = Command::new("git")
        .current_dir(&cwd)
        .args(["status", "--short"])
        .hide_console()
        .output()
        .map_err(|e| format!("Failed to run git status: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git status failed: {}", stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn git_repo_root(cwd: &Path) -> Result<PathBuf, String> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(["rev-parse", "--show-toplevel"])
        .hide_console()
        .output()
        .map_err(|error| format!("Failed to inspect git repository: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        return Err("Git repository root is empty".to_string());
    }
    Ok(PathBuf::from(root))
}

fn run_snapshot_git(root: &Path, index_path: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_INDEX_FILE", index_path)
        .args(args)
        .hide_console()
        .output()
        .map_err(|error| format!("Failed to run git {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Materialize the current working tree (tracked and untracked, excluding ignored files) as a
/// temporary Git tree object without touching the user's real index or working files.
fn create_worktree_tree(cwd: &Path) -> Result<String, String> {
    let root = git_repo_root(cwd)?;
    let index_path =
        std::env::temp_dir().join(format!("agentcabin-turn-index-{}", uuid::Uuid::new_v4()));

    let result = (|| {
        let read_head = run_snapshot_git(&root, &index_path, &["read-tree", "HEAD"]);
        if read_head.is_err() {
            run_snapshot_git(&root, &index_path, &["read-tree", "--empty"])?;
        }
        run_snapshot_git(&root, &index_path, &["add", "-A", "--", "."])?;
        let tree = run_snapshot_git(&root, &index_path, &["write-tree"])?;
        if !is_valid_git_oid(&tree) {
            return Err("git write-tree returned an invalid object id".to_string());
        }
        Ok(tree)
    })();

    let _ = std::fs::remove_file(&index_path);
    let _ = std::fs::remove_file(index_path.with_extension("lock"));
    result
}

fn is_valid_git_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn diff_worktree_trees(
    cwd: &Path,
    before: &str,
    after: &str,
    paths: &[String],
) -> Result<String, String> {
    if !is_valid_git_oid(before) || !is_valid_git_oid(after) {
        return Err("Invalid Git snapshot object id".to_string());
    }
    let root = git_repo_root(cwd)?;
    let mut args = vec![
        "diff-tree",
        "--no-commit-id",
        "--no-renames",
        "--binary",
        "--full-index",
        "-p",
        before,
        after,
    ];
    let normalized_paths: Vec<String> = paths
        .iter()
        .filter_map(|p| {
            let p_trim = p.trim();
            if p_trim.is_empty() {
                return None;
            }
            let p_path = Path::new(p_trim);
            if p_path.is_absolute() {
                if let Ok(rel) = p_path.strip_prefix(&root) {
                    return Some(rel.to_string_lossy().to_string());
                }
            }
            Some(p_trim.to_string())
        })
        .collect();

    if !normalized_paths.is_empty() {
        args.push("--");
        for p in &normalized_paths {
            args.push(p.as_str());
        }
    }

    let output = Command::new("git")
        .current_dir(&root)
        .args(&args)
        .hide_console()
        .output()
        .map_err(|error| format!("Failed to diff turn snapshots: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn apply_patch_to_worktree(cwd: &Path, patch: &[u8], reverse: bool) -> Result<(), String> {
    if patch.is_empty() {
        return Ok(());
    }
    if patch.len() > 32 * 1024 * 1024 {
        return Err("Turn patch is too large to apply safely".to_string());
    }
    let root = git_repo_root(cwd)?;

    let run_apply = |check: bool| -> Result<(), String> {
        let mut command = Command::new("git");
        command.current_dir(&root).arg("apply");
        if reverse {
            command.arg("--reverse");
        }
        if check {
            command.arg("--check");
        }
        let mut child = command
            .arg("--whitespace=nowarn")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .hide_console()
            .spawn()
            .map_err(|error| format!("Failed to start git apply: {error}"))?;
        child
            .stdin
            .take()
            .ok_or_else(|| "git apply stdin is unavailable".to_string())?
            .write_all(patch)
            .map_err(|error| format!("Failed to write git patch: {error}"))?;
        let output = child
            .wait_with_output()
            .map_err(|error| format!("Failed to wait for git apply: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(())
    };

    run_apply(true)?;
    run_apply(false)
}

#[tauri::command]
pub async fn create_git_turn_snapshot(cwd: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || create_worktree_tree(Path::new(&cwd)))
        .await
        .map_err(|error| format!("Git snapshot task failed: {error}"))?
}

#[tauri::command]
pub async fn diff_git_turn_snapshot(
    cwd: String,
    before_tree: String,
    paths: Option<Vec<String>>,
) -> Result<String, String> {
    if let Some(ref list) = paths {
        if list.is_empty() {
            return Ok(String::new());
        }
    }
    tokio::task::spawn_blocking(move || {
        let cwd = Path::new(&cwd);
        let after_tree = create_worktree_tree(cwd)?;
        diff_worktree_trees(
            cwd,
            &before_tree,
            &after_tree,
            paths.as_deref().unwrap_or(&[]),
        )
    })
    .await
    .map_err(|error| format!("Git snapshot diff task failed: {error}"))?
}

#[tauri::command]
pub async fn apply_git_turn_patch(cwd: String, patch: String, reverse: bool) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        apply_patch_to_worktree(Path::new(&cwd), patch.as_bytes(), reverse)
    })
    .await
    .map_err(|error| format!("Git patch task failed: {error}"))?
}

#[tauri::command]
pub async fn persist_turn_file_summary(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    run_id: String,
    summary_id: String,
    cwd: String,
    diff: String,
) -> Result<(), String> {
    if crate::storage::runs::get_run(&run_id).is_none() {
        return Err("Cannot save a file summary for an unknown run".to_string());
    }
    if summary_id.trim().is_empty() || summary_id.len() > 128 {
        return Err("Invalid turn file summary id".to_string());
    }
    if diff.trim().is_empty() {
        return Err("Cannot save an empty turn diff".to_string());
    }
    if diff.len() > 32 * 1024 * 1024 {
        return Err("Turn patch is too large to save safely".to_string());
    }
    git_repo_root(Path::new(&cwd))?;

    emitter.persist_and_emit(
        &run_id,
        &crate::models::BusEvent::TurnFileSummary {
            run_id: run_id.clone(),
            summary_id,
            cwd,
            diff,
        },
    );
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct GitBranchInfo {
    pub name: String,
    pub is_current: bool,
    pub uncommitted_count: usize,
    pub worktree_path: Option<String>,
}

fn checked_out_worktree_paths(cwd: &str) -> Result<HashMap<String, String>, String> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(["worktree", "list", "--porcelain"])
        .hide_console()
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let mut branches = HashMap::new();
    let mut worktree_path = None;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            worktree_path = Some(path.to_string());
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            if let Some(path) = worktree_path.as_ref() {
                branches.insert(branch.to_string(), path.clone());
            }
        } else if line.is_empty() {
            worktree_path = None;
        }
    }
    Ok(branches)
}

#[tauri::command]
pub async fn list_git_branches(cwd: String) -> Result<Vec<GitBranchInfo>, String> {
    log::debug!("[git] list_git_branches: cwd={}", cwd);
    let output = Command::new("git")
        .current_dir(&cwd)
        .args(["branch", "--list"])
        .hide_console()
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let status_output = Command::new("git")
        .current_dir(&cwd)
        .args(["status", "--porcelain"])
        .hide_console()
        .output()
        .ok();
    let uncommitted_count = status_output
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
        .unwrap_or(0);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let worktree_paths = checked_out_worktree_paths(&cwd).unwrap_or_default();
    let mut branches = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (is_current, name_start) = if let Some(rest) = trimmed.strip_prefix('*') {
            (true, rest)
        } else if let Some(rest) = trimmed.strip_prefix('+') {
            (false, rest)
        } else {
            (false, trimmed)
        };
        let name = name_start.trim().to_string();
        if !name.is_empty() && !name.contains("(HEAD detached") {
            branches.push(GitBranchInfo {
                name: name.clone(),
                is_current,
                uncommitted_count: if is_current { uncommitted_count } else { 0 },
                worktree_path: if is_current {
                    None
                } else {
                    worktree_paths.get(&name).cloned()
                },
            });
        }
    }
    Ok(branches)
}

#[tauri::command]
pub async fn checkout_git_branch(cwd: String, branch: String) -> Result<String, String> {
    log::debug!("[git] checkout_git_branch: cwd={}, branch={}", cwd, branch);
    if let Some(path) = checked_out_worktree_paths(&cwd)
        .ok()
        .and_then(|paths| paths.get(&branch).cloned())
    {
        let current_branch = Command::new("git")
            .current_dir(&cwd)
            .args(["branch", "--show-current"])
            .hide_console()
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .unwrap_or_default();
        if current_branch != branch {
            return Err(format!(
                "branch '{branch}' is already checked out at '{path}'"
            ));
        }
    }
    let output = Command::new("git")
        .current_dir(&cwd)
        .args(["checkout", &branch])
        .hide_console()
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    Ok(branch)
}

#[tauri::command]
pub async fn create_git_branch(cwd: String, branch: String) -> Result<String, String> {
    log::debug!("[git] create_git_branch: cwd={}, branch={}", cwd, branch);
    let output = Command::new("git")
        .current_dir(&cwd)
        .args(["checkout", "-b", &branch])
        .hide_console()
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    Ok(branch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn run(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .expect("git is installed for git command tests");
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
    fn reports_worktree_path_for_branch_checked_out_elsewhere() {
        let dir = repo();
        let cwd = dir.path().to_string_lossy().to_string();
        run(dir.path(), &["branch", "dev"]);
        let target = dir.path().join("agent-worktree");
        let target_string = target.to_string_lossy().to_string();
        run(
            dir.path(),
            &["worktree", "add", target_string.as_str(), "dev"],
        );

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let branches = runtime
            .block_on(list_git_branches(cwd))
            .expect("branch list should load");
        let dev = branches
            .iter()
            .find(|branch| branch.name == "dev")
            .expect("dev branch should be listed");
        let listed_path = Path::new(dev.worktree_path.as_deref().unwrap())
            .canonicalize()
            .unwrap();
        assert_eq!(listed_path, target.canonicalize().unwrap());

        let main = branches
            .iter()
            .find(|branch| branch.name == "main")
            .expect("main branch should be listed");
        assert!(main.worktree_path.is_none());
    }

    #[test]
    fn checkout_reports_when_branch_is_checked_out_elsewhere() {
        let dir = repo();
        let cwd = dir.path().to_string_lossy().to_string();
        run(dir.path(), &["branch", "dev"]);
        let target = dir.path().join("agent-worktree");
        let target_string = target.to_string_lossy().to_string();
        run(
            dir.path(),
            &["worktree", "add", target_string.as_str(), "dev"],
        );
        let expected_path = target.canonicalize().unwrap().display().to_string();

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let error = runtime
            .block_on(checkout_git_branch(cwd, "dev".to_string()))
            .expect_err("checkout should be rejected");
        assert_eq!(
            error,
            format!("branch 'dev' is already checked out at '{expected_path}'")
        );
    }

    #[test]
    fn turn_snapshot_diff_tracks_shell_edits_and_untracked_files() {
        let dir = repo();
        let before = create_worktree_tree(dir.path()).unwrap();
        fs::write(dir.path().join("README"), "test\nchanged by shell\n").unwrap();
        fs::write(dir.path().join("new.txt"), "new\n").unwrap();
        let after = create_worktree_tree(dir.path()).unwrap();

        let diff = diff_worktree_trees(dir.path(), &before, &after, &[]).unwrap();
        assert!(diff.contains("diff --git a/README b/README"));
        assert!(diff.contains("+changed by shell"));
        assert!(diff.contains("diff --git a/new.txt b/new.txt"));
    }

    #[test]
    fn turn_snapshot_diff_constrains_to_specified_paths() {
        let dir = repo();
        let before = create_worktree_tree(dir.path()).unwrap();
        fs::write(dir.path().join("README"), "test\nchanged\n").unwrap();
        fs::write(dir.path().join("other.txt"), "other\n").unwrap();
        let after = create_worktree_tree(dir.path()).unwrap();

        // Diff ONLY README: must NOT contain other.txt from concurrent work
        let diff =
            diff_worktree_trees(dir.path(), &before, &after, &["README".to_string()]).unwrap();
        assert!(diff.contains("diff --git a/README b/README"));
        assert!(!diff.contains("other.txt"));
    }

    #[test]
    fn reverse_turn_patch_restores_the_pre_turn_snapshot() {
        let dir = repo();
        let before = create_worktree_tree(dir.path()).unwrap();
        fs::write(dir.path().join("README"), "test\nchanged\n").unwrap();
        fs::write(dir.path().join("new.txt"), "new\n").unwrap();
        let after = create_worktree_tree(dir.path()).unwrap();
        let diff = diff_worktree_trees(dir.path(), &before, &after, &[]).unwrap();

        apply_patch_to_worktree(dir.path(), diff.as_bytes(), true).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("README")).unwrap(),
            "test\n"
        );
        assert!(!dir.path().join("new.txt").exists());
    }

    #[test]
    fn stale_turn_patch_fails_without_changing_newer_work() {
        let dir = repo();
        let before = create_worktree_tree(dir.path()).unwrap();
        fs::write(dir.path().join("README"), "test\nfirst turn\n").unwrap();
        let after = create_worktree_tree(dir.path()).unwrap();
        let diff = diff_worktree_trees(dir.path(), &before, &after, &[]).unwrap();

        fs::write(dir.path().join("README"), "test\nnewer conflicting turn\n").unwrap();
        let error = apply_patch_to_worktree(dir.path(), diff.as_bytes(), true)
            .expect_err("a stale historical patch must not overwrite newer work");

        assert!(!error.is_empty());
        assert_eq!(
            fs::read_to_string(dir.path().join("README")).unwrap(),
            "test\nnewer conflicting turn\n"
        );
    }
}

#[tauri::command]
pub async fn run_terminal_command(cwd: String, command: String) -> Result<String, String> {
    log::debug!(
        "[terminal] run_terminal_command: cwd={}, cmd={}",
        cwd,
        command
    );
    let shell = if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "zsh"
    };
    let flag = if cfg!(target_os = "windows") {
        "/C"
    } else {
        "-c"
    };

    let output = Command::new(shell)
        .current_dir(&cwd)
        .args([flag, &command])
        .hide_console()
        .output()
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() && !stderr.is_empty() {
        return Ok(format!("{}{}", stdout, stderr));
    }
    Ok(if stdout.is_empty() { stderr } else { stdout })
}
