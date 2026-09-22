import type { BusEvent, CliCommand, CliModelInfo, TimelineEntry } from "$lib/types";
import type { SessionStore } from "$lib/stores/session-store.svelte";
import {
  getDefaultAgentCapabilities,
  type EffectiveAgentCapabilities,
} from "$lib/utils/agent-capabilities";
import type { WorkRuntimeClient, WorkRuntimeCapabilities, WorkRuntimePreferences } from "./types";

export interface FakeWorkRuntimeOptions {
  provider?: string;
  displayName?: string;
  agent?: string;
  models?: CliModelInfo[];
  preferences?: WorkRuntimePreferences;
  capabilities?: Partial<WorkRuntimeCapabilities>;
  composerCapabilities?: EffectiveAgentCapabilities;
}

/**
 * Deterministic client for the Work readiness gate.
 *
 * This fixture deliberately has no Tauri, Pi, filesystem, or network
 * dependency. It proves the Work UI consumes a runtime contract instead of
 * reaching into a provider-specific client while an adapter is being built.
 */
export class FakeWorkRuntimeClient implements WorkRuntimeClient {
  readonly provider: string;
  readonly capabilities: WorkRuntimeCapabilities;

  private readonly displayName: string;
  private readonly models: CliModelInfo[];
  private readonly composerCapabilities: EffectiveAgentCapabilities;
  private preferences: WorkRuntimePreferences;

  constructor(options: FakeWorkRuntimeOptions = {}) {
    this.provider = options.provider?.trim() || "fake";
    this.displayName = options.displayName?.trim() || "Fake Work Runtime";
    this.models = [...(options.models || [])];
    this.preferences = { ...(options.preferences || {}) };
    this.capabilities = {
      supportsResume: false,
      supportsContinuation: false,
      supportsFollowUp: true,
      supportsSubagents: false,
      supportsSlashCommands: true,
      ...options.capabilities,
    };
    this.composerCapabilities =
      options.composerCapabilities || getDefaultAgentCapabilities(options.agent || this.provider);
  }

  getDisplayName(): string {
    return this.displayName;
  }

  async loadModels(): Promise<void> {}
  loadModelsLive(_runId: string): void {}

  getModels(): CliModelInfo[] {
    return [...this.models];
  }

  getSlashCommands(session: SessionStore): CliCommand[] {
    return [...session.sessionCommands];
  }

  normalizeQueuedText(text: string, _commands: CliCommand[]): string {
    return text;
  }

  isMissingSessionError(cause: unknown): boolean {
    return /session.*(not found|required)/i.test(
      cause instanceof Error ? cause.message : String(cause),
    );
  }

  getStartingLabel(): string {
    return `正在启动 ${this.displayName}`;
  }

  getDefaultAgent(): string {
    return this.provider;
  }

  async loadRuntimePreferences(): Promise<WorkRuntimePreferences> {
    return { ...this.preferences };
  }

  async persistEffort(effort: string): Promise<void> {
    this.preferences = { ...this.preferences, effort };
  }

  isRuntimeConfirmation(_mode?: string): boolean {
    return false;
  }

  getComposerCapabilities(_session: SessionStore): EffectiveAgentCapabilities {
    return this.composerCapabilities;
  }

  isDiagnosticTimelineEntry(_entry: TimelineEntry): boolean {
    return false;
  }

  canClone(session: SessionStore): boolean {
    return Boolean(this.capabilities.supportsResume && session.run?.id);
  }

  canOpenSessionTree(session: SessionStore): boolean {
    return Boolean(this.capabilities.supportsResume && session.run?.id);
  }

  isSubagentActivityEvent(event: BusEvent): boolean {
    return (
      this.capabilities.supportsSubagents &&
      ["tool_start", "tool_end", "run_state"].includes(event.type)
    );
  }
}

export function createFakeWorkRuntimeClient(
  options: FakeWorkRuntimeOptions = {},
): WorkRuntimeClient {
  return new FakeWorkRuntimeClient(options);
}
