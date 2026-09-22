import type { PiFeatureCapabilities, PiGoalState } from "$lib/types";

export type PiGoalCommandKind = "start" | "resume" | "pause" | "clear" | "status" | "other";

/** A cold Pi composer is pending extension discovery, not unavailable. */
export function isPiFeatureAvailable(
  capabilities: PiFeatureCapabilities | null,
  feature: keyof Pick<
    PiFeatureCapabilities,
    "planAvailable" | "goalAvailable" | "permissionAvailable"
  >,
): boolean {
  return capabilities === null || capabilities[feature];
}

/**
 * A newly discovered Goal extension has no `goal-state` session entry yet.
 * Pi reports that as the store's initial `unavailable` value, but the command
 * is usable and should render as an idle Goal control in the composer.
 */
export function coercePiGoalStateForComposer(
  state: PiGoalState,
  capabilities: Pick<PiFeatureCapabilities, "goalAvailable">,
): PiGoalState {
  if (capabilities.goalAvailable && state.phase === "unavailable") {
    return { ...state, phase: "inactive" };
  }
  return state;
}

export function classifyPiGoalCommand(args: string): PiGoalCommandKind {
  const action = args.trim().split(/\s+/, 1)[0]?.toLowerCase() ?? "";
  if (action === "resume") return "resume";
  if (action === "pause") return "pause";
  if (action === "clear" || action === "stop") return "clear";
  if (action === "status") return "status";
  if (["edit", "exit"].includes(action)) return "other";
  return "start";
}

export function shouldBlockPiGoalCommand(planPhase: string, args: string): boolean {
  const kind = classifyPiGoalCommand(args);
  return planPhase === "active" && (kind === "start" || kind === "resume");
}

export function planEntryNeedsGoalPause(
  planPhase: string,
  goalPhase: string,
  args: string,
): boolean {
  return planPhase !== "active" && args.trim().toLowerCase() !== "exit" && goalPhase === "active";
}
