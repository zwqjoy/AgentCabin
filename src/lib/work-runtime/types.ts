import type { BusEvent, CliCommand, CliModelInfo, TimelineEntry } from "$lib/types";
import type { SessionStore } from "$lib/stores/session-store.svelte";
import type { EffectiveAgentCapabilities } from "$lib/utils/agent-capabilities";

/**
 * Compatibility default for installations that have no persisted Work
 * runtime yet. New settings and persisted Run metadata take precedence.
 */
export const DEFAULT_WORK_RUNTIME = "pi" as const;

export interface WorkRuntimeCapabilities {
  supportsResume: boolean;
  supportsContinuation: boolean;
  supportsFollowUp: boolean;
  supportsSubagents: boolean;
  supportsSlashCommands: boolean;
}

export interface WorkRuntimePreferences {
  model?: string;
  effort?: string;
}

export interface WorkRuntimeClient {
  readonly provider: string;
  readonly capabilities: WorkRuntimeCapabilities;

  getDisplayName(): string;
  loadModels(): Promise<void>;
  loadModelsLive(runId: string): void;
  getModels(): CliModelInfo[];
  getSlashCommands(session: SessionStore): CliCommand[];
  normalizeQueuedText(text: string, commands: CliCommand[]): string;
  isMissingSessionError(cause: unknown): boolean;
  getStartingLabel(): string;
  getDefaultAgent(): string;
  loadRuntimePreferences(): Promise<WorkRuntimePreferences>;
  persistEffort(effort: string): Promise<void>;
  isRuntimeConfirmation(mode?: string): boolean;

  getComposerCapabilities(session: SessionStore): EffectiveAgentCapabilities;
  isDiagnosticTimelineEntry(entry: TimelineEntry): boolean;
  canClone(session: SessionStore): boolean;
  canOpenSessionTree(session: SessionStore): boolean;
  isSubagentActivityEvent(event: BusEvent): boolean;
}
