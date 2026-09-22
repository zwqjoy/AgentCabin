use super::ensure_work_enabled;
use crate::work::models::{WorkArtifactStorageMode, WorkArtifactSummary, WorkWorkspace};
use crate::work::{artifacts, workspace};

/// Artifacts are attributed to a WorkRun, but the conversation surface only
/// knows the session run id from the URL. Resolve a session run id to the
/// WorkRun that owns it; unknown ids pass through unchanged so callers may
/// still address a WorkRun directly.
fn resolve_artifact_run_scope(run_id: Option<String>) -> Option<String> {
    let run_id = run_id?;
    match crate::storage::runs::get_run(&run_id) {
        Some(meta) => meta.work_run_id.or(Some(run_id)),
        None => Some(run_id),
    }
}

#[tauri::command]
pub fn work_list_artifacts(
    workspace_id: String,
    run_id: Option<String>,
) -> Result<Vec<WorkArtifactSummary>, String> {
    ensure_work_enabled()?;
    let run_id = resolve_artifact_run_scope(run_id);
    artifacts::list(&workspace_id, run_id.as_deref())
}

#[tauri::command]
pub fn work_register_artifact(
    workspace_id: String,
    path: String,
    title: String,
    artifact_type: Option<String>,
    run_id: Option<String>,
) -> Result<WorkArtifactSummary, String> {
    ensure_work_enabled()?;
    artifacts::register(
        &workspace_id,
        &path,
        &title,
        artifact_type.as_deref(),
        run_id.as_deref(),
    )
}

#[tauri::command]
pub fn work_update_office_artifact(
    workspace_id: String,
    artifact_id: String,
    content_base64: String,
    run_id: Option<String>,
) -> Result<WorkArtifactSummary, String> {
    ensure_work_enabled()?;
    let run_scope = resolve_artifact_run_scope(run_id);
    artifacts::update_office_base64(
        &workspace_id,
        &artifact_id,
        &content_base64,
        run_scope.as_deref(),
    )
}

#[tauri::command]
pub fn work_create_office_artifact(
    workspace_id: String,
    path: String,
    title: String,
    artifact_type: String,
    content_base64: String,
    run_id: Option<String>,
) -> Result<WorkArtifactSummary, String> {
    ensure_work_enabled()?;
    let run_scope = resolve_artifact_run_scope(run_id);
    artifacts::create_office_base64(
        &workspace_id,
        &path,
        &title,
        &artifact_type,
        &content_base64,
        run_scope.as_deref(),
    )
}

#[tauri::command]
pub fn work_validate_artifact(
    workspace_id: String,
    artifact_id: String,
    run_id: Option<String>,
) -> Result<WorkArtifactSummary, String> {
    ensure_work_enabled()?;
    let run_id = resolve_artifact_run_scope(run_id);
    artifacts::validate(&workspace_id, &artifact_id, run_id.as_deref())
}

#[tauri::command]
pub fn work_deliver(
    workspace_id: String,
    artifact_id: String,
    run_id: Option<String>,
) -> Result<WorkArtifactSummary, String> {
    ensure_work_enabled()?;
    let run_id = resolve_artifact_run_scope(run_id);
    artifacts::deliver(&workspace_id, &artifact_id, run_id.as_deref())
}

#[tauri::command]
pub fn work_delete_artifact(workspace_id: String, artifact_id: String) -> Result<(), String> {
    ensure_work_enabled()?;
    artifacts::delete(&workspace_id, &artifact_id)
}

#[tauri::command]
pub fn work_export_artifact(
    workspace_id: String,
    artifact_id: String,
    destination_path: String,
    run_id: Option<String>,
) -> Result<String, String> {
    ensure_work_enabled()?;
    let run_scope = resolve_artifact_run_scope(run_id);
    let result = artifacts::export(
        &workspace_id,
        &artifact_id,
        &destination_path,
        run_scope.as_deref(),
    );
    if result.is_err() {
        let standalone_run_id = run_scope.as_deref().unwrap_or(&workspace_id);
        if let Ok(exported) =
            artifacts::export_standalone(standalone_run_id, &artifact_id, &destination_path)
        {
            return Ok(exported);
        }
    }
    result
}

#[tauri::command]
pub fn work_copy_artifact_to_primary(
    workspace_id: String,
    artifact_id: String,
    run_id: Option<String>,
) -> Result<String, String> {
    ensure_work_enabled()?;
    let run_scope = resolve_artifact_run_scope(run_id);
    artifacts::copy_to_primary_work_root(&workspace_id, &artifact_id, run_scope.as_deref())
}

#[tauri::command]
pub fn work_set_artifact_storage_mode(
    workspace_id: String,
    mode: WorkArtifactStorageMode,
) -> Result<WorkWorkspace, String> {
    ensure_work_enabled()?;
    workspace::manager().set_artifact_storage_mode(&workspace_id, mode)
}

/// List artifacts for a standalone (workspace-less) Work task.
#[tauri::command]
pub fn work_list_standalone_artifacts(run_id: String) -> Result<Vec<WorkArtifactSummary>, String> {
    ensure_work_enabled()?;
    artifacts::list_standalone(&run_id)
}

/// Register a deliverable as an artifact for a standalone Work task.
#[tauri::command]
pub fn work_register_standalone_artifact(
    run_id: String,
    path: String,
    title: String,
    artifact_type: Option<String>,
) -> Result<WorkArtifactSummary, String> {
    ensure_work_enabled()?;
    artifacts::register_standalone(&run_id, &path, &title, artifact_type.as_deref())
}

#[tauri::command]
pub fn work_update_standalone_office_artifact(
    run_id: String,
    artifact_id: String,
    content_base64: String,
) -> Result<WorkArtifactSummary, String> {
    ensure_work_enabled()?;
    artifacts::update_standalone_office_base64(&run_id, &artifact_id, &content_base64)
}

#[tauri::command]
pub fn work_create_standalone_office_artifact(
    run_id: String,
    path: String,
    title: String,
    artifact_type: String,
    content_base64: String,
) -> Result<WorkArtifactSummary, String> {
    ensure_work_enabled()?;
    artifacts::create_standalone_office_base64(
        &run_id,
        &path,
        &title,
        &artifact_type,
        &content_base64,
    )
}

/// Validate a standalone artifact file.
#[tauri::command]
pub fn work_validate_standalone_artifact(
    run_id: String,
    artifact_id: String,
) -> Result<WorkArtifactSummary, String> {
    ensure_work_enabled()?;
    artifacts::validate_standalone(&run_id, &artifact_id)
}

/// Deliver a standalone artifact.
#[tauri::command]
pub fn work_deliver_standalone_artifact(
    run_id: String,
    artifact_id: String,
) -> Result<WorkArtifactSummary, String> {
    ensure_work_enabled()?;
    artifacts::deliver_standalone(&run_id, &artifact_id)
}

/// Delete a standalone artifact.
#[tauri::command]
pub fn work_delete_standalone_artifact(run_id: String, artifact_id: String) -> Result<(), String> {
    ensure_work_enabled()?;
    artifacts::delete_standalone(&run_id, &artifact_id)
}

/// Export a standalone artifact to an absolute destination path.
#[tauri::command]
pub fn work_export_standalone_artifact(
    run_id: String,
    artifact_id: String,
    destination_path: String,
) -> Result<String, String> {
    ensure_work_enabled()?;
    artifacts::export_standalone(&run_id, &artifact_id, &destination_path)
}

/// Retrieve authoritative artifact acceptance checks for a run.
#[tauri::command]
pub fn work_get_artifact_acceptance(
    run_id: String,
) -> Result<crate::work::models::WorkArtifactAcceptance, String> {
    ensure_work_enabled()?;
    let paths = crate::work::paths::WorkPaths::app();
    let task_manager = crate::work::tasks::TaskManager::new(paths.clone());
    let meta = crate::storage::runs::get_run(&run_id);

    let (workspace_id, task_opt) = if let Some(ref m) = meta {
        let task = m
            .work_task_id
            .as_ref()
            .and_then(|tid| task_manager.get_task(tid).ok());
        (m.workspace_id.clone().unwrap_or_default(), task)
    } else {
        // Fallback: search tasks for this run_id
        let tasks = task_manager.list_tasks(None).unwrap_or_default();
        let mut found = None;
        for t in tasks {
            if task_manager.get_run(&t.id, &run_id).is_ok() {
                found = Some((t.workspace_id.clone(), Some(t)));
                break;
            }
        }
        found.unwrap_or((String::new(), None))
    };

    if let Some(task) = task_opt {
        artifacts::check_required_artifacts_with_paths(&paths, &workspace_id, &run_id, &task)
    } else {
        Ok(crate::work::models::WorkArtifactAcceptance {
            run_id,
            checks: Vec::new(),
            required_count: 0,
            satisfied_count: 0,
            missing_count: 0,
            invalid_count: 0,
            satisfied: true,
        })
    }
}
