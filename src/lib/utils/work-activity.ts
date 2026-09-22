import type { BusToolItem } from "$lib/types";

export type WorkActivityTool = Pick<BusToolItem, "tool_name" | "status" | "input">;

export type WorkActivityStatus =
  | "thinking"
  | "running"
  | "waiting"
  | "error"
  | "completed"
  | "idle";

export interface WorkActivitySummary {
  currentLabel: string;
  statusLabel: string;
  status: WorkActivityStatus;
  completedCount: number;
  failedCount: number;
  totalCount: number;
}

const WORK_TOOL_LABELS: Record<string, string> = {
  web_search: "搜索网页",
  web_open: "打开页面",
  web_extract: "摘取证据",
  web_cite: "生成引用",
  browser_navigate: "打开网页",
  browser_snapshot: "读取页面",
  browser_take_screenshot: "截取页面",
  browser_wait_for: "等待页面",
  browser_tabs: "管理标签页",
  browser_close: "关闭浏览器",
  browser_click: "点击页面元素",
  browser_type: "填写页面内容",
  browser_select_option: "选择页面选项",
  browser_scroll: "滚动页面",
  work_run_command: "运行命令",
  work_execute: "执行能力",
  work_read_file: "读取文件",
  work_list_files: "列出文件",
  work_write_file: "写入文件",
  work_edit_file: "编辑文件",
  work_set_goal: "设定目标",
  work_replace_plan: "制定计划",
  work_update_step: "更新步骤",
  work_save_checkpoint: "保存检查点",
  work_register_artifact: "注册成果",
  work_list_artifacts: "查看成果",
  work_validate_artifact: "验证成果",
  work_deliver: "交付成果",
  work_request_directory_access: "请求目录访问",
  work_list_tools: "查看工具",
  work_discover_capabilities: "发现能力",
  work_activate_tools: "启用工具",
  work_workspace_info: "查看工作区",
  work_delegate: "委派助手",
  work_agent_wait: "等待助手返回",
  work_agent_status: "查看助手状态",
  work_agent_steer: "指导助手",
  work_agent_stop: "停止助手",
};

const SUBAGENT_ROLES: Record<string, string> = {
  researcher: "研究助手",
  worker: "执行助手",
  reviewer: "审阅助手",
};

export function workToolLabel(name: string): string {
  return WORK_TOOL_LABELS[name] ?? (name === "mcp" ? "MCP 外部工具" : "处理能力");
}

export function workToolProgressLabel(name: string): string {
  if (name === "work_agent_wait") return "等待助手返回";
  if (name === "work_agent_status") return "查看助手状态";
  return `正在${workToolLabel(name)}`;
}

function truncate(value: string, maxLength: number): string {
  const text = value.trim();
  if (text.length <= maxLength) return text;
  return `${text.slice(0, maxLength - 1)}…`;
}

export function workToolDetail(tool: Pick<BusToolItem, "tool_name" | "input">): string {
  const input = (tool.input ?? {}) as Record<string, unknown>;
  const name = tool.tool_name;

  if (name === "work_delegate") {
    const role =
      SUBAGENT_ROLES[
        String(input.role ?? "")
          .trim()
          .toLowerCase()
      ] ?? "助手";
    const task = truncate(String(input.task ?? ""), 72);
    return task ? `${role} · ${task}` : role;
  }
  if (name === "web_search") {
    return truncate(String(input.query ?? input.q ?? ""), 80);
  }
  if (
    name === "web_open" ||
    name === "web_extract" ||
    name === "web_cite" ||
    name === "browser_navigate"
  ) {
    const url = String(input.url ?? "").trim();
    if (!url) return "";
    try {
      return new URL(url).hostname;
    } catch {
      return truncate(url, 80);
    }
  }
  if (name === "work_run_command" || name === "work_execute") {
    return truncate(String(input.command ?? input.cmd ?? ""), 120);
  }
  if (name === "work_read_file" || name === "work_write_file" || name === "work_edit_file") {
    const path = String(input.path ?? input.target ?? "");
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] || path;
  }
  if (name === "work_replace_plan" || name === "work_update_step") {
    const items = Array.isArray(input.steps)
      ? input.steps
      : Array.isArray(input.plan)
        ? input.plan
        : [];
    return items.length > 0 ? `${items.length} 项` : "";
  }
  if (
    name === "work_register_artifact" ||
    name === "work_validate_artifact" ||
    name === "work_deliver"
  ) {
    return truncate(String(input.title ?? input.name ?? ""), 60);
  }
  if (name === "work_request_directory_access") {
    return truncate(String(input.path ?? input.target ?? ""), 80);
  }
  return "";
}

function isWaiting(status: string): boolean {
  return status === "permission_prompt" || status === "ask_pending";
}

function isFailed(status: string): boolean {
  return status === "error" || status === "permission_denied" || status === "denied";
}

export function summarizeWorkActivity(
  entries: WorkActivityTool[],
  thinkingText: string,
  running: boolean,
): WorkActivitySummary {
  const totalCount = entries.length;
  const completedCount = entries.filter((entry) => entry.status === "success").length;
  const failedCount = entries.filter((entry) => isFailed(entry.status)).length;
  const activeEntry = [...entries]
    .reverse()
    .find((entry) => entry.status === "running" || isWaiting(entry.status));

  if (activeEntry) {
    const waiting = isWaiting(activeEntry.status);
    return {
      currentLabel: waiting ? "等待你确认" : workToolProgressLabel(activeEntry.tool_name),
      statusLabel: waiting
        ? "需要你确认"
        : totalCount > 0
          ? `已完成 ${completedCount}/${totalCount}`
          : "进行中",
      status: waiting ? "waiting" : "running",
      completedCount,
      failedCount,
      totalCount,
    };
  }

  if (failedCount > 0) {
    return {
      currentLabel: `有 ${failedCount} 项执行失败`,
      statusLabel: `${failedCount} 项失败`,
      status: "error",
      completedCount,
      failedCount,
      totalCount,
    };
  }

  if (running && thinkingText.trim()) {
    return {
      currentLabel: "正在分析任务方案",
      statusLabel: totalCount > 0 ? `已完成 ${completedCount}/${totalCount}` : "分析中",
      status: "thinking",
      completedCount,
      failedCount,
      totalCount,
    };
  }

  if (running) {
    return {
      currentLabel: "正在处理任务",
      statusLabel: totalCount > 0 ? `已完成 ${completedCount}/${totalCount}` : "进行中",
      status: "running",
      completedCount,
      failedCount,
      totalCount,
    };
  }

  if (totalCount > 0) {
    return {
      currentLabel: "本轮处理已完成",
      statusLabel: `已完成 ${completedCount}/${totalCount}`,
      status: "completed",
      completedCount,
      failedCount,
      totalCount,
    };
  }

  return {
    currentLabel: thinkingText.trim() ? "模型过程已记录" : "准备开始",
    statusLabel: "",
    status: thinkingText.trim() ? "thinking" : "idle",
    completedCount,
    failedCount,
    totalCount,
  };
}
