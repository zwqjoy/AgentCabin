import { describe, it, expect } from "vitest";
import { normalizeToolActivity, isInteractionTool } from "$lib/utils/tool-activity-adapter";
import type { BusToolItem } from "$lib/types";

describe("tool-activity-adapter", () => {
  it("correctly identifies interaction tools", () => {
    const regular: BusToolItem = {
      tool_use_id: "t1",
      tool_name: "Bash",
      input: { command: "ls" },
      status: "running",
    };
    expect(isInteractionTool(regular)).toBe(false);

    const permissionTool: BusToolItem = {
      tool_use_id: "t2",
      tool_name: "Bash",
      input: { command: "rm -rf /" },
      status: "permission_prompt",
    };
    expect(isInteractionTool(permissionTool)).toBe(true);

    const askTool: BusToolItem = {
      tool_use_id: "t3",
      tool_name: "AskUserQuestion",
      input: { question: "Choose database" },
      status: "running",
    };
    expect(isInteractionTool(askTool)).toBe(true);

    const exitPlanTool: BusToolItem = {
      tool_use_id: "t4",
      tool_name: "ExitPlanMode",
      input: {},
      status: "success",
    };
    expect(isInteractionTool(exitPlanTool)).toBe(true);
  });

  describe("command normalization and dynamic state", () => {
    it("handles running command state", () => {
      const tool: BusToolItem = {
        tool_use_id: "c1",
        tool_name: "Bash",
        input: { command: "pnpm check" },
        status: "running",
      };
      const act = normalizeToolActivity(tool, { locale: "zh-CN" });
      expect(act.category).toBe("command");
      expect(act.state).toBe("running");
      expect(act.verb).toBe("正在运行");
      expect(act.target).toBe("pnpm check");
      expect(act.iconKind).toBe("terminal");
    });

    it("handles success command state without '已' flood", () => {
      const tool: BusToolItem = {
        tool_use_id: "c1",
        tool_name: "Bash",
        input: { command: "pnpm check" },
        output: { content: "All checks passed\nexit status 0" },
        status: "success",
      };
      const act = normalizeToolActivity(tool, { locale: "zh-CN" });
      expect(act.state).toBe("success");
      expect(act.verb).toBe("运行");
      expect(act.target).toBe("pnpm check");
      expect(act.detail.isError).toBe(false);
    });

    it("handles failed command state with exit code", () => {
      const tool: BusToolItem = {
        tool_use_id: "c2",
        tool_name: "Bash",
        input: { command: "pnpm test" },
        output: { content: "FAIL src/main.test.ts\nexited with code 1" },
        status: "error",
      };
      const act = normalizeToolActivity(tool, { locale: "zh-CN" });
      expect(act.state).toBe("failed");
      expect(act.verb).toBe("运行失败");
      expect(act.detail.exitCode).toBe(1);
      expect(act.detail.isError).toBe(true);
    });

    it("supports English locale for commands", () => {
      const toolRun: BusToolItem = {
        tool_use_id: "c1",
        tool_name: "Bash",
        input: { command: "cargo build" },
        status: "running",
      };
      expect(normalizeToolActivity(toolRun, { locale: "en" }).verb).toBe("Running");

      const toolSuccess: BusToolItem = {
        tool_use_id: "c1",
        tool_name: "Bash",
        input: { command: "cargo build" },
        status: "success",
      };
      expect(normalizeToolActivity(toolSuccess, { locale: "en" }).verb).toBe("Ran");

      const toolFail: BusToolItem = {
        tool_use_id: "c1",
        tool_name: "Bash",
        input: { command: "cargo build" },
        status: "error",
      };
      expect(normalizeToolActivity(toolFail, { locale: "en" }).verb).toBe("Failed to run");
    });
  });

  describe("file read & edit normalization", () => {
    it("handles file read", () => {
      const tool: BusToolItem = {
        tool_use_id: "r1",
        tool_name: "Read",
        input: { file_path: "src/routes/chat/+page.svelte" },
        status: "success",
      };
      const act = normalizeToolActivity(tool, { locale: "zh-CN" });
      expect(act.category).toBe("read");
      expect(act.verb).toBe("读取");
      expect(act.target).toBe("+page.svelte");
      expect(act.detail.filePath).toBe("src/routes/chat/+page.svelte");
      expect(act.iconKind).toBe("book");
    });

    it("handles file edit and generates diff", () => {
      const tool: BusToolItem = {
        tool_use_id: "e1",
        tool_name: "Edit",
        input: {
          file_path: "src/config.ts",
          old_string: "export const port = 3000;",
          new_string: "export const port = 8080;",
        },
        status: "success",
      };
      const act = normalizeToolActivity(tool, { locale: "zh-CN" });
      expect(act.category).toBe("edit");
      expect(act.verb).toBe("编辑");
      expect(act.target).toBe("config.ts");
      expect(act.iconKind).toBe("pencil");
      expect(act.detail.diff).toContain("- export const port = 3000;");
      expect(act.detail.diff).toContain("+ export const port = 8080;");
    });
  });

  describe("search normalization", () => {
    it("extracts pattern and search scope", () => {
      const tool: BusToolItem = {
        tool_use_id: "s1",
        tool_name: "grep_search",
        input: {
          Query: "TimelineEntry",
          SearchPath: "/home/user/app/src",
        },
        output: {
          content:
            "src/types.ts:1882:export type TimelineEntry\nsrc/store.ts:300:entry: TimelineEntry",
        },
        status: "success",
      };
      const act = normalizeToolActivity(tool, { locale: "zh-CN" });
      expect(act.category).toBe("search");
      expect(act.verb).toBe("搜索");
      expect(act.target).toBe('"TimelineEntry" in src');
      expect(act.iconKind).toBe("search");
      expect(act.detail.matchesCount).toBeGreaterThanOrEqual(2);
    });
  });

  describe("subagent delegation", () => {
    it("handles subagent tool normalization", () => {
      const tool: BusToolItem = {
        tool_use_id: "a1",
        tool_name: "Task",
        input: {
          subagent_type: "Explore",
          prompt: "Investigate database connection issues",
        },
        status: "running",
      };
      const act = normalizeToolActivity(tool, { locale: "zh-CN" });
      expect(act.category).toBe("agent");
      expect(act.verb).toBe("子代理正在执行");
      expect(act.target).toBe("Investigate database connection issues");
      expect(act.iconKind).toBe("bot");
    });
  });
});
