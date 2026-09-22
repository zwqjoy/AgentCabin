/**
 * Auto-name gating logic for chat sessions.
 *
 * Extracted as pure functions so the latch/guard semantics can be
 * unit-tested without DOM or Svelte runtime.
 */

/** Derive an auto-name from the first line of a prompt. Truncates at 40 chars. */
export function deriveAutoName(prompt: string): string {
  const firstLine = prompt.split("\n")[0].trim();
  if (!firstLine) return "";
  return firstLine.length > 40 ? firstLine.slice(0, 40) + "…" : firstLine;
}

export interface PiShellTitleInput {
  agent: string;
  prompt: string | undefined;
  name: string | undefined;
  message: string;
}

/**
 * Derive the first title for a Pi shell that was created before a slash command
 * was dispatched. Pi skill commands use that shell path, so they otherwise
 * bypass the normal prompt-based auto-naming effect.
 */
export function derivePiShellTitle(input: PiShellTitleInput): string {
  if (input.agent !== "pi") return "";
  if (input.name?.trim() || input.prompt?.trim()) return "";
  return deriveAutoName(input.message);
}

export interface AutoNameState {
  phase: string;
  runId: string | undefined;
  runName: string | undefined;
  prompt: string | undefined;
  autoNameDone: boolean;
}

/**
 * Determine whether auto-name should fire for the current state.
 *
 * Returns `{ fire: true, autoName }` when all conditions are met:
 * - phase is "idle"
 * - run exists with a prompt but no name yet
 * - auto-name has not already been attempted for this run (`autoNameDone` is false)
 */
export function shouldAutoName(state: AutoNameState): { fire: boolean; autoName?: string } {
  if (state.phase !== "idle") return { fire: false };
  if (!state.runId) return { fire: false };
  if (state.runName) return { fire: false };
  if (state.autoNameDone) return { fire: false };
  if (!state.prompt) return { fire: false };

  const autoName = deriveAutoName(state.prompt);
  if (!autoName) return { fire: false };

  return { fire: true, autoName };
}
