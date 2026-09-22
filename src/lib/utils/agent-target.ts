import type { AgentTarget, TaskRun } from "$lib/types";
import type { AppMode } from "$lib/types/work";

export type { AgentTarget };

type RunIdentity = Partial<Pick<TaskRun, "agent_target" | "app_mode" | "agent" | "workspace_id">>;

export const NATIVE_TARGETS: readonly AgentTarget[] = [
  "native:codex",
  "native:claude",
  "native:grok",
];

export const PI_TARGETS: readonly AgentTarget[] = ["pi:code"];

export const WORK_TARGET = "work" as const;

/** Legacy wire target accepted for old persisted Work runs. */
export const LEGACY_WORK_TARGET = "pi:work" as const;

export function isWorkTarget(target: AgentTarget): boolean {
  return target === WORK_TARGET || target === LEGACY_WORK_TARGET;
}

function canonicalizeTarget(target: AgentTarget): AgentTarget {
  return target === LEGACY_WORK_TARGET ? WORK_TARGET : target;
}

/**
 * Derive the canonical AgentTarget for a run, with backwards-compatible fallback for legacy runs.
 */
export function getAgentTarget(run: RunIdentity): AgentTarget {
  if (run.app_mode === "work") {
    return WORK_TARGET;
  }
  if (run.app_mode === "code" && run.agent_target && !isWorkTarget(run.agent_target)) {
    return canonicalizeTarget(run.agent_target);
  }
  if (run.app_mode === undefined && run.agent_target) {
    return canonicalizeTarget(run.agent_target);
  }
  switch (run.agent) {
    case "pi":
      return "pi:code";
    case "codex":
      return "native:codex";
    case "grok":
      return "native:grok";
    default:
      return "native:claude";
  }
}

/**
 * Resolve the product mode from the run identity.
 *
 * `app_mode` is authoritative for new and migrated runs. The target fallback
 * only preserves filtering for older records that predate the explicit mode.
 */
export function isWorkRun(run: RunIdentity): boolean {
  if (run.app_mode !== undefined) return run.app_mode === "work";
  return run.agent_target !== undefined && isWorkTarget(run.agent_target);
}

export function getRunAppMode(run: RunIdentity): AppMode {
  return isWorkRun(run) ? "work" : "code";
}

export function getRunRuntime(run: RunIdentity): string {
  if (run.agent) return run.agent;
  const target = getAgentTarget(run);
  switch (target) {
    case "native:codex":
      return "codex";
    case "native:claude":
      return "claude";
    case "native:grok":
      return "grok";
    case "pi:code":
    case "pi:work":
    default:
      return "pi";
  }
}

export function isNativeTarget(target: AgentTarget): boolean {
  return target.startsWith("native:");
}

export function isPiTarget(target: AgentTarget): boolean {
  return target === "pi:code";
}

export function getTargetRoute(
  target: AgentTarget,
  options?: { runId?: string; workspaceId?: string; resume?: string },
): string {
  const query = new URLSearchParams();
  if (options?.runId) {
    query.set("run", options.runId);
  }
  if (options?.resume) {
    query.set("resume", options.resume);
  }

  switch (target) {
    case "pi:work":
    case "work":
      if (options?.workspaceId) {
        query.set("workspace", options.workspaceId);
      }
      return query.toString() ? `/chat/work?${query.toString()}` : "/chat/work";

    case "pi:code":
      return query.toString() ? `/chat/pi?${query.toString()}` : "/chat/pi";

    case "native:codex":
      if (!options?.runId) query.set("agent", "codex");
      return query.toString() ? `/chat?${query.toString()}` : "/chat";

    case "native:grok":
      if (!options?.runId) query.set("agent", "grok");
      return query.toString() ? `/chat?${query.toString()}` : "/chat";

    case "native:claude":
    default:
      return query.toString() ? `/chat?${query.toString()}` : "/chat";
  }
}

/** Build a route from product identity first, preserving legacy target routing. */
export function getRunRoute(
  run: RunIdentity,
  options?: { runId?: string; workspaceId?: string; resume?: string },
): string {
  const workRun = isWorkRun(run);
  const target = workRun ? WORK_TARGET : getAgentTarget(run);
  const workspaceId = options?.workspaceId ?? (workRun ? run.workspace_id : undefined);
  return getTargetRoute(target, { ...options, workspaceId });
}

export function getConversationDeleteFallbackRoute(target: AgentTarget): string {
  return getTargetRoute(target);
}

export function getTargetBadgeName(target: AgentTarget): string {
  switch (target) {
    case "native:codex":
      return "Codex";
    case "native:claude":
      return "Claude Code";
    case "native:grok":
      return "Grok";
    case "pi:code":
      return "Pi Code";
    case "work":
      return "Work";
    case "pi:work":
      return "Work";
  }
}
