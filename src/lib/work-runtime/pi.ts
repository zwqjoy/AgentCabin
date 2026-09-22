import type { CliCommand, CliModelInfo } from "$lib/types";
import type { SessionStore } from "$lib/stores/session-store.svelte";
import { getPiModels, loadPiModels, loadPiModelsLive } from "$lib/stores";
import { getWorkColdStartNativeCommands, normalizePiSlashText } from "$lib/utils/slash-commands";
import * as api from "$lib/api";
import {
  getDefaultAgentCapabilities,
  type EffectiveAgentCapabilities,
} from "$lib/utils/agent-capabilities";
import { resolveManagedProviderDefaultModel } from "$lib/utils/provider-routing";
import type { WorkRuntimeClient, WorkRuntimeCapabilities, WorkRuntimePreferences } from "./types";

export class PiWorkRuntimeClient implements WorkRuntimeClient {
  readonly provider = "pi";
  readonly capabilities: WorkRuntimeCapabilities = {
    supportsResume: true,
    supportsContinuation: true,
    supportsFollowUp: true,
    supportsSubagents: true,
    supportsSlashCommands: true,
  };

  getDisplayName(): string {
    return "本地 Pi";
  }

  async loadModels(): Promise<void> {
    await loadPiModels();
  }

  loadModelsLive(runId: string): void {
    void loadPiModelsLive(runId);
  }

  getModels(): CliModelInfo[] {
    return getPiModels();
  }

  getSlashCommands(session: SessionStore): CliCommand[] {
    return session.sessionCommands.length > 0
      ? session.sessionCommands
      : getWorkColdStartNativeCommands();
  }

  normalizeQueuedText(text: string, commands: CliCommand[]): string {
    return normalizePiSlashText(text, commands);
  }

  isMissingSessionError(cause: unknown): boolean {
    return /no session found matching|pi session.*(not found|required)|session_id.*(not found|required)/i.test(
      cause instanceof Error ? cause.message : String(cause),
    );
  }

  getStartingLabel(): string {
    return "正在启动 Pi";
  }

  getDefaultAgent(): string {
    return "pi";
  }

  async loadRuntimePreferences(): Promise<WorkRuntimePreferences> {
    const [agentSettings, userSettings] = await Promise.all([
      api.getAgentSettings(this.provider),
      api.getUserSettings(),
    ]);
    const binding = userSettings.agent_provider_bindings?.pi;
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
        userSettings.pi_provider?.model?.trim() ||
        userSettings.default_model?.trim() ||
        undefined,
      effort: agentSettings.effort?.trim() || undefined,
    };
  }

  async persistEffort(effort: string): Promise<void> {
    await api.updateAgentSettings(this.provider, { effort });
  }

  isRuntimeConfirmation(mode?: string): boolean {
    return mode === "pi_extension_confirm";
  }

  getComposerCapabilities(session: SessionStore): EffectiveAgentCapabilities {
    const isPi = (session.run?.agent ?? session.agent) === "pi";
    const baseline = session.run ? session.capabilities : getDefaultAgentCapabilities("pi");
    if (!session.run || !isPi) return baseline;

    const live = session.piCapabilities;
    return {
      ...baseline,
      protocol: { ...baseline.protocol },
      runtime: {
        ...baseline.runtime,
        fork: baseline.runtime.fork && live?.forkAvailable === true,
        steer: baseline.runtime.steer && live?.steerAvailable === true,
        followUp: baseline.runtime.followUp && live?.followUpAvailable === true,
      },
      ui: { ...baseline.ui },
      execution: { ...baseline.execution },
    };
  }

  isDiagnosticTimelineEntry(entry: import("$lib/types").TimelineEntry): boolean {
    return entry.kind === "command_output" && /^\[pi rpc stderr\]/i.test(entry.content.trim());
  }

  canClone(session: SessionStore): boolean {
    const isPi = (session.run?.agent ?? session.agent) === "pi";
    return Boolean(session.run?.id && isPi && session.piCapabilities?.cloneAvailable === true);
  }

  canOpenSessionTree(session: SessionStore): boolean {
    const isPi = (session.run?.agent ?? session.agent) === "pi";
    return Boolean(
      session.run?.id && isPi && session.piCapabilities?.sessionTreeAvailable === true,
    );
  }

  isSubagentActivityEvent(event: import("$lib/types").BusEvent): boolean {
    return (
      event.type === "tool_start" ||
      event.type === "tool_end" ||
      event.type === "work_task_state" ||
      event.type === "run_state"
    );
  }
}

export const piWorkRuntimeClient = new PiWorkRuntimeClient();
