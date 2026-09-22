/**
 * Lightweight cache of per-agent settings needed by sidebar components.
 * Avoids threading agentSettings through 3+ component layers.
 * Loaded on startup; refreshed on every updateAgentSettings call via refreshAgent().
 */
import * as api from "$lib/api";
import { dbgWarn } from "$lib/utils/debug";

let _cache = $state<Record<string, { noSessionPersistence: boolean }>>({});
let _loaded = $state(false);
let _loading = false;

/**
 * Get cached no_session_persistence for an agent.
 * Returns true (conservative — hide resume button) if cache not yet loaded.
 */
export function getNoSessionPersistence(agent: string): boolean {
  if (!_loaded) return true; // conservative: don't show resume before cache ready
  return _cache[agent]?.noSessionPersistence ?? false;
}

/** Whether the cache has been loaded at least once. */
export function isAgentSettingsCacheLoaded(): boolean {
  return _loaded;
}

/** Load/refresh agent settings cache for known agents. */
export async function loadAgentSettingsCache(): Promise<void> {
  if (_loading) return;
  _loading = true;
  try {
    const [claude, codex, pi, grok] = await Promise.all([
      api.getAgentSettings("claude").catch(() => null),
      api.getAgentSettings("codex").catch(() => null),
      api.getAgentSettings("pi").catch(() => null),
      api.getAgentSettings("grok").catch(() => null),
    ]);
    _cache = {
      claude: { noSessionPersistence: claude?.no_session_persistence ?? false },
      codex: { noSessionPersistence: codex?.no_session_persistence ?? false },
      pi: { noSessionPersistence: pi?.no_session_persistence ?? false },
      grok: { noSessionPersistence: grok?.no_session_persistence ?? false },
    };
    _loaded = true;
  } catch (e) {
    dbgWarn("agent-settings-cache", "load failed", e);
  } finally {
    _loading = false;
  }
}

/** Refresh cache for a single agent. Call after updateAgentSettings(). */
export async function refreshAgentSettingsCache(agent: string): Promise<void> {
  try {
    const settings = await api.getAgentSettings(agent);
    _cache = {
      ..._cache,
      [agent]: { noSessionPersistence: settings?.no_session_persistence ?? false },
    };
  } catch {
    // ignore — cache stays at previous value
  }
}
