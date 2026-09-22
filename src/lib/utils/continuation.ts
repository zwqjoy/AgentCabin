import type { TaskRun } from "$lib/types";

export interface ContinuationDeps {
  getContinuationContext: (runId: string, anchorId?: string) => Promise<string>;
  startRun: (
    prompt: string,
    cwd: string,
    agent: string,
    model?: string,
    remoteHostName?: string,
    platformId?: string,
    executionPath?: string,
    continuationContext?: string,
  ) => Promise<TaskRun>;
  copyRunHistory: (sourceRunId: string, targetRunId: string, anchorId?: string) => Promise<void>;
  onRunCreated?: (run: TaskRun) => void;
  resumeSession: (runId: string, mode: "new", initialMessage?: string) => Promise<string | null>;
}

export interface ContinuationRequest {
  sourceRun: TaskRun;
  anchorId?: string;
  targetAgent?: string;
  targetCwd: string;
  model?: string;
  remoteHostName?: string;
  platformId?: string;
}

/**
 * Start a new chat branch from a visible message or from the end of the conversation.
 *
 * The caller owns worktree creation/cleanup. Keeping the session transition here makes the
 * important contract testable: history is copied before replay, and the new Agent session is
 * started without an automatic user turn.
 */
export async function startContinuationSession(
  request: ContinuationRequest,
  deps: ContinuationDeps,
): Promise<string> {
  const context = await deps.getContinuationContext(request.sourceRun.id, request.anchorId);
  const targetRun = await deps.startRun(
    "",
    request.targetCwd,
    request.targetAgent ?? request.sourceRun.agent,
    request.model,
    request.remoteHostName,
    request.platformId,
    "session_actor",
    context,
  );
  deps.onRunCreated?.(targetRun);

  await deps.copyRunHistory(request.sourceRun.id, targetRun.id, request.anchorId);

  // Replay the copied history and start only the session shell. The user's next composer input
  // is the first model turn in the new chat.
  const resultId = await deps.resumeSession(targetRun.id, "new");
  if (!resultId) throw new Error("新对话启动失败");
  return resultId;
}
