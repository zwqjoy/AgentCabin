import type { TimelineEntry } from "$lib/types";
import type { WorkSubagentRecord, WorkSubagentSummary } from "$lib/types/work";

export function areSubagentRecordsEqual(a: WorkSubagentRecord[], b: WorkSubagentRecord[]): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    const ra = a[i];
    const rb = b[i];
    if (
      ra.agentId !== rb.agentId ||
      ra.status !== rb.status ||
      ra.role !== rb.role ||
      ra.resultSummary !== rb.resultSummary ||
      ra.error !== rb.error
    ) {
      return false;
    }
  }
  return true;
}

export function projectWorkSubagent(
  record: WorkSubagentRecord,
  visibleTimeline: TimelineEntry[],
): WorkSubagentSummary {
  const rawStatus = (record.status || "running").toLowerCase();
  const status: "running" | "completed" | "failed" | "stopped" | "interrupted" =
    rawStatus === "completed"
      ? "completed"
      : rawStatus === "failed"
        ? "failed"
        : rawStatus === "stopped"
          ? "stopped"
          : rawStatus === "interrupted"
            ? "interrupted"
            : "running";
  const statusText =
    status === "completed"
      ? "已完成"
      : status === "failed"
        ? record.error
          ? `失败: ${record.error.slice(0, 30)}`
          : "执行失败"
        : status === "stopped"
          ? "已停止"
          : status === "interrupted"
            ? "已中断"
            : "正在执行";

  // Find matching task description from visible timeline
  let task = record.resultSummary || "";
  for (const entry of visibleTimeline) {
    if (entry.kind === "tool" && entry.tool.tool_name === "work_delegate") {
      const input = (entry.tool.input ?? {}) as Record<string, unknown>;
      const output = (entry.tool.output ?? {}) as Record<string, unknown>;
      const details = (
        output.details && typeof output.details === "object" ? output.details : output
      ) as Record<string, unknown>;
      const entryAgentId = String(details.agent_id ?? input.agent_id ?? entry.id);
      if (entryAgentId === record.agentId || entryAgentId === record.providerRunId) {
        if (input.task) task = String(input.task);
        break;
      }
    }
  }

  return {
    id: record.agentId,
    agentId: record.agentId,
    role: record.role,
    task,
    resultSummary: record.resultSummary || undefined,
    error: record.error || undefined,
    status,
    statusText,
  };
}
