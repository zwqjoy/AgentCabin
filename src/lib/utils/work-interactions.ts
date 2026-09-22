import type { TaskRun } from "$lib/types";
import type { InboxItem, TaskStandingRule, ToolRiskClass } from "$lib/types/work";

/**
 * Filter inbox items to only those belonging to the given current conversation (TaskRun).
 * Matches strictly on durable identifiers: workspace_id, work_run_id, work_task_id, or session run id.
 */
export function filterCurrentRunInteractions(
  items: InboxItem[],
  run: TaskRun | null | undefined,
  onlyPending = true,
): InboxItem[] {
  if (!run) return [];

  const matched = items.filter((item) => {
    if (onlyPending && item.status !== "pending") return false;
    if (run.workspace_id && item.workspaceId && item.workspaceId !== run.workspace_id) {
      return false;
    }

    // Match by work_run_id if both have it
    if (run.work_run_id && item.runId === run.work_run_id) return true;

    // Match by session run id if item.runId is session id
    if (run.id && item.runId === run.id) return true;

    // Match by task_id if work_run_id is not specified or matches
    if (run.work_task_id && item.taskId === run.work_task_id) {
      if (!run.work_run_id || item.runId === run.work_run_id) return true;
    }

    return false;
  });

  return [...matched].sort(
    (a, b) => a.createdAt.localeCompare(b.createdAt) || a.id.localeCompare(b.id),
  );
}

/**
 * Determine whether an inbox item is an interactive question rather than an
 * approval. Question items must keep the conversation in an answerable state
 * even when the underlying session has already stopped waiting for the tool.
 */
export function isQuestionInteraction(item: InboxItem): boolean {
  return (
    item.itemType === "question_elicitation" ||
    item.payload.toolName === "ask_questions" ||
    item.payload.interactionKind === "question_elicitation"
  );
}

/**
 * Keep the bottom-of-chat pending list from duplicating an interaction that is
 * already attached to its inline tool card in the current conversation.
 *
 * The Inbox item remains the single source of truth; this only changes which
 * presentation surface renders it. Items without an inline tool card stay in
 * the bottom list so auth, questions, and other unattached interactions remain
 * actionable.
 */
export function excludeInlineInteractions(
  items: InboxItem[],
  inlineInteractionIds: ReadonlySet<string>,
): InboxItem[] {
  return items.filter((item) => !inlineInteractionIds.has(item.id));
}

/**
 * Normalize an absolute path for user display by replacing home directories with ~.
 */
export function formatDirectoryPath(path: string): string {
  if (!path) return "";
  const trimmed = path.trim();
  return trimmed.replace(/^\/Users\/[^/]+/, "~").replace(/^\/home\/[^/]+/, "~");
}

/**
 * Extract a user-friendly folder title segment from a directory path.
 */
export function getDirectoryDisplayName(path: string): string {
  if (!path) return "目录";
  const normalized = path.replace(/[/\\]+$/, "");
  const segments = normalized.split(/[/\\]/);
  const last = segments[segments.length - 1] || "";
  const lower = last.toLowerCase();

  if (lower === "crm") return "客户数据目录";
  if (lower === "sales") return "销售数据目录";
  if (lower === "documents" || lower === "docs") return "文档目录";
  if (last) return `${last} 目录`;
  return "目录";
}

/**
 * Determine if an inbox item represents an external directory access root request.
 */
export function isAccessRootRequest(item: InboxItem): boolean {
  return (
    item.itemType === "access_root_request" ||
    item.payload.toolName === "work_request_directory_access"
  );
}

/**
 * Extract directory path from an interaction's payload.
 */
export function getDirectoryPathFromItem(item: InboxItem): string {
  if (typeof item.payload.path === "string" && item.payload.path.trim()) {
    return item.payload.path.trim();
  }
  if (
    item.payload.parameters &&
    typeof item.payload.parameters.path === "string" &&
    item.payload.parameters.path.trim()
  ) {
    return item.payload.parameters.path.trim();
  }
  if (
    item.payload.parameters &&
    typeof item.payload.parameters.target === "string" &&
    item.payload.parameters.target.trim()
  ) {
    return item.payload.parameters.target.trim();
  }
  return "";
}

/**
 * Determine if an inbox item represents an app connection request.
 */
export function isAppConnectionRequest(item: InboxItem): boolean {
  return (
    item.itemType === "app_connection_request" ||
    item.payload.interactionKind === "app_connection_request"
  );
}

/** Determine if an Inbox item is the generic Connector Package auth request. */
export function isConnectorAuthRequest(item: InboxItem): boolean {
  return (
    item.itemType === "connector_auth_request" ||
    item.payload.interactionKind === "connector_auth_request"
  );
}

/**
 * Produce user-facing Chinese title for an interaction card.
 */
export function getInteractionTitle(item: InboxItem): string {
  if (item.payload.failureKind === "automation_failure") {
    return item.title || "自动化运行失败";
  }
  if (isAccessRootRequest(item)) {
    const rawPath = getDirectoryPathFromItem(item);
    const dirName = getDirectoryDisplayName(rawPath);
    return `允许访问${dirName}？`;
  }
  if (isConnectorAuthRequest(item)) {
    const connectorId = item.payload.connectorId || item.payload.appId || "外部连接器";
    return `连接并授权 ${connectorId}？`;
  }
  if (isAppConnectionRequest(item)) {
    const connectorId = item.payload.connectorId || item.payload.appId || "外部连接器";
    return `连接并授权 ${connectorId}？`;
  }
  if (item.itemType === "plan_approval") {
    return "方案审批";
  }
  if (item.itemType === "artifact_validation") {
    return "成果验证";
  }
  if (item.itemType === "question_elicitation") {
    return item.payload.question || "等待你补充信息";
  }
  if (item.itemType === "permission_request") {
    const semantic = describePermissionAction(item);
    if (semantic) return semantic;
  }
  if (item.title && !item.title.startsWith("Approval Needed:")) {
    return item.title;
  }
  return "等待确认";
}

/** Human-readable labels for work_execute actions, shown in approval cards. */
const WORK_ACTION_LABELS: Record<string, string> = {
  inspect_dataset: "查看数据集结构",
  summarize_dataset: "汇总数据集",
  export_table: "导出表格",
  join_datasets: "合并数据集",
  compare_periods: "对比时间区间",
  generate: "生成文档",
  render: "渲染演示",
  load_table: "加载表格",
  pivot: "透视分析",
  cohort_analysis: "队列分析",
  period_analysis: "区间分析",
  region_and_dimension_analysis: "区域与维度分析",
};

/** Produce a semantic title like "导出表格（input/Q2-sales.xlsx）" for permission requests. */
function describePermissionAction(item: InboxItem): string | null {
  const toolName = item.payload.toolName ?? "";
  const params = (item.payload.parameters ?? {}) as Record<string, unknown>;
  if (toolName === "work_execute") {
    const action = typeof params.action === "string" ? params.action : "";
    const resId =
      typeof params.resource_id === "string"
        ? params.resource_id
        : typeof params.resourceId === "string"
          ? params.resourceId
          : "";
    const label = WORK_ACTION_LABELS[action] || action || "执行能力";
    const file =
      typeof params.file === "string"
        ? params.file
        : typeof params.path === "string"
          ? params.path
          : "";
    if (file) return `${label}（${file}）`;
    if (resId) return `${label}（${resId}）`;
    return label;
  }
  if (toolName === "work_read_file") {
    const path = typeof params.path === "string" ? params.path : "";
    return path ? `读取文件（${path}）` : "读取文件";
  }
  if (toolName === "work_list_files") {
    const path = typeof params.path === "string" ? params.path : "";
    return path ? `列出文件（${path}）` : "列出文件";
  }
  if (toolName === "work_write_file" || toolName === "work_edit_file") {
    const path = typeof params.path === "string" ? params.path : "";
    const verb = toolName === "work_write_file" ? "写入文件" : "编辑文件";
    return path ? `${verb}（${path}）` : verb;
  }
  if (toolName === "work_run_command") {
    const target = typeof params.target === "string" ? params.target : "";
    return target ? `运行命令：${target}` : "运行命令";
  }
  return null;
}

/**
 * Produce user-facing Chinese description for an interaction card without fabricating facts.
 */
export function getInteractionDescription(item: InboxItem): string {
  if (item.payload.failureKind === "automation_failure") {
    return item.payload.failureReason || item.description || "自动化运行失败，请选择重试或取消。";
  }
  if (isAccessRootRequest(item)) {
    const purpose =
      item.payload.purpose ||
      (typeof item.payload.parameters?.purpose === "string" ? item.payload.parameters.purpose : "");
    if (
      purpose &&
      purpose.trim() &&
      !purpose.includes("Task requires external directory access.") &&
      !purpose.includes("Execution is paused")
    ) {
      return purpose.trim();
    }
    const isWritable = item.payload.writable === true || item.payload.parameters?.writable === true;
    return isWritable
      ? "Agent 需要读写该目录中的数据，以继续当前任务。"
      : "Agent 需要读取该目录中的数据，以继续当前任务。";
  }

  if (isConnectorAuthRequest(item)) {
    const connectorId = item.payload.connectorId || item.payload.appId || "该连接器";
    if (item.payload.runtimeKind === "cli") {
      return `连接器「${connectorId}」需要先完成 CLI 认证，认证信息只保留在 AgentCabin Host。`;
    }
    const scopes = item.payload.requestedScopes?.filter(Boolean) ?? [];
    return scopes.length > 0
      ? `为继续当前任务，需要授权「${connectorId}」的 ${scopes.join("、")} 权限。`
      : `为继续当前任务，需要在 AgentCabin Host 中完成「${connectorId}」授权。`;
  }

  if (
    item.description &&
    !item.description.includes("Execution is paused pending human approval.")
  ) {
    return item.description;
  }

  if (item.itemType === "permission_request") {
    return "Agent 请求执行此操作以继续当前任务。";
  }

  return "Agent 需要你的确认以继续执行。";
}
export function riskClassForTool(toolName: string): ToolRiskClass {
  const name = toolName.toLowerCase();
  if (
    name.startsWith("work_read") ||
    name.startsWith("work_list") ||
    name.startsWith("work_inspect") ||
    name.startsWith("read_") ||
    name.startsWith("view_") ||
    name.startsWith("list_")
  ) {
    return "read";
  }
  if (
    name.startsWith("work_write") ||
    name.startsWith("work_edit") ||
    name.startsWith("work_scratch") ||
    name.startsWith("work_artifact_create") ||
    name.startsWith("write_") ||
    name.startsWith("edit_")
  ) {
    return "write_local";
  }
  if (
    name.startsWith("work_execute") ||
    name.startsWith("work_run_command") ||
    name.startsWith("bash") ||
    name.startsWith("run_command") ||
    name.startsWith("exec_")
  ) {
    return "exec";
  }
  return "external";
}

/**
 * Build a task-scoped standing rule from an inbox item's standingRuleProposal.
 * Returns null when the item carries no proposal (button should stay hidden).
 */
const VALID_RISK_CLASSES: readonly ToolRiskClass[] = ["read", "write_local", "exec", "external"];

export function buildStandingRuleFromProposal(item: InboxItem): TaskStandingRule | null {
  const prop = item.payload.standingRuleProposal;
  if (!prop || !prop.toolName || !prop.targetPattern) return null;
  // Prefer the backend-resolved risk class (action-level for work_execute);
  // fall back to the tool-name heuristic for older proposals.
  const riskClass = VALID_RISK_CLASSES.includes(prop.riskClass as ToolRiskClass)
    ? (prop.riskClass as ToolRiskClass)
    : riskClassForTool(prop.toolName);
  return {
    id: `rule-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    toolName: prop.toolName,
    targetPattern: prop.targetPattern,
    riskClass,
    grantedAt: new Date().toISOString(),
  };
}
