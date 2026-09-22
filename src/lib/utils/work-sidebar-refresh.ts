import type { TaskRun } from "$lib/types";

type RunRef = Pick<TaskRun, "id">;

export interface WorkSessionRefreshTargets {
  workspaceIds: string[];
  refreshStandalone: boolean;
}

/**
 * Find expanded workspaces whose session list has not been hydrated yet.
 * This covers workspaces restored from the sidebar preference before a
 * workspace is selected in the URL.
 */
export function getExpandedWorkSessionLoadTargets(
  workspaceIds: readonly string[],
  expandedWorkspaceIds: Iterable<string>,
  loadedWorkspaceIds: Iterable<string>,
): string[] {
  const expanded = new Set(expandedWorkspaceIds);
  const loaded = new Set(loadedWorkspaceIds);
  return workspaceIds.filter(
    (workspaceId) => expanded.has(workspaceId) && !loaded.has(workspaceId),
  );
}

/**
 * Map a run-state event to the sidebar session lists that need a fresh read.
 * Standalone Work tasks are kept in a separate list from Workspace sessions.
 */
export function getWorkSessionRefreshTargets(
  runId: string,
  sessionsByWorkspace: Record<string, RunRef[]>,
  standaloneSessions: RunRef[],
): WorkSessionRefreshTargets {
  const workspaceIds = Object.entries(sessionsByWorkspace)
    .filter(([, sessions]) => sessions.some((session) => session.id === runId))
    .map(([workspaceId]) => workspaceId);

  return {
    workspaceIds,
    refreshStandalone: standaloneSessions.some((session) => session.id === runId),
  };
}
