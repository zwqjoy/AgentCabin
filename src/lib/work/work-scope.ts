/**
 * WorkScope is the single domain boundary between "a Work conversation" and
 * the two backend API families that implement it.
 *
 * Historically the frontend had two parallel product modes:
 *   - Workspace conversations  → work_list_artifacts / work_deliver / …
 *   - Standalone conversations → work_list_standalone_artifacts / …
 *
 * After the product simplification a standalone conversation is no longer a
 * separate mode. It is simply a conversation whose scope has
 * `workspaceId === null`. UI components must never know which backend family
 * they are calling; `work-resource-service.ts` translates a WorkScope into
 * the concrete backend invocation.
 */
export interface WorkScope {
  /** `null` for a standalone (workspace-less) conversation. */
  workspaceId: string | null;
}

export function workspaceScope(workspaceId: string): WorkScope {
  return { workspaceId };
}

export function standaloneScope(): WorkScope {
  return { workspaceId: null };
}

export function isStandaloneScope(scope: WorkScope): boolean {
  return scope.workspaceId === null;
}

/**
 * The id used by "open file / open directory" style backend commands.
 * Standalone sessions reuse the session run id as their storage namespace,
 * which mirrors the existing backend contract.
 */
export function scopeStorageId(scope: WorkScope, standaloneRunId: string): string {
  return scope.workspaceId ?? standaloneRunId;
}
