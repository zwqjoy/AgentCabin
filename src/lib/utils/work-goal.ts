import type {
  AcceptanceCriterion,
  CriterionStatus,
  GoalStatus,
  VerifierType,
} from "$lib/types/work";

export interface GoalProgressSummary {
  total: number;
  passed: number;
  failed: number;
  insufficient: number;
  pending: number;
  percent: number;
}

export function computeGoalProgress(criteria: AcceptanceCriterion[]): GoalProgressSummary {
  const total = criteria.length;
  if (total === 0) {
    return { total: 0, passed: 0, failed: 0, insufficient: 0, pending: 0, percent: 100 };
  }

  let passed = 0;
  let failed = 0;
  let insufficient = 0;
  let pending = 0;

  for (const c of criteria) {
    switch (c.status) {
      case "passed":
        passed++;
        break;
      case "failed":
        failed++;
        break;
      case "insufficient_evidence":
        insufficient++;
        break;
      case "checking":
      case "pending":
      default:
        pending++;
        break;
    }
  }

  const percent = Math.round((passed / total) * 100);
  return { total, passed, failed, insufficient, pending, percent };
}

export function formatGoalStatus(status: GoalStatus): {
  label: string;
  badgeClass: string;
  dotClass: string;
} {
  switch (status) {
    case "passed":
      return {
        label: "目标达成",
        badgeClass:
          "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20",
        dotClass: "bg-emerald-500",
      };
    case "failed":
      return {
        label: "验收未通过",
        badgeClass: "bg-red-500/10 text-red-600 dark:text-red-400 border-red-500/20",
        dotClass: "bg-red-500",
      };
    case "insufficient_evidence":
      return {
        label: "证据不足",
        badgeClass: "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20",
        dotClass: "bg-amber-500",
      };
    case "not_applicable":
      return {
        label: "机器验收不适用",
        badgeClass: "bg-slate-500/10 text-slate-500 dark:text-slate-400 border-slate-500/20",
        dotClass: "bg-slate-400",
      };
    case "checking":
      return {
        label: "系统验收中",
        badgeClass: "bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20",
        dotClass: "bg-blue-500 animate-pulse",
      };
    case "pending":
    default:
      return {
        label: "进行中",
        badgeClass: "bg-muted text-muted-foreground border-border",
        dotClass: "bg-muted-foreground",
      };
  }
}

export function formatCriterionStatus(status: CriterionStatus): {
  icon: string;
  label: string;
  iconClass: string;
} {
  switch (status) {
    case "passed":
      return { icon: "✓", label: "已达成", iconClass: "text-emerald-600 dark:text-emerald-400" };
    case "failed":
      return { icon: "✕", label: "未通过", iconClass: "text-red-600 dark:text-red-400" };
    case "insufficient_evidence":
      return {
        icon: "△",
        label: "待补充依据",
        iconClass: "text-amber-600 dark:text-amber-400",
      };
    case "checking":
      return { icon: "◷", label: "校验中", iconClass: "text-blue-500 animate-spin" };
    case "pending":
    default:
      return { icon: "○", label: "待执行", iconClass: "text-muted-foreground" };
  }
}

export function formatVerifierType(type: VerifierType): {
  label: string;
  badgeClass: string;
} {
  switch (type) {
    case "artifact":
      return {
        label: "成果校验",
        badgeClass: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-300",
      };
    case "file":
      return { label: "文件校验", badgeClass: "bg-blue-500/10 text-blue-600 dark:text-blue-300" };
    case "structured":
      return {
        label: "结构化校验",
        badgeClass: "bg-indigo-500/10 text-indigo-600 dark:text-indigo-300",
      };
    case "machine":
      return {
        label: "机器确定性",
        badgeClass: "bg-purple-500/10 text-purple-600 dark:text-purple-300",
      };
    case "llm":
      return {
        label: "模型评估",
        badgeClass: "bg-amber-500/10 text-amber-600 dark:text-amber-300",
      };
    case "composite":
    default:
      return { label: "来源核验", badgeClass: "bg-cyan-500/10 text-cyan-600 dark:text-cyan-300" };
  }
}
