//! WorkRunReceipt: a pure read-only projection over existing authorities.
//!
//! Like `progress.rs`, this module adds **no new persistent authority**. The
//! receipt is recomputed on demand from:
//!   * the WorkRun record (status, timing, goal) or the standalone session meta
//!   * the runtime Ledger (`ToolProposed` / `ToolResult` / `FileChanged` facts)
//!   * the 网络访问 browser ledger (`browser-ledger.json`, written by the Pi
//!     web adapter with the URLs it actually fetched)
//!   * the Artifact registry (workspace or standalone)
//!   * session `meta.json` (runtime provider / model)
//!
//! Hard rules (Release Gate "Sources / Changes"):
//!   * a source is `accessed` only when a durable successful fetch/open event
//!     proves it; appearing in search results alone never marks access;
//!   * file changes come from durable `file_changed` facts first, and only
//!     fall back to ledger tool outputs (with a heuristic classification) for
//!     ledgers written before this fact existed.

use std::collections::HashMap;
use std::path::Path;

use chrono::Utc;
use serde::Deserialize;

use crate::work::artifacts;
use crate::work::ledger::WorkRuntimeLedger;
use crate::work::models::{
    RuntimeFact, WorkArtifactSummary, WorkFileChangeKind, WorkProgressToolSummary,
    WorkReceiptFileChange, WorkReceiptSearchQuery, WorkReceiptSource, WorkReceiptSourceKind,
    WorkRunReceipt, WorkRunStatus,
};
use crate::work::paths::WorkPaths;
use crate::work::tasks::TaskManager;

/// Project the receipt for a Workspace WorkRun.
pub fn project_run_receipt(
    paths: &WorkPaths,
    task_id: &str,
    run_id: &str,
) -> Result<WorkRunReceipt, String> {
    let task_manager = TaskManager::new(paths.clone());
    let run = task_manager.get_run(task_id, run_id)?;
    if run.task_id != task_id {
        return Err(format!(
            "Run '{run_id}' does not belong to task '{task_id}' (actual: '{}')",
            run.task_id
        ));
    }

    let ledger_run = LedgerProjection::collect(paths, task_id, run_id, &run.workspace_id);
    let browser = load_browser_ledger(run.session_id.as_deref());
    let artifacts = if run.workspace_id.trim().is_empty() {
        artifacts::list_standalone_with_paths(paths, run_id).unwrap_or_default()
    } else {
        artifacts::list_with_paths(paths, &run.workspace_id, Some(run_id)).unwrap_or_default()
    };
    let meta = run
        .session_id
        .as_deref()
        .and_then(crate::storage::runs::get_run);

    let mut receipt = base_receipt(task_id, run_id, run_id);
    receipt.workspace_id = run.workspace_id.clone();
    receipt.session_id = run.session_id.clone();
    receipt.standalone = false;
    receipt.status = run.status;
    receipt.goal = run.task_state.goal.clone().or_else(|| {
        // Legacy fallback: the task title is the last-known goal statement.
        task_manager
            .get_task(task_id)
            .ok()
            .map(|task| task.title.clone())
    });
    receipt.started_at = run.started_at.clone();
    receipt.finished_at = run.finished_at.clone();
    receipt.duration_ms = run.duration_ms;
    receipt.error_message = run.error_message.clone();
    if let Some(meta) = meta.as_ref() {
        receipt.runtime = Some(meta.agent.clone());
        receipt.model = meta.model.clone();
    }
    apply_projection(&mut receipt, artifacts, ledger_run, browser, run.status);
    Ok(receipt)
}

/// Project the receipt for a standalone (workspace-less) Work chat run.
/// Standalone Work chats fall back to `task_id == run_id` for their ledger.
pub fn project_standalone_receipt(
    paths: &WorkPaths,
    run_id: &str,
) -> Result<WorkRunReceipt, String> {
    let meta = crate::storage::runs::get_run(run_id);
    let ledger_run = LedgerProjection::collect(paths, run_id, run_id, "");
    let browser = load_browser_ledger(Some(run_id));
    let artifacts = artifacts::list_standalone_with_paths(paths, run_id).unwrap_or_default();

    let status = meta
        .as_ref()
        .map(|meta| match meta.status {
            crate::models::RunStatus::Completed => WorkRunStatus::Completed,
            crate::models::RunStatus::Failed => WorkRunStatus::Failed,
            crate::models::RunStatus::Stopped => WorkRunStatus::Cancelled,
            crate::models::RunStatus::Running | crate::models::RunStatus::Idle => {
                WorkRunStatus::Running
            }
            crate::models::RunStatus::Pending => WorkRunStatus::Queued,
        })
        .unwrap_or(WorkRunStatus::Completed);

    let mut receipt = base_receipt(run_id, run_id, run_id);
    receipt.workspace_id = String::new();
    receipt.session_id = Some(run_id.to_string());
    receipt.standalone = true;
    receipt.status = status;
    receipt.started_at = meta
        .as_ref()
        .map(|meta| meta.started_at.clone())
        .unwrap_or_default();
    receipt.finished_at = meta.as_ref().and_then(|meta| meta.ended_at.clone());
    if let Some(meta) = meta.as_ref() {
        receipt.runtime = Some(meta.agent.clone());
        receipt.model = meta.model.clone();
    }
    apply_projection(&mut receipt, artifacts, ledger_run, browser, status);
    Ok(receipt)
}

fn base_receipt(task_id: &str, run_id: &str, _fallback: &str) -> WorkRunReceipt {
    WorkRunReceipt {
        task_id: task_id.to_string(),
        work_run_id: run_id.to_string(),
        workspace_id: String::new(),
        session_id: None,
        standalone: false,
        status: WorkRunStatus::default(),
        goal: None,
        runtime: None,
        model: None,
        started_at: String::new(),
        finished_at: None,
        duration_ms: None,
        error_message: None,
        artifacts: Vec::new(),
        sources: Vec::new(),
        search_queries: Vec::new(),
        input_files: Vec::new(),
        changed_files: Vec::new(),
        tool_summary: WorkProgressToolSummary::default(),
        ledger_available: false,
        generated_at: Utc::now().to_rfc3339(),
    }
}

fn apply_projection(
    receipt: &mut WorkRunReceipt,
    artifacts: Vec<WorkArtifactSummary>,
    ledger: LedgerProjection,
    browser: Option<BrowserLedger>,
    _status: WorkRunStatus,
) {
    receipt.artifacts = artifacts;
    receipt.ledger_available = ledger.available;
    receipt.tool_summary = ledger.tool_summary;
    receipt.input_files = ledger.input_files;

    let mut sources: HashMap<String, WorkReceiptSource> = HashMap::new();

    // 1. Ledger-backed events (Tool Pipeline execution: web_search / web_open /
    //    browser_navigate / library_read). Access requires a successful ToolResult.
    for source in ledger.sources {
        merge_source(&mut sources, source);
    }

    // 2. 网络访问 ledger (Pi browser adapter): searches surface URLs, pages
    //    prove real access. This file is the only durable record of Pi-side
    //    web tool results because those tools run outside the Tool Pipeline.
    if let Some(ledger) = browser {
        for search in &ledger.searches {
            receipt.search_queries.push(WorkReceiptSearchQuery {
                query: search.query.clone(),
                provider: search.provider.clone(),
                result_count: search.results.len(),
                timestamp: search.created_at.clone(),
            });
            for result in &search.results {
                if result.url.trim().is_empty() {
                    continue;
                }
                merge_source(
                    &mut sources,
                    WorkReceiptSource {
                        url: result.url.clone(),
                        title: (!result.title.trim().is_empty()).then(|| result.title.clone()),
                        provider: search.provider.clone(),
                        surfaced_by_search: true,
                        accessed: false,
                        access_count: 0,
                        first_seen_at: Some(search.created_at.clone()),
                        last_accessed_at: None,
                        kinds: vec![WorkReceiptSourceKind::WebSearch],
                    },
                );
            }
        }
        for page in &ledger.pages {
            if page.url.trim().is_empty() {
                continue;
            }
            merge_source(
                &mut sources,
                WorkReceiptSource {
                    url: page.url.clone(),
                    title: (!page.title.trim().is_empty()).then(|| page.title.clone()),
                    provider: None,
                    surfaced_by_search: false,
                    accessed: true,
                    access_count: 1,
                    first_seen_at: Some(page.created_at.clone()),
                    last_accessed_at: Some(page.created_at.clone()),
                    kinds: vec![WorkReceiptSourceKind::WebPage],
                },
            );
        }
        receipt
            .search_queries
            .sort_by(|left, right| left.timestamp.cmp(&right.timestamp));
    }

    let mut sources: Vec<WorkReceiptSource> = sources.into_values().collect();
    sources.sort_by(|left, right| {
        right.accessed.cmp(&left.accessed).then_with(|| {
            right
                .last_accessed_at
                .cmp(&left.last_accessed_at)
                .then_with(|| left.first_seen_at.cmp(&right.first_seen_at))
        })
    });
    receipt.sources = sources;
    receipt.changed_files = ledger.changes;
}

fn merge_source(sources: &mut HashMap<String, WorkReceiptSource>, incoming: WorkReceiptSource) {
    let entry = sources.entry(incoming.url.clone()).or_default();
    if entry.url.is_empty() {
        entry.url = incoming.url.clone();
    }
    if entry.title.is_none() {
        entry.title = incoming.title.clone();
    }
    if entry.provider.is_none() {
        entry.provider = incoming.provider.clone();
    }
    entry.surfaced_by_search |= incoming.surfaced_by_search;
    entry.accessed |= incoming.accessed;
    entry.access_count += incoming.access_count;
    entry.first_seen_at = match (entry.first_seen_at.take(), incoming.first_seen_at) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    entry.last_accessed_at = match (entry.last_accessed_at.take(), incoming.last_accessed_at) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    for kind in incoming.kinds {
        if !entry.kinds.contains(&kind) {
            entry.kinds.push(kind);
        }
    }
}

// ============================================================================
// Ledger fact scanning
// ============================================================================

struct LedgerProjection {
    available: bool,
    tool_summary: WorkProgressToolSummary,
    sources: Vec<WorkReceiptSource>,
    input_files: Vec<String>,
    changes: Vec<WorkReceiptFileChange>,
}

impl LedgerProjection {
    fn collect(paths: &WorkPaths, task_id: &str, run_id: &str, workspace_id: &str) -> Self {
        let ledger_path = match paths.task_run_ledger_path(task_id, run_id) {
            Ok(path) => path,
            Err(_) => {
                return Self::empty();
            }
        };
        if !ledger_path.exists() {
            return Self::empty();
        }
        let ledger = WorkRuntimeLedger::for_path(ledger_path);
        let facts = match ledger.list_facts() {
            Ok(facts) => facts,
            Err(error) => {
                log::warn!("[receipt] ledger unreadable for task={task_id} run={run_id}: {error}");
                return Self::empty();
            }
        };
        Self::from_facts_with_context(&facts, Some(paths), workspace_id)
    }

    fn empty() -> Self {
        Self {
            available: false,
            tool_summary: WorkProgressToolSummary::default(),
            sources: Vec::new(),
            input_files: Vec::new(),
            changes: Vec::new(),
        }
    }

    #[cfg(test)]
    fn from_facts(facts: &[RuntimeFact]) -> Self {
        Self::from_facts_with_context(facts, None, "")
    }

    fn from_facts_with_context(
        facts: &[RuntimeFact],
        paths: Option<&WorkPaths>,
        workspace_id: &str,
    ) -> Self {
        let mut summary = WorkProgressToolSummary::default();
        let mut tool_names: HashMap<&str, &str> = HashMap::new();
        let mut sources: Vec<WorkReceiptSource> = Vec::new();
        let mut input_files: Vec<String> = Vec::new();
        let mut change_order: Vec<String> = Vec::new();
        let mut changes: HashMap<String, WorkFileChangeKind> = HashMap::new();
        let mut results_seen: HashMap<&str, bool> = HashMap::new();

        for fact in facts {
            match fact {
                RuntimeFact::ToolProposed {
                    tool_call_id,
                    tool_name,
                    ..
                } => {
                    summary.proposed += 1;
                    tool_names.entry(tool_call_id).or_insert(tool_name);
                }
                RuntimeFact::ToolStarted { tool_call_id, .. } => {
                    if results_seen.contains_key(tool_call_id.as_str()) {
                        continue;
                    }
                    summary.started += 1;
                }
                RuntimeFact::ToolResult {
                    tool_call_id,
                    success,
                    outputs,
                    timestamp,
                    ..
                } => {
                    if results_seen
                        .insert(tool_call_id.as_str(), *success)
                        .is_none()
                    {
                        if *success {
                            summary.completed += 1;
                        } else {
                            summary.failed += 1;
                        }
                    }
                    if !*success {
                        continue;
                    }
                    let tool_name = tool_names.get(tool_call_id.as_str()).copied().unwrap_or("");
                    match tool_name {
                        "web_search" => {
                            for url in outputs {
                                if !is_http_url(url) {
                                    continue;
                                }
                                sources.push(WorkReceiptSource {
                                    url: url.clone(),
                                    title: None,
                                    provider: None,
                                    surfaced_by_search: true,
                                    accessed: false,
                                    access_count: 0,
                                    first_seen_at: Some(timestamp.clone()),
                                    last_accessed_at: None,
                                    kinds: vec![WorkReceiptSourceKind::WebSearch],
                                });
                            }
                        }
                        "web_open" | "web_fetch" => {
                            for url in outputs {
                                if !is_http_url(url) {
                                    continue;
                                }
                                sources.push(WorkReceiptSource {
                                    url: url.clone(),
                                    title: None,
                                    provider: None,
                                    surfaced_by_search: false,
                                    accessed: true,
                                    access_count: 1,
                                    first_seen_at: Some(timestamp.clone()),
                                    last_accessed_at: Some(timestamp.clone()),
                                    kinds: vec![WorkReceiptSourceKind::WebPage],
                                });
                            }
                        }
                        "browser_navigate" => {
                            for url in outputs {
                                if !is_http_url(url) {
                                    continue;
                                }
                                sources.push(WorkReceiptSource {
                                    url: url.clone(),
                                    title: None,
                                    provider: None,
                                    surfaced_by_search: false,
                                    accessed: true,
                                    access_count: 1,
                                    first_seen_at: Some(timestamp.clone()),
                                    last_accessed_at: Some(timestamp.clone()),
                                    kinds: vec![WorkReceiptSourceKind::Browser],
                                });
                            }
                        }
                        "library_read" => {
                            for library_ref in outputs {
                                let Some(item_id) = library_ref.strip_prefix("library:") else {
                                    continue;
                                };
                                let item_id = item_id.trim();
                                if item_id.is_empty() {
                                    continue;
                                }
                                let title = paths
                                    .and_then(|paths| {
                                        crate::work::library::LibraryManager::new(paths.clone())
                                            .get_item_scoped(
                                                item_id,
                                                (!workspace_id.trim().is_empty())
                                                    .then_some(workspace_id),
                                            )
                                            .ok()
                                    })
                                    .map(|item| item.title)
                                    .unwrap_or_else(|| format!("资料库条目 {item_id}"));
                                sources.push(WorkReceiptSource {
                                    url: format!("library://{item_id}"),
                                    title: Some(title),
                                    provider: Some("AgentCabin Library".to_string()),
                                    surfaced_by_search: false,
                                    accessed: true,
                                    access_count: 1,
                                    first_seen_at: Some(timestamp.clone()),
                                    last_accessed_at: Some(timestamp.clone()),
                                    kinds: vec![WorkReceiptSourceKind::Library],
                                });
                            }
                        }
                        "work_read_file" => {
                            for path in outputs {
                                if path.starts_with("input/") && !input_files.contains(path) {
                                    input_files.push(path.clone());
                                }
                            }
                        }
                        // Legacy fallback: ledgers written before the durable
                        // file_changed fact only recorded paths in outputs.
                        "work_write_file" | "work_edit_file" => {
                            for path in outputs {
                                if is_http_url(path) {
                                    continue;
                                }
                                let kind = legacy_change_kind(tool_name, path);
                                apply_change(&mut change_order, &mut changes, path, kind);
                            }
                        }
                        _ => {}
                    }
                }
                RuntimeFact::FileChanged {
                    path, change_kind, ..
                } => {
                    apply_change(&mut change_order, &mut changes, path, *change_kind);
                }
                _ => {}
            }
        }

        // running = started but never resolved with a final result
        summary.running = summary
            .started
            .saturating_sub(summary.completed + summary.failed);

        let changes = change_order
            .into_iter()
            .filter_map(|path| {
                changes.get(&path).map(|kind| WorkReceiptFileChange {
                    path,
                    change_kind: *kind,
                })
            })
            .collect();

        Self {
            available: true,
            tool_summary: summary,
            sources,
            input_files,
            changes,
        }
    }
}

/// Merge a per-path change with the same terminal-kind semantics the UI shows:
/// Deleted always wins; an already-created file stays "created" when later
/// modified; otherwise the newest kind applies.
fn apply_change(
    order: &mut Vec<String>,
    changes: &mut HashMap<String, WorkFileChangeKind>,
    path: &str,
    kind: WorkFileChangeKind,
) {
    let normalized = path.trim().replace('\\', "/");
    if normalized.is_empty() || is_http_url(&normalized) {
        return;
    }
    if !changes.contains_key(&normalized) {
        order.push(normalized.clone());
    }
    let next = match (changes.get(&normalized), kind) {
        (_, WorkFileChangeKind::Deleted) => WorkFileChangeKind::Deleted,
        (Some(WorkFileChangeKind::Deleted), other) => other,
        (Some(WorkFileChangeKind::Created), WorkFileChangeKind::Modified) => {
            WorkFileChangeKind::Created
        }
        (_, other) => other,
    };
    changes.insert(normalized, next);
}

fn legacy_change_kind(tool_name: &str, path: &str) -> WorkFileChangeKind {
    if tool_name == "work_edit_file" {
        WorkFileChangeKind::Modified
    } else if path.starts_with("output/") {
        // Deliverables written by a run that did not exist before are the
        // overwhelmingly common case for output/ writes.
        WorkFileChangeKind::Created
    } else {
        WorkFileChangeKind::Modified
    }
}

fn is_http_url(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("http://") || trimmed.starts_with("https://")
}

// ============================================================================
// 网络访问 (Pi browser adapter) ledger reader
// ============================================================================

#[derive(Debug, Deserialize, Default)]
struct BrowserLedger {
    #[serde(default)]
    searches: Vec<BrowserSearchRecord>,
    #[serde(default)]
    pages: Vec<BrowserPageRecord>,
}

#[derive(Debug, Deserialize)]
struct BrowserSearchRecord {
    #[serde(default)]
    query: String,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    results: Vec<BrowserSearchResultRecord>,
}

#[derive(Debug, Deserialize)]
struct BrowserSearchResultRecord {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
}

#[derive(Debug, Deserialize)]
struct BrowserPageRecord {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    created_at: String,
}

/// Read `~/.agentcabin/runs/<session_run_id>/browser/browser-ledger.json`.
/// This is the ledger the Pi 网络访问 adapter maintains for searches it
/// performed and pages it actually fetched; it never contains agent claims.
fn load_browser_ledger(session_run_id: Option<&str>) -> Option<BrowserLedger> {
    let session_run_id = session_run_id?;
    let path = crate::storage::run_dir(session_run_id)
        .join("browser")
        .join("browser-ledger.json");
    read_browser_ledger_file(&path)
}

fn read_browser_ledger_file(path: &Path) -> Option<BrowserLedger> {
    let content = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<BrowserLedger>(&content) {
        Ok(ledger) => Some(ledger),
        Err(error) => {
            log::warn!(
                "[receipt] browser ledger unreadable at {}: {error}",
                path.display()
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::models::{
        ExecutionContext, SideEffectClass, ToolConcurrencyClass, WorkRunStatus,
    };

    fn proposed(id: &str, name: &str) -> RuntimeFact {
        RuntimeFact::ToolProposed {
            tool_call_id: id.to_string(),
            tool_name: name.to_string(),
            action: "run".to_string(),
            arguments_hash: "hash".to_string(),
            expected_outputs: Vec::new(),
            side_effect_class: SideEffectClass::Read,
            concurrency_class: ToolConcurrencyClass::ParallelSafe,
            timestamp: "2026-08-30T00:00:00Z".to_string(),
        }
    }

    fn result(id: &str, success: bool, outputs: &[&str]) -> RuntimeFact {
        RuntimeFact::ToolResult {
            tool_call_id: id.to_string(),
            success,
            status: if success { "success" } else { "failed" }.to_string(),
            failure_kind: None,
            exit_code: Some(if success { 0 } else { 1 }),
            error: None,
            outputs: outputs.iter().map(|value| value.to_string()).collect(),
            side_effect_class: SideEffectClass::Read,
            timestamp: "2026-08-30T00:01:00Z".to_string(),
        }
    }

    #[test]
    fn sources_require_successful_events_and_search_never_marks_access() {
        let facts = vec![
            proposed("c1", "web_search"),
            result("c1", true, &["https://example.com/a"]),
            proposed("c2", "web_open"),
            result("c2", true, &["https://example.com/a"]),
            proposed("c3", "web_open"),
            result("c3", false, &["https://example.com/failed"]),
        ];
        let projection = LedgerProjection::from_facts(&facts);
        let merged: Vec<WorkReceiptSource> = {
            let mut map: HashMap<String, WorkReceiptSource> = HashMap::new();
            for source in projection.sources {
                merge_source(&mut map, source);
            }
            let mut list: Vec<_> = map.into_values().collect();
            list.sort_by(|left, right| left.url.cmp(&right.url));
            list
        };
        assert_eq!(merged.len(), 1, "failed fetch must not create a source");
        assert_eq!(merged[0].url, "https://example.com/a");
        assert!(merged[0].surfaced_by_search);
        assert!(merged[0].accessed);
        assert_eq!(merged[0].access_count, 1);
    }

    #[test]
    fn search_only_source_stays_unaccessed() {
        let facts = vec![
            proposed("c1", "web_search"),
            result("c1", true, &["https://example.com/b"]),
        ];
        let projection = LedgerProjection::from_facts(&facts);
        assert_eq!(projection.sources.len(), 1);
        assert!(!projection.sources[0].accessed);
        assert!(projection.sources[0].surfaced_by_search);
    }

    #[test]
    fn file_changed_facts_win_over_legacy_outputs_and_merge_kinds() {
        let facts = vec![
            // Legacy output-based fallback: created for output/, modified otherwise.
            proposed("w1", "work_write_file"),
            result("w1", true, &["output/report.xlsx"]),
            // Durable facts: the same file is later modified, then deleted.
            RuntimeFact::FileChanged {
                tool_call_id: "e1".to_string(),
                path: "output/notes.md".to_string(),
                change_kind: WorkFileChangeKind::Created,
                timestamp: "2026-08-30T00:02:00Z".to_string(),
            },
            RuntimeFact::FileChanged {
                tool_call_id: "e2".to_string(),
                path: "output/notes.md".to_string(),
                change_kind: WorkFileChangeKind::Modified,
                timestamp: "2026-08-30T00:03:00Z".to_string(),
            },
            RuntimeFact::FileChanged {
                tool_call_id: "d1".to_string(),
                path: "scratch/tmp.txt".to_string(),
                change_kind: WorkFileChangeKind::Deleted,
                timestamp: "2026-08-30T00:04:00Z".to_string(),
            },
        ];
        let projection = LedgerProjection::from_facts(&facts);
        let find = |path: &str| {
            projection
                .changes
                .iter()
                .find(|change| change.path == path)
                .map(|change| change.change_kind)
        };
        assert_eq!(
            find("output/report.xlsx"),
            Some(WorkFileChangeKind::Created)
        );
        assert_eq!(find("output/notes.md"), Some(WorkFileChangeKind::Created));
        assert_eq!(find("scratch/tmp.txt"), Some(WorkFileChangeKind::Deleted));
    }

    #[test]
    fn input_files_come_from_successful_reads_of_input_area() {
        let facts = vec![
            proposed("r1", "work_read_file"),
            result("r1", true, &["input/q2-sales.xlsx"]),
            proposed("r2", "work_read_file"),
            result("r2", false, &["input/missing.csv"]),
        ];
        let projection = LedgerProjection::from_facts(&facts);
        assert_eq!(
            projection.input_files,
            vec!["input/q2-sales.xlsx".to_string()]
        );
    }

    #[test]
    fn library_reads_become_accessed_receipt_sources() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = WorkPaths::new(temp.path().join("data"));
        let item = crate::work::library::LibraryManager::new(paths.clone())
            .save_item(crate::work::models::LibraryItem {
                id: "lib-receipt".to_string(),
                workspace_id: Some("ws-receipt".to_string()),
                title: "产品定价政策".to_string(),
                description: String::new(),
                category: crate::work::models::LibraryCategory::Doc,
                content: "正文".to_string(),
                tags: Vec::new(),
                source_path: None,
                collection: Some("产品资料".to_string()),
                metadata: std::collections::BTreeMap::new(),
                citations: Vec::new(),
                source_artifact_id: None,
                created_at: String::new(),
                updated_at: String::new(),
            })
            .unwrap();
        let facts = vec![
            proposed("library-call", "library_read"),
            result("library-call", true, &[&format!("library:{}", item.id)]),
        ];

        let projection =
            LedgerProjection::from_facts_with_context(&facts, Some(&paths), "ws-receipt");
        assert_eq!(projection.sources.len(), 1);
        assert_eq!(projection.sources[0].url, "library://lib-receipt");
        assert_eq!(projection.sources[0].title.as_deref(), Some("产品定价政策"));
        assert!(projection.sources[0].accessed);
        assert_eq!(
            projection.sources[0].kinds,
            vec![WorkReceiptSourceKind::Library]
        );
    }

    #[test]
    fn tool_summary_counts_each_call_once() {
        let facts = vec![
            proposed("c1", "web_search"),
            RuntimeFact::ToolStarted {
                tool_call_id: "c1".to_string(),
                execution_id: "exec-1".to_string(),
                timestamp: "2026-08-30T00:00:30Z".to_string(),
            },
            result("c1", true, &["https://example.com/a"]),
            // Duplicate replay of the same result must not double-count.
            result("c1", true, &["https://example.com/a"]),
            proposed("c2", "web_open"),
            RuntimeFact::ToolStarted {
                tool_call_id: "c2".to_string(),
                execution_id: "exec-2".to_string(),
                timestamp: "2026-08-30T00:00:40Z".to_string(),
            },
        ];
        let projection = LedgerProjection::from_facts(&facts);
        assert_eq!(projection.tool_summary.proposed, 2);
        assert_eq!(projection.tool_summary.started, 2);
        assert_eq!(projection.tool_summary.completed, 1);
        assert_eq!(projection.tool_summary.running, 1);
    }

    #[test]
    fn browser_ledger_pages_prove_access_and_searches_surface_urls() {
        let dir =
            std::env::temp_dir().join(format!("agentcabin-receipt-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("browser-ledger.json");
        std::fs::write(
            &file,
            serde_json::json!({
                "version": 1,
                "searches": [{
                    "search_id": "search-1",
                    "query": "q2 sales",
                    "provider": "agentcabin/mock",
                    "created_at": "2026-08-30T00:00:00Z",
                    "results": [{
                        "source_id": "search-1_source_1",
                        "title": "Q2 Report",
                        "url": "https://example.com/q2",
                        "snippet": "..."
                    }]
                }],
                "pages": [{
                    "page_id": "page-1",
                    "source_id": "search-1_source_1",
                    "url": "https://example.com/q2",
                    "title": "Q2 Report",
                    "content_type": "text/html",
                    "created_at": "2026-08-30T00:00:30Z",
                    "text_chars": 1234
                }],
                "passages": [],
                "citations": []
            })
            .to_string(),
        )
        .unwrap();
        let ledger = read_browser_ledger_file(&file).expect("parse browser ledger");
        assert_eq!(ledger.searches.len(), 1);
        assert_eq!(ledger.pages.len(), 1);

        let mut sources: HashMap<String, WorkReceiptSource> = HashMap::new();
        for result in &ledger.searches[0].results {
            merge_source(
                &mut sources,
                WorkReceiptSource {
                    url: result.url.clone(),
                    title: Some(result.title.clone()),
                    provider: ledger.searches[0].provider.clone(),
                    surfaced_by_search: true,
                    accessed: false,
                    access_count: 0,
                    first_seen_at: Some(ledger.searches[0].created_at.clone()),
                    last_accessed_at: None,
                    kinds: vec![WorkReceiptSourceKind::WebSearch],
                },
            );
        }
        for page in &ledger.pages {
            merge_source(
                &mut sources,
                WorkReceiptSource {
                    url: page.url.clone(),
                    title: Some(page.title.clone()),
                    provider: None,
                    surfaced_by_search: false,
                    accessed: true,
                    access_count: 1,
                    first_seen_at: Some(page.created_at.clone()),
                    last_accessed_at: Some(page.created_at.clone()),
                    kinds: vec![WorkReceiptSourceKind::WebPage],
                },
            );
        }
        let source = &sources["https://example.com/q2"];
        assert!(source.surfaced_by_search && source.accessed);
        assert_eq!(source.provider.as_deref(), Some("agentcabin/mock"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn receipt_serializes_the_camel_case_frontend_contract() {
        let receipt = base_receipt("task-1", "run-1", "run-1");
        let value = serde_json::to_value(&receipt).unwrap();
        assert_eq!(value["taskId"], "task-1");
        assert_eq!(value["workRunId"], "run-1");
        assert_eq!(value["status"], "running");
        assert_eq!(value["toolSummary"]["completed"], 0);
        assert_eq!(value["ledgerAvailable"], false);
        assert!(value.get("finishedAt").is_none());
    }

    #[test]
    fn file_changed_fact_round_trips_snake_case() {
        let fact = RuntimeFact::FileChanged {
            tool_call_id: "call-1".to_string(),
            path: "output/a.pptx".to_string(),
            change_kind: WorkFileChangeKind::Created,
            timestamp: "2026-08-30T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&fact).unwrap();
        assert!(json.contains("\"type\":\"file_changed\""));
        assert!(json.contains("\"change_kind\":\"created\""));
        let parsed: RuntimeFact = serde_json::from_str(&json).unwrap();
        match parsed {
            RuntimeFact::FileChanged {
                path, change_kind, ..
            } => {
                assert_eq!(path, "output/a.pptx");
                assert_eq!(change_kind, WorkFileChangeKind::Created);
            }
            _ => panic!("wrong fact parsed"),
        }
        // Older ledgers never contain file_changed; unknown-to-old readers are
        // unaffected because existing variants are unchanged.
        let _ = ExecutionContext::Attended;
        let _ = WorkRunStatus::Completed;
    }
}
