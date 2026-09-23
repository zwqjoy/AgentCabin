import { describe, it, expect, vi, beforeEach } from "vitest";
import type { BusEvent, TimelineEntry } from "$lib/types";
import type { WorkArtifactSummary, WorkWorkspaceSummary } from "$lib/types/work";
import {
  deriveWorkProgressPhase,
  isAgentTurnCompleted,
  isWorkTaskCompleted,
} from "$lib/utils/work-progress";

vi.mock("$lib/api", () => ({
  getRun: vi.fn(),
  getBusEvents: vi.fn().mockResolvedValue([]),
  getRunEvents: vi.fn().mockResolvedValue([]),
  getUserSettings: vi.fn().mockResolvedValue({}),
  getAgentSettings: vi.fn().mockResolvedValue({}),
  getPiExtensionUiState: vi.fn().mockResolvedValue(null),
  syncCliSession: vi.fn().mockResolvedValue({ newEvents: 0 }),
}));

vi.mock("$lib/utils/snapshot-cache", () => ({
  readSnapshot: vi.fn().mockResolvedValue(null),
  writeSnapshot: vi.fn().mockResolvedValue(undefined),
  deleteSnapshot: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("$lib/api/work", () => ({
  listWorkspaces: vi.fn(),
  getWorkWorkspace: vi.fn(),
  createWorkspace: vi.fn(),
  restoreWorkspace: vi.fn(),
  archiveWorkspace: vi.fn(),
  listWorkArtifacts: vi.fn(),
  startWorkSession: vi.fn(),
  sendWorkMessage: vi.fn(),
  cancelWorkTurn: vi.fn(),
  resumeWorkSession: vi.fn(),
  stopWorkSession: vi.fn(),
}));

import { WorkSessionStore } from "$lib/stores/work-session-store.svelte";
import {
  resumeWorkSession,
  startWorkSession,
  sendWorkMessage,
  cancelWorkTurn,
  stopWorkSession,
} from "$lib/api/work";
import * as api from "$lib/api";
import simpleChatEvents from "./__fixtures__/simple-chat.json";

describe("Work Session Store & State Transitions", () => {
  const dummyWorkspace: WorkWorkspaceSummary = {
    id: "ws-test-1",
    name: "Test Workspace",
    root: "/path/to/workspace",
    inputDir: "/path/to/workspace/input",
    scratchDir: "/path/to/workspace/scratch",
    outputDir: "/path/to/workspace/output",
    contextDir: "/path/to/workspace/context",
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    artifactCount: 1,
    archived: false,
    accessRoots: [],
    defaultPolicy: {
      executionMode: "direct",
      maxAutomatedSteps: 50,
      allowExternalConnectors: false,
      standingRules: [],
    },
  };

  const dummyArtifact: WorkArtifactSummary = {
    id: "art-1",
    workspaceId: "ws-test-1",
    artifactType: "pptx",
    category: "presentation",
    mimeType: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    previewKind: "office",
    canPreview: true,
    version: 1,
    title: "Presentation.pptx",
    path: "output/Presentation.pptx",
    status: "ready",
    size: 1024,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("requires delivered Artifacts before treating a Work task as completed", () => {
    const inputReady = {
      sessionPhase: "completed",
      runStatus: "completed" as const,
      tasks: [{ id: "t1", text: "Create presentation", status: "completed" as const }],
      artifacts: [dummyArtifact],
      hasPendingPermission: false,
      hasElicitation: false,
    };

    expect(deriveWorkProgressPhase(inputReady)).toBe("awaiting_delivery");
    expect(isWorkTaskCompleted(inputReady)).toBe(false);
    expect(isAgentTurnCompleted("completed")).toBe(true);

    const inputValidated = {
      ...inputReady,
      artifacts: [{ ...dummyArtifact, status: "validated" as const }],
    };
    expect(deriveWorkProgressPhase(inputValidated)).toBe("awaiting_delivery");
    expect(isWorkTaskCompleted(inputValidated)).toBe(false);

    const inputDelivered = {
      ...inputReady,
      artifacts: [{ ...dummyArtifact, status: "delivered" as const }],
    };
    expect(deriveWorkProgressPhase(inputDelivered)).toBe("completed");
    expect(isWorkTaskCompleted(inputDelivered)).toBe(true);
  });

  it("handles Work session continuation and restore state guard", () => {
    const activeSession = {
      workspaceId: dummyWorkspace.id,
      runId: "run-work-100",
      sessionPhase: "idle",
      archived: false,
    };

    expect(activeSession.archived).toBe(false);
    expect(activeSession.runId).toBe("run-work-100");

    const archivedWorkspace = { ...dummyWorkspace, archived: true };
    expect(archivedWorkspace.archived).toBe(true);
  });

  it("pushes optimistic user message immediately on start and send", async () => {
    const store = new WorkSessionStore();
    store.workspaceId = dummyWorkspace.id;

    const mockRun = {
      id: "run-work-101",
      session_id: "pi-session-101",
      workspace_id: dummyWorkspace.id,
      name: "1",
      prompt: "1",
      status: "running" as const,
      created_at: "2026-01-01T00:00:00Z",
      app_mode: "work" as const,
      agent: "pi" as const,
      execution_path: "session_actor" as const,
      cwd: "/path/to/workspace",
      model: "test-model",
      cli_version: "1.0.0",
      cost_usd: 0,
      input_tokens: 0,
      output_tokens: 0,
      cache_read_tokens: 0,
      cache_write_tokens: 0,
      duration_ms: 0,
      auth_mode: "cli",
      started_at: "2026-01-01T00:00:00Z",
      user_turn_count: 1,
      archived: false,
      pinned: false,
    };

    vi.mocked(startWorkSession).mockResolvedValueOnce(mockRun);
    vi.mocked(api.getRun).mockResolvedValue(mockRun);

    const run = await store.start("1");
    expect(run.id).toBe("run-work-101");
    expect(startWorkSession).toHaveBeenCalledWith(
      dummyWorkspace.id,
      "1",
      undefined,
      undefined,
      "office",
      "pi",
    );
    expect(store.session.timeline.some((e) => e.kind === "user" && e.content === "1")).toBe(true);

    store.session.piCapabilities = {
      commandsReady: true,
      planAvailable: false,
      goalAvailable: false,
      permissionAvailable: false,
      sessionTreeAvailable: false,
      forkAvailable: true,
      cloneAvailable: true,
      steerAvailable: true,
      followUpAvailable: true,
      clearQueueAvailable: false,
      navigateTreeAvailable: false,
    };
    vi.mocked(sendWorkMessage).mockResolvedValueOnce();
    await store.send("2");
    expect(store.session.timeline.some((e) => e.kind === "user" && e.content === "2")).toBe(true);
  });

  it("hydrates the started Work run without replacing the optimistic prompt", async () => {
    const store = new WorkSessionStore();
    store.workspaceId = dummyWorkspace.id;
    const mockRun = {
      id: "run-work-hydrate",
      prompt: "Hello",
      workspace_id: dummyWorkspace.id,
      status: "pending" as const,
      app_mode: "work" as const,
      agent: "pi" as const,
      cwd: "/path/to/workspace",
    };
    const events = (simpleChatEvents as BusEvent[]).map((event) => ({
      ...event,
      run_id: mockRun.id,
    }));

    vi.mocked(startWorkSession).mockResolvedValueOnce(mockRun as never);
    vi.mocked(api.getBusEvents).mockResolvedValueOnce(events);
    vi.mocked(api.getRun).mockResolvedValue(mockRun as never);

    await store.start("Hello");
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Startup follows the ordinary session path: one initial replay plus the
    // live bus subscription, with no background durable-event polling.
    expect(api.getBusEvents).toHaveBeenCalledTimes(1);
    expect(store.session.timeline.filter((entry) => entry.kind === "user")).toHaveLength(1);
    expect(store.session.timeline.some((entry) => entry.kind === "assistant")).toBe(true);
    expect(store.session.phase).toBe("idle");
  });

  it("preserves the existing transcript when resuming a Work run", async () => {
    const store = new WorkSessionStore();
    const stoppedRun = {
      id: "run-work-resume-history",
      session_id: "pi-session-resume-history",
      prompt: "查询历史信息",
      cwd: "/path/to/workspace",
      agent: "pi" as const,
      auth_mode: "cli" as const,
      status: "stopped" as const,
      execution_path: "session_actor" as const,
    };
    const resumedRun = { ...stoppedRun, status: "running" as const };

    store.session.run = stoppedRun as never;
    store.session.timeline = [
      {
        kind: "user",
        id: "old-user",
        anchorId: "old-user",
        content: "查询历史信息",
        ts: "2026-01-01T00:00:00Z",
      },
      {
        kind: "assistant",
        id: "old-assistant",
        anchorId: "old-assistant",
        content: "这是之前的回答",
        ts: "2026-01-01T00:00:01Z",
      },
    ] as TimelineEntry[];
    const loadRun = vi.spyOn(store.session, "loadRun");
    vi.mocked(resumeWorkSession).mockResolvedValueOnce(resumedRun as never);

    await store.resume("你好");

    expect(resumeWorkSession).toHaveBeenCalledWith(stoppedRun.id, "你好", undefined);
    expect(loadRun).not.toHaveBeenCalled();
    expect(
      store.session.timeline
        .filter(
          (entry): entry is Extract<TimelineEntry, { kind: "user" | "assistant" }> =>
            entry.kind === "user" || entry.kind === "assistant",
        )
        .map((entry) => entry.content),
    ).toEqual(["查询历史信息", "这是之前的回答", "你好"]);
    expect((store.session.run as { status?: string } | null)?.status).toBe("running");
  });

  it("stops without replaying and preserves the visible transcript", async () => {
    const store = new WorkSessionStore();
    const run = {
      id: "run-work-stop-preserve",
      workspace_id: dummyWorkspace.id,
      prompt: "分析长文本",
      status: "running" as const,
      app_mode: "work" as const,
      agent: "pi" as const,
      execution_path: "session_actor" as const,
      cwd: "/path/to/workspace",
    };
    store.workspaceId = dummyWorkspace.id;
    store.session.run = run as never;
    store.session.phase = "running";
    store.session.timeline = [
      {
        kind: "user",
        id: "stop-user",
        anchorId: "stop-user",
        content: run.prompt,
        ts: "2026-01-01T00:00:00Z",
      },
    ] as TimelineEntry[];

    const loadRun = vi.spyOn(store.session, "loadRun");
    vi.mocked(stopWorkSession).mockResolvedValueOnce({ ...run, status: "stopped" } as never);

    await store.stop();

    expect(stopWorkSession).toHaveBeenCalledWith(run.id);
    expect(loadRun).not.toHaveBeenCalled();
    expect(store.session.phase).toBe("stopped");
    expect(store.session.timeline).toHaveLength(1);
    expect((store.session.run as { status?: string } | null)?.status).toBe("stopped");
  });

  it("cancels a running turn without stopping the provider session", async () => {
    const store = new WorkSessionStore();
    const run = {
      id: "run-pi-cancel",
      workspace_id: dummyWorkspace.id,
      prompt: "输出一份长报告",
      status: "running" as const,
      app_mode: "work" as const,
      agent: "pi" as const,
      execution_path: "session_actor" as const,
      cwd: "/path/to/workspace",
    };
    store.workspaceId = dummyWorkspace.id;
    store.session.run = run as never;
    store.session.phase = "running";
    // sessionModeControl is false in Pi's static defaults; it must be negotiated
    // via ACP session_init at runtime. Simulate that negotiation so canCancelTurn works.
    store.session.sessionCapabilities = { protocol: { sessionModeControl: true } } as never;
    vi.mocked(cancelWorkTurn).mockResolvedValueOnce(undefined);

    expect(store.canCancelTurn).toBe(true);
    await store.cancelTurn();

    expect(cancelWorkTurn).toHaveBeenCalledWith(run.id);
    expect(stopWorkSession).not.toHaveBeenCalled();
    expect(store.cancellingTurn).toBe(false);
  });

  it("releases the Work UI when the stop IPC never settles", async () => {
    vi.useFakeTimers();
    try {
      const store = new WorkSessionStore();
      const run = {
        id: "run-work-stop-hanging",
        workspace_id: dummyWorkspace.id,
        prompt: "停止卡住的任务",
        status: "running" as const,
        app_mode: "work" as const,
        agent: "pi" as const,
        execution_path: "session_actor" as const,
        cwd: "/path/to/workspace",
      };
      store.workspaceId = dummyWorkspace.id;
      store.session.run = run as never;
      store.session.phase = "running";
      vi.mocked(stopWorkSession).mockImplementation(() => new Promise<never>(() => {}));

      const stopPromise = store.stop();
      await vi.advanceTimersByTimeAsync(10_000);
      await stopPromise;

      expect(store.session.phase).toBe("stopped");
      expect((store.session.run as { status?: string } | null)?.status).toBe("stopped");
      expect(store.error).toContain("停止 Work 会话超时");
    } finally {
      vi.useRealTimers();
    }
  });

  it("coalesces repeated stop clicks into one IPC request", async () => {
    vi.useFakeTimers();
    try {
      const store = new WorkSessionStore();
      const run = {
        id: "run-work-stop-duplicate",
        workspace_id: dummyWorkspace.id,
        prompt: "重复点击停止",
        status: "running" as const,
        app_mode: "work" as const,
        agent: "pi" as const,
        execution_path: "session_actor" as const,
        cwd: "/path/to/workspace",
      };
      store.workspaceId = dummyWorkspace.id;
      store.session.run = run as never;
      store.session.phase = "running";
      vi.mocked(stopWorkSession).mockImplementation(() => new Promise<never>(() => {}));

      const first = store.stop();
      const second = store.stop();
      expect(stopWorkSession).toHaveBeenCalledTimes(1);
      expect(store.stopping).toBe(true);

      await vi.advanceTimersByTimeAsync(10_000);
      await Promise.all([first, second]);
      expect(store.stopping).toBe(false);
      expect(store.session.phase).toBe("stopped");
    } finally {
      vi.useRealTimers();
    }
  });

  it("clears the loading state when run metadata IPC hangs", async () => {
    vi.useFakeTimers();
    try {
      const store = new WorkSessionStore();
      vi.mocked(api.getRun).mockImplementation(() => new Promise<never>(() => {}));

      const loadPromise = store.load(dummyWorkspace.id, "run-work-hanging", false);
      await vi.advanceTimersByTimeAsync(15_000);
      await loadPromise;

      expect(store.loading).toBe(false);
      expect(store.error).toContain("加载 Work 会话超时");
    } finally {
      vi.useRealTimers();
    }
  });

  it("clears the loading state when the initial session reset throws", async () => {
    const store = new WorkSessionStore();
    vi.spyOn(store.session, "loadRun").mockRejectedValueOnce(new Error("reset failed"));

    await store.load(dummyWorkspace.id);

    expect(store.loading).toBe(false);
    expect(store.error).toBe("reset failed");
  });

  it("does not block a newly started Work run on history catch-up", async () => {
    vi.useFakeTimers();
    try {
      const store = new WorkSessionStore();
      store.workspaceId = dummyWorkspace.id;
      const mockRun = {
        id: "run-work-start-hanging",
        workspace_id: dummyWorkspace.id,
        status: "pending" as const,
        app_mode: "work" as const,
        agent: "pi" as const,
      };
      vi.mocked(startWorkSession).mockResolvedValueOnce(mockRun as never);
      vi.mocked(api.getBusEvents).mockImplementation(() => new Promise<never>(() => {}));

      const run = await store.start("新任务");
      expect(run.id).toBe(mockRun.id);
      expect(store.session.run?.id).toBe(mockRun.id);
      expect(store.session.phase).toBe("spawning");
      expect(store.starting).toBe(false);

      await vi.advanceTimersByTimeAsync(10_000);

      // A slow history read is diagnostic only; it must not turn a live Work
      // run into a failed or permanently loading composer.
      expect(store.error).toBe("");
      expect(store.session.run?.id).toBe(mockRun.id);
    } finally {
      vi.useRealTimers();
    }
  });

  it("projects Pi capabilities from the cold-start baseline to live fail-closed state", async () => {
    const store = new WorkSessionStore();

    expect(store.composerCapabilities.runtime.steer).toBe(true);
    expect(store.composerCapabilities.runtime.followUp).toBe(true);
    expect(store.canSteer).toBe(false);
    expect(store.canFollowUp).toBe(false);

    store.session.agent = "pi";
    store.session.run = {
      id: "run-work-capabilities",
      prompt: "test",
      cwd: "/path/to/workspace",
      agent: "pi",
      auth_mode: "cli",
      status: "running",
      started_at: "2026-01-01T00:00:00Z",
      execution_path: "session_actor",
    } as never;
    store.session.phase = "running";

    expect(store.composerCapabilities.runtime.steer).toBe(false);
    expect(store.composerCapabilities.runtime.followUp).toBe(false);
    expect(store.canSteer).toBe(false);
    expect(store.canFollowUp).toBe(false);

    store.session.piCapabilities = {
      commandsReady: true,
      planAvailable: false,
      goalAvailable: false,
      permissionAvailable: false,
      sessionTreeAvailable: true,
      forkAvailable: true,
      cloneAvailable: true,
      steerAvailable: true,
      followUpAvailable: true,
      clearQueueAvailable: false,
      navigateTreeAvailable: true,
    };

    expect(store.composerCapabilities.runtime.steer).toBe(true);
    expect(store.composerCapabilities.runtime.followUp).toBe(true);
    expect(store.canSteer).toBe(true);
    expect(store.canFollowUp).toBe(true);
    expect(store.canFork).toBe(true);
    expect(store.canClone).toBe(true);
    expect(store.canOpenSessionTree).toBe(true);

    const sendQueuedMessage = vi
      .spyOn(store.session, "sendQueuedMessage")
      .mockResolvedValue(undefined);
    await store.sendQueuedMessage("稍后执行", [], "followUp");
    expect(sendQueuedMessage).toHaveBeenCalledWith("稍后执行", [], "followUp");

    store.session.piCapabilities = {
      ...store.session.piCapabilities,
      followUpAvailable: false,
      steerAvailable: true,
    };
    await store.sendQueuedMessage("立即引导", [], "steer");
    expect(sendQueuedMessage).toHaveBeenCalledWith("立即引导", [], "steer");

    store.session.piCapabilities = {
      ...store.session.piCapabilities,
      steerAvailable: false,
    };
    await store.sendQueuedMessage("不可用", [], "steer");
    expect(sendQueuedMessage).toHaveBeenCalledTimes(2);
    expect(store.error).toContain("Steer");
  });

  it("treats legacy non-Pi Work run as read-only with disabled capabilities", () => {
    const store = new WorkSessionStore();
    store.session.run = { id: "legacy-dsh", agent: "dsh" } as never;

    expect(store.composerCapabilities.runtime.steer).toBe(false);
    expect(store.canSteer).toBe(false);
    expect(store.canClone).toBe(false);
  });

  it("keeps Pi transport diagnostics out of the Work timeline projection", async () => {
    const store = new WorkSessionStore();
    store.session.timeline = [
      {
        kind: "command_output",
        id: "diagnostic",
        anchorId: "diagnostic",
        content: "[pi rpc stderr] internal transport detail",
        ts: "2026-01-01T00:00:00Z",
      },
      {
        kind: "command_output",
        id: "visible",
        anchorId: "visible",
        content: "Work-visible output",
        ts: "2026-01-01T00:00:01Z",
      },
    ] as TimelineEntry[];

    expect(store.visibleTimeline.map((entry) => entry.id)).toEqual(["visible"]);
  });
});
