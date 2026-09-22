import type {
  InboxItem,
  WorkArtifactStatus,
  WorkArtifactSummary,
  WorkProgressSnapshot,
  WorkResultOutcome,
  WorkResultPresentation,
  WorkRunProgressView,
} from "$lib/types/work";

/**
 * Format artifact type into user-friendly Chinese label.
 */
export function formatArtifactType(type: string, path?: string): string {
  const normalizedType = type?.trim().toLowerCase() || "";
  let ext = "";
  if (path) {
    const parts = path.split(".");
    if (parts.length > 1) {
      ext = parts[parts.length - 1].toLowerCase();
    }
  }

  const effective = normalizedType || ext;

  switch (effective) {
    case "pdf":
      return "PDF";
    case "docx":
    case "doc":
      return "Word 文档";
    case "xlsx":
    case "xls":
      return "Excel 表格";
    case "pptx":
    case "ppt":
      return "PowerPoint";
    case "md":
    case "markdown":
      return "Markdown";
    case "txt":
    case "text":
      return "文本";
    case "csv":
    case "tsv":
      return "CSV";
    case "png":
    case "jpg":
    case "jpeg":
    case "gif":
    case "webp":
    case "svg":
    case "bmp":
    case "image":
      return "图片";
    case "html":
    case "htm":
      return "HTML";
    case "json":
      return "JSON";
    case "webm":
    case "mp4":
    case "mov":
    case "mkv":
    case "video":
      return "视频";
    default:
      return ext ? ext.toUpperCase() : "文件";
  }
}

/**
 * User-facing artifact lifecycle status label and secondary guidance.
 * "validated" means the file passed checks; only "delivered" is a final
 * deliverable state. Neither status guarantees semantic 100% quality.
 */
export function formatArtifactStatus(status: WorkArtifactStatus | "expected"): {
  label: string;
  tooltip: string;
} {
  switch (status) {
    case "expected":
      return { label: "预期产出", tooltip: "任务声明的预期交付文件，等待创建" };
    case "creating":
      return { label: "生成中", tooltip: "正在生成文件内容" };
    case "ready":
      return { label: "待文件检查", tooltip: "文件已生成，等待格式检查" };
    case "invalid":
      return { label: "文件无效", tooltip: "文件格式检查未通过" };
    case "failed":
      return { label: "生成失败", tooltip: "生成文件过程出错" };
    case "validated":
      return { label: "文件检查通过", tooltip: "已通过基本文件结构与格式检查" };
    case "delivered":
      return { label: "已交付", tooltip: "文件已交付至工作区输出目录" };
    default:
      return { label: "未知", tooltip: "" };
  }
}

/**
 * Format byte count into human readable string.
 */
export function formatFileSize(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * Artifact Hub category label (filter chips and card badges).
 */
export function formatArtifactCategory(
  category: import("$lib/types/work").WorkArtifactCategory,
): string {
  switch (category) {
    case "document":
      return "文档";
    case "spreadsheet":
      return "表格";
    case "presentation":
      return "演示文稿";
    case "pdf":
      return "PDF";
    case "image":
      return "图片";
    case "html":
      return "HTML";
    case "code":
      return "代码";
    case "archive":
      return "压缩包";
    default:
      return "其他";
  }
}

/**
 * Compact duration label for run receipts (e.g. 2m43s).
 */
export function formatDurationMs(durationMs: number | null | undefined): string {
  if (durationMs == null || durationMs < 0) return "—";
  const totalSeconds = Math.round(durationMs / 1000);
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  if (minutes < 60) return seconds === 0 ? `${minutes}m` : `${minutes}m${seconds}s`;
  const hours = Math.floor(minutes / 60);
  const restMinutes = minutes % 60;
  return restMinutes === 0 ? `${hours}h` : `${hours}h${restMinutes}m`;
}

export interface DeriveWorkResultOptions {
  progressView?: WorkRunProgressView | null;
  progressSnapshot?: WorkProgressSnapshot | null;
  artifacts?: WorkArtifactSummary[];
  /**
   * If false (default), progressSnapshot is ignored when progressView is missing,
   * preventing workspace-backed runs from falsely falling back to session-derived terminal states.
   * Only standalone runs should set this to true.
   */
  allowSessionFallback?: boolean;
  /** 可靠的任务运行时长（毫秒）。 */
  durationMs?: number | null;
  /** 任务开始时间 ISO 字符串。 */
  startedAt?: string | null;
  /** 任务完成时间 ISO 字符串。 */
  completedAt?: string | null;
}

export interface WorkOperationalEvidenceOptions {
  progressView?: WorkRunProgressView | null;
  progressSnapshot?: WorkProgressSnapshot | null;
  artifacts?: WorkArtifactSummary[];
  pendingInteractions?: InboxItem[];
}

/**
 * Return whether a Work conversation has crossed from a normal answer into an
 * operational task. A workspace-backed session can have WorkTask/WorkRun ids
 * even when the agent only answered a short question, so ids alone are not a
 * reliable signal for showing task-oriented UI.
 */
export function hasOperationalWorkEvidence(options: WorkOperationalEvidenceOptions): boolean {
  const { progressView, progressSnapshot, artifacts = [], pendingInteractions = [] } = options;
  // Be fail-closed: a goal, proposed plan/tool, terminal status, or generic
  // error can all exist for an answer-only session and therefore are not
  // evidence that Work actually operated on the user's behalf.
  const steps = progressView?.steps ?? progressSnapshot?.tasks ?? [];
  const hasExecutedStep = steps.some((step) => step.status !== "pending");
  const toolSummary = progressView?.toolSummary;
  const toolCallCount = progressSnapshot?.toolCallCount ?? 0;
  const subagents = progressView?.agents ?? progressSnapshot?.subagents ?? [];
  const hasToolActivity = Boolean(
    toolCallCount > 0 ||
    (toolSummary &&
      [toolSummary.started, toolSummary.completed, toolSummary.failed, toolSummary.running].some(
        (count) => count > 0,
      )),
  );
  const hasAttention = Boolean(
    pendingInteractions.length > 0 ||
    (progressView?.attention && progressView.attention.kind !== "guardian_anomaly") ||
    (progressSnapshot?.pendingApprovalCount ?? 0) > 0 ||
    progressSnapshot?.pendingAccessRoot ||
    progressSnapshot?.pendingElicitation,
  );
  const hasConcreteActivity = Boolean(
    progressView?.currentActivity?.toolName ||
    progressView?.currentActivity?.stepId ||
    progressView?.currentActivity?.kind === "tool" ||
    progressView?.currentActivity?.kind === "step" ||
    progressView?.currentActivity?.kind === "researching" ||
    progressView?.currentActivity?.kind === "implementing" ||
    progressView?.currentActivity?.kind === "reviewing",
  );

  return Boolean(
    artifacts.length > 0 ||
    hasExecutedStep ||
    hasToolActivity ||
    subagents.length > 0 ||
    hasAttention ||
    hasConcreteActivity ||
    progressView?.checkpoint ||
    progressSnapshot?.activeToolName,
  );
}

/**
 * Return whether the available Work evidence is strong enough to claim task
 * completion. Ending an Agent turn, having a goal, or proposing a tool is not
 * enough: completion must be supported by completed steps, delivered files,
 * or successful tool execution with no failed or running calls.
 */
export function hasWorkCompletionEvidence(options: WorkOperationalEvidenceOptions): boolean {
  const { progressView, progressSnapshot, artifacts = [] } = options;
  const steps = progressView?.steps ?? progressSnapshot?.tasks ?? [];
  const goalSpec = progressView?.goalSpec ?? progressSnapshot?.taskState?.goalSpec ?? null;
  const toolSummary = progressView?.toolSummary;

  if (artifacts.some((artifact) => artifact.status !== "delivered")) return false;
  if (
    toolSummary &&
    (toolSummary.failed > 0 || toolSummary.running > 0) &&
    goalSpec?.status !== "passed"
  ) {
    return false;
  }
  if (steps.length > 0 && !steps.every((step) => step.status === "completed")) return false;
  if (goalSpec && goalSpec.status !== "passed" && goalSpec.status !== "not_applicable") {
    return false;
  }

  if (artifacts.length > 0 || steps.length > 0) return true;

  // A successful tool-only operation is still real execution evidence. A
  // mixed or failed tool batch stays non-terminal until the run is retried or
  // otherwise produces a verified completion signal.
  return Boolean(
    toolSummary &&
    toolSummary.completed > 0 &&
    toolSummary.failed === 0 &&
    toolSummary.running === 0,
  );
}

/**
 * Derive the structured WorkResultPresentation model for terminal task result cards.
 * Pure presentation derivation — NO persistent side-effects or synthetic AI summarization.
 */
export function deriveWorkResultPresentation(
  options: DeriveWorkResultOptions,
): WorkResultPresentation {
  const { progressView, progressSnapshot, artifacts = [], allowSessionFallback = false } = options;

  // 1. Outcome & Terminal detection (Authoritative WorkRunProgressView takes precedence over Session snapshot)
  let outcome: WorkResultOutcome = "unknown";
  let isTerminal = false;

  const hasUndeliveredArtifacts = artifacts.some((artifact) => artifact.status !== "delivered");
  const hasCompletionEvidence = hasWorkCompletionEvidence({
    progressView,
    progressSnapshot,
    artifacts,
  });
  const completionClaimedByView = (view: WorkRunProgressView): boolean =>
    view.runStatus === "completed" || view.phase === "completed";
  const completionClaimedBySnapshot = progressSnapshot
    ? progressSnapshot.runStatus === "completed" ||
      progressSnapshot.phase === "completed" ||
      progressSnapshot.sessionPhase === "completed"
    : false;

  if (progressView) {
    const status = progressView.runStatus;
    const phase = progressView.phase;

    if (status === "waiting_delivery" || phase === "awaiting_delivery") {
      // Delivery acceptance is an active, recoverable state. It must never be
      // rendered as a terminal success just because the Agent turn ended.
      outcome = "unknown";
      isTerminal = false;
    } else if (status === "recoverable" || phase === "recoverable") {
      outcome = "unknown";
      isTerminal = false;
    } else if (status === "completed" || phase === "completed") {
      if (hasUndeliveredArtifacts || !hasCompletionEvidence) {
        outcome = "unknown";
        isTerminal = false;
      } else {
        outcome = "completed";
        isTerminal = true;
      }
    } else if (status === "failed" || phase === "failed" || phase === "blocked") {
      outcome = "failed";
      isTerminal = true;
    } else if (status === "cancelled" || phase === "cancelled") {
      outcome = "cancelled";
      isTerminal = true;
    }
  } else if (allowSessionFallback && progressSnapshot) {
    const status = progressSnapshot.runStatus;
    const phase = progressSnapshot.phase;
    const sessionPhase = progressSnapshot.sessionPhase;

    if (status === "completed" || phase === "completed" || sessionPhase === "completed") {
      if (hasUndeliveredArtifacts || !hasCompletionEvidence) {
        outcome = "unknown";
        isTerminal = false;
      } else {
        outcome = "completed";
        isTerminal = true;
      }
    } else if (status === "failed" || phase === "failed" || sessionPhase === "failed") {
      outcome = "failed";
      isTerminal = true;
    } else if (status === "cancelled" || phase === "cancelled" || sessionPhase === "cancelled") {
      outcome = "cancelled";
      isTerminal = true;
    }
  }

  // 2. Goal and Steps completion
  const goal = progressView?.goal ?? progressSnapshot?.taskState?.goal ?? undefined;
  const steps = progressView?.steps ?? progressSnapshot?.tasks ?? [];
  const completedSteps = steps.filter((s) => s.status === "completed").length;
  const totalSteps = steps.length;

  // 3. Artifacts statistics
  const artifactCount = artifacts.length;
  const deliveredCount = artifacts.filter((a) => a.status === "delivered").length;
  const problematicCount = artifacts.filter(
    (a) => a.status === "invalid" || a.status === "failed",
  ).length;
  const hasFileResults = artifactCount > 0;

  // 4. Primary Artifact Selection (Deterministic heuristic)
  // Priority: delivered/validated -> ready -> none (invalid/failed are excluded)
  let primaryArtifact: WorkArtifactSummary | undefined;
  const validCandidates = artifacts.filter(
    (a) => a.status === "delivered" || a.status === "validated",
  );
  if (validCandidates.length > 0) {
    primaryArtifact = validCandidates[0];
  } else {
    const readyCandidates = artifacts.filter((a) => a.status === "ready");
    if (readyCandidates.length > 0) {
      primaryArtifact = readyCandidates[0];
    }
  }

  // 5. Title & Subtitle construction
  let title = "";
  let subtitle: string | undefined;

  if (outcome === "completed") {
    title = "任务已完成";
    if (!hasFileResults) {
      subtitle = "执行已完成，没有文件型成果。";
    }
  } else if (outcome === "failed") {
    if (hasFileResults && deliveredCount > 0) {
      title = "任务未完全完成";
      subtitle = `执行过程中产生了 ${artifactCount} 个成果文件，这些文件仍可查看或导出。`;
    } else {
      title = "任务未完成";
    }
  } else if (outcome === "cancelled") {
    title = "任务已取消";
    if (hasFileResults) {
      subtitle = "已产生的文件仍可查看。";
    }
  } else if (
    progressView?.runStatus === "waiting_delivery" ||
    progressView?.phase === "awaiting_delivery"
  ) {
    title = "等待交付物验收";
    subtitle = "当前 Run 已结束执行，但必需文件尚未通过验收。";
  } else if (
    hasUndeliveredArtifacts &&
    (progressView ? completionClaimedByView(progressView) : completionClaimedBySnapshot)
  ) {
    title = "等待交付物验收";
    subtitle = "当前 Run 已结束执行，但仍有成果未完成交付。";
  } else if (progressView?.runStatus === "recoverable" || progressView?.phase === "recoverable") {
    title = "任务可恢复";
    subtitle = "任务在执行中断后等待你的恢复选择。";
  }

  // 6. Error / failed-step extraction (factual only: snapshot error text, subagent errors,
  // and the last in_progress step. No fabricated reasons.)
  let error: string | undefined;
  if (progressView) {
    const agentWithErrors = progressView.agents.filter((a) => a.error && a.error.trim());
    if (agentWithErrors.length > 0) {
      error = agentWithErrors.map((a) => (a.role ? `${a.role}: ${a.error}` : a.error)).join("；");
    }
  }
  if (!error && progressSnapshot?.error?.trim()) {
    error = progressSnapshot.error.trim();
  }
  if (!error && progressSnapshot?.subagents) {
    const subagentErrors = progressSnapshot.subagents
      .filter((s) => s.error && s.error.trim())
      .map((s) => (s.role ? `${s.role}: ${s.error}` : s.error));
    if (subagentErrors.length > 0) error = subagentErrors.join("；");
  }
  const inProgressSteps = steps.filter((s) => s.status === "in_progress");
  const failedStep =
    inProgressSteps.length > 0 ? inProgressSteps[inProgressSteps.length - 1].text : undefined;

  if (outcome === "failed") {
    if (error && !subtitle) subtitle = error;
  } else if (outcome === "cancelled" && !subtitle && error) {
    subtitle = error;
  }

  // 7. Conservative collaboration summary (Factual subagent accomplishments only; NO fake PASS verdicts)
  let collaborationSummary: string | undefined;
  if (progressView?.agents && progressView.agents.length > 0) {
    const resCompleted = progressView.agents.filter(
      (a) => a.role === "researcher" && a.status === "completed",
    ).length;
    const hasReviewerCompleted = progressView.agents.some(
      (a) => a.role === "reviewer" && a.status === "completed",
    );

    if (resCompleted > 0 && hasReviewerCompleted) {
      collaborationSummary = `${resCompleted} 位研究助手完成调查 · 独立审阅已完成`;
    } else if (resCompleted > 0) {
      collaborationSummary = `${resCompleted} 位研究助手完成调查`;
    } else if (hasReviewerCompleted) {
      collaborationSummary = "独立审阅已完成";
    }
  }

  // 8. Duration calculation
  // Priority: 1. reliable durationMs -> 2. startedAt + completedAt -> 3. undefined (hidden)
  let durationMs: number | undefined;
  let durationFormatted: string | undefined;

  if (options.durationMs != null && options.durationMs > 0) {
    durationMs = options.durationMs;
    durationFormatted = formatDurationMs(durationMs);
  } else if (options.startedAt && options.completedAt) {
    const start = new Date(options.startedAt).getTime();
    const end = new Date(options.completedAt).getTime();
    if (!isNaN(start) && !isNaN(end) && end >= start) {
      durationMs = end - start;
      durationFormatted = formatDurationMs(durationMs);
    }
  }

  return {
    outcome,
    title,
    subtitle,
    goal: goal || undefined,
    completedSteps,
    totalSteps,
    artifactCount,
    deliveredCount,
    problematicCount,
    primaryArtifact,
    hasFileResults,
    isTerminal,
    collaborationSummary,
    error,
    failedStep,
    durationMs,
    durationFormatted,
  };
}
