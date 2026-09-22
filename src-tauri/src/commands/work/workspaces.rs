use super::ensure_work_enabled;
use crate::work::models::{
    WorkAccessRoot, WorkFileSummary, WorkProfile, WorkRulesInfo, WorkScratchCleanupResult,
    WorkWorkspace,
};
use crate::work::tasks::TaskManager;
use crate::work::{files, paths::WorkPaths, profile, workspace};

#[tauri::command]
pub fn work_get_profile() -> Result<WorkProfile, String> {
    profile::get_profile()
}

#[tauri::command]
pub fn work_get_rules_info() -> Result<WorkRulesInfo, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    let file_path = paths.work_profile_dir().join("AGENTS.md");
    let exists = file_path.exists();
    let content = if exists {
        std::fs::read_to_string(&file_path).map_err(|e| e.to_string())?
    } else {
        String::new()
    };
    Ok(WorkRulesInfo {
        path: file_path.to_string_lossy().to_string(),
        content,
        exists,
    })
}

#[tauri::command]
pub fn work_get_context_plan(
    run_id: String,
) -> Result<Option<crate::work::context::WorkContextPlan>, String> {
    ensure_work_enabled()?;
    crate::work::context::load(&run_id)
}

#[tauri::command]
pub fn work_save_rules(content: String) -> Result<(), String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    paths.ensure_layout()?;
    let file_path = paths.work_profile_dir().join("AGENTS.md");
    std::fs::write(&file_path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn work_create_workspace(name: String) -> Result<WorkWorkspace, String> {
    ensure_work_enabled()?;
    workspace::manager().create(&name)
}

#[tauri::command]
pub fn work_create_workspace_from_folder(
    folder_path: String,
    name: Option<String>,
) -> Result<WorkWorkspace, String> {
    ensure_work_enabled()?;
    workspace::manager().create_from_folder(&folder_path, name.as_deref())
}

#[tauri::command]
pub fn work_relink_workspace_folder(
    id: String,
    folder_path: String,
) -> Result<WorkWorkspace, String> {
    ensure_work_enabled()?;
    workspace::manager().relink_folder(&id, &folder_path)
}

#[tauri::command]
pub fn work_list_workspaces() -> Result<Vec<WorkWorkspace>, String> {
    ensure_work_enabled()?;
    workspace::manager().list()
}

#[tauri::command]
pub fn work_list_archived_workspaces() -> Result<Vec<WorkWorkspace>, String> {
    ensure_work_enabled()?;
    workspace::manager().list_archived()
}

#[tauri::command]
pub fn work_get_workspace(id: String) -> Result<WorkWorkspace, String> {
    ensure_work_enabled()?;
    workspace::manager().get(&id)
}

#[tauri::command]
pub fn work_rename_workspace(id: String, name: String) -> Result<WorkWorkspace, String> {
    ensure_work_enabled()?;
    workspace::manager().rename(&id, &name)
}

#[tauri::command]
pub fn work_archive_workspace(id: String) -> Result<WorkWorkspace, String> {
    ensure_work_enabled()?;
    workspace::manager().archive(&id)
}

#[tauri::command]
pub fn work_restore_workspace(id: String) -> Result<WorkWorkspace, String> {
    ensure_work_enabled()?;
    workspace::manager().restore(&id)
}

/// Permanently delete a Workspace: cascades to its tasks (and their runs), then removes the
/// workspace data directory. This is irreversible — the UI must confirm explicitly.
#[tauri::command]
pub fn work_delete_workspace(id: String) -> Result<(), String> {
    ensure_work_enabled()?;
    let task_manager = TaskManager::new(WorkPaths::app());
    for task in task_manager.list_tasks(Some(&id))? {
        task_manager.delete_task(&task.id)?;
    }
    workspace::manager().delete(&id)
}

#[tauri::command]
pub fn work_list_access_roots(workspace_id: String) -> Result<Vec<WorkAccessRoot>, String> {
    ensure_work_enabled()?;
    workspace::manager().list_access_roots(&workspace_id)
}

#[tauri::command]
pub fn work_add_access_root(
    workspace_id: String,
    path: String,
    writable: bool,
) -> Result<Vec<WorkAccessRoot>, String> {
    ensure_work_enabled()?;
    workspace::manager().add_access_root(&workspace_id, &path, writable)
}

#[tauri::command]
pub fn work_set_access_root_writable(
    workspace_id: String,
    path: String,
    writable: bool,
) -> Result<Vec<WorkAccessRoot>, String> {
    ensure_work_enabled()?;
    workspace::manager().set_access_root_writable(&workspace_id, &path, writable)
}

#[tauri::command]
pub fn work_remove_access_root(
    workspace_id: String,
    path: String,
) -> Result<Vec<WorkAccessRoot>, String> {
    ensure_work_enabled()?;
    workspace::manager().remove_access_root(&workspace_id, &path)
}

#[tauri::command]
pub fn work_list_files(
    workspace_id: String,
    area: Option<String>,
) -> Result<Vec<WorkFileSummary>, String> {
    ensure_work_enabled()?;
    files::list(&workspace_id, area.as_deref())
}

#[tauri::command]
pub fn work_import_file(
    workspace_id: String,
    source_path: String,
) -> Result<WorkFileSummary, String> {
    ensure_work_enabled()?;
    files::import(&workspace_id, &source_path)
}

#[tauri::command]
pub fn work_open_file(workspace_id: String, path: String) -> Result<(), String> {
    ensure_work_enabled()?;
    files::open(&workspace_id, &path)
}

#[tauri::command]
pub fn work_read_context_file(workspace_id: String, path: String) -> Result<String, String> {
    ensure_work_enabled()?;
    files::read_context_file(&workspace_id, &path)
}

#[tauri::command]
pub fn work_save_context_file(
    workspace_id: String,
    path: String,
    content: String,
    overwrite: bool,
) -> Result<WorkFileSummary, String> {
    ensure_work_enabled()?;
    files::save_context_file(&workspace_id, &path, &content, overwrite)
}

#[tauri::command]
pub fn work_remove_context_file(workspace_id: String, path: String) -> Result<(), String> {
    ensure_work_enabled()?;
    files::remove_context_file(&workspace_id, &path)
}

#[tauri::command]
pub fn work_remove_file(workspace_id: String, path: String) -> Result<(), String> {
    ensure_work_enabled()?;
    files::remove_input_file(&workspace_id, &path)
}

#[tauri::command]
pub fn work_cleanup_scratch(
    workspace_id: String,
    retention_days: u32,
    dry_run: bool,
) -> Result<WorkScratchCleanupResult, String> {
    ensure_work_enabled()?;
    files::cleanup_scratch(&workspace_id, retention_days, dry_run)
}

#[tauri::command]
pub fn work_open_workspace(id: String) -> Result<WorkWorkspace, String> {
    ensure_work_enabled()?;
    workspace::manager().open(&id)
}

#[tauri::command]
pub fn work_open_directory(workspace_id: String, path: String) -> Result<(), String> {
    ensure_work_enabled()?;
    files::open_directory(&workspace_id, &path)
}

#[tauri::command]
pub fn work_list_library_items(
    workspace_id: Option<String>,
    category: Option<crate::work::models::LibraryCategory>,
    query: Option<String>,
) -> Result<Vec<crate::work::models::LibraryItemSummary>, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let manager = crate::work::library::LibraryManager::new(paths);
    manager.list_items(workspace_id.as_deref(), category, query.as_deref())
}

#[tauri::command]
pub fn work_get_library_item(
    id: String,
    workspace_id: Option<String>,
) -> Result<crate::work::models::LibraryItem, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let manager = crate::work::library::LibraryManager::new(paths);
    manager.get_item_scoped(&id, workspace_id.as_deref())
}

#[tauri::command]
pub fn work_save_library_item(
    item: crate::work::models::LibraryItem,
) -> Result<crate::work::models::LibraryItem, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let manager = crate::work::library::LibraryManager::new(paths);
    manager.save_item(item)
}

#[tauri::command]
pub fn work_delete_library_item(id: String, workspace_id: Option<String>) -> Result<(), String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let manager = crate::work::library::LibraryManager::new(paths);
    manager.delete_item_scoped(&id, workspace_id.as_deref())
}

#[tauri::command]
pub fn work_create_library_item_from_artifact(
    workspace_id: String,
    artifact_id: String,
    title: String,
    category: crate::work::models::LibraryCategory,
) -> Result<crate::work::models::LibraryItem, String> {
    ensure_work_enabled()?;
    let paths = WorkPaths::app();
    let manager = crate::work::library::LibraryManager::new(paths);
    manager.create_from_artifact(&workspace_id, &artifact_id, &title, category)
}

#[tauri::command]
pub fn work_add_library_file(
    workspace_id: Option<String>,
    source_path: String,
    title: Option<String>,
    category: crate::work::models::LibraryCategory,
    collection: Option<String>,
    tags: Option<Vec<String>>,
) -> Result<crate::work::models::LibraryItem, String> {
    ensure_work_enabled()?;
    let manager = crate::work::library::LibraryManager::new(WorkPaths::app());
    manager.add_file(
        workspace_id.as_deref(),
        &source_path,
        title.as_deref(),
        category,
        collection.as_deref(),
        tags.unwrap_or_default(),
    )
}

#[tauri::command]
pub fn work_add_library_directory(
    workspace_id: Option<String>,
    source_path: String,
    category: crate::work::models::LibraryCategory,
    collection: Option<String>,
    tags: Option<Vec<String>>,
) -> Result<Vec<crate::work::models::LibraryItem>, String> {
    ensure_work_enabled()?;
    let manager = crate::work::library::LibraryManager::new(WorkPaths::app());
    manager.add_directory(
        workspace_id.as_deref(),
        &source_path,
        category,
        collection.as_deref(),
        tags.unwrap_or_default(),
    )
}

#[tauri::command]
pub fn work_rename_library_item(
    id: String,
    workspace_id: Option<String>,
    title: String,
) -> Result<crate::work::models::LibraryItem, String> {
    ensure_work_enabled()?;
    let manager = crate::work::library::LibraryManager::new(WorkPaths::app());
    manager.rename_item(&id, workspace_id.as_deref(), &title)
}
