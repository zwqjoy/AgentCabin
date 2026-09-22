import { describe, expect, it } from "vitest";
import type { WorkRunProgressView } from "$lib/types/work";
import {
  mapProgressViewToStatusLabel,
  mapRunProgressPhaseToUIPhase,
  workProgressPercent,
  workRunProgressDisplayPhase,
  workRunProgressToSnapshot,
} from "$lib/utils/work-progress";

describe("Work Run Progress Projection to UI mapping", () => {
  it("maps all backend WorkRunProgressPhases to appropriate UI phases", () => {
    expect(mapRunProgressPhaseToUIPhase("queued")).toBe("planning");
    expect(mapRunProgressPhaseToUIPhase("planning")).toBe("planning");
    expect(mapRunProgressPhaseToUIPhase("running")).toBe("running");
    expect(mapRunProgressPhaseToUIPhase("researching")).toBe("running");
    expect(mapRunProgressPhaseToUIPhase("implementing")).toBe("running");
    expect(mapRunProgressPhaseToUIPhase("reviewing")).toBe("running");
    expect(mapRunProgressPhaseToUIPhase("waiting_user")).toBe("waiting_approval");
    expect(mapRunProgressPhaseToUIPhase("blocked")).toBe("failed");
    expect(mapRunProgressPhaseToUIPhase("failed")).toBe("failed");
    expect(mapRunProgressPhaseToUIPhase("completed")).toBe("completed");
    expect(mapRunProgressPhaseToUIPhase("cancelled")).toBe("cancelled");
  });

  it("converts WorkRunProgressView to WorkProgressSnapshot faithfully", () => {
    const view: WorkRunProgressView = {
      taskId: "task-100",
      workRunId: "run-100",
      sessionId: "session-100",
      runStatus: "running",
      phase: "implementing",
      goal: "优化重试机制",
      steps: [
        { id: "s1", text: "重构网络模块", status: "completed" },
        { id: "s2", text: "增加单元测试", status: "in_progress" },
        { id: "s3", text: "验证回滚", status: "pending" },
      ],
      checkpoint: {
        summary: "网络模块已重构完成",
        currentStepId: "s2",
        createdAt: "2026-08-20T21:00:00Z",
      },
      currentActivity: {
        kind: "implementing",
        stepId: "s2",
        toolName: "work_write_file",
        agentRole: "worker",
        detail: "正在实现修改",
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
        {
          agentId: "researcher-1",
          childIndex: 2,
          role: "researcher",
          status: "completed",
          resultSummary: "已找到参考文档",
          error: null,
        },
      ],
      toolSummary: {
        proposed: 5,
        started: 4,
        completed: 3,
        failed: 0,
        running: 1,
      },
      updatedAt: "2026-08-20T21:00:00Z",
    };

    const snapshot = workRunProgressToSnapshot(view);

    expect(snapshot.phase).toBe("running");
    expect(snapshot.tasks).toEqual(view.steps);
    expect(snapshot.taskState?.goal).toBe("优化重试机制");
    expect(snapshot.taskState?.plan).toEqual(view.steps);
    expect(snapshot.taskState?.checkpoint?.summary).toBe("网络模块已重构完成");
    expect(snapshot.activeToolName).toBe("work_write_file");
    expect(snapshot.toolCallCount).toBe(4);
    expect(snapshot.subagents).toHaveLength(2);
    expect(snapshot.subagents![0]).toEqual({
      id: "worker-1",
      agentId: "worker-1",
      role: "worker",
      task: "",
      status: "running",
      statusText: "正在执行",
    });
    expect(snapshot.subagents![1]).toEqual({
      id: "researcher-1",
      agentId: "researcher-1",
      role: "researcher",
      task: "已找到参考文档",
      resultSummary: "已找到参考文档",
      status: "completed",
      statusText: "已完成",
    });
  });

  it("handles attention mapping in workRunProgressToSnapshot", () => {
    const view: WorkRunProgressView = {
      taskId: "task-2",
      workRunId: "run-2",
      sessionId: "session-2",
      runStatus: "running",
      phase: "waiting_user",
      goal: "执行系统部署",
      steps: [],
      checkpoint: null,
      currentActivity: {
        kind: "waiting_user",
        detail: "等待授权目录访问",
      },
      attention: {
        kind: "access_root_request",
        count: 1,
      },
      agents: [],
      toolSummary: {
        proposed: 1,
        started: 0,
        completed: 0,
        failed: 0,
        running: 0,
      },
      updatedAt: "2026-08-20T21:00:00Z",
    };

    const snapshot = workRunProgressToSnapshot(view);
    expect(snapshot.phase).toBe("waiting_approval");
    expect(snapshot.pendingApprovalCount).toBe(1);
    expect(snapshot.pendingAccessRoot).toBe(true);
    expect(snapshot.pendingElicitation).toBe(false);
  });

  it("does not project stale attention for a terminal WorkRun", () => {
    const view: WorkRunProgressView = {
      taskId: "task-cancelled",
      workRunId: "run-cancelled",
      sessionId: "session-cancelled",
      runStatus: "cancelled",
      phase: "waiting_user",
      goal: null,
      steps: [],
      checkpoint: null,
      currentActivity: null,
      attention: { kind: "user_input", count: 1 },
      agents: [],
      toolSummary: { proposed: 0, started: 0, completed: 0, failed: 0, running: 0 },
      updatedAt: "2026-08-20T21:00:00Z",
    };

    const snapshot = workRunProgressToSnapshot(view);
    expect(snapshot.phase).toBe("cancelled");
    expect(snapshot.pendingApprovalCount).toBe(0);
    expect(snapshot.pendingAccessRoot).toBe(false);
    expect(snapshot.pendingElicitation).toBe(false);
  });

  it("calculates progress percentage accurately", () => {
    expect(workProgressPercent([])).toBeNull();
    expect(
      workProgressPercent([
        { id: "1", text: "a", status: "completed" },
        { id: "2", text: "b", status: "in_progress" },
      ]),
    ).toBe(50);
    expect(
      workProgressPercent([
        { id: "1", text: "a", status: "completed" },
        { id: "2", text: "b", status: "completed" },
        { id: "3", text: "c", status: "completed" },
      ]),
    ).toBe(100);
  });

  it("maps WorkRunProgressView to top-level status labels correctly", () => {
    const base: WorkRunProgressView = {
      taskId: "task-1",
      workRunId: "run-1",
      sessionId: "session-1",
      runStatus: "running",
      phase: "running",
      goal: null,
      steps: [],
      checkpoint: null,
      currentActivity: null,
      attention: null,
      agents: [],
      toolSummary: { proposed: 0, started: 0, completed: 0, failed: 0, running: 0 },
      updatedAt: "2026-08-20T21:00:00Z",
    };

    expect(mapProgressViewToStatusLabel({ ...base, phase: "planning" })).toBe("正在制定计划…");
    expect(mapProgressViewToStatusLabel({ ...base, phase: "researching" })).toBe("正在调查研究…");
    expect(mapProgressViewToStatusLabel({ ...base, phase: "implementing" })).toBe("正在执行任务…");
    expect(mapProgressViewToStatusLabel({ ...base, phase: "reviewing" })).toBe("正在独立审查…");
    expect(mapProgressViewToStatusLabel({ ...base, phase: "waiting_user" })).toBe("等待你处理");
    expect(
      mapProgressViewToStatusLabel({
        ...base,
        phase: "running",
        attention: { kind: "access_root_request", count: 1 },
      }),
    ).toBe("等待你处理");
    expect(
      mapProgressViewToStatusLabel({ ...base, phase: "completed", runStatus: "completed" }),
    ).toBe("等待下一步");
    expect(
      mapProgressViewToStatusLabel({
        ...base,
        phase: "completed",
        runStatus: "completed",
        toolSummary: { proposed: 1, started: 1, completed: 1, failed: 0, running: 0 },
      }),
    ).toBe("任务已完成");
    expect(
      mapProgressViewToStatusLabel({
        ...base,
        phase: "waiting_user",
        runStatus: "cancelled",
        attention: { kind: "user_input", count: 1 },
      }),
    ).toBe("已取消");
    expect(
      workRunProgressDisplayPhase(
        {
          ...base,
          phase: "waiting_user",
          runStatus: "cancelled",
          attention: { kind: "user_input", count: 1 },
        },
        [],
      ),
    ).toBe("cancelled");
    expect(mapProgressViewToStatusLabel({ ...base, phase: "failed", runStatus: "failed" })).toBe(
      "执行失败",
    );
    expect(
      mapProgressViewToStatusLabel({ ...base, phase: "cancelled", runStatus: "cancelled" }),
    ).toBe("已取消");
  });
});
