import type { BrowserActionType, BrowserSessionStatus, BrowserTraceEntry } from "$lib/types/work";

export interface BrowserActionMeta {
  label: string;
  icon: string;
  badgeClass: string;
}

/** Return whether a runtime tool name belongs to the Work browser surface. */
export function isBrowserToolName(toolName: string | null | undefined): boolean {
  const normalized = toolName?.trim().toLowerCase() ?? "";
  return normalized.includes("browser") || normalized.includes("web_");
}

export function formatBrowserAction(action: BrowserActionType): BrowserActionMeta {
  switch (action) {
    case "navigate":
      return {
        label: "页面导航",
        icon: "🌐",
        badgeClass: "bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20",
      };
    case "snapshot":
      return {
        label: "读取页面快照",
        icon: "👁️",
        badgeClass: "bg-purple-500/10 text-purple-600 dark:text-purple-400 border-purple-500/20",
      };
    case "screenshot":
      return {
        label: "页面截屏",
        icon: "📸",
        badgeClass:
          "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20",
      };
    case "click":
      return {
        label: "点击元素",
        icon: "👆",
        badgeClass: "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20",
      };
    case "type":
      return {
        label: "文本输入",
        icon: "⌨️",
        badgeClass: "bg-cyan-500/10 text-cyan-600 dark:text-cyan-400 border-cyan-500/20",
      };
    case "select_option":
      return {
        label: "下拉选择",
        icon: "🔽",
        badgeClass: "bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 border-indigo-500/20",
      };
    case "scroll":
      return {
        label: "页面滚动",
        icon: "📜",
        badgeClass: "bg-slate-500/10 text-slate-600 dark:text-slate-400 border-slate-500/20",
      };
    case "wait_for":
      return {
        label: "等待延时",
        icon: "⏳",
        badgeClass: "bg-yellow-500/10 text-yellow-600 dark:text-yellow-400 border-yellow-500/20",
      };
    case "tabs":
      return {
        label: "标签页管理",
        icon: "📑",
        badgeClass: "bg-teal-500/10 text-teal-600 dark:text-teal-400 border-teal-500/20",
      };
    case "takeover":
      return {
        label: "用户人工接管",
        icon: "🧑‍💻",
        badgeClass: "bg-rose-500/10 text-rose-600 dark:text-rose-400 border-rose-500/20",
      };
    case "close":
      return {
        label: "关闭浏览器",
        icon: "🚪",
        badgeClass: "bg-muted text-muted-foreground border-border",
      };
    default:
      return {
        label: "浏览器操作",
        icon: "⚡",
        badgeClass: "bg-muted text-muted-foreground border-border",
      };
  }
}

export function formatBrowserStatus(status: BrowserSessionStatus): {
  label: string;
  colorClass: string;
  dotClass: string;
} {
  switch (status) {
    case "idle":
      return {
        label: "待命",
        colorClass: "text-muted-foreground bg-muted/50 border-border",
        dotClass: "bg-muted-foreground",
      };
    case "running":
      return {
        label: "执行中",
        colorClass: "text-blue-600 dark:text-blue-400 bg-blue-500/10 border-blue-500/20",
        dotClass: "bg-blue-500 animate-pulse",
      };
    case "waiting_approval":
      return {
        label: "等待审批",
        colorClass: "text-amber-600 dark:text-amber-400 bg-amber-500/10 border-amber-500/20",
        dotClass: "bg-amber-500 animate-ping",
      };
    case "paused":
      return {
        label: "已暂停",
        colorClass: "text-yellow-600 dark:text-yellow-400 bg-yellow-500/10 border-yellow-500/20",
        dotClass: "bg-yellow-500",
      };
    case "taking_over":
      return {
        label: "人工接管中",
        colorClass: "text-rose-600 dark:text-rose-400 bg-rose-500/10 border-rose-500/20",
        dotClass: "bg-rose-500 animate-pulse",
      };
    case "completed":
      return {
        label: "已完成",
        colorClass:
          "text-emerald-600 dark:text-emerald-400 bg-emerald-500/10 border-emerald-500/20",
        dotClass: "bg-emerald-500",
      };
    case "failed":
      return {
        label: "执行失败",
        colorClass: "text-red-600 dark:text-red-400 bg-red-500/10 border-red-500/20",
        dotClass: "bg-red-500",
      };
    case "closed":
      return {
        label: "已关闭",
        colorClass: "text-muted-foreground bg-muted border-border",
        dotClass: "bg-muted-foreground/60",
      };
  }
}

/**
 * 敏感信息脱敏：避免密码、Bearer token、API Key 等明文记录与展示。
 */
export function sanitizeTraceText(text: string): string {
  if (!text) return "";
  return text
    .replace(/(bearer\s+)[a-zA-Z0-9_.-]{12,}/gi, "$1[REDACTED]")
    .replace(/(api[_-]?key["':\s=]+)[a-zA-Z0-9_.-]{12,}/gi, "$1[REDACTED]")
    .replace(/(password["':\s=]+["']?)[^\s"']+(["']?)/gi, "$1******$2")
    .replace(/(secret["':\s=]+["']?)[^\s"']+(["']?)/gi, "$1[REDACTED]$2");
}

export function filterBrowserTraces(
  traces: BrowserTraceEntry[],
  query: string,
): BrowserTraceEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return traces;
  return traces.filter(
    (t) =>
      t.description.toLowerCase().includes(q) ||
      (t.targetUrl && t.targetUrl.toLowerCase().includes(q)) ||
      (t.selector && t.selector.toLowerCase().includes(q)) ||
      (t.error && t.error.toLowerCase().includes(q)),
  );
}
