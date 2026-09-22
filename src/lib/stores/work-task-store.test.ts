import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("$lib/api/work", () => ({
  addWorkStandingRule: vi.fn(),
  createWorkTask: vi.fn(),
  finishWorkRun: vi.fn(),
  getWorkTask: vi.fn(),
  listWorkRuns: vi.fn().mockResolvedValue([]),
  listWorkTasks: vi.fn(),
  startWorkRun: vi.fn(),
  updateWorkTask: vi.fn(),
}));

import { getWorkTask, listWorkRuns, listWorkTasks, startWorkRun } from "$lib/api/work";
import type { WorkRun, WorkTask } from "$lib/types/work";
import { WorkTaskStore } from "./work-task-store.svelte";

const task: WorkTask = {
  id: "task-1",
  workspaceId: "workspace-1",
  title: "Workspace task",
  instructions: "Run the task",
  status: "active",
  policy: {
    executionMode: "direct",
    maxAutomatedSteps: 10,
    allowExternalConnectors: false,
    standingRules: [],
  },
  schedule: null,
  runCount: 0,
  lastRunId: null,
  lastRunAt: null,
  createdAt: "2026-08-13T00:00:00Z",
  updatedAt: "2026-08-13T00:00:00Z",
  source: "manual",
};

const run: WorkRun = {
  id: "run-1",
  taskId: task.id,
  workspaceId: task.workspaceId,
  sessionId: "session-1",
  trigger: "manual",
  status: "running",
  executionContext: "attended",
  scheduledFor: null,
  skippedReason: null,
  taskState: {
    version: 1,
    revision: 0,
    goal: null,
    plan: [],
    checkpoint: null,
    updatedAt: "2026-08-13T00:00:00Z",
  },
  errorMessage: null,
  startedAt: "2026-08-13T00:00:00Z",
  finishedAt: null,
  durationMs: null,
};

const dialogTask: WorkTask = {
  ...task,
  id: "dialog-task-1",
  source: "dialog",
  title: "Conversation task",
};

describe("WorkTaskStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(startWorkRun).mockResolvedValue(run);
    vi.mocked(listWorkTasks).mockResolvedValue([task]);
    vi.mocked(getWorkTask).mockResolvedValue(dialogTask);
  });

  it("keeps the active Workspace filter when refreshing after Run Now", async () => {
    const store = new WorkTaskStore();
    store.tasks = [task];
    store.activeTaskId = task.id;

    await store.startRun(task.id);

    expect(listWorkTasks).toHaveBeenLastCalledWith(task.workspaceId);
  });

  it("refreshes task and run state when Run Now fails after creating a run", async () => {
    const store = new WorkTaskStore();
    store.tasks = [task];
    store.activeTaskId = task.id;
    vi.mocked(startWorkRun).mockRejectedValueOnce(
      new Error("Failed to attach Work session 'session-1' to WorkRun 'run-1'"),
    );

    await expect(store.startRun(task.id)).rejects.toThrow("Failed to attach Work session");

    expect(listWorkTasks).toHaveBeenLastCalledWith(task.workspaceId);
    expect(listWorkRuns).toHaveBeenLastCalledWith(task.id);
    expect(store.error).toContain("Failed to attach Work session");
  });

  it("caches dialog tasks without exposing them in the scheduled task list", async () => {
    const store = new WorkTaskStore();
    vi.mocked(listWorkTasks).mockResolvedValueOnce([task, dialogTask]);

    await store.fetchTasks(task.workspaceId);

    expect(store.tasks).toEqual([task]);
    expect(store.getTaskById(dialogTask.id)).toEqual(dialogTask);
    expect(getWorkTask).not.toHaveBeenCalled();
  });

  it("loads a dialog task by id for the active conversation", async () => {
    const store = new WorkTaskStore();

    await expect(store.fetchTask(dialogTask.id)).resolves.toEqual(dialogTask);

    expect(getWorkTask).toHaveBeenCalledWith(dialogTask.id);
    expect(store.tasks).toEqual([]);
    expect(store.getTaskById(dialogTask.id)).toEqual(dialogTask);
  });
});
