import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const eventState = vi.hoisted(() => ({
  handler: null as ((event: { run_id: string; type: string; projection?: unknown }) => void) | null,
}));

vi.mock("$lib/api/work", () => ({
  getWorkProjection: vi.fn(),
}));

vi.mock("$lib/stores/event-middleware", () => ({
  getEventMiddleware: () => ({
    subscribeEvents: (
      handler: (event: { run_id: string; type: string; projection?: unknown }) => void,
    ) => {
      eventState.handler = handler;
      return () => {
        if (eventState.handler === handler) eventState.handler = null;
      };
    },
  }),
}));

import { getWorkProjection } from "$lib/api/work";
import type { WorkProjection } from "$lib/types/work";
import { WorkProjectionStore } from "./work-projection-store.svelte";

const mockProjection = (overrides?: Partial<WorkProjection>): WorkProjection => ({
  runId: "run-1",
  taskId: "run-1",
  workRunId: "run-1",
  workspaceId: "ws-1",
  automationTaskId: null,
  sessionId: "session-1",
  runStatus: "running",
  status: "running",
  phase: "running",
  goal: "修复登录问题",
  plan: [
    { id: "s1", text: "调查日志", status: "completed" },
    { id: "s2", text: "修复代码", status: "in_progress" },
  ],
  steps: [
    { id: "s1", text: "调查日志", status: "completed" },
    { id: "s2", text: "修复代码", status: "in_progress" },
  ],
  checkpoint: {
    summary: "已定位到错误",
    currentStepId: "s2",
    createdAt: "2026-08-20T20:00:00Z",
  },
  currentActivity: {
    kind: "tool",
    toolName: "work_run_command",
    detail: "正在运行命令",
  },
  attention: null,
  health: null,
  agents: [],
  toolSummary: {
    proposed: 3,
    started: 2,
    completed: 1,
    failed: 0,
    running: 1,
  },
  artifacts: [],
  pendingInteractions: [],
  availableActions: {
    canSend: false,
    canStop: true,
    canResume: false,
    canRecover: false,
  },
  runtimeStatus: "running",
  automationStatus: null,
  updatedAt: "2026-08-20T20:00:00Z",
  ...overrides,
});

describe("WorkProjectionStore", () => {
  let store: WorkProjectionStore;

  beforeEach(() => {
    vi.useFakeTimers();
    store = new WorkProjectionStore();
    vi.mocked(getWorkProjection).mockReset();
    eventState.handler = null;
  });

  afterEach(() => {
    store.clear();
    vi.useRealTimers();
  });

  it("fetches and caches projection by runId", async () => {
    const proj = mockProjection();
    vi.mocked(getWorkProjection).mockResolvedValue(proj);

    const result = await store.fetch("run-1");
    expect(result).toEqual(proj);
    expect(store.getProjection("run-1")).toEqual(proj);
    expect(store.getProgress("run-1")).toEqual(proj);
    expect(getWorkProjection).toHaveBeenCalledWith("run-1");
  });

  it("applies work_projection_changed event directly without network fetch", async () => {
    const proj1 = mockProjection({ phase: "running" });
    const proj2 = mockProjection({
      phase: "idle",
      runStatus: "completed",
      availableActions: { canSend: true, canStop: false, canResume: false, canRecover: false },
    });
    vi.mocked(getWorkProjection).mockResolvedValue(proj1);

    store.subscribe("run-1");
    await vi.advanceTimersByTimeAsync(0);
    expect(store.getProjection("run-1")?.phase).toBe("running");

    // Emit event directly
    eventState.handler?.({
      run_id: "run-1",
      type: "work_projection_changed",
      projection: proj2,
    });

    expect(store.getProjection("run-1")?.phase).toBe("idle");
    expect(store.getProjection("run-1")?.availableActions.canSend).toBe(true);
    // getWorkProjection was called only once on subscribe, not again on event
    expect(getWorkProjection).toHaveBeenCalledTimes(1);
  });

  it("determines terminal state by scope accurately", () => {
    // Interactive idle session must NOT be terminal (listening continues)
    const interactiveIdle = mockProjection({
      runtimeStatus: "idle",
      automationStatus: null,
      status: "completed",
    });
    expect(store.isTerminal(interactiveIdle)).toBe(false);

    // Interactive failed session IS terminal
    const interactiveFailed = mockProjection({
      runtimeStatus: "failed",
      automationStatus: null,
      status: "failed",
    });
    expect(store.isTerminal(interactiveFailed)).toBe(true);

    // Automation completed IS terminal
    const automationCompleted = mockProjection({
      runtimeStatus: "running",
      automationStatus: "completed",
    });
    expect(store.isTerminal(automationCompleted)).toBe(true);

    // Automation running is NOT terminal
    const automationRunning = mockProjection({
      runtimeStatus: "running",
      automationStatus: "running",
    });
    expect(store.isTerminal(automationRunning)).toBe(false);
  });
});
