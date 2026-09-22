import type { RunStatus, StructuredTask } from "$lib/types";
import type {
  WorkArtifactStatus,
  WorkArtifactSummary,
  WorkProgressPhase,
  WorkRunProgressView,
} from "$lib/types/work";
import { hasWorkCompletionEvidence } from "$lib/utils/work-result";

export interface WorkProgressPhaseInput {
  sessionPhase: string;
  runStatus: RunStatus | null | undefined;
  workRunStatus?: import("$lib/types/work").WorkRunStatus | null;
  tasks: StructuredTask[];
  artifacts: Array<{ status: WorkArtifactStatus }>;
  hasPendingPermission: boolean;
  hasElicitation: boolean;
}

const TERMINAL_WORK_RUN_STATUSES = new Set(["completed", "failed", "cancelled", "skipped"]);

const TERMINAL_SESSION_STATUSES = new Set(["completed", "failed", "cancelled", "stopped"]);

export function isTerminalWorkRunStatus(status: string | null | undefined): boolean {
  return Boolean(status && TERMINAL_WORK_RUN_STATUSES.has(status));
}

export function isTerminalSessionStatus(status: string | null | undefined): boolean {
  return Boolean(status && TERMINAL_SESSION_STATUSES.has(status));
}

/**
 * Derive the user-facing Work lifecycle from the durable session and Artifact state.
 * A terminal Agent/session signal is not enough to claim a completed Work task:
 * every Artifact visible for that Run must be durably delivered first. The
 * backend WorkRun status remains authoritative when it is available, while this
 * guard prevents stale session snapshots from briefly showing a false success.
 */
export function deriveWorkProgressPhase(input: WorkProgressPhaseInput): WorkProgressPhase {
  const runStatus = input.runStatus ?? null;
  const workRunStatus = input.workRunStatus ?? null;
  const sessionPhase = input.sessionPhase;
  const tasks = input.tasks;
  const artifacts = input.artifacts;
  const hasInvalidArtifact = artifacts.some(
    (artifact) => artifact.status === "invalid" || artifact.status === "failed",
  );
  const hasUndeliveredArtifact = artifacts.some((artifact) => artifact.status !== "delivered");

  if (workRunStatus === "recoverable") return "recoverable";
  if (workRunStatus === "waiting_delivery") return "awaiting_delivery";
  if (workRunStatus === "failed") return "failed";
  if (workRunStatus === "cancelled") return "cancelled";
  if (runStatus === "failed" || sessionPhase === "failed" || hasInvalidArtifact) return "failed";
  if (runStatus === "cancelled" || sessionPhase === "cancelled") return "cancelled";
  if (runStatus === "stopped" || sessionPhase === "stopped") return "stopped";

  const completionClaimed =
    workRunStatus === "completed" || sessionPhase === "completed" || runStatus === "completed";
  if (completionClaimed) {
    // A terminal WorkRun/session must not be downgraded to an approval or
    // elicitation wait by an Inbox record that was persisted just before Stop.
    // Preserve delivery/planning truth while keeping the terminal lifecycle
    // authoritative for the user-facing state.
    if (hasUndeliveredArtifact) return "awaiting_delivery";
    const hasPlan = tasks.length > 0;
    if (!hasPlan || tasks.every((task) => task.status === "completed")) {
      return "completed";
    }
    return "planning";
  }

  if (input.hasPendingPermission) return "waiting_approval";
  if (input.hasElicitation) return "waiting_input";

  if (sessionPhase === "spawning") return "planning";
  if (sessionPhase === "running") return "running";

  if (tasks.some((task) => task.status === "in_progress")) return "planning";
  return "idle";
}

export function isAgentTurnCompleted(sessionPhase: string, runStatus?: RunStatus | null): boolean {
  return sessionPhase === "completed" || runStatus === "completed";
}

export function isWorkTaskCompleted(input: WorkProgressPhaseInput): boolean {
  return deriveWorkProgressPhase(input) === "completed";
}

export function workProgressPercent(tasks: StructuredTask[]): number | null {
  if (tasks.length === 0) return null;
  const completed = tasks.filter((task) => task.status === "completed").length;
  return Math.round((completed / tasks.length) * 100);
}

export function mapRunProgressPhaseToUIPhase(
  phase: import("$lib/types/work").WorkRunProgressPhase,
): WorkProgressPhase {
  switch (phase) {
    case "queued":
    case "planning":
      return "planning";
    case "running":
    case "researching":
    case "implementing":
    case "reviewing":
      return "running";
    case "waiting_user":
      return "waiting_approval";
    case "recoverable":
      return "recoverable";
    case "awaiting_delivery":
      return "awaiting_delivery";
    case "blocked":
    case "failed":
      return "failed";
    case "completed":
      return "completed";
    case "cancelled":
      return "cancelled";
    default:
      return "running";
  }
}

/**
 * Project a WorkRun phase conservatively for user-facing surfaces. The host
 * can finish an Agent turn before the Work task has actually completed, so a
 * completed phase must still agree with its steps, artifacts, and tool result.
 */
export function workRunProgressDisplayPhase(
  view: WorkRunProgressView,
  artifacts: WorkArtifactSummary[] = [],
): WorkProgressPhase {
  // WorkRun status is the durable lifecycle authority. A stale attention
  // record must not turn a cancelled/failed/completed run back into a
  // "waiting for you" banner after the user clicks Stop.
  if (view.runStatus === "cancelled" || view.runStatus === "skipped") return "cancelled";
  if (view.runStatus === "failed") return "failed";

  const uiPhase = mapRunProgressPhaseToUIPhase(view.phase);
  if (view.runStatus === "completed" && uiPhase !== "completed") {
    if (hasWorkCompletionEvidence({ progressView: view, artifacts })) return "completed";
    if (
      artifacts.some((artifact) => artifact.status === "invalid" || artifact.status === "failed")
    ) {
      return "failed";
    }
    if (artifacts.some((artifact) => artifact.status !== "delivered")) {
      return "awaiting_delivery";
    }
    return "idle";
  }
  if (uiPhase !== "completed" || hasWorkCompletionEvidence({ progressView: view, artifacts })) {
    return uiPhase;
  }
  if (artifacts.some((artifact) => artifact.status === "invalid" || artifact.status === "failed")) {
    return "failed";
  }
  if (artifacts.some((artifact) => artifact.status !== "delivered")) {
    return "awaiting_delivery";
  }
  if (view.toolSummary.running > 0) return "running";
  if (view.toolSummary.failed > 0) return "failed";
  return "idle";
}

export function workRunProgressToSnapshot(
  view: import("$lib/types/work").WorkRunProgressView,
  artifacts: import("$lib/types/work").WorkArtifactSummary[] = [],
  sessionError = "",
): import("$lib/types/work").WorkProgressSnapshot {
  const uiPhase = workRunProgressDisplayPhase(view, artifacts);
  const subagents: import("$lib/types/work").WorkSubagentSummary[] = view.agents.map((a) => {
    const isRunning = a.status === "running";
    const isFailed = a.status === "failed";
    const isInterrupted = a.status === "interrupted";
    const isStopped = a.status === "stopped";
    const statusText = isRunning
      ? "正在执行"
      : isFailed
        ? "执行失败"
        : isStopped
          ? "已停止"
          : isInterrupted
            ? "已中断"
            : "已完成";

    const subagent: import("$lib/types/work").WorkSubagentSummary = {
      id: a.agentId,
      agentId: a.agentId,
      role: a.role,
      task: a.resultSummary || a.error || "",
      status:
        (a.status as "running" | "completed" | "failed" | "stopped" | "interrupted") || "running",
      statusText,
    };
    if (a.resultSummary) subagent.resultSummary = a.resultSummary;
    if (a.error) subagent.error = a.error;
    return subagent;
  });
  const hasActionableAttention = !isTerminalWorkRunStatus(view.runStatus);

  return {
    phase: uiPhase,
    sessionPhase: view.runStatus === "running" ? "running" : view.runStatus,
    runStatus: (view.runStatus === "waiting_approval" ||
    view.runStatus === "waiting_input" ||
    view.runStatus === "queued"
      ? "running"
      : view.runStatus) as RunStatus,
    tasks: view.steps,
    taskState: {
      version: 1,
      revision: 0,
      goal: view.goal ?? null,
      plan: view.steps,
      checkpoint: view.checkpoint ?? null,
      updatedAt: view.updatedAt,
    },
    activeToolName: view.currentActivity?.toolName ?? "",
    pendingApprovalCount: hasActionableAttention && view.attention ? view.attention.count : 0,
    pendingAccessRoot: hasActionableAttention && view.attention?.kind === "access_root_request",
    pendingElicitation: hasActionableAttention && view.attention?.kind === "user_input",
    error: sessionError,
    toolCallCount: view.toolSummary.started,
    subagents,
  };
}

export function mapProgressViewToStatusLabel(
  view: import("$lib/types/work").WorkRunProgressView,
  artifacts: import("$lib/types/work").WorkArtifactSummary[] = [],
): string {
  const displayPhase = workRunProgressDisplayPhase(view, artifacts);
  if (displayPhase === "stopped") return "已停止";
  if (displayPhase === "cancelled") return "已取消";
  if (displayPhase === "failed") return "执行失败";
  if (displayPhase === "completed") return "任务已完成";
  if (displayPhase === "awaiting_delivery") return "等待交付物验收";

  if (view.phase === "recoverable") {
    return "可恢复，等待你的选择";
  }
  if (
    view.phase === "waiting_user" ||
    (view.attention && !isTerminalWorkRunStatus(view.runStatus))
  ) {
    return "等待你处理";
  }
  if (view.phase === "completed") {
    return displayPhase === "running" ? "正在执行…" : "等待下一步";
  }
  switch (view.phase) {
    case "planning":
      return "正在制定计划…";
    case "researching":
      return "正在调查研究…";
    case "implementing":
      return "正在执行任务…";
    case "reviewing":
      return "正在独立审查…";
    case "running":
      return "正在执行…";
    case "failed":
    case "blocked":
      return "执行失败";
    case "cancelled":
      return "已取消";
    case "queued":
      return "排队中…";
    default:
      if (view.runStatus === "running") return "正在执行…";
      if (view.runStatus === "completed") return "任务已完成";
      if (view.runStatus === "failed") return "执行失败";
      if (view.runStatus === "cancelled") return "已取消";
      return "等待下一条消息";
  }
}
