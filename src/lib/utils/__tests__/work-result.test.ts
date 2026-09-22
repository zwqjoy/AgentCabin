import { describe, expect, it } from "vitest";
import type {
  WorkArtifactSummary,
  WorkProgressSnapshot,
  WorkRunProgressView,
} from "$lib/types/work";
import {
  deriveWorkResultPresentation,
  formatArtifactCategory,
  formatArtifactStatus,
  formatArtifactType,
  formatDurationMs,
  formatFileSize,
  hasOperationalWorkEvidence,
  hasWorkCompletionEvidence,
} from "$lib/utils/work-result";

const mockArtifact = (overrides?: Partial<WorkArtifactSummary>): WorkArtifactSummary => ({
  id: "art-1",
  workspaceId: "ws-1",
  runId: "run-1",
  artifactType: "pdf",
  category: "pdf",
  mimeType: "application/pdf",
  previewKind: "pdf",
  canPreview: true,
  version: 1,
  title: "分析报告.pdf",
  path: "output/report.pdf",
  status: "delivered",
  size: 24576,
  createdAt: "2026-08-20T20:00:00Z",
  updatedAt: "2026-08-20T20:00:00Z",
  ...overrides,
});

const mockProgressView = (overrides?: Partial<WorkRunProgressView>): WorkRunProgressView => ({
  taskId: "task-1",
  workRunId: "run-1",
  sessionId: "session-1",
  runStatus: "completed",
  phase: "completed",
  goal: "修复登录问题",
  steps: [
    { id: "s1", text: "定位问题", status: "completed" },
    { id: "s2", text: "修复代码", status: "completed" },
  ],
  checkpoint: null,
  currentActivity: null,
  attention: null,
  agents: [],
  toolSummary: { proposed: 2, started: 2, completed: 2, failed: 0, running: 0 },
  updatedAt: "2026-08-20T20:00:00Z",
  ...overrides,
});

describe("Work Result Presentation Model (deriveWorkResultPresentation)", () => {
  // Case 1: completed + 2 delivered -> outcome completed, artifactCount 2, deliveredCount 2, primaryArtifact exists
  it("Case 1: handles completed run with delivered artifacts", () => {
    const art1 = mockArtifact({ id: "a1", title: "report.pdf", status: "delivered" });
    const art2 = mockArtifact({ id: "a2", title: "data.xlsx", status: "delivered" });
    const view = mockProgressView({ runStatus: "completed", phase: "completed" });

    const result = deriveWorkResultPresentation({
      progressView: view,
      artifacts: [art1, art2],
    });

    expect(result.outcome).toBe("completed");
    expect(result.isTerminal).toBe(true);
    expect(result.title).toBe("任务已完成");
    expect(result.artifactCount).toBe(2);
    expect(result.deliveredCount).toBe(2);
    expect(result.problematicCount).toBe(0);
    expect(result.hasFileResults).toBe(true);
    expect(result.primaryArtifact?.id).toBe("a1");
    expect(result.completedSteps).toBe(2);
    expect(result.totalSteps).toBe(2);
  });

  // Case 2: completed + no artifacts -> valid completed result, hasFileResults false
  it("Case 2: handles completed run with zero artifacts without claiming failure", () => {
    const view = mockProgressView({
      runStatus: "completed",
      phase: "completed",
      goal: "解释系统架构",
    });

    const result = deriveWorkResultPresentation({
      progressView: view,
      artifacts: [],
    });

    expect(result.outcome).toBe("completed");
    expect(result.isTerminal).toBe(true);
    expect(result.title).toBe("任务已完成");
    expect(result.subtitle).toBe("执行已完成，没有文件型成果。");
    expect(result.hasFileResults).toBe(false);
    expect(result.artifactCount).toBe(0);
    expect(result.primaryArtifact).toBeUndefined();
  });

  // Case 3: failed + delivered artifact -> outcome failed, artifact remains accessible
  it("Case 3: handles failed run with partially produced artifacts", () => {
    const art = mockArtifact({ id: "a1", title: "partial.csv", status: "delivered" });
    const view = mockProgressView({
      runStatus: "failed",
      phase: "failed",
    });

    const result = deriveWorkResultPresentation({
      progressView: view,
      artifacts: [art],
    });

    expect(result.outcome).toBe("failed");
    expect(result.isTerminal).toBe(true);
    expect(result.title).toBe("任务未完全完成");
    expect(result.subtitle).toContain("执行过程中产生了 1 个成果文件");
    expect(result.hasFileResults).toBe(true);
    expect(result.primaryArtifact?.id).toBe("a1");
  });

  // Case 4: cancelled + artifact -> cancelled, artifact retained
  it("Case 4: handles cancelled run with retained artifacts", () => {
    const art = mockArtifact({ id: "a1", title: "draft.md", status: "delivered" });
    const view = mockProgressView({
      runStatus: "cancelled",
      phase: "cancelled",
    });

    const result = deriveWorkResultPresentation({
      progressView: view,
      artifacts: [art],
    });

    expect(result.outcome).toBe("cancelled");
    expect(result.isTerminal).toBe(true);
    expect(result.title).toBe("任务已取消");
    expect(result.subtitle).toBe("已产生的文件仍可查看。");
    expect(result.hasFileResults).toBe(true);
  });

  // Case 5: delivered + invalid -> deliveredCount 1, problematicCount 1
  it("Case 5: differentiates between delivered and invalid artifacts", () => {
    const art1 = mockArtifact({ id: "a1", status: "delivered" });
    const art2 = mockArtifact({ id: "a2", status: "invalid" });
    const view = mockProgressView({ runStatus: "completed" });

    const result = deriveWorkResultPresentation({
      progressView: view,
      artifacts: [art1, art2],
    });

    expect(result.artifactCount).toBe(2);
    expect(result.deliveredCount).toBe(1);
    expect(result.problematicCount).toBe(1);
    expect(result.primaryArtifact?.id).toBe("a1");
  });

  // Case 6: invalid-only artifacts -> no successful primaryArtifact
  it("Case 6: does not elect invalid artifacts as primaryArtifact", () => {
    const art1 = mockArtifact({ id: "a1", status: "invalid" });
    const art2 = mockArtifact({ id: "a2", status: "failed" });
    const view = mockProgressView({ runStatus: "completed" });

    const result = deriveWorkResultPresentation({
      progressView: view,
      artifacts: [art1, art2],
    });

    expect(result.artifactCount).toBe(2);
    expect(result.deliveredCount).toBe(0);
    expect(result.problematicCount).toBe(2);
    expect(result.primaryArtifact).toBeUndefined();
  });

  it("does not turn a completed session into a terminal success with an undelivered Artifact", () => {
    const view = mockProgressView({ runStatus: "completed", phase: "completed" });
    const result = deriveWorkResultPresentation({
      progressView: view,
      artifacts: [mockArtifact({ status: "validated" })],
    });

    expect(result.outcome).toBe("unknown");
    expect(result.isTerminal).toBe(false);
    expect(result.title).toBe("等待交付物验收");
    expect(result.deliveredCount).toBe(0);
  });

  // Case 7: reviewer completed -> Result model MUST NOT synthesize PASS
  it("Case 7: reports factual reviewer completion without inventing PASS verdict", () => {
    const view = mockProgressView({
      runStatus: "completed",
      agents: [
        {
          agentId: "res-1",
          childIndex: 1,
          role: "researcher",
          status: "completed",
          resultSummary: null,
          error: null,
        },
        {
          agentId: "rev-1",
          childIndex: 2,
          role: "reviewer",
          status: "completed",
          resultSummary: null,
          error: null,
        },
      ],
    });

    const result = deriveWorkResultPresentation({
      progressView: view,
      artifacts: [],
    });

    expect(result.collaborationSummary).toBe("1 位研究助手完成调查 · 独立审阅已完成");
    expect(result.collaborationSummary).not.toContain("PASS");
    expect(result.collaborationSummary).not.toContain("通过");
  });

  // Case 8: authoritative WorkRun completed + stale session running -> completed result wins
  it("Case 8: authoritative WorkRun terminal status takes precedence over session snapshot", () => {
    const view = mockProgressView({ runStatus: "completed", phase: "completed" });
    const staleSnapshot: WorkProgressSnapshot = {
      phase: "running",
      sessionPhase: "running",
      runStatus: "running",
      tasks: [],
      taskState: null,
      activeToolName: "work_run_command",
      pendingApprovalCount: 0,
      pendingAccessRoot: false,
      pendingElicitation: false,
      error: "",
      toolCallCount: 3,
    };

    const result = deriveWorkResultPresentation({
      progressView: view,
      progressSnapshot: staleSnapshot,
      artifacts: [],
    });

    expect(result.outcome).toBe("completed");
    expect(result.isTerminal).toBe(true);
  });

  // Case 9: standalone terminal session -> Session fallback works
  it("Case 9: standalone session without WorkRun uses snapshot fallback", () => {
    const standaloneSnapshot: WorkProgressSnapshot = {
      phase: "completed",
      sessionPhase: "completed",
      runStatus: "completed",
      tasks: [{ id: "t1", text: "调查", status: "completed" }],
      taskState: {
        version: 1,
        revision: 0,
        goal: "排查内存泄漏",
        plan: [{ id: "t1", text: "调查", status: "completed" }],
        checkpoint: null,
        updatedAt: "2026-08-20T20:00:00Z",
      },
      activeToolName: "",
      pendingApprovalCount: 0,
      pendingAccessRoot: false,
      pendingElicitation: false,
      error: "",
      toolCallCount: 5,
    };

    const result = deriveWorkResultPresentation({
      progressView: null,
      progressSnapshot: standaloneSnapshot,
      allowSessionFallback: true,
      artifacts: [mockArtifact({ id: "art-standalone", title: "leak-report.md" })],
    });

    expect(result.outcome).toBe("completed");
    expect(result.isTerminal).toBe(true);
    expect(result.goal).toBe("排查内存泄漏");
    expect(result.completedSteps).toBe(1);
    expect(result.artifactCount).toBe(1);
    expect(result.primaryArtifact?.title).toBe("leak-report.md");
  });

  // Case 11: workspace-backed run with missing progressView MUST NOT fall back to session terminal snapshot
  it("Case 11: workspace-backed run with missing progressView MUST NOT fall back to session terminal snapshot", () => {
    const staleCompletedSnapshot: WorkProgressSnapshot = {
      phase: "completed",
      sessionPhase: "completed",
      runStatus: "completed",
      tasks: [{ id: "t1", text: "调查", status: "completed" }],
      taskState: null,
      activeToolName: "",
      pendingApprovalCount: 0,
      pendingAccessRoot: false,
      pendingElicitation: false,
      error: "",
      toolCallCount: 5,
    };

    const result = deriveWorkResultPresentation({
      progressView: null,
      progressSnapshot: staleCompletedSnapshot,
      allowSessionFallback: false,
      artifacts: [],
    });

    expect(result.isTerminal).toBe(false);
    expect(result.outcome).toBe("unknown");
    expect(result.title).toBe("");
  });

  it("does not promote an answer-only completed WorkRun into task UI", () => {
    expect(
      hasOperationalWorkEvidence({
        progressView: mockProgressView({
          goal: "hi",
          steps: [],
          toolSummary: { proposed: 0, started: 0, completed: 0, failed: 0, running: 0 },
        }),
        artifacts: [],
      }),
    ).toBe(false);
  });

  it("does not promote planning metadata or terminal status without execution evidence", () => {
    expect(
      hasOperationalWorkEvidence({
        progressView: mockProgressView({
          goal: "整理一份报告",
          goalSpec: {
            goalId: "goal-1",
            workspaceId: "ws-1",
            runId: "run-1",
            statement: "整理一份报告",
            criteria: [
              {
                id: "criterion-1",
                description: "报告存在",
                verifierType: "file",
                status: "pending",
                evidenceRefs: [],
              },
            ],
            status: "pending",
            repairRound: 0,
            maxRepairRounds: 1,
            createdAt: "2026-08-20T20:00:00Z",
            updatedAt: "2026-08-20T20:00:00Z",
          },
          steps: [{ id: "s1", text: "读取文件", status: "pending" }],
          toolSummary: { proposed: 1, started: 0, completed: 0, failed: 0, running: 0 },
        }),
        artifacts: [],
      }),
    ).toBe(false);
    expect(
      hasOperationalWorkEvidence({
        progressView: mockProgressView({
          runStatus: "failed",
          phase: "failed",
          goal: null,
          steps: [],
          toolSummary: { proposed: 0, started: 0, completed: 0, failed: 0, running: 0 },
        }),
        artifacts: [],
      }),
    ).toBe(false);
  });

  it("promotes a WorkRun once it has execution evidence", () => {
    expect(
      hasOperationalWorkEvidence({
        progressView: mockProgressView({
          steps: [{ id: "s1", text: "读取文件", status: "completed" }],
        }),
        artifacts: [],
      }),
    ).toBe(true);
    expect(
      hasOperationalWorkEvidence({
        progressView: mockProgressView({
          steps: [],
          toolSummary: { proposed: 0, started: 1, completed: 1, failed: 0, running: 0 },
        }),
        artifacts: [],
      }),
    ).toBe(true);
  });

  it("does not treat a completed Agent turn with failed tools as task completion", () => {
    const view = mockProgressView({
      runStatus: "completed",
      phase: "completed",
      steps: [
        { id: "s1", text: "获取数据", status: "in_progress" },
        { id: "s2", text: "生成结果", status: "pending" },
      ],
      toolSummary: { proposed: 2, started: 2, completed: 0, failed: 2, running: 0 },
    });

    expect(hasWorkCompletionEvidence({ progressView: view, artifacts: [] })).toBe(false);
    const result = deriveWorkResultPresentation({ progressView: view, artifacts: [] });
    expect(result.outcome).toBe("unknown");
    expect(result.isTerminal).toBe(false);
  });

  it("requires every planned step to complete before claiming completion", () => {
    const view = mockProgressView({
      steps: [
        { id: "s1", text: "读取文件", status: "completed" },
        { id: "s2", text: "写入结果", status: "pending" },
      ],
    });

    expect(hasWorkCompletionEvidence({ progressView: view, artifacts: [] })).toBe(false);
  });

  it("does not claim completion when a tool batch contains a failure", () => {
    const view = mockProgressView({
      steps: [{ id: "s1", text: "读取文件", status: "completed" }],
      toolSummary: { proposed: 1, started: 1, completed: 0, failed: 1, running: 0 },
    });

    expect(hasWorkCompletionEvidence({ progressView: view, artifacts: [] })).toBe(false);
  });

  it("accepts delivered artifacts as completion evidence", () => {
    const view = mockProgressView({
      steps: [],
      toolSummary: { proposed: 0, started: 0, completed: 0, failed: 0, running: 0 },
    });

    expect(hasWorkCompletionEvidence({ progressView: view, artifacts: [mockArtifact()] })).toBe(
      true,
    );
  });

  // Formatting helpers
  it("formats artifact types and statuses accurately", () => {
    expect(formatArtifactType("pdf")).toBe("PDF");
    expect(formatArtifactType("docx")).toBe("Word 文档");
    expect(formatArtifactType("xlsx")).toBe("Excel 表格");
    expect(formatArtifactType("pptx")).toBe("PowerPoint");
    expect(formatArtifactType("md")).toBe("Markdown");
    expect(formatArtifactType("txt")).toBe("文本");
    expect(formatArtifactType("csv")).toBe("CSV");
    expect(formatArtifactType("png")).toBe("图片");
    expect(formatArtifactType("", "output/chart.svg")).toBe("图片");
    expect(formatArtifactType("json")).toBe("JSON");
    expect(formatArtifactType("html")).toBe("HTML");

    expect(formatArtifactStatus("creating").label).toBe("生成中");
    expect(formatArtifactStatus("ready").label).toBe("待文件检查");
    expect(formatArtifactStatus("invalid").label).toBe("文件无效");
    expect(formatArtifactStatus("failed").label).toBe("生成失败");
    expect(formatArtifactStatus("validated").label).toBe("文件检查通过");
    expect(formatArtifactStatus("delivered").label).toBe("已交付");

    expect(formatFileSize(500)).toBe("500 B");
    expect(formatFileSize(2048)).toBe("2.0 KB");
    expect(formatFileSize(2 * 1024 * 1024)).toBe("2.0 MB");
  });

  // Artifact Hub helpers
  it("formats artifact categories and receipt durations", () => {
    expect(formatArtifactCategory("document")).toBe("文档");
    expect(formatArtifactCategory("spreadsheet")).toBe("表格");
    expect(formatArtifactCategory("presentation")).toBe("演示文稿");
    expect(formatArtifactCategory("pdf")).toBe("PDF");
    expect(formatArtifactCategory("image")).toBe("图片");
    expect(formatArtifactCategory("html")).toBe("HTML");
    expect(formatArtifactCategory("code")).toBe("代码");
    expect(formatArtifactCategory("archive")).toBe("压缩包");
    expect(formatArtifactCategory("other")).toBe("其他");

    expect(formatDurationMs(null)).toBe("—");
    expect(formatDurationMs(0)).toBe("0s");
    expect(formatDurationMs(45_000)).toBe("45s");
    expect(formatDurationMs(163_000)).toBe("2m43s");
    expect(formatDurationMs(120_000)).toBe("2m");
    expect(formatDurationMs(3_600_000)).toBe("1h");
    expect(formatDurationMs(3_660_000)).toBe("1h1m");
  });

  describe("Duration derivation rules (Case 5 & Case 6)", () => {
    it("Case 5: uses reliable durationMs when available (e.g. 74000 -> 1m14s)", () => {
      const view = mockProgressView({ runStatus: "completed", phase: "completed" });
      const result = deriveWorkResultPresentation({
        progressView: view,
        artifacts: [mockArtifact()],
        durationMs: 74000,
      });

      expect(result.durationMs).toBe(74000);
      expect(result.durationFormatted).toBe("1m14s");
    });

    it("Case 5b: calculates duration from startedAt and completedAt when durationMs is not provided", () => {
      const view = mockProgressView({ runStatus: "completed", phase: "completed" });
      const result = deriveWorkResultPresentation({
        progressView: view,
        artifacts: [mockArtifact()],
        startedAt: "2026-09-13T10:00:00.000Z",
        completedAt: "2026-09-13T10:00:48.000Z",
      });

      expect(result.durationMs).toBe(48000);
      expect(result.durationFormatted).toBe("48s");
    });

    it("priority rule: reliable durationMs takes precedence over startedAt/completedAt timestamps", () => {
      const view = mockProgressView({ runStatus: "completed", phase: "completed" });
      const result = deriveWorkResultPresentation({
        progressView: view,
        artifacts: [mockArtifact()],
        durationMs: 30000,
        startedAt: "2026-09-13T10:00:00.000Z",
        completedAt: "2026-09-13T10:02:00.000Z", // 120s
      });

      expect(result.durationMs).toBe(30000);
      expect(result.durationFormatted).toBe("30s");
    });

    it("Case 6: duration is undefined when no reliable time data exists (never emits '-')", () => {
      const view = mockProgressView({ runStatus: "completed", phase: "completed" });
      const result = deriveWorkResultPresentation({
        progressView: view,
        artifacts: [mockArtifact()],
      });

      expect(result.durationMs).toBeUndefined();
      expect(result.durationFormatted).toBeUndefined();
    });

    it("invalid timestamps result in undefined duration (never negative or NaN)", () => {
      const view = mockProgressView({ runStatus: "completed", phase: "completed" });
      const result = deriveWorkResultPresentation({
        progressView: view,
        artifacts: [mockArtifact()],
        startedAt: "2026-09-13T10:05:00.000Z",
        completedAt: "2026-09-13T10:00:00.000Z", // completed before start
      });

      expect(result.durationMs).toBeUndefined();
      expect(result.durationFormatted).toBeUndefined();
    });
  });
});
