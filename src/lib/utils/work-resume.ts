import type { RunStatus, TaskRun } from "$lib/types";

const RESUMABLE_WORK_STATUSES: RunStatus[] = ["idle", "running", "completed", "failed", "stopped"];

/**
 * Whether sending from an existing Work conversation should resume its run.
 *
 * Work's Pi actor is allowed to be gone while the conversation remains usable.
 * Terminal runs therefore resume on the next send instead of starting a new
 * standalone run. The capability/read-only guards keep unsupported runtimes
 * from entering a path they cannot handle.
 */
export function canResumeWorkSession(
  run: Pick<TaskRun, "id" | "status"> | null,
  sessionAlive: boolean,
  supportsResume: boolean,
  readOnly: boolean,
): boolean {
  return Boolean(
    run?.id &&
    !sessionAlive &&
    supportsResume &&
    !readOnly &&
    RESUMABLE_WORK_STATUSES.includes(run.status),
  );
}
