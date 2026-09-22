use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::storage;
use crate::work::models::{
    ArtifactProducer, ArtifactSourceRef, ArtifactVerificationEvidence, RuntimeFact,
    WorkArtifactAcceptance, WorkArtifactCategory, WorkArtifactCheck, WorkArtifactCheckStatus,
    WorkArtifactPreviewKind, WorkArtifactRequirement, WorkArtifactStatus, WorkArtifactStorageMode,
    WorkArtifactSummary, WorkRootKind, WorkTask,
};
use crate::work::paths::WorkPaths;
use crate::work::workspace;

const MAX_REGISTRY_BYTES: u64 = 4 * 1024 * 1024;
const MAX_OFFICE_EDIT_BYTES: usize = 25 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ArtifactRegistry {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    artifacts: Vec<StoredArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct StoredArtifact {
    id: String,
    workspace_id: String,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(alias = "type")]
    artifact_type: String,
    title: String,
    path: String,
    status: WorkArtifactStatus,
    #[serde(default)]
    size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    producer: Option<ArtifactProducer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    evidence: Option<ArtifactVerificationEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sources: Vec<ArtifactSourceRef>,
    /// Content revision counter. 0 (legacy entries) is displayed as 1.
    #[serde(default)]
    version: u32,
    /// Human-readable one-line validation outcome persisted on each
    /// validate/deliver transition. Legacy entries derive it from evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    validation_summary: Option<String>,
    created_at: String,
    updated_at: String,
}

pub fn list(workspace_id: &str, run_id: Option<&str>) -> Result<Vec<WorkArtifactSummary>, String> {
    list_with_paths(&WorkPaths::app(), workspace_id, run_id)
}

pub fn list_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    run_id: Option<&str>,
) -> Result<Vec<WorkArtifactSummary>, String> {
    crate::work::workspace::WorkspaceManager::new(paths.clone()).get(workspace_id)?;
    let mut registry = read_registry(paths, workspace_id)?;
    // Reconcile the workspace-wide registry only for the Workspace overview.
    // A run-scoped read must never discover an old output file and attribute it
    // to the currently open WorkRun.
    if run_id.is_none() && reconcile_output_artifacts(paths, workspace_id, &mut registry)? {
        write_registry(paths, workspace_id, &registry)?;
    }
    let mut artifacts = registry
        .artifacts
        .into_iter()
        .filter_map(|artifact| sanitize_artifact(paths, workspace_id, artifact).ok())
        .filter(|artifact| run_id.is_none() || artifact.run_id.as_deref() == run_id)
        .map(to_summary)
        .collect::<Vec<_>>();
    artifacts.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(artifacts)
}

pub fn register(
    workspace_id: &str,
    relative_path: &str,
    title: &str,
    artifact_type: Option<&str>,
    run_id: Option<&str>,
) -> Result<WorkArtifactSummary, String> {
    workspace::manager().get(workspace_id)?;
    register_with_paths(
        &WorkPaths::app(),
        workspace_id,
        relative_path,
        title,
        artifact_type,
        run_id,
    )
}

pub fn register_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    relative_path: &str,
    title: &str,
    artifact_type: Option<&str>,
    run_id: Option<&str>,
) -> Result<WorkArtifactSummary, String> {
    register_with_provenance_and_paths(
        paths,
        workspace_id,
        relative_path,
        title,
        artifact_type,
        run_id,
        run_id.map(|r| ArtifactProducer {
            run_id: r.to_string(),
            producer_tool_call_id: None,
            execution_id: None,
        }),
        Vec::new(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn register_with_provenance_and_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    relative_path: &str,
    title: &str,
    artifact_type: Option<&str>,
    run_id: Option<&str>,
    producer: Option<ArtifactProducer>,
    sources: Vec<ArtifactSourceRef>,
) -> Result<WorkArtifactSummary, String> {
    let path = output_file_path(paths, workspace_id, relative_path)?;
    let metadata =
        fs::metadata(&path).map_err(|error| format!("Artifact file not found: {error}"))?;
    if !metadata.is_file() {
        return Err("Artifact path is not a file".into());
    }

    let title = normalize_label(title, "Artifact title", 160)?;
    let artifact_type = normalize_label(
        artifact_type.unwrap_or_else(|| {
            path.extension()
                .and_then(|value| value.to_str())
                .unwrap_or("file")
        }),
        "Artifact type",
        40,
    )?;
    // Register is the only lifecycle entry point: an output file that passes the
    // format check is immediately considered validated & delivered, so the UI
    // never needs separate validate/deliver actions. An invalid file fails here.
    let size = validate_artifact_file(&path, &artifact_type)
        .map_err(|error| format!("Artifact validation failed: {error}"))?;
    let sha256 = compute_file_sha256(&path)?;
    let mut registry = read_registry(paths, workspace_id)?;
    let now = Utc::now().to_rfc3339();
    let relative_path = normalize_relative_path(relative_path)?;

    let evidence = ArtifactVerificationEvidence {
        verified_at: now.clone(),
        validator_version: "v1.0.0".to_string(),
        sha256: sha256.clone(),
        size,
        checks: vec![
            "format_valid".to_string(),
            "hash_computed".to_string(),
            "path_confined".to_string(),
        ],
        verification_status: WorkArtifactStatus::Delivered,
    };

    // Prefer an exact run-scoped match; otherwise claim the unattributed entry
    // discovered by output reconciliation so the same path is not duplicated.
    let existing_index = registry
        .artifacts
        .iter()
        .position(|artifact| artifact.path == relative_path && artifact.run_id.as_deref() == run_id)
        .or_else(|| {
            registry
                .artifacts
                .iter()
                .position(|artifact| artifact.path == relative_path && artifact.run_id.is_none())
        });
    let artifact = if let Some(index) = existing_index {
        let existing = &mut registry.artifacts[index];
        // Bump the content revision when the same path is re-registered with
        // different bytes; identical content keeps the version stable.
        let content_changed = existing.sha256.as_deref().is_none_or(|old| old != sha256);
        let next_version = if content_changed {
            existing.version.max(1) + u32::from(existing.sha256.is_some())
        } else {
            existing.version.max(1)
        };
        existing.title = title;
        existing.artifact_type = artifact_type;
        existing.status = WorkArtifactStatus::Delivered;
        existing.run_id = run_id.map(str::to_string);
        existing.size = size;
        existing.sha256 = Some(sha256);
        existing.evidence = Some(evidence);
        existing.version = next_version;
        existing.validation_summary = Some(VALIDATION_SUMMARY_PASSED.to_string());
        if producer.is_some() {
            existing.producer = producer;
        }
        if !sources.is_empty() {
            existing.sources = sources;
        }
        existing.updated_at = now;
        existing.clone()
    } else {
        let artifact = StoredArtifact {
            id: Uuid::new_v4().to_string(),
            workspace_id: workspace_id.to_string(),
            run_id: run_id.map(str::to_string),
            artifact_type,
            title,
            path: relative_path,
            status: WorkArtifactStatus::Delivered,
            size,
            sha256: Some(sha256),
            producer,
            evidence: Some(evidence),
            sources,
            version: 1,
            validation_summary: Some(VALIDATION_SUMMARY_PASSED.to_string()),
            created_at: now.clone(),
            updated_at: now,
        };
        registry.artifacts.push(artifact.clone());
        artifact
    };
    write_registry(paths, workspace_id, &registry)?;
    Ok(to_summary(artifact))
}

/// Extract verified source provenance from runtime ledger facts.
pub fn extract_sources_from_ledger(
    paths: &WorkPaths,
    task_id: &str,
    run_id: &str,
    candidate_source_tool_call_ids: &[String],
) -> Vec<ArtifactSourceRef> {
    let Ok(ledger) = crate::work::ledger::WorkRuntimeLedger::open(paths, task_id, run_id) else {
        return Vec::new();
    };
    let Ok(facts) = ledger.list_facts() else {
        return Vec::new();
    };

    let target_ids: Vec<String> = if candidate_source_tool_call_ids.is_empty() {
        // Collect all research / information-gathering tool call IDs with successful results
        facts
            .iter()
            .filter_map(|f| match f {
                RuntimeFact::ToolProposed {
                    tool_call_id,
                    tool_name,
                    ..
                } => {
                    let is_info_tool = tool_name.contains("search")
                        || tool_name.contains("browser")
                        || tool_name.contains("open")
                        || tool_name.contains("fetch")
                        || tool_name.starts_with("mcp_")
                        || tool_name.starts_with("connector_");
                    if is_info_tool {
                        Some(tool_call_id.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect()
    } else {
        candidate_source_tool_call_ids.to_vec()
    };

    let mut sources = Vec::new();
    for tool_call_id in target_ids {
        // Find proposed tool to get tool_name
        let proposed = facts.iter().find_map(|f| match f {
            RuntimeFact::ToolProposed {
                tool_call_id: id,
                tool_name,
                ..
            } if id == &tool_call_id => Some(tool_name.clone()),
            _ => None,
        });

        // Find successful ToolResult
        let result_fact = facts.iter().rev().find_map(|f| match f {
            RuntimeFact::ToolResult {
                tool_call_id: id,
                success: true,
                outputs,
                timestamp,
                ..
            } if id == &tool_call_id => Some((outputs.clone(), timestamp.clone())),
            _ => None,
        });

        if let (Some(tool_name), Some((outputs, timestamp))) = (proposed, result_fact) {
            let source_type = if tool_name.contains("search") {
                "web_search"
            } else if tool_name.contains("browser")
                || tool_name.contains("open")
                || tool_name.contains("fetch")
            {
                "web_open"
            } else if tool_name.starts_with("mcp_") {
                "mcp"
            } else {
                "connector"
            };

            let mut hasher = Sha256::new();
            for out in &outputs {
                hasher.update(out.as_bytes());
            }
            let result_digest = format!("{:x}", hasher.finalize());

            let url = outputs.iter().find_map(|out| {
                let trimmed = out.trim();
                if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                    Some(trimmed.to_string())
                } else {
                    None
                }
            });

            sources.push(ArtifactSourceRef {
                source_tool_call_id: tool_call_id.clone(),
                source_type: source_type.to_string(),
                url,
                result_digest,
                captured_at: timestamp,
                is_claimed: false,
            });
        }
    }
    sources
}

pub fn update_office_base64(
    workspace_id: &str,
    artifact_id: &str,
    content_base64: &str,
    run_id: Option<&str>,
) -> Result<WorkArtifactSummary, String> {
    workspace::manager().get(workspace_id)?;
    update_office_base64_with_paths(
        &WorkPaths::app(),
        workspace_id,
        artifact_id,
        content_base64,
        run_id,
    )
}

pub fn update_office_base64_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    artifact_id: &str,
    content_base64: &str,
    run_id: Option<&str>,
) -> Result<WorkArtifactSummary, String> {
    let bytes = decode_office_base64(content_base64)?;
    let (relative_path, artifact_type) = {
        let mut registry = read_registry(paths, workspace_id)?;
        let artifact = find_artifact_mut(&mut registry, workspace_id, artifact_id, run_id)?;
        let artifact_type = normalize_office_type(&artifact.artifact_type)?;
        (artifact.path.clone(), artifact_type)
    };
    let path = output_file_path(paths, workspace_id, &relative_path)?;
    let size = write_validated_office_bytes(&path, &artifact_type, &bytes)?;
    let sha256 = compute_file_sha256(&path)?;
    let now = Utc::now().to_rfc3339();
    let mut registry = read_registry(paths, workspace_id)?;
    let artifact = find_artifact_mut(&mut registry, workspace_id, artifact_id, run_id)?;
    artifact.status = WorkArtifactStatus::Delivered;
    // Office editing replaces the document bytes: always a new revision.
    artifact.version = artifact.version.max(1) + 1;
    artifact.size = size;
    artifact.sha256 = Some(sha256.clone());
    artifact.evidence = Some(ArtifactVerificationEvidence {
        verified_at: now.clone(),
        validator_version: "v1.0.0".to_string(),
        sha256,
        size,
        checks: vec![
            "format_valid".to_string(),
            "hash_computed".to_string(),
            "path_confined".to_string(),
        ],
        verification_status: WorkArtifactStatus::Delivered,
    });
    artifact.validation_summary = Some(VALIDATION_SUMMARY_PASSED.to_string());
    artifact.updated_at = now;
    let result = artifact.clone();
    write_registry(paths, workspace_id, &registry)?;
    Ok(to_summary(result))
}

pub fn create_office_base64(
    workspace_id: &str,
    relative_path: &str,
    title: &str,
    artifact_type: &str,
    content_base64: &str,
    run_id: Option<&str>,
) -> Result<WorkArtifactSummary, String> {
    workspace::manager().get(workspace_id)?;
    create_office_base64_with_paths(
        &WorkPaths::app(),
        workspace_id,
        relative_path,
        title,
        artifact_type,
        content_base64,
        run_id,
    )
}

pub fn create_office_base64_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    relative_path: &str,
    title: &str,
    artifact_type: &str,
    content_base64: &str,
    run_id: Option<&str>,
) -> Result<WorkArtifactSummary, String> {
    let bytes = decode_office_base64(content_base64)?;
    let artifact_type = normalize_office_type(artifact_type)?;
    let title = normalize_label(title, "Artifact title", 160)?;
    let path = output_file_path(paths, workspace_id, relative_path)?;
    if path.exists() {
        return Err("Office Artifact already exists".into());
    }
    write_validated_office_bytes(&path, &artifact_type, &bytes)?;
    register_with_paths(
        paths,
        workspace_id,
        relative_path,
        &title,
        Some(&artifact_type),
        run_id,
    )
}

pub fn validate(
    workspace_id: &str,
    artifact_id: &str,
    run_id: Option<&str>,
) -> Result<WorkArtifactSummary, String> {
    workspace::manager().get(workspace_id)?;
    let paths = WorkPaths::app();
    validate_with_paths(&paths, workspace_id, artifact_id, run_id)
}

pub fn validate_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    artifact_id: &str,
    run_id: Option<&str>,
) -> Result<WorkArtifactSummary, String> {
    let mut registry = read_registry(paths, workspace_id)?;
    let artifact = find_artifact_mut(&mut registry, workspace_id, artifact_id, run_id)?;
    let path = output_file_path(paths, workspace_id, &artifact.path);
    let was_delivered = artifact.status == WorkArtifactStatus::Delivered;
    let now = Utc::now().to_rfc3339();
    match path.as_ref().map_err(|e| e.clone()).and_then(|p| {
        let size = validate_artifact_file(p, &artifact.artifact_type)?;
        let sha = compute_file_sha256(p)?;
        Ok((size, sha))
    }) {
        Ok((size, sha256)) => {
            let target_status = if was_delivered {
                WorkArtifactStatus::Delivered
            } else {
                WorkArtifactStatus::Validated
            };
            artifact.status = target_status.clone();
            artifact.size = size;
            artifact.sha256 = Some(sha256.clone());
            artifact.evidence = Some(ArtifactVerificationEvidence {
                verified_at: now.clone(),
                validator_version: "v1.0.0".to_string(),
                sha256,
                size,
                checks: vec![
                    "format_valid".to_string(),
                    "hash_computed".to_string(),
                    "path_confined".to_string(),
                ],
                verification_status: target_status,
            });
            artifact.validation_summary = Some(VALIDATION_SUMMARY_PASSED.to_string());
        }
        Err(error) => {
            artifact.status = WorkArtifactStatus::Invalid;
            artifact.evidence = None;
            artifact.validation_summary = Some(format!("验证失败：{error}"));
        }
    }
    artifact.updated_at = now;
    let result = artifact.clone();
    write_registry(paths, workspace_id, &registry)?;
    Ok(to_summary(result))
}

pub fn deliver(
    workspace_id: &str,
    artifact_id: &str,
    run_id: Option<&str>,
) -> Result<WorkArtifactSummary, String> {
    workspace::manager().get(workspace_id)?;
    deliver_with_paths(&WorkPaths::app(), workspace_id, artifact_id, run_id)
}

pub fn deliver_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    artifact_id: &str,
    run_id: Option<&str>,
) -> Result<WorkArtifactSummary, String> {
    let mut registry = read_registry(paths, workspace_id)?;
    let artifact = find_artifact_mut(&mut registry, workspace_id, artifact_id, run_id)?;
    let path = output_file_path(paths, workspace_id, &artifact.path)?;
    let (size, sha256) = match (
        validate_artifact_file(&path, &artifact.artifact_type),
        compute_file_sha256(&path),
    ) {
        (Ok(size), Ok(sha)) => (size, sha),
        (Err(error), _) | (_, Err(error)) => {
            artifact.status = WorkArtifactStatus::Invalid;
            artifact.evidence = None;
            artifact.validation_summary = Some(format!("验证失败：{error}"));
            artifact.updated_at = Utc::now().to_rfc3339();
            let _ = write_registry(paths, workspace_id, &registry);
            return Err(format!("Artifact validation failed: {error}"));
        }
    };
    if !matches!(
        artifact.status,
        WorkArtifactStatus::Validated | WorkArtifactStatus::Delivered
    ) {
        return Err("Artifact must be validated before delivery".into());
    }
    let now = Utc::now().to_rfc3339();
    artifact.status = WorkArtifactStatus::Delivered;
    artifact.size = size;
    artifact.sha256 = Some(sha256.clone());
    artifact.evidence = Some(ArtifactVerificationEvidence {
        verified_at: now.clone(),
        validator_version: "v1.0.0".to_string(),
        sha256,
        size,
        checks: vec![
            "format_valid".to_string(),
            "hash_computed".to_string(),
            "path_confined".to_string(),
        ],
        verification_status: WorkArtifactStatus::Delivered,
    });
    artifact.validation_summary = Some(VALIDATION_SUMMARY_PASSED.to_string());
    artifact.updated_at = now;
    let result = artifact.clone();
    write_registry(paths, workspace_id, &registry)?;
    Ok(to_summary(result))
}

pub fn delete(workspace_id: &str, artifact_id: &str) -> Result<(), String> {
    workspace::manager().get(workspace_id)?;
    delete_with_paths(&WorkPaths::app(), workspace_id, artifact_id)
}

pub fn delete_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    artifact_id: &str,
) -> Result<(), String> {
    let mut registry = read_registry(paths, workspace_id)?;
    let index = registry
        .artifacts
        .iter()
        .position(|artifact| artifact.workspace_id == workspace_id && artifact.id == artifact_id)
        .ok_or_else(|| format!("Work Artifact '{artifact_id}' not found"))?;
    let path = registry.artifacts[index].path.clone();
    let still_referenced = registry
        .artifacts
        .iter()
        .any(|artifact| artifact.id != artifact_id && artifact.path == path);
    if still_referenced {
        // Another registry entry points at the same output file: keep the file.
        registry.artifacts.remove(index);
        write_registry(paths, workspace_id, &registry)?;
        return Ok(());
    }
    // Remove the physical file as well; otherwise the next workspace-level
    // reconciliation would rediscover it as a brand-new Artifact.
    let file_path = output_file_path(paths, workspace_id, &path)?;
    match fs::symlink_metadata(&file_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err("Refusing to remove a symbolic link".into());
            }
            if metadata.is_file() {
                fs::remove_file(&file_path)
                    .map_err(|error| format!("Failed to remove Artifact file: {error}"))?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // The file is already gone: dropping the registry entry is enough.
        }
        Err(error) => return Err(format!("Cannot inspect Artifact file: {error}")),
    }
    registry.artifacts.remove(index);
    write_registry(paths, workspace_id, &registry)?;
    Ok(())
}

pub fn compute_file_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub struct ArtifactValidator;

impl ArtifactValidator {
    pub fn validate_file(path: &Path, artifact_type: &str) -> Result<u64, String> {
        validate_artifact_file(path, artifact_type)
    }

    pub fn compute_sha256(path: &Path) -> Result<String, String> {
        compute_file_sha256(path)
    }
}

fn validate_artifact_file(path: &Path, artifact_type: &str) -> Result<u64, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("symbolic links are not valid Artifacts".into());
    }
    if !metadata.is_file() {
        return Err("path is not a file".into());
    }
    if metadata.len() == 0 {
        return Err("file is empty".into());
    }

    let kind = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or(artifact_type)
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut header = [0u8; 16];
    let bytes_read = file.read(&mut header).map_err(|error| error.to_string())?;

    let matches_header =
        |expected: &[u8]| bytes_read >= expected.len() && header.starts_with(expected);
    match kind.as_str() {
        "pdf" => {
            if !matches_header(b"%PDF-") {
                return Err("file does not contain a PDF signature".into());
            }
        }
        "png" => {
            if !matches_header(b"\x89PNG\r\n\x1a\n") {
                return Err("file does not contain a PNG signature".into());
            }
        }
        "jpg" | "jpeg" => {
            if bytes_read < 2 || header[..2] != [0xff, 0xd8] {
                return Err("file does not contain a JPEG signature".into());
            }
        }
        "gif" => {
            if !(matches_header(b"GIF87a") || matches_header(b"GIF89a")) {
                return Err("file does not contain a GIF signature".into());
            }
        }
        "webp" => {
            if bytes_read < 12 || &header[..4] != b"RIFF" || &header[8..12] != b"WEBP" {
                return Err("file does not contain a WebP signature".into());
            }
        }
        "docx" => validate_office_zip(path, &["[Content_Types].xml", "word/document.xml"])?,
        "pptx" => validate_office_zip(path, &["[Content_Types].xml", "ppt/presentation.xml"])?,
        "xlsx" => validate_office_zip(path, &["[Content_Types].xml", "xl/workbook.xml"])?,
        "json" => {
            let content = fs::read(path).map_err(|e| format!("cannot read JSON file: {e}"))?;
            serde_json::from_slice::<serde_json::Value>(&content)
                .map_err(|e| format!("invalid JSON structure: {e}"))?;
        }
        "csv" => {
            let content = fs::read_to_string(path)
                .map_err(|e| format!("CSV file is not valid UTF-8: {e}"))?;
            if content.trim().is_empty() {
                return Err("CSV file is empty".into());
            }
        }
        "md" | "markdown" | "txt" => {
            let _ = fs::read_to_string(path)
                .map_err(|e| format!("text file is not valid UTF-8: {e}"))?;
        }
        "py" | "sh" | "js" | "ts" => {
            let content = fs::read_to_string(path)
                .map_err(|e| format!("script file is not valid UTF-8: {e}"))?;
            if content.trim().is_empty() {
                return Err("script file is empty".into());
            }
        }
        _ => {
            // For plain text and other registered types, opening and reading the
            // file is the strongest format-independent check available here.
        }
    }
    Ok(metadata.len())
}

fn validate_office_zip(path: &Path, required_entries: &[&str]) -> Result<(), String> {
    let file =
        fs::File::open(path).map_err(|error| format!("cannot open Office package: {error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("invalid Office ZIP package: {error}"))?;
    for entry in required_entries {
        archive
            .by_name(entry)
            .map_err(|error| format!("Office package is missing {entry}: {error}"))?;
    }
    Ok(())
}

pub fn export(
    workspace_id: &str,
    artifact_id: &str,
    destination_path: &str,
    run_id: Option<&str>,
) -> Result<String, String> {
    workspace::manager().get(workspace_id)?;
    export_with_paths(
        &WorkPaths::app(),
        workspace_id,
        artifact_id,
        destination_path,
        run_id,
    )
}

pub fn export_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    artifact_id: &str,
    destination_path: &str,
    run_id: Option<&str>,
) -> Result<String, String> {
    let registry = read_registry(paths, workspace_id)?;
    let artifact = registry
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.workspace_id == workspace_id
                && artifact.id == artifact_id
                && (run_id.is_none() || artifact.run_id.as_deref() == run_id)
        })
        .ok_or_else(|| format!("Work Artifact '{artifact_id}' not found"))?;
    if artifact.status != WorkArtifactStatus::Delivered {
        return Err("Artifact must be delivered before export".into());
    }

    let source_path = output_file_path(paths, workspace_id, &artifact.path)?;
    validate_artifact_file(&source_path, &artifact.artifact_type)
        .map_err(|error| format!("Artifact source is not valid: {error}"))?;
    let source = fs::canonicalize(&source_path)
        .map_err(|error| format!("Artifact source is not readable: {error}"))?;

    let destination = PathBuf::from(destination_path.trim());
    if !destination.is_absolute() {
        return Err("Export destination must be an absolute path".into());
    }
    let file_name = destination
        .file_name()
        .ok_or_else(|| "Export destination must include a file name".to_string())?;
    let parent = destination
        .parent()
        .ok_or_else(|| "Export destination has no parent directory".to_string())?;
    let parent_metadata = fs::metadata(parent)
        .map_err(|error| format!("Export destination directory is not available: {error}"))?;
    if !parent_metadata.is_dir() {
        return Err("Export destination parent is not a directory".into());
    }
    let canonical_parent = fs::canonicalize(parent).map_err(|error| error.to_string())?;
    let target = canonical_parent.join(file_name);
    if let Ok(target_metadata) = fs::symlink_metadata(&target) {
        if target_metadata.file_type().is_symlink() {
            return Err("Refusing to overwrite a symbolic link".into());
        }
        if !target_metadata.is_file() {
            return Err("Export destination is not a file".into());
        }
        if fs::canonicalize(&target).map_err(|error| error.to_string())? == source {
            return Err("Export destination is the same as the Artifact source".into());
        }
    } else if target == source {
        return Err("Export destination is the same as the Artifact source".into());
    }

    fs::copy(&source, &target).map_err(|error| format!("Failed to export Artifact: {error}"))?;
    Ok(target.to_string_lossy().into_owned())
}

/// Copy a delivered Artifact from AgentCabin's managed output into the
/// user-selected local project folder without overwriting an unrelated file.
pub fn copy_to_primary_work_root(
    workspace_id: &str,
    artifact_id: &str,
    run_id: Option<&str>,
) -> Result<String, String> {
    copy_to_primary_work_root_with_paths(&WorkPaths::app(), workspace_id, artifact_id, run_id)
}

pub fn copy_to_primary_work_root_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    artifact_id: &str,
    run_id: Option<&str>,
) -> Result<String, String> {
    let manager = workspace::WorkspaceManager::new(paths.clone());
    let workspace = manager.get(workspace_id)?;
    if workspace.root_kind != WorkRootKind::LocalFolder {
        return Err("当前 Workspace 没有关联可复制的本地工作目录".into());
    }
    if workspace.artifact_storage_mode == WorkArtifactStorageMode::PrimaryWorkRoot {
        return Err("当前已启用直接保存模式，成果本来就在本地工作目录中".into());
    }

    let registry = read_registry(paths, workspace_id)?;
    let artifact = registry
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.workspace_id == workspace_id
                && artifact.id == artifact_id
                && (run_id.is_none() || artifact.run_id.as_deref() == run_id)
        })
        .ok_or_else(|| format!("Work Artifact '{artifact_id}' not found"))?;
    if artifact.status != WorkArtifactStatus::Delivered {
        return Err("Artifact must be delivered before copying to the local work directory".into());
    }

    let source_path = output_file_path(paths, workspace_id, &artifact.path)?;
    let source_metadata = fs::symlink_metadata(&source_path)
        .map_err(|error| format!("Artifact source is not readable: {error}"))?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
        return Err("Artifact source must be a regular file".into());
    }
    validate_artifact_file(&source_path, &artifact.artifact_type)
        .map_err(|error| format!("Artifact source is not valid: {error}"))?;
    let source = fs::canonicalize(&source_path)
        .map_err(|error| format!("Artifact source is not readable: {error}"))?;

    let primary_root = manager.resolve_primary_work_root(&workspace)?;
    let canonical_primary =
        fs::canonicalize(&primary_root).map_err(|error| format!("本地工作目录不可用: {error}"))?;
    let relative = normalize_relative_path(&artifact.path)?;
    let destination_candidate = canonical_primary.join(&relative);
    crate::work::paths::ensure_within(&canonical_primary, &destination_candidate)?;
    let parent = destination_candidate
        .parent()
        .ok_or_else(|| "本地成果目标没有父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建本地成果目录: {error}"))?;
    ensure_directory_is_safe(parent, "本地成果目标目录", false)?;
    let canonical_parent =
        fs::canonicalize(parent).map_err(|error| format!("无法解析本地成果目录: {error}"))?;
    let file_name = destination_candidate
        .file_name()
        .ok_or_else(|| "本地成果目标没有文件名".to_string())?;
    let target = canonical_parent.join(file_name);
    crate::work::paths::ensure_within(&canonical_primary, &target)?;

    if let Ok(metadata) = fs::symlink_metadata(&target) {
        if metadata.file_type().is_symlink() {
            return Err("拒绝覆盖本地成果目录中的符号链接".into());
        }
        if !metadata.is_file() {
            return Err("本地成果目标已存在且不是文件".into());
        }
        if fs::read(&target).map_err(|error| error.to_string())?
            == fs::read(&source).map_err(|error| error.to_string())?
        {
            return Ok(target.to_string_lossy().into_owned());
        }
        return Err(format!(
            "本地成果目标已存在且内容不同，未覆盖: {}",
            target.display()
        ));
    }

    fs::copy(&source, &target).map_err(|error| format!("复制成果到本地工作目录失败: {error}"))?;
    Ok(target.to_string_lossy().into_owned())
}

fn read_registry(paths: &WorkPaths, workspace_id: &str) -> Result<ArtifactRegistry, String> {
    let path = paths
        .workspace_dir(workspace_id)?
        .join("context")
        .join("artifacts.json");
    if !path.is_file() {
        return Ok(ArtifactRegistry {
            version: 1,
            artifacts: Vec::new(),
        });
    }
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_REGISTRY_BYTES {
        return Err("Work Artifact registry is too large".into());
    }
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&content)
        .map_err(|error| format!("Invalid Work Artifact registry: {error}"))
}

fn reconcile_output_artifacts(
    paths: &WorkPaths,
    workspace_id: &str,
    registry: &mut ArtifactRegistry,
) -> Result<bool, String> {
    const MAX_DISCOVERED_ARTIFACTS: usize = 1000;
    let workspace_root = paths.workspace_dir(workspace_id)?;
    let output_root = paths.resolve_workspace_path(workspace_id, Path::new("output"), false)?;
    if !output_root.is_dir() {
        return Ok(false);
    }
    let logical_root = if output_root.starts_with(&workspace_root) {
        workspace_root.clone()
    } else {
        let workspace = workspace::WorkspaceManager::new(paths.clone()).get(workspace_id)?;
        workspace::WorkspaceManager::new(paths.clone()).resolve_primary_work_root(&workspace)?
    };
    let mut files = Vec::new();
    collect_output_files(&output_root, &mut files, MAX_DISCOVERED_ARTIFACTS);
    let mut changed = false;
    for file in files {
        let relative = file
            .strip_prefix(&logical_root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let metadata = fs::metadata(&file).map_err(|error| error.to_string())?;
        if let Some(existing) = registry
            .artifacts
            .iter_mut()
            .find(|artifact| artifact.path == relative)
        {
            if existing.size != metadata.len() {
                existing.size = metadata.len();
                existing.updated_at = Utc::now().to_rfc3339();
                changed = true;
            }
            continue;
        }
        let now = Utc::now().to_rfc3339();
        registry.artifacts.push(StoredArtifact {
            id: Uuid::new_v4().to_string(),
            workspace_id: workspace_id.to_string(),
            run_id: None,
            artifact_type: file
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("file")
                .to_string(),
            title: file
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Artifact")
                .to_string(),
            path: relative,
            status: WorkArtifactStatus::Ready,
            size: metadata.len(),
            sha256: None,
            producer: None,
            evidence: None,
            sources: Vec::new(),
            version: 0,
            validation_summary: None,
            created_at: now.clone(),
            updated_at: now,
        });
        changed = true;
    }
    Ok(changed)
}

fn collect_output_files(directory: &Path, output: &mut Vec<PathBuf>, max_entries: usize) {
    if output.len() >= max_entries {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if output.len() >= max_entries {
            return;
        }
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_output_files(&path, output, max_entries);
        } else if metadata.is_file() {
            output.push(path);
        }
    }
}

fn write_registry(
    paths: &WorkPaths,
    workspace_id: &str,
    registry: &ArtifactRegistry,
) -> Result<(), String> {
    let path = paths
        .workspace_dir(workspace_id)?
        .join("context")
        .join("artifacts.json");
    let content = serde_json::to_string_pretty(registry).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(&temporary, format!("{content}\n")).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    let _ = workspace::WorkspaceManager::new(paths.clone()).touch(workspace_id);
    Ok(())
}

fn sanitize_artifact(
    paths: &WorkPaths,
    workspace_id: &str,
    artifact: StoredArtifact,
) -> Result<StoredArtifact, String> {
    if artifact.workspace_id != workspace_id {
        return Err("Artifact workspace does not match".into());
    }
    let _ = output_file_path(paths, workspace_id, &artifact.path)?;
    Ok(artifact)
}

fn find_artifact_mut<'a>(
    registry: &'a mut ArtifactRegistry,
    workspace_id: &str,
    artifact_id: &str,
    run_id: Option<&str>,
) -> Result<&'a mut StoredArtifact, String> {
    registry
        .artifacts
        .iter_mut()
        .find(|artifact| {
            artifact.workspace_id == workspace_id
                && artifact.id == artifact_id
                && (run_id.is_none() || artifact.run_id.as_deref() == run_id)
        })
        .ok_or_else(|| format!("Work Artifact '{artifact_id}' not found"))
}

fn output_file_path(
    paths: &WorkPaths,
    workspace_id: &str,
    relative_path: &str,
) -> Result<std::path::PathBuf, String> {
    let relative = normalize_relative_path(relative_path)?;
    if !relative.starts_with("output/") {
        return Err("Work Artifacts must be inside output/".into());
    }
    paths.resolve_workspace_path(workspace_id, Path::new(&relative), false)
}

/// Evaluate all artifact requirements against the current WorkRun.
///
/// This is intentionally read-only. A file is accepted only when the registry
/// entry belongs to this run, the path remains inside `output/`, the file is a
/// non-empty regular file, its declared type matches, and the existing format
/// validator passes. The legacy id/title/path list is converted to the same
/// requirement shape so old task manifests keep their meaning.
pub fn check_required_artifacts_with_paths(
    paths: &WorkPaths,
    workspace_id: &str,
    run_id: &str,
    task: &WorkTask,
) -> Result<WorkArtifactAcceptance, String> {
    let mut requirements = task
        .required_artifacts
        .iter()
        .map(|path| WorkArtifactRequirement {
            path: path.clone(),
            title: None,
            artifact_type: None,
            required: true,
        })
        .collect::<Vec<_>>();
    requirements.extend(task.artifact_requirements.iter().cloned());

    if requirements.iter().all(|requirement| !requirement.required) {
        return Ok(WorkArtifactAcceptance {
            run_id: run_id.to_string(),
            checks: Vec::new(),
            required_count: 0,
            satisfied_count: 0,
            missing_count: 0,
            invalid_count: 0,
            satisfied: true,
        });
    }

    let registry = read_registry(paths, workspace_id)?;
    let mut checks = Vec::with_capacity(requirements.len());
    let mut required_count = 0;
    let mut satisfied_count = 0;
    let mut missing_count = 0;
    let mut invalid_count = 0;

    for requirement in requirements {
        if requirement.required {
            required_count += 1;
        }

        let raw_target = requirement.path.trim();
        let normalized_target = normalize_relative_path(raw_target).ok();
        let found = registry.artifacts.iter().find(|artifact| {
            if artifact.workspace_id != workspace_id || artifact.run_id.as_deref() != Some(run_id) {
                return false;
            }
            let path_match = normalized_target
                .as_deref()
                .map(|path| artifact.path == path)
                .unwrap_or(false);
            let identity_match = artifact.id == raw_target
                || artifact.title == raw_target
                || Path::new(&artifact.path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    == Some(raw_target);
            let title_match = requirement
                .title
                .as_deref()
                .map(|title| artifact.title == title.trim())
                .unwrap_or(true);
            (path_match || identity_match) && title_match
        });

        let mut check = if let Some(artifact) = found {
            let mut status = WorkArtifactCheckStatus::Satisfied;
            let mut message = "文件存在、归属于当前 Run，且已通过验证".to_string();
            let mut size = None;

            if let Some(expected_type) = requirement.artifact_type.as_deref() {
                let expected = expected_type
                    .trim()
                    .trim_start_matches('.')
                    .to_ascii_lowercase();
                let actual = artifact
                    .artifact_type
                    .trim()
                    .trim_start_matches('.')
                    .to_ascii_lowercase();
                let extension = Path::new(&artifact.path)
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if expected != actual && expected != extension {
                    status = WorkArtifactCheckStatus::Invalid;
                    message = format!("文件类型不匹配：要求 {expected_type}，记录为 {actual}");
                }
            }

            let resolved_path = match output_file_path(paths, workspace_id, &artifact.path) {
                Ok(path) => Some(path),
                Err(error) => {
                    status = WorkArtifactCheckStatus::Invalid;
                    message = format!("文件路径不合法：{error}");
                    None
                }
            };

            if matches!(status, WorkArtifactCheckStatus::Satisfied) {
                if artifact.status != WorkArtifactStatus::Delivered {
                    status = WorkArtifactCheckStatus::Invalid;
                    message = format!("文件尚未完成交付（当前状态 {:?}）", artifact.status);
                } else if let Some(path) = resolved_path.as_deref() {
                    match fs::symlink_metadata(path) {
                        Ok(metadata)
                            if metadata.is_file() && !metadata.file_type().is_symlink() =>
                        {
                            if metadata.len() == 0 {
                                status = WorkArtifactCheckStatus::Invalid;
                                message = "文件为空".to_string();
                            } else {
                                size = Some(metadata.len());
                                if let Err(error) =
                                    validate_artifact_file(path, &artifact.artifact_type)
                                {
                                    status = WorkArtifactCheckStatus::Invalid;
                                    message = format!("文件验证失败：{error}");
                                } else if let Some(ref expected_sha) = artifact.sha256 {
                                    match compute_file_sha256(path) {
                                        Ok(actual_sha) if actual_sha != *expected_sha => {
                                            status = WorkArtifactCheckStatus::Invalid;
                                            message = "文件自上次验证后被修改（SHA-256 哈希不一致），原证据已失效，需重新验证".to_string();
                                        }
                                        Err(err) => {
                                            status = WorkArtifactCheckStatus::Invalid;
                                            message = format!("计算文件哈希失败：{err}");
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        Ok(_) => {
                            status = WorkArtifactCheckStatus::Invalid;
                            message = "交付物不是普通文件或是符号链接".to_string();
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            status = WorkArtifactCheckStatus::Missing;
                            message = "文件不存在".to_string();
                        }
                        Err(error) => {
                            status = WorkArtifactCheckStatus::Invalid;
                            message = format!("无法检查文件：{error}");
                        }
                    }
                }
            }

            WorkArtifactCheck {
                requirement: requirement.clone(),
                status,
                artifact_id: Some(artifact.id.clone()),
                resolved_path: Some(artifact.path.clone()),
                message,
                size,
            }
        } else {
            WorkArtifactCheck {
                requirement: requirement.clone(),
                status: WorkArtifactCheckStatus::Missing,
                artifact_id: None,
                resolved_path: normalized_target,
                message: "当前 Run 没有登记这份交付物".to_string(),
                size: None,
            }
        };

        match check.status {
            WorkArtifactCheckStatus::Satisfied => {
                satisfied_count += usize::from(requirement.required)
            }
            WorkArtifactCheckStatus::Missing => missing_count += usize::from(requirement.required),
            WorkArtifactCheckStatus::Invalid => invalid_count += usize::from(requirement.required),
        }
        // Optional requirements are shown to the UI but do not block completion.
        if !requirement.required && matches!(check.status, WorkArtifactCheckStatus::Missing) {
            check.message = format!("可选交付物：{}", check.message);
        }
        checks.push(check);
    }

    Ok(WorkArtifactAcceptance {
        run_id: run_id.to_string(),
        checks,
        required_count,
        satisfied_count,
        missing_count,
        invalid_count,
        satisfied: missing_count == 0 && invalid_count == 0,
    })
}

fn normalize_relative_path(path: &str) -> Result<String, String> {
    let mut path = path.trim().replace('\\', "/");
    if path.is_empty() || path.contains("\0") {
        return Err("Artifact path must be a relative Workspace path".into());
    }
    if path.starts_with('/') {
        if let Some(idx) = path.rfind("/output/") {
            path = path[idx + 1..].to_string();
        } else if let Some(idx) = path.rfind("/scratch/") {
            path = path[idx + 1..].to_string();
        } else {
            return Err("Artifact path must be a relative Workspace path".into());
        }
    }
    let normalized = path.trim_start_matches("./");
    if normalized == "."
        || normalized == ".."
        || normalized.starts_with("../")
        || normalized
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("Artifact path cannot contain . or ..".into());
    }
    Ok(normalized.to_string())
}

fn normalize_label(value: &str, label: &str, max_chars: usize) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max_chars || value.chars().any(char::is_control)
    {
        return Err(format!(
            "{label} is empty, too long, or contains control characters"
        ));
    }
    Ok(value.to_string())
}

fn decode_office_base64(content_base64: &str) -> Result<Vec<u8>, String> {
    let payload = content_base64
        .split_once(',')
        .map(|(_, value)| value)
        .unwrap_or(content_base64)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|error| format!("Invalid Office document base64: {error}"))?;
    if bytes.is_empty() {
        return Err("Office document cannot be empty".into());
    }
    if bytes.len() > MAX_OFFICE_EDIT_BYTES {
        return Err(format!(
            "Office document is too large (maximum {} MB)",
            MAX_OFFICE_EDIT_BYTES / 1024 / 1024
        ));
    }
    Ok(bytes)
}

fn normalize_office_type(value: &str) -> Result<String, String> {
    let kind = value.trim().trim_start_matches('.').to_ascii_lowercase();
    if matches!(kind.as_str(), "docx" | "xlsx" | "pptx") {
        Ok(kind)
    } else {
        Err("Only DOCX, XLSX, and PPTX documents can be created or edited in Work".into())
    }
}

fn write_validated_office_bytes(
    path: &Path,
    artifact_type: &str,
    bytes: &[u8],
) -> Result<u64, String> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            return Err("Refusing to overwrite a symbolic link".into());
        }
        if !metadata.is_file() {
            return Err("Office Artifact target is not a file".into());
        }
    }
    let parent = path
        .parent()
        .ok_or_else(|| "Office Artifact target has no parent directory".to_string())?;
    let temporary = parent.join(format!(
        ".{}.office-edit-{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("artifact"),
        Uuid::new_v4()
    ));
    crate::work::models::atomic_replace_file(&temporary, bytes)?;
    let validation = validate_artifact_file(&temporary, artifact_type);
    if let Err(error) = validation {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Office document validation failed: {error}"));
    }
    let result = crate::work::models::atomic_replace_file(path, bytes)
        .and_then(|_| validate_artifact_file(path, artifact_type));
    let _ = fs::remove_file(&temporary);
    result
}

/// Normalized Artifact Hub category derived from the registered
/// `artifact_type` (extension) with a path-extension fallback.
pub fn artifact_category(artifact_type: &str, path: &str) -> WorkArtifactCategory {
    let ext = artifact_type
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let ext = match ext.as_str() {
        "" | "file" => path
            .rsplit('.')
            .next()
            .filter(|value| value.len() <= 8 && *value != path)
            .unwrap_or("file")
            .to_ascii_lowercase(),
        other => other.to_string(),
    };
    match ext.as_str() {
        "pdf" => WorkArtifactCategory::Pdf,
        "doc" | "docx" | "docm" | "odt" | "rtf" | "pages" | "txt" | "md" | "markdown" => {
            WorkArtifactCategory::Document
        }
        "xls" | "xlsx" | "xlsm" | "csv" | "tsv" | "ods" | "numbers" => {
            WorkArtifactCategory::Spreadsheet
        }
        "ppt" | "pptx" | "pptm" | "key" | "odp" => WorkArtifactCategory::Presentation,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "tiff" | "tif" | "heic"
        | "ico" => WorkArtifactCategory::Image,
        "html" | "htm" => WorkArtifactCategory::Html,
        "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "py" | "rs" | "go" | "java" | "kt" | "c"
        | "h" | "cpp" | "hpp" | "cs" | "rb" | "php" | "swift" | "sh" | "bash" | "zsh" | "sql"
        | "json" | "yaml" | "yml" | "toml" | "xml" | "css" | "scss" | "vue" | "svelte"
        | "ipynb" => WorkArtifactCategory::Code,
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" => WorkArtifactCategory::Archive,
        _ => WorkArtifactCategory::Other,
    }
}

/// Best-effort MIME type derived from the extension. Kept deliberately small:
/// the registry never trusts file contents, and consumers fall back to
/// `application/octet-stream` semantics for unknown types.
pub fn artifact_mime_type(artifact_type: &str, path: &str) -> String {
    let ext = artifact_type
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let ext = match ext.as_str() {
        "" | "file" => {
            let tail = path.rsplit('.').next().unwrap_or("");
            if tail.len() <= 8 && tail != path {
                tail.to_ascii_lowercase()
            } else {
                "bin".to_string()
            }
        }
        other => other.to_string(),
    };
    let mime = match ext.as_str() {
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "tiff" | "tif" => "image/tiff",
        "ico" => "image/x-icon",
        "html" | "htm" => "text/html",
        "md" | "markdown" => "text/markdown",
        "txt" | "log" => "text/plain",
        "csv" => "text/csv",
        "json" => "application/json",
        "yaml" | "yml" => "application/yaml",
        "xml" => "application/xml",
        "js" | "mjs" | "cjs" => "text/javascript",
        "ts" => "text/typescript",
        "py" => "text/x-python",
        "sh" | "bash" | "zsh" => "text/x-shellscript",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "ogg" => "video/ogg",
        "mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    };
    mime.to_string()
}

/// Unified preview descriptor: which frontend renderer applies and whether a
/// preview is possible at all. Mirrors ArtifactPreviewModal's dispatch so the
/// Hub and the panel share one authority instead of two extension tables.
pub fn artifact_preview(artifact_type: &str, path: &str) -> (WorkArtifactPreviewKind, bool) {
    let normalized_path = path.to_ascii_lowercase();
    let name = normalized_path
        .rsplit('/')
        .next()
        .unwrap_or(normalized_path.as_str());
    // Special filenames that render as plain text/code regardless of suffix.
    if matches!(name, "dockerfile" | "makefile" | "agents.md" | "readme.md") {
        return (WorkArtifactPreviewKind::Markdown, true);
    }
    let ext = artifact_type
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let ext = match ext.as_str() {
        "" | "file" => name.rsplit('.').next().filter(|v| *v != name).unwrap_or(""),
        other => other,
    };
    let kind = match ext {
        "pdf" => WorkArtifactPreviewKind::Pdf,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "tiff" | "tif" | "ico" => {
            WorkArtifactPreviewKind::Image
        }
        "mp4" | "webm" | "mov" | "mkv" | "ogg" => WorkArtifactPreviewKind::Video,
        "md" | "markdown" => WorkArtifactPreviewKind::Markdown,
        "html" | "htm" => WorkArtifactPreviewKind::Html,
        "csv" | "tsv" => WorkArtifactPreviewKind::Csv,
        "docx" | "xlsx" | "pptx" => WorkArtifactPreviewKind::Office,
        "mmd" | "mermaid" => WorkArtifactPreviewKind::Mermaid,
        "txt" | "log" | "json" | "yaml" | "yml" | "toml" | "xml" | "js" | "mjs" | "cjs" | "ts"
        | "tsx" | "jsx" | "py" | "rs" | "go" | "java" | "c" | "h" | "cpp" | "sh" | "bash"
        | "sql" | "css" | "scss" | "vue" | "svelte" | "ipynb" => WorkArtifactPreviewKind::Code,
        _ => WorkArtifactPreviewKind::Binary,
    };
    let can_preview = kind != WorkArtifactPreviewKind::Binary;
    (kind, can_preview)
}

const VALIDATION_SUMMARY_PASSED: &str = "格式有效 · SHA-256 已计算 · 路径受限";

fn to_summary(artifact: StoredArtifact) -> WorkArtifactSummary {
    let category = artifact_category(&artifact.artifact_type, &artifact.path);
    let mime_type = artifact_mime_type(&artifact.artifact_type, &artifact.path);
    let (preview_kind, can_preview) = artifact_preview(&artifact.artifact_type, &artifact.path);
    let validation_summary = artifact.validation_summary.or_else(|| {
        artifact.evidence.as_ref().map(|_| {
            if artifact.status == WorkArtifactStatus::Invalid {
                "验证未通过".to_string()
            } else {
                VALIDATION_SUMMARY_PASSED.to_string()
            }
        })
    });
    WorkArtifactSummary {
        id: artifact.id,
        workspace_id: artifact.workspace_id,
        run_id: artifact.run_id,
        artifact_type: artifact.artifact_type,
        category,
        mime_type,
        title: artifact.title,
        path: artifact.path,
        status: artifact.status,
        size: artifact.size,
        sha256: artifact.sha256,
        producer: artifact.producer,
        evidence: artifact.evidence,
        validation_summary,
        version: if artifact.version == 0 {
            1
        } else {
            artifact.version
        },
        preview_kind,
        can_preview,
        sources: artifact.sources,
        created_at: artifact.created_at,
        updated_at: artifact.updated_at,
    }
}

// ============================================================================
// Standalone (workspace-less) artifact operations
//
// Standalone tasks store deliverables under `standalone_tasks/<run_id>/output/`
// with a per-run `artifacts.json` registry. The registry reuses the same
// `ArtifactRegistry` / `StoredArtifact` types; the `workspace_id` field in
// each entry holds the run id to keep the struct layout stable.
// ============================================================================

fn standalone_registry_path(paths: &WorkPaths, run_id: &str) -> Result<PathBuf, String> {
    Ok(paths.standalone_task_dir(run_id)?.join("artifacts.json"))
}

fn standalone_output_file_path(
    paths: &WorkPaths,
    run_id: &str,
    relative_path: &str,
) -> Result<PathBuf, String> {
    let relative = normalize_relative_path(relative_path)?;
    if !relative.starts_with("output/") {
        return Err("Work Artifacts must be inside output/".into());
    }
    let dir = paths.standalone_task_dir(run_id)?;
    let candidate = dir.join(&relative);
    crate::work::paths::ensure_within(&dir, &candidate)?;
    Ok(candidate)
}

fn read_standalone_registry(paths: &WorkPaths, run_id: &str) -> Result<ArtifactRegistry, String> {
    let path = standalone_registry_path(paths, run_id)?;
    if !path.exists() {
        return Ok(ArtifactRegistry::default());
    }
    let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("Standalone Artifact registry must not be a symbolic link".into());
    }
    if metadata.len() > MAX_REGISTRY_BYTES {
        return Err("Standalone Artifact registry is too large".into());
    }
    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    serde_json::from_str(&content).map_err(|error| format!("Invalid Artifact registry: {error}"))
}

fn write_standalone_registry(
    paths: &WorkPaths,
    run_id: &str,
    registry: &ArtifactRegistry,
) -> Result<(), String> {
    let path = standalone_registry_path(paths, run_id)?;
    if let Some(parent) = path.parent() {
        storage::ensure_dir(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(registry).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(&temporary, format!("{content}\n")).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(())
}

fn reconcile_standalone_output(
    paths: &WorkPaths,
    run_id: &str,
    registry: &mut ArtifactRegistry,
) -> Result<bool, String> {
    let output_root = paths.standalone_output_dir(run_id)?;
    if !output_root.is_dir() {
        return Ok(false);
    }
    let task_root = paths.standalone_task_dir(run_id)?;
    let mut files = Vec::new();
    collect_output_files(&output_root, &mut files, 1000);
    let mut changed = false;
    for file in files {
        let relative = file
            .strip_prefix(&task_root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let metadata = fs::metadata(&file).map_err(|error| error.to_string())?;
        if let Some(existing) = registry
            .artifacts
            .iter_mut()
            .find(|artifact| artifact.path == relative)
        {
            if existing.size != metadata.len() {
                existing.size = metadata.len();
                existing.updated_at = Utc::now().to_rfc3339();
                changed = true;
            }
            continue;
        }
        let now = Utc::now().to_rfc3339();
        registry.artifacts.push(StoredArtifact {
            id: Uuid::new_v4().to_string(),
            workspace_id: run_id.to_string(),
            run_id: Some(run_id.to_string()),
            artifact_type: file
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("file")
                .to_string(),
            title: file
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Artifact")
                .to_string(),
            path: relative,
            status: WorkArtifactStatus::Ready,
            size: metadata.len(),
            sha256: None,
            producer: None,
            evidence: None,
            sources: Vec::new(),
            version: 0,
            validation_summary: None,
            created_at: now.clone(),
            updated_at: now,
        });
        changed = true;
    }
    Ok(changed)
}

pub fn list_standalone(run_id: &str) -> Result<Vec<WorkArtifactSummary>, String> {
    list_standalone_with_paths(&WorkPaths::app(), run_id)
}

pub fn list_standalone_with_paths(
    paths: &WorkPaths,
    run_id: &str,
) -> Result<Vec<WorkArtifactSummary>, String> {
    let mut registry = read_standalone_registry(paths, run_id)?;
    if reconcile_standalone_output(paths, run_id, &mut registry)? {
        write_standalone_registry(paths, run_id, &registry)?;
    }
    Ok(registry
        .artifacts
        .into_iter()
        .filter_map(|artifact| sanitize_standalone_artifact(paths, run_id, artifact).ok())
        .map(to_summary)
        .collect())
}

pub fn register_standalone(
    run_id: &str,
    path: &str,
    title: &str,
    artifact_type: Option<&str>,
) -> Result<WorkArtifactSummary, String> {
    register_standalone_with_paths(&WorkPaths::app(), run_id, path, title, artifact_type)
}

pub fn register_standalone_with_paths(
    paths: &WorkPaths,
    run_id: &str,
    path: &str,
    title: &str,
    artifact_type: Option<&str>,
) -> Result<WorkArtifactSummary, String> {
    let full_path = standalone_output_file_path(paths, run_id, path)?;
    let metadata = fs::symlink_metadata(&full_path)
        .map_err(|error| format!("Artifact file not found at {path}: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("Artifact path is a symbolic link".into());
    }
    let kind = artifact_type
        .map(|value| value.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            full_path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("file")
                .to_string()
        });
    let kind = normalize_label(&kind, "Artifact type", 40)?;
    let title = normalize_label(title, "Artifact title", 160)?;
    let size = validate_artifact_file(&full_path, &kind)
        .map_err(|error| format!("Artifact validation failed: {error}"))?;
    let sha256 = compute_file_sha256(&full_path)?;
    let now = Utc::now().to_rfc3339();
    let evidence = ArtifactVerificationEvidence {
        verified_at: now.clone(),
        validator_version: "v1.0.0".to_string(),
        sha256: sha256.clone(),
        size,
        checks: vec![
            "format_valid".to_string(),
            "hash_computed".to_string(),
            "path_confined".to_string(),
        ],
        verification_status: WorkArtifactStatus::Delivered,
    };
    let artifact = StoredArtifact {
        id: Uuid::new_v4().to_string(),
        workspace_id: run_id.to_string(),
        run_id: Some(run_id.to_string()),
        artifact_type: kind,
        title,
        path: normalize_relative_path(path)?,
        status: WorkArtifactStatus::Delivered,
        size,
        sha256: Some(sha256),
        producer: Some(ArtifactProducer {
            run_id: run_id.to_string(),
            producer_tool_call_id: None,
            execution_id: None,
        }),
        evidence: Some(evidence),
        sources: Vec::new(),
        version: 0, // overwritten by the revision lookup below
        validation_summary: Some(VALIDATION_SUMMARY_PASSED.to_string()),
        created_at: now.clone(),
        updated_at: now,
    };
    let mut registry = read_standalone_registry(paths, run_id)?;
    // Preserve and bump the content revision across re-registration.
    let next_version = registry
        .artifacts
        .iter()
        .find(|existing| existing.path == artifact.path)
        .map(|existing| {
            let content_changed = existing.sha256 != artifact.sha256;
            if content_changed && existing.sha256.is_some() {
                existing.version.max(1) + 1
            } else {
                existing.version.max(1)
            }
        })
        .unwrap_or(1);
    let mut artifact = artifact;
    artifact.version = next_version;
    registry
        .artifacts
        .retain(|existing| existing.path != artifact.path);
    let result = artifact.clone();
    registry.artifacts.push(artifact);
    write_standalone_registry(paths, run_id, &registry)?;
    Ok(to_summary(result))
}

pub fn update_standalone_office_base64(
    run_id: &str,
    artifact_id: &str,
    content_base64: &str,
) -> Result<WorkArtifactSummary, String> {
    update_standalone_office_base64_with_paths(
        &WorkPaths::app(),
        run_id,
        artifact_id,
        content_base64,
    )
}

pub fn update_standalone_office_base64_with_paths(
    paths: &WorkPaths,
    run_id: &str,
    artifact_id: &str,
    content_base64: &str,
) -> Result<WorkArtifactSummary, String> {
    let bytes = decode_office_base64(content_base64)?;
    let (relative_path, artifact_type) = {
        let mut registry = read_standalone_registry(paths, run_id)?;
        let artifact = find_standalone_artifact_mut(&mut registry, run_id, artifact_id)?;
        let artifact_type = normalize_office_type(&artifact.artifact_type)?;
        (artifact.path.clone(), artifact_type)
    };
    let path = standalone_output_file_path(paths, run_id, &relative_path)?;
    let size = write_validated_office_bytes(&path, &artifact_type, &bytes)?;
    let mut registry = read_standalone_registry(paths, run_id)?;
    let artifact = find_standalone_artifact_mut(&mut registry, run_id, artifact_id)?;
    artifact.status = WorkArtifactStatus::Delivered;
    // Office editing replaces the document bytes: always a new revision.
    artifact.version = artifact.version.max(1) + 1;
    artifact.size = size;
    artifact.validation_summary = Some(VALIDATION_SUMMARY_PASSED.to_string());
    artifact.updated_at = Utc::now().to_rfc3339();
    let result = artifact.clone();
    write_standalone_registry(paths, run_id, &registry)?;
    Ok(to_summary(result))
}

pub fn create_standalone_office_base64(
    run_id: &str,
    relative_path: &str,
    title: &str,
    artifact_type: &str,
    content_base64: &str,
) -> Result<WorkArtifactSummary, String> {
    create_standalone_office_base64_with_paths(
        &WorkPaths::app(),
        run_id,
        relative_path,
        title,
        artifact_type,
        content_base64,
    )
}

pub fn create_standalone_office_base64_with_paths(
    paths: &WorkPaths,
    run_id: &str,
    relative_path: &str,
    title: &str,
    artifact_type: &str,
    content_base64: &str,
) -> Result<WorkArtifactSummary, String> {
    let bytes = decode_office_base64(content_base64)?;
    let artifact_type = normalize_office_type(artifact_type)?;
    let title = normalize_label(title, "Artifact title", 160)?;
    let path = standalone_output_file_path(paths, run_id, relative_path)?;
    if path.exists() {
        return Err("Office Artifact already exists".into());
    }
    write_validated_office_bytes(&path, &artifact_type, &bytes)?;
    let registered =
        register_standalone_with_paths(paths, run_id, relative_path, &title, Some(&artifact_type))?;
    let mut registry = read_standalone_registry(paths, run_id)?;
    let artifact = find_standalone_artifact_mut(&mut registry, run_id, &registered.id)?;
    artifact.status = WorkArtifactStatus::Delivered;
    artifact.updated_at = Utc::now().to_rfc3339();
    artifact.size = bytes.len() as u64;
    let result = artifact.clone();
    write_standalone_registry(paths, run_id, &registry)?;
    Ok(to_summary(result))
}

pub fn validate_standalone(run_id: &str, artifact_id: &str) -> Result<WorkArtifactSummary, String> {
    let paths = WorkPaths::app();
    let mut registry = read_standalone_registry(&paths, run_id)?;
    let artifact = find_standalone_artifact_mut(&mut registry, run_id, artifact_id)?;
    let path = standalone_output_file_path(&paths, run_id, &artifact.path)?;
    let artifact_type = artifact.artifact_type.clone();
    let was_delivered = artifact.status == WorkArtifactStatus::Delivered;
    let size = match validate_artifact_file(&path, &artifact_type) {
        Ok(size) => size,
        Err(error) => {
            artifact.status = WorkArtifactStatus::Invalid;
            artifact.validation_summary = Some(format!("验证失败：{error}"));
            artifact.updated_at = Utc::now().to_rfc3339();
            let _ = write_standalone_registry(&paths, run_id, &registry);
            return Err(format!("Artifact validation failed: {error}"));
        }
    };
    artifact.status = if was_delivered {
        WorkArtifactStatus::Delivered
    } else {
        WorkArtifactStatus::Validated
    };
    artifact.size = size;
    artifact.validation_summary = Some(VALIDATION_SUMMARY_PASSED.to_string());
    artifact.updated_at = Utc::now().to_rfc3339();
    let result = artifact.clone();
    write_standalone_registry(&paths, run_id, &registry)?;
    Ok(to_summary(result))
}

pub fn deliver_standalone(run_id: &str, artifact_id: &str) -> Result<WorkArtifactSummary, String> {
    let paths = WorkPaths::app();
    let mut registry = read_standalone_registry(&paths, run_id)?;
    let artifact = find_standalone_artifact_mut(&mut registry, run_id, artifact_id)?;
    let path = standalone_output_file_path(&paths, run_id, &artifact.path)?;
    let artifact_type = artifact.artifact_type.clone();
    let size = match validate_artifact_file(&path, &artifact_type) {
        Ok(size) => size,
        Err(error) => {
            artifact.status = WorkArtifactStatus::Invalid;
            artifact.validation_summary = Some(format!("验证失败：{error}"));
            artifact.updated_at = Utc::now().to_rfc3339();
            let _ = write_standalone_registry(&paths, run_id, &registry);
            return Err(format!("Artifact validation failed: {error}"));
        }
    };
    if !matches!(
        artifact.status,
        WorkArtifactStatus::Validated | WorkArtifactStatus::Delivered
    ) {
        return Err("Artifact must be validated before delivery".into());
    }
    artifact.status = WorkArtifactStatus::Delivered;
    artifact.size = size;
    artifact.validation_summary = Some(VALIDATION_SUMMARY_PASSED.to_string());
    artifact.updated_at = Utc::now().to_rfc3339();
    let result = artifact.clone();
    write_standalone_registry(&paths, run_id, &registry)?;
    Ok(to_summary(result))
}

pub fn delete_standalone(run_id: &str, artifact_id: &str) -> Result<(), String> {
    let paths = WorkPaths::app();
    let mut registry = read_standalone_registry(&paths, run_id)?;
    let index = registry
        .artifacts
        .iter()
        .position(|artifact| artifact.id == artifact_id)
        .ok_or_else(|| format!("Work Artifact '{artifact_id}' not found"))?;
    let path = registry.artifacts[index].path.clone();
    let still_referenced = registry
        .artifacts
        .iter()
        .any(|artifact| artifact.id != artifact_id && artifact.path == path);
    if still_referenced {
        registry.artifacts.remove(index);
        write_standalone_registry(&paths, run_id, &registry)?;
        return Ok(());
    }
    let file_path = standalone_output_file_path(&paths, run_id, &path)?;
    match fs::symlink_metadata(&file_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err("Refusing to remove a symbolic link".into());
            }
            if metadata.is_file() {
                fs::remove_file(&file_path)
                    .map_err(|error| format!("Failed to remove Artifact file: {error}"))?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Cannot inspect Artifact file: {error}")),
    }
    registry.artifacts.remove(index);
    write_standalone_registry(&paths, run_id, &registry)?;
    Ok(())
}

pub fn export_standalone(
    run_id: &str,
    artifact_id: &str,
    destination_path: &str,
) -> Result<String, String> {
    let paths = WorkPaths::app();
    let registry = read_standalone_registry(&paths, run_id)?;
    let artifact = registry
        .artifacts
        .iter()
        .find(|artifact| artifact.id == artifact_id)
        .ok_or_else(|| format!("Work Artifact '{artifact_id}' not found"))?;
    if artifact.status != WorkArtifactStatus::Delivered {
        return Err("Artifact must be delivered before export".into());
    }
    let source_path = standalone_output_file_path(&paths, run_id, &artifact.path)?;
    validate_artifact_file(&source_path, &artifact.artifact_type)
        .map_err(|error| format!("Artifact source is not valid: {error}"))?;
    let source = fs::canonicalize(&source_path)
        .map_err(|error| format!("Artifact source is not readable: {error}"))?;

    let destination = PathBuf::from(destination_path.trim());
    if !destination.is_absolute() {
        return Err("Export destination must be an absolute path".into());
    }
    let file_name = destination
        .file_name()
        .ok_or_else(|| "Export destination must include a file name".to_string())?;
    let parent = destination
        .parent()
        .ok_or_else(|| "Export destination has no parent directory".to_string())?;
    let parent_metadata = fs::metadata(parent)
        .map_err(|error| format!("Export destination directory is not available: {error}"))?;
    if !parent_metadata.is_dir() {
        return Err("Export destination parent is not a directory".into());
    }
    let canonical_parent = fs::canonicalize(parent).map_err(|error| error.to_string())?;
    let target = canonical_parent.join(file_name);
    if let Ok(target_metadata) = fs::symlink_metadata(&target) {
        if target_metadata.file_type().is_symlink() {
            return Err("Refusing to overwrite a symbolic link".into());
        }
    }
    fs::copy(&source, &target)
        .map_err(|error| format!("Failed to copy Artifact to destination: {error}"))?;
    let canonical_target = fs::canonicalize(&target)
        .map_err(|error| format!("Failed to resolve exported path: {error}"))?;
    Ok(canonical_target.to_string_lossy().to_string())
}

/// Copy a completed standalone run into a Workspace without deleting the
/// original run directory. Keeping the source makes the operation retryable
/// if a later metadata write fails, while the Workspace registry becomes the
/// authoritative view after assignment.
pub fn migrate_standalone_to_workspace(run_id: &str, workspace_id: &str) -> Result<usize, String> {
    workspace::manager().get(workspace_id)?;
    migrate_standalone_to_workspace_with_paths(&WorkPaths::app(), run_id, workspace_id)
}

pub fn migrate_standalone_to_workspace_with_paths(
    paths: &WorkPaths,
    run_id: &str,
    workspace_id: &str,
) -> Result<usize, String> {
    let standalone_root = paths.standalone_task_dir(run_id)?;
    let workspace_root = paths.workspace_dir(workspace_id)?;
    if !workspace_root.is_dir() {
        return Err("Target Workspace directory does not exist".into());
    }
    ensure_directory_is_safe(&standalone_root, "Standalone task root", false)?;
    ensure_directory_is_safe(&standalone_root.join("output"), "Standalone output", true)?;
    ensure_directory_is_safe(&standalone_root.join("scratch"), "Standalone scratch", true)?;
    ensure_directory_is_safe(&workspace_root, "Workspace root", false)?;
    ensure_directory_is_safe(&workspace_root.join("output"), "Workspace output", false)?;
    ensure_directory_is_safe(&workspace_root.join("scratch"), "Workspace scratch", false)?;

    let mut source_registry = read_standalone_registry(paths, run_id)?;
    if reconcile_standalone_output(paths, run_id, &mut source_registry)? {
        write_standalone_registry(paths, run_id, &source_registry)?;
    }
    let mut target_registry = read_registry(paths, workspace_id)?;
    let mut target_paths_by_source = HashMap::<String, String>::new();
    let mut planned = Vec::new();

    // Preflight every registered output before copying anything. A missing
    // source should be visible to the user instead of producing a partially
    // migrated Workspace that looks complete.
    for artifact in &source_registry.artifacts {
        let source_path = standalone_output_file_path(paths, run_id, &artifact.path)?;
        let metadata = fs::symlink_metadata(&source_path)
            .map_err(|error| format!("Standalone Artifact source is unavailable: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "Standalone Artifact source is not a regular file: {}",
                artifact.path
            ));
        }

        if let Some(existing) = target_registry.artifacts.iter().find(|candidate| {
            candidate.id == artifact.id && candidate.run_id.as_deref() == Some(run_id)
        }) {
            let existing_path = output_file_path(paths, workspace_id, &existing.path)?;
            if !existing_path.is_file() {
                return Err(format!(
                    "Workspace Artifact {} is missing its copied file",
                    existing.id
                ));
            }
            planned.push((artifact.clone(), existing.path.clone(), source_path));
            continue;
        }

        let target_path = if let Some(path) = target_paths_by_source.get(&artifact.path) {
            path.clone()
        } else {
            let path = choose_migrated_artifact_path(
                paths,
                workspace_id,
                run_id,
                &artifact.path,
                &target_registry,
            )?;
            target_paths_by_source.insert(artifact.path.clone(), path.clone());
            path
        };
        planned.push((artifact.clone(), target_path, source_path));
    }

    for (_, target_relative, source_path) in &planned {
        let target_path = output_file_path(paths, workspace_id, target_relative)?;
        copy_regular_file_without_overwrite(source_path, &target_path)?;
    }

    // Scratch notes are not registered Artifacts, but copying them preserves
    // the user's working context when the session is reopened in its Workspace.
    copy_directory_contents(
        &standalone_root.join("scratch"),
        &workspace_root.join("scratch"),
        run_id,
    )?;

    let mut migrated_count = 0;
    for (source, target_relative, _) in planned {
        let mut migrated = source;
        migrated.workspace_id = workspace_id.to_string();
        migrated.run_id = Some(run_id.to_string());
        migrated.path = target_relative;
        if target_registry
            .artifacts
            .iter()
            .any(|candidate| candidate.id == migrated.id)
            && !target_registry.artifacts.iter().any(|candidate| {
                candidate.id == migrated.id && candidate.run_id.as_deref() == Some(run_id)
            })
        {
            migrated.id = Uuid::new_v4().to_string();
        }

        if let Some(index) = target_registry.artifacts.iter().position(|candidate| {
            candidate.id == migrated.id && candidate.run_id.as_deref() == Some(run_id)
        }) {
            target_registry.artifacts[index] = migrated;
        } else {
            target_registry.artifacts.push(migrated);
        }
        migrated_count += 1;
    }
    if migrated_count > 0 {
        write_registry(paths, workspace_id, &target_registry)?;
    }
    Ok(migrated_count)
}

fn choose_migrated_artifact_path(
    paths: &WorkPaths,
    workspace_id: &str,
    run_id: &str,
    preferred: &str,
    registry: &ArtifactRegistry,
) -> Result<String, String> {
    let preferred = normalize_relative_path(preferred)?;
    let occupied = |relative: &str| {
        registry
            .artifacts
            .iter()
            .any(|artifact| artifact.path == relative)
            || output_file_path(paths, workspace_id, relative)
                .map(|path| path.exists())
                .unwrap_or(true)
    };
    if !occupied(&preferred) {
        return Ok(preferred);
    }

    let suffix = preferred
        .strip_prefix("output/")
        .unwrap_or(preferred.as_str());
    for attempt in 0..10_000usize {
        let base = format!("output/imported/{run_id}/{suffix}");
        let candidate = if attempt == 0 {
            base
        } else {
            let path = Path::new(&base);
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("artifact");
            let extension = path.extension().and_then(|value| value.to_str());
            let file_name = extension
                .map(|ext| format!("{stem}-{attempt}.{ext}"))
                .unwrap_or_else(|| format!("{stem}-{attempt}"));
            let parent = path
                .parent()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("{parent}/{file_name}")
        };
        if !occupied(&candidate) {
            return Ok(candidate);
        }
    }
    Err("Could not allocate a collision-free Workspace Artifact path".into())
}

fn copy_regular_file_without_overwrite(source: &Path, target: &Path) -> Result<(), String> {
    let source_metadata = fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
        return Err(format!(
            "Refusing to migrate non-file source: {}",
            source.display()
        ));
    }
    if let Ok(target_metadata) = fs::symlink_metadata(target) {
        if target_metadata.file_type().is_symlink() {
            return Err(format!(
                "Refusing to overwrite a symbolic link: {}",
                target.display()
            ));
        }
        if !target_metadata.is_file() {
            return Err(format!(
                "Migration target is not a file: {}",
                target.display()
            ));
        }
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::copy(source, target)
        .map_err(|error| format!("Failed to copy standalone Artifact: {error}"))?;
    Ok(())
}

fn copy_directory_contents(
    source_root: &Path,
    target_root: &Path,
    run_id: &str,
) -> Result<(), String> {
    if !source_root.is_dir() {
        return Ok(());
    }
    ensure_directory_is_safe(source_root, "Migration source directory", false)?;
    ensure_directory_is_safe(target_root, "Migration target directory", true)?;
    let mut files = Vec::new();
    collect_regular_files(source_root, &mut files)?;
    for source in files {
        let relative = source
            .strip_prefix(source_root)
            .map_err(|error| error.to_string())?;
        let preferred = target_root.join(relative);
        let target = if preferred.exists() {
            target_root.join("imported").join(run_id).join(relative)
        } else {
            preferred
        };
        copy_regular_file_without_overwrite(&source, &target)?;
    }
    Ok(())
}

fn ensure_directory_is_safe(path: &Path, label: &str, optional: bool) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if optional && error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("{label} is unavailable: {error}")),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!("{label} must not be a symbolic link"));
    }
    if !metadata.is_dir() {
        return Err(format!("{label} must be a directory"));
    }
    Ok(())
}

fn collect_regular_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(root).map_err(|error| error.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Refusing to migrate a symbolic link: {}",
                path.display()
            ));
        }
        if metadata.is_dir() {
            collect_regular_files(&path, files)?;
        } else if metadata.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn sanitize_standalone_artifact(
    paths: &WorkPaths,
    run_id: &str,
    artifact: StoredArtifact,
) -> Result<StoredArtifact, String> {
    let _ = standalone_output_file_path(paths, run_id, &artifact.path)?;
    Ok(artifact)
}

fn find_standalone_artifact_mut<'a>(
    registry: &'a mut ArtifactRegistry,
    run_id: &str,
    artifact_id: &str,
) -> Result<&'a mut StoredArtifact, String> {
    registry
        .artifacts
        .iter_mut()
        .find(|artifact| artifact.id == artifact_id && artifact.workspace_id == run_id)
        .ok_or_else(|| format!("Work Artifact '{artifact_id}' not found"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::paths::WorkPaths;
    use std::io::{Cursor, Write};
    use tempfile::TempDir;

    fn setup(temp: &TempDir) -> (WorkPaths, String) {
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let workspace_id = "demo".to_string();
        let root = paths.workspace_dir(&workspace_id).unwrap();
        for area in ["input", "scratch", "output", "context"] {
            fs::create_dir_all(root.join(area)).unwrap();
        }
        (paths, workspace_id)
    }

    fn minimal_office_bytes(extension: &str) -> Vec<u8> {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        archive.start_file("[Content_Types].xml", options).unwrap();
        archive.write_all(b"<Types/>").unwrap();
        let entry = match extension {
            "docx" => "word/document.xml",
            "xlsx" => "xl/workbook.xml",
            "pptx" => "ppt/presentation.xml",
            _ => panic!("unsupported test format"),
        };
        archive.start_file(entry, options).unwrap();
        archive.write_all(b"<document/>").unwrap();
        archive.finish().unwrap().into_inner()
    }

    #[test]
    fn rejects_artifacts_outside_output() {
        let temp = TempDir::new().unwrap();
        let (paths, workspace_id) = setup(&temp);
        assert!(output_file_path(&paths, &workspace_id, "scratch/draft.md").is_err());
        assert!(normalize_relative_path("output/../scratch/file").is_err());
    }

    #[test]
    fn hub_classification_mimetype_and_preview_are_derived() {
        assert_eq!(
            artifact_category("pptx", "output/Q3-review.pptx"),
            WorkArtifactCategory::Presentation
        );
        assert_eq!(
            artifact_category("xlsx", "output/analysis.xlsx"),
            WorkArtifactCategory::Spreadsheet
        );
        assert_eq!(
            artifact_category("pdf", "output/brief.pdf"),
            WorkArtifactCategory::Pdf
        );
        assert_eq!(
            artifact_category("md", "output/notes.md"),
            WorkArtifactCategory::Document
        );
        assert_eq!(
            artifact_category("png", "output/chart.png"),
            WorkArtifactCategory::Image
        );
        assert_eq!(
            artifact_category("html", "output/page.html"),
            WorkArtifactCategory::Html
        );
        assert_eq!(
            artifact_category("py", "output/build.py"),
            WorkArtifactCategory::Code
        );
        assert_eq!(
            artifact_category("zip", "output/bundle.zip"),
            WorkArtifactCategory::Archive
        );
        // Free-form extension-less entries fall back to the path extension.
        assert_eq!(
            artifact_category("file", "output/report.docx"),
            WorkArtifactCategory::Document
        );
        assert_eq!(
            artifact_category("weird", "output/data"),
            WorkArtifactCategory::Other
        );

        assert_eq!(
            artifact_mime_type("xlsx", "output/a.xlsx"),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
        assert_eq!(artifact_mime_type("md", "output/a.md"), "text/markdown");
        assert_eq!(
            artifact_mime_type("bin", "output/a.bin"),
            "application/octet-stream"
        );

        assert_eq!(
            artifact_preview("pdf", "output/a.pdf"),
            (WorkArtifactPreviewKind::Pdf, true)
        );
        assert_eq!(
            artifact_preview("pptx", "output/a.pptx"),
            (WorkArtifactPreviewKind::Office, true)
        );
        assert_eq!(
            artifact_preview("doc", "output/a.doc"),
            (WorkArtifactPreviewKind::Binary, false)
        );
        assert_eq!(
            artifact_preview("csv", "output/a.csv"),
            (WorkArtifactPreviewKind::Csv, true)
        );
    }

    #[test]
    fn re_registering_changed_content_bumps_version_and_summary() {
        let temp = TempDir::new().unwrap();
        let (paths, workspace_id) = setup(&temp);
        let output_dir = paths.workspace_dir(&workspace_id).unwrap().join("output");
        fs::create_dir_all(&output_dir).unwrap();
        let file = output_dir.join("report.md");
        fs::write(&file, "# v1").unwrap();

        let first = register_with_paths(
            &paths,
            &workspace_id,
            "output/report.md",
            "Report",
            None,
            None,
        )
        .unwrap();
        assert_eq!(first.version, 1);
        assert!(first.validation_summary.is_some());
        assert_eq!(first.category, WorkArtifactCategory::Document);
        assert_eq!(first.preview_kind, WorkArtifactPreviewKind::Markdown);
        assert!(first.can_preview);

        // Re-register the same bytes: version must stay stable.
        let same = register_with_paths(
            &paths,
            &workspace_id,
            "output/report.md",
            "Report",
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            same.version, 1,
            "identical content must not bump the version"
        );

        // Content change: version bumps once.
        fs::write(&file, "# v2 changed").unwrap();
        let second = register_with_paths(
            &paths,
            &workspace_id,
            "output/report.md",
            "Report",
            None,
            None,
        )
        .unwrap();
        assert_eq!(second.version, 2);
        assert_eq!(second.id, first.id, "same entry is updated, not duplicated");
    }

    #[test]
    fn legacy_registry_entries_get_derived_hub_fields() {
        let legacy_entry = StoredArtifact {
            id: "legacy-1".into(),
            workspace_id: "ws-1".into(),
            run_id: None,
            artifact_type: "md".into(),
            title: "Legacy".into(),
            path: "output/legacy.md".into(),
            status: WorkArtifactStatus::Delivered,
            size: 6,
            sha256: Some("old-hash".into()),
            producer: None,
            evidence: Some(ArtifactVerificationEvidence {
                verified_at: "2026-01-01T00:00:00Z".into(),
                validator_version: "v1.0.0".into(),
                sha256: "old-hash".into(),
                size: 6,
                checks: vec!["format_valid".to_string()],
                verification_status: WorkArtifactStatus::Delivered,
            }),
            sources: Vec::new(),
            version: 0,
            validation_summary: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };

        // Entries written before the Hub fields existed must still produce a
        // complete summary: version 0 displays as 1 and the validation summary
        // is derived from the durable evidence.
        let summary = to_summary(legacy_entry);
        assert_eq!(summary.version, 1);
        assert!(summary.validation_summary.is_some());
        assert_eq!(summary.mime_type, "text/markdown");
        assert_eq!(summary.category, WorkArtifactCategory::Document);
        assert!(summary.can_preview);

        // An invalid legacy entry derives a failing summary instead of None.
        let invalid = StoredArtifact {
            id: "legacy-2".into(),
            workspace_id: "ws-1".into(),
            run_id: None,
            artifact_type: "md".into(),
            title: "Broken".into(),
            path: "output/broken.md".into(),
            status: WorkArtifactStatus::Invalid,
            size: 0,
            sha256: None,
            producer: None,
            evidence: Some(ArtifactVerificationEvidence {
                verified_at: "2026-01-01T00:00:00Z".into(),
                validator_version: "v1.0.0".into(),
                sha256: "old-hash".into(),
                size: 6,
                checks: vec!["format_valid".to_string()],
                verification_status: WorkArtifactStatus::Invalid,
            }),
            sources: Vec::new(),
            version: 0,
            validation_summary: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        let invalid_summary = to_summary(invalid);
        assert_eq!(
            invalid_summary.validation_summary.as_deref(),
            Some("验证未通过")
        );
    }

    #[test]
    fn reads_and_writes_artifact_lifecycle_registry() {
        let temp = TempDir::new().unwrap();
        let (paths, workspace_id) = setup(&temp);
        let output = paths
            .workspace_dir(&workspace_id)
            .unwrap()
            .join("output/report.md");
        fs::write(&output, "report").unwrap();
        let mut registry = ArtifactRegistry {
            version: 1,
            artifacts: vec![StoredArtifact {
                id: "artifact-1".into(),
                workspace_id: workspace_id.clone(),
                run_id: None,
                artifact_type: "md".into(),
                title: "Report".into(),
                path: "output/report.md".into(),
                status: WorkArtifactStatus::Ready,
                size: 6,
                sha256: None,
                producer: None,
                evidence: None,
                sources: Vec::new(),
                version: 1,
                validation_summary: None,
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            }],
        };
        write_registry(&paths, &workspace_id, &registry).unwrap();
        let loaded = read_registry(&paths, &workspace_id).unwrap();
        assert_eq!(loaded.artifacts.len(), 1);
        let artifact = find_artifact_mut(&mut registry, &workspace_id, "artifact-1", None).unwrap();
        artifact.status = WorkArtifactStatus::Validated;
        assert_eq!(artifact.status, WorkArtifactStatus::Validated);
    }

    #[test]
    fn reads_pi_snake_case_nested_artifact_fields() {
        let temp = TempDir::new().unwrap();
        let (paths, workspace_id) = setup(&temp);
        let registry_path = paths
            .workspace_dir(&workspace_id)
            .unwrap()
            .join("context/artifacts.json");
        let legacy_registry = serde_json::json!({
            "version": 1,
            "artifacts": [{
                "id": "legacy-artifact",
                "workspace_id": workspace_id,
                "run_id": "run-1",
                "artifact_type": "md",
                "title": "Report",
                "path": "output/report.md",
                "status": "delivered",
                "size": 7,
                "producer": {
                    "run_id": "run-1",
                    "producer_tool_call_id": "tool-1",
                    "execution_id": "exec-1"
                },
                "evidence": {
                    "verified_at": "2026-01-01T00:00:00Z",
                    "validator_version": "v1.0.0",
                    "sha256": "digest",
                    "size": 7,
                    "checks": ["format_valid"],
                    "verification_status": "delivered"
                },
                "sources": [{
                    "source_tool_call_id": "source-1",
                    "source_type": "web_open",
                    "url": "https://example.com",
                    "result_digest": "source-digest",
                    "captured_at": "2026-01-01T00:00:00Z",
                    "is_claimed": false
                }],
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z"
            }]
        });
        fs::write(
            registry_path,
            serde_json::to_string_pretty(&legacy_registry).unwrap(),
        )
        .unwrap();

        let loaded = read_registry(&paths, &workspace_id).unwrap();
        let artifact = &loaded.artifacts[0];
        assert_eq!(artifact.producer.as_ref().unwrap().run_id, "run-1");
        assert_eq!(
            artifact
                .producer
                .as_ref()
                .unwrap()
                .producer_tool_call_id
                .as_deref(),
            Some("tool-1")
        );
        assert_eq!(
            artifact.evidence.as_ref().unwrap().verification_status,
            WorkArtifactStatus::Delivered
        );
        assert_eq!(artifact.sources[0].source_tool_call_id, "source-1");
    }

    #[test]
    fn discovers_unregistered_output_files_as_ready_artifacts() {
        let temp = TempDir::new().unwrap();
        let (paths, workspace_id) = setup(&temp);
        let output = paths.workspace_dir(&workspace_id).unwrap().join("output");
        fs::write(output.join("report.md"), "# report\n").unwrap();
        let mut registry = ArtifactRegistry::default();
        assert!(reconcile_output_artifacts(&paths, &workspace_id, &mut registry).unwrap());
        assert_eq!(registry.artifacts.len(), 1);
        assert_eq!(registry.artifacts[0].path, "output/report.md");
        assert_eq!(registry.artifacts[0].status, WorkArtifactStatus::Ready);
        assert_eq!(registry.artifacts[0].run_id, None);
    }

    #[test]
    fn rejects_corrupt_office_packages_and_delivers_valid_minimal_pptx() {
        let temp = TempDir::new().unwrap();
        let (paths, workspace_id) = setup(&temp);
        let output = paths.workspace_dir(&workspace_id).unwrap().join("output");
        let pptx = output.join("report.pptx");
        fs::write(&pptx, b"this is not a presentation").unwrap();

        let registry = ArtifactRegistry {
            version: 1,
            artifacts: vec![StoredArtifact {
                id: "pptx-1".into(),
                workspace_id: workspace_id.clone(),
                run_id: None,
                artifact_type: "pptx".into(),
                title: "Report".into(),
                path: "output/report.pptx".into(),
                status: WorkArtifactStatus::Ready,
                size: 0,
                sha256: None,
                producer: None,
                evidence: None,
                sources: Vec::new(),
                version: 1,
                validation_summary: None,
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            }],
        };
        write_registry(&paths, &workspace_id, &registry).unwrap();

        let invalid = validate_with_paths(&paths, &workspace_id, "pptx-1", None).unwrap();
        assert_eq!(invalid.status, WorkArtifactStatus::Invalid);

        let file = fs::File::create(&pptx).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        archive.start_file("[Content_Types].xml", options).unwrap();
        archive.write_all(b"<Types/>").unwrap();
        archive.start_file("ppt/presentation.xml", options).unwrap();
        archive.write_all(b"<presentation/>").unwrap();
        archive.finish().unwrap();

        let validated = validate_with_paths(&paths, &workspace_id, "pptx-1", None).unwrap();
        assert_eq!(validated.status, WorkArtifactStatus::Validated);
        let delivered = deliver_with_paths(&paths, &workspace_id, "pptx-1", None).unwrap();
        assert_eq!(delivered.status, WorkArtifactStatus::Delivered);
    }

    #[test]
    fn creates_and_updates_workspace_office_artifact_atomically() {
        let temp = TempDir::new().unwrap();
        let (paths, workspace_id) = setup(&temp);
        let original = minimal_office_bytes("docx");
        let updated = minimal_office_bytes("docx");
        let encoded = base64::engine::general_purpose::STANDARD.encode(&original);
        let created = create_office_base64_with_paths(
            &paths,
            &workspace_id,
            "output/report.docx",
            "Report",
            "docx",
            &encoded,
            Some("run-1"),
        )
        .unwrap();
        assert_eq!(created.status, WorkArtifactStatus::Delivered);
        assert_eq!(created.run_id.as_deref(), Some("run-1"));

        let updated_summary = update_office_base64_with_paths(
            &paths,
            &workspace_id,
            &created.id,
            &base64::engine::general_purpose::STANDARD.encode(&updated),
            Some("run-1"),
        )
        .unwrap();
        assert_eq!(updated_summary.status, WorkArtifactStatus::Delivered);
        let path = output_file_path(&paths, &workspace_id, "output/report.docx").unwrap();
        assert_eq!(fs::read(path).unwrap(), updated);
    }

    #[test]
    fn standalone_office_mutations_are_scoped_and_validate_packages() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        paths.ensure_layout().unwrap();
        let run_id = "standalone-run";
        paths.ensure_standalone_task_dir(run_id).unwrap();
        let bytes = minimal_office_bytes("xlsx");
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let created = create_standalone_office_base64_with_paths(
            &paths,
            run_id,
            "output/budget.xlsx",
            "Budget",
            "xlsx",
            &encoded,
        )
        .unwrap();
        assert_eq!(created.status, WorkArtifactStatus::Delivered);
        assert!(update_standalone_office_base64_with_paths(
            &paths,
            run_id,
            &created.id,
            "not-base64",
        )
        .is_err());
        assert!(create_standalone_office_base64_with_paths(
            &paths,
            run_id,
            "scratch/out.xlsx",
            "Escape",
            "xlsx",
            &encoded,
        )
        .is_err());
    }

    #[test]
    fn register_claims_unattributed_reconciled_artifact() {
        let temp = TempDir::new().unwrap();
        let (paths, workspace_id) = setup(&temp);
        let output = paths.workspace_dir(&workspace_id).unwrap().join("output");
        fs::write(output.join("joined.json"), "{}").unwrap();
        let mut registry = ArtifactRegistry::default();
        assert!(reconcile_output_artifacts(&paths, &workspace_id, &mut registry).unwrap());
        write_registry(&paths, &workspace_id, &registry).unwrap();

        let registered = register_with_paths(
            &paths,
            &workspace_id,
            "output/joined.json",
            "Joined",
            None,
            Some("run-1"),
        )
        .unwrap();
        assert_eq!(registered.run_id.as_deref(), Some("run-1"));
        let registry = read_registry(&paths, &workspace_id).unwrap();
        assert_eq!(registry.artifacts.len(), 1);
        assert_eq!(registry.artifacts[0].run_id.as_deref(), Some("run-1"));
    }

    #[test]
    fn required_artifact_acceptance_is_run_scoped_and_validates_files() {
        let temp = TempDir::new().unwrap();
        let (paths, workspace_id) = setup(&temp);
        let task_manager = crate::work::tasks::TaskManager::new(paths.clone());
        let mut task = task_manager
            .create_task(&workspace_id, "Acceptance", "Create a report", None)
            .unwrap();
        task.artifact_requirements = vec![WorkArtifactRequirement {
            path: "output/report.md".to_string(),
            title: Some("Report".to_string()),
            artifact_type: Some("md".to_string()),
            required: true,
        }];
        task_manager.update_task(&mut task).unwrap();
        let run = task_manager
            .start_run(&task.id, None, crate::work::models::WorkRunTrigger::Manual)
            .unwrap();

        let missing =
            check_required_artifacts_with_paths(&paths, &workspace_id, &run.id, &task).unwrap();
        assert!(!missing.satisfied);
        assert_eq!(missing.missing_count, 1);

        let output = paths
            .workspace_dir(&workspace_id)
            .unwrap()
            .join("output/report.md");
        fs::write(&output, "# Report\n").unwrap();
        let registered = register_with_paths(
            &paths,
            &workspace_id,
            "output/report.md",
            "Report",
            Some("md"),
            Some(&run.id),
        )
        .unwrap();
        assert_eq!(registered.status, WorkArtifactStatus::Delivered);

        let accepted =
            check_required_artifacts_with_paths(&paths, &workspace_id, &run.id, &task).unwrap();
        assert!(accepted.satisfied);
        assert_eq!(accepted.satisfied_count, 1);

        fs::write(&output, "").unwrap();
        let invalid =
            check_required_artifacts_with_paths(&paths, &workspace_id, &run.id, &task).unwrap();
        assert!(!invalid.satisfied);
        assert_eq!(invalid.invalid_count, 1);
    }

    #[test]
    fn delete_removes_entry_and_unreferenced_file() {
        let temp = TempDir::new().unwrap();
        let (paths, workspace_id) = setup(&temp);
        let output = paths
            .workspace_dir(&workspace_id)
            .unwrap()
            .join("output/report.md");
        fs::write(&output, "report").unwrap();
        let registry = ArtifactRegistry {
            version: 1,
            artifacts: vec![StoredArtifact {
                id: "artifact-1".into(),
                workspace_id: workspace_id.clone(),
                run_id: None,
                artifact_type: "md".into(),
                title: "Report".into(),
                path: "output/report.md".into(),
                status: WorkArtifactStatus::Ready,
                size: 6,
                sha256: None,
                producer: None,
                evidence: None,
                sources: Vec::new(),
                version: 1,
                validation_summary: None,
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            }],
        };
        write_registry(&paths, &workspace_id, &registry).unwrap();

        delete_with_paths(&paths, &workspace_id, "artifact-1").unwrap();
        assert!(!output.exists());
        let registry = read_registry(&paths, &workspace_id).unwrap();
        assert!(registry.artifacts.is_empty());
        assert!(delete_with_paths(&paths, &workspace_id, "artifact-1").is_err());
    }

    #[test]
    fn delete_keeps_file_when_another_entry_references_same_path() {
        let temp = TempDir::new().unwrap();
        let (paths, workspace_id) = setup(&temp);
        let output = paths
            .workspace_dir(&workspace_id)
            .unwrap()
            .join("output/joined.json");
        fs::write(&output, "{}").unwrap();
        let entry = |id: &str, run_id: Option<&str>| StoredArtifact {
            id: id.into(),
            workspace_id: workspace_id.clone(),
            run_id: run_id.map(str::to_string),
            artifact_type: "json".into(),
            title: "Joined".into(),
            path: "output/joined.json".into(),
            status: WorkArtifactStatus::Ready,
            size: 2,
            sha256: None,
            producer: None,
            evidence: None,
            sources: Vec::new(),
            version: 1,
            validation_summary: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        let registry = ArtifactRegistry {
            version: 1,
            artifacts: vec![
                entry("artifact-1", None),
                entry("artifact-2", Some("run-1")),
            ],
        };
        write_registry(&paths, &workspace_id, &registry).unwrap();

        delete_with_paths(&paths, &workspace_id, "artifact-1").unwrap();
        assert!(output.exists());
        let remaining = read_registry(&paths, &workspace_id).unwrap();
        assert_eq!(remaining.artifacts.len(), 1);
        assert_eq!(remaining.artifacts[0].id, "artifact-2");

        delete_with_paths(&paths, &workspace_id, "artifact-2").unwrap();
        assert!(!output.exists());
    }

    #[test]
    fn migrates_standalone_artifacts_and_scratch_idempotently() {
        let temp = TempDir::new().unwrap();
        let (paths, workspace_id) = setup(&temp);
        let run_id = "standalone-run";
        let standalone = paths.ensure_standalone_task_dir(run_id).unwrap();
        fs::write(standalone.join("output/report.md"), "standalone report").unwrap();
        fs::write(standalone.join("scratch/notes.md"), "draft notes").unwrap();
        register_standalone_with_paths(&paths, run_id, "output/report.md", "Report", Some("md"))
            .unwrap();

        // A same-named Workspace file must never be overwritten.
        let workspace_output = paths
            .workspace_dir(&workspace_id)
            .unwrap()
            .join("output/report.md");
        fs::write(&workspace_output, "workspace report").unwrap();

        assert_eq!(
            migrate_standalone_to_workspace_with_paths(&paths, run_id, &workspace_id).unwrap(),
            1
        );
        let registry = read_registry(&paths, &workspace_id).unwrap();
        let migrated = registry
            .artifacts
            .iter()
            .find(|artifact| artifact.run_id.as_deref() == Some(run_id))
            .unwrap();
        assert_ne!(migrated.path, "output/report.md");
        assert_eq!(
            fs::read_to_string(output_file_path(&paths, &workspace_id, &migrated.path).unwrap())
                .unwrap(),
            "standalone report"
        );
        assert_eq!(
            fs::read_to_string(
                paths
                    .workspace_dir(&workspace_id)
                    .unwrap()
                    .join("scratch/notes.md")
            )
            .unwrap(),
            "draft notes"
        );

        // Retrying the command must not duplicate the registry entry.
        assert_eq!(
            migrate_standalone_to_workspace_with_paths(&paths, run_id, &workspace_id).unwrap(),
            1
        );
        assert_eq!(
            read_registry(&paths, &workspace_id)
                .unwrap()
                .artifacts
                .iter()
                .filter(|artifact| artifact.run_id.as_deref() == Some(run_id))
                .count(),
            1
        );
    }

    #[test]
    fn exports_validated_artifact_to_user_selected_path() {
        let temp = TempDir::new().unwrap();
        let (paths, workspace_id) = setup(&temp);
        let output = paths
            .workspace_dir(&workspace_id)
            .unwrap()
            .join("output/report.md");
        fs::write(&output, "report").unwrap();
        let registry = ArtifactRegistry {
            version: 1,
            artifacts: vec![StoredArtifact {
                id: "artifact-1".into(),
                workspace_id: workspace_id.clone(),
                run_id: None,
                artifact_type: "md".into(),
                title: "Report".into(),
                path: "output/report.md".into(),
                status: WorkArtifactStatus::Validated,
                size: 6,
                sha256: None,
                producer: None,
                evidence: None,
                sources: Vec::new(),
                version: 1,
                validation_summary: None,
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            }],
        };
        write_registry(&paths, &workspace_id, &registry).unwrap();
        let destination_dir = temp.path().join("exports");
        fs::create_dir_all(&destination_dir).unwrap();
        let destination = destination_dir.join("report.md");

        let not_delivered = export_with_paths(
            &paths,
            &workspace_id,
            "artifact-1",
            destination.to_str().unwrap(),
            None,
        );
        assert!(not_delivered.unwrap_err().contains("must be delivered"));

        let delivered = deliver_with_paths(&paths, &workspace_id, "artifact-1", None).unwrap();
        assert_eq!(delivered.status, WorkArtifactStatus::Delivered);

        let exported = export_with_paths(
            &paths,
            &workspace_id,
            "artifact-1",
            destination.to_str().unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(
            Path::new(&exported),
            fs::canonicalize(&destination).unwrap().as_path()
        );
        assert_eq!(fs::read_to_string(destination).unwrap(), "report");
    }

    #[test]
    fn copies_delivered_artifact_to_local_primary_work_root_without_overwrite() {
        let temp = TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let manager = workspace::WorkspaceManager::new(paths.clone());
        let project = temp.path().join("Project");
        fs::create_dir_all(&project).unwrap();
        let workspace = manager
            .create_from_folder(project.to_str().unwrap(), None)
            .unwrap();
        let source = paths
            .workspace_dir(&workspace.id)
            .unwrap()
            .join("output/report.md");
        fs::write(&source, "# report\n").unwrap();
        let artifact = register_with_paths(
            &paths,
            &workspace.id,
            "output/report.md",
            "Report",
            Some("md"),
            Some("run-1"),
        )
        .unwrap();

        let copied = copy_to_primary_work_root_with_paths(
            &paths,
            &workspace.id,
            &artifact.id,
            Some("run-1"),
        )
        .unwrap();
        let destination = project.join("output/report.md");
        assert_eq!(Path::new(&copied), fs::canonicalize(&destination).unwrap());
        assert_eq!(fs::read_to_string(&destination).unwrap(), "# report\n");

        // Repeating the same explicit copy is idempotent.
        assert_eq!(
            copy_to_primary_work_root_with_paths(
                &paths,
                &workspace.id,
                &artifact.id,
                Some("run-1"),
            )
            .unwrap(),
            copied
        );

        fs::write(&destination, "different\n").unwrap();
        let error = copy_to_primary_work_root_with_paths(
            &paths,
            &workspace.id,
            &artifact.id,
            Some("run-1"),
        )
        .unwrap_err();
        assert!(error.contains("内容不同"));
    }
}
