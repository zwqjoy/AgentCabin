/**
 * Work Identity Normalization Utility
 *
 * Establishes strict semantic separation between:
 * 1. Task ID: Durable WorkTask ID (e.g. "task-100"). Undefined for sessions without a WorkTask.
 * 2. WorkRun ID: Durable execution instance of a WorkTask or session execution (e.g. "run-200").
 * 3. Session Run ID: Central run storage ID (e.g. "session-xyz" or "run-200").
 * 4. Workspace ID: Workspace identifier (e.g. "AgentCabin"). Undefined for standalone.
 * 5. Standalone Work: Workspace-less run using standalone APIs and sessionRunId.
 */

export interface WorkIdentitySourceRun {
  id?: string | null;
  session_id?: string | null;
  work_task_id?: string | null;
  work_run_id?: string | null;
  workspace_id?: string | null;
}

export interface WorkIdentitySourceProgress {
  taskId?: string | null;
  workRunId?: string | null;
  automationTaskId?: string | null;
  sessionId?: string | null;
}

export interface NormalizedWorkIdentity {
  /**
   * Durable WorkTask ID (e.g. "task-100").
   * Only defined when the run legitimately belongs to a durable WorkTask.
   * NEVER forged to equal run.id or workRunId.
   */
  taskId?: string;

  /**
   * Durable WorkRun ID (e.g. "run-200").
   * Represents the execution instance.
   * Priority: run.work_run_id > progressView.workRunId > run.id > conversationRunId.
   */
  workRunId: string;

  /**
   * Central storage session run ID (e.g. "session-xyz" or "run-200").
   */
  sessionRunId: string;

  /**
   * Workspace identifier (e.g. "AgentCabin"). Undefined for standalone.
   */
  workspaceId?: string;

  /**
   * Whether this is a standalone (workspace-less) Work session.
   */
  isStandalone: boolean;

  /**
   * Whether this run can legitimately load a workspace WorkRunReceipt.
   * Requires: !isStandalone && Boolean(taskId) && Boolean(workRunId) && taskId !== workRunId.
   */
  canLoadWorkspaceReceipt: boolean;
}

export interface NormalizeWorkIdentityOptions {
  run?: WorkIdentitySourceRun | null;
  progressView?: WorkIdentitySourceProgress | null;
  workspaceId?: string | null;
  conversationRunId?: string | null;
}

export function normalizeWorkIdentity(
  options: NormalizeWorkIdentityOptions,
): NormalizedWorkIdentity {
  const sessionRunId = options.run?.id?.trim() || options.conversationRunId?.trim() || "";
  const workspaceId = options.run?.workspace_id?.trim() || options.workspaceId?.trim() || undefined;
  const isStandalone = !workspaceId;

  // Resolve workRunId
  // Priority: 1. run.work_run_id (explicit contract)
  //           2. progressView.workRunId (durable projection)
  //           3. run.id / conversationRunId (session fallback)
  const candidateWorkRunId =
    options.run?.work_run_id?.trim() || options.progressView?.workRunId?.trim() || sessionRunId;

  // Resolve taskId
  // Priority: 1. run.work_task_id (authoritative session metadata)
  //           2. progressView.automationTaskId (authoritative backend projection)
  //           3. progressView.taskId IF and ONLY IF it is not equal to candidateWorkRunId or sessionRunId
  // NEVER fall back to run.id or workRunId!
  let resolvedTaskId: string | undefined;

  const rawRunTaskId = options.run?.work_task_id?.trim();
  const rawAutoTaskId = options.progressView?.automationTaskId?.trim();
  const rawProgressTaskId = options.progressView?.taskId?.trim();

  if (rawRunTaskId) {
    resolvedTaskId = rawRunTaskId;
  } else if (rawAutoTaskId) {
    resolvedTaskId = rawAutoTaskId;
  } else if (
    rawProgressTaskId &&
    rawProgressTaskId !== candidateWorkRunId &&
    rawProgressTaskId !== sessionRunId
  ) {
    resolvedTaskId = rawProgressTaskId;
  }

  // A workspace receipt can only be loaded if we are in a workspace, have both
  // taskId and workRunId, and taskId != workRunId.
  const canLoadWorkspaceReceipt =
    !isStandalone &&
    Boolean(resolvedTaskId) &&
    Boolean(candidateWorkRunId) &&
    resolvedTaskId !== candidateWorkRunId;

  return {
    taskId: resolvedTaskId,
    workRunId: candidateWorkRunId,
    sessionRunId,
    workspaceId,
    isStandalone,
    canLoadWorkspaceReceipt,
  };
}
