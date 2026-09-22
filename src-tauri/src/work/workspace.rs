use std::fs;
use std::path::Path;

use chrono::Utc;
use uuid::Uuid;

use crate::work::models::{
    WorkAccessRoot, WorkArtifactStorageMode, WorkPolicy, WorkRootKind, WorkWorkspace,
};
use crate::work::paths::{validate_workspace_id, WorkPaths};

const WORKSPACE_AREAS: &[&str] = &["input", "scratch", "output", "context"];

#[derive(Debug, Clone)]
pub struct WorkspaceManager {
    paths: WorkPaths,
}

impl WorkspaceManager {
    pub fn new(paths: WorkPaths) -> Self {
        Self { paths }
    }

    pub fn create(&self, name: &str) -> Result<WorkWorkspace, String> {
        let name = normalize_name(name)?;
        self.paths.ensure_layout()?;
        let id = Uuid::new_v4().to_string();
        let root = self.paths.workspace_dir(&id)?;
        for area in WORKSPACE_AREAS {
            fs::create_dir_all(root.join(area)).map_err(|e| e.to_string())?;
        }

        let now = Utc::now().to_rfc3339();
        let workspace = WorkWorkspace {
            id,
            name,
            root: root.to_string_lossy().into_owned(),
            input_dir: root.join("input").to_string_lossy().into_owned(),
            scratch_dir: root.join("scratch").to_string_lossy().into_owned(),
            output_dir: root.join("output").to_string_lossy().into_owned(),
            context_dir: root.join("context").to_string_lossy().into_owned(),
            created_at: now.clone(),
            updated_at: now,
            artifact_count: 0,
            archived: false,
            access_roots: Vec::new(),
            default_policy: WorkPolicy::default_for_workspace(),
            default_model: None,
            default_effort: None,
            root_kind: WorkRootKind::Managed,
            primary_work_root: None,
            working_root_valid: Some(true),
            artifact_storage_mode: WorkArtifactStorageMode::Managed,
        };
        self.write_manifest(&workspace)?;
        Ok(workspace)
    }

    pub fn create_from_folder(
        &self,
        folder_path: &str,
        custom_name: Option<&str>,
    ) -> Result<WorkWorkspace, String> {
        let canonical_folder = canonical_directory(folder_path)?;
        let folder_str = canonical_folder.to_string_lossy().into_owned();

        // Security check: cannot be inside ~/.agentcabin
        let data_root = self.paths.data_root();
        let canonical_data_root =
            fs::canonicalize(data_root).unwrap_or_else(|_| data_root.to_path_buf());
        if canonical_folder == canonical_data_root
            || canonical_folder.starts_with(&canonical_data_root)
        {
            return Err("不能选择 AgentCabin 内部数据目录作为工作空间".into());
        }

        let name = match custom_name.map(str::trim).filter(|s| !s.is_empty()) {
            Some(custom) => normalize_name(custom)?,
            None => {
                let derived = canonical_folder
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("Workspace");
                normalize_name(derived)?
            }
        };

        self.paths.ensure_layout()?;
        let id = Uuid::new_v4().to_string();
        let root = self.paths.workspace_dir(&id)?;
        for area in WORKSPACE_AREAS {
            fs::create_dir_all(root.join(area)).map_err(|e| e.to_string())?;
        }

        let now = Utc::now().to_rfc3339();
        let workspace = WorkWorkspace {
            id,
            name,
            root: root.to_string_lossy().into_owned(),
            input_dir: root.join("input").to_string_lossy().into_owned(),
            scratch_dir: root.join("scratch").to_string_lossy().into_owned(),
            output_dir: root.join("output").to_string_lossy().into_owned(),
            context_dir: root.join("context").to_string_lossy().into_owned(),
            created_at: now.clone(),
            updated_at: now,
            artifact_count: 0,
            archived: false,
            access_roots: Vec::new(),
            default_policy: WorkPolicy::default_for_workspace(),
            default_model: None,
            default_effort: None,
            root_kind: WorkRootKind::LocalFolder,
            primary_work_root: Some(folder_str),
            working_root_valid: Some(true),
            artifact_storage_mode: WorkArtifactStorageMode::Managed,
        };
        self.write_manifest(&workspace)?;
        Ok(workspace)
    }

    pub fn relink_folder(&self, id: &str, folder_path: &str) -> Result<WorkWorkspace, String> {
        let mut workspace = self.get(id)?;
        if workspace.artifact_storage_mode == WorkArtifactStorageMode::PrimaryWorkRoot
            && workspace.artifact_count > 0
        {
            return Err(
                "直接保存模式下已有成果，不能更换关联目录；请先导出或清理成果后再更换目录".into(),
            );
        }
        let canonical_folder = canonical_directory(folder_path)?;
        let folder_str = canonical_folder.to_string_lossy().into_owned();

        let data_root = self.paths.data_root();
        let canonical_data_root =
            fs::canonicalize(data_root).unwrap_or_else(|_| data_root.to_path_buf());
        if canonical_folder == canonical_data_root
            || canonical_folder.starts_with(&canonical_data_root)
        {
            return Err("不能选择 AgentCabin 内部数据目录作为工作空间".into());
        }

        workspace.root_kind = WorkRootKind::LocalFolder;
        workspace.primary_work_root = Some(folder_str);
        workspace.updated_at = Utc::now().to_rfc3339();
        self.write_manifest(&workspace)?;
        Ok(workspace)
    }

    /// Change the physical location of future `output/` artifacts.
    ///
    /// We intentionally do not migrate existing registry entries here. An
    /// artifact stores a logical `output/...` path, so switching after the
    /// first artifact would make the same record point at an ambiguous
    /// physical file. Requiring an empty registry keeps the transition
    /// explicit and reversible.
    pub fn set_artifact_storage_mode(
        &self,
        id: &str,
        mode: WorkArtifactStorageMode,
    ) -> Result<WorkWorkspace, String> {
        let mut workspace = self.get(id)?;
        if workspace.artifact_storage_mode == mode {
            return Ok(workspace);
        }
        if mode == WorkArtifactStorageMode::PrimaryWorkRoot
            && workspace.root_kind != WorkRootKind::LocalFolder
        {
            return Err("只有关联本地文件夹的 Workspace 才能直接保存成果".into());
        }
        if workspace.artifact_count > 0 {
            return Err("已有成果，不能切换保存位置；请先导出或清理成果后再切换".into());
        }
        if mode == WorkArtifactStorageMode::PrimaryWorkRoot {
            let primary_root = self.resolve_primary_work_root(&workspace)?;
            fs::create_dir_all(primary_root.join("output"))
                .map_err(|error| format!("无法创建本地 output 目录: {error}"))?;
        }
        workspace.artifact_storage_mode = mode;
        workspace.updated_at = Utc::now().to_rfc3339();
        self.write_manifest(&workspace)?;
        self.get(id)
    }

    pub fn resolve_primary_work_root(
        &self,
        workspace: &WorkWorkspace,
    ) -> Result<std::path::PathBuf, String> {
        match workspace.root_kind {
            WorkRootKind::Managed => {
                let path = std::path::PathBuf::from(&workspace.root);
                if !path.exists() {
                    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
                }
                Ok(path)
            }
            WorkRootKind::LocalFolder => {
                let raw = workspace
                    .primary_work_root
                    .as_deref()
                    .ok_or_else(|| "本地工作目录未配置".to_string())?;
                canonical_directory(raw).map_err(|e| format!("工作目录不可用 ({}): {}", raw, e))
            }
        }
    }

    pub fn is_primary_work_root_available(&self, workspace: &WorkWorkspace) -> bool {
        match workspace.root_kind {
            WorkRootKind::Managed => true,
            WorkRootKind::LocalFolder => {
                if let Some(raw) = &workspace.primary_work_root {
                    let trimmed = raw.trim();
                    !trimmed.is_empty() && Path::new(trimmed).is_dir()
                } else {
                    false
                }
            }
        }
    }

    pub fn list(&self) -> Result<Vec<WorkWorkspace>, String> {
        self.list_filtered(false)
    }

    pub fn list_archived(&self) -> Result<Vec<WorkWorkspace>, String> {
        self.list_filtered(true)
    }

    fn list_filtered(&self, archived: bool) -> Result<Vec<WorkWorkspace>, String> {
        self.paths.ensure_layout()?;
        let mut workspaces = Vec::new();
        let entries = fs::read_dir(self.paths.workspaces_dir()).map_err(|e| e.to_string())?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            let Ok(workspace) = self.get(&id) else {
                continue;
            };
            if workspace.archived == archived {
                workspaces.push(workspace);
            }
        }
        workspaces.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(workspaces)
    }

    pub fn get(&self, id: &str) -> Result<WorkWorkspace, String> {
        validate_workspace_id(id)?;
        let path = self.paths.manifest_path(id)?;
        let content = fs::read_to_string(&path).map_err(|e| format!("Workspace not found: {e}"))?;
        let mut workspace: WorkWorkspace = serde_json::from_str(&content)
            .map_err(|e| format!("Invalid Workspace manifest: {e}"))?;
        if workspace.id != id {
            return Err("Workspace manifest id does not match its directory".into());
        }
        workspace.artifact_count = self.count_artifacts(id) as u32;
        workspace.working_root_valid = Some(self.is_primary_work_root_available(&workspace));
        Ok(workspace)
    }

    pub fn touch(&self, id: &str) -> Result<(), String> {
        let mut workspace = self.get(id)?;
        workspace.updated_at = Utc::now().to_rfc3339();
        self.write_manifest(&workspace)
    }

    fn count_artifacts(&self, id: &str) -> usize {
        let Ok(workspace_dir) = self.paths.workspace_dir(id) else {
            return 0;
        };
        let registry_path = workspace_dir.join("context").join("artifacts.json");
        let Ok(content) = fs::read_to_string(registry_path) else {
            return 0;
        };
        let Ok(parsed): Result<serde_json::Value, _> = serde_json::from_str(&content) else {
            return 0;
        };
        parsed
            .get("artifacts")
            .and_then(|v| v.as_array())
            .map(|arr| arr.len())
            .unwrap_or(0)
    }

    pub fn open(&self, id: &str) -> Result<WorkWorkspace, String> {
        let workspace = self.get(id)?;
        if workspace.archived {
            return Err("Archived Workspace cannot start a session".into());
        }
        self.resolve_primary_work_root(&workspace)?;
        Ok(workspace)
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<WorkWorkspace, String> {
        let mut workspace = self.get(id)?;
        workspace.name = normalize_name(name)?;
        workspace.updated_at = Utc::now().to_rfc3339();
        self.write_manifest(&workspace)?;
        Ok(workspace)
    }

    pub fn archive(&self, id: &str) -> Result<WorkWorkspace, String> {
        let mut workspace = self.get(id)?;
        workspace.archived = true;
        workspace.updated_at = Utc::now().to_rfc3339();
        self.write_manifest(&workspace)?;
        Ok(workspace)
    }

    pub fn restore(&self, id: &str) -> Result<WorkWorkspace, String> {
        let mut workspace = self.get(id)?;
        workspace.archived = false;
        workspace.updated_at = Utc::now().to_rfc3339();
        self.write_manifest(&workspace)?;
        Ok(workspace)
    }

    /// Permanently delete the Workspace directory (manifest + input/scratch/output/context).
    ///
    /// The id is validated by `get()`/`workspace_dir()` (alphanumeric, `-`, `_` only), so the
    /// removed path is always `{data_root}/workspaces/{id}` and cannot escape the data root.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let workspace = self.get(id)?;
        let root = self.paths.workspace_dir(&workspace.id)?;
        fs::remove_dir_all(&root).map_err(|e| format!("Failed to delete Workspace {id}: {e}"))
    }

    /// Persist an already-mutated workspace (updates timestamp and writes manifest).
    pub fn update(&self, workspace: &mut WorkWorkspace) -> Result<(), String> {
        workspace.updated_at = Utc::now().to_rfc3339();
        self.write_manifest(workspace)
    }

    pub fn list_access_roots(&self, id: &str) -> Result<Vec<WorkAccessRoot>, String> {
        Ok(self.get(id)?.access_roots)
    }

    pub fn add_access_root(
        &self,
        id: &str,
        path: &str,
        writable: bool,
    ) -> Result<Vec<WorkAccessRoot>, String> {
        let mut workspace = self.get(id)?;
        let path = canonical_directory(path)?;
        if let Ok(primary_root) = self.resolve_primary_work_root(&workspace) {
            if path == primary_root || path.starts_with(&primary_root) {
                return Err("工作目录及其子目录默认已具备访问权限，无需重复添加".into());
            }
        }
        if let Ok(workspace_state_root) = canonical_directory(&workspace.root) {
            if path == workspace_state_root || path.starts_with(&workspace_state_root) {
                return Err("工作空间管理目录无需作为外部目录添加".into());
            }
        }

        let path = path.to_string_lossy().into_owned();
        if workspace.access_roots.iter().any(|root| root.path == path) {
            return Err("这个目录已经添加到当前 Workspace".into());
        }
        if workspace.access_roots.len() >= 64 {
            return Err("单个 Workspace 最多添加 64 个外部目录".into());
        }

        workspace
            .access_roots
            .push(WorkAccessRoot { path, writable });
        workspace.updated_at = Utc::now().to_rfc3339();
        self.write_manifest(&workspace)?;
        Ok(workspace.access_roots)
    }

    pub fn set_access_root_writable(
        &self,
        id: &str,
        path: &str,
        writable: bool,
    ) -> Result<Vec<WorkAccessRoot>, String> {
        let mut workspace = self.get(id)?;
        let path = access_root_path(path)?;
        let root = workspace
            .access_roots
            .iter_mut()
            .find(|root| root.path == path)
            .ok_or_else(|| "找不到这个外部目录".to_string())?;
        root.writable = writable;
        workspace.updated_at = Utc::now().to_rfc3339();
        self.write_manifest(&workspace)?;
        Ok(workspace.access_roots)
    }

    pub fn remove_access_root(&self, id: &str, path: &str) -> Result<Vec<WorkAccessRoot>, String> {
        let mut workspace = self.get(id)?;
        let path = access_root_path(path)?;
        let previous_len = workspace.access_roots.len();
        workspace.access_roots.retain(|root| root.path != path);
        if workspace.access_roots.len() == previous_len {
            return Err("找不到这个外部目录".into());
        }
        workspace.updated_at = Utc::now().to_rfc3339();
        self.write_manifest(&workspace)?;
        Ok(workspace.access_roots)
    }

    fn write_manifest(&self, workspace: &WorkWorkspace) -> Result<(), String> {
        let path = self.paths.manifest_path(&workspace.id)?;
        let content = serde_json::to_string_pretty(workspace).map_err(|e| e.to_string())?;
        fs::write(path, format!("{content}\n")).map_err(|e| e.to_string())
    }
}

fn canonical_directory(path: &str) -> Result<std::path::PathBuf, String> {
    let path = normalize_absolute_path(path)?;
    let path = fs::canonicalize(path).map_err(|error| format!("无法访问目录: {error}"))?;
    if !path.is_dir() {
        return Err("选择的路径不是目录".into());
    }
    Ok(path)
}

fn normalize_absolute_path(path: &str) -> Result<std::path::PathBuf, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("目录路径不能为空".into());
    }
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err("目录路径必须是绝对路径".into());
    }
    Ok(path.to_path_buf())
}

fn access_root_path(path: &str) -> Result<String, String> {
    let normalized = normalize_absolute_path(path)?;
    let path = fs::canonicalize(&normalized).unwrap_or(normalized);
    Ok(path.to_string_lossy().into_owned())
}

fn normalize_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Workspace name cannot be empty".into());
    }
    if name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err("Workspace name is too long or contains control characters".into());
    }
    Ok(name.to_string())
}

pub fn manager() -> WorkspaceManager {
    WorkspaceManager::new(WorkPaths::app())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_manager(temp: &TempDir) -> WorkspaceManager {
        WorkspaceManager::new(WorkPaths::new(temp.path().join("data")))
    }

    #[test]
    fn creates_workspace_manifest_and_areas() {
        let temp = TempDir::new().unwrap();
        let manager = test_manager(&temp);
        let workspace = manager.create("Sales analysis").unwrap();
        let root = std::path::Path::new(&workspace.root);

        assert!(root.join("manifest.json").is_file());
        assert!(root.join("input").is_dir());
        assert!(root.join("scratch").is_dir());
        assert!(root.join("output").is_dir());
        assert!(root.join("context").is_dir());
        assert_eq!(manager.list().unwrap().len(), 1);
    }

    #[test]
    fn rename_and_archive_are_non_destructive() {
        let temp = TempDir::new().unwrap();
        let manager = test_manager(&temp);
        let workspace = manager.create("Draft").unwrap();
        let renamed = manager.rename(&workspace.id, "Final").unwrap();
        assert_eq!(renamed.name, "Final");

        let archived = manager.archive(&workspace.id).unwrap();
        assert!(archived.archived);
        assert!(manager.list().unwrap().is_empty());
        assert_eq!(manager.get(&workspace.id).unwrap().name, "Final");
        let restored = manager.restore(&workspace.id).unwrap();
        assert!(!restored.archived);
        assert_eq!(manager.list().unwrap().len(), 1);
        assert!(manager.list_archived().unwrap().is_empty());
    }

    #[test]
    fn rejects_invalid_names() {
        let temp = TempDir::new().unwrap();
        let manager = test_manager(&temp);
        assert!(manager.create(" ").is_err());
        assert!(manager.rename("missing", "Name").is_err());
    }

    #[test]
    fn delete_removes_workspace_directory() {
        let temp = TempDir::new().unwrap();
        let manager = test_manager(&temp);
        let workspace = manager.create("Disposable").unwrap();
        let root = std::path::PathBuf::from(&workspace.root);
        assert!(root.join("manifest.json").is_file());

        manager.delete(&workspace.id).unwrap();
        assert!(!root.exists());
        assert!(manager.list().unwrap().is_empty());
        assert!(manager.get(&workspace.id).is_err());
        assert!(manager.delete(&workspace.id).is_err());
    }

    #[test]
    fn persists_external_access_roots_and_permissions() {
        let temp = TempDir::new().unwrap();
        let manager = test_manager(&temp);
        let workspace = manager.create("Access").unwrap();
        let external = temp.path().join("external");
        fs::create_dir_all(&external).unwrap();

        let roots = manager
            .add_access_root(&workspace.id, external.to_str().unwrap(), false)
            .unwrap();
        assert_eq!(roots.len(), 1);
        assert!(!roots[0].writable);

        let roots = manager
            .set_access_root_writable(&workspace.id, external.to_str().unwrap(), true)
            .unwrap();
        assert!(roots[0].writable);

        let stored = manager.get(&workspace.id).unwrap();
        assert_eq!(stored.access_roots, roots);

        let roots = manager
            .remove_access_root(&workspace.id, external.to_str().unwrap())
            .unwrap();
        assert!(roots.is_empty());
    }

    #[test]
    fn creates_local_folder_workspace_without_polluting_target() {
        let temp = TempDir::new().unwrap();
        let manager = test_manager(&temp);

        let user_folder = temp.path().join("UserProject");
        fs::create_dir_all(&user_folder).unwrap();
        fs::write(user_folder.join("index.ts"), "console.log('hello');").unwrap();

        let ws = manager
            .create_from_folder(user_folder.to_str().unwrap(), None)
            .unwrap();

        assert_eq!(ws.name, "UserProject");
        assert_eq!(ws.root_kind, WorkRootKind::LocalFolder);
        assert_eq!(
            ws.primary_work_root.as_deref(),
            Some(user_folder.canonicalize().unwrap().to_str().unwrap())
        );

        // Verify managed harness state is in ~/.agentcabin/workspaces/<id>
        let harness_root = Path::new(&ws.root);
        assert!(harness_root.join("manifest.json").is_file());
        assert!(harness_root.join("input").is_dir());
        assert!(harness_root.join("scratch").is_dir());
        assert!(harness_root.join("output").is_dir());
        assert!(harness_root.join("context").is_dir());

        // Verify user directory has NO .agentcabin or internal state written to it
        assert!(!user_folder.join(".agentcabin").exists());
        assert!(!user_folder.join("manifest.json").exists());
        assert!(user_folder.join("index.ts").exists());

        // Resolve primary work root
        let resolved = manager.resolve_primary_work_root(&ws).unwrap();
        assert_eq!(resolved, user_folder.canonicalize().unwrap());
        assert!(manager.is_primary_work_root_available(&ws));
    }

    #[test]
    fn legacy_workspace_deserializes_and_functions_as_managed() {
        let temp = TempDir::new().unwrap();
        let manager = test_manager(&temp);

        // Simulate a legacy manifest without rootKind and primaryWorkRoot fields
        let id = "legacy-test-uuid";
        let ws_dir = manager.paths.workspace_dir(id).unwrap();
        fs::create_dir_all(&ws_dir).unwrap();
        for area in WORKSPACE_AREAS {
            fs::create_dir_all(ws_dir.join(area)).unwrap();
        }

        let legacy_json = serde_json::json!({
            "id": id,
            "name": "Legacy Project",
            "root": ws_dir.to_str().unwrap(),
            "inputDir": ws_dir.join("input").to_str().unwrap(),
            "scratchDir": ws_dir.join("scratch").to_str().unwrap(),
            "outputDir": ws_dir.join("output").to_str().unwrap(),
            "contextDir": ws_dir.join("context").to_str().unwrap(),
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z",
            "artifactCount": 0,
            "archived": false,
            "accessRoots": [],
            "defaultPolicy": {
                "executionMode": "direct",
                "maxAutomatedSteps": 50,
                "allowExternalConnectors": false,
                "standingRules": []
            }
        });
        fs::write(
            manager.paths.manifest_path(id).unwrap(),
            legacy_json.to_string(),
        )
        .unwrap();

        let ws = manager.get(id).unwrap();
        assert_eq!(ws.root_kind, WorkRootKind::Managed);
        assert!(ws.primary_work_root.is_none());
        assert_eq!(ws.configured_primary_work_root(), ws_dir.to_str().unwrap());

        let resolved = manager.resolve_primary_work_root(&ws).unwrap();
        assert_eq!(resolved, ws_dir);
        assert!(manager.open(id).is_ok());
    }

    #[test]
    fn workspace_model_preferences_persist_in_manifest() {
        let temp = TempDir::new().unwrap();
        let manager = test_manager(&temp);
        let mut workspace = manager.create("Model Settings").unwrap();
        workspace.default_model = Some("provider/model".to_string());
        workspace.default_effort = Some("high".to_string());
        manager.update(&mut workspace).unwrap();

        let reloaded = manager.get(&workspace.id).unwrap();
        assert_eq!(reloaded.default_model.as_deref(), Some("provider/model"));
        assert_eq!(reloaded.default_effort.as_deref(), Some("high"));
    }

    #[test]
    fn missing_local_folder_fails_closed_and_allows_relink() {
        let temp = TempDir::new().unwrap();
        let manager = test_manager(&temp);

        let user_folder = temp.path().join("TemporaryProject");
        fs::create_dir_all(&user_folder).unwrap();

        let ws = manager
            .create_from_folder(user_folder.to_str().unwrap(), Some("My Temp Work"))
            .unwrap();
        assert_eq!(ws.name, "My Temp Work");

        // Delete the user folder
        fs::remove_dir_all(&user_folder).unwrap();

        // get() still succeeds so UI can display the workspace
        let retrieved = manager.get(&ws.id).unwrap();
        assert!(!manager.is_primary_work_root_available(&retrieved));
        assert!(manager.resolve_primary_work_root(&retrieved).is_err());

        // open() must FAIL CLOSED
        assert!(manager.open(&ws.id).is_err());

        // Relink to a new valid folder
        let new_folder = temp.path().join("RestoredProject");
        fs::create_dir_all(&new_folder).unwrap();

        let relinked = manager
            .relink_folder(&ws.id, new_folder.to_str().unwrap())
            .unwrap();
        assert_eq!(
            relinked.primary_work_root.as_deref(),
            Some(new_folder.canonicalize().unwrap().to_str().unwrap())
        );

        // Now open succeeds again
        assert!(manager.open(&ws.id).is_ok());
        assert_eq!(
            manager.resolve_primary_work_root(&relinked).unwrap(),
            new_folder.canonicalize().unwrap()
        );
    }

    #[test]
    fn rejects_agentcabin_internal_data_dir_as_local_folder() {
        let temp = TempDir::new().unwrap();
        let manager = test_manager(&temp);

        let internal_dir = temp.path().join("data");
        fs::create_dir_all(&internal_dir).unwrap();

        assert!(manager
            .create_from_folder(internal_dir.to_str().unwrap(), None)
            .is_err());
    }

    #[test]
    fn rejects_primary_work_root_and_subdirectories_in_access_roots() {
        let temp = TempDir::new().unwrap();
        let manager = test_manager(&temp);

        let project_dir = temp.path().join("MainProject");
        let sub_dir = project_dir.join("src").join("nested");
        fs::create_dir_all(&sub_dir).unwrap();

        let ws = manager
            .create_from_folder(project_dir.to_str().unwrap(), Some("MainProject"))
            .unwrap();

        // Adding exact primary work root is rejected
        let err1 = manager.add_access_root(&ws.id, project_dir.to_str().unwrap(), false);
        assert!(err1.is_err());
        assert!(err1.unwrap_err().contains("无需重复添加"));

        // Adding subdirectory of primary work root is rejected
        let err2 = manager.add_access_root(&ws.id, sub_dir.to_str().unwrap(), false);
        assert!(err2.is_err());
        assert!(err2.unwrap_err().contains("无需重复添加"));

        // Adding an external unrelated directory is allowed
        let external_dir = temp.path().join("ExternalDocs");
        fs::create_dir_all(&external_dir).unwrap();
        let ok = manager.add_access_root(&ws.id, external_dir.to_str().unwrap(), true);
        assert!(ok.is_ok());
        assert_eq!(ok.unwrap().len(), 1);
    }

    #[test]
    fn working_root_valid_tracks_availability_and_list_access_roots_is_safe_when_missing() {
        let temp = TempDir::new().unwrap();
        let manager = test_manager(&temp);

        let user_folder = temp.path().join("VolatileProject");
        fs::create_dir_all(&user_folder).unwrap();

        let external_dir = temp.path().join("SharedData");
        fs::create_dir_all(&external_dir).unwrap();

        let ws = manager
            .create_from_folder(user_folder.to_str().unwrap(), Some("VolatileProject"))
            .unwrap();
        manager
            .add_access_root(&ws.id, external_dir.to_str().unwrap(), false)
            .unwrap();

        let ws_get = manager.get(&ws.id).unwrap();
        assert_eq!(ws_get.working_root_valid, Some(true));

        // Delete user folder
        fs::remove_dir_all(&user_folder).unwrap();

        // get() and list() now reflect working_root_valid: Some(false)
        let ws_missing = manager.get(&ws.id).unwrap();
        assert_eq!(ws_missing.working_root_valid, Some(false));

        let list = manager.list().unwrap();
        let found = list.iter().find(|w| w.id == ws.id).unwrap();
        assert_eq!(found.working_root_valid, Some(false));

        // list_access_roots still succeeds without throwing errors
        let roots = manager.list_access_roots(&ws.id).unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(
            roots[0].path,
            external_dir.canonicalize().unwrap().to_str().unwrap()
        );
    }
}
