import type { BusEvent, CliCommand, CliModelInfo, TaskRun, TimelineEntry } from "$lib/types";
import type { SessionStore } from "$lib/stores/session-store.svelte";
import { getPiModels, loadPiModels, loadPiModelsLive } from "$lib/stores";
import { getWorkColdStartNativeCommands, normalizePiSlashText } from "$lib/utils/slash-commands";
import * as api from "$lib/api";
import {
  getDefaultAgentCapabilities,
  type EffectiveAgentCapabilities,
} from "$lib/utils/agent-capabilities";
import { getAgentDisplayName } from "$lib/utils/agent-metadata";
import { resolveManagedProviderDefaultModel } from "$lib/utils/provider-routing";

export interface WorkRuntimePreferences {
  model?: string;
  effort?: string;
}

/** Check if a Work run belongs to a legacy non-Pi provider (e.g. historical DSH/Claude/Codex). */
export function isLegacyWorkRun(run?: TaskRun | null): boolean {
  return Boolean(run?.agent && run.agent !== "pi");
}

/** Legacy Work runs are strictly read-only. */
export function isWorkRunReadOnly(run?: TaskRun | null): boolean {
  return isLegacyWorkRun(run);
}

export function getWorkAgentDisplayName(agent?: string): string {
  if (!agent || agent === "pi") return "本地 Pi";
  return getAgentDisplayName(agent);
}

export async function loadWorkModels(): Promise<void> {
  await loadPiModels();
}

export function loadWorkModelsLive(runId: string): void {
  void loadPiModelsLive(runId);
}

export function getWorkModels(): CliModelInfo[] {
  return getPiModels();
}

export function getWorkSlashCommands(session: SessionStore): CliCommand[] {
  return session.sessionCommands.length > 0
    ? session.sessionCommands
    : getWorkColdStartNativeCommands();
}

export function normalizeWorkQueuedText(text: string, commands: CliCommand[]): string {
  return normalizePiSlashText(text, commands);
}

export function isMissingPiWorkSessionError(cause: unknown): boolean {
  return /no session found matching|pi session.*(not found|required)|session_id.*(not found|required)/i.test(
    cause instanceof Error ? cause.message : String(cause),
  );
}

export function getWorkStartingLabel(): string {
  return "正在启动 Pi";
}

export async function loadWorkPreferences(): Promise<WorkRuntimePreferences> {
  const [agentSettings, userSettings] = await Promise.all([
    api.getAgentSettings("pi"),
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

export async function persistWorkEffort(effort: string): Promise<void> {
  await api.updateAgentSettings("pi", { effort });
}

export function isPiWorkRuntimeConfirmation(mode?: string): boolean {
  return mode === "pi_extension_confirm";
}

export function getWorkComposerCapabilities(session: SessionStore): EffectiveAgentCapabilities {
  if (isWorkRunReadOnly(session.run)) {
    return getDefaultAgentCapabilities("__read_only__");
  }

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

export function isWorkDiagnosticTimelineEntry(entry: TimelineEntry): boolean {
  return entry.kind === "command_output" && /^\[pi rpc stderr\]/i.test(entry.content.trim());
}

export function canCloneWorkSession(session: SessionStore): boolean {
  const isPi = (session.run?.agent ?? session.agent) === "pi";
  return Boolean(session.run?.id && isPi && session.piCapabilities?.cloneAvailable === true);
}

export function canOpenWorkSessionTree(session: SessionStore): boolean {
  const isPi = (session.run?.agent ?? session.agent) === "pi";
  return Boolean(session.run?.id && isPi && session.piCapabilities?.sessionTreeAvailable === true);
}
