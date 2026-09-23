import type { TaskRun } from "$lib/types";
import type { InboxItem } from "$lib/types/work";
import { filterCurrentRunInteractions, isQuestionInteraction } from "$lib/utils/work-interactions";

/**
 * Return the small attention label shown beside a Work conversation in the
 * sidebar. The Inbox remains the source of truth; this only derives display
 * text from its pending items.
 */
export function getWorkSessionAttentionLabel(
  items: InboxItem[],
  run: TaskRun | null | undefined,
): "等待批准" | "等待输入" | "" {
  const pendingItems = filterCurrentRunInteractions(items, run, true);
  if (pendingItems.length === 0) return "";

  // Work permission, plan, directory-access, and artifact decisions are all
  // approval-style interruptions. A question is the only input-style wait.
  return pendingItems.some((item) => !isQuestionInteraction(item)) ? "等待批准" : "等待输入";
}

/**
 * Check if the given status label represents a blocking attention state
 * that requires user intervention.
 */
export function isUrgentAttentionLabel(label: string): boolean {
  return label === "等待批准" || label === "等待输入" || label.includes("等待");
}
