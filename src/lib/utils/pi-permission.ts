/**
 * Pi's user-facing permission modes. The extension still owns the actual
 * allow/ask/deny evaluation; these values only select the policy overlay.
 */
export type PiPermissionMode = "guarded" | "accept_edits" | "auto_approve";

export interface PiPermissionApplyOptions {
  runId: string | null | undefined;
  sessionAlive: boolean;
  /** Effective protocol capability for host-driven permission changes. */
  liveControlAvailable?: boolean;
  persistMode: (mode: PiPermissionMode) => Promise<unknown>;
  setLiveMode: (runId: string, mode: PiPermissionMode) => Promise<unknown>;
}

export interface PiPermissionApplyResult {
  mode: PiPermissionMode;
  appliedToLiveSession: boolean;
}

export function normalizePiPermissionMode(mode: string): PiPermissionMode {
  switch (mode.trim()) {
    case "accept_edits":
    case "acceptEdits":
    case "auto_read":
      return "accept_edits";
    case "auto_approve":
    case "auto":
    case "auto_all":
    case "bypass":
    case "bypassPermissions":
      return "auto_approve";
    default:
      return "guarded";
  }
}

/**
 * Apply Pi's permission preference without creating a session as a side effect.
 * Persist the project preference first, then update a live session when one exists.
 */
export async function applyPiPermissionMode(
  mode: string,
  options: PiPermissionApplyOptions,
): Promise<PiPermissionApplyResult> {
  const normalized = normalizePiPermissionMode(mode);
  const appliedToLiveSession = Boolean(
    options.runId && options.sessionAlive && options.liveControlAvailable !== false,
  );

  await options.persistMode(normalized);
  if (appliedToLiveSession && options.runId) {
    await options.setLiveMode(options.runId, normalized);
  }

  return { mode: normalized, appliedToLiveSession };
}
