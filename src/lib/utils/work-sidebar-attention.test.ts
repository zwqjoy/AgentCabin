import { describe, expect, it } from "vitest";
import type { TaskRun } from "$lib/types";
import type { InboxItem } from "$lib/types/work";
import { getWorkSessionAttentionLabel } from "$lib/utils/work-sidebar-attention";

function makeRun(overrides: Partial<TaskRun> = {}): TaskRun {
  return {
    id: "session-1",
    prompt: "test",
    cwd: "/project",
    agent: "pi",
    auth_mode: "cli",
    status: "running",
    started_at: "2026-08-15T00:00:00Z",
    execution_path: "session_actor",
    workspace_id: "ws-1",
    work_task_id: "task-1",
    work_run_id: "run-work-1",
    ...overrides,
  };
}

function makeItem(itemType: InboxItem["itemType"]): InboxItem {
  return {
    id: `item-${itemType}`,
    taskId: "task-1",
    runId: "run-work-1",
    workspaceId: "ws-1",
    itemType,
    status: "pending",
    title: "等待确认",
    description: "需要你的确认",
    payload: {},
    createdAt: "2026-08-15T01:00:00Z",
  };
}

describe("Work sidebar attention label", () => {
  it("shows approval text for a matching pending approval", () => {
    expect(getWorkSessionAttentionLabel([makeItem("permission_request")], makeRun())).toBe(
      "等待批准",
    );
  });

  it("shows input text for a matching question", () => {
    expect(getWorkSessionAttentionLabel([makeItem("question_elicitation")], makeRun())).toBe(
      "等待输入",
    );
  });

  it("does not surface resolved or unrelated inbox items", () => {
    expect(
      getWorkSessionAttentionLabel(
        [
          { ...makeItem("permission_request"), status: "approved" },
          { ...makeItem("plan_approval"), id: "other", runId: "other-run" },
        ],
        makeRun(),
      ),
    ).toBe("");
  });
});
