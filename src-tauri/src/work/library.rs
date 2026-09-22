use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use uuid::Uuid;

use crate::work::models::{
    atomic_write_json, LibraryCategory, LibraryCitation, LibraryItem, LibraryItemSummary,
};
use crate::work::paths::{validate_workspace_id, WorkPaths};

/// Library item ids are used verbatim as file names; keep them in the same
/// safe character space the rest of the Work storage layer enforces.
fn validate_item_id(id: &str) -> Result<(), String> {
    validate_workspace_id(id).map_err(|_| format!("Invalid library item id: '{id}'"))
}

#[derive(Debug, Clone)]
pub struct LibraryManager {
    paths: WorkPaths,
}

impl LibraryManager {
    pub fn new(paths: WorkPaths) -> Self {
        Self { paths }
    }

    pub fn list_items(
        &self,
        workspace_id: Option<&str>,
        category: Option<LibraryCategory>,
        query: Option<&str>,
    ) -> Result<Vec<LibraryItemSummary>, String> {
        self.list_items_filtered(workspace_id, category, query, None)
    }

    pub fn list_items_filtered(
        &self,
        workspace_id: Option<&str>,
        category: Option<LibraryCategory>,
        query: Option<&str>,
        collection: Option<&str>,
    ) -> Result<Vec<LibraryItemSummary>, String> {
        if let Some(workspace_id) = workspace_id.filter(|id| !id.trim().is_empty()) {
            validate_workspace_id(workspace_id)?;
        }
        let mut items = Vec::new();

        // 1. Read global items
        let global_dir = self.paths.global_library_dir();
        if global_dir.exists() {
            self.collect_items_from_dir(&global_dir, &mut items)?;
        }

        // 2. Read workspace-specific items if workspace_id provided
        if let Some(ws_id) = workspace_id {
            if !ws_id.is_empty() {
                let ws_lib_dir = self.paths.workspace_library_dir(ws_id)?;
                if ws_lib_dir.exists() {
                    self.collect_items_from_dir(&ws_lib_dir, &mut items)?;
                }
            }
        }

        // 3. Filter by category
        if let Some(cat) = category {
            items.retain(|item| item.category == cat);
        }

        // 4. Filter by query
        if let Some(q) = query {
            let q_lower = q.trim().to_lowercase();
            if !q_lower.is_empty() {
                items.retain(|item| {
                    item.title.to_lowercase().contains(&q_lower)
                        || item.description.to_lowercase().contains(&q_lower)
                        || item.content_preview.to_lowercase().contains(&q_lower)
                        || item
                            .tags
                            .iter()
                            .any(|t| t.to_lowercase().contains(&q_lower))
                        || item
                            .source_path
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&q_lower)
                });
            }
        }

        if let Some(collection) = collection {
            let collection = collection.trim().to_lowercase();
            if !collection.is_empty() {
                items.retain(|item| {
                    item.collection.as_deref().unwrap_or("").to_lowercase() == collection
                });
            }
        }

        // Sort by updated_at descending
        items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(items)
    }

    pub fn get_item(&self, id: &str) -> Result<LibraryItem, String> {
        validate_item_id(id)?;
        if let Some(path) = self.find_item_file(id)? {
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Failed reading library item {id}: {e}"))?;
            serde_json::from_str(&content)
                .map_err(|e| format!("Failed parsing library item {id}: {e}"))
        } else {
            Err(format!("Library item '{id}' not found"))
        }
    }

    /// Read an item in the global scope or the requested Workspace scope.
    /// Workspace callers may see global items plus their own workspace items,
    /// but can never address another Workspace's private library entry.
    pub fn get_item_scoped(
        &self,
        id: &str,
        workspace_id: Option<&str>,
    ) -> Result<LibraryItem, String> {
        validate_item_id(id)?;
        let workspace_id = workspace_id.filter(|value| !value.trim().is_empty());
        if let Some(workspace_id) = workspace_id {
            validate_workspace_id(workspace_id)?;
        }
        let path = self
            .find_item_file_scoped(id, workspace_id)?
            .ok_or_else(|| format!("Library item '{id}' not found in the requested scope"))?;
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed reading library item {id}: {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed parsing library item {id}: {e}"))
    }

    pub fn save_item(&self, mut item: LibraryItem) -> Result<LibraryItem, String> {
        let now = Utc::now().to_rfc3339();
        if let Some(workspace_id) = item.workspace_id.as_deref() {
            if !workspace_id.trim().is_empty() {
                validate_workspace_id(workspace_id)?;
            }
        }
        if item.id.trim().is_empty() {
            item.id = format!("lib-{}", Uuid::new_v4());
            item.created_at = now.clone();
        } else {
            validate_item_id(&item.id)?;
        }
        item.updated_at = now;

        let target_dir = match &item.workspace_id {
            Some(ws_id) if !ws_id.is_empty() => self.paths.ensure_workspace_library_dir(ws_id)?,
            _ => self.paths.ensure_global_library_dir()?,
        };

        // An existing item must be updated in place within its own scope;
        // writing the same id into a second scope would shadow or orphan it.
        if let Some(existing) = self.find_item_file(&item.id)? {
            if existing.parent() != Some(target_dir.as_path()) {
                return Err(format!(
                    "Library item '{}' already exists in another scope; edit it there or use a new id",
                    item.id
                ));
            }
        }

        let file_path = target_dir.join(format!("{}.json", item.id));
        atomic_write_json(&file_path, &item)?;
        Ok(item)
    }

    pub fn delete_item(&self, id: &str) -> Result<(), String> {
        validate_item_id(id)?;
        if let Some(path) = self.find_item_file(id)? {
            fs::remove_file(&path)
                .map_err(|e| format!("Failed deleting library item {id}: {e}"))?;
            Ok(())
        } else {
            Err(format!("Library item '{id}' not found"))
        }
    }

    pub fn delete_item_scoped(&self, id: &str, workspace_id: Option<&str>) -> Result<(), String> {
        validate_item_id(id)?;
        let workspace_id = workspace_id.filter(|value| !value.trim().is_empty());
        if let Some(workspace_id) = workspace_id {
            validate_workspace_id(workspace_id)?;
        }
        if let Some(path) = self.find_item_file_scoped(id, workspace_id)? {
            fs::remove_file(&path)
                .map_err(|e| format!("Failed deleting library item {id}: {e}"))?;
            Ok(())
        } else {
            Err(format!(
                "Library item '{id}' not found in the requested scope"
            ))
        }
    }

    pub fn rename_item(
        &self,
        id: &str,
        workspace_id: Option<&str>,
        title: &str,
    ) -> Result<LibraryItem, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("Library item title cannot be empty".to_string());
        }
        let mut item = self.get_item_scoped(id, workspace_id)?;
        item.title = title.to_string();
        self.save_item(item)
    }

    /// Import one readable file into a Workspace-scoped Library snapshot.
    /// The source path is retained as provenance, while content remains a
    /// durable snapshot so later source edits cannot silently change a task.
    pub fn add_file(
        &self,
        workspace_id: Option<&str>,
        source_path: &str,
        title: Option<&str>,
        category: LibraryCategory,
        collection: Option<&str>,
        tags: Vec<String>,
    ) -> Result<LibraryItem, String> {
        if let Some(workspace_id) = workspace_id {
            validate_workspace_id(workspace_id)?;
        }
        let source_path = source_path.trim();
        if source_path.is_empty() {
            return Err("Library source path cannot be empty".to_string());
        }
        let resolved = self.resolve_import_source(workspace_id, source_path)?;
        let metadata = fs::symlink_metadata(&resolved).map_err(|e| {
            format!(
                "Failed to inspect library source '{}': {e}",
                resolved.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "Library source must be a regular file, not a directory or symlink: {}",
                resolved.display()
            ));
        }

        let bytes = fs::read(&resolved).map_err(|e| {
            format!(
                "Failed reading library source '{}': {e}",
                resolved.display()
            )
        })?;
        let extension = resolved
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_lowercase());
        let content = match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(_) => format!("[二进制文件快照: {}]", resolved.display()),
        };
        let filename = resolved
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Library file");
        let now = Utc::now().to_rfc3339();
        let mut metadata_map = std::collections::BTreeMap::new();
        metadata_map.insert("size_bytes".to_string(), metadata.len().to_string());
        if let Some(extension) = extension {
            metadata_map.insert("extension".to_string(), extension);
        }

        let citation = LibraryCitation {
            source_type: "file".to_string(),
            source_id: Some(source_path.to_string()),
            label: Some(filename.to_string()),
            url: None,
            artifact_id: None,
            run_id: None,
            tool_call_id: None,
            created_at: now.clone(),
        };
        self.save_item(LibraryItem {
            id: format!("lib-{}", Uuid::new_v4()),
            workspace_id: workspace_id.map(String::from),
            title: title
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(filename)
                .to_string(),
            description: format!("从文件快照导入: {filename}"),
            category,
            content,
            tags,
            source_path: Some(source_path.to_string()),
            collection: collection
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(String::from),
            metadata: metadata_map,
            citations: vec![citation],
            source_artifact_id: None,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    /// Import every regular file under a directory. Symlinks are skipped so
    /// a directory import cannot escape the already-authorized source root.
    pub fn add_directory(
        &self,
        workspace_id: Option<&str>,
        source_path: &str,
        category: LibraryCategory,
        collection: Option<&str>,
        tags: Vec<String>,
    ) -> Result<Vec<LibraryItem>, String> {
        if let Some(workspace_id) = workspace_id {
            validate_workspace_id(workspace_id)?;
        }
        let resolved = self.resolve_import_source(workspace_id, source_path)?;
        let metadata = fs::symlink_metadata(&resolved).map_err(|e| {
            format!(
                "Failed to inspect library directory '{}': {e}",
                resolved.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!(
                "Library source must be a regular directory, not a file or symlink: {}",
                resolved.display()
            ));
        }

        let mut files = Vec::new();
        Self::collect_regular_files(&resolved, &mut files)?;
        files.sort();
        let mut imported = Vec::with_capacity(files.len());
        for path in files {
            let path = path.to_string_lossy().into_owned();
            imported.push(self.add_file(
                workspace_id,
                &path,
                None,
                category,
                collection,
                tags.clone(),
            )?);
        }
        Ok(imported)
    }

    /// Resolve a host-selected import source. Relative paths remain confined
    /// to the selected Workspace. Absolute paths are allowed here because the
    /// caller is an explicit native file/directory picker action; the runtime
    /// Library tools never call this path. Direct symlinks are rejected by the
    /// caller before the snapshot is read.
    fn resolve_import_source(
        &self,
        workspace_id: Option<&str>,
        source_path: &str,
    ) -> Result<PathBuf, String> {
        let path = Path::new(source_path);
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }
        let workspace_id = workspace_id.ok_or_else(|| {
            "Relative Library import paths require a Workspace; choose an absolute path instead"
                .to_string()
        })?;
        self.paths
            .resolve_and_confine_target(workspace_id, None, source_path, false)
    }

    pub fn create_from_artifact(
        &self,
        workspace_id: &str,
        artifact_id: &str,
        title: &str,
        category: LibraryCategory,
    ) -> Result<LibraryItem, String> {
        let artifact = crate::work::artifacts::list_with_paths(&self.paths, workspace_id, None)?
            .into_iter()
            .find(|a| a.id == artifact_id)
            .ok_or_else(|| {
                format!("Work Artifact '{artifact_id}' not found in workspace '{workspace_id}'")
            })?;
        let artifact_path =
            self.paths
                .resolve_workspace_path(workspace_id, Path::new(&artifact.path), false)?;

        let content = if artifact_path.is_file() {
            match fs::read_to_string(&artifact_path) {
                Ok(text) => text,
                Err(_) => format!("[二进制文件产物: {}]", artifact.title),
            }
        } else {
            format!("[产物文件已交付: {}]", artifact.path)
        };

        let item = LibraryItem {
            id: format!("lib-{}", Uuid::new_v4()),
            workspace_id: Some(workspace_id.to_string()),
            title: if title.trim().is_empty() {
                artifact.title.clone()
            } else {
                title.trim().to_string()
            },
            description: format!("沉淀自任务成果物: {}", artifact.title),
            category,
            content,
            tags: vec!["artifact".to_string(), artifact.artifact_type],
            source_path: Some(artifact.path.clone()),
            collection: None,
            metadata: {
                let mut metadata = std::collections::BTreeMap::new();
                metadata.insert("artifact_version".to_string(), artifact.version.to_string());
                metadata.insert("mime_type".to_string(), artifact.mime_type.clone());
                metadata
            },
            citations: vec![LibraryCitation {
                source_type: "artifact".to_string(),
                source_id: Some(artifact_id.to_string()),
                label: Some(artifact.title.clone()),
                url: None,
                artifact_id: Some(artifact_id.to_string()),
                run_id: artifact.run_id.clone(),
                tool_call_id: None,
                created_at: Utc::now().to_rfc3339(),
            }],
            source_artifact_id: Some(artifact_id.to_string()),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        self.save_item(item)
    }

    fn collect_items_from_dir(
        &self,
        dir: &Path,
        items: &mut Vec<LibraryItemSummary>,
    ) -> Result<(), String> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(item) = serde_json::from_str::<LibraryItem>(&content) {
                        items.push(item.summary());
                    }
                }
            }
        }
        Ok(())
    }

    fn find_item_file(&self, id: &str) -> Result<Option<PathBuf>, String> {
        let file_name = format!("{id}.json");

        // 1. Check global library
        let global_file = self.paths.global_library_dir().join(&file_name);
        if global_file.is_file() {
            return Ok(Some(global_file));
        }

        // 2. Check all workspace libraries
        let ws_dir = self.paths.workspaces_dir();
        if ws_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(ws_dir) {
                for entry in entries.flatten() {
                    let candidate = entry.path().join("library").join(&file_name);
                    if candidate.is_file() {
                        return Ok(Some(candidate));
                    }
                }
            }
        }

        Ok(None)
    }

    fn find_item_file_scoped(
        &self,
        id: &str,
        workspace_id: Option<&str>,
    ) -> Result<Option<PathBuf>, String> {
        let global_file = self.paths.global_library_dir().join(format!("{id}.json"));
        if global_file.is_file() {
            return Ok(Some(global_file));
        }
        if let Some(workspace_id) = workspace_id {
            let workspace_file = self
                .paths
                .workspace_library_dir(workspace_id)?
                .join(format!("{id}.json"));
            if workspace_file.is_file() {
                return Ok(Some(workspace_file));
            }
        }
        Ok(None)
    }

    fn collect_regular_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                Self::collect_regular_files(&entry.path(), files)?;
            } else if metadata.is_file() {
                files.push(entry.path());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_library_crud_and_query_filters() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = LibraryManager::new(paths.clone());

        // 1. Save global item
        let global_item = LibraryItem {
            id: "".into(),
            workspace_id: None,
            title: "代码审查规范".into(),
            description: "通用 Rust 与 TS 代码规范".into(),
            category: LibraryCategory::Rule,
            content: "所有 public 函数必须提供文档注释。".into(),
            tags: vec!["standards".into(), "code".into()],
            source_path: None,
            collection: None,
            metadata: std::collections::BTreeMap::new(),
            citations: Vec::new(),
            source_artifact_id: None,
            created_at: "".into(),
            updated_at: "".into(),
        };
        let saved_global = manager.save_item(global_item).unwrap();
        assert!(!saved_global.id.is_empty());
        assert_eq!(saved_global.category, LibraryCategory::Rule);

        // 2. Save workspace item
        let ws_item = LibraryItem {
            id: "".into(),
            workspace_id: Some("ws-alpha".into()),
            title: "季度报告模版".into(),
            description: "财务与业务关键指标周报模版".into(),
            category: LibraryCategory::Template,
            content: "# 季度经营复盘\n\n## 1. 营收概览\n".into(),
            tags: vec!["finance".into(), "template".into()],
            source_path: None,
            collection: Some("finance".into()),
            metadata: std::collections::BTreeMap::new(),
            citations: Vec::new(),
            source_artifact_id: None,
            created_at: "".into(),
            updated_at: "".into(),
        };
        let saved_ws = manager.save_item(ws_item).unwrap();

        // 3. Query without workspace_id (should only see global)
        let list_global_only = manager.list_items(None, None, None).unwrap();
        assert_eq!(list_global_only.len(), 1);
        assert_eq!(list_global_only[0].id, saved_global.id);

        // 4. Query with ws-alpha (should see global + ws-alpha)
        let list_all_for_ws = manager.list_items(Some("ws-alpha"), None, None).unwrap();
        assert_eq!(list_all_for_ws.len(), 2);

        // 5. Query by category
        let list_rules = manager
            .list_items(Some("ws-alpha"), Some(LibraryCategory::Rule), None)
            .unwrap();
        assert_eq!(list_rules.len(), 1);
        assert_eq!(list_rules[0].id, saved_global.id);

        // 6. Query by keyword
        let list_query = manager
            .list_items(Some("ws-alpha"), None, Some("财务"))
            .unwrap();
        assert_eq!(list_query.len(), 1);
        assert_eq!(list_query[0].id, saved_ws.id);

        // 7. Get item
        let fetched = manager.get_item(&saved_ws.id).unwrap();
        assert_eq!(fetched.title, "季度报告模版");
        assert!(fetched.content.contains("营收概览"));

        // 8. Delete item
        manager.delete_item(&saved_ws.id).unwrap();
        assert!(manager.get_item(&saved_ws.id).is_err());
        assert_eq!(
            manager
                .list_items(Some("ws-alpha"), None, None)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn test_library_rejects_unsafe_ids_and_scope_collisions() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = LibraryManager::new(paths.clone());

        // Traversal-style ids must be rejected on every entry point.
        assert!(manager.get_item("../evil").is_err());
        assert!(manager.delete_item("a/b").is_err());
        assert!(manager.get_item("").is_err());

        let base = LibraryItem {
            id: "lib-manual-1".to_string(),
            workspace_id: Some("ws-beta".to_string()),
            title: "ws 资料".to_string(),
            description: String::new(),
            category: LibraryCategory::Doc,
            content: "正文".to_string(),
            tags: Vec::new(),
            source_path: None,
            collection: None,
            metadata: std::collections::BTreeMap::new(),
            citations: Vec::new(),
            source_artifact_id: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        manager.save_item(base.clone()).unwrap();

        // Same id re-saved into another scope must not create a shadow copy.
        let cross_scope = LibraryItem {
            workspace_id: None,
            title: "全局同名资料".to_string(),
            ..base.clone()
        };
        assert!(manager.save_item(cross_scope).is_err());

        // Same id within the same scope is a normal update.
        let update = LibraryItem {
            title: "ws 资料(更新)".to_string(),
            ..base
        };
        let updated = manager.save_item(update).unwrap();
        assert_eq!(updated.title, "ws 资料(更新)");
        assert_eq!(
            manager
                .list_items(Some("ws-beta"), None, None)
                .unwrap()
                .iter()
                .filter(|i| i.id == "lib-manual-1")
                .count(),
            1
        );
    }

    #[test]
    fn test_library_imports_host_files_and_keeps_scope_provenance() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = LibraryManager::new(paths);
        let source_dir = temp.path().join("knowledge");
        std::fs::create_dir_all(source_dir.join("nested")).unwrap();
        let source_file = source_dir.join("pricing.md");
        std::fs::write(&source_file, "# Pricing\n2026").unwrap();
        std::fs::write(source_dir.join("nested").join("policy.txt"), "Policy").unwrap();

        let global = manager
            .add_file(
                None,
                source_file.to_str().unwrap(),
                Some("产品定价"),
                LibraryCategory::Doc,
                Some("产品资料"),
                vec!["pricing".to_string()],
            )
            .unwrap();
        assert!(global.workspace_id.is_none());
        assert_eq!(global.source_path.as_deref(), source_file.to_str());
        assert_eq!(global.collection.as_deref(), Some("产品资料"));
        assert_eq!(
            global.metadata.get("extension").map(String::as_str),
            Some("md")
        );
        assert_eq!(global.citations.len(), 1);
        assert_eq!(global.citations[0].source_type, "file");

        let imported = manager
            .add_directory(
                Some("ws-library"),
                source_dir.to_str().unwrap(),
                LibraryCategory::Doc,
                Some("团队知识"),
                vec!["imported".to_string()],
            )
            .unwrap();
        assert_eq!(imported.len(), 2);
        assert!(imported
            .iter()
            .all(|item| item.workspace_id.as_deref() == Some("ws-library")));
        assert!(imported
            .iter()
            .all(|item| item.citations.iter().any(|c| c.source_type == "file")));

        let other_scope = manager
            .get_item_scoped(&imported[0].id, Some("ws-other"))
            .unwrap_err();
        assert!(other_scope.contains("not found"));
        let fetched = manager
            .get_item_scoped(&imported[0].id, Some("ws-library"))
            .unwrap();
        assert_eq!(fetched.collection.as_deref(), Some("团队知识"));

        let renamed = manager
            .rename_item(&imported[0].id, Some("ws-library"), "团队政策")
            .unwrap();
        assert_eq!(renamed.title, "团队政策");
        manager
            .delete_item_scoped(&renamed.id, Some("ws-library"))
            .unwrap();
        assert!(manager
            .get_item_scoped(&renamed.id, Some("ws-library"))
            .is_err());
    }
}
