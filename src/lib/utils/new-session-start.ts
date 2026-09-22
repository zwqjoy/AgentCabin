export type NewSessionStartMode = "shell" | "prompt";

/**
 * Resolve how a new chat should be created before the route is opened.
 *
 * A shell start is used by Pi feature controls and slash-command discovery;
 * ordinary composer input must start a real prompt turn.
 */
export function resolveNewSessionStartMode(piShell = false): NewSessionStartMode {
  return piShell ? "shell" : "prompt";
}
