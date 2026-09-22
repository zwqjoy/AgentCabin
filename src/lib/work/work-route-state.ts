/**
 * Work route state.
 *
 * The Work page previously read eight URL params directly and let a legacy
 * `view` param select an entire page render tree (Inbox / Tasks / Library /
 * Archived / Home). After the product simplification Work has one primary
 * surface — a conversation — plus optional auxiliary panels (drawers) and
 * filters. This module is the single place that turns a URL into that state
 * and maps legacy URLs onto it so old links keep working.
 */

/** Auxiliary panels. These never represent a different product mode. */
export type WorkPanel = "files" | "pending" | "library" | "archived" | "automation";

const LEGACY_VIEW_PANELS: Record<string, WorkPanel> = {
  inbox: "pending",
  library: "library",
  archived: "archived",
  tasks: "automation",
  automation: "automation",
};

const WORK_PANELS: readonly WorkPanel[] = ["files", "pending", "library", "archived", "automation"];

export interface WorkRouteState {
  /** Workspace id from `?workspace=`; `null` when absent. */
  workspaceId: string | null;
  /** Conversation/run id from `?run=`; `null` when absent. */
  runId: string | null;
  /** `?newSession=1` — a fresh conversation for the selected workspace. */
  newConversation: boolean;
  /** `?new=1` — the workspace creation dialog. */
  createWorkspace: boolean;
  /** `?fresh=1` — first-run rename hint for a quick-started workspace. */
  fresh: boolean;
  /** `?prompt=` — pre-filled composer text for a new conversation. */
  prompt: string | null;
  /** `?preset=` — Work preset for a fresh conversation. */
  preset: string | null;
  /**
   * Auxiliary panel to show alongside the conversation. Legacy `?view=` URLs
   * are mapped onto panels here (e.g. `?view=inbox` → `pending`).
   */
  panel: WorkPanel | null;
  /** Raw legacy `?view=` value, kept for compatibility decisions. */
  legacyView: string | null;
}

export function parseWorkRouteState(url: URL | { searchParams: URLSearchParams }): WorkRouteState {
  const params = url.searchParams;
  const workspaceId = params.get("workspace");
  const runId = params.get("run");
  const legacyView = params.get("view");
  const panelParam = params.get("panel");
  const panel = WORK_PANELS.includes(panelParam as WorkPanel)
    ? (panelParam as WorkPanel)
    : legacyView
      ? (LEGACY_VIEW_PANELS[legacyView] ?? null)
      : null;
  return {
    workspaceId: workspaceId?.trim() ? workspaceId : null,
    runId: runId?.trim() ? runId : null,
    newConversation: params.get("newSession") === "1",
    createWorkspace: params.get("new") === "1",
    fresh: params.get("fresh") === "1",
    prompt: params.get("prompt"),
    preset: params.get("preset"),
    panel,
    legacyView: legacyView?.trim() ? legacyView : null,
  };
}

/**
 * A standalone conversation is simply a conversation whose scope has no
 * workspace. A legacy view that only opens a panel does not make the page a
 * different product mode.
 */
export function isStandaloneRoute(state: WorkRouteState): boolean {
  return state.workspaceId === null;
}

/**
 * Whether the workspace view (files / settings, no conversation yet) should
 * render for the selected workspace.
 */
export function showsWorkspaceHome(state: WorkRouteState, workspaceExists: boolean): boolean {
  return Boolean(state.workspaceId) && workspaceExists && !state.runId && !state.newConversation;
}

/** Whether an active conversation surface should render. */
export function showsConversation(state: WorkRouteState, workspaceExists: boolean): boolean {
  if (state.workspaceId) {
    return workspaceExists && Boolean(state.runId || state.newConversation);
  }
  // Standalone: any non-legacy URL is the conversation surface.
  return !state.legacyView;
}
