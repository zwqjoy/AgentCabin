<script lang="ts">
  import SessionInfoPanel from "$lib/components/SessionInfoPanel.svelte";
  import WorkArtifactPanel from "./WorkArtifactPanel.svelte";
  import WorkProgressPanel from "./WorkProgressPanel.svelte";
  import WorkRunReceiptPanel from "./WorkRunReceiptPanel.svelte";
  import { workTaskStore } from "$lib/stores/work-task-store.svelte";
  import { hasOperationalWorkEvidence, hasWorkCompletionEvidence } from "$lib/utils/work-result";
  import { normalizeWorkIdentity } from "$lib/utils/work-identity";
  import { isQuestionInteraction } from "$lib/utils/work-interactions";
  import { workRunProgressDisplayPhase } from "$lib/utils/work-progress";
  import type { SessionInfoData } from "$lib/types";
  import type {
    InboxItem,
    WorkArtifactSummary,
    WorkProgressPhase,
    WorkProgressSnapshot,
    WorkRunReceipt,
    WorkRunRecovery,
    WorkRunProgressView,
    WorkArtifactStorageMode,
  } from "$lib/types/work";

  interface Props {
    sessionInfo: SessionInfoData | null;
    progress: WorkProgressSnapshot | null;
    progressView?: WorkRunProgressView | null;
    recovery?: WorkRunRecovery | null;
    readOnly?: boolean;
    artifacts: WorkArtifactSummary[];
    pendingInteractions?: InboxItem[];
    workspaceRoot?: string;
    primaryWorkRoot?: string;
    artifactStorageMode?: WorkArtifactStorageMode;
    onExportArtifact: (id: string) => Promise<void>;
    onOpenArtifact: (id: string) => Promise<void>;
    onDeleteArtifact: (id: string) => Promise<void>;
    onCopyArtifactToPrimary?: (id: string) => Promise<string>;
    onValidateArtifact?: (id: string) => Promise<void>;
    onDeliverArtifact?: (id: string) => Promise<void>;
    onOpenArtifactDirectory?: () => Promise<void>;
    onSaveOfficeArtifact?: (id: string, contentBase64: string) => Promise<void>;
    /** 任务回执加载器（Ledger 投影）；提供后显示“任务回执”分区。 */
    getReceipt?: () => Promise<WorkRunReceipt | null>;
  }

  let {
    sessionInfo,
    progress,
    progressView = null,
    recovery = null,
    readOnly = false,
    artifacts,
    pendingInteractions = [],
    workspaceRoot = "",
    primaryWorkRoot = "",
    artifactStorageMode = "managed",
    onExportArtifact,
    onOpenArtifact,
    onDeleteArtifact,
    onCopyArtifactToPrimary,
    onValidateArtifact,
    onDeliverArtifact,
    onOpenArtifactDirectory,
    onSaveOfficeArtifact,
    getReceipt,
  }: Props = $props();

  // Auto-expand progress panel when there are tasks/operational work, while
  // other operational details remain available on demand.
  let progressOpen = $state(true);
  let artifactsOpen = $state(false);
  let receiptOpen = $state(false);
  let infoOpen = $state(false);

  let receipt = $state<WorkRunReceipt | null>(null);
  let receiptLoading = $state(false);
  let receiptError = $state("");
  let receiptLoadedFor = $state("");
  let lastTerminalReceiptRefreshKey = $state("");
  let lastInspectedRunKey = $state("");

  const receiptKey = $derived(progressView?.workRunId ?? sessionInfo?.runId ?? "__none__");

  // The receipt is a durable Ledger projection; reset it when the inspected
  // run changes so an old run's receipt can never be shown for a new one.
  $effect(() => {
    const key = receiptKey;
    if (key !== lastInspectedRunKey) {
      lastInspectedRunKey = key;
      receipt = null;
      receiptError = "";
      receiptLoadedFor = "";
      lastTerminalReceiptRefreshKey = "";
    }
  });

  async function loadReceipt(force = false) {
    if (!getReceipt || receiptLoading) return;
    const key = receiptKey;
    if (key === "__none__") {
      receipt = null;
      return;
    }
    if (!force && receiptLoadedFor === key) return;
    receiptLoading = true;
    receiptError = "";
    try {
      receipt = await getReceipt();
      receiptLoadedFor = key;
    } catch (cause) {
      receiptError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      receiptLoading = false;
    }
  }

  function toggleReceipt() {
    receiptOpen = !receiptOpen;
    if (receiptOpen) void loadReceipt();
  }

  // Auto-refresh the receipt ONCE per run/terminal-status when open.
  $effect(() => {
    const status = progressView?.runStatus;
    const key = receiptKey;
    const isTerminal = status === "completed" || status === "failed" || status === "cancelled";

    if (receiptOpen && isTerminal && key !== "__none__") {
      const terminalKey = `${key}:${status}`;
      if (lastTerminalReceiptRefreshKey !== terminalKey) {
        lastTerminalReceiptRefreshKey = terminalKey;
        void loadReceipt(true);
      }
    }
  });

  let normalPendingInteractions = $derived(
    pendingInteractions.filter((item) => !item.payload.recoveryAction && !item.payload.recoveryKey),
  );

  let workIdentity = $derived(
    normalizeWorkIdentity({
      run: sessionInfo?.runId
        ? {
            id: sessionInfo.runId,
            work_task_id: progressView?.taskId,
            work_run_id: progressView?.workRunId,
          }
        : null,
      progressView,
      conversationRunId: sessionInfo?.runId ?? null,
    }),
  );

  let effectiveTaskId = $derived(
    workIdentity.taskId ??
      (sessionInfo &&
      "taskId" in sessionInfo &&
      (sessionInfo as unknown as { taskId: string }).taskId !== workIdentity.workRunId
        ? (sessionInfo as unknown as { taskId: string }).taskId
        : null),
  );
  let currentTask = $derived(effectiveTaskId ? workTaskStore.getTaskById(effectiveTaskId) : null);
  let artifactRequirements = $derived(currentTask?.artifactRequirements ?? []);
  let requiredArtifacts = $derived(currentTask?.requiredArtifacts ?? []);
  let hasOperationalWork = $derived(
    Boolean(recovery) ||
      hasOperationalWorkEvidence({
        progressView,
        progressSnapshot: progress,
        artifacts,
        pendingInteractions,
      }),
  );
  let tasks = $derived(progressView?.steps ?? progress?.tasks ?? []);
  let hasTasks = $derived(tasks.length > 0 || Boolean(effectiveTaskId) || hasOperationalWork);
  let lastAutoExpandedKey = $state("");

  const taskRunKey = $derived(
    hasTasks
      ? `${progressView?.workRunId ?? sessionInfo?.runId ?? sessionInfo?.sessionId ?? effectiveTaskId ?? "task"}:${tasks.length > 0 ? "steps" : "nosteps"}`
      : "",
  );

  // Automatically expand the progress panel whenever a task or operational run is present/starts
  $effect(() => {
    if (!taskRunKey) {
      lastAutoExpandedKey = "";
      return;
    }
    if (taskRunKey !== lastAutoExpandedKey) {
      lastAutoExpandedKey = taskRunKey;
      progressOpen = true;
    }
  });
  let hasCompletionEvidence = $derived(
    hasWorkCompletionEvidence({
      progressView,
      progressSnapshot: progress,
      artifacts,
    }),
  );
  let hasArtifactDetails = $derived(
    artifacts.length > 0 ||
      artifactRequirements.length > 0 ||
      requiredArtifacts.length > 0 ||
      Boolean(recovery?.acceptance?.checks?.length),
  );
  let attemptedTaskId = $state("");
  $effect(() => {
    const taskId = effectiveTaskId;
    if (taskId && !currentTask && taskId !== attemptedTaskId) {
      attemptedTaskId = taskId;
      void workTaskStore.fetchTask(taskId).catch(() => {});
    }
  });

  const phaseLabels: Record<WorkProgressPhase, string> = {
    planning: "正在规划",
    running: "正在执行",
    waiting_approval: "等待你处理",
    waiting_input: "等待你的回答",
    validating: "验证成果",
    awaiting_delivery: "等待交付",
    recoverable: "可恢复",
    completed: "已完成",
    failed: "执行失败",
    cancelled: "已取消",
    stopped: "已停止",
    idle: "等待下一步",
  };

  let phase = $derived.by(() => {
    if (normalPendingInteractions.length > 0) {
      return normalPendingInteractions.some(isQuestionInteraction)
        ? "waiting_input"
        : "waiting_approval";
    }
    // A completed WorkRun can leave the live session in stopped state during
    // actor cleanup. Completion evidence must win over that stale session
    // status; an explicit stop without delivery evidence remains stopped.
    if (progress?.runStatus === "stopped") {
      return hasCompletionEvidence ? "completed" : "stopped";
    }
    if (progressView) return workRunProgressDisplayPhase(progressView, artifacts);
    const rawPhase = progress?.phase ?? "idle";
    if (rawPhase !== "completed" || hasCompletionEvidence) return rawPhase;
    return artifacts.some((artifact) => artifact.status !== "delivered")
      ? "awaiting_delivery"
      : "idle";
  });
  // A pending human interaction is actionable evidence even when the live
  // session has stopped. Keep it visible until the Inbox item is resolved.
  let phaseLabel = $derived(phaseLabels[phase] ?? "等待下一步");
  let phaseClass = $derived(
    phase === "completed"
      ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
      : phase === "failed"
        ? "bg-red-500/10 text-red-700 dark:text-red-300"
        : phase === "waiting_approval" || phase === "waiting_input"
          ? "bg-orange-500/10 text-orange-700 dark:text-orange-300"
          : phase === "recoverable" || phase === "awaiting_delivery"
            ? "bg-amber-500/10 text-amber-700 dark:text-amber-300"
            : phase === "running"
              ? "bg-amber-500/10 text-amber-700 dark:text-amber-300"
              : "bg-muted text-muted-foreground",
  );

  let hasInspectorSections = $derived(
    Boolean(recovery) ||
      normalPendingInteractions.length > 0 ||
      hasOperationalWork ||
      tasks.length > 0 ||
      hasArtifactDetails,
  );
</script>

<div
  class="pointer-events-auto flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain bg-background/20"
>
  {#if hasInspectorSections}
    {#if recovery}
      <section class="border-b border-border/60 bg-amber-500/[0.04] p-3">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <span class="h-2 w-2 shrink-0 animate-pulse rounded-full bg-amber-500"></span>
              <span class="text-sm font-semibold text-foreground">
                {recovery.status === "waiting_delivery" ? "等待成果确认" : "任务需要恢复"}
              </span>
            </div>
            <p class="mt-1 text-[11px] leading-4 text-muted-foreground">{recovery.reason}</p>
          </div>
        </div>
      </section>
    {/if}

    {#if normalPendingInteractions.length > 0}
      <section class="border-b border-border/60 bg-orange-500/[0.04] p-3">
        <div class="flex items-center justify-between gap-3">
          <span class="flex min-w-0 items-center gap-2 text-sm font-semibold text-foreground">
            <span class="h-2 w-2 shrink-0 animate-pulse rounded-full bg-orange-500"></span>
            当前对话中有 {normalPendingInteractions.length} 项需要你处理
          </span>
        </div>
      </section>
    {/if}

    {#if hasOperationalWork || tasks.length > 0}
      <section class="border-b border-border/60">
        <button
          type="button"
          class="flex min-h-14 w-full items-center justify-between gap-3 px-4 py-3 text-left transition-colors hover:bg-accent/40"
          aria-expanded={progressOpen}
          onclick={() => (progressOpen = !progressOpen)}
        >
          <span class="flex min-w-0 items-start gap-2.5">
            <span
              class="mt-0.5 text-muted-foreground transition-transform {progressOpen
                ? 'rotate-90'
                : ''}"
              aria-hidden="true">›</span
            >
            <span class="min-w-0">
              <span class="flex items-center gap-2">
                <span
                  class="h-2 w-2 shrink-0 rounded-full {phase === 'running'
                    ? 'animate-pulse bg-amber-500'
                    : phase === 'failed'
                      ? 'bg-red-500'
                      : phase === 'waiting_approval' || phase === 'waiting_input'
                        ? 'animate-pulse bg-orange-500'
                        : 'bg-primary'}"
                ></span>
                <span class="text-sm font-semibold text-foreground">进度</span>
              </span>
              <span class="mt-1 block text-left text-[11px] leading-4 text-muted-foreground">
                长任务的计划、工具执行、确认和成果交付状态会持续显示在这里。
              </span>
            </span>
          </span>
          <span class="shrink-0 rounded-full px-2 py-1 text-[10px] font-medium {phaseClass}"
            >{phaseLabel}</span
          >
        </button>
        {#if progressOpen}
          <div class="px-3 pb-3">
            <WorkProgressPanel
              {progress}
              {progressView}
              {artifacts}
              acceptance={recovery?.acceptance ?? null}
              {artifactRequirements}
              {requiredArtifacts}
              compact
              showHeader={false}
              showArtifactSummary={false}
            />
          </div>
        {/if}
      </section>
    {/if}

    {#if hasArtifactDetails}
      <section class="border-b border-border/60">
        <div class="flex min-h-14 items-center gap-1 pr-2">
          <button
            type="button"
            class="flex min-w-0 flex-1 items-center justify-between gap-3 px-4 py-3 text-left transition-colors hover:bg-accent/40"
            aria-expanded={artifactsOpen}
            onclick={() => (artifactsOpen = !artifactsOpen)}
          >
            <span class="flex min-w-0 items-center gap-2.5">
              <span
                class="text-muted-foreground transition-transform {artifactsOpen
                  ? 'rotate-90'
                  : ''}"
                aria-hidden="true">›</span
              >
              <span class="text-sm font-semibold text-foreground">成果</span>
              <span
                class="rounded-full bg-emerald-500/10 px-2 py-0.5 text-[11px] text-emerald-600 dark:text-emerald-300"
                >{artifacts.length}</span
              >
            </span>
          </button>
          {#if onOpenArtifactDirectory}
            <button
              type="button"
              class="flex shrink-0 items-center gap-1 rounded-lg px-2 py-1.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              title="打开 output 目录"
              onclick={() => void onOpenArtifactDirectory?.()}
            >
              <svg
                viewBox="0 0 24 24"
                class="h-3.5 w-3.5"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path d="M3.5 6.5h6l2 2h9v9a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z" />
              </svg>
              打开目录
            </button>
          {/if}
        </div>
        {#if artifactsOpen}
          <div class="px-3 pb-3">
            <WorkArtifactPanel
              {artifacts}
              requirements={artifactRequirements}
              {requiredArtifacts}
              acceptance={recovery?.acceptance ?? null}
              {readOnly}
              {workspaceRoot}
              {primaryWorkRoot}
              {artifactStorageMode}
              compact
              showHeader={false}
              onExport={onExportArtifact}
              onOpen={onOpenArtifact}
              onDelete={onDeleteArtifact}
              onCopyToPrimary={onCopyArtifactToPrimary}
              onValidate={onValidateArtifact}
              onDeliver={onDeliverArtifact}
              onSaveOffice={onSaveOfficeArtifact}
            />
          </div>
        {/if}
      </section>
    {/if}

    {#if getReceipt && hasOperationalWork}
      <section class="border-b border-border/60">
        <div class="flex min-h-14 items-center gap-1 pr-2">
          <button
            type="button"
            class="flex min-w-0 flex-1 items-center justify-between gap-3 px-4 py-3 text-left transition-colors hover:bg-accent/40"
            aria-expanded={receiptOpen}
            onclick={toggleReceipt}
          >
            <span class="flex min-w-0 items-center gap-2.5">
              <span
                class="text-muted-foreground transition-transform {receiptOpen ? 'rotate-90' : ''}"
                aria-hidden="true">›</span
              >
              <span class="text-sm font-semibold text-foreground">任务回执</span>
              {#if receipt}
                <span
                  class="rounded-full bg-blue-500/10 px-2 py-0.5 text-[11px] text-blue-600 dark:text-blue-300"
                  >{receipt.artifacts.length} 成果 · {receipt.sources.filter((s) => s.accessed)
                    .length} 来源</span
                >
              {/if}
            </span>
          </button>
          {#if receiptOpen}
            <button
              type="button"
              class="flex shrink-0 items-center gap-1 rounded-lg px-2 py-1.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              title="重新汇总"
              onclick={() => void loadReceipt(true)}
            >
              刷新
            </button>
          {/if}
        </div>
        {#if receiptOpen}
          <div class="px-3 pb-3">
            <WorkRunReceiptPanel {receipt} loading={receiptLoading} error={receiptError} />
          </div>
        {/if}
      </section>
    {/if}

    {#if hasOperationalWork}
      <section class="border-b border-border/60">
        <button
          type="button"
          class="flex min-h-12 w-full items-center justify-between gap-3 px-4 py-3 text-left transition-colors hover:bg-accent/40"
          aria-expanded={infoOpen}
          onclick={() => (infoOpen = !infoOpen)}
        >
          <span class="flex items-center gap-2.5">
            <span
              class="text-muted-foreground transition-transform {infoOpen ? 'rotate-90' : ''}"
              aria-hidden="true">›</span
            >
            <span class="text-sm font-semibold text-foreground">执行详情</span>
          </span>
        </button>
        {#if infoOpen}
          <div class="px-3 pb-3">
            <SessionInfoPanel info={sessionInfo} activeTab="info" />
          </div>
        {/if}
      </section>
    {/if}

    {#if hasOperationalWork || tasks.length > 0}
      <div class="flex-1 px-4 py-4 text-[11px] leading-5 text-muted-foreground">
        工作空间知识和访问权限属于工作空间级资源，请在“材料”中管理授权目录和输入文件。
      </div>
    {/if}
  {:else}
    <div class="flex flex-1 flex-col items-center justify-center px-6 py-12 text-center">
      <div
        class="mb-3 flex h-10 w-10 items-center justify-center rounded-xl border border-border/60 bg-muted/30 text-muted-foreground/60"
      >
        <svg
          viewBox="0 0 24 24"
          class="h-5 w-5"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M9 11l3 3L22 4" />
          <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
        </svg>
      </div>
      <div class="text-xs font-medium text-foreground/80">暂无任务进度与成果</div>
      <p class="mt-1.5 max-w-[240px] text-[11px] leading-relaxed text-muted-foreground/60">
        当前为常规问答。当发起多步骤任务或生成交付文件时，实时状态将显示在这里。
      </p>
    </div>
  {/if}
</div>
