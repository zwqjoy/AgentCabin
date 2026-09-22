import { getWorkProjection, getWorkRunProgress } from "$lib/api/work";
import { getEventMiddleware } from "$lib/stores/event-middleware";
import type { BusEvent } from "$lib/types";
import type { WorkRunProgressView } from "$lib/types/work";

/**
 * Progress is a durable projection over Work state, ledger facts and pending
 * interactions. Token deltas do not change any of those inputs, so refreshing
 * the projection for every streamed token only floods the Tauri main thread.
 */
export function shouldRefreshWorkRunProgress(event: BusEvent): boolean {
  switch (event.type) {
    case "work_projection_changed":
    case "structured_task_state":
    case "work_task_state":
    case "tool_start":
    case "tool_end":
    case "run_state":
    case "permission_prompt":
    case "permission_denied":
    case "elicitation_prompt":
    case "control_cancelled":
    case "task_notification":
    case "files_persisted":
      return true;
    default:
      return false;
  }
}

export class WorkRunProgressStore {
  progressByRunId = $state<Record<string, WorkRunProgressView>>({});
  private subscriberCounts = new Map<string, number>();
  private taskIds = new Map<string, string>();
  private eventUnsubscribers = new Map<string, () => void>();
  private eventRefreshTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private inFlight = new Map<string, Promise<WorkRunProgressView | null>>();
  private pendingEventRefreshes = new Set<string>();

  getProgress(runId: string | null | undefined): WorkRunProgressView | null {
    if (!runId) return null;
    return this.progressByRunId[runId] ?? null;
  }

  async fetch(taskIdOrRunId: string, runIdMaybe?: string): Promise<WorkRunProgressView | null> {
    const runId = runIdMaybe ?? taskIdOrRunId;
    const taskId = runIdMaybe ? taskIdOrRunId : "";
    if (!runId) return null;
    const existing = this.inFlight.get(runId);
    if (existing) return existing;

    const promise = (async () => {
      try {
        let progress: WorkRunProgressView;
        if (taskId) {
          try {
            progress = await getWorkRunProgress(taskId, runId);
          } catch {
            progress = await getWorkProjection(runId);
          }
        } else {
          progress = await getWorkProjection(runId);
        }
        const current = this.progressByRunId[runId];
        if (
          !current ||
          current.runStatus !== progress.runStatus ||
          current.phase !== progress.phase ||
          current.steps?.length !== progress.steps?.length ||
          JSON.stringify(current) !== JSON.stringify(progress)
        ) {
          this.progressByRunId = {
            ...this.progressByRunId,
            [runId]: progress,
          };
        }
        if (this.isTerminal(progress)) {
          this.stopWatching(runId);
        }
        return progress;
      } catch {
        return this.progressByRunId[runId] ?? null;
      } finally {
        this.inFlight.delete(runId);
        if (
          this.pendingEventRefreshes.delete(runId) &&
          (this.subscriberCounts.get(runId) ?? 0) > 0
        ) {
          const currentTaskId = this.taskIds.get(runId) ?? "";
          this.scheduleEventRefresh(currentTaskId, runId);
        }
      }
    })();

    this.inFlight.set(runId, promise);
    return promise;
  }

  invalidate(taskId: string, runId?: string) {
    const effectiveRunId = runId ?? taskId;
    if (!effectiveRunId) return;
    void this.fetch(taskId, runId);
  }

  subscribe(taskIdOrRunId: string, runIdMaybe?: string): () => void {
    const runId = runIdMaybe ?? taskIdOrRunId;
    const taskId = runIdMaybe ? taskIdOrRunId : "";
    if (!runId) return () => {};

    const count = (this.subscriberCounts.get(runId) ?? 0) + 1;
    this.subscriberCounts.set(runId, count);
    if (taskId) {
      this.taskIds.set(runId, taskId);
    }

    if (count === 1) {
      this.startEventWatching(taskId, runId);
    }

    // Initial fetch
    void this.fetch(taskId, runId).then((progress) => {
      const activeSubscribers = this.subscriberCounts.get(runId) ?? 0;
      if (activeSubscribers <= 0) {
        return;
      }
      if (progress && this.isTerminal(progress)) {
        this.stopWatching(runId);
      }
    });

    return () => {
      const current = (this.subscriberCounts.get(runId) ?? 1) - 1;
      if (current <= 0) {
        this.subscriberCounts.delete(runId);
        this.taskIds.delete(runId);
        this.stopWatching(runId);
      } else {
        this.subscriberCounts.set(runId, current);
      }
    };
  }

  private startEventWatching(taskId: string, runId: string): void {
    if (this.eventUnsubscribers.has(runId)) return;
    const unsubscribe = getEventMiddleware().subscribeEvents((event) => {
      const sessionId = this.progressByRunId[runId]?.sessionId;
      if (event.run_id !== runId && event.run_id !== sessionId) return;
      if ((this.subscriberCounts.get(runId) ?? 0) <= 0) return;

      if (event.type === "work_projection_changed" && "projection" in event) {
        const projection = (event as { projection: WorkRunProgressView }).projection;
        if (projection && (event.run_id === runId || projection.workRunId === runId)) {
          this.progressByRunId = {
            ...this.progressByRunId,
            [runId]: projection,
          };
          if (this.isTerminal(projection)) {
            this.stopWatching(runId);
          }
          return;
        }
      }

      if (!shouldRefreshWorkRunProgress(event)) return;
      this.scheduleEventRefresh(taskId, runId);
    });
    this.eventUnsubscribers.set(runId, unsubscribe);
  }

  private scheduleEventRefresh(taskId: string, runId: string): void {
    if (this.inFlight.has(runId)) {
      // Keep one trailing refresh so a terminal/tool event arriving during the
      // current projection read cannot be lost.
      this.pendingEventRefreshes.add(runId);
      return;
    }
    if (this.eventRefreshTimers.has(runId)) return;
    // The backend persists the progress projection immediately before/around
    // event emission. A short coalescing window lets a token/tool burst settle
    // while still making state changes visible in the next frame.
    const timer = setTimeout(() => {
      this.eventRefreshTimers.delete(runId);
      void this.fetch(taskId, runId);
    }, 80);
    this.eventRefreshTimers.set(runId, timer);
  }

  /** Stop all live refresh work for a run. */
  stopWatching(runId: string): void {
    const unsubscribe = this.eventUnsubscribers.get(runId);
    if (unsubscribe) {
      unsubscribe();
      this.eventUnsubscribers.delete(runId);
    }
    const refreshTimer = this.eventRefreshTimers.get(runId);
    if (refreshTimer !== undefined) {
      clearTimeout(refreshTimer);
      this.eventRefreshTimers.delete(runId);
    }
    this.pendingEventRefreshes.delete(runId);
  }

  clear() {
    for (const runId of new Set([
      ...this.eventUnsubscribers.keys(),
      ...this.eventRefreshTimers.keys(),
    ])) {
      this.stopWatching(runId);
    }
    this.subscriberCounts.clear();
    this.taskIds.clear();
    this.inFlight.clear();
    this.pendingEventRefreshes.clear();
    this.progressByRunId = {};
  }

  isTerminal(progress: WorkRunProgressView): boolean {
    const status = progress.runStatus;
    return (
      status === "completed" ||
      status === "failed" ||
      status === "cancelled" ||
      status === "skipped"
    );
  }
}

export const workRunProgressStore = new WorkRunProgressStore();
