//! Guardian: Work Runtime Ledger anomaly detection.
//!
//! Pure functional evaluation module that analyzes `RuntimeFact`s in the Ledger
//! to identify stalls, duplicate tool loops, failure streaks, budget overflows,
//! subagent runaways, and runtime liveness changes.
//!
//! Guardian is strictly a detect/diagnose component. It NEVER performs
//! blind retries, duplicate recovery logic, or second state machine authority.

use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::work::ledger::WorkRuntimeLedger;
use crate::work::models::{
    GuardianAnomalyKind, GuardianConfig, RunHealth, RuntimeFact, RuntimeLiveness,
    WorkRunBudgetView, WorkRunStatus,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardianEvaluationResult {
    pub anomaly_kind: GuardianAnomalyKind,
    pub step_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub reason: String,
    pub threshold: Option<String>,
    pub suggested_action: Option<String>,
    pub is_critical: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuardianHealthReport {
    pub health: RunHealth,
    pub reason: Option<String>,
    pub anomalies: Vec<GuardianEvaluationResult>,
    pub new_facts: Vec<RuntimeFact>,
    pub steering_nudge: Option<String>,
    pub warning_count: u32,
    pub stalled_since: Option<String>,
    pub last_meaningful_progress_at: Option<String>,
    pub budget_view: WorkRunBudgetView,
    pub liveness: RuntimeLiveness,
}

pub struct Guardian;

impl Guardian {
    /// Pure evaluation: scan chronological ledger facts and produce detected anomalies.
    ///
    /// Deduplicates against already recorded facts in `facts`.
    pub fn evaluate(
        facts: &[RuntimeFact],
        run_status: WorkRunStatus,
        config: &GuardianConfig,
        now_iso: &str,
    ) -> Vec<GuardianEvaluationResult> {
        let report = Self::evaluate_health(facts, run_status, config, now_iso, None);
        report.anomalies
    }

    /// Durably records all anomalies and newly generated facts from a Guardian
    /// health evaluation into the runtime ledger. Each individual fact is
    /// idempotent across independent Ledger handles.
    pub fn persist_health_report(
        ledger: &WorkRuntimeLedger,
        report: &GuardianHealthReport,
        default_tool_call_id: Option<&str>,
    ) -> Result<(), String> {
        for anomaly in &report.anomalies {
            ledger.record_guardian_event_once(
                anomaly.anomaly_kind,
                anomaly.step_id.as_deref(),
                anomaly.tool_call_id.as_deref().or(default_tool_call_id),
                &anomaly.reason,
                anomaly.threshold.as_deref(),
                anomaly.suggested_action.as_deref(),
            )?;
        }
        for fact in &report.new_facts {
            ledger.record_guardian_fact_once(fact)?;
        }
        Ok(())
    }

    /// Full health evaluation: compute current RunHealth, anomalies, typed facts, and budget projection.
    pub fn evaluate_health(
        facts: &[RuntimeFact],
        run_status: WorkRunStatus,
        config: &GuardianConfig,
        now_iso: &str,
        current_liveness: Option<RuntimeLiveness>,
    ) -> GuardianHealthReport {
        let mut anomalies = Vec::new();
        let mut new_facts = Vec::new();
        let now_dt = parse_iso_or_utc(now_iso);
        let mut health = RunHealth::Healthy;
        let mut health_reason = None;
        let mut warning_count = 0u32;
        let mut stalled_since = None;
        let mut steering_nudge = None;

        let last_meaningful_at = find_last_meaningful_progress_timestamp(facts);

        // 1. Stall Detection (Only when run status is active / Running)
        if run_status == WorkRunStatus::Running {
            let active_step = find_current_step_id(facts);
            let active_tool = find_active_tool_call_id(facts);
            let has_ended_steps = facts
                .iter()
                .any(|f| matches!(f, RuntimeFact::StepEnded { .. }));

            // If all previous steps have ended and there is currently no active step or in-flight tool,
            // the agent has finished its planned work items and is awaiting next user input / idle.
            // Do not flag this completed/idle state as a stall anomaly.
            let should_check_stall = if has_ended_steps {
                active_step.is_some() || active_tool.is_some()
            } else {
                true
            };

            if should_check_stall {
                if let Some(ref last_progress_time) = last_meaningful_at {
                    let last_dt = parse_iso_or_utc(last_progress_time);
                    let elapsed_ms = (now_dt - last_dt).num_milliseconds().max(0) as u64;

                    if elapsed_ms >= config.stall_after_ms {
                        stalled_since = Some(last_progress_time.clone());
                        health = RunHealth::Stalled;
                        let reason = format!(
                            "运行在 {} 秒内无有意义进展（阈值：{} 秒）",
                            elapsed_ms / 1000,
                            config.stall_after_ms / 1000
                        );
                        health_reason = Some(reason.clone());
                        anomalies.push(GuardianEvaluationResult {
                            anomaly_kind: GuardianAnomalyKind::Stall,
                            step_id: active_step.clone(),
                            tool_call_id: active_tool
                                .clone()
                                .or_else(|| find_current_tool_call_id(facts)),
                            reason: reason.clone(),
                            threshold: Some(format!("stall_after_ms={}", config.stall_after_ms)),
                            suggested_action: Some(
                                "检查网络或外部依赖连接，或进行恢复/重启".to_string(),
                            ),
                            is_critical: true,
                        });

                        // Add typed RunStalled fact if not recorded yet
                        let already_stalled_fact = facts.iter().any(|f| match f {
                            RuntimeFact::RunStalled { idle_seconds, .. } => {
                                *idle_seconds >= (config.stall_after_ms / 1000)
                            }
                            _ => false,
                        });
                        if !already_stalled_fact {
                            new_facts.push(RuntimeFact::RunStalled {
                                idle_seconds: elapsed_ms / 1000,
                                last_progress_fact: active_step,
                                timestamp: now_iso.to_string(),
                            });
                        }
                    } else if elapsed_ms >= config.stall_warn_after_ms {
                        warning_count += 1;
                        if health == RunHealth::Healthy {
                            health = RunHealth::Warning;
                            health_reason = Some(format!(
                                "运行在 {} 秒内无新进展（软警告阈值：{} 秒）",
                                elapsed_ms / 1000,
                                config.stall_warn_after_ms / 1000
                            ));
                        }
                    }
                }
            }
        }

        // 2. Duplicate Tool Loop Detection
        if let Some((loop_result, count, fingerprint, tool_name)) =
            check_duplicate_tool_streak(facts, config.max_duplicate_calls)
        {
            let threshold = if is_polling_tool(&tool_name) {
                config.max_duplicate_calls.max(60)
            } else {
                config.max_duplicate_calls
            };
            let is_hard_loop = count >= threshold + 2;
            if is_hard_loop {
                health = RunHealth::Stalled;
                health_reason = Some(loop_result.reason.clone());
            } else {
                warning_count += 1;
                if health == RunHealth::Healthy {
                    health = RunHealth::Warning;
                    health_reason = Some(loop_result.reason.clone());
                }
            }

            // Check if steering nudge can be emitted
            let steering_count = count_steering_facts(facts);
            if steering_count < config.max_steering_nudges {
                steering_nudge = Some(format!(
                    "检测到连续重复执行工具 '{}'。请停止重复相同操作，评估现有输出并尝试不同策略或向用户求助。",
                    tool_name
                ));
            }

            // Typed LoopDetected fact
            let already_has_loop_fact = facts.iter().any(|f| match f {
                RuntimeFact::LoopDetected {
                    fingerprint: fp,
                    repeat_count,
                    ..
                } => fp == &fingerprint && *repeat_count >= count,
                _ => false,
            });
            if !already_has_loop_fact {
                new_facts.push(RuntimeFact::LoopDetected {
                    fingerprint,
                    repeat_count: count,
                    tool_name,
                    timestamp: now_iso.to_string(),
                });
            }

            anomalies.push(loop_result);
        }

        // 3. Failure Streak Detection
        if let Some((streak_result, failures, last_tool)) =
            check_failure_streak(facts, config.max_failure_streak)
        {
            warning_count += 1;
            if health == RunHealth::Healthy || health == RunHealth::Warning {
                health = RunHealth::Degraded;
                health_reason = Some(streak_result.reason.clone());
            }

            let already_streak_fact = facts.iter().any(|f| match f {
                RuntimeFact::ToolFailureStreakDetected {
                    consecutive_failures,
                    ..
                } => *consecutive_failures >= failures,
                _ => false,
            });
            if !already_streak_fact {
                new_facts.push(RuntimeFact::ToolFailureStreakDetected {
                    consecutive_failures: failures,
                    last_tool_name: last_tool,
                    timestamp: now_iso.to_string(),
                });
            }

            let steering_count = count_steering_facts(facts);
            if steering_count < config.max_steering_nudges && steering_nudge.is_none() {
                steering_nudge = Some(
                    "连续工具执行失败。请检查依赖、入参或环境，重新规划执行路径。".to_string(),
                );
            }

            anomalies.push(streak_result);
        }

        // 4. Optional tool/subagent limit detection. Liveness and failure
        // diagnostics remain enabled by default; no total-duration budget is
        // applied to Work runs.
        let mut budget_view = WorkRunBudgetView::default();

        // 4a. Max Automated Steps / Tool Count
        let tool_count = facts
            .iter()
            .filter(|f| matches!(f, RuntimeFact::ToolProposed { .. }))
            .count() as u32;
        budget_view.tool_calls_used = Some(tool_count);

        if let Some(max_steps) = config.max_automated_steps {
            budget_view.tool_calls_limit = Some(max_steps);
            if tool_count >= max_steps {
                health = RunHealth::NeedsAttention;
                let reason = format!(
                    "工具调用总次数（{} 次）已达到最大步骤预算（{} 次）",
                    tool_count, max_steps
                );
                health_reason = Some(reason.clone());
                anomalies.push(GuardianEvaluationResult {
                    anomaly_kind: GuardianAnomalyKind::BudgetExceeded,
                    step_id: find_current_step_id(facts),
                    tool_call_id: None,
                    reason,
                    threshold: Some(format!("max_automated_steps={max_steps}")),
                    suggested_action: Some("检查计划完成度或手动批准更多步骤".to_string()),
                    is_critical: true,
                });

                let already_exceeded = facts.iter().any(|f| match f {
                    RuntimeFact::BudgetExceeded { budget_kind, .. } => budget_kind == "tool_calls",
                    _ => false,
                });
                if !already_exceeded {
                    new_facts.push(RuntimeFact::BudgetExceeded {
                        budget_kind: "tool_calls".to_string(),
                        used: tool_count as u64,
                        limit: max_steps as u64,
                        timestamp: now_iso.to_string(),
                    });
                }
            } else if tool_count >= (max_steps * 8 / 10) {
                warning_count += 1;
                if health == RunHealth::Healthy {
                    health = RunHealth::Warning;
                    health_reason = Some(format!(
                        "工具调用总次数（{} 次）已接近预算上限（{} 次）",
                        tool_count, max_steps
                    ));
                }

                let already_warned = facts.iter().any(|f| match f {
                    RuntimeFact::BudgetWarning { budget_kind, .. } => budget_kind == "tool_calls",
                    _ => false,
                });
                if !already_warned {
                    new_facts.push(RuntimeFact::BudgetWarning {
                        budget_kind: "tool_calls".to_string(),
                        used: tool_count as u64,
                        limit: max_steps as u64,
                        timestamp: now_iso.to_string(),
                    });
                }
            }
        }

        // 4b. Subagent Spawns Budget
        let subagent_count = facts
            .iter()
            .filter(|f| matches!(f, RuntimeFact::SubagentSpawned { .. }))
            .count() as u32;
        budget_view.subagent_spawns_used = Some(subagent_count);

        if let Some(max_subagents) = config.max_subagent_spawns {
            budget_view.subagent_spawns_limit = Some(max_subagents);
            if subagent_count >= max_subagents {
                health = RunHealth::NeedsAttention;
                let reason = format!(
                    "子代理委派总数（{} 个）已达到最大预算（{} 个）",
                    subagent_count, max_subagents
                );
                health_reason = Some(reason.clone());
                anomalies.push(GuardianEvaluationResult {
                    anomaly_kind: GuardianAnomalyKind::BudgetExceeded,
                    step_id: find_current_step_id(facts),
                    tool_call_id: None,
                    reason,
                    threshold: Some(format!("max_subagent_spawns={max_subagents}")),
                    suggested_action: Some("避免继续委派新子代理，复用现有子任务结果".to_string()),
                    is_critical: true,
                });

                let already_exceeded = facts.iter().any(|f| match f {
                    RuntimeFact::BudgetExceeded { budget_kind, .. } => {
                        budget_kind == "subagent_spawns"
                    }
                    _ => false,
                });
                if !already_exceeded {
                    new_facts.push(RuntimeFact::BudgetExceeded {
                        budget_kind: "subagent_spawns".to_string(),
                        used: subagent_count as u64,
                        limit: max_subagents as u64,
                        timestamp: now_iso.to_string(),
                    });
                }
            }
        }

        let is_any_exceeded = anomalies
            .iter()
            .any(|a| a.anomaly_kind == GuardianAnomalyKind::BudgetExceeded);
        let is_any_warning = (budget_view.tool_calls_used.is_some()
            && budget_view.tool_calls_limit.is_some()
            && budget_view.tool_calls_used.unwrap_or(0)
                >= (budget_view.tool_calls_limit.unwrap_or(u32::MAX) * 8 / 10))
            || (budget_view.subagent_spawns_used.is_some()
                && budget_view.subagent_spawns_limit.is_some()
                && budget_view.subagent_spawns_used.unwrap_or(0)
                    >= (budget_view.subagent_spawns_limit.unwrap_or(u32::MAX) * 8 / 10));

        budget_view.is_any_exceeded = is_any_exceeded;
        budget_view.is_any_warning = is_any_warning && !is_any_exceeded;

        // 5. Subagent Health Detection
        if let Some(subagent_anomaly) = check_subagent_health(facts, run_status) {
            warning_count += 1;
            if health == RunHealth::Healthy || health == RunHealth::Warning {
                health = RunHealth::NeedsAttention;
                health_reason = Some(subagent_anomaly.reason.clone());
            }
            anomalies.push(subagent_anomaly);
        }

        // 6. Provider Degraded Detection
        if let Some(provider_result) = check_provider_degraded(facts) {
            warning_count += 1;
            if health == RunHealth::Healthy || health == RunHealth::Warning {
                health = RunHealth::Degraded;
                health_reason = Some(provider_result.reason.clone());
            }
            anomalies.push(provider_result);
        }

        // 7. Runtime Liveness Evaluation
        let resolved_liveness = current_liveness.unwrap_or_else(|| {
            if health == RunHealth::Stalled {
                RuntimeLiveness::AliveButStalled
            } else {
                RuntimeLiveness::Alive
            }
        });

        // 8. Record health / liveness state transitions into durable facts
        let last_recorded_health = facts.iter().rev().find_map(|f| match f {
            RuntimeFact::RunHealthChanged { current, .. } => Some(*current),
            _ => None,
        });
        if let Some(last) = last_recorded_health {
            if last != health {
                new_facts.push(RuntimeFact::RunHealthChanged {
                    previous: last,
                    current: health,
                    reason: health_reason
                        .clone()
                        .unwrap_or_else(|| format!("{:?}", health).to_lowercase()),
                    timestamp: now_iso.to_string(),
                });
            }
        } else if health != RunHealth::Healthy {
            new_facts.push(RuntimeFact::RunHealthChanged {
                previous: RunHealth::Healthy,
                current: health,
                reason: health_reason
                    .clone()
                    .unwrap_or_else(|| format!("{:?}", health).to_lowercase()),
                timestamp: now_iso.to_string(),
            });
        }

        let last_recorded_liveness = facts.iter().rev().find_map(|f| match f {
            RuntimeFact::RuntimeLivenessChanged { current, .. } => Some(current.as_str()),
            _ => None,
        });
        let current_liveness_str = match resolved_liveness {
            RuntimeLiveness::Alive => "alive",
            RuntimeLiveness::AliveButStalled => "alive_but_stalled",
            RuntimeLiveness::Disconnected => "disconnected",
            RuntimeLiveness::Dead => "dead",
        };
        if let Some(last) = last_recorded_liveness {
            if last != current_liveness_str {
                new_facts.push(RuntimeFact::RuntimeLivenessChanged {
                    previous: last.to_string(),
                    current: current_liveness_str.to_string(),
                    reason: health_reason
                        .clone()
                        .unwrap_or_else(|| current_liveness_str.to_string()),
                    timestamp: now_iso.to_string(),
                });
            }
        } else if resolved_liveness != RuntimeLiveness::Alive {
            new_facts.push(RuntimeFact::RuntimeLivenessChanged {
                previous: "alive".to_string(),
                current: current_liveness_str.to_string(),
                reason: health_reason
                    .clone()
                    .unwrap_or_else(|| current_liveness_str.to_string()),
                timestamp: now_iso.to_string(),
            });
        }

        // Deduplicate anomaly results against already recorded facts
        let deduped_anomalies = anomalies
            .into_iter()
            .filter(|candidate| {
                !facts.iter().any(|fact| match fact {
                    RuntimeFact::GuardianAnomalyDetected {
                        anomaly_kind,
                        tool_call_id,
                        step_id,
                        ..
                    } => {
                        *anomaly_kind == candidate.anomaly_kind
                            && tool_call_id.as_deref() == candidate.tool_call_id.as_deref()
                            && step_id.as_deref() == candidate.step_id.as_deref()
                    }
                    _ => false,
                })
            })
            .collect();

        GuardianHealthReport {
            health,
            reason: health_reason,
            anomalies: deduped_anomalies,
            new_facts,
            steering_nudge,
            warning_count,
            stalled_since,
            last_meaningful_progress_at: last_meaningful_at,
            budget_view,
            liveness: resolved_liveness,
        }
    }
}

/// Canonical tool fingerprint normalization:
/// Parses arguments JSON, strips volatile keys (timestamps, uuid, request IDs),
/// sorts keys recursively, and formats canonically.
pub fn canonical_tool_fingerprint(
    tool_name: &str,
    raw_args_or_hash: &str,
    target: Option<&str>,
) -> String {
    let normalized_args = if let Ok(val) = serde_json::from_str::<Value>(raw_args_or_hash) {
        normalize_json_value(&val)
    } else {
        raw_args_or_hash.trim().to_string()
    };
    let normalized_target = target
        .map(|t| t.trim().to_ascii_lowercase())
        .unwrap_or_default();
    format!("{tool_name}:{normalized_args}:{normalized_target}")
}

fn normalize_json_value(val: &Value) -> String {
    match val {
        Value::Object(map) => {
            let mut sorted = BTreeMap::new();
            for (k, v) in map {
                let lower_k = k.to_ascii_lowercase();
                if is_volatile_key(&lower_k) {
                    continue;
                }
                sorted.insert(k.clone(), normalize_json_value(v));
            }
            let pairs: Vec<String> = sorted
                .into_iter()
                .map(|(k, v)| format!("\"{k}\":{v}"))
                .collect();
            format!("{{{}}}", pairs.join(","))
        }
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(normalize_json_value).collect();
            format!("[{}]", items.join(","))
        }
        Value::String(s) => format!("\"{}\"", s.trim()),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
    }
}

fn is_volatile_key(k: &str) -> bool {
    matches!(
        k,
        "timestamp"
            | "time"
            | "current_time"
            | "now"
            | "created_at"
            | "updated_at"
            | "request_id"
            | "req_id"
            | "id"
            | "uuid"
            | "nonce"
            | "_t"
            | "trace_id"
            | "span_id"
            | "session_id"
    )
}

fn parse_iso_or_utc(iso: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(iso)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or(DateTime::UNIX_EPOCH)
}

/// Identifies whether a fact constitutes genuine meaningful task progress.
pub fn is_meaningful_progress_fact(fact: &RuntimeFact) -> bool {
    match fact {
        RuntimeFact::StepStarted { .. }
        | RuntimeFact::StepEnded { .. }
        | RuntimeFact::RunStarted { .. }
        | RuntimeFact::TurnStarted { .. }
        | RuntimeFact::TurnEnded { .. }
        | RuntimeFact::SubagentCompleted { .. }
        | RuntimeFact::SubagentFailed { .. }
        | RuntimeFact::SubagentSpawned { .. }
        | RuntimeFact::SteeringClaimed { .. } => true,
        RuntimeFact::ToolResult { success, .. } => *success,
        RuntimeFact::RecoveryResolved { .. } => true,
        _ => false,
    }
}

fn find_last_meaningful_progress_timestamp(facts: &[RuntimeFact]) -> Option<String> {
    facts.iter().rev().find_map(|fact| {
        if is_meaningful_progress_fact(fact) {
            match fact {
                RuntimeFact::StepStarted { timestamp, .. }
                | RuntimeFact::StepEnded { timestamp, .. }
                | RuntimeFact::RunStarted { timestamp, .. }
                | RuntimeFact::TurnStarted { timestamp, .. }
                | RuntimeFact::TurnEnded { timestamp, .. }
                | RuntimeFact::ToolResult { timestamp, .. }
                | RuntimeFact::SubagentCompleted { timestamp, .. }
                | RuntimeFact::SubagentFailed { timestamp, .. }
                | RuntimeFact::SubagentSpawned { timestamp, .. }
                | RuntimeFact::SteeringClaimed { timestamp, .. }
                | RuntimeFact::RecoveryResolved { timestamp, .. } => Some(timestamp.clone()),
                _ => None,
            }
        } else {
            None
        }
    })
}

fn find_current_step_id(facts: &[RuntimeFact]) -> Option<String> {
    let mut current = None;
    for fact in facts {
        match fact {
            RuntimeFact::StepStarted { step_id, .. } => current = Some(step_id.clone()),
            RuntimeFact::StepEnded { step_id, .. }
                if current.as_deref() == Some(step_id.as_str()) =>
            {
                current = None;
            }
            _ => {}
        }
    }
    current
}

fn find_active_tool_call_id(facts: &[RuntimeFact]) -> Option<String> {
    let mut completed_tools = std::collections::HashSet::new();
    for fact in facts.iter().rev() {
        match fact {
            RuntimeFact::ToolResult { tool_call_id, .. } => {
                completed_tools.insert(tool_call_id.as_str());
            }
            RuntimeFact::ToolProposed { tool_call_id, .. }
            | RuntimeFact::ToolStarted { tool_call_id, .. }
                if !completed_tools.contains(tool_call_id.as_str()) =>
            {
                return Some(tool_call_id.clone());
            }
            _ => {}
        }
    }
    None
}

fn find_current_tool_call_id(facts: &[RuntimeFact]) -> Option<String> {
    facts.iter().rev().find_map(|fact| match fact {
        RuntimeFact::ToolProposed { tool_call_id, .. }
        | RuntimeFact::ToolStarted { tool_call_id, .. } => Some(tool_call_id.clone()),
        _ => None,
    })
}

fn count_steering_facts(facts: &[RuntimeFact]) -> u32 {
    facts
        .iter()
        .filter(|f| matches!(f, RuntimeFact::SteeringQueued { .. }))
        .count() as u32
}

pub fn is_polling_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "desktop_observe"
            | "desktop_screenshot"
            | "observe_ui"
            | "inspect_ui"
            | "browser_snapshot"
            | "browser_wait_for"
            | "work_agent_status"
            | "work_agent_wait"
    )
}

fn check_duplicate_tool_streak(
    facts: &[RuntimeFact],
    max_duplicate_calls: u32,
) -> Option<(GuardianEvaluationResult, u32, String, String)> {
    if max_duplicate_calls == 0 {
        return None;
    }

    let proposed: Vec<(String, String, String)> = facts
        .iter()
        .filter_map(|fact| match fact {
            RuntimeFact::ToolProposed {
                tool_call_id,
                tool_name,
                arguments_hash,
                expected_outputs,
                ..
            } => {
                let target = expected_outputs.first().map(String::as_str);
                let fp = canonical_tool_fingerprint(tool_name, arguments_hash, target);
                Some((tool_call_id.clone(), tool_name.clone(), fp))
            }
            _ => None,
        })
        .collect();

    let last = proposed.last()?;
    let effective_max = if is_polling_tool(&last.1) {
        max_duplicate_calls.max(60)
    } else {
        max_duplicate_calls
    };

    if proposed.len() < effective_max as usize {
        return None;
    }

    let mut streak = 0u32;
    for item in proposed.iter().rev() {
        if item.2 == last.2 {
            streak += 1;
        } else {
            break;
        }
    }

    if streak >= effective_max {
        Some((
            GuardianEvaluationResult {
                anomaly_kind: GuardianAnomalyKind::DuplicateTool,
                step_id: find_current_step_id(facts),
                tool_call_id: Some(last.0.clone()),
                reason: format!(
                    "工具 '{}' 连续以等价参数调用了 {} 次（阈值：{} 次）",
                    last.1, streak, effective_max
                ),
                threshold: Some(format!("max_duplicate_calls={effective_max}")),
                suggested_action: Some("停止重复调用相同工具，检查是否陷入循环逻辑".to_string()),
                is_critical: streak >= effective_max + 2,
            },
            streak,
            last.2.clone(),
            last.1.clone(),
        ))
    } else {
        None
    }
}

fn check_failure_streak(
    facts: &[RuntimeFact],
    max_failure_streak: u32,
) -> Option<(GuardianEvaluationResult, u32, String)> {
    if max_failure_streak == 0 {
        return None;
    }

    let mut failure_streak = 0u32;
    let mut last_call_id = None;
    let mut last_error = None;
    let mut last_tool_name = "unknown_tool".to_string();

    // Map tool_call_id to tool_name
    let mut tool_names = BTreeMap::new();
    for fact in facts {
        if let RuntimeFact::ToolProposed {
            tool_call_id,
            tool_name,
            ..
        } = fact
        {
            tool_names.insert(tool_call_id.clone(), tool_name.clone());
        }
    }

    for fact in facts.iter().rev() {
        if let RuntimeFact::ToolResult {
            tool_call_id,
            success,
            error,
            ..
        } = fact
        {
            if *success {
                // Success strictly resets the failure streak
                break;
            } else {
                failure_streak += 1;
                if last_call_id.is_none() {
                    last_call_id = Some(tool_call_id.clone());
                    last_error = error.clone();
                    if let Some(name) = tool_names.get(tool_call_id) {
                        last_tool_name = name.clone();
                    }
                }
            }
        }
    }

    if failure_streak >= max_failure_streak {
        let err_suffix = last_error
            .map(|e| format!("；最近一次错误：{e}"))
            .unwrap_or_default();
        Some((
            GuardianEvaluationResult {
                anomaly_kind: GuardianAnomalyKind::ToolFailureStreak,
                step_id: find_current_step_id(facts),
                tool_call_id: last_call_id,
                reason: format!(
                    "连续工具执行失败已达 {} 次（阈值：{} 次）{}",
                    failure_streak, max_failure_streak, err_suffix
                ),
                threshold: Some(format!("max_failure_streak={max_failure_streak}")),
                suggested_action: Some(
                    "检查工具入参、依赖或环境，并在修复后通过恢复继续".to_string(),
                ),
                is_critical: true,
            },
            failure_streak,
            last_tool_name,
        ))
    } else {
        None
    }
}

fn check_subagent_health(
    facts: &[RuntimeFact],
    parent_status: WorkRunStatus,
) -> Option<GuardianEvaluationResult> {
    // If parent is terminal, check if any subagent spawned without terminal fact
    if !parent_status.is_active() {
        let mut uncompleted = Vec::new();
        for fact in facts {
            match fact {
                RuntimeFact::SubagentSpawned { agent_id, role, .. } => {
                    uncompleted.push((agent_id.clone(), role.clone()));
                }
                RuntimeFact::SubagentCompleted { agent_id, .. }
                | RuntimeFact::SubagentFailed { agent_id, .. }
                | RuntimeFact::SubagentInterrupted { agent_id, .. } => {
                    uncompleted.retain(|(id, _)| id != agent_id);
                }
                _ => {}
            }
        }
        if !uncompleted.is_empty() {
            let names: Vec<String> = uncompleted
                .into_iter()
                .map(|(id, r)| format!("{r}({id})"))
                .collect();
            return Some(GuardianEvaluationResult {
                anomaly_kind: GuardianAnomalyKind::SubagentHealth,
                step_id: find_current_step_id(facts),
                tool_call_id: None,
                reason: format!("父任务已终态，但仍有子代理未完成：{}", names.join("、")),
                threshold: Some("orphan_subagent".to_string()),
                suggested_action: Some("子任务需要被中断或等待终态记录".to_string()),
                is_critical: true,
            });
        }
    }
    None
}

fn check_provider_degraded(facts: &[RuntimeFact]) -> Option<GuardianEvaluationResult> {
    let mut consecutive_failures = Vec::new();
    for fact in facts.iter().rev() {
        if let RuntimeFact::ToolResult {
            success,
            error,
            failure_kind,
            tool_call_id,
            ..
        } = fact
        {
            if *success {
                break;
            }
            let is_provider_issue = matches!(
                failure_kind,
                Some(crate::work::executor::ExecutionFailureKind::CapabilityFailure)
            ) && error
                .as_deref()
                .map(|e| {
                    e.contains("provider unavailable")
                        || e.contains("无法连接")
                        || e.contains("connect error")
                        || e.contains("adapter unavailable")
                })
                .unwrap_or(false);

            if is_provider_issue {
                consecutive_failures.push(tool_call_id.clone());
            } else {
                break;
            }
        }
    }

    if consecutive_failures.len() >= 2 {
        Some(GuardianEvaluationResult {
            anomaly_kind: GuardianAnomalyKind::ProviderDegraded,
            step_id: find_current_step_id(facts),
            tool_call_id: consecutive_failures.first().cloned(),
            reason: format!(
                "外部 Provider / Adapter 连续出现 {} 次连接不可用故障",
                consecutive_failures.len()
            ),
            threshold: Some("consecutive_provider_failures>=2".to_string()),
            suggested_action: Some("检查外部服务健康状态、网络连接或重试配额".to_string()),
            is_critical: true,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::models::{SideEffectClass, ToolConcurrencyClass};

    #[test]
    fn canonical_fingerprint_ignores_volatile_keys_and_key_order() {
        let json1 =
            r#"{"path":"output/file.txt","timestamp":"2026-08-28T12:00:00Z","request_id":"req-1"}"#;
        let json2 =
            r#"{"request_id":"req-2","path":"output/file.txt","timestamp":"2026-08-28T12:00:05Z"}"#;

        let fp1 = canonical_tool_fingerprint("work_read_file", json1, None);
        let fp2 = canonical_tool_fingerprint("work_read_file", json2, None);

        assert_eq!(
            fp1, fp2,
            "Fingerprints must match regardless of key order or volatile fields"
        );
    }

    #[test]
    fn detects_stall_when_running_exceeds_threshold() {
        let facts = vec![
            RuntimeFact::RunStarted {
                task_id: "task-1".to_string(),
                work_run_id: "run-1".to_string(),
                execution_context: crate::work::models::ExecutionContext::Attended,
                collaboration_mode: crate::work::models::CollaborationMode::Default,
                timestamp: "2026-08-28T12:00:00Z".to_string(),
            },
            RuntimeFact::StepStarted {
                step_id: "step-1".to_string(),
                title: Some("Initial step".to_string()),
                timestamp: "2026-08-28T12:00:01Z".to_string(),
            },
        ];

        let config = GuardianConfig {
            stall_after_ms: 30_000,
            stall_warn_after_ms: 15_000,
            ..Default::default()
        };

        // 40s later -> stall
        let report = Guardian::evaluate_health(
            &facts,
            WorkRunStatus::Running,
            &config,
            "2026-08-28T12:00:41Z",
            None,
        );
        assert_eq!(report.health, RunHealth::Stalled);
        assert_eq!(report.anomalies.len(), 1);
        assert_eq!(report.anomalies[0].anomaly_kind, GuardianAnomalyKind::Stall);
        assert_eq!(report.anomalies[0].step_id.as_deref(), Some("step-1"));
        assert!(report
            .new_facts
            .iter()
            .any(|f| matches!(f, RuntimeFact::RunStalled { .. })));

        // WaitingApproval must NOT stall
        let waiting = Guardian::evaluate(
            &facts,
            WorkRunStatus::WaitingApproval,
            &config,
            "2026-08-28T12:00:41Z",
        );
        assert!(waiting.is_empty(), "WaitingApproval must not stall");

        // When all steps have ended and no step/tool is active, it must NOT stall
        let mut completed_facts = facts.clone();
        completed_facts.push(RuntimeFact::StepEnded {
            step_id: "step-1".to_string(),
            status: "completed".to_string(),
            timestamp: "2026-08-28T12:00:05Z".to_string(),
        });
        let finished_report = Guardian::evaluate_health(
            &completed_facts,
            WorkRunStatus::Running,
            &config,
            "2026-08-28T12:05:00Z",
            None,
        );
        assert_eq!(
            finished_report.health,
            RunHealth::Healthy,
            "Finished steps awaiting next user turn must not trigger stall"
        );
        assert!(finished_report.anomalies.is_empty());
    }

    #[test]
    fn detects_duplicate_tool_streak_and_steers() {
        let mut facts = Vec::new();
        for i in 1..=3 {
            facts.push(RuntimeFact::ToolProposed {
                tool_call_id: format!("call-{i}"),
                tool_name: "work_read_file".to_string(),
                action: "read".to_string(),
                arguments_hash: r#"{"path":"test.txt"}"#.to_string(),
                expected_outputs: Vec::new(),
                side_effect_class: SideEffectClass::Read,
                concurrency_class: ToolConcurrencyClass::ParallelSafe,
                timestamp: format!("2026-08-28T12:00:0{i}Z"),
            });
        }

        let config = GuardianConfig {
            max_duplicate_calls: 3,
            ..Default::default()
        };

        let report = Guardian::evaluate_health(
            &facts,
            WorkRunStatus::Running,
            &config,
            "2026-08-28T12:00:04Z",
            None,
        );
        assert!(report.health == RunHealth::Warning || report.health == RunHealth::Stalled);
        assert!(report
            .anomalies
            .iter()
            .any(|a| a.anomaly_kind == GuardianAnomalyKind::DuplicateTool));
        assert!(report.steering_nudge.is_some());
        assert!(report
            .new_facts
            .iter()
            .any(|f| matches!(f, RuntimeFact::LoopDetected { .. })));
    }

    #[test]
    fn allows_polling_tools_without_false_positive_duplicate_loop() {
        let mut facts = Vec::new();
        for i in 1..=20 {
            facts.push(RuntimeFact::ToolProposed {
                tool_call_id: format!("call-{i}"),
                tool_name: "desktop_observe".to_string(),
                action: "observe".to_string(),
                arguments_hash: r#"{"pid":94187}"#.to_string(),
                expected_outputs: Vec::new(),
                side_effect_class: SideEffectClass::Read,
                concurrency_class: ToolConcurrencyClass::ParallelSafe,
                timestamp: format!("2026-08-28T12:00:{:02}Z", i),
            });
        }

        let config = GuardianConfig {
            max_duplicate_calls: 3,
            ..Default::default()
        };

        let report = Guardian::evaluate_health(
            &facts,
            WorkRunStatus::Running,
            &config,
            "2026-08-28T12:00:21Z",
            None,
        );
        // 20 calls of desktop_observe must NOT trigger duplicate tool warning (threshold 60)
        assert_eq!(report.health, RunHealth::Healthy);
        assert!(!report
            .anomalies
            .iter()
            .any(|a| a.anomaly_kind == GuardianAnomalyKind::DuplicateTool));
    }

    #[test]
    fn detects_failure_streak_and_resets_on_success() {
        let mut facts = Vec::new();
        for i in 1..=3 {
            facts.push(RuntimeFact::ToolResult {
                tool_call_id: format!("call-{i}"),
                success: false,
                status: "failed".to_string(),
                failure_kind: None,
                exit_code: Some(1),
                error: Some("file not found".to_string()),
                outputs: Vec::new(),
                side_effect_class: SideEffectClass::Read,
                timestamp: format!("2026-08-28T12:00:0{i}Z"),
            });
        }

        let config = GuardianConfig {
            max_failure_streak: 3,
            ..Default::default()
        };

        let report = Guardian::evaluate_health(
            &facts,
            WorkRunStatus::Running,
            &config,
            "2026-08-28T12:00:04Z",
            None,
        );
        assert_eq!(report.health, RunHealth::Degraded);
        assert!(report
            .anomalies
            .iter()
            .any(|a| a.anomaly_kind == GuardianAnomalyKind::ToolFailureStreak));

        // When a success follows, the streak resets to 0
        facts.push(RuntimeFact::ToolResult {
            tool_call_id: "call-4".to_string(),
            success: true,
            status: "success".to_string(),
            failure_kind: None,
            exit_code: Some(0),
            error: None,
            outputs: Vec::new(),
            side_effect_class: SideEffectClass::Read,
            timestamp: "2026-08-28T12:00:05Z".to_string(),
        });

        let reset_report = Guardian::evaluate_health(
            &facts,
            WorkRunStatus::Running,
            &config,
            "2026-08-28T12:00:06Z",
            None,
        );
        assert!(!reset_report
            .anomalies
            .iter()
            .any(|a| a.anomaly_kind == GuardianAnomalyKind::ToolFailureStreak));
    }

    #[test]
    fn does_not_enforce_total_duration_budget() {
        let facts = vec![RuntimeFact::RunStarted {
            task_id: "task-1".to_string(),
            work_run_id: "run-1".to_string(),
            execution_context: crate::work::models::ExecutionContext::Attended,
            collaboration_mode: crate::work::models::CollaborationMode::Default,
            timestamp: "2026-08-28T12:00:00Z".to_string(),
        }];

        let report = Guardian::evaluate_health(
            &facts,
            WorkRunStatus::Running,
            &GuardianConfig::default(),
            "2026-08-28T14:00:00Z",
            None,
        );
        assert!(report.budget_view.tool_calls_limit.is_none());
        assert!(report.budget_view.subagent_spawns_limit.is_none());
        assert!(!report.budget_view.is_any_warning);
        assert!(!report.budget_view.is_any_exceeded);
        assert!(!report
            .anomalies
            .iter()
            .any(|a| a.anomaly_kind == GuardianAnomalyKind::BudgetExceeded));
    }
}
