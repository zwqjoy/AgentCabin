import type { PiGoalPhase } from "$lib/types";

export type ComposerFeatureAction = "goal" | "plan";

export type ComposerClearAction =
  | "pi-plan-exit"
  | "plan-mode-off"
  | "pi-goal-clear"
  | "codex-goal-clear"
  | "local-goal-clear";

/**
 * Decide whether the composer should expose its slash-command affordance.
 *
 * The slash affordance follows the agent's slash-command capability, not the current
 * catalog length. Some providers (notably Grok) advertise their native commands only
 * after a session starts, so an empty catalog during cold start must not hide the entry
 * point. AgentCabin-local actions and skill entries still do not populate the menu.
 */
export function composerSlashEnabled(
  agent: string,
  protocolSlashCommands: boolean,
  uiSlashCommandMenu: boolean,
  hasRun: boolean,
): boolean {
  void hasRun;
  // Grok's ACP command catalog arrives via available_commands_update after session/new;
  // keep its native slash affordance visible while that discovery is pending.
  return (protocolSlashCommands && uiSlashCommandMenu) || agent === "grok";
}

export function getComposerFeatureActions(options: {
  goalAvailable: boolean;
  planAvailable: boolean;
}): ComposerFeatureAction[] {
  const actions: ComposerFeatureAction[] = [];
  if (options.goalAvailable) actions.push("goal");
  if (options.planAvailable) actions.push("plan");
  return actions;
}

/**
 * Gate a Goal/Plan menu item without deadlocking Pi cold start.
 *
 * A new Pi chat must be allowed to bootstrap its shell before runtime
 * capabilities can be discovered. Once a session already exists, pending
 * discovery remains a real block so commands are not sent prematurely.
 */
export function isComposerFeatureActionBlocked(options: {
  disabled: boolean;
  discoveryPending: boolean;
  operationPending: boolean;
  allowDiscoveryPendingAction?: boolean;
}): boolean {
  return (
    options.disabled ||
    options.operationPending ||
    (options.discoveryPending && !options.allowDiscoveryPendingAction)
  );
}

export function getComposerClearAction(
  agent: string,
  feature: ComposerFeatureAction,
): ComposerClearAction {
  if (feature === "plan") return agent === "pi" ? "pi-plan-exit" : "plan-mode-off";
  if (agent === "pi") return "pi-goal-clear";
  if (agent === "codex") return "codex-goal-clear";
  return "local-goal-clear";
}

export function isPiGoalChipActive(phase: PiGoalPhase | undefined): boolean {
  return ["active", "paused", "budget_limited", "complete"].includes(phase ?? "");
}

export function composerFeatureDescription(
  feature: ComposerFeatureAction,
  active: boolean,
  pending: boolean,
): string {
  if (pending) return feature === "goal" ? "正在检测目标能力…" : "正在检测计划能力…";
  if (feature === "goal") return active ? "打开目标设置" : "设置要持续追求的目标";
  return active ? "关闭计划模式" : "开启计划模式";
}
