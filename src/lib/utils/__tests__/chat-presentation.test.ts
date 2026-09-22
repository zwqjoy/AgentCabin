import { describe, it, expect } from "vitest";
import {
  buildChatPresentationTurns,
  formatActivityGroupSummary,
  extractLatestThinkingLine,
  extractLatestNarrationLine,
} from "$lib/utils/chat-presentation";
import type { TimelineEntry } from "$lib/types";

describe("chat-presentation", () => {
  it("interleaves reasoning and activity groups in chronological order (matches Codex Case 3)", () => {
    const timeline: TimelineEntry[] = [
      {
        kind: "user",
        id: "u1",
        anchorId: "u1",
        content: "检查项目为什么 build 失败",
        ts: "2026-09-13T09:00:00Z",
      },
      // Intermediate assistant with thinking
      {
        kind: "assistant",
        id: "a1",
        anchorId: "a1",
        content: "",
        thinkingText: "我先检查相关代码，确认事件是如何进入 timeline 的。",
        ts: "2026-09-13T09:00:02Z",
      },
      // Tool 1 & 2
      {
        kind: "tool",
        id: "t1",
        anchorId: "t1",
        tool: {
          tool_use_id: "t1",
          tool_name: "Read",
          input: { file_path: "package.json" },
          status: "success",
        },
        ts: "2026-09-13T09:00:04Z",
      },
      {
        kind: "tool",
        id: "t2",
        anchorId: "t2",
        tool: {
          tool_use_id: "t2",
          tool_name: "Read",
          input: { file_path: "vite.config.ts" },
          status: "success",
        },
        ts: "2026-09-13T09:00:05Z",
      },
      // Interleaved reasoning 2
      {
        kind: "assistant",
        id: "a2",
        anchorId: "a2",
        content: "",
        thinkingText: "当前配置已读取，我接下来执行构建命令复现错误。",
        ts: "2026-09-13T09:00:06Z",
      },
      // Tool 3
      {
        kind: "tool",
        id: "t3",
        anchorId: "t3",
        tool: {
          tool_use_id: "t3",
          tool_name: "Bash",
          input: { command: "pnpm build" },
          status: "success",
        },
        ts: "2026-09-13T09:00:08Z",
      },
      // Final assistant response
      {
        kind: "assistant",
        id: "a3",
        anchorId: "a3",
        content: "构建失败的原因是缺少依赖。",
        ts: "2026-09-13T09:00:10Z",
      },
    ];

    const turns = buildChatPresentationTurns(timeline, undefined, { u1: true }, undefined, "zh-CN");
    expect(turns).toHaveLength(1);
    const turn = turns[0];

    // User message
    expect(turn.userMessage?.content).toBe("检查项目为什么 build 失败");

    // Interleaved process blocks:
    // [0] Reasoning: 我先检查相关代码...
    // [1] ActivityGroup: package.json, vite.config.ts (2 files read)
    // [2] Reasoning: 当前配置已读取...
    // [3] ActivityGroup: pnpm build (1 command)
    expect(turn.processBlocks).toHaveLength(4);

    expect(turn.processBlocks[0].type).toBe("reasoning");
    if (turn.processBlocks[0].type === "reasoning") {
      expect(turn.processBlocks[0].content).toContain("我先检查相关代码");
    }

    expect(turn.processBlocks[1].type).toBe("activity-group");
    if (turn.processBlocks[1].type === "activity-group") {
      expect(turn.processBlocks[1].activities).toHaveLength(2);
      expect(turn.processBlocks[1].summaryLabel).toBe("读取了 2 个文件");
    }

    expect(turn.processBlocks[2].type).toBe("reasoning");
    if (turn.processBlocks[2].type === "reasoning") {
      expect(turn.processBlocks[2].content).toContain("当前配置已读取");
    }

    expect(turn.processBlocks[3].type).toBe("activity-group");
    if (turn.processBlocks[3].type === "activity-group") {
      expect(turn.processBlocks[3].activities).toHaveLength(1);
      expect(turn.processBlocks[3].summaryLabel).toBe("运行了 1 个命令");
    }

    // Final answer is cleanly separated
    expect(turn.finalMessage).toBeDefined();
    expect(turn.finalMessage?.content).toBe("构建失败的原因是缺少依赖。");

    // Semantic summary of the whole turn
    expect(turn.turnSemanticSummary).toBe("读取了 2 个文件并运行了 1 个命令");
  });

  it("extracts interaction tools into interactionBlocks and keeps turn uncollapsed", () => {
    const timeline: TimelineEntry[] = [
      {
        kind: "user",
        id: "u1",
        anchorId: "u1",
        content: "清理缓存",
        ts: "2026-09-13T09:00:00Z",
      },
      {
        kind: "tool",
        id: "t1",
        anchorId: "t1",
        tool: {
          tool_use_id: "t1",
          tool_name: "Bash",
          input: { command: "rm -rf .cache" },
          status: "permission_prompt",
        },
        ts: "2026-09-13T09:00:02Z",
      },
    ];

    const turns = buildChatPresentationTurns(timeline);
    expect(turns).toHaveLength(1);
    const turn = turns[0];

    // Interaction block exists and activity groups do not hide it
    expect(turn.interactionBlocks).toHaveLength(1);
    expect(turn.interactionBlocks[0].type).toBe("permission");
    expect(turn.isCollapsed).toBe(false); // must not collapse when permission needed
  });

  it("handles live streaming reasoning and final message", () => {
    const timeline: TimelineEntry[] = [
      {
        kind: "user",
        id: "u1",
        anchorId: "u1",
        content: "写一个快速排序",
        ts: "2026-09-13T09:00:00Z",
      },
    ];

    const liveState = {
      isRunning: true,
      thinkingText: "思考中：选取基准元素，双指针划分区间...",
      streamingText: "下面是快速排序的 TypeScript 实现：",
      durationMs: 4000,
      streamingDisposition: "final" as const,
    };

    const turns = buildChatPresentationTurns(timeline, liveState);
    expect(turns).toHaveLength(1);
    const turn = turns[0];

    expect(turn.isRunning).toBe(true);
    expect(turn.isCollapsed).toBe(false); // live execution is expanded

    // Streaming reasoning block
    expect(turn.processBlocks).toHaveLength(1);
    expect(turn.processBlocks[0].type).toBe("reasoning");
    if (turn.processBlocks[0].type === "reasoning") {
      expect(turn.processBlocks[0].content).toContain("选取基准元素");
      expect(turn.processBlocks[0].isStreaming).toBe(true);
    }

    // Streaming final message
    expect(turn.finalMessage?.isStreaming).toBe(true);
    expect(turn.finalMessage?.content).toBe("下面是快速排序的 TypeScript 实现：");
  });

  it("formats English activity group summaries correctly", () => {
    const summary = formatActivityGroupSummary(
      [
        {
          id: "1",
          tool: {} as any,
          category: "read",
          state: "success",
          iconKind: "book",
          verb: "Read",
          target: "a.ts",
          detail: {},
        },
        {
          id: "2",
          tool: {} as any,
          category: "read",
          state: "success",
          iconKind: "book",
          verb: "Read",
          target: "b.ts",
          detail: {},
        },
        {
          id: "3",
          tool: {} as any,
          category: "command",
          state: "success",
          iconKind: "terminal",
          verb: "Ran",
          target: "pnpm test",
          detail: {},
        },
      ],
      true,
    );

    expect(summary.label).toBe("Read 2 files and ran 1 command");
    expect(summary.iconKind).toBe("terminal");
  });

  it("extracts clean latest thinking line (DSH latestLine)", () => {
    const rawThinking = `
我需要先思考下这个项目的结构。
首先查看 package.json 中的依赖项。
- 检查是否存在 **typescript** 依赖冲突
**检查完毕，正在分析 tsconfig 配置...**
`;
    const line = extractLatestThinkingLine(rawThinking);
    expect(line).toBe("检查完毕，正在分析 tsconfig 配置...");
  });

  it("populates activeThinkingPreview during running live thinking", () => {
    const timeline: TimelineEntry[] = [
      {
        kind: "user",
        id: "u1",
        anchorId: "u1",
        content: "请帮我重构",
        ts: "2026-09-13T09:00:00Z",
      },
    ];

    const turns = buildChatPresentationTurns(
      timeline,
      {
        isRunning: true,
        thinkingText: "我正在检查架构设计...\n**确认方案无误，开始生成代码...**",
      },
      {},
      new Map(),
      "zh-CN",
    );

    expect(turns.length).toBe(1);
    expect(turns[0].isRunning).toBe(true);
    expect(turns[0].activeThinkingPreview).toBe("确认方案无误，开始生成代码...");
  });

  it("inserts interactive tool blocks in-stream chronologically inside processBlocks", () => {
    const timeline: TimelineEntry[] = [
      {
        kind: "user",
        id: "u1",
        anchorId: "u1",
        content: "重命名文件",
        ts: "2026-09-13T09:00:00Z",
      },
      {
        kind: "assistant",
        id: "a1",
        anchorId: "a1",
        content: "",
        thinkingText: "需要用户确认",
        ts: "2026-09-13T09:00:01Z",
      },
      {
        kind: "tool",
        id: "t1",
        anchorId: "t1",
        tool: {
          tool_use_id: "t1",
          tool_name: "AskUserQuestion",
          input: { question: "确认重命名吗？" },
          status: "ask_pending",
        },
        ts: "2026-09-13T09:00:02Z",
      },
    ];

    const turns = buildChatPresentationTurns(timeline);
    expect(turns).toHaveLength(1);
    const turn = turns[0];
    expect(turn.hasPendingInteraction).toBe(true);
    expect(turn.processBlocks).toHaveLength(2);
    expect(turn.processBlocks[0].type).toBe("reasoning");
    expect(turn.processBlocks[1].type).toBe("interaction");
    if (turn.processBlocks[1].type === "interaction") {
      expect(turn.processBlocks[1].item.tool.tool_name).toBe("AskUserQuestion");
      expect(turn.processBlocks[1].isPending).toBe(true);
    }
  });

  it("strictly honors collapse precedence: pending > user explicit toggle > running default > completed default", () => {
    const makeTimeline = (hasPending: boolean): TimelineEntry[] => [
      {
        kind: "user",
        id: "u1",
        anchorId: "u1",
        content: "执行任务",
        ts: "2026-09-13T09:00:00Z",
      },
      {
        kind: "tool",
        id: "t1",
        anchorId: "t1",
        tool: {
          tool_use_id: "t1",
          tool_name: hasPending ? "AskUserQuestion" : "Bash",
          input: hasPending ? { question: "继续吗？" } : { command: "ls" },
          status: hasPending ? "ask_pending" : "success",
        },
        ts: "2026-09-13T09:00:02Z",
      },
    ];

    // Case 1: Running without pending interaction defaults to expanded (isCollapsed: false)
    const runningTurn = buildChatPresentationTurns(makeTimeline(false), { isRunning: true })[0];
    expect(runningTurn.isCollapsed).toBe(false);

    // Case 2: User explicitly collapsed while running -> MUST honor user choice!
    const userCollapsedTurn = buildChatPresentationTurns(
      makeTimeline(false),
      { isRunning: true },
      { u1: false },
    )[0];
    expect(userCollapsedTurn.isCollapsed).toBe(true);

    // Case 3: Has pending interaction -> overrides user explicit toggle to keep visible!
    const pendingTurn = buildChatPresentationTurns(
      makeTimeline(true),
      { isRunning: true },
      { u1: false },
    )[0];
    expect(pendingTurn.isCollapsed).toBe(false);

    // Case 4: Completed with no pending interaction defaults to collapsed
    const completedTurn = buildChatPresentationTurns(makeTimeline(false))[0];
    expect(completedTurn.isCollapsed).toBe(true);
  });

  it("computes sane duration for the pseudo-turn before the first user message (no epoch offset)", () => {
    // Session resume / greeting entries that exist before any user prompt must not
    // measure duration against the Unix epoch (which would render e.g. "用时 497025h").
    const timeline: TimelineEntry[] = [
      {
        kind: "tool",
        id: "t1",
        anchorId: "t1",
        tool: {
          tool_use_id: "t1",
          tool_name: "Read",
          input: { file_path: "README.md" },
          status: "success",
        },
        ts: "2026-09-13T09:00:00Z",
      },
      {
        kind: "assistant",
        id: "a1",
        anchorId: "a1",
        content: "欢迎回来，已恢复上次会话。",
        ts: "2026-09-13T09:00:05Z",
      },
    ];

    const turns = buildChatPresentationTurns(timeline, undefined, undefined, undefined, "zh-CN");
    expect(turns).toHaveLength(1);
    expect(turns[0].userMessage).toBeUndefined();
    // Duration spans the pseudo-turn's own entries (5s), not the epoch.
    expect(turns[0].durationMs).toBe(5000);
    expect(turns[0].durationFormatted).toBe("用时 5s");
  });

  it("falls back to a minimal duration when timestamps are invalid (no NaN leakage)", () => {
    const timeline: TimelineEntry[] = [
      {
        kind: "user",
        id: "u1",
        anchorId: "u1",
        content: "你好",
        ts: "not-a-date",
      },
      {
        kind: "assistant",
        id: "a1",
        anchorId: "a1",
        content: "你好！",
        ts: "also-not-a-date",
      },
    ];

    const turns = buildChatPresentationTurns(timeline);
    expect(turns).toHaveLength(1);
    expect(Number.isNaN(turns[0].durationMs)).toBe(false);
    expect(turns[0].durationMs).toBe(1000);
    expect(turns[0].durationFormatted).not.toContain("NaN");
  });

  it("activeToolPreview reflects the latest running activity, not the first", () => {
    // Parallel (or stale-status) tools can both report "running"; the pill should
    // surface the most recent action the agent is performing.
    const timeline: TimelineEntry[] = [
      {
        kind: "user",
        id: "u1",
        anchorId: "u1",
        content: "并行执行",
        ts: "2026-09-13T09:00:00Z",
      },
      {
        kind: "tool",
        id: "t1",
        anchorId: "t1",
        tool: {
          tool_use_id: "t1",
          tool_name: "Bash",
          input: { command: "pnpm build" },
          status: "running",
        },
        ts: "2026-09-13T09:00:02Z",
      },
      {
        kind: "tool",
        id: "t2",
        anchorId: "t2",
        tool: {
          tool_use_id: "t2",
          tool_name: "Read",
          input: { file_path: "src/app.ts" },
          status: "running",
        },
        ts: "2026-09-13T09:00:03Z",
      },
    ];

    const turns = buildChatPresentationTurns(
      timeline,
      { isRunning: true },
      undefined,
      undefined,
      "zh-CN",
    );
    expect(turns).toHaveLength(1);
    expect(turns[0].activeToolPreview).toBeDefined();
    expect(turns[0].activeToolPreview).toContain("app.ts");
    expect(turns[0].activeToolPreview).not.toContain("pnpm build");
  });

  it("suppresses cross-session git diff pollution when turn has no mutation tools", () => {
    const timeline: TimelineEntry[] = [
      {
        kind: "user",
        id: "u1",
        anchorId: "u1",
        content: "请帮我查找 context_window 相关代码",
        ts: "2026-09-13T09:00:00Z",
      },
      {
        kind: "tool",
        id: "t1",
        anchorId: "t1",
        tool: {
          tool_use_id: "t1",
          tool_name: "Read",
          input: { file_path: "src/capabilities.rs" },
          status: "success",
        },
        ts: "2026-09-13T09:00:02Z",
      },
      {
        kind: "turn_summary",
        id: "ts1",
        anchorId: "ts1",
        cwd: "/app",
        diff: "diff --git a/WorkChatSurface.svelte b/WorkChatSurface.svelte\n--- a/WorkChatSurface.svelte\n+++ b/WorkChatSurface.svelte\n@@ -1 +1 @@\n-old\n+new",
        ts: "2026-09-13T09:00:05Z",
      },
      {
        kind: "assistant",
        id: "a1",
        anchorId: "a1",
        content: "已查到代码位于 capabilities.rs",
        ts: "2026-09-13T09:00:06Z",
      },
    ];

    const turns = buildChatPresentationTurns(timeline);
    expect(turns[0].turnSummary).toBeUndefined();
  });

  it("retains turnSummary when diff matches the files mutated by tools in the turn", () => {
    const timeline: TimelineEntry[] = [
      {
        kind: "user",
        id: "u1",
        anchorId: "u1",
        content: "修改 capabilities.rs",
        ts: "2026-09-13T09:00:00Z",
      },
      {
        kind: "tool",
        id: "t1",
        anchorId: "t1",
        tool: {
          tool_use_id: "t1",
          tool_name: "Edit",
          input: { file_path: "src/capabilities.rs" },
          status: "success",
        },
        ts: "2026-09-13T09:00:02Z",
      },
      {
        kind: "turn_summary",
        id: "ts1",
        anchorId: "ts1",
        cwd: "/app",
        diff: "diff --git a/src/capabilities.rs b/src/capabilities.rs\n--- a/src/capabilities.rs\n+++ b/src/capabilities.rs\n@@ -1 +1 @@\n-old\n+new",
        ts: "2026-09-13T09:00:05Z",
      },
      {
        kind: "assistant",
        id: "a1",
        anchorId: "a1",
        content: "已修改 capabilities.rs",
        ts: "2026-09-13T09:00:06Z",
      },
    ];

    const turns = buildChatPresentationTurns(timeline);
    expect(turns[0].turnSummary).toBeDefined();
    expect(turns[0].turnSummary?.diff).toContain("capabilities.rs");
  });

  it("recognizes artifact tools and formats artifact registration summary correctly", () => {
    const timeline: TimelineEntry[] = [
      {
        kind: "user",
        id: "u1",
        anchorId: "u1",
        content: "生成最终报告",
        ts: "2026-09-13T09:00:00Z",
      },
      {
        kind: "tool",
        id: "t1",
        anchorId: "t1",
        tool: {
          tool_use_id: "t1",
          tool_name: "work_register_artifact",
          input: { title: "系统架构调研报告.md", path: "/artifacts/report.md" },
          status: "success",
        },
        ts: "2026-09-13T09:00:02Z",
      },
    ];

    const turns = buildChatPresentationTurns(timeline, undefined, undefined, undefined, "zh-CN");
    expect(turns).toHaveLength(1);
    const turn = turns[0];
    const group = turn.processBlocks.find((b) => b.type === "activity-group");
    expect(group).toBeDefined();
    if (group && group.type === "activity-group") {
      expect(group.iconKind).toBe("box");
      expect(group.summaryLabel).toBe("登记了 1 个成果");
      expect(group.activities[0].category).toBe("artifact");
      expect(group.activities[0].target).toBe("系统架构调研报告.md");
    }

    // English locale test
    const turnsEn = buildChatPresentationTurns(timeline, undefined, undefined, undefined, "en-US");
    const groupEn = turnsEn[0].processBlocks.find((b) => b.type === "activity-group");
    if (groupEn && groupEn.type === "activity-group") {
      expect(groupEn.summaryLabel).toBe("Registered 1 artifact");
    }
  });

  it("normalizes work_delegate subagent tool activity properly", () => {
    const timeline: TimelineEntry[] = [
      {
        kind: "user",
        id: "u1",
        anchorId: "u1",
        content: "派发调研任务",
        ts: "2026-09-13T09:00:00Z",
      },
      {
        kind: "tool",
        id: "t1",
        anchorId: "t1",
        tool: {
          tool_use_id: "t1",
          tool_name: "work_delegate",
          input: { role: "researcher", task: "分析前端架构性能瓶颈" },
          status: "running",
        },
        ts: "2026-09-13T09:00:02Z",
      },
    ];

    const turns = buildChatPresentationTurns(timeline, { isRunning: true });
    expect(turns).toHaveLength(1);
    const turn = turns[0];
    const group = turn.processBlocks.find((b) => b.type === "activity-group");
    expect(group).toBeDefined();
    if (group && group.type === "activity-group") {
      expect(group.iconKind).toBe("bot");
      expect(group.hasRunning).toBe(true);
      expect(group.activities[0].category).toBe("agent");
      expect(group.activities[0].detail.prompt).toBe("分析前端架构性能瓶颈");
    }
  });

  it("correctly identifies latest activity group vs historical completed groups during live stream", () => {
    const timeline: TimelineEntry[] = [
      {
        kind: "user",
        id: "u1",
        anchorId: "u1",
        content: "多阶段任务",
        ts: "2026-09-13T09:00:00Z",
      },
      // Completed Group 1: read file
      {
        kind: "tool",
        id: "t1",
        anchorId: "t1",
        tool: {
          tool_use_id: "t1",
          tool_name: "Read",
          input: { file_path: "config.json" },
          status: "success",
        },
        ts: "2026-09-13T09:00:02Z",
      },
      // Interleaving reasoning
      {
        kind: "assistant",
        id: "a1",
        anchorId: "a1",
        content: "",
        thinkingText: "配置已读取完毕，现在开始执行测试",
        ts: "2026-09-13T09:00:03Z",
      },
      // Running Group 2: bash command
      {
        kind: "tool",
        id: "t2",
        anchorId: "t2",
        tool: {
          tool_use_id: "t2",
          tool_name: "Bash",
          input: { command: "pnpm test" },
          status: "running",
        },
        ts: "2026-09-13T09:00:05Z",
      },
    ];

    const turns = buildChatPresentationTurns(timeline, { isRunning: true });
    expect(turns).toHaveLength(1);
    const turn = turns[0];

    const groups = turn.processBlocks.filter(
      (b): b is Extract<typeof b, { type: "activity-group" }> => b.type === "activity-group",
    );
    expect(groups).toHaveLength(2);

    const [group1, group2] = groups;
    // Historical group 1: completed, not running
    expect(group1.hasRunning).toBe(false);

    // Active group 2: running
    expect(group2.hasRunning).toBe(true);

    // The latest activity group ID in turn.processBlocks is group 2
    let latestGroupId = null;
    for (let i = turn.processBlocks.length - 1; i >= 0; i--) {
      if (turn.processBlocks[i].type === "activity-group") {
        latestGroupId = turn.processBlocks[i].id;
        break;
      }
    }
    expect(latestGroupId).toBe(group2.id);

    // Simulation of ChatProcessStream's shouldDefaultExpand rule:
    // shouldDefaultExpand = block.hasRunning || (turn.isRunning && isLatestGroup)
    const shouldDefaultExpand1 =
      group1.hasRunning || (turn.isRunning && group1.id === latestGroupId);
    const shouldDefaultExpand2 =
      group2.hasRunning || (turn.isRunning && group2.id === latestGroupId);

    // Group 1 should NOT default expand (historical auto-collapses)
    expect(shouldDefaultExpand1).toBe(false);
    // Group 2 SHOULD default expand (active running)
    expect(shouldDefaultExpand2).toBe(true);
  });

  describe("Running / Final lifecycle semantics and collapse isolation", () => {
    // Case 1: running turn ends with narration
    it("Case 1: running turn ending with narration keeps narration in processBlocks and finalMessage is undefined", () => {
      const timeline: TimelineEntry[] = [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: "开始修复问题",
          ts: "2026-09-13T09:00:00Z",
        },
        {
          kind: "assistant",
          id: "a1",
          anchorId: "a1",
          content: "修复 3：ChatProcessStream —— onAnswer 带上 item id",
          ts: "2026-09-13T09:00:01Z",
        },
        {
          kind: "tool",
          id: "t1",
          anchorId: "t1",
          tool: {
            tool_use_id: "t1",
            tool_name: "Read",
            input: { file_path: "src/app.ts" },
            status: "success",
          },
          ts: "2026-09-13T09:00:02Z",
        },
        {
          kind: "assistant",
          id: "a2",
          anchorId: "a2",
          content: "Now Fix 4: Code page ... Let me double-check ...",
          ts: "2026-09-13T09:00:03Z",
        },
      ];

      const turns = buildChatPresentationTurns(timeline, { isRunning: true });
      expect(turns).toHaveLength(1);
      const turn = turns[0];

      // All intermediate narrations and activities are in processBlocks
      expect(turn.processBlocks).toHaveLength(3);
      expect(turn.processBlocks[0]).toMatchObject({
        type: "narration",
        content: "修复 3：ChatProcessStream —— onAnswer 带上 item id",
      });
      expect(turn.processBlocks[1]).toMatchObject({
        type: "activity-group",
      });
      expect(turn.processBlocks[2]).toMatchObject({
        type: "narration",
        content: "Now Fix 4: Code page ... Let me double-check ...",
      });

      // Crucial: not falsely elevated to finalMessage during running
      expect(turn.finalMessage).toBeUndefined();
    });

    // Case 2: running + streaming final answer
    it("Case 2: running turn with streamingText surfaces streaming finalMessage while keeping historical entries in processBlocks", () => {
      const timeline: TimelineEntry[] = [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: "生成修复方案",
          ts: "2026-09-13T09:00:00Z",
        },
        {
          kind: "assistant",
          id: "a1",
          anchorId: "a1",
          content: "正在分析问题...",
          ts: "2026-09-13T09:00:01Z",
        },
        {
          kind: "tool",
          id: "t1",
          anchorId: "t1",
          tool: {
            tool_use_id: "t1",
            tool_name: "Bash",
            input: { command: "git status" },
            status: "success",
          },
          ts: "2026-09-13T09:00:02Z",
        },
      ];

      const turns = buildChatPresentationTurns(timeline, {
        isRunning: true,
        streamingText: "正在生成最终总结...",
        streamingDisposition: "final",
      });
      expect(turns).toHaveLength(1);
      const turn = turns[0];

      expect(turn.processBlocks).toHaveLength(2);
      expect(turn.processBlocks[0]).toMatchObject({
        type: "narration",
        content: "正在分析问题...",
      });
      expect(turn.processBlocks[1]).toMatchObject({
        type: "activity-group",
      });

      expect(turn.finalMessage).toBeDefined();
      expect(turn.finalMessage?.content).toBe("正在生成最终总结...");
      expect(turn.finalMessage?.isStreaming).toBe(true);
    });

    // Case 3: completed turn
    it("Case 3: completed turn correctly promotes the last assistant answer to finalMessage", () => {
      const timeline: TimelineEntry[] = [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: "开始修复问题",
          ts: "2026-09-13T09:00:00Z",
        },
        {
          kind: "assistant",
          id: "a1",
          anchorId: "a1",
          content: "修复 3：ChatProcessStream —— onAnswer 带上 item id",
          ts: "2026-09-13T09:00:01Z",
        },
        {
          kind: "tool",
          id: "t1",
          anchorId: "t1",
          tool: {
            tool_use_id: "t1",
            tool_name: "Read",
            input: { file_path: "src/app.ts" },
            status: "success",
          },
          ts: "2026-09-13T09:00:02Z",
        },
        {
          kind: "assistant",
          id: "a2",
          anchorId: "a2",
          content: "所有问题修复完成，测试全部通过。",
          ts: "2026-09-13T09:00:03Z",
        },
      ];

      const turns = buildChatPresentationTurns(timeline, { isRunning: false });
      expect(turns).toHaveLength(1);
      const turn = turns[0];

      // a1 and t1 remain in processBlocks
      expect(turn.processBlocks).toHaveLength(2);
      expect(turn.processBlocks[0]).toMatchObject({
        type: "narration",
        content: "修复 3：ChatProcessStream —— onAnswer 带上 item id",
      });
      expect(turn.processBlocks[1]).toMatchObject({
        type: "activity-group",
      });

      // a2 is correctly extracted as finalMessage
      expect(turn.finalMessage).toBeDefined();
      expect(turn.finalMessage?.content).toBe("所有问题修复完成，测试全部通过。");
      expect(turn.finalMessage?.isStreaming).toBe(false);
    });

    // Case 4: user explicitly collapsed running turn
    it("Case 4: explicit collapse on running turn is preserved across intermediate updates and only overridden by pending interactions", () => {
      const baseTimeline: TimelineEntry[] = [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: "长任务开始",
          ts: "2026-09-13T09:00:00Z",
        },
        {
          kind: "assistant",
          id: "a1",
          anchorId: "a1",
          content: "第一阶段：分析中...",
          ts: "2026-09-13T09:00:01Z",
        },
      ];

      // User manually collapsed turn u1
      const expandedTurns: Record<string, boolean> = { u1: false };

      const turn1 = buildChatPresentationTurns(baseTimeline, { isRunning: true }, expandedTurns)[0];
      expect(turn1.isCollapsed).toBe(true);

      // Subsequent update 1: add tool
      const timelineWithTool: TimelineEntry[] = [
        ...baseTimeline,
        {
          kind: "tool",
          id: "t1",
          anchorId: "t1",
          tool: {
            tool_use_id: "t1",
            tool_name: "Bash",
            input: { command: "cargo build" },
            status: "running",
          },
          ts: "2026-09-13T09:00:02Z",
        },
      ];
      const turn2 = buildChatPresentationTurns(
        timelineWithTool,
        { isRunning: true },
        expandedTurns,
      )[0];
      expect(turn2.isCollapsed).toBe(true);

      // Subsequent update 2: add new narration
      const timelineWithNarration: TimelineEntry[] = [
        ...timelineWithTool,
        {
          kind: "assistant",
          id: "a2",
          anchorId: "a2",
          content: "第二阶段：检查测试用例...",
          ts: "2026-09-13T09:00:03Z",
        },
      ];
      const turn3 = buildChatPresentationTurns(
        timelineWithNarration,
        { isRunning: true },
        expandedTurns,
      )[0];
      expect(turn3.isCollapsed).toBe(true);

      // Subsequent update 3: add live reasoning
      const turn4 = buildChatPresentationTurns(
        timelineWithNarration,
        { isRunning: true, thinkingText: "正在权衡重构范围..." },
        expandedTurns,
      )[0];
      expect(turn4.isCollapsed).toBe(true);

      // Only when a pending interaction arrives does it force expansion
      const timelineWithPending: TimelineEntry[] = [
        ...timelineWithNarration,
        {
          kind: "tool",
          id: "t2",
          anchorId: "t2",
          tool: {
            tool_use_id: "t2",
            tool_name: "AskUserQuestion",
            input: { question: "是否允许修改配置文件？" },
            status: "ask_pending",
          },
          ts: "2026-09-13T09:00:04Z",
        },
      ];
      const turn5 = buildChatPresentationTurns(
        timelineWithPending,
        { isRunning: true },
        expandedTurns,
      )[0];
      expect(turn5.hasPendingInteraction).toBe(true);
      expect(turn5.isCollapsed).toBe(false);
    });

    // Case 5: running narration does not penetrate collapse
    it("Case 5: running intermediate narration never leaks outside ChatProcessStream when turn is collapsed", () => {
      const timeline: TimelineEntry[] = [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: "继续重构",
          ts: "2026-09-13T09:00:00Z",
        },
        {
          kind: "tool",
          id: "t1",
          anchorId: "t1",
          tool: {
            tool_use_id: "t1",
            tool_name: "Read",
            input: { file_path: "src/lib.rs" },
            status: "success",
          },
          ts: "2026-09-13T09:00:02Z",
        },
        {
          kind: "assistant",
          id: "a1",
          anchorId: "a1",
          content: "Now Fix 4: Code page ... Let me double-check ...",
          ts: "2026-09-13T09:00:05Z",
        },
      ];

      const expandedTurns = { u1: false };
      const turn = buildChatPresentationTurns(timeline, { isRunning: true }, expandedTurns)[0];

      // Turn is collapsed
      expect(turn.isCollapsed).toBe(true);
      // finalMessage is undefined - so nothing renders outside ChatProcessStream!
      expect(turn.finalMessage).toBeUndefined();
      // Narration is strictly inside processBlocks
      const narrations = turn.processBlocks.filter((b) => b.type === "narration");
      expect(narrations).toHaveLength(1);
      expect(
        (narrations[0] as Extract<(typeof narrations)[0], { type: "narration" }>).content,
      ).toBe("Now Fix 4: Code page ... Let me double-check ...");
    });

    // Chronological order verification: Narration -> Tool Activities -> Subsequent Narration -> Subsequent Tool Activities
    it("maintains strict chronological sequence: Narration precedes its corresponding Activity Group", () => {
      const timeline: TimelineEntry[] = [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: "请探索代码库并运行测试",
          ts: "2026-09-13T09:00:00Z",
        },
        // Phase 1 Narration
        {
          kind: "assistant",
          id: "a1",
          anchorId: "a1",
          content: "Let me start by exploring the codebase...",
          ts: "2026-09-13T09:00:01Z",
        },
        // Phase 1 Tools: 4 reads + 2 commands
        {
          kind: "tool",
          id: "t1",
          anchorId: "t1",
          tool: {
            tool_use_id: "t1",
            tool_name: "Read",
            input: { file_path: "1" },
            status: "success",
          },
          ts: "2026-09-13T09:00:02Z",
        },
        {
          kind: "tool",
          id: "t2",
          anchorId: "t2",
          tool: {
            tool_use_id: "t2",
            tool_name: "Read",
            input: { file_path: "2" },
            status: "success",
          },
          ts: "2026-09-13T09:00:03Z",
        },
        {
          kind: "tool",
          id: "t3",
          anchorId: "t3",
          tool: {
            tool_use_id: "t3",
            tool_name: "Read",
            input: { file_path: "3" },
            status: "success",
          },
          ts: "2026-09-13T09:00:04Z",
        },
        {
          kind: "tool",
          id: "t4",
          anchorId: "t4",
          tool: {
            tool_use_id: "t4",
            tool_name: "Read",
            input: { file_path: "4" },
            status: "success",
          },
          ts: "2026-09-13T09:00:05Z",
        },
        {
          kind: "tool",
          id: "t5",
          anchorId: "t5",
          tool: {
            tool_use_id: "t5",
            tool_name: "Bash",
            input: { command: "ls" },
            status: "success",
          },
          ts: "2026-09-13T09:00:06Z",
        },
        {
          kind: "tool",
          id: "t6",
          anchorId: "t6",
          tool: {
            tool_use_id: "t6",
            tool_name: "Bash",
            input: { command: "pwd" },
            status: "success",
          },
          ts: "2026-09-13T09:00:07Z",
        },
        // Phase 2 Narration
        {
          kind: "assistant",
          id: "a2",
          anchorId: "a2",
          content: "Now let's look at the existing tests...",
          ts: "2026-09-13T09:00:08Z",
        },
        // Phase 2 Tools: 6 commands
        ...Array.from({ length: 6 }, (_, i) => ({
          kind: "tool" as const,
          id: `cmd-${i}`,
          anchorId: `cmd-${i}`,
          tool: {
            tool_use_id: `cmd-${i}`,
            tool_name: "Bash",
            input: { command: `test-${i}` },
            status: "success" as const,
          },
          ts: `2026-09-13T09:00:1${i}Z`,
        })),
      ];

      const turns = buildChatPresentationTurns(
        timeline,
        { isRunning: true },
        undefined,
        undefined,
        "zh-CN",
      );
      expect(turns).toHaveLength(1);
      const turn = turns[0];

      // processBlocks order must strictly be:
      // 0: narration 1
      // 1: activity-group 1 (读取了 4 个文件并运行了 2 个命令)
      // 2: narration 2
      // 3: activity-group 2 (运行了 6 个命令)
      expect(turn.processBlocks).toHaveLength(4);
      expect(turn.processBlocks[0]).toMatchObject({
        type: "narration",
        content: "Let me start by exploring the codebase...",
      });
      expect(turn.processBlocks[1]).toMatchObject({
        type: "activity-group",
        summaryLabel: "读取了 4 个文件并运行了 2 个命令",
      });
      expect(turn.processBlocks[2]).toMatchObject({
        type: "narration",
        content: "Now let's look at the existing tests...",
      });
      expect(turn.processBlocks[3]).toMatchObject({
        type: "activity-group",
        summaryLabel: "运行了 6 个命令",
      });

      // And finalMessage remains undefined while running
      expect(turn.finalMessage).toBeUndefined();
    });
  });

  describe("Running Turn Live Streaming vs Provisional Narration Lifecycle", () => {
    it("extractLatestNarrationLine cleans markdown and returns the latest meaningful line", () => {
      expect(extractLatestNarrationLine("")).toBe("");
      expect(
        extractLatestNarrationLine("Let me verify my suspected bugs with a quick scratch test."),
      ).toBe("Let me verify my suspected bugs with a quick scratch test.");
      expect(
        extractLatestNarrationLine("### Fix 3: ChatProcessStream\n\n**Now Fix 4: Code page...**\n"),
      ).toBe("Now Fix 4: Code page...");
      expect(extractLatestNarrationLine("- Step 1: explore\n- Step 2: run bash command")).toBe(
        "Step 2: run bash command",
      );
    });

    // Test 1: Provisional narration during running turn
    it("Test 1 (provisional narration): live streaming assistant text during a running turn is strictly provisional narration inside processBlocks, NOT finalMessage", () => {
      const timeline: TimelineEntry[] = [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: "Why is the test failing?",
          ts: "2026-09-13T10:00:00Z",
        },
      ];

      const turns = buildChatPresentationTurns(timeline, {
        isRunning: true,
        streamingText: "Let me verify my suspected bugs with a quick scratch test.",
      });

      expect(turns).toHaveLength(1);
      const turn = turns[0];
      expect(turn.isRunning).toBe(true);

      // Final message must NOT be populated with provisional narration
      expect(turn.finalMessage).toBeUndefined();

      // Provisional narration is strictly inside processBlocks
      expect(turn.processBlocks).toHaveLength(1);
      expect(turn.processBlocks[0]).toMatchObject({
        type: "narration",
        content: "Let me verify my suspected bugs with a quick scratch test.",
        isStreaming: true,
      });

      // Active narration preview is exposed for the process pill
      expect(turn.activeNarrationPreview).toBe(
        "Let me verify my suspected bugs with a quick scratch test.",
      );
    });

    // Test 2: Tool follow-up seamlessly transitions within processBlocks
    it("Test 2 (tool follow-up): when a tool call follows provisional narration, narration stays in processBlocks seamlessly without jumping", () => {
      // Step A: provisional streaming narration while running
      const timelineStepA: TimelineEntry[] = [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: "Check logs",
          ts: "2026-09-13T10:00:00Z",
        },
      ];
      const turnsStepA = buildChatPresentationTurns(timelineStepA, {
        isRunning: true,
        streamingText: "Let me check the logs with cat...",
      });
      expect(turnsStepA[0].finalMessage).toBeUndefined();
      expect(turnsStepA[0].processBlocks[0]).toMatchObject({
        type: "narration",
        content: "Let me check the logs with cat...",
        isStreaming: true,
      });

      // Step B: model emits tool call, message completes, tool runs
      const timelineStepB: TimelineEntry[] = [
        ...timelineStepA,
        {
          kind: "assistant",
          id: "a1",
          anchorId: "a1",
          content: "Let me check the logs with cat...",
          ts: "2026-09-13T10:00:01Z",
        },
        {
          kind: "tool",
          id: "t1",
          anchorId: "t1",
          tool: {
            tool_use_id: "t1",
            tool_name: "Bash",
            input: { command: "cat server.log" },
            status: "running",
          },
          ts: "2026-09-13T10:00:02Z",
        },
      ];
      const turnsStepB = buildChatPresentationTurns(timelineStepB, {
        isRunning: true,
        streamingText: "",
      });

      // Narration was already in processBlocks in Step A, and is still in processBlocks in Step B!
      expect(turnsStepB[0].finalMessage).toBeUndefined();
      expect(turnsStepB[0].processBlocks).toHaveLength(2);
      expect(turnsStepB[0].processBlocks[0]).toMatchObject({
        type: "narration",
        content: "Let me check the logs with cat...",
        isStreaming: false,
      });
      expect(turnsStepB[0].processBlocks[1]).toMatchObject({
        type: "activity-group",
        hasRunning: true,
      });
      // Active tool preview takes precedence when a tool is running
      expect(turnsStepB[0].activeToolPreview).toContain("cat server.log");
    });

    // Test 3: Multi-stage narration + tool execution
    it("Test 3 (multi-stage narration + tool): multiple stages of narration and tool execution all stay ordered in processBlocks", () => {
      const timeline: TimelineEntry[] = [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: "Fix all issues",
          ts: "2026-09-13T10:00:00Z",
        },
        {
          kind: "assistant",
          id: "a1",
          anchorId: "a1",
          content: "Stage 1: inspecting codebase",
          ts: "2026-09-13T10:00:01Z",
        },
        {
          kind: "tool",
          id: "t1",
          anchorId: "t1",
          tool: {
            tool_use_id: "t1",
            tool_name: "ReadFile",
            input: { path: "package.json" },
            status: "success",
          },
          ts: "2026-09-13T10:00:02Z",
        },
        {
          kind: "assistant",
          id: "a2",
          anchorId: "a2",
          content: "Stage 2: running test suite",
          ts: "2026-09-13T10:00:03Z",
        },
        {
          kind: "tool",
          id: "t2",
          anchorId: "t2",
          tool: {
            tool_use_id: "t2",
            tool_name: "Bash",
            input: { command: "vitest run" },
            status: "success",
          },
          ts: "2026-09-13T10:00:04Z",
        },
      ];

      // Agent is now live streaming Stage 3 narration
      const turns = buildChatPresentationTurns(timeline, {
        isRunning: true,
        streamingText: "Stage 3: applying final patch to presentation engine...",
      });

      expect(turns).toHaveLength(1);
      const turn = turns[0];
      expect(turn.finalMessage).toBeUndefined();

      // All 5 process blocks in strictly chronological order:
      // narration 1 -> group 1 -> narration 2 -> group 2 -> live streaming narration 3
      expect(turn.processBlocks).toHaveLength(5);
      expect(turn.processBlocks[0]).toMatchObject({
        type: "narration",
        content: "Stage 1: inspecting codebase",
        isStreaming: false,
      });
      expect(turn.processBlocks[1]).toMatchObject({
        type: "activity-group",
      });
      expect(turn.processBlocks[2]).toMatchObject({
        type: "narration",
        content: "Stage 2: running test suite",
        isStreaming: false,
      });
      expect(turn.processBlocks[3]).toMatchObject({
        type: "activity-group",
      });
      expect(turn.processBlocks[4]).toMatchObject({
        type: "narration",
        content: "Stage 3: applying final patch to presentation engine...",
        isStreaming: true,
      });

      expect(turn.activeNarrationPreview).toBe(
        "Stage 3: applying final patch to presentation engine...",
      );
    });

    // Test 4: Genuine final answer transition
    it("Test 4 (genuine final answer): when turn completes (or streamingDisposition === 'final'), finalMessage is populated", () => {
      const timeline: TimelineEntry[] = [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: "Explain result",
          ts: "2026-09-13T10:00:00Z",
        },
        {
          kind: "assistant",
          id: "a1",
          anchorId: "a1",
          content: "Let me check something first...",
          ts: "2026-09-13T10:00:01Z",
        },
        {
          kind: "tool",
          id: "t1",
          anchorId: "t1",
          tool: {
            tool_use_id: "t1",
            tool_name: "Bash",
            input: { command: "echo test" },
            status: "success",
          },
          ts: "2026-09-13T10:00:02Z",
        },
        {
          kind: "assistant",
          id: "a2",
          anchorId: "a2",
          content: "Here is the final verified explanation and solution.",
          ts: "2026-09-13T10:00:03Z",
        },
      ];

      // Mode A: isRunning = false (Turn completed)
      const completedTurns = buildChatPresentationTurns(timeline, {
        isRunning: false,
      });
      expect(completedTurns[0].finalMessage).toBeDefined();
      expect(completedTurns[0].finalMessage?.content).toBe(
        "Here is the final verified explanation and solution.",
      );
      expect(completedTurns[0].finalMessage?.isStreaming).toBe(false);
      // Intermediate narration a1 stays in processBlocks, while a2 is promoted to finalMessage
      expect(completedTurns[0].processBlocks).toHaveLength(2);
      expect(completedTurns[0].processBlocks[0]).toMatchObject({
        type: "narration",
        content: "Let me check something first...",
      });
      expect(completedTurns[0].processBlocks[1]).toMatchObject({
        type: "activity-group",
      });

      // Mode B: isRunning = true but runtime explicitly signals streamingDisposition: "final"
      const streamingFinalTurns = buildChatPresentationTurns([timeline[0]], {
        isRunning: true,
        streamingText: "Streaming the guaranteed final summary...",
        streamingDisposition: "final",
      });
      expect(streamingFinalTurns[0].finalMessage).toBeDefined();
      expect(streamingFinalTurns[0].finalMessage?.content).toBe(
        "Streaming the guaranteed final summary...",
      );
      expect(streamingFinalTurns[0].finalMessage?.isStreaming).toBe(true);
      expect(streamingFinalTurns[0].processBlocks).toHaveLength(0);
    });

    // Test 5: User collapse isolation
    it("Test 5 (user collapse isolation): when user manually collapses running turn, streaming narration remains contained inside processBlocks, does not leak outside, and updates activeNarrationPreview", () => {
      const timeline: TimelineEntry[] = [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: "Heavy refactor task",
          ts: "2026-09-13T10:00:00Z",
        },
        {
          kind: "assistant",
          id: "a1",
          anchorId: "a1",
          content: "Fix 3: ChatProcessStream —— onAnswer 带上 item id",
          ts: "2026-09-13T10:00:01Z",
        },
        {
          kind: "tool",
          id: "t1",
          anchorId: "t1",
          tool: {
            tool_use_id: "t1",
            tool_name: "Bash",
            input: { command: "git status" },
            status: "success",
          },
          ts: "2026-09-13T10:00:02Z",
        },
      ];

      // User collapsed turn u1 explicitly
      const expandedTurns = { u1: false };
      const turns = buildChatPresentationTurns(
        timeline,
        {
          isRunning: true,
          streamingText: "Now Fix 4: Code page Presentation Engine parity check",
        },
        expandedTurns,
      );

      expect(turns).toHaveLength(1);
      const turn = turns[0];

      // Turn is collapsed
      expect(turn.isCollapsed).toBe(true);

      // finalMessage MUST BE undefined so nothing renders outside ChatProcessStream
      expect(turn.finalMessage).toBeUndefined();

      // Streaming text is contained in processBlocks (which are hidden by isCollapsed)
      expect(turn.processBlocks).toHaveLength(3);
      expect(turn.processBlocks[2]).toMatchObject({
        type: "narration",
        content: "Now Fix 4: Code page Presentation Engine parity check",
        isStreaming: true,
      });

      // The top process pill preview exposes the active narration line for the collapsed header
      expect(turn.activeNarrationPreview).toBe(
        "Now Fix 4: Code page Presentation Engine parity check",
      );
    });

    // Test 6: Code/Work parity
    it("Test 6 (Code/Work parity): presentation engine produces identical structure regardless of Code or Work consumer invocation", () => {
      const timeline: TimelineEntry[] = [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: "Parity test",
          ts: "2026-09-13T10:00:00Z",
        },
        {
          kind: "assistant",
          id: "a1",
          anchorId: "a1",
          content: "Investigating workspace files",
          ts: "2026-09-13T10:00:01Z",
        },
      ];

      const liveState = {
        isRunning: true,
        thinkingText: "Analyzing differences...",
        streamingText: "Comparing Code vs Work surfaces...",
      };

      // Code invocation style
      const codeTurns = buildChatPresentationTurns(timeline, liveState, { u1: true }, undefined);

      // Work invocation style (with custom interaction checker)
      const workTurns = buildChatPresentationTurns(
        timeline,
        liveState,
        { u1: true },
        undefined,
        undefined,
        () => false,
      );

      expect(codeTurns[0].isRunning).toBe(workTurns[0].isRunning);
      expect(codeTurns[0].isCollapsed).toBe(workTurns[0].isCollapsed);
      expect(codeTurns[0].finalMessage).toBeUndefined();
      expect(workTurns[0].finalMessage).toBeUndefined();
      expect(codeTurns[0].processBlocks).toEqual(workTurns[0].processBlocks);
      expect(codeTurns[0].activeThinkingPreview).toBe(workTurns[0].activeThinkingPreview);
      expect(codeTurns[0].activeNarrationPreview).toBe(workTurns[0].activeNarrationPreview);
    });

    // Test 7: Provider neutrality
    it("Test 7 (provider neutrality): Pi, Claude, and Codex execution models all behave consistently with zero provider branching", () => {
      const createTimelineForProvider = (providerName: string): TimelineEntry[] => [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: `Test with provider ${providerName}`,
          ts: "2026-09-13T10:00:00Z",
        },
        {
          kind: "assistant",
          id: "a1",
          anchorId: "a1",
          model: providerName === "pi" ? "claude-3-7-sonnet" : providerName,
          content: `Executing provider-neutral tool step`,
          ts: "2026-09-13T10:00:01Z",
        },
        {
          kind: "tool",
          id: "t1",
          anchorId: "t1",
          tool: {
            tool_use_id: "t1",
            tool_name: "Bash",
            input: { command: "pwd" },
            status: "success",
          },
          ts: "2026-09-13T10:00:02Z",
        },
      ];

      for (const provider of ["pi", "claude", "codex"]) {
        const turns = buildChatPresentationTurns(createTimelineForProvider(provider), {
          isRunning: true,
          streamingText: `Currently running under ${provider}...`,
        });

        expect(turns[0].isRunning).toBe(true);
        expect(turns[0].finalMessage).toBeUndefined();
        expect(turns[0].processBlocks).toHaveLength(3);
        expect(turns[0].processBlocks[2]).toMatchObject({
          type: "narration",
          content: `Currently running under ${provider}...`,
          isStreaming: true,
        });
        expect(turns[0].activeNarrationPreview).toBe(`Currently running under ${provider}...`);
      }
    });

    // Test 8: Multi-line streaming narration on a manually collapsed running turn
    it("Test 8 (collapsed running turn, multi-line streaming narration): streamingText stays in processBlocks as streaming narration and preview shows the latest line", () => {
      const timeline: TimelineEntry[] = [
        {
          kind: "user",
          id: "u1",
          anchorId: "u1",
          content: "Run final checks",
          ts: "2026-09-13T10:00:00Z",
        },
      ];

      const turns = buildChatPresentationTurns(
        timeline,
        {
          isRunning: true,
          streamingText: "Checking current state...\nNow running the final verification...",
        },
        { u1: false },
      );

      expect(turns).toHaveLength(1);
      const turn = turns[0];

      expect(turn.isRunning).toBe(true);
      // User explicitly collapsed the running turn
      expect(turn.isCollapsed).toBe(true);
      // Nothing leaks outside ChatProcessStream while running
      expect(turn.finalMessage).toBeUndefined();

      // Multi-line streamingText is strictly contained as a streaming narration block
      const narrations = turn.processBlocks.filter((b) => b.type === "narration");
      expect(narrations).toHaveLength(1);
      expect(narrations[0]).toMatchObject({
        type: "narration",
        content: "Checking current state...\nNow running the final verification...",
        isStreaming: true,
      });

      // Collapsed header preview shows the latest narration line
      expect(turn.activeNarrationPreview).toBe("Now running the final verification...");
    });
  });
});
