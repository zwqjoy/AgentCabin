import { describe, expect, it } from "vitest";
import {
  computeAutomationQuickStats,
  formatAutomationRunStatus,
  formatAutomationTrigger,
  formatRunDuration,
} from "../work-automation";
import type { WorkTask } from "$lib/types/work";

describe("work-automation utility", () => {
  it("computes automation quick stats accurately", () => {
    const tasks: WorkTask[] = [
      {
        id: "t1",
        workspaceId: "ws-1",
        title: "每日分析",
        instructions: "",
        status: "in_run",
        source: "manual",
        policy: {
          executionMode: "auto",
          maxAutomatedSteps: 30,
          allowExternalConnectors: true,
          standingRules: [],
        },
        schedule: {
          kind: "daily",
          enabled: true,
          timezone: "UTC",
        },
        requiredArtifacts: [],
        artifactRequirements: [],
        runCount: 3,
        lastRunId: "run-1",
        lastRunAt: "2026-08-30T00:00:00Z",
        createdAt: "",
        updatedAt: "",
      },
      {
        id: "t2",
        workspaceId: "ws-1",
        title: "周报生成",
        instructions: "",
        status: "active",
        source: "manual",
        policy: {
          executionMode: "auto",
          maxAutomatedSteps: 30,
          allowExternalConnectors: true,
          standingRules: [],
        },
        schedule: {
          kind: "weekly",
          enabled: false,
          timezone: "UTC",
        },
        requiredArtifacts: [],
        artifactRequirements: [],
        runCount: 0,
        lastRunId: null,
        lastRunAt: null,
        createdAt: "",
        updatedAt: "",
      },
      {
        id: "t3",
        workspaceId: "ws-1",
        title: "待人工处理任务",
        instructions: "",
        status: "needs_attention",
        source: "manual",
        policy: {
          executionMode: "auto",
          maxAutomatedSteps: 30,
          allowExternalConnectors: true,
          standingRules: [],
        },
        schedule: null,
        requiredArtifacts: [],
        artifactRequirements: [],
        runCount: 1,
        lastRunId: "run-3",
        lastRunAt: "2026-08-29T10:00:00Z",
        createdAt: "",
        updatedAt: "",
      },
    ];

    const stats = computeAutomationQuickStats(tasks);
    expect(stats.total).toBe(3);
    expect(stats.scheduled).toBe(1);
    expect(stats.inRun).toBe(1);
    expect(stats.needsAttention).toBe(1);
  });

  it("formats trigger labels correctly", () => {
    expect(formatAutomationTrigger("scheduled").label).toBe("定时触发");
    expect(formatAutomationTrigger("event_triggered").label).toBe("事件触发");
    expect(formatAutomationTrigger("manual").label).toBe("手动触发");
  });

  it("formats run statuses properly", () => {
    expect(formatAutomationRunStatus("completed").label).toBe("已完成");
    expect(formatAutomationRunStatus("failed").label).toBe("执行失败");
    expect(formatAutomationRunStatus("running").label).toBe("运行中");
    expect(formatAutomationRunStatus("waiting_input").label).toBe("需人工确认");
  });

  it("formats run durations cleanly", () => {
    expect(formatRunDuration(null)).toBe("-");
    expect(formatRunDuration(0)).toBe("-");
    expect(formatRunDuration(15000)).toBe("15秒");
    expect(formatRunDuration(125000)).toBe("2分5秒");
    expect(formatRunDuration(3665000)).toBe("1小时1分");
  });
});
