import type { WorkRunStatus, WorkRunTrigger, WorkTask } from "$lib/types/work";

export interface AutomationQuickStats {
  total: number;
  scheduled: number;
  active: number;
  inRun: number;
  needsAttention: number;
}

export function computeAutomationQuickStats(tasks: WorkTask[]): AutomationQuickStats {
  let scheduled = 0;
  let inRun = 0;
  let needsAttention = 0;
  let active = 0;

  for (const t of tasks) {
    if (t.status === "archived") continue;
    if (t.schedule?.enabled) scheduled++;
    if (t.status === "in_run") inRun++;
    if (t.status === "needs_attention") needsAttention++;
    if (t.status === "active") active++;
  }

  return {
    total: tasks.filter((t) => t.status !== "archived").length,
    scheduled,
    active,
    inRun,
    needsAttention,
  };
}

export function formatAutomationTrigger(trigger: WorkRunTrigger): {
  label: string;
  badgeClass: string;
} {
  switch (trigger) {
    case "scheduled":
      return {
        label: "定时触发",
        badgeClass: "bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20",
      };
    case "event_triggered":
      return {
        label: "事件触发",
        badgeClass: "bg-purple-500/10 text-purple-600 dark:text-purple-400 border-purple-500/20",
      };
    case "manual":
    default:
      return {
        label: "手动触发",
        badgeClass: "bg-muted text-muted-foreground border-border",
      };
  }
}

export function formatAutomationRunStatus(status: WorkRunStatus): {
  label: string;
  badgeClass: string;
  dotClass: string;
} {
  switch (status) {
    case "completed":
      return {
        label: "已完成",
        badgeClass:
          "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20",
        dotClass: "bg-emerald-500",
      };
    case "failed":
      return {
        label: "执行失败",
        badgeClass: "bg-red-500/10 text-red-600 dark:text-red-400 border-red-500/20",
        dotClass: "bg-red-500",
      };
    case "running":
      return {
        label: "运行中",
        badgeClass: "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20",
        dotClass: "bg-amber-500 animate-pulse",
      };
    case "waiting_approval":
    case "waiting_input":
      return {
        label: "需人工确认",
        badgeClass: "bg-orange-500/10 text-orange-600 dark:text-orange-400 border-orange-500/20",
        dotClass: "bg-orange-500 animate-pulse",
      };
    case "waiting_delivery":
      return {
        label: "等待交付物",
        badgeClass: "bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20",
        dotClass: "bg-blue-500",
      };
    case "skipped":
      return {
        label: "已跳过",
        badgeClass: "bg-muted text-muted-foreground border-border",
        dotClass: "bg-muted-foreground",
      };
    case "cancelled":
      return {
        label: "已取消",
        badgeClass: "bg-muted text-muted-foreground border-border",
        dotClass: "bg-muted-foreground",
      };
    case "recoverable":
      return {
        label: "可恢复",
        badgeClass: "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20",
        dotClass: "bg-amber-500",
      };
    case "queued":
    default:
      return {
        label: "排队中",
        badgeClass: "bg-muted text-muted-foreground border-border",
        dotClass: "bg-muted-foreground",
      };
  }
}

export function formatRunDuration(ms?: number | null): string {
  if (!ms || ms <= 0) return "-";
  const seconds = Math.floor(ms / 1000);
  if (seconds < 60) return `${seconds}秒`;
  const minutes = Math.floor(seconds / 60);
  const remainingSecs = seconds % 60;
  if (minutes < 60) return `${minutes}分${remainingSecs}秒`;
  const hours = Math.floor(minutes / 60);
  return `${hours}小时${minutes % 60}分`;
}
