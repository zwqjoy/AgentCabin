use chrono::Utc;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Manager};
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

use crate::agent::adapter::ActorSessionMap;
use crate::agent::capability_resolver::RuntimeProviderKind;
use crate::agent::spawn_locks::SpawnLocks;
use crate::models::BusEvent;
use crate::storage;
use crate::web_server::broadcaster::BroadcastEmitter;
use crate::work::models::{
    ExecutionContext, InboxItemStatus, WorkPolicy, WorkRunStatus, WorkRunTrigger,
};
use crate::work::paths::WorkPaths;
use crate::work::runtime::WorkLaunchOverrides;

use super::approval::{SmokeApprovalPolicy, UnexpectedApproval};
use super::config::SmokeConfig;
use super::report::{EffortInfo, ModelInfo, RuntimeInfo, SmokeFailure, SmokeReport};
use super::scenarios::{
    generate_core_contract_prompt, generate_smoke_workspace_name, ContractFixture,
    REQUIRED_ARTIFACT_PATH,
};
use super::verifier::{verify_core_contract_smoke, ApprovalStats, LedgerStats, VerifierInput};

struct SettingsSnapshot {
    work_default_runtime: Option<String>,
    pi_model: Option<String>,
    pi_effort: Option<String>,
}

fn snapshot_user_settings() -> SettingsSnapshot {
    let user_settings = storage::settings::get_user_settings();
    let agent_settings = storage::settings::get_agent_settings("pi");
    SettingsSnapshot {
        work_default_runtime: user_settings.work_default_runtime,
        pi_model: agent_settings.model,
        pi_effort: agent_settings.effort,
    }
}

fn verify_settings_unchanged(snapshot: &SettingsSnapshot) -> Result<(), String> {
    let current_user = storage::settings::get_user_settings();
    let current_agent = storage::settings::get_agent_settings("pi");

    if current_user.work_default_runtime != snapshot.work_default_runtime {
        return Err(format!(
            "User work_default_runtime mutated from {:?} to {:?}",
            snapshot.work_default_runtime, current_user.work_default_runtime
        ));
    }
    if current_agent.model != snapshot.pi_model {
        return Err(format!(
            "Pi agent setting model mutated from {:?} to {:?}",
            snapshot.pi_model, current_agent.model
        ));
    }
    if current_agent.effort != snapshot.pi_effort {
        return Err(format!(
            "Pi agent setting effort mutated from {:?} to {:?}",
            snapshot.pi_effort, current_agent.effort
        ));
    }
    Ok(())
}

pub async fn run_smoke_mode(app: AppHandle, config_path: String) -> i32 {
    let started_instant = Instant::now();
    let started_at = Utc::now().to_rfc3339();

    println!("============================================================");
    println!("AgentCabin Real Agent Smoke Runner V1");
    println!("============================================================");

    let config = match SmokeConfig::from_file(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("PRECHECK FAILED: Invalid smoke config: {e}");
            return 1;
        }
    };

    println!("\nSmoke Preflight\n");
    println!("Scenario: {}", config.scenario);
    println!("Runtime: {}", config.runtime);
    println!("Model: {}", config.model);
    println!("Effort: {}", config.effort);
    println!("Timeout: {}s\n", config.timeout_secs);

    // Preflight checks
    if let Err(e) = crate::work::runtime::get_pi_work_runtime("pi") {
        eprintln!("PRECHECK FAILED: Work runtime router does not support Pi: {e}");
        return 1;
    }

    let work_paths = WorkPaths::app();
    if let Err(e) = work_paths.ensure_layout() {
        eprintln!("PRECHECK FAILED: Work data layout not writable: {e}");
        return 1;
    }

    let report_dir = config.effective_report_dir(&storage::data_dir());
    if let Err(e) = std::fs::create_dir_all(&report_dir) {
        eprintln!("PRECHECK FAILED: Report directory not writable: {e}");
        return 1;
    }

    let initial_settings = snapshot_user_settings();

    // Helper for generating fail reports on early exits
    let emit_early_fail = |failure_kind: &str, reason: &str, preserved_ws: Option<String>| {
        let duration_ms = started_instant.elapsed().as_millis() as u64;
        let report = SmokeReport {
            version: 1,
            scenario: config.scenario.clone(),
            verdict: "FAIL".to_string(),
            started_at: started_at.clone(),
            ended_at: Utc::now().to_rfc3339(),
            duration_ms,
            runtime: RuntimeInfo {
                requested: "pi".to_string(),
                effective: "unknown".to_string(),
            },
            model: ModelInfo {
                requested: config.model.clone(),
                run_meta: "unknown".to_string(),
                runtime_reported: None,
                runtime_verified: false,
            },
            effort: EffortInfo {
                requested: config.effort.clone(),
                effective_launch: None,
                verified: false,
            },
            run: None,
            approvals: ApprovalStats {
                requested: 0,
                approved: 0,
                unexpected: 0,
            },
            ledger: LedgerStats {
                tool_proposed: 0,
                tool_started: 0,
                tool_result: 0,
                tool_failures: 0,
                duplicate_tool_started: 0,
            },
            artifacts: Vec::new(),
            semantic_assertions: Vec::new(),
            failure: Some(SmokeFailure {
                failure_kind: failure_kind.to_string(),
                reason: reason.to_string(),
                details: None,
            }),
            preserved_workspace_path: preserved_ws,
        };
        if let Ok((json_path, md_path)) = report.write_to_dir(&report_dir) {
            println!(
                "[smoke] Fail report written to:\n  {}\n  {}",
                json_path.display(),
                md_path.display()
            );
        }
    };

    // Retrieve managed state from Tauri
    let emitter = match app.try_state::<Arc<BroadcastEmitter>>() {
        Some(e) => e.inner().clone(),
        None => {
            let msg = "BroadcastEmitter not found in app state";
            eprintln!("PRECHECK FAILED: {msg}");
            emit_early_fail("environment_not_ready", msg, None);
            return 1;
        }
    };
    let sessions = match app.try_state::<ActorSessionMap>() {
        Some(s) => s.inner().clone(),
        None => {
            let msg = "ActorSessionMap not found in app state";
            eprintln!("PRECHECK FAILED: {msg}");
            emit_early_fail("environment_not_ready", msg, None);
            return 1;
        }
    };
    let spawn_locks = match app.try_state::<SpawnLocks>() {
        Some(l) => l.inner().clone(),
        None => {
            let msg = "SpawnLocks not found in app state";
            eprintln!("PRECHECK FAILED: {msg}");
            emit_early_fail("environment_not_ready", msg, None);
            return 1;
        }
    };
    let cancel_token = match app.try_state::<CancellationToken>() {
        Some(t) => t.inner().clone(),
        None => {
            let msg = "CancellationToken not found in app state";
            eprintln!("PRECHECK FAILED: {msg}");
            emit_early_fail("environment_not_ready", msg, None);
            return 1;
        }
    };

    // 1. Setup external fixture
    let fixture = match ContractFixture::setup(config.external_fixture_dir.as_deref()) {
        Ok(f) => f,
        Err(e) => {
            let msg = format!("Could not setup fixture: {e}");
            eprintln!("PRECHECK FAILED: {msg}");
            emit_early_fail("environment_not_ready", &msg, None);
            return 1;
        }
    };
    let approval_policy = SmokeApprovalPolicy::new(fixture.contracts_dir.clone());

    // 2. Setup Workspace & Task
    let ws_name = generate_smoke_workspace_name();
    let ws_mgr = crate::work::workspace::WorkspaceManager::new(work_paths.clone());
    let workspace = match ws_mgr.create(&ws_name) {
        Ok(ws) => ws,
        Err(e) => {
            let msg = format!("Could not create smoke workspace: {e}");
            eprintln!("PRECHECK FAILED: {msg}");
            fixture.cleanup();
            emit_early_fail("environment_not_ready", &msg, None);
            return 1;
        }
    };

    let prompt = generate_core_contract_prompt(&fixture.contracts_dir);
    let task_mgr = crate::work::tasks::TaskManager::new(work_paths.clone());
    let mut task = match task_mgr.create_task_with_schedule(
        &workspace.id,
        "Smoke: Process contracts and archive",
        &prompt,
        Some(WorkPolicy::default_for_workspace()),
        None,
    ) {
        Ok(t) => t,
        Err(e) => {
            let msg = format!("Could not create smoke task: {e}");
            eprintln!("PRECHECK FAILED: {msg}");
            let _ = ws_mgr.delete(&workspace.id);
            fixture.cleanup();
            emit_early_fail("environment_not_ready", &msg, Some(workspace.root.clone()));
            return 1;
        }
    };
    task.required_artifacts = vec![REQUIRED_ARTIFACT_PATH.to_string()];
    if let Err(e) = task_mgr.update_task(&mut task) {
        let msg = format!("Could not set required artifact on task: {e}");
        eprintln!("PRECHECK FAILED: {msg}");
        let _ = ws_mgr.delete(&workspace.id);
        fixture.cleanup();
        emit_early_fail("environment_not_ready", &msg, Some(workspace.root.clone()));
        return 1;
    }

    println!("[smoke] Workspace created: {}", workspace.id);
    println!("[smoke] Task created: {}", task.id);
    println!(
        "[smoke] External fixture contracts: {}",
        fixture.contracts_dir.display()
    );

    // 3. Launch WorkTaskRun with Overrides
    let overrides = WorkLaunchOverrides {
        runtime: Some(RuntimeProviderKind::Pi),
        model: Some(config.model.clone()),
        effort: Some(config.effort.clone()),
        forbid_model_fallback: true,
    };

    let launch_result = crate::commands::work::tasks::start_work_task_run_with_overrides(
        &work_paths,
        &emitter,
        &sessions,
        &spawn_locks,
        &cancel_token,
        &task.id,
        WorkRunTrigger::Manual,
        ExecutionContext::Attended,
        None,
        Some(overrides),
    )
    .await;

    let run = match launch_result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[smoke] Failed to start work task run: {e}");
            let kind = if e.contains("Model fallback forbidden") {
                "model_unavailable"
            } else {
                "runtime_start_failed"
            };
            emit_early_fail(kind, &e, Some(workspace.root.clone()));
            fixture.cleanup();
            return 1;
        }
    };

    println!("[smoke] Run started: {}", run.id);

    // 4. Event-driven wait loop
    let broadcaster = emitter.broadcaster();
    let mut rx_a = broadcaster.subscribe_a();
    let mut rx_b = broadcaster.subscribe_b();

    let deadline = Instant::now() + Duration::from_secs(config.timeout_secs);
    let mut approvals_requested = 0;
    let mut approvals_approved = 0;
    let mut unexpected_approvals: Vec<UnexpectedApproval> = Vec::new();
    let mut runtime_reported_model: Option<String> = None;
    let mut failure_reason: Option<(String, String)> = None;

    println!(
        "[smoke] Waiting for task completion (timeout: {}s)...",
        config.timeout_secs
    );

    'wait_loop: loop {
        if Instant::now() >= deadline {
            failure_reason = Some((
                "timeout".to_string(),
                format!("Run timed out after {}s", config.timeout_secs),
            ));
            let _ = crate::work::session::stop(&emitter, &sessions, &spawn_locks, &run.id).await;
            break 'wait_loop;
        }

        // Process any pending inbox approvals
        let inbox_mgr = crate::work::inbox::InboxManager::new(work_paths.clone());
        if let Ok(all_items) = inbox_mgr.list_items(false, Some(&task.id)) {
            let pending_items = all_items
                .into_iter()
                .filter(|item| item.run_id == run.id && item.status == InboxItemStatus::Pending);
            for item in pending_items {
                approvals_requested += 1;
                println!(
                    "[smoke] Pending inbox item detected: {} ({:?})",
                    item.id, item.item_type
                );

                match approval_policy.evaluate_inbox_item(&item) {
                    Ok(()) => {
                        println!(
                            "[smoke] Approving whitelisted AccessRootRequest for item {}",
                            item.id
                        );
                        match crate::commands::work::interactions::resolve_inbox_item_impl(
                            &emitter,
                            &sessions,
                            &spawn_locks,
                            &cancel_token,
                            item.id.clone(),
                            InboxItemStatus::Approved,
                            None,
                        )
                        .await
                        {
                            Ok(_) => {
                                approvals_approved += 1;
                                println!(
                                    "[smoke] Inbox item {} successfully resolved as Approved",
                                    item.id
                                );
                            }
                            Err(e) => {
                                eprintln!("[smoke] Failed to resolve inbox item {}: {e}", item.id);
                                failure_reason = Some((
                                    "internal_error".to_string(),
                                    format!("Failed to resolve inbox item: {e}"),
                                ));
                                break 'wait_loop;
                            }
                        }
                    }
                    Err(unexpected) => {
                        eprintln!("[smoke] UNEXPECTED APPROVAL: {:?}", unexpected);
                        unexpected_approvals.push(unexpected.clone());
                        failure_reason = Some((
                            "unexpected_approval".to_string(),
                            format!("Rejected non-whitelisted approval: {}", unexpected.reason),
                        ));
                        // Reject the inbox item explicitly via production resolution before stopping run
                        let _ = crate::commands::work::interactions::resolve_inbox_item_impl(
                            &emitter,
                            &sessions,
                            &spawn_locks,
                            &cancel_token,
                            item.id.clone(),
                            InboxItemStatus::Rejected,
                            None,
                        )
                        .await;
                        let _ =
                            crate::work::session::stop(&emitter, &sessions, &spawn_locks, &run.id)
                                .await;
                        break 'wait_loop;
                    }
                }
            }
        }

        // Check durable WorkRun state
        if let Ok(current_run) = task_mgr.get_run(&task.id, &run.id) {
            match current_run.status {
                WorkRunStatus::Completed => {
                    println!("[smoke] WorkRun reached Completed status");
                    break 'wait_loop;
                }
                WorkRunStatus::WaitingInput => {
                    failure_reason = Some((
                        "unexpected_user_input".to_string(),
                        "Task entered WaitingInput state".to_string(),
                    ));
                    let _ = crate::work::session::stop(&emitter, &sessions, &spawn_locks, &run.id)
                        .await;
                    break 'wait_loop;
                }
                WorkRunStatus::Recoverable => {
                    failure_reason = Some((
                        "unexpected_recovery".to_string(),
                        "Task entered Recoverable state".to_string(),
                    ));
                    let _ = crate::work::session::stop(&emitter, &sessions, &spawn_locks, &run.id)
                        .await;
                    break 'wait_loop;
                }
                WorkRunStatus::Failed => {
                    failure_reason = Some((
                        "tool_failure".to_string(),
                        current_run
                            .error_message
                            .unwrap_or_else(|| "Task marked Failed".to_string()),
                    ));
                    break 'wait_loop;
                }
                WorkRunStatus::Cancelled => {
                    failure_reason = Some((
                        "internal_error".to_string(),
                        "Task was cancelled".to_string(),
                    ));
                    break 'wait_loop;
                }
                WorkRunStatus::Skipped => {
                    failure_reason = Some(("skipped".to_string(), "Task was skipped".to_string()));
                    break 'wait_loop;
                }
                WorkRunStatus::Queued
                | WorkRunStatus::Running
                | WorkRunStatus::WaitingApproval
                | WorkRunStatus::WaitingDelivery => {
                    // Normal in-progress states
                }
            }
        }

        // Wait for next event or 3s watchdog
        tokio::select! {
            _ = sleep(Duration::from_millis(3000)) => {
                // Watchdog tick
            }
            msg = rx_a.recv() => {
                if let Ok(msg) = msg {
                    if let Ok(bus_event) = serde_json::from_value::<BusEvent>(msg.payload) {
                        if let BusEvent::MessageComplete { model, .. } = bus_event {
                            if model.is_some() {
                                runtime_reported_model = model;
                            }
                        }
                    }
                }
            }
            msg = rx_b.recv() => {
                if let Ok(msg) = msg {
                    if let Ok(bus_event) = serde_json::from_value::<BusEvent>(msg.payload) {
                        if let BusEvent::MessageComplete { model, .. } = bus_event {
                            if model.is_some() {
                                runtime_reported_model = model;
                            }
                        }
                    }
                }
            }
        }
    }

    // 5. Verification Phase
    let ended_at = Utc::now().to_rfc3339();
    let duration_ms = started_instant.elapsed().as_millis() as u64;

    let verifier_input = VerifierInput {
        paths: &work_paths,
        workspace_id: &workspace.id,
        task_id: &task.id,
        run_id: &run.id,
        requested_model: &config.model,
        fixture_contracts_dir: &fixture.contracts_dir,
        approvals_requested,
        approvals_approved,
        unexpected_approvals: unexpected_approvals.len(),
    };

    let verification_result = verify_core_contract_smoke(&verifier_input);

    let (run_stats, approval_stats, ledger_stats, artifacts, semantic_assertions, verifier_failure) =
        match verification_result {
            Ok(res) => {
                let failure = if let Some(ref manual_fail) = failure_reason {
                    Some(SmokeFailure {
                        failure_kind: manual_fail.0.clone(),
                        reason: manual_fail.1.clone(),
                        details: None,
                    })
                } else if let Some((kind, reason)) = res.first_failure_kind_and_reason() {
                    Some(SmokeFailure {
                        failure_kind: kind.to_string(),
                        reason,
                        details: None,
                    })
                } else {
                    None
                };
                (
                    Some(res.run_stats),
                    res.approval_stats,
                    res.ledger_stats,
                    res.artifacts,
                    res.semantic_assertions,
                    failure,
                )
            }
            Err(e) => (
                None,
                ApprovalStats {
                    requested: approvals_requested,
                    approved: approvals_approved,
                    unexpected: unexpected_approvals.len(),
                },
                LedgerStats {
                    tool_proposed: 0,
                    tool_started: 0,
                    tool_result: 0,
                    tool_failures: 0,
                    duplicate_tool_started: 0,
                },
                Vec::new(),
                Vec::new(),
                Some(SmokeFailure {
                    failure_kind: "internal_error".to_string(),
                    reason: e,
                    details: None,
                }),
            ),
        };

    // Verify settings remained unchanged
    let mut final_failure = verifier_failure;
    if final_failure.is_none() {
        if let Err(settings_err) = verify_settings_unchanged(&initial_settings) {
            final_failure = Some(SmokeFailure {
                failure_kind: "user_settings_mutated".to_string(),
                reason: settings_err,
                details: None,
            });
        }
    }

    // Verify model reported by runtime if present
    let model_runtime_verified = if let Some(ref reported) = runtime_reported_model {
        let requested_norm = config.model.trim().to_ascii_lowercase();
        let reported_norm = reported.trim().to_ascii_lowercase();
        let matched =
            reported_norm.contains(&requested_norm) || requested_norm.contains(&reported_norm);
        if !matched && final_failure.is_none() {
            final_failure = Some(SmokeFailure {
                failure_kind: "model_mismatch".to_string(),
                reason: format!(
                    "Runtime reported model '{}' does not match requested model '{}'",
                    reported, config.model
                ),
                details: None,
            });
        }
        matched
    } else {
        false
    };

    // Verify effort: must enter PASS gate before calculating verdict
    let captured_effort = crate::work::runtime::pi::get_last_launch_effort(&run.id);
    let effective_effort_str = captured_effort.as_ref().and_then(|e| e.as_deref());

    let (effort_verified, effort_failure) = match crate::work::smoke::verifier::verify_effort_gate(
        &config.effort,
        effective_effort_str,
    ) {
        Ok(verified) => (verified, None),
        Err((kind, reason)) => (
            false,
            Some(SmokeFailure {
                failure_kind: kind,
                reason,
                details: None,
            }),
        ),
    };

    if final_failure.is_none() && effort_failure.is_some() {
        final_failure = effort_failure;
    }

    let verdict = if final_failure.is_none() {
        "PASS".to_string()
    } else {
        "FAIL".to_string()
    };

    let is_pass = verdict == "PASS";

    let preserved_workspace_path = if is_pass && !config.keep_workspace {
        None
    } else {
        Some(workspace.root.clone())
    };

    let run_meta = crate::storage::runs::get_run(&run.id);
    let effective_runtime = run_meta
        .as_ref()
        .map(|m| m.agent.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let run_meta_model = run_meta
        .as_ref()
        .and_then(|m| m.model.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let report = SmokeReport {
        version: 1,
        scenario: config.scenario.clone(),
        verdict: verdict.clone(),
        started_at,
        ended_at,
        duration_ms,
        runtime: RuntimeInfo {
            requested: config.runtime.clone(),
            effective: effective_runtime,
        },
        model: ModelInfo {
            requested: config.model.clone(),
            run_meta: run_meta_model,
            runtime_reported: runtime_reported_model,
            runtime_verified: model_runtime_verified,
        },
        effort: EffortInfo {
            requested: config.effort.clone(),
            effective_launch: captured_effort.flatten(),
            verified: effort_verified,
        },
        run: run_stats,
        approvals: approval_stats,
        ledger: ledger_stats,
        artifacts,
        semantic_assertions,
        failure: final_failure,
        preserved_workspace_path,
    };

    let report_write_ok = match report.write_to_dir(&report_dir) {
        Ok((json_path, md_path)) => {
            println!("\nSmoke Report written to:");
            println!("  JSON:     {}", json_path.display());
            println!("  Markdown: {}\n", md_path.display());
            true
        }
        Err(e) => {
            eprintln!("[smoke] report_write_failed: Failed to write smoke report: {e}");
            false
        }
    };

    println!("============================================================");
    println!("Smoke Verdict: {}", verdict);
    println!("============================================================");

    // Cleanup on PASS only if report write also succeeded and keep_workspace was not requested
    if is_pass && report_write_ok && !config.keep_workspace {
        let _ = ws_mgr.delete(&workspace.id);
        fixture.cleanup();
    }

    if is_pass && report_write_ok {
        0
    } else {
        1
    }
}
