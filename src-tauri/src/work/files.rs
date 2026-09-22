use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Duration, Utc};

use crate::work::models::{WorkFileSummary, WorkScratchCleanupResult};
use crate::work::paths::WorkPaths;
use crate::work::workspace;
use crate::work::workspace::WorkspaceManager;

const MAX_IMPORT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_CONTEXT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_LIST_FILES: usize = 2_000;
pub const DEFAULT_SCRATCH_RETENTION_DAYS: u32 = 14;
const MAX_SCRATCH_RETENTION_DAYS: u32 = 3_650;
const WORKSPACE_AREAS: &[&str] = &["input", "scratch", "output", "context"];
const INTERNAL_CONTEXT_PATHS: &[&str] = &["context/artifacts.json"];

pub fn list(workspace_id: &str, area: Option<&str>) -> Result<Vec<WorkFileSummary>, String> {
    list_with_paths(&WorkPaths::app(), workspace_id, area)
}

pub fn import(workspace_id: &str, source_path: &str) -> Result<WorkFileSummary, String> {
    let result = import_with_paths(&WorkPaths::app(), workspace_id, source_path)?;
    let _ = workspace::manager().touch(workspace_id);
    Ok(result)
}

pub fn open(workspace_id: &str, relative_path: &str) -> Result<(), String> {
    open_with_paths(&WorkPaths::app(), workspace_id, relative_path)
}

pub fn open_directory(workspace_id: &str, relative_path: &str) -> Result<(), String> {
    open_directory_with_paths(&WorkPaths::app(), workspace_id, relative_path)
}

pub fn read_context_file(workspace_id: &str, relative_path: &str) -> Result<String, String> {
    read_context_file_with_paths(&WorkPaths::app(), workspace_id, relative_path)
}

pub fn save_context_file(
    workspace_id: &str,
    relative_path: &str,
    content: &str,
    overwrite: bool,
) -> Result<WorkFileSummary, String> {
    let result = save_context_file_with_paths(
        &WorkPaths::app(),
        workspace_id,
        relative_path,
        content,
        overwrite,
    )?;
    let _ = workspace::manager().touch(workspace_id);
    Ok(result)
}

pub fn remove_context_file(workspace_id: &str, relative_path: &str) -> Result<(), String> {
    remove_context_file_with_paths(&WorkPaths::app(), workspace_id, relative_path)?;
    let _ = workspace::manager().touch(workspace_id);
    Ok(())
}

pub fn remove_input_file(workspace_id: &str, relative_path: &str) -> Result<(), String> {
    remove_input_file_with_paths(&WorkPaths::app(), workspace_id, relative_path)?;
    let _ = workspace::manager().touch(workspace_id);
    Ok(())
}

pub fn cleanup_scratch(
    workspace_id: &str,
    retention_days: u32,
    dry_run: bool,
) -> Result<WorkScratchCleanupResult, String> {
    let result =
        cleanup_scratch_with_paths(&WorkPaths::app(), workspace_id, retention_days, dry_run)?;
    if !dry_run && !result.removed.is_empty() {
        let _ = workspace::manager().touch(workspace_id);
    }
    Ok(result)
}

pub fn open_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    relative_path: &str,
) -> Result<(), String> {
    let clean = relative_path.trim().trim_start_matches("file://");
    let p = Path::new(clean);

    // 1. Direct absolute path check
    if p.is_absolute() {
        if let Ok(canon) = fs::canonicalize(p) {
            if canon.is_file() {
                return open_default_application(&canon);
            }
        }
    }

    // 2. Internal workspace areas (input, scratch, output, context)
    if let Ok(path) = existing_workspace_file(paths, workspace_id, clean) {
        return open_default_application(&path);
    }

    // 3. Standalone task files
    if let Ok(path) = existing_standalone_file(paths, workspace_id, clean) {
        return open_default_application(&path);
    }

    // 4. Workspace primary folder, access roots, and workspace root
    if let Ok(ws) = workspace::manager().get(workspace_id) {
        // 4a. Check primary work root
        if let Ok(primary_root) = workspace::manager().resolve_primary_work_root(&ws) {
            let candidate = primary_root.join(clean);
            if let Ok(canon) = fs::canonicalize(&candidate) {
                if canon.is_file() {
                    return open_default_application(&canon);
                }
            }

            // If clean has a leading folder component matching primary_root's name, strip it
            // e.g. primary_root is "/.../3人行", clean is "3人行/chapters/001.md"
            if let Some(folder_name) = primary_root.file_name().and_then(|n| n.to_str()) {
                if clean.starts_with(folder_name) {
                    let stripped = clean[folder_name.len()..].trim_start_matches(['/', '\\']);
                    let candidate = primary_root.join(stripped);
                    if let Ok(canon) = fs::canonicalize(&candidate) {
                        if canon.is_file() {
                            return open_default_application(&canon);
                        }
                    }
                }
            }

            // If primary_root contains a subdirectory with the file
            // (e.g. primary_root is "/.../love-novelist", and inside it is "3人行/chapters/001.md", but clean is "chapters/001.md")
            if let Ok(entries) = fs::read_dir(&primary_root) {
                for entry in entries.flatten() {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_dir() {
                            let sub_candidate = entry.path().join(clean);
                            if let Ok(canon) = fs::canonicalize(&sub_candidate) {
                                if canon.is_file() {
                                    return open_default_application(&canon);
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4b. Check access roots
        for root in &ws.access_roots {
            let candidate = Path::new(&root.path).join(clean);
            if let Ok(canon) = fs::canonicalize(&candidate) {
                if canon.is_file() {
                    return open_default_application(&canon);
                }
            }
        }

        // 4c. Check workspace directory
        if let Ok(ws_dir) = paths.workspace_dir(workspace_id) {
            let candidate = ws_dir.join(clean);
            if let Ok(canon) = fs::canonicalize(&candidate) {
                if canon.is_file() {
                    return open_default_application(&canon);
                }
            }
        }
    }

    Err(format!(
        "File '{relative_path}' not found in workspace or task '{workspace_id}'"
    ))
}

pub fn open_directory_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    relative_path: &str,
) -> Result<(), String> {
    let raw = relative_path.trim();
    let p = Path::new(raw);

    // 1. If target is absolute or points to a folder directly
    if p.is_absolute() {
        if let Ok(canon) = fs::canonicalize(p) {
            if canon.is_dir() {
                if let Ok(ws) = workspace::manager().get(workspace_id) {
                    if let Ok(primary_root) = workspace::manager().resolve_primary_work_root(&ws) {
                        if canon == primary_root || canon.starts_with(&primary_root) {
                            return open_default_application(&canon);
                        }
                    }
                    for root in &ws.access_roots {
                        if let Ok(ar_canon) = fs::canonicalize(&root.path) {
                            if canon == ar_canon || canon.starts_with(&ar_canon) {
                                return open_default_application(&canon);
                            }
                        }
                    }
                    if let Ok(ws_root) = fs::canonicalize(&ws.root) {
                        if canon == ws_root || canon.starts_with(&ws_root) {
                            return open_default_application(&canon);
                        }
                    }
                }
            }
        }
    }

    // 2. If target is empty or "."
    if raw.is_empty() || raw == "." {
        if let Ok(ws) = workspace::manager().get(workspace_id) {
            if let Ok(primary_root) = workspace::manager().resolve_primary_work_root(&ws) {
                return open_default_application(&primary_root);
            }
        }
    }

    // 3. Try resolve_and_confine_target
    if let Ok(candidate) = paths.resolve_and_confine_target(workspace_id, None, raw, false) {
        if candidate.is_dir() {
            if let Ok(canon) = fs::canonicalize(&candidate) {
                return open_default_application(&canon);
            }
        }
    }

    // 4. Standalone task path fallback
    if let Ok(candidate) = paths.resolve_standalone_path(workspace_id, Path::new(raw), false) {
        if candidate.is_dir() {
            if let Ok(canon) = fs::canonicalize(&candidate) {
                return open_default_application(&canon);
            }
        }
    }

    Err(format!(
        "Directory '{relative_path}' not found in workspace or task '{workspace_id}'"
    ))
}

pub fn read_context_file_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    relative_path: &str,
) -> Result<String, String> {
    let path = context_file_path(paths, workspace_id, relative_path, true)?;
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("Cannot inspect Workspace context file: {error}"))?;
    if metadata.len() > MAX_CONTEXT_BYTES {
        return Err(format!(
            "Workspace context file is too large (maximum {} MB)",
            MAX_CONTEXT_BYTES / 1024 / 1024
        ));
    }
    fs::read_to_string(path)
        .map_err(|error| format!("Failed to read Workspace context file: {error}"))
}

pub fn save_context_file_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    relative_path: &str,
    content: &str,
    overwrite: bool,
) -> Result<WorkFileSummary, String> {
    let path = context_file_path(paths, workspace_id, relative_path, false)?;
    if path.exists() && !overwrite {
        return Err("Workspace context file already exists".into());
    }
    let size = content.len() as u64;
    if size > MAX_CONTEXT_BYTES {
        return Err(format!(
            "Workspace context file is too large (maximum {} MB)",
            MAX_CONTEXT_BYTES / 1024 / 1024
        ));
    }
    fs::write(&path, content)
        .map_err(|error| format!("Failed to save Workspace context file: {error}"))?;
    let workspace_root = paths.workspace_dir(workspace_id)?;
    summary_for_path(&workspace_root, &path)
}

pub fn remove_context_file_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    relative_path: &str,
) -> Result<(), String> {
    let path = context_file_path(paths, workspace_id, relative_path, true)?;
    fs::remove_file(path)
        .map_err(|error| format!("Failed to remove Workspace context file: {error}"))
}

fn context_file_path(
    paths: &WorkPaths,
    workspace_id: &str,
    relative_path: &str,
    require_existing: bool,
) -> Result<PathBuf, String> {
    open_workspace(paths, workspace_id)?;
    let relative = Path::new(relative_path);
    let components = relative.components().collect::<Vec<_>>();
    if components.len() != 2
        || !matches!(components[0], Component::Normal(value) if value.to_string_lossy() == "context")
    {
        return Err("Workspace context path must be a direct file under context/".into());
    }
    let file_name = match components[1] {
        Component::Normal(value) => value.to_string_lossy(),
        _ => return Err("Workspace context path must point to a file".into()),
    };
    if file_name.trim().is_empty()
        || file_name == "."
        || file_name == ".."
        || file_name
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err("Workspace context file name is empty or unsafe".into());
    }
    if is_internal_context_path(relative_path) {
        return Err(
            "This Workspace context file is managed internally and cannot be edited".into(),
        );
    }

    let candidate = paths.resolve_workspace_path(workspace_id, relative, true)?;
    let workspace_root = fs::canonicalize(paths.workspace_dir(workspace_id)?)
        .map_err(|error| format!("Cannot read Workspace root: {error}"))?;
    let context_root = fs::canonicalize(workspace_root.join("context"))
        .map_err(|error| format!("Cannot read Workspace context directory: {error}"))?;
    let parent = candidate
        .parent()
        .ok_or_else(|| "Workspace context path has no parent".to_string())?;
    let parent = fs::canonicalize(parent)
        .map_err(|error| format!("Cannot read Workspace context directory: {error}"))?;
    if parent != context_root {
        return Err("Workspace context path must stay inside context/".into());
    }

    match fs::symlink_metadata(&candidate) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err("Symlinked Workspace context files are not supported".into())
        }
        Ok(metadata) if !metadata.is_file() => Err("Workspace context path must be a file".into()),
        Ok(_) => Ok(candidate),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !require_existing => {
            Ok(candidate)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err("Workspace context file was not found".into())
        }
        Err(error) => Err(format!("Cannot inspect Workspace context file: {error}")),
    }
}

fn remove_input_file_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    relative_path: &str,
) -> Result<(), String> {
    open_workspace(paths, workspace_id)?;
    let candidate = paths.resolve_workspace_path(workspace_id, Path::new(relative_path), false)?;
    let workspace_root = fs::canonicalize(paths.workspace_dir(workspace_id)?)
        .map_err(|error| format!("Cannot read Workspace root: {error}"))?;
    let input_root = fs::canonicalize(workspace_root.join("input"))
        .map_err(|error| format!("Cannot read Workspace input directory: {error}"))?;
    let path = fs::canonicalize(&candidate)
        .map_err(|error| format!("Cannot read Workspace input file: {error}"))?;
    if !path.starts_with(&input_root) {
        return Err("Only files in the Workspace input area can be removed".into());
    }
    let metadata = fs::symlink_metadata(&candidate)
        .map_err(|error| format!("Cannot inspect Workspace input file: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("Symlinked Workspace input files cannot be removed".into());
    }
    if !metadata.is_file() {
        return Err("Workspace input path must be a file".into());
    }
    fs::remove_file(candidate)
        .map_err(|error| format!("Failed to remove Workspace input file: {error}"))
}

pub fn list_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    area: Option<&str>,
) -> Result<Vec<WorkFileSummary>, String> {
    open_workspace(paths, workspace_id)?;
    let area = normalize_area(area)?;
    let workspace_root = paths.workspace_dir(workspace_id)?;
    let workspace = WorkspaceManager::new(paths.clone()).get(workspace_id)?;
    let primary_root = WorkspaceManager::new(paths.clone())
        .resolve_primary_work_root(&workspace)
        .ok();
    let areas = area
        .map(|value| vec![value])
        .unwrap_or_else(|| WORKSPACE_AREAS.to_vec());

    let mut files = Vec::new();
    for area in areas {
        let root = paths.resolve_workspace_path(workspace_id, Path::new(area), false)?;
        let summary_root = primary_root
            .as_ref()
            .filter(|primary| root.starts_with(primary))
            .unwrap_or(&workspace_root);
        collect_files(summary_root, &root, &mut files)?;
        if files.len() >= MAX_LIST_FILES {
            break;
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files.truncate(MAX_LIST_FILES);
    Ok(files)
}

pub fn cleanup_scratch_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    retention_days: u32,
    dry_run: bool,
) -> Result<WorkScratchCleanupResult, String> {
    cleanup_scratch_with_paths_at(paths, workspace_id, retention_days, dry_run, Utc::now())
}

fn cleanup_scratch_with_paths_at(
    paths: &WorkPaths,
    workspace_id: &str,
    retention_days: u32,
    dry_run: bool,
    now: DateTime<Utc>,
) -> Result<WorkScratchCleanupResult, String> {
    if !(1..=MAX_SCRATCH_RETENTION_DAYS).contains(&retention_days) {
        return Err(format!(
            "Scratch retention must be between 1 and {MAX_SCRATCH_RETENTION_DAYS} days"
        ));
    }

    open_workspace(paths, workspace_id)?;
    let cutoff = now - Duration::days(i64::from(retention_days));
    let candidates = list_with_paths(paths, workspace_id, Some("scratch"))?
        .into_iter()
        .filter(|file| is_older_than(file, cutoff))
        .collect::<Vec<_>>();

    if dry_run {
        return Ok(WorkScratchCleanupResult {
            retention_days,
            cutoff_at: cutoff.to_rfc3339(),
            dry_run: true,
            candidates,
            removed: Vec::new(),
            removed_bytes: 0,
        });
    }

    let mut removed = Vec::new();
    let mut removed_bytes = 0;
    for candidate in &candidates {
        let path = match existing_scratch_file_path(paths, workspace_id, &candidate.path) {
            Ok(path) => path,
            Err(error) if error.contains("not found") => continue,
            Err(error) => return Err(error),
        };
        let latest = summary_for_path(&paths.workspace_dir(workspace_id)?, &path)?;
        if !is_older_than(&latest, cutoff) {
            continue;
        }
        fs::remove_file(&path)
            .map_err(|error| format!("Failed to remove scratch file: {error}"))?;
        removed_bytes += latest.size;
        removed.push(latest);
    }

    Ok(WorkScratchCleanupResult {
        retention_days,
        cutoff_at: cutoff.to_rfc3339(),
        dry_run: false,
        candidates,
        removed,
        removed_bytes,
    })
}

pub fn import_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    source_path: &str,
) -> Result<WorkFileSummary, String> {
    open_workspace(paths, workspace_id)?;
    let source = PathBuf::from(source_path);
    if !source.is_absolute() {
        return Err("Input file path must be absolute".into());
    }
    let source =
        fs::canonicalize(&source).map_err(|error| format!("Cannot read input file: {error}"))?;
    let metadata =
        fs::metadata(&source).map_err(|error| format!("Cannot stat input file: {error}"))?;
    if !metadata.is_file() {
        return Err("Input path must be a file".into());
    }
    if metadata.len() > MAX_IMPORT_BYTES {
        return Err(format!(
            "Input file is too large (maximum {} MB)",
            MAX_IMPORT_BYTES / 1024 / 1024
        ));
    }

    let name = sanitize_file_name(
        source
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "Input file has no usable file name".to_string())?,
    )?;
    let relative = unique_input_path(paths, workspace_id, &name)?;
    let destination = paths.resolve_workspace_path(workspace_id, Path::new(&relative), false)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::copy(&source, &destination)
        .map_err(|error| format!("Failed to import input file: {error}"))?;

    let workspace_root = paths.workspace_dir(workspace_id)?;
    summary_for_path(&workspace_root, &destination)
}

fn open_workspace(paths: &WorkPaths, workspace_id: &str) -> Result<(), String> {
    WorkspaceManager::new(paths.clone())
        .open(workspace_id)
        .map(|_| ())
}

fn existing_workspace_file(
    paths: &WorkPaths,
    workspace_id: &str,
    relative_path: &str,
) -> Result<PathBuf, String> {
    if is_internal_context_path(relative_path) {
        return Err("This Workspace context file is managed internally".into());
    }
    let candidate = paths.resolve_workspace_path(workspace_id, Path::new(relative_path), false)?;
    let workspace_root = fs::canonicalize(paths.workspace_dir(workspace_id)?)
        .map_err(|error| format!("Cannot read Workspace root: {error}"))?;
    let path = fs::canonicalize(&candidate)
        .map_err(|error| format!("Cannot read Workspace file: {error}"))?;
    let primary_root = WorkspaceManager::new(paths.clone())
        .get(workspace_id)
        .ok()
        .and_then(|workspace| {
            WorkspaceManager::new(paths.clone())
                .resolve_primary_work_root(&workspace)
                .ok()
        });
    if !path.starts_with(&workspace_root)
        && !primary_root
            .as_ref()
            .is_some_and(|primary| path.starts_with(primary))
    {
        return Err("Workspace file resolves outside the Workspace".into());
    }
    if !path.is_file() {
        return Err("Workspace path must be a file".into());
    }
    Ok(path)
}

fn existing_standalone_file(
    paths: &WorkPaths,
    run_id: &str,
    relative_path: &str,
) -> Result<PathBuf, String> {
    let candidate = paths.resolve_standalone_path(run_id, Path::new(relative_path), false)?;
    let task_root = fs::canonicalize(paths.standalone_task_dir(run_id)?)
        .map_err(|error| format!("Cannot read standalone task root: {error}"))?;
    let path = fs::canonicalize(&candidate)
        .map_err(|error| format!("Cannot read standalone task file: {error}"))?;
    if !path.starts_with(&task_root) {
        return Err("Standalone task file resolves outside the task directory".into());
    }
    if !path.is_file() {
        return Err("Standalone task path must be a file".into());
    }
    Ok(path)
}

fn existing_scratch_file_path(
    paths: &WorkPaths,
    workspace_id: &str,
    relative_path: &str,
) -> Result<PathBuf, String> {
    let candidate = paths.resolve_workspace_path(workspace_id, Path::new(relative_path), false)?;
    if !relative_path.replace('\\', "/").starts_with("scratch/") {
        return Err("Only files in the Workspace scratch area can be cleaned".into());
    }
    let metadata = fs::symlink_metadata(&candidate)
        .map_err(|error| format!("Cannot inspect scratch file: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("Symlinked Workspace scratch files cannot be cleaned".into());
    }
    if !metadata.is_file() {
        return Err("Workspace scratch path must be a file".into());
    }
    let workspace_root = fs::canonicalize(paths.workspace_dir(workspace_id)?)
        .map_err(|error| format!("Cannot read Workspace root: {error}"))?;
    let scratch_root = fs::canonicalize(workspace_root.join("scratch"))
        .map_err(|error| format!("Cannot read Workspace scratch directory: {error}"))?;
    let resolved = fs::canonicalize(&candidate)
        .map_err(|error| format!("Cannot read Workspace scratch file: {error}"))?;
    if !resolved.starts_with(&scratch_root) {
        return Err("Workspace scratch file resolves outside scratch/".into());
    }
    Ok(candidate)
}

fn is_older_than(file: &WorkFileSummary, cutoff: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(&file.modified_at)
        .map(|modified| modified.with_timezone(&Utc) < cutoff)
        .unwrap_or(false)
}

fn is_internal_context_path(relative_path: &str) -> bool {
    INTERNAL_CONTEXT_PATHS
        .iter()
        .any(|path| *path == relative_path.replace('\\', "/"))
}

fn open_default_application(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|error| {
                format!("Failed to open file with the default application: {error}")
            })?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let path = path.to_string_lossy().into_owned();
        std::process::Command::new("cmd")
            .args(["/C", "start", "", path.as_str()])
            .spawn()
            .map_err(|error| {
                format!("Failed to open file with the default application: {error}")
            })?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|error| {
                format!("Failed to open file with the default application: {error}")
            })?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("Opening Workspace files is unsupported on this platform".into())
}

fn normalize_area(area: Option<&str>) -> Result<Option<&str>, String> {
    let area = area.map(str::trim).filter(|value| !value.is_empty());
    if let Some(area) = area {
        if !WORKSPACE_AREAS.contains(&area) {
            return Err("Unknown Workspace file area".into());
        }
    }
    Ok(area)
}

fn collect_files(
    workspace_root: &Path,
    directory: &Path,
    output: &mut Vec<WorkFileSummary>,
) -> Result<(), String> {
    if output.len() >= MAX_LIST_FILES || !directory.is_dir() {
        return Ok(());
    }
    let entries = fs::read_dir(directory).map_err(|error| error.to_string())?;
    for entry in entries {
        if output.len() >= MAX_LIST_FILES {
            break;
        }
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_files(workspace_root, &path, output)?;
        } else if file_type.is_file() {
            let summary = summary_for_path(workspace_root, &path)?;
            if !is_internal_context_path(&summary.path) {
                output.push(summary);
            }
        }
    }
    Ok(())
}

fn summary_for_path(workspace_root: &Path, path: &Path) -> Result<WorkFileSummary, String> {
    let relative = path
        .strip_prefix(workspace_root)
        .map_err(|_| "Workspace file is outside the Workspace".to_string())?;
    let relative = relative.to_string_lossy().replace('\\', "/");
    let area = relative
        .split('/')
        .next()
        .filter(|value| WORKSPACE_AREAS.contains(value))
        .ok_or_else(|| "Workspace file is outside a known area".to_string())?
        .to_string();
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let modified_at = metadata
        .modified()
        .ok()
        .map(|value| DateTime::<Utc>::from(value).to_rfc3339())
        .unwrap_or_default();
    Ok(WorkFileSummary {
        path: relative,
        name,
        area,
        size: metadata.len(),
        modified_at,
    })
}

fn unique_input_path(paths: &WorkPaths, workspace_id: &str, name: &str) -> Result<String, String> {
    let first = format!("input/{name}");
    let first_path = paths.resolve_workspace_path(workspace_id, Path::new(&first), false)?;
    if !first_path.exists() {
        return Ok(first);
    }

    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(name);
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 2..=10_000 {
        let candidate_name = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem} ({index}).{extension}"),
            _ => format!("{stem} ({index})"),
        };
        let candidate = format!("input/{candidate_name}");
        let candidate_path =
            paths.resolve_workspace_path(workspace_id, Path::new(&candidate), false)?;
        if !candidate_path.exists() {
            return Ok(candidate);
        }
    }
    Err("Too many files with the same name in input".into())
}

fn sanitize_file_name(name: &str) -> Result<String, String> {
    let mut normalized = name
        .chars()
        .map(|character| {
            if character.is_control() || matches!(character, '/' | '\\') {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    normalized = normalized.trim().to_string();
    if normalized.is_empty() || normalized == "." || normalized == ".." {
        return Err("Input file name is empty or unsafe".into());
    }
    if normalized.chars().count() > 180 {
        normalized = normalized.chars().take(180).collect();
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::workspace::WorkspaceManager;
    use tempfile::TempDir;

    fn setup() -> (TempDir, WorkPaths, String) {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let workspace = WorkspaceManager::new(paths.clone())
            .create("Import test")
            .unwrap();
        (temp, paths, workspace.id)
    }

    #[test]
    fn imports_file_into_read_only_input_area() {
        let (temp, paths, workspace_id) = setup();
        let source = temp.path().join("source.txt");
        fs::write(&source, "hello").unwrap();

        let summary = import_with_paths(&paths, &workspace_id, source.to_str().unwrap()).unwrap();
        assert_eq!(summary.path, "input/source.txt");
        assert_eq!(
            fs::read_to_string(
                paths
                    .workspace_dir(&workspace_id)
                    .unwrap()
                    .join(&summary.path)
            )
            .unwrap(),
            "hello"
        );
        assert!(paths
            .resolve_workspace_path(&workspace_id, Path::new("input/source.txt"), true)
            .is_err());
    }

    #[test]
    fn duplicate_names_are_kept_without_overwriting() {
        let (temp, paths, workspace_id) = setup();
        let source = temp.path().join("source.txt");
        fs::write(&source, "one").unwrap();
        import_with_paths(&paths, &workspace_id, source.to_str().unwrap()).unwrap();
        fs::write(&source, "two").unwrap();
        let second = import_with_paths(&paths, &workspace_id, source.to_str().unwrap()).unwrap();
        assert_eq!(second.path, "input/source (2).txt");
    }

    #[test]
    fn lists_files_without_following_symlinks() {
        let (temp, paths, workspace_id) = setup();
        let workspace = paths.workspace_dir(&workspace_id).unwrap();
        fs::write(workspace.join("input/readme.md"), "hello").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(temp.path(), workspace.join("input/outside")).unwrap();
        }
        let files = list_with_paths(&paths, &workspace_id, Some("input")).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "input/readme.md");
    }

    #[test]
    fn refuses_to_open_files_outside_workspace() {
        let (_temp, paths, workspace_id) = setup();
        assert!(existing_workspace_file(&paths, &workspace_id, "../outside.txt").is_err());
    }

    #[test]
    fn removes_only_input_files() {
        let (temp, paths, workspace_id) = setup();
        let source = temp.path().join("source.txt");
        fs::write(&source, "hello").unwrap();
        let summary = import_with_paths(&paths, &workspace_id, source.to_str().unwrap()).unwrap();
        let file_path = paths
            .workspace_dir(&workspace_id)
            .unwrap()
            .join(&summary.path);

        remove_input_file_with_paths(&paths, &workspace_id, &summary.path).unwrap();

        assert!(!file_path.exists());
    }

    #[test]
    fn refuses_to_remove_files_outside_input() {
        let (_temp, paths, workspace_id) = setup();
        let workspace = paths.workspace_dir(&workspace_id).unwrap();
        let draft = workspace.join("scratch/draft.txt");
        fs::write(&draft, "draft").unwrap();

        assert!(remove_input_file_with_paths(&paths, &workspace_id, "scratch/draft.txt").is_err());
        assert!(draft.exists());
    }

    #[test]
    fn saves_reads_updates_and_removes_context_files() {
        let (_temp, paths, workspace_id) = setup();

        let saved = save_context_file_with_paths(
            &paths,
            &workspace_id,
            "context/project-rules.md",
            "# Rules\n\nUse concise answers.\n",
            false,
        )
        .unwrap();
        assert_eq!(saved.path, "context/project-rules.md");
        assert_eq!(
            read_context_file_with_paths(&paths, &workspace_id, &saved.path).unwrap(),
            "# Rules\n\nUse concise answers.\n"
        );

        save_context_file_with_paths(
            &paths,
            &workspace_id,
            &saved.path,
            "# Rules\n\nUse direct answers.\n",
            true,
        )
        .unwrap();
        assert_eq!(
            read_context_file_with_paths(&paths, &workspace_id, &saved.path).unwrap(),
            "# Rules\n\nUse direct answers.\n"
        );

        remove_context_file_with_paths(&paths, &workspace_id, &saved.path).unwrap();
        assert!(read_context_file_with_paths(&paths, &workspace_id, &saved.path).is_err());
    }

    #[test]
    fn hides_and_protects_internal_artifact_registry() {
        let (_temp, paths, workspace_id) = setup();
        let workspace = paths.workspace_dir(&workspace_id).unwrap();
        let registry = workspace.join("context/artifacts.json");
        fs::write(&registry, "{\"version\":1,\"artifacts\":[]}").unwrap();

        let files = list_with_paths(&paths, &workspace_id, Some("context")).unwrap();
        assert!(files.is_empty());
        assert!(
            read_context_file_with_paths(&paths, &workspace_id, "context/artifacts.json").is_err()
        );
        assert!(save_context_file_with_paths(
            &paths,
            &workspace_id,
            "context/artifacts.json",
            "{}",
            true,
        )
        .is_err());
        assert!(
            remove_context_file_with_paths(&paths, &workspace_id, "context/artifacts.json")
                .is_err()
        );
        assert!(existing_workspace_file(&paths, &workspace_id, "context/artifacts.json").is_err());
        assert!(registry.exists());
    }

    #[test]
    fn previews_and_cleans_stale_scratch_files() {
        let (_temp, paths, workspace_id) = setup();
        let scratch = paths.workspace_dir(&workspace_id).unwrap().join("scratch");
        let draft = scratch.join("old-draft.md");
        fs::write(&draft, "draft").unwrap();
        let now = Utc::now() + Duration::days(2);

        let preview = cleanup_scratch_with_paths_at(&paths, &workspace_id, 1, true, now).unwrap();
        assert!(preview.dry_run);
        assert_eq!(preview.candidates.len(), 1);
        assert!(draft.exists());

        let result = cleanup_scratch_with_paths_at(&paths, &workspace_id, 1, false, now).unwrap();
        assert!(!result.dry_run);
        assert_eq!(result.removed.len(), 1);
        assert_eq!(result.removed_bytes, 5);
        assert!(!draft.exists());
    }

    #[test]
    fn keeps_recent_scratch_files_within_retention() {
        let (_temp, paths, workspace_id) = setup();
        let draft = paths
            .workspace_dir(&workspace_id)
            .unwrap()
            .join("scratch/recent.md");
        fs::write(&draft, "recent").unwrap();

        let result =
            cleanup_scratch_with_paths_at(&paths, &workspace_id, 1, false, Utc::now()).unwrap();
        assert!(result.candidates.is_empty());
        assert!(result.removed.is_empty());
        assert!(draft.exists());
    }

    #[test]
    fn context_files_stay_in_the_direct_context_area() {
        let (_temp, paths, workspace_id) = setup();

        for path in [
            "input/source.md",
            "scratch/draft.md",
            "context/nested/rules.md",
            "../outside.md",
        ] {
            assert!(
                save_context_file_with_paths(&paths, &workspace_id, path, "content", false)
                    .is_err(),
                "accepted unsafe context path {path:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_remove_symlinked_input_files() {
        use std::os::unix::fs::symlink;

        let (_temp, paths, workspace_id) = setup();
        let input = paths.workspace_dir(&workspace_id).unwrap().join("input");
        let target = input.join("target.txt");
        let link = input.join("link.txt");
        fs::write(&target, "target").unwrap();
        symlink(&target, &link).unwrap();

        assert!(remove_input_file_with_paths(&paths, &workspace_id, "input/link.txt").is_err());
        assert!(target.exists());
        assert!(link.exists());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_open_symlinked_files_outside_workspace() {
        use std::os::unix::fs::symlink;

        let (temp, paths, workspace_id) = setup();
        let workspace = paths.workspace_dir(&workspace_id).unwrap();
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, "outside").unwrap();
        symlink(&outside, workspace.join("input/outside.txt")).unwrap();

        assert!(existing_workspace_file(&paths, &workspace_id, "input/outside.txt").is_err());
    }
}
