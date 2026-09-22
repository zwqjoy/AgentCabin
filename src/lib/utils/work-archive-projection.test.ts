import { describe, expect, it } from "vitest";
import type { TaskRun } from "$lib/types";
import {
  applyWorkArchiveProjectionMutation,
  enforceWorkArchiveProjectionInvariant,
  filterArchivedWorkSessions,
  filterRecentWorkSessions,
  type WorkArchiveProjection,
} from "$lib/utils/work-archive-projection";

function run(id: string, archived?: boolean): TaskRun {
  return {
    id,
    prompt: id,
    cwd: "/tmp",
    agent: "pi",
    auth_mode: "default",
    status: "completed",
    started_at: "2026-09-22T00:00:00Z",
    ...(archived === undefined ? {} : { archived }),
  } as TaskRun;
}

function projection(recent: TaskRun[] = [], archived: TaskRun[] = []): WorkArchiveProjection {
  return { recentSessions: recent, archivedSessions: archived };
}

describe("Work archive projections", () => {
  it("A: filters archived sessions out of Recent defensively", () => {
    expect(filterRecentWorkSessions([run("recent"), run("archived", true)], () => true)).toEqual([
      run("recent"),
    ]);
  });

  it("B: keeps archived sessions in an authoritative projection without workspace tree state", () => {
    expect(filterArchivedWorkSessions([run("archived", true)], () => true)).toEqual([
      run("archived", true),
    ]);
  });

  it("C: archives by removing from Recent and adding to Archived", () => {
    const result = applyWorkArchiveProjectionMutation(projection([run("a")]), {
      kind: "update",
      runId: "a",
      patch: { archived: true },
    });
    expect(result.recentSessions).toEqual([]);
    expect(result.archivedSessions).toEqual([run("a", true)]);
  });

  it("D: restores by removing from Archived and adding to Recent", () => {
    const result = applyWorkArchiveProjectionMutation(projection([], [run("a", true)]), {
      kind: "update",
      runId: "a",
      patch: { archived: false },
    });
    expect(result.recentSessions).toEqual([run("a", false)]);
    expect(result.archivedSessions).toEqual([]);
  });

  it("E: never leaves a session in both projections", () => {
    expect(
      enforceWorkArchiveProjectionInvariant(
        projection([run("a"), run("b", true)], [run("a", true)]),
      ),
    ).toEqual(projection([], [run("a", true)]));
  });

  it("F: delete removes a session from both projections", () => {
    const result = applyWorkArchiveProjectionMutation(projection([run("a")], [run("b", true)]), {
      kind: "delete",
      runIds: ["a", "b"],
    });
    expect(result).toEqual(projection());
  });
});
