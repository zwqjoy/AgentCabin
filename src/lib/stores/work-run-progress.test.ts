import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const eventState = vi.hoisted(() => ({
  handler: null as ((event: { run_id: string; type: string }) => void) | null,
}));

vi.mock("$lib/api/work", () => ({
  getWorkRunProgress: vi.fn(),
  getWorkProjection: vi.fn(),
}));

vi.mock("$lib/stores/event-middleware", () => ({
  getEventMiddleware: () => ({
    subscribeEvents: (handler: (event: { run_id: string; type: string }) => void) => {
      eventState.handler = handler;
      return () => {
        if (eventState.handler === handler) eventState.handler = null;
      };
    },
  }),
}));

import { getWorkRunProgress } from "$lib/api/work";
import type { WorkRunProgressView } from "$lib/types/work";
import { WorkRunProgressStore } from "./work-run-progress.svelte";

const mockView = (overrides?: Partial<WorkRunProgressView>): WorkRunProgressView => ({
  taskId: "task-1",
  workRunId: "run-1",
  sessionId: "session-1",
  runStatus: "running",
  phase: "running",
  goal: "修复登录问题",
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
  agents: [
    {
      agentId: "worker-1",
      childIndex: 1,
      role: "worker",
      status: "running",
      resultSummary: null,
      error: null,
    },
  ],
  toolSummary: {
    proposed: 3,
    started: 2,
    completed: 1,
    failed: 0,
    running: 1,
  },
  updatedAt: "2026-08-20T20:00:00Z",
  ...overrides,
});

describe("WorkRunProgressStore", () => {
  let store: WorkRunProgressStore;

  beforeEach(() => {
    vi.useFakeTimers();
    store = new WorkRunProgressStore();
    vi.mocked(getWorkRunProgress).mockReset();
    eventState.handler = null;
  });

  afterEach(() => {
    store.clear();
    vi.useRealTimers();
  });

  it("fetches and caches progress for a run", async () => {
    const view = mockView();
    vi.mocked(getWorkRunProgress).mockResolvedValue(view);

    const result = await store.fetch("task-1", "run-1");
    expect(result).toEqual(view);
    expect(store.getProgress("run-1")).toEqual(view);
    expect(getWorkRunProgress).toHaveBeenCalledWith("task-1", "run-1");
  });

  it("deduplicates concurrent fetches for the same run", async () => {
    const view = mockView();
    let resolvePromise: (v: WorkRunProgressView) => void;
    const promise = new Promise<WorkRunProgressView>((resolve) => {
      resolvePromise = resolve;
    });
    vi.mocked(getWorkRunProgress).mockReturnValue(promise);

    const fetch1 = store.fetch("task-1", "run-1");
    const fetch2 = store.fetch("task-1", "run-1");

    expect(getWorkRunProgress).toHaveBeenCalledTimes(1);

    resolvePromise!(view);
    const [res1, res2] = await Promise.all([fetch1, fetch2]);
    expect(res1).toEqual(view);
    expect(res2).toEqual(view);
  });

  it("uses event watching without a polling timer", async () => {
    vi.mocked(getWorkRunProgress).mockResolvedValue(mockView({ runStatus: "running" }));

    const unsubscribe = store.subscribe("task-1", "run-1");
    await vi.advanceTimersByTimeAsync(0);
    expect(getWorkRunProgress).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(10_000);
    expect(getWorkRunProgress).toHaveBeenCalledTimes(1);
    unsubscribe();
  });

  it("refreshes when runtime events use the linked session id", async () => {
    const activeView = mockView({ sessionId: "session-1", runStatus: "running" });
    const terminalView = mockView({
      sessionId: "session-1",
      runStatus: "completed",
      phase: "completed",
    });
    vi.mocked(getWorkRunProgress)
      .mockResolvedValueOnce(activeView)
      .mockResolvedValueOnce(terminalView);

    const unsubscribe = store.subscribe("task-1", "run-1");
    await vi.advanceTimersByTimeAsync(0);
    expect(eventState.handler).not.toBeNull();

    eventState.handler?.({ run_id: "unrelated-session", type: "run_state" });
    await vi.advanceTimersByTimeAsync(100);
    expect(getWorkRunProgress).toHaveBeenCalledTimes(1);

    eventState.handler?.({ run_id: "session-1", type: "run_state" });
    await vi.advanceTimersByTimeAsync(80);
    expect(getWorkRunProgress).toHaveBeenCalledTimes(2);
    expect(store.getProgress("run-1")?.runStatus).toBe("completed");

    unsubscribe();
  });

  it("does not refresh durable progress for streamed text deltas", async () => {
    vi.mocked(getWorkRunProgress).mockResolvedValue(
      mockView({ sessionId: "session-1", runStatus: "running" }),
    );

    const unsubscribe = store.subscribe("task-1", "run-1");
    await vi.advanceTimersByTimeAsync(0);
    expect(getWorkRunProgress).toHaveBeenCalledTimes(1);

    for (let index = 0; index < 200; index += 1) {
      eventState.handler?.({
        run_id: "session-1",
        type: index % 2 === 0 ? "thinking_delta" : "message_delta",
      });
    }
    await vi.advanceTimersByTimeAsync(1_000);
    expect(getWorkRunProgress).toHaveBeenCalledTimes(1);

    eventState.handler?.({ run_id: "session-1", type: "tool_start" });
    await vi.advanceTimersByTimeAsync(80);
    expect(getWorkRunProgress).toHaveBeenCalledTimes(2);

    unsubscribe();
  });

  it("keeps one trailing lifecycle refresh when an event arrives during a fetch", async () => {
    let resolveInitial: (view: WorkRunProgressView) => void;
    const initial = new Promise<WorkRunProgressView>((resolve) => {
      resolveInitial = resolve;
    });
    vi.mocked(getWorkRunProgress)
      .mockReturnValueOnce(initial)
      .mockResolvedValueOnce(mockView({ runStatus: "running" }));

    const unsubscribe = store.subscribe("task-1", "run-1");
    expect(getWorkRunProgress).toHaveBeenCalledTimes(1);

    eventState.handler?.({ run_id: "run-1", type: "tool_end" });
    eventState.handler?.({ run_id: "run-1", type: "run_state" });
    resolveInitial!(mockView({ runStatus: "running" }));
    await vi.advanceTimersByTimeAsync(80);

    expect(getWorkRunProgress).toHaveBeenCalledTimes(2);
    unsubscribe();
  });

  it("handles multiple subscribers gracefully", async () => {
    const activeView = mockView({ runStatus: "running" });
    vi.mocked(getWorkRunProgress).mockResolvedValue(activeView);

    const unsub1 = store.subscribe("task-1", "run-1");
    const unsub2 = store.subscribe("task-1", "run-1");

    await vi.advanceTimersByTimeAsync(0);

    // Unsubscribe first subscriber -> the event watcher stays active
    unsub1();
    eventState.handler?.({ run_id: "run-1", type: "run_state" });
    await vi.advanceTimersByTimeAsync(80);
    expect(getWorkRunProgress).toHaveBeenCalledTimes(2);

    // Unsubscribe second subscriber -> the event watcher stops
    unsub2();
    eventState.handler?.({ run_id: "run-1", type: "run_state" });
    await vi.advanceTimersByTimeAsync(200);
    expect(getWorkRunProgress).toHaveBeenCalledTimes(2);
  });
});
