import { getWorkProjection } from "$lib/api/work";
import { getEventMiddleware } from "$lib/stores/event-middleware";
import type { BusEvent } from "$lib/types";
import type { WorkProjection } from "$lib/types/work";

/**
 * Determines whether a runtime bus event warrants refreshing the durable Work projection.
 */
export function shouldRefreshWorkProjection(event: BusEvent): boolean {
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

export class WorkProjectionStore {
  projectionByRunId = $state<Record<string, WorkProjection>>({});
  private subscriberCounts = new Map<string, number>();
  private eventUnsubscribers = new Map<string, () => void>();
  private eventRefreshTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private inFlight = new Map<string, Promise<WorkProjection | null>>();
  private pendingEventRefreshes = new Set<string>();

  getProjection(runId: string | null | undefined): WorkProjection | null {
    if (!runId) return null;
    return this.projectionByRunId[runId] ?? null;
  }

  // Alias for backward compatibility with WorkRunProgressView consumers
  getProgress(runId: string | null | undefined): WorkProjection | null {
    return this.getProjection(runId);
  }

  async fetch(runId: string, _taskId?: string | null): Promise<WorkProjection | null> {
    if (!runId) return null;
    const existing = this.inFlight.get(runId);
    if (existing) return existing;

    const promise = (async () => {
      try {
        const projection = await getWorkProjection(runId);
        const current = this.projectionByRunId[runId];
        if (
          !current ||
          current.runStatus !== projection.runStatus ||
          current.phase !== projection.phase ||
          current.plan?.length !== projection.plan?.length ||
          current.updatedAt !== projection.updatedAt ||
          JSON.stringify(current) !== JSON.stringify(projection)
        ) {
          this.projectionByRunId = {
            ...this.projectionByRunId,
            [runId]: projection,
          };
        }
        if (this.isTerminal(projection)) {
          this.stopWatching(runId);
        }
        return projection;
      } catch {
        return this.projectionByRunId[runId] ?? null;
      } finally {
        this.inFlight.delete(runId);
        if (
          this.pendingEventRefreshes.delete(runId) &&
          (this.subscriberCounts.get(runId) ?? 0) > 0
        ) {
          this.scheduleEventRefresh(runId);
        }
      }
    })();

    this.inFlight.set(runId, promise);
    return promise;
  }

  invalidate(runId: string) {
    if (!runId) return;
    void this.fetch(runId);
  }

  subscribe(runIdOrTaskId: string, runIdMaybe?: string): () => void {
    const runId = runIdMaybe ?? runIdOrTaskId;
    if (!runId) return () => {};

    const count = (this.subscriberCounts.get(runId) ?? 0) + 1;
    this.subscriberCounts.set(runId, count);

    if (count === 1) {
      this.startEventWatching(runId);
    }

    // Initial fetch
    void this.fetch(runId).then((projection) => {
      const activeSubscribers = this.subscriberCounts.get(runId) ?? 0;
      if (activeSubscribers <= 0) {
        return;
      }
      if (projection && this.isTerminal(projection)) {
        this.stopWatching(runId);
      }
    });

    return () => {
      const current = (this.subscriberCounts.get(runId) ?? 1) - 1;
      if (current <= 0) {
        this.subscriberCounts.delete(runId);
        this.stopWatching(runId);
      } else {
        this.subscriberCounts.set(runId, current);
      }
    };
  }

  private startEventWatching(runId: string): void {
    if (this.eventUnsubscribers.has(runId)) return;
    const unsubscribe = getEventMiddleware().subscribeEvents((event) => {
      const sessionId = this.projectionByRunId[runId]?.sessionId;
      if (event.run_id !== runId && event.run_id !== sessionId) return;
      if ((this.subscriberCounts.get(runId) ?? 0) <= 0) return;

      if (event.type === "work_projection_changed" && "projection" in event) {
        const projection = (event as { projection: WorkProjection }).projection;
        if (projection && projection.runId === runId) {
          this.projectionByRunId = {
            ...this.projectionByRunId,
            [runId]: projection,
          };
          if (this.isTerminal(projection)) {
            this.stopWatching(runId);
          }
          return;
        }
      }

      if (!shouldRefreshWorkProjection(event)) return;
      this.scheduleEventRefresh(runId);
    });
    this.eventUnsubscribers.set(runId, unsubscribe);
  }

  private scheduleEventRefresh(runId: string): void {
    if (this.inFlight.has(runId)) {
      this.pendingEventRefreshes.add(runId);
      return;
    }
    if (this.eventRefreshTimers.has(runId)) return;
    const timer = setTimeout(() => {
      this.eventRefreshTimers.delete(runId);
      void this.fetch(runId);
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
    this.inFlight.clear();
    this.pendingEventRefreshes.clear();
    this.projectionByRunId = {};
  }

  isTerminal(projection: WorkProjection): boolean {
    if (projection.automationStatus != null) {
      const status = projection.automationStatus;
      return (
        status === "completed" ||
        status === "failed" ||
        status === "cancelled" ||
        status === "skipped"
      );
    }
    const rStatus = projection.runtimeStatus;
    return rStatus === "failed" || rStatus === "stopped";
  }
}

export const workProjectionStore = new WorkProjectionStore();
