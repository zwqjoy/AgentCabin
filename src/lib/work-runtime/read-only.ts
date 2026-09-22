import type { BusEvent, CliCommand, CliModelInfo, TimelineEntry } from "$lib/types";
import type { SessionStore } from "$lib/stores/session-store.svelte";
import {
  getDefaultAgentCapabilities,
  type EffectiveAgentCapabilities,
} from "$lib/utils/agent-capabilities";
import { getAgentDisplayName } from "$lib/utils/agent-metadata";
import type { WorkRuntimeCapabilities, WorkRuntimeClient, WorkRuntimePreferences } from "./types";

export class ReadOnlyWorkRuntimeClient implements WorkRuntimeClient {
  constructor(public readonly provider: string = "unknown") {}

  readonly capabilities: WorkRuntimeCapabilities = {
    supportsResume: false,
    supportsContinuation: false,
    supportsFollowUp: false,
    supportsSubagents: false,
    supportsSlashCommands: false,
  };

  getDisplayName(): string {
    return getAgentDisplayName(this.provider);
  }

  async loadModels(): Promise<void> {}
  loadModelsLive(_runId: string): void {}
  getModels(): CliModelInfo[] {
    return [];
  }
  getSlashCommands(_session: SessionStore): CliCommand[] {
    return [];
  }
  normalizeQueuedText(text: string, _commands: CliCommand[]): string {
    return text;
  }
  isMissingSessionError(_cause: unknown): boolean {
    return false;
  }
  getStartingLabel(): string {
    return "当前运行时不受支持（只读）";
  }
  getDefaultAgent(): string {
    return this.provider;
  }
  async loadRuntimePreferences(): Promise<WorkRuntimePreferences> {
    return {};
  }
  async persistEffort(_effort: string): Promise<void> {
    throw new Error(`Runtime '${this.provider}' is read-only`);
  }
  isRuntimeConfirmation(_mode?: string): boolean {
    return false;
  }
  getComposerCapabilities(_session: SessionStore): EffectiveAgentCapabilities {
    return getDefaultAgentCapabilities("__read_only__");
  }

  isDiagnosticTimelineEntry(_entry: TimelineEntry): boolean {
    return false;
  }
  canClone(_session: SessionStore): boolean {
    return false;
  }
  canOpenSessionTree(_session: SessionStore): boolean {
    return false;
  }
  isSubagentActivityEvent(_event: BusEvent): boolean {
    return false;
  }
}

export function createReadOnlyWorkRuntimeClient(provider?: string): WorkRuntimeClient {
  return new ReadOnlyWorkRuntimeClient(provider || "unknown");
}
