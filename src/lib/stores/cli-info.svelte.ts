import * as api from "$lib/api";
import type { CodexModel, PiModel } from "$lib/api";
import { getGrokModels as fetchGrokModels } from "$lib/grok-api";
import type { CliInfo, CliModelInfo, CliCommand, CodexModelList } from "$lib/types";
import { dbg, dbgWarn } from "$lib/utils/debug";

let _info: CliInfo | null = $state(null);
let _loading = false;
let _loaded = false;

export function getCliModels(): CliModelInfo[] {
  return _info?.models ?? [];
}

export function getCliCommands(): CliCommand[] {
  return _info?.commands ?? [];
}

/** The model currently active in Claude Code (from ~/.claude/settings.json). */
export function getCliCurrentModel(): string | undefined {
  return _info?.current_model ?? undefined;
}

export function getCliInfo_cached(): CliInfo | null {
  return _info;
}

export async function loadCliInfo(force = false): Promise<CliInfo | null> {
  if (_loaded && !force) return _info;
  if (_loading) return _info; // dedupe concurrent calls
  _loading = true;
  try {
    dbg("cli-info", "loading", { force });
    _info = await api.getCliInfo(force);
    _loaded = true;
    dbg("cli-info", "loaded", { models: _info?.models.length });
  } catch (e) {
    dbgWarn("cli-info", "failed to load", e);
  } finally {
    _loading = false;
  }
  return _info;
}

// ── Codex Models ──

// Pulled live from `codex app-server` (model/list) — see api.getCodexModels.
// No hardcoded list: new upstream models (e.g. gpt-5.5) appear without an app release.
let _codex: CodexModelList | null = $state(null);
let _codexLoading = false;
let _codexLoaded = false;

export function getCodexModels(): CliModelInfo[] {
  return _codex?.models ?? [];
}

/** The model marked `isDefault` in the live Codex catalog, if known. */
export function getCodexDefaultModel(): string | undefined {
  return _codex?.defaultModel ?? undefined;
}

// Pulled from Pi's pre-session `--list-models` and upgraded by the live RPC
// `get_available_models` response once the actor starts.
let _piModels: CliModelInfo[] = $state([]);
let _piLoading = false;
let _piLoaded = false;
let _piLiveRunId: string | null = null;
let _modelCatalogGeneration = 0;

export function getPiModels(): CliModelInfo[] {
  return _piModels;
}

/**
 * Global Provider edits change the backend-backed Pi/Grok catalogs. Clear the loaded markers so
 * the next Runtime surface fetches the new projection instead of reusing a module-level cache
 * created before the settings edit. Keep the old arrays until a refresh completes to avoid a
 * transient empty picker.
 */
export function invalidateModelCatalogs(): void {
  _modelCatalogGeneration += 1;
  _piLoaded = false;
  _piLiveRunId = null;
  _grokLoaded = false;
  dbg("models", "invalidated provider-backed model catalogs");
}

function normalizePiModels(data: PiModel[]): CliModelInfo[] {
  const seen = new Set<string>();
  const models: CliModelInfo[] = [];
  for (const model of data) {
    if (!model.id) continue;
    const value =
      model.provider && model.provider !== "agentcabin"
        ? `${model.provider}/${model.id}`
        : model.id;
    if (seen.has(value)) continue;
    seen.add(value);
    const thinkingLevels = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
    const supportedEffortLevels = model.thinkingLevelMap
      ? thinkingLevels.filter((level) => {
          const mapped = model.thinkingLevelMap?.[level];
          if (mapped === null) return false;
          return (level !== "xhigh" && level !== "max") || mapped !== undefined;
        })
      : thinkingLevels;
    models.push({
      value,
      displayName: model.name || model.id,
      description: model.provider || "Pi",
      contextWindow: model.contextWindow,
      maxTokens: model.maxTokens,
      supportsEffort: model.reasoning === true,
      supportedEffortLevels: model.reasoning ? supportedEffortLevels : undefined,
    });
  }
  return models;
}

/** Load Pi's catalog before the first session so the picker is not limited to the current model. */
export async function loadPiModels(force = false): Promise<void> {
  if ((_piLoaded && !force) || _piLoading) return;
  _piLoading = true;
  const generation = _modelCatalogGeneration;
  try {
    dbg("models", "loading pi models (pre-session)", { force });
    const models = await api.getPiModels();
    if (generation !== _modelCatalogGeneration) return;
    // A live actor is authoritative. Do not let a slower one-shot process
    // overwrite the live catalog after the session has already started.
    if (_piLiveRunId) return;
    if (models.length === 0) return;
    _piModels = models;
    _piLoaded = true;
    dbg("models", "loaded pi models (pre-session)", { models: models.length });
  } catch (e) {
    dbgWarn("models", "failed to load pi models (pre-session)", e);
  } finally {
    _piLoading = false;
    if (generation !== _modelCatalogGeneration && !_piLoading) {
      void loadPiModels(true);
    }
  }
}

export async function loadPiModelsLive(runId: string): Promise<void> {
  const generation = _modelCatalogGeneration;
  _piLiveRunId = runId;
  const retryDelays = [0, 100, 250, 500];
  for (const delay of retryDelays) {
    if (delay > 0) await new Promise((resolve) => setTimeout(resolve, delay));
    try {
      dbg("models", "loading pi models (live)", { runId, delay });
      const { data } = await api.listPiModels(runId);
      const models = normalizePiModels(data ?? []);
      if (models.length === 0) continue;
      if (_piLiveRunId !== runId || generation !== _modelCatalogGeneration) return;
      _piModels = models;
      _piLoaded = true;
      dbg("models", "loaded pi models (live)", { models: models.length });
      return;
    } catch (e) {
      if (delay === retryDelays.at(-1)) {
        dbgWarn("models", "failed to load pi models (live)", e);
      }
    }
  }
  dbgWarn("models", "live Pi model catalog returned no models", { runId });
}

// ── Grok Build Models ──

// Grok Build owns its model/provider registry. Load it independently via `grok models`
// rather than falling through Claude, Codex or Pi model discovery.
let _grokModels: CliModelInfo[] = $state([]);
let _grokLoading = false;
let _grokLoaded = false;

export function getGrokModels(): CliModelInfo[] {
  return _grokModels;
}

export async function loadGrokModels(force = false): Promise<void> {
  if ((_grokLoaded && !force) || _grokLoading) return;
  _grokLoading = true;
  const generation = _modelCatalogGeneration;
  try {
    // When Grok is in API Key mode (custom provider), show the provider's models
    // instead of Grok's own catalog.
    const settings = await api.getUserSettings();
    const binding = settings.agent_provider_bindings?.grok;
    if (binding?.mode === "custom" && binding.provider_id) {
      const provider = settings.global_providers?.find((p) => p.id === binding.provider_id);
      if (generation !== _modelCatalogGeneration) return;
      if (provider?.models && provider.models.length > 0) {
        _grokModels = provider.models.map((m) => ({
          value: m.id,
          displayName: m.name?.trim() || m.id,
          description: "",
          contextWindow: m.context_window,
          supportsEffort: m.supports_reasoning ?? (m.supported_effort_levels?.length ?? 0) > 0,
          supportedEffortLevels: m.supported_effort_levels?.length
            ? [...m.supported_effort_levels]
            : m.supports_reasoning
              ? ["low", "medium", "high", "xhigh"]
              : undefined,
        }));
        _grokLoaded = true;
        dbg("models", "loaded grok provider models", { count: _grokModels.length });
        return;
      }
    }
    // Fall back to Grok's own catalog (CLI auth mode)
    dbg("models", "loading grok models", { force });
    const models = await fetchGrokModels();
    if (generation !== _modelCatalogGeneration) return;
    if (models.length > 0) {
      _grokModels = models;
      _grokLoaded = true;
    }
    dbg("models", "loaded grok models", { models: models.length });
  } catch (e) {
    dbgWarn("models", "failed to load grok models", e);
  } finally {
    _grokLoading = false;
  }
}

/**
 * Resolve the model list to show for an agent, folding in third-party platform models.
 * Each first-class agent gets an explicit branch so adding a new runtime never silently
 * inherits another agent's model catalog.
 *
 * - Claude → Claude CLI/platform catalog.
 * - Codex → live Codex app-server catalog.
 * - Pi → Pi `--list-models` / live RPC catalog.
 * - Grok → Grok Build `grok models` catalog.
 */
export function getModelsForAgent(
  agent: string,
  opts: { platformModels?: CliModelInfo[]; liveModels?: CliModelInfo[]; merge?: boolean } = {},
): CliModelInfo[] {
  // A session-provided catalog is authoritative for every provider. This keeps
  // the picker generic and lets protocol adapters add models without a new UI
  // branch for each agent.
  if (opts.liveModels && opts.liveModels.length > 0) return opts.liveModels;
  const platform = opts.platformModels ?? [];
  if (agent === "codex") return getCodexModels();
  if (agent === "pi") {
    if (opts.merge) return [...platform, ...getPiModels()];
    return getPiModels().length > 0 ? getPiModels() : platform;
  }
  if (agent === "grok") return getGrokModels();
  if (agent === "dsh") return platform; // Legacy compatibility
  if (opts.merge) return [...platform, ...getCliModels()];
  return platform.length > 0 ? platform : getCliModels();
}

export async function loadCodexModels(force = false): Promise<CodexModelList | null> {
  if (_codexLoaded && !force) return _codex;
  if (_codexLoading) return _codex; // dedupe concurrent calls
  _codexLoading = true;
  try {
    dbg("cli-info", "loading codex models", { force });
    _codex = await api.getCodexModels(force);
    _codexLoaded = true;
    dbg("cli-info", "loaded codex models", {
      models: _codex?.models.length,
      default: _codex?.defaultModel,
    });
  } catch (e) {
    dbgWarn("cli-info", "failed to load codex models", e);
  } finally {
    _codexLoading = false;
  }
  return _codex;
}

/**
 * Normalize the raw `model/list` catalog (CodexModel[]) into our flat CliModelInfo[].
 * Mirrors the backend `map_models` (codex_control.rs) so the live and pre-session paths
 * produce identical shapes: filter hidden, prefer `model` over `id` for the --model value,
 * flatten supportedReasoningEfforts → supportedEffortLevels, capture the isDefault model.
 */
function normalizeCodexModels(data: CodexModel[]): CodexModelList {
  const models: CliModelInfo[] = [];
  let defaultModel: string | undefined;

  for (const m of data) {
    if (m.hidden) continue;
    const value = m.model || m.id;
    if (!value) continue;

    const effortLevels = (m.supportedReasoningEfforts ?? [])
      .map((e) => e.reasoningEffort)
      .filter((s): s is string => !!s);

    if (m.isDefault) defaultModel = value;

    models.push({
      value,
      displayName: m.displayName || value,
      description: m.description ?? "",
      supportsEffort: effortLevels.length > 0,
      supportedEffortLevels: effortLevels.length > 0 ? effortLevels : undefined,
    });
  }

  return { models, defaultModel };
}

/**
 * Refresh the Codex model catalog from a LIVE app-server session (control `model_list`).
 * The live session is authoritative for the model it's actually running, so its result
 * upgrades the pre-session catalog populated by loadCodexModels (same `_codex` cache, so
 * all pickers see it). Fire-and-forget; never throws. No-op if the session yields no models.
 */
export async function loadCodexModelsLive(runId: string): Promise<void> {
  try {
    dbg("models", "loading codex models (live)", { runId });
    const { data } = await api.listCodexModels(runId);
    const list = normalizeCodexModels(data ?? []);
    // Empty catalog => session not ready / old CLI without model/list. Keep the existing
    // (pre-session) cache rather than blanking the pickers.
    if (list.models.length === 0) {
      dbgWarn("models", "live codex model/list returned no models, keeping cache", { runId });
      return;
    }
    _codex = list;
    _codexLoaded = true;
    dbg("models", "loaded codex models (live)", {
      models: list.models.length,
      default: list.defaultModel,
    });
  } catch (e) {
    dbgWarn("models", "failed to load codex models (live)", e);
  }
}

// ── CLI Version Info ──

export interface CliVersionInfo {
  installed?: string;
  channel?: string;
  latest?: string;
  stable?: string;
}

let _versionInfo: CliVersionInfo | null = $state(null);
let _versionLoading = $state(false);

// ── Codex Version (global cache) ──
let _codexVersion: string | null = $state(null);
export function getCodexVersion(): string | null {
  return _codexVersion;
}

export function getCliVersionInfo_cached(): CliVersionInfo | null {
  return _versionInfo;
}

export function isCliVersionLoading(): boolean {
  return _versionLoading;
}

/** Update the cached installed version (e.g. after CLI self-updates during a session). */
export function updateInstalledVersion(version: string): void {
  if (!version || !_versionInfo) return;
  if (_versionInfo.installed === version) return;
  _versionInfo = { ..._versionInfo, installed: version };
}

export async function loadCliVersionInfo(): Promise<void> {
  if (_versionLoading) return;
  _versionLoading = true;
  try {
    dbg("cli-info", "loadCliVersionInfo");
    const [cliCheck, codexCheck, distTags, cliConfig] = await Promise.all([
      api.checkAgentCli("claude").catch(() => null),
      api.checkAgentCli("codex").catch(() => null),
      api.getCliDistTags().catch(() => ({ latest: undefined, stable: undefined })),
      api.getCliConfig().catch((): Record<string, unknown> => ({})),
    ]);

    // Cache Codex version before Claude early return
    _codexVersion = codexCheck?.version ?? null;

    if (!cliCheck?.found) {
      _versionInfo = null;
      dbg("cli-info", "loadCliVersionInfo: CLI not found");
      return;
    }

    _versionInfo = {
      installed: cliCheck.version ?? undefined,
      channel: ((cliConfig as Record<string, unknown>).autoUpdatesChannel as string) ?? undefined,
      latest: distTags.latest ?? undefined,
      stable: distTags.stable ?? undefined,
    };
    dbg("cli-info", "loadCliVersionInfo done", _versionInfo);
  } catch (e) {
    dbgWarn("cli-info", "loadCliVersionInfo failed", e);
  } finally {
    _versionLoading = false;
  }
}
