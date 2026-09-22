/**
 * Utilities for realm-scoped project workspace storage.
 *
 * Isolates working directory (CWD), pinned project lists, and expanded
 * project tree states between Native Agent (/chat) and Pi Code (/chat/pi).
 */

export type ProjectRealm = "native" | "pi";

export const PROJECT_CWD_NATIVE_KEY = "agentcabin:project-cwd:native";
export const PROJECT_CWD_PI_KEY = "agentcabin:project-cwd:pi";
export const PINNED_CWDS_NATIVE_KEY = "agentcabin:pinned-cwds:native";
export const PINNED_CWDS_PI_KEY = "agentcabin:pinned-cwds:pi";
export const EXPANDED_PROJECTS_NATIVE_KEY = "agentcabin:expanded-projects:native";
export const EXPANDED_PROJECTS_PI_KEY = "agentcabin:expanded-projects:pi";

export const LEGACY_PROJECT_CWD_KEY = "agentcabin:project-cwd";
export const LEGACY_PINNED_CWDS_KEY = "agentcabin:pinned-cwds";
export const LEGACY_EXPANDED_PROJECTS_KEY = "agentcabin:expanded-projects";

export function getProjectCwdKey(realm: ProjectRealm = "native"): string {
  return realm === "pi" ? PROJECT_CWD_PI_KEY : PROJECT_CWD_NATIVE_KEY;
}

export function getPinnedCwdsKey(realm: ProjectRealm = "native"): string {
  return realm === "pi" ? PINNED_CWDS_PI_KEY : PINNED_CWDS_NATIVE_KEY;
}

export function getExpandedProjectsKey(realm: ProjectRealm = "native"): string {
  return realm === "pi" ? EXPANDED_PROJECTS_PI_KEY : EXPANDED_PROJECTS_NATIVE_KEY;
}

export function getSavedProjectCwd(realm: ProjectRealm = "native"): string {
  if (typeof window === "undefined" || typeof localStorage === "undefined") return "";
  try {
    const key = getProjectCwdKey(realm);
    const val = localStorage.getItem(key);
    return val ?? "";
  } catch {
    return "";
  }
}

export function setSavedProjectCwd(cwd: string, realm: ProjectRealm = "native"): void {
  if (typeof window === "undefined" || typeof localStorage === "undefined") return;
  try {
    const key = getProjectCwdKey(realm);
    if (cwd) {
      localStorage.setItem(key, cwd);
    } else {
      localStorage.removeItem(key);
    }
  } catch {
    /* ignore storage error */
  }
}

export function getSavedPinnedCwds(realm: ProjectRealm = "native"): string[] {
  if (typeof window === "undefined" || typeof localStorage === "undefined") return [];
  try {
    const key = getPinnedCwdsKey(realm);
    const raw = localStorage.getItem(key);
    if (raw !== null) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed) && parsed.every((v: unknown) => typeof v === "string")) {
        return parsed as string[];
      }
    }
    const fallbackKey = realm === "pi" ? PINNED_CWDS_NATIVE_KEY : PINNED_CWDS_PI_KEY;
    const fallbackRaw =
      localStorage.getItem(fallbackKey) ?? localStorage.getItem(LEGACY_PINNED_CWDS_KEY);
    if (fallbackRaw !== null) {
      const parsed = JSON.parse(fallbackRaw);
      if (Array.isArray(parsed) && parsed.every((v: unknown) => typeof v === "string")) {
        return parsed as string[];
      }
    }
  } catch {
    /* ignore parse/storage error */
  }
  return [];
}

export function setSavedPinnedCwds(cwds: string[], realm: ProjectRealm = "native"): void {
  if (typeof window === "undefined" || typeof localStorage === "undefined") return;
  try {
    const key = getPinnedCwdsKey(realm);
    localStorage.setItem(key, JSON.stringify(cwds));
  } catch {
    /* ignore storage error */
  }
}

export function getSavedExpandedProjects(realm: ProjectRealm = "native"): Set<string> {
  if (typeof window === "undefined" || typeof localStorage === "undefined") return new Set();
  try {
    const key = getExpandedProjectsKey(realm);
    const raw = localStorage.getItem(key);
    if (raw !== null) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed) && parsed.every((v: unknown) => typeof v === "string")) {
        return new Set(parsed as string[]);
      }
    }
    const fallbackKey = realm === "pi" ? EXPANDED_PROJECTS_NATIVE_KEY : EXPANDED_PROJECTS_PI_KEY;
    const fallbackRaw =
      localStorage.getItem(fallbackKey) ?? localStorage.getItem(LEGACY_EXPANDED_PROJECTS_KEY);
    if (fallbackRaw !== null) {
      const parsed = JSON.parse(fallbackRaw);
      if (Array.isArray(parsed) && parsed.every((v: unknown) => typeof v === "string")) {
        return new Set(parsed as string[]);
      }
    }
  } catch {
    /* ignore parse/storage error */
  }
  return new Set();
}

export function setSavedExpandedProjects(
  expanded: Set<string>,
  realm: ProjectRealm = "native",
): void {
  if (typeof window === "undefined" || typeof localStorage === "undefined") return;
  try {
    const key = getExpandedProjectsKey(realm);
    localStorage.setItem(key, JSON.stringify(Array.from(expanded)));
  } catch {
    /* ignore storage error */
  }
}
