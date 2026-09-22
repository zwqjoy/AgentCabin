import type { TaskRun } from "$lib/types";

export const RUNS_CHANGED_EVENT = "agentcabin:runs-changed";

export type RunMutation =
  | {
      kind: "create";
      run: TaskRun;
    }
  | {
      kind: "update";
      runId: string;
      patch: Partial<Pick<TaskRun, "name" | "pinned" | "archived" | "unread" | "status">>;
    }
  | {
      kind: "delete";
      runIds: string[];
    };

/** Apply a successful local run mutation without reloading every events.jsonl file. */
export function applyRunMutation(runs: TaskRun[], mutation: RunMutation): TaskRun[] {
  if (mutation.kind === "create") {
    const index = runs.findIndex((run) => run.id === mutation.run.id);
    if (index < 0) return [mutation.run, ...runs];

    const next = [...runs];
    next[index] = mutation.run;
    return next;
  }

  if (mutation.kind === "delete") {
    const deletedIds = new Set(mutation.runIds);
    if (deletedIds.size === 0) return runs;
    return runs.filter((run) => !deletedIds.has(run.id));
  }

  const index = runs.findIndex((run) => run.id === mutation.runId);
  if (index < 0) return runs;

  const next = [...runs];
  next[index] = { ...next[index], ...mutation.patch };
  return next;
}

export function dispatchRunMutation(mutation: RunMutation): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent<RunMutation>(RUNS_CHANGED_EVENT, { detail: mutation }));
}
