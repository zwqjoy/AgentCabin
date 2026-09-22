import type { AgentTarget } from "$lib/types";
import type { AppMode } from "$lib/types/work";
import { isPiTarget, isWorkTarget } from "$lib/utils/agent-target";

export type AppRealm = "native" | "pi";
export type PiSubMode = "code" | "work";

export const CODE_MODE_PATH = "/chat";
export const NATIVE_MODE_PATH = "/chat";
export const PI_CODE_PATH = "/chat/pi";
export const WORK_MODE_PATH = "/chat/work";

export function getAgentTargetFromUrl(url: URL): AgentTarget {
  const { pathname, searchParams } = url;
  if (
    pathname.startsWith(WORK_MODE_PATH) ||
    searchParams.get("mode") === "work" ||
    (pathname.startsWith("/chat/plugins") &&
      (searchParams.get("scope") === "work" || searchParams.get("scope") === "pi-work"))
  ) {
    return "work";
  }
  if (
    pathname.startsWith(PI_CODE_PATH) ||
    (pathname.startsWith("/chat/plugins") &&
      (searchParams.get("scope") === "pi-code" || searchParams.get("realm") === "pi"))
  ) {
    return "pi:code";
  }
  const agentParam = searchParams.get("agent");
  if (agentParam === "codex") return "native:codex";
  if (agentParam === "grok") return "native:grok";
  if (agentParam === "claude") return "native:claude";
  if (agentParam === "pi") return "pi:code";

  if (!pathname.startsWith("/chat")) {
    const savedMode = getSavedAppMode();
    if (savedMode === "work") return "work";
    const savedRealm = getSavedRealm();
    if (savedRealm === "pi") {
      const savedSubMode = getSavedPiSubMode();
      return savedSubMode === "work" ? "work" : "pi:code";
    }
  }

  return "native:claude";
}

export function getAppRealm(url: URL): AppRealm {
  const target = getAgentTargetFromUrl(url);
  // `AppRealm` is a legacy sidebar-storage partition, not the runtime
  // provider. Keep Work in its existing partition while its product target
  // remains runtime-neutral.
  return isPiTarget(target) || isWorkTarget(target) ? "pi" : "native";
}

export function getAppMode(url: URL): AppMode {
  const target = getAgentTargetFromUrl(url);
  return isWorkTarget(target) ? "work" : "code";
}

export function getModeHref(mode: AppMode, workspaceId?: string): string {
  if (mode === "work") {
    return workspaceId
      ? `${WORK_MODE_PATH}?workspace=${encodeURIComponent(workspaceId)}`
      : WORK_MODE_PATH;
  }
  return CODE_MODE_PATH;
}

export function getRealmHref(
  realm: AppRealm,
  piSubMode: "code" | "work" = "code",
  workspaceId?: string,
): string {
  if (realm === "pi") {
    if (piSubMode === "work") {
      return workspaceId
        ? `${WORK_MODE_PATH}?workspace=${encodeURIComponent(workspaceId)}`
        : WORK_MODE_PATH;
    }
    return PI_CODE_PATH;
  }
  return NATIVE_MODE_PATH;
}

export const APP_REALM_STORAGE_KEY = "agentcabin:app-realm";
export const PI_SUBMODE_STORAGE_KEY = "agentcabin:pi-submode";
export const APP_MODE_STORAGE_KEY = "agentcabin:app-mode";

function safeGetItem(key: string): string | null {
  if (typeof window === "undefined" || typeof localStorage === "undefined") return null;
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function safeSetItem(key: string, value: string): void {
  if (typeof window === "undefined" || typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(key, value);
  } catch {
    // Ignore localStorage errors
  }
}

export function getSavedRealm(): AppRealm {
  if (
    safeGetItem(APP_MODE_STORAGE_KEY) === "work" ||
    safeGetItem(PI_SUBMODE_STORAGE_KEY) === "work"
  ) {
    return "pi";
  }
  const saved = safeGetItem(APP_REALM_STORAGE_KEY);
  if (saved === "native" || saved === "pi") return saved;
  return "native";
}

export function setSavedRealm(realm: AppRealm): void {
  safeSetItem(APP_REALM_STORAGE_KEY, realm);
}

export function getSavedPiSubMode(): "code" | "work" {
  const saved = safeGetItem(PI_SUBMODE_STORAGE_KEY);
  if (saved === "code" || saved === "work") return saved;
  if (safeGetItem(APP_MODE_STORAGE_KEY) === "work") return "work";
  return "code";
}

export function setSavedPiSubMode(subMode: "code" | "work"): void {
  safeSetItem(PI_SUBMODE_STORAGE_KEY, subMode);
  safeSetItem(APP_MODE_STORAGE_KEY, subMode);
  if (subMode === "work") {
    safeSetItem(APP_REALM_STORAGE_KEY, "pi");
  }
}

export function getSavedAppMode(): AppMode {
  const saved = safeGetItem(APP_MODE_STORAGE_KEY);
  if (saved === "work" || saved === "code") return saved;
  if (safeGetItem(PI_SUBMODE_STORAGE_KEY) === "work") return "work";
  return "code";
}

export function setSavedAppMode(mode: AppMode): void {
  safeSetItem(APP_MODE_STORAGE_KEY, mode);
  safeSetItem(PI_SUBMODE_STORAGE_KEY, mode);
  if (mode === "work") {
    safeSetItem(APP_REALM_STORAGE_KEY, "pi");
  }
}

export class AppModeStore {
  mode = $state<AppMode>("code");
  realm = $state<AppRealm>("native");
  target = $state<AgentTarget>("native:claude");

  sync(url: URL): void {
    this.mode = getAppMode(url);
    this.realm = getAppRealm(url);
    this.target = getAgentTargetFromUrl(url);
  }
}
