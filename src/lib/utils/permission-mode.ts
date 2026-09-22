import { normalizePiPermissionMode } from "./pi-permission";

/**
 * Resolve the permission policy that must be attached to the next process/message
 * startup. The UI stores Claude/Codex/Grok modes as CLI names; Pi has its own
 * permission extension policy.
 *
 * An explicit override is used by session-scoped flows such as ExitPlanMode and
 * always wins over the current composer value. Returning undefined for a generic
 * agent with no loaded mode deliberately leaves the persisted backend setting in
 * charge instead of guessing a different policy during settings refresh.
 */
export function resolveStartupPermissionMode(
  agent: string,
  genericMode: string,
  piMode: string,
  explicitOverride?: string,
): string | undefined {
  const requested =
    explicitOverride?.trim() || (agent === "pi" ? piMode.trim() : genericMode.trim());

  if (agent === "pi") {
    return normalizePiPermissionMode(requested);
  }
  if (!requested) return undefined;

  // Accept both app names (settings/API) and CLI names (store/session actors).
  switch (requested) {
    case "ask":
    case "ask-all":
    case "ask_all":
      return "default";
    case "auto_read":
      return "acceptEdits";
    case "auto_all":
    case "auto-grant":
    case "auto_grant":
      return "bypassPermissions";
    case "dont_ask":
      return "dontAsk";
    case "delegate":
      return "acceptEdits";
    default:
      return requested;
  }
}
