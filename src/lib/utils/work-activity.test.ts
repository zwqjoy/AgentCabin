import { describe, expect, it } from "vitest";
import { summarizeWorkActivity, workToolDetail, workToolLabel } from "$lib/utils/work-activity";

const tool = (
  tool_name: string,
  status: "running" | "success" | "error" | "permission_prompt",
  input: Record<string, unknown> = {},
) => ({ tool_name, status, input });

describe("work activity presentation", () => {
  it("translates internal subagent tool names into user-facing labels", () => {
    expect(workToolLabel("work_delegate")).toBe("委派助手");
    expect(workToolLabel("work_agent_wait")).toBe("等待助手返回");
    expect(workToolLabel("unknown_work_tool")).toBe("处理能力");
  });

  it("keeps useful delegation context without exposing the tool protocol", () => {
    expect(
      workToolDetail({
        tool_name: "work_delegate",
        input: { role: "researcher", task: "整理本周竞品变化和证据" },
      }),
    ).toBe("研究助手 · 整理本周竞品变化和证据");
  });

  it("surfaces a user confirmation wait as the primary current state", () => {
    const summary = summarizeWorkActivity(
      [tool("work_delegate", "success"), tool("work_agent_wait", "permission_prompt")],
      "",
      true,
    );

    expect(summary.currentLabel).toBe("等待你确认");
    expect(summary.statusLabel).toBe("需要你确认");
    expect(summary.status).toBe("waiting");
    expect(summary.completedCount).toBe(1);
    expect(summary.totalCount).toBe(2);
  });

  it("summarizes thinking without making raw thinking the primary status", () => {
    const summary = summarizeWorkActivity([], "正在检查工作区并整理方案", true);

    expect(summary.currentLabel).toBe("正在分析任务方案");
    expect(summary.statusLabel).toBe("分析中");
    expect(summary.status).toBe("thinking");
  });

  it("reports completed actions with one consistent count", () => {
    const summary = summarizeWorkActivity(
      [tool("work_read_file", "success"), tool("work_write_file", "success")],
      "",
      false,
    );

    expect(summary.currentLabel).toBe("本轮处理已完成");
    expect(summary.statusLabel).toBe("已完成 2/2");
    expect(summary.status).toBe("completed");
  });
});
