export const SIDEBAR_WIDTH_STORAGE_KEY = "agentcabin:sidebar-width";
export const SIDEBAR_LAYOUT_STORAGE_KEY = "agentcabin:sidebar-width-layout";
export const SIDEBAR_LAYOUT_VERSION = "4";
export const SIDEBAR_WIDTH_DEFAULT = 320;
// Keep the rail usable and prevent brand title clipping
export const SIDEBAR_WIDTH_MIN = 280;
export const SIDEBAR_WIDTH_MAX = 460;
export const SIDEBAR_WIDTH_CHANGED_EVENT = "agentcabin:sidebar-width-changed";

export function clampSidebarWidth(width: number): number {
  if (!Number.isFinite(width)) return SIDEBAR_WIDTH_DEFAULT;
  return Math.min(SIDEBAR_WIDTH_MAX, Math.max(SIDEBAR_WIDTH_MIN, Math.round(width)));
}

export function readSidebarWidth(): number {
  if (typeof window === "undefined") return SIDEBAR_WIDTH_DEFAULT;

  if (localStorage.getItem(SIDEBAR_LAYOUT_STORAGE_KEY) !== SIDEBAR_LAYOUT_VERSION) {
    localStorage.setItem(SIDEBAR_LAYOUT_STORAGE_KEY, SIDEBAR_LAYOUT_VERSION);
    localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(SIDEBAR_WIDTH_DEFAULT));
    return SIDEBAR_WIDTH_DEFAULT;
  }

  const raw = parseInt(localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY) ?? "", 10);
  return Number.isFinite(raw) ? clampSidebarWidth(raw) : SIDEBAR_WIDTH_DEFAULT;
}

export function persistSidebarWidth(width: number): number {
  const next = clampSidebarWidth(width);
  if (typeof window !== "undefined") {
    localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(next));
    window.dispatchEvent(new CustomEvent(SIDEBAR_WIDTH_CHANGED_EVENT, { detail: { width: next } }));
  }
  return next;
}
