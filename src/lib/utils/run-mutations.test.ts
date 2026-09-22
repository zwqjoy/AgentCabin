import { describe, expect, it } from "vitest";
import type { TaskRun } from "$lib/types";
import { applyRunMutation } from "./run-mutations";

function makeRun(id: string, overrides: Partial<TaskRun> = {}): TaskRun {
  return {
    id,
    prompt: id,
    cwd: "/project",
    agent: "pi",
    auth_mode: "cli",
    status: "stopped",
    started_at: "2026-08-05T00:00:00Z",
    execution_path: "session_actor",
    ...overrides,
  };
}

describe("applyRunMutation", () => {
  it("updates only the targeted run locally", () => {
    const first = makeRun("run-1", { pinned: false });
    const second = makeRun("run-2", { archived: false });

    const result = applyRunMutation([first, second], {
      kind: "update",
      runId: "run-2",
      patch: { archived: true, name: "Renamed" },
    });

    expect(result).toEqual([first, { ...second, archived: true, name: "Renamed" }]);
    expect(result[0]).toBe(first);
  });

  it("removes all runs in a deleted conversation", () => {
    const runs = [makeRun("run-1"), makeRun("run-2"), makeRun("run-3")];

    expect(applyRunMutation(runs, { kind: "delete", runIds: ["run-1", "run-3"] })).toEqual([
      runs[1],
    ]);
  });

  it("leaves the list unchanged for an unknown run", () => {
    const runs = [makeRun("run-1")];
    const result = applyRunMutation(runs, {
      kind: "update",
      runId: "missing",
      patch: { pinned: true },
    });

    expect(result).toBe(runs);
  });

  it("inserts a newly-created run before session startup finishes", () => {
    const existing = makeRun("run-1");
    const created = makeRun("run-new", { status: "pending" });

    const result = applyRunMutation([existing], {
      kind: "create",
      run: created,
    });

    expect(result).toEqual([created, existing]);
  });
});
