export type WorkspaceRowAction = { kind: "navigate"; href: string } | { kind: "stay" };

/**
 * Keep the chat surface mounted while a newly-started run is being reflected
 * through bindable session state. Only route identity is allowed to replace
 * the surface; otherwise the first run can tear itself down before navigation
 * writes the run id into the URL.
 */
export function getWorkConversationSurfaceKey(workspaceId: string, routeRunId: string): string {
  if (workspaceId) return `${workspaceId}:${routeRunId || "new"}`;
  return routeRunId || "standalone-new";
}

/**
 * Resolve the keyed chat-surface identity after a route change.
 *
 * Starting the first run updates the URL from new -> run without changing the
 * live conversation. Preserve the existing key for that one transition so the
 * surface that owns the starting session is not destroyed. Every other route
 * change represents a conversation or workspace selection and gets a new key.
 */
export function getWorkConversationSurfaceKeyAfterRouteChange(
  currentKey: string,
  previousWorkspaceId: string,
  previousRunId: string,
  nextWorkspaceId: string,
  nextRunId: string,
  isAdoptedRun = false,
): string {
  const adoptedFirstRun =
    isAdoptedRun && previousWorkspaceId === nextWorkspaceId && !previousRunId && Boolean(nextRunId);
  return adoptedFirstRun ? currentKey : getWorkConversationSurfaceKey(nextWorkspaceId, nextRunId);
}

/**
 * Resolve the action for clicking a workspace name. Expansion is controlled
 * by the separate chevron, so clicking an already selected workspace that is
 * already showing its configuration should not change the sidebar state.
 */
export function getWorkspaceRowAction(
  selectedWorkspaceId: string,
  selectedRunId: string,
  selectedView: string,
  hasNewConversation: boolean,
  targetWorkspaceId: string,
): WorkspaceRowAction {
  const isAlreadyShowingConfiguration =
    selectedWorkspaceId === targetWorkspaceId &&
    !selectedRunId &&
    !selectedView &&
    !hasNewConversation;

  if (isAlreadyShowingConfiguration) return { kind: "stay" };
  return {
    kind: "navigate",
    href: `/chat/work?workspace=${encodeURIComponent(targetWorkspaceId)}`,
  };
}

/**
 * Determine whether the top-level "对话" (Work conversation home) entry is active.
 *
 * It should only be active when there is no workspace, no active/selected run,
 * no just-started run, and no secondary view open.
 */
export function isWorkHomeSelected(
  selectedWorkspaceId: string,
  selectedRunId: string,
  selectedView: string,
  hasNewConversation: boolean,
  startedRunId = "",
): boolean {
  return (
    !selectedWorkspaceId && !selectedRunId && !selectedView && !hasNewConversation && !startedRunId
  );
}

/**
 * Resolve the active run ID for Work sidebar selection.
 *
 * Uses the route run ID if present, or falls back to the newly-started run ID
 * during the transition before SvelteKit navigation finishes.
 */
export function getWorkActiveRunId(selectedRunId: string, startedRunId = ""): string {
  return selectedRunId || startedRunId;
}
