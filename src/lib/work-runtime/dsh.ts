import type { BusEvent, CliCommand, CliModelInfo, TimelineEntry } from "$lib/types";
import type { SessionStore } from "$lib/stores/session-store.svelte";
import * as api from "$lib/api";
import {
  getDefaultAgentCapabilities,
  type EffectiveAgentCapabilities,
} from "$lib/utils/agent-capabilities";
import { resolveManagedProviderDefaultModel } from "$lib/utils/provider-routing";
import type { WorkRuntimeClient, WorkRuntimeCapabilities, WorkRuntimePreferences } from "./types";

/**
 * Work's DSH adapter only owns the runtime-facing contract. Models are supplied
 * by the managed global-provider catalog in WorkChatSurface; every Work tool
 * still goes through the authenticated Host bridge.
 */
export class DshWorkRuntimeClient implements WorkRuntimeClient {
  readonly provider = "dsh";
  readonly capabilities: WorkRuntimeCapabilities = {
    supportsResume: true,
    supportsContinuation: true,
    supportsFollowUp: true,
    supportsSubagents: false,
    supportsSlashCommands: false,
  };

  getDisplayName(): string {
    return "DeepSeek Harness (DSH)";
  }

  async loadModels(): Promise<void> {
    // DSH receives its model route from the Work-managed provider projection.
  }

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

  isMissingSessionError(cause: unknown): boolean {
    return /dsh session.*(not found|required)|session.*(not found|required)/i.test(
      cause instanceof Error ? cause.message : String(cause),
    );
  }

  getStartingLabel(): string {
    return "正在启动 DSH";
  }

  getDefaultAgent(): string {
    return "dsh";
  }

  async loadRuntimePreferences(): Promise<WorkRuntimePreferences> {
    const [agentSettings, userSettings] = await Promise.all([
      api.getAgentSettings(this.provider),
      api.getUserSettings(),
    ]);
    const binding = userSettings.agent_provider_bindings?.dsh;
    let bindingModel = "";
    if (binding?.mode === "custom") {
      const provider = binding.provider_id
        ? userSettings.global_providers?.find((item) => item.id === binding.provider_id)
        : undefined;
      bindingModel = resolveManagedProviderDefaultModel(
        binding,
        provider?.models?.map((item) => item.id) ?? [],
      );
    }
    return {
      model:
        bindingModel ||
        agentSettings.model?.trim() ||
        userSettings.default_model?.trim() ||
        undefined,
      effort: agentSettings.effort?.trim() || undefined,
    };
  }

  async persistEffort(effort: string): Promise<void> {
    await api.updateAgentSettings(this.provider, { effort });
  }

  isRuntimeConfirmation(_mode?: string): boolean {
    // Work approvals are emitted by ToolPipeline/Inbox, not by the DSH SDK.
    return false;
  }

  getComposerCapabilities(_session: SessionStore): EffectiveAgentCapabilities {
    return getDefaultAgentCapabilities(this.provider);
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

export const dshWorkRuntimeClient = new DshWorkRuntimeClient();
