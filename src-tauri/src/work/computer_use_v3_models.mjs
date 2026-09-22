/**
 * AgentCabin Computer Use V3 Unified Domain Models & State Store.
 *
 * Encapsulates UiRoot, UiState, UiElement, and StateStore.
 */

export const BACKEND_MACOS_AX = "macos_ax";
export const BACKEND_WINDOWS_UIA = "windows_uia";
export const BACKEND_CDP = "cdp";

export const ROOT_KIND_DESKTOP_WINDOW = "desktop_window";
export const ROOT_KIND_BROWSER_PAGE = "browser_page";
export const ROOT_KIND_ELECTRON_PAGE = "electron_page";
export const ROOT_KIND_DIALOG = "dialog";
export const ROOT_KIND_SHEET = "sheet";
export const ROOT_KIND_POPOVER = "popover";

/**
 * Creates a normalized UiRoot representation.
 */
export function createUiRoot({
  ref,
  kind = ROOT_KIND_DESKTOP_WINDOW,
  backend = BACKEND_MACOS_AX,
  resourceKey,
  title = "",
  appName = "",
  pid,
  windowId,
  nativeRootRef,
  browserTargetId,
  cdpEndpoint,
  cdpTargetId,
  cdpPageUrl,
  url,
  frame,
  isOnscreen = true,
}) {
  return {
    ref,
    kind,
    backend,
    resourceKey: resourceKey || (
      backend === BACKEND_CDP
        ? `cdp-page:${browserTargetId || url || ref}`
        : `desktop-root:${ref}`
    ),
    title: String(title || ""),
    appName: String(appName || ""),
    pid: pid ? Number(pid) : undefined,
    windowId: windowId ? Number(windowId) : undefined,
    nativeRootRef: nativeRootRef ? String(nativeRootRef) : undefined,
    browserTargetId: browserTargetId ? String(browserTargetId) : undefined,
    cdpEndpoint: cdpEndpoint ? String(cdpEndpoint) : undefined,
    cdpTargetId: cdpTargetId ? String(cdpTargetId) : undefined,
    cdpPageUrl: cdpPageUrl ? String(cdpPageUrl) : undefined,
    url: url ? String(url) : undefined,
    frame: frame && typeof frame === "object" ? {
      x: Number(frame.x || 0),
      y: Number(frame.y || 0),
      width: Number(frame.width || frame.w || 0),
      height: Number(frame.height || frame.h || 0),
    } : undefined,
    isOnscreen: Boolean(isOnscreen),
  };
}

/**
 * Creates a normalized UiElement representation.
 */
export function createUiElement({
  ref,
  role = "element",
  name,
  title,
  label,
  value,
  description,
  rect,
  capabilities = [],
  children = [],
  backendRef,
  legacyRef,
  nativeRef,
  evidence = { semantic: true },
}) {
  const wireRef = String(backendRef || nativeRef || legacyRef || ref || "").replace(/^@/, "");
  return {
    ref: ref.startsWith("@") ? ref : `@${ref}`,
    role: String(role || "element"),
    name: name || title || label,
    title: title || name || label,
    label: label || title || name,
    value: value !== undefined && value !== null ? String(value) : undefined,
    description: description ? String(description) : undefined,
    rect: rect && typeof rect === "object" ? {
      x: Number(rect.x || 0),
      y: Number(rect.y || 0),
      width: Number(rect.width || rect.w || 0),
      height: Number(rect.height || rect.h || 0),
    } : undefined,
    capabilities: Array.isArray(capabilities) ? capabilities : [],
    children: Array.isArray(children) ? children : [],
    backendRef: wireRef,
    legacyRef: wireRef,
    nativeRef: wireRef,
    evidence,
  };
}

/**
 * Domain State Repository: encapsulates immutable states, latest state by resource,
 * and eviction policies.
 */
export class StateStore {
  constructor(limit = 128) {
    this.limit = limit;
    this.states = new Map();
    this.latestStates = new Map();
  }

  saveState(state) {
    this.states.set(state.stateId, state);
    this.latestStates.set(state.resourceKey, state);
    this.evict();
    return state;
  }

  getState(stateId) {
    const state = this.states.get(String(stateId || ""));
    if (!state) {
      throw new Error(`UI state '${stateId}' is unavailable. Observe the root again.`);
    }
    return state;
  }

  getLatestState(resourceKey) {
    return this.latestStates.get(resourceKey);
  }

  hasState(stateId) {
    return this.states.has(String(stateId || ""));
  }

  evict() {
    while (this.states.size > this.limit) {
      const oldestId = this.states.keys().next().value;
      const oldest = this.states.get(oldestId);
      this.states.delete(oldestId);
      if (this.latestStates.get(oldest?.resourceKey) === oldest) {
        this.latestStates.delete(oldest.resourceKey);
      }
    }
  }

  clear() {
    this.states.clear();
    this.latestStates.clear();
  }

  get size() {
    return this.states.size;
  }
}
