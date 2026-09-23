<script lang="ts">
  import type { PiTodoState } from "$lib/types";
  import type {
    InboxItem,
    RunHealth,
    RuntimeLiveness,
    WorkArtifactAcceptance,
    WorkArtifactCheckStatus,
    WorkArtifactRequirement,
    WorkArtifactSummary,
    WorkProgressPhase,
    WorkProgressSnapshot,
    WorkRunProgressView,
  } from "$lib/types/work";
  import { workRunProgressDisplayPhase, workRunProgressToSnapshot } from "$lib/utils/work-progress";
  import { hasOperationalWorkEvidence, hasWorkCompletionEvidence } from "$lib/utils/work-result";
  import { isQuestionInteraction } from "$lib/utils/work-interactions";
  import { workToolLabel } from "$lib/utils/work-activity";
  import WorkGoalCard from "./WorkGoalCard.svelte";
  import TaskChecklistCard from "$lib/components/TaskChecklistCard.svelte";
  import Modal from "$lib/components/Modal.svelte";
  import { verifyWorkGoal, triggerWorkGoalRepair } from "$lib/api/work";

  interface Props {
    progress?: WorkProgressSnapshot | null;
    progressView?: WorkRunProgressView | null;
    artifacts: WorkArtifactSummary[];
    acceptance?: WorkArtifactAcceptance | null;
    artifactRequirements?: WorkArtifactRequirement[];
    requiredArtifacts?: string[];
    compact?: boolean;
    showHeader?: boolean;
    showArtifactSummary?: boolean;
    pendingInteractions?: InboxItem[];
    piTodoState?: PiTodoState;
  }

  let {
    progress = null,
    progressView = null,
    artifacts,
    acceptance = null,
    artifactRequirements = [],
    requiredArtifacts = [],
    compact = false,
    showHeader = true,
    showArtifactSummary = true,
    pendingInteractions = [],
    piTodoState = { phases: [] },
  }: Props = $props();

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

  const phaseClasses: Record<WorkProgressPhase, string> = {
    planning: "bg-violet-500/10 text-violet-700 dark:text-violet-300",
    running: "bg-amber-500/10 text-amber-700 dark:text-amber-300",
    waiting_approval: "bg-orange-500/10 text-orange-700 dark:text-orange-300",
    waiting_input: "bg-orange-500/10 text-orange-700 dark:text-orange-300",
    validating: "bg-blue-500/10 text-blue-700 dark:text-blue-300",
    awaiting_delivery: "bg-cyan-500/10 text-cyan-700 dark:text-cyan-300",
    recoverable: "bg-amber-500/10 text-amber-700 dark:text-amber-300",
    completed: "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
    failed: "bg-red-500/10 text-red-700 dark:text-red-300",
    cancelled: "bg-muted text-muted-foreground",
    stopped: "bg-muted text-muted-foreground",
    idle: "bg-muted text-muted-foreground",
  };

  // Resolved snapshot: if progressView exists, convert; otherwise fallback to progress
  let resolvedSnapshot = $derived(
    progressView && progress?.runStatus !== "stopped"
      ? workRunProgressToSnapshot(progressView, artifacts, progress?.error ?? "")
      : progress,
  );
  let hasCompletionEvidence = $derived(
    hasWorkCompletionEvidence({
      progressView,
      progressSnapshot: resolvedSnapshot,
      artifacts,
    }),
  );

  let phase = $derived.by(() => {
    if (pendingInteractions.length > 0) {
      return pendingInteractions.some(isQuestionInteraction) ? "waiting_input" : "waiting_approval";
    }
    // A completed WorkRun can leave the live session in stopped state during
    // actor cleanup. Completion evidence must win over that stale session
    // status; an explicit stop without delivery evidence remains stopped.
    if (resolvedSnapshot?.runStatus === "stopped") {
      return hasCompletionEvidence ? "completed" : "stopped";
    }
    if (progressView) return workRunProgressDisplayPhase(progressView, artifacts);
    return resolvedSnapshot?.phase ?? "idle";
  });
  let phaseLabel = $derived(phaseLabels[phase] ?? "等待下一步");
  let phaseClass = $derived(phaseClasses[phase] ?? "bg-muted text-muted-foreground");
  let tasks = $derived(progressView ? progressView.steps : (resolvedSnapshot?.tasks ?? []));
  let piTodoSections = $derived(
    piTodoState.phases.map((phase, phaseIndex) => ({
      id: `pi-todo-phase-${phaseIndex}`,
      title: phase.name,
      tasks: phase.tasks.map((task, taskIndex) => ({
        id: `pi-todo-${phaseIndex}-${taskIndex}`,
        text: task.name,
        description: task.description,
        status: task.status,
      })),
    })),
  );
  let completedTasks = $derived(tasks.filter((task) => task.status === "completed").length);
  let deliveredArtifacts = $derived(
    artifacts.filter((artifact) => artifact.status === "delivered").length,
  );
  let validatedArtifacts = $derived(
    artifacts.filter(
      (artifact) => artifact.status === "validated" || artifact.status === "delivered",
    ).length,
  );

  let goal = $derived(progressView?.goal ?? resolvedSnapshot?.taskState?.goal ?? null);
  let goalSpec = $derived(progressView?.goalSpec ?? resolvedSnapshot?.taskState?.goalSpec ?? null);
  let verifyingGoal = $state(false);

  async function handleVerifyGoal() {
    if (!progressView?.taskId || !progressView?.workRunId || verifyingGoal) return;
    verifyingGoal = true;
    try {
      await verifyWorkGoal(progressView.taskId, progressView.workRunId);
    } catch {
      // ignore
    } finally {
      verifyingGoal = false;
    }
  }

  async function handleTriggerRepair() {
    if (!progressView?.taskId || !progressView?.workRunId || verifyingGoal) return;
    verifyingGoal = true;
    try {
      await triggerWorkGoalRepair(progressView.taskId, progressView.workRunId);
    } catch {
      // ignore
    } finally {
      verifyingGoal = false;
    }
  }

  let checkpoint = $derived(
    progressView?.checkpoint ?? resolvedSnapshot?.taskState?.checkpoint ?? null,
  );
  let checkpointSummaryOverflowing = $derived(
    Boolean(checkpoint?.summary && checkpoint.summary.length > 160),
  );
  let checkpointDetailsOpen = $state(false);
  let currentActivity = $derived(progressView?.currentActivity ?? null);
  let attention = $derived(progressView?.attention ?? null);
  let isTerminal = $derived(
    phase === "completed" || phase === "failed" || phase === "cancelled" || phase === "stopped",
  );
  let allTasksCompleted = $derived(tasks.length > 0 && completedTasks === tasks.length);
  let dismissedAnomaly = $state(false);

  let lastAttentionKey = $state<string | null>(null);
  $effect(() => {
    const currentKey = attention ? `${attention.kind}:${attention.count}` : null;
    if (currentKey !== lastAttentionKey) {
      lastAttentionKey = currentKey;
      dismissedAnomaly = false;
    }
  });

  let visibleAttention = $derived.by(() => {
    if (isTerminal) return null;
    if (!attention) return null;
    if (attention.kind === "guardian_anomaly") {
      if (allTasksCompleted || dismissedAnomaly) return null;
    }
    return attention;
  });
  // Pending Inbox items remain actionable after the live actor stops. The
  // resolution itself is the terminal transition for the human loop.
  let visiblePendingInteractions = $derived(pendingInteractions);
  let toolSummary = $derived(progressView?.toolSummary ?? null);

  let health = $derived(progressView?.health ?? null);
  let healthStatus = $derived<RunHealth>(health?.status ?? "healthy");
  let healthReason = $derived(health?.reason ?? null);
  let liveness = $derived<RuntimeLiveness | null>(health?.liveness ?? null);
  let hasOperationalWork = $derived(
    hasOperationalWorkEvidence({
      progressView,
      progressSnapshot: resolvedSnapshot,
      artifacts,
      pendingInteractions,
    }),
  );
  // A plain Work goal is descriptive metadata, not an acceptance contract.
  // Only an explicit GoalSpec should opt a run into verification/repair UI.
  let shouldShowGoalCard = $derived(Boolean(goalSpec) && hasOperationalWork);

  function healthStatusLabel(status: RunHealth): string {
    switch (status) {
      case "healthy":
        return "健康";
      case "warning":
        return "预警";
      case "stalled":
        return "停滞";
      case "degraded":
        return "降级";
      case "needs_attention":
        return "需关注";
      default:
        return status;
    }
  }

  function healthStatusClass(status: RunHealth): string {
    switch (status) {
      case "healthy":
        return "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300 border-emerald-500/30";
      case "warning":
        return "bg-amber-500/10 text-amber-700 dark:text-amber-300 border-amber-500/30";
      case "stalled":
        return "bg-rose-500/10 text-rose-700 dark:text-rose-300 border-rose-500/30";
      case "degraded":
        return "bg-orange-500/10 text-orange-700 dark:text-orange-300 border-orange-500/30";
      case "needs_attention":
        return "bg-red-500/10 text-red-700 dark:text-red-300 border-red-500/30";
      default:
        return "bg-muted text-muted-foreground border-border";
    }
  }

  function livenessLabel(l: RuntimeLiveness): string {
    switch (l) {
      case "alive":
        return "运行正常";
      case "alive_but_stalled":
        return "存活但停滞";
      case "disconnected":
        return "适配器断连";
      case "dead":
        return "进程中断";
      default:
        return l;
    }
  }

  let allRequirements = $derived.by(() => {
    const list: WorkArtifactRequirement[] = [];
    const seenPaths = new Set<string>();

    if (artifactRequirements && artifactRequirements.length > 0) {
      for (const req of artifactRequirements) {
        if (!seenPaths.has(req.path)) {
          seenPaths.add(req.path);
          list.push({
            path: req.path,
            title: req.title || req.path,
            artifactType: req.artifactType,
            required: req.required !== false,
          });
        }
      }
    }

    if (requiredArtifacts && requiredArtifacts.length > 0) {
      for (const path of requiredArtifacts) {
        if (path && !seenPaths.has(path)) {
          seenPaths.add(path);
          list.push({
            path,
            title: path,
            artifactType: null,
            required: true,
          });
        }
      }
    }

    return list;
  });

  let acceptanceCriteria = $derived.by(() => {
    if (acceptance && acceptance.checks && acceptance.checks.length > 0) {
      return acceptance.checks.map((c) => ({
        path: c.requirement.path,
        title: c.requirement.title || c.requirement.path,
        required: c.requirement.required !== false,
        status: c.status as WorkArtifactCheckStatus,
        message: c.message,
        artifactId: c.artifactId,
      }));
    }
    if (allRequirements.length > 0) {
      return allRequirements.map((r) => {
        const matchingArtifact = artifacts.find(
          (a) => a.path.endsWith(r.path) || a.title === r.title || a.path === r.path,
        );
        let status: WorkArtifactCheckStatus = "missing";
        let message = "等待生成";
        if (matchingArtifact) {
          if (matchingArtifact.status === "delivered") {
            status = "satisfied";
            message = "已生成并通过交付验收";
          } else if (matchingArtifact.status === "validated") {
            status = "missing";
            message = "文件格式检查通过，等待最终交付";
          } else if (
            matchingArtifact.status === "invalid" ||
            matchingArtifact.status === "failed"
          ) {
            status = "invalid";
            message = "文件格式检查未通过";
          } else {
            status = "missing";
            message = "生成中 / 待检查";
          }
        }
        return {
          path: r.path,
          title: r.title || r.path,
          required: r.required !== false,
          status,
          message,
          artifactId: matchingArtifact?.id ?? null,
        };
      });
    }
    return [];
  });

  let requiredCriteria = $derived(acceptanceCriteria.filter((c) => c.required));
  let satisfiedRequiredCriteriaCount = $derived(
    requiredCriteria.filter((c) => c.status === "satisfied").length,
  );
  let satisfiedTotalCriteriaCount = $derived(
    acceptanceCriteria.filter((c) => c.status === "satisfied").length,
  );

  let criteriaSummaryBadge = $derived.by(() => {
    if (requiredCriteria.length > 0) {
      return `${satisfiedRequiredCriteriaCount}/${requiredCriteria.length} 项必需达标`;
    }
    if (acceptanceCriteria.length > 0) {
      return `${satisfiedTotalCriteriaCount}/${acceptanceCriteria.length} 项已达标`;
    }
    return "";
  });

  let deliveryProgressLabel = $derived.by(() => {
    if (acceptance && acceptance.requiredCount > 0) {
      return `${acceptance.satisfiedCount}/${acceptance.requiredCount} 项必需达标`;
    }
    if (requiredCriteria.length > 0) {
      return `${satisfiedRequiredCriteriaCount}/${requiredCriteria.length} 项必需交付`;
    }
    if (acceptanceCriteria.length > 0) {
      return `${satisfiedTotalCriteriaCount}/${acceptanceCriteria.length} 项已交付`;
    }
    return `${deliveredArtifacts}/${artifacts.length} 已交付`;
  });

  function checkStatusClass(status: WorkArtifactCheckStatus): string {
    if (status === "satisfied")
      return "text-emerald-600 dark:text-emerald-400 bg-emerald-500/10 border-emerald-500/30";
    if (status === "invalid")
      return "text-red-600 dark:text-red-400 bg-red-500/10 border-red-500/30";
    return "text-amber-600 dark:text-amber-400 bg-amber-500/10 border-amber-500/30";
  }

  function checkStatusLabel(status: WorkArtifactCheckStatus): string {
    if (status === "satisfied") return "验收通过";
    if (status === "invalid") return "需修复";
    return "等待交付";
  }

  function toolLabel(name: string): string {
    return workToolLabel(name);
  }

  function friendlyRoleLabel(role: string): string {
    switch (role.toLowerCase()) {
      case "researcher":
        return "研究助手";
      case "worker":
        return "执行助手";
      case "reviewer":
        return "审阅助手";
      default:
        return role;
    }
  }

  function subagentStatusLabel(status: string): string {
    switch (status) {
      case "running":
        return "执行中";
      case "completed":
        return "已完成";
      case "failed":
        return "已失败";
      case "cancelled":
        return "已取消";
      default:
        return status;
    }
  }

  let subagents = $derived(resolvedSnapshot?.subagents ?? []);
  let runningSubagentsCount = $derived(subagents.filter((s) => s.status === "running").length);
  let completedSubagentsCount = $derived(subagents.filter((s) => s.status === "completed").length);
  const SUBAGENT_TOOL_NAMES = new Set([
    "work_delegate",
    "work_research_swarm",
    "work_implement_review_fix",
    "work_agent_wait",
    "work_agent_status",
    "work_agent_steer",
    "work_agent_stop",
  ]);
  let subagentToolActive = $derived(
    [currentActivity?.toolName, resolvedSnapshot?.activeToolName].some((name) =>
      name ? SUBAGENT_TOOL_NAMES.has(name) : false,
    ),
  );
  let showSubagentSummary = $derived(
    subagents.length > 0 ||
      subagentToolActive ||
      (isTerminal && (progress?.subagents !== undefined || progressView?.agents !== undefined)),
  );

  // Keep the team section itself user-controlled. Individual child cards use
  // native <details> below so their open state survives live progress updates
  // without relying on a reactive click state that can be replaced mid-turn.
  let teamSectionOpen = $state(true);

  function formatSubagentSummary(raw: string): string {
    if (!raw) return "";
    const returnIdx = raw.indexOf("Return:");
    if (returnIdx >= 0) {
      const jsonStr = raw.slice(returnIdx + 7).trim();
      try {
        const parsed = JSON.parse(jsonStr);
        if (parsed && typeof parsed === "object") {
          if (parsed.message) return String(parsed.message);
          if (parsed.summary) return String(parsed.summary);
          if (parsed.result) return String(parsed.result);
          if (parsed.findings) return String(parsed.findings);
        }
      } catch {
        // ignore invalid JSON
      }
    }
    try {
      const parsed = JSON.parse(raw);
      if (parsed && typeof parsed === "object") {
        if (parsed.message) return String(parsed.message);
        if (parsed.summary) return String(parsed.summary);
        if (parsed.result) return String(parsed.result);
      }
    } catch {
      // ignore invalid JSON
    }
    return raw;
  }

  let subagentsHeaderSummary = $derived.by(() => {
    if (subagents.length === 0) return "";
    const allResearchers = subagents.every((s) => s.role.toLowerCase() === "researcher");
    if (allResearchers && subagents.length > 1) {
      if (runningSubagentsCount > 0) {
        return `并行研究中 · ${completedSubagentsCount}/${subagents.length} 已完成`;
      }
      return `并行研究 · ${completedSubagentsCount}/${subagents.length} 完成`;
    }
    if (runningSubagentsCount > 0) {
      return `${runningSubagentsCount} 运行中 / ${subagents.length} 总计`;
    }
    return `${subagents.length} 位协作成员`;
  });
</script>

<section
  class={compact ? "bg-transparent" : "rounded-xl border border-border/60 bg-background/35 p-3.5"}
  aria-label="进度"
>
  {#if showHeader}
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0">
        <div class="flex items-center gap-2">
          <span
            class="h-2 w-2 rounded-full {phase === 'running'
              ? 'bg-amber-500 animate-pulse'
              : phase === 'failed'
                ? 'bg-red-500'
                : phase === 'waiting_approval' || phase === 'waiting_input'
                  ? 'bg-orange-500 animate-pulse'
                  : 'bg-primary'}"
          ></span>
          <h3 id="work-progress-title" class="text-xs font-semibold text-foreground">进度</h3>
        </div>
        <p class="mt-1 text-[10px] leading-4 text-muted-foreground">
          长任务的计划、工具执行、确认和成果交付状态会持续显示在这里。
        </p>
      </div>
      <div class="flex items-center gap-1.5 shrink-0">
        {#if health && healthStatus !== "healthy"}
          <span
            class="rounded-full border px-2 py-0.5 text-[10px] font-medium {healthStatusClass(
              healthStatus,
            )}"
          >
            状态：{healthStatusLabel(healthStatus)}
          </span>
        {/if}
        <span class="shrink-0 rounded-full px-2 py-1 text-[10px] font-medium {phaseClass}"
          >{phaseLabel}</span
        >
      </div>
    </div>
  {/if}

  <!-- Inbox is the canonical human-action queue. -->
  {#if visiblePendingInteractions.length > 0}
    <div
      class="mt-3 flex items-center justify-between gap-3 rounded-lg border border-orange-500/30 bg-orange-500/10 px-2.5 py-2 text-[10px] text-orange-700 dark:text-orange-300"
    >
      <span class="flex min-w-0 items-center gap-1.5 font-semibold">
        <span class="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-orange-500"></span>
        Inbox 中有 {visiblePendingInteractions.length} 项需要你处理
      </span>
      <a
        href="/chat/work?view=inbox"
        class="shrink-0 rounded-md bg-orange-500/20 px-2 py-0.5 font-semibold hover:bg-orange-500/30"
      >
        打开 Inbox →
      </a>
    </div>
  {:else if visibleAttention}
    <div
      class="mt-3 flex items-start justify-between gap-2 rounded-lg border border-orange-500/30 bg-orange-500/10 px-2.5 py-2 text-[10px] text-orange-700 dark:text-orange-300"
    >
      <div class="flex items-start gap-2 min-w-0 flex-1">
        <span class="h-1.5 w-1.5 shrink-0 mt-1 animate-pulse rounded-full bg-orange-500"></span>
        {#if visibleAttention.kind === "access_root_request"}
          <span class="truncate font-medium">Agent 正在等待目录访问授权。</span>
        {:else if visibleAttention.kind === "plan_approval"}
          <span class="truncate font-medium">Agent 正在等待你确认工作计划。</span>
        {:else if visibleAttention.kind === "user_input"}
          <span class="truncate font-medium">Agent 正在等待你补充信息。</span>
        {:else if visibleAttention.kind === "guardian_anomaly"}
          <div class="flex flex-col gap-0.5 min-w-0 flex-1">
            <div class="flex items-center gap-1.5 font-semibold">
              <span>运行异常 · {healthStatusLabel(healthStatus)}</span>
              {#if liveness && liveness !== "alive"}
                <span class="text-[9px] opacity-80 font-normal">({livenessLabel(liveness)})</span>
              {/if}
            </div>
            <p class="leading-4 opacity-90 text-[10px]">
              {healthReason ?? "运行状态发现异常，请检查状态或恢复。"}
            </p>
            {#if health?.stalledSince}
              <p class="text-[9px] opacity-75">停滞起始：{health.stalledSince}</p>
            {/if}
          </div>
        {:else}
          <span class="truncate font-medium">Agent 正在等待你的处理。</span>
        {/if}
      </div>
      {#if visibleAttention.kind === "guardian_anomaly" && visiblePendingInteractions.length === 0}
        <div class="flex items-center gap-1 shrink-0 mt-0.5">
          <a
            href="/chat/work?view=tasks"
            class="rounded-md bg-orange-500/20 px-2 py-0.5 text-[10px] font-semibold text-orange-700 dark:text-orange-300 hover:bg-orange-500/30 transition-colors"
          >
            查看任务 &rarr;
          </a>
          <button
            type="button"
            onclick={() => {
              dismissedAnomaly = true;
            }}
            class="rounded-md p-1 text-orange-700/70 dark:text-orange-300/70 hover:text-orange-700 dark:hover:text-orange-300 hover:bg-orange-500/20 transition-colors"
            title="忽略此提示"
            aria-label="忽略此提示"
          >
            <svg
              class="h-3.5 w-3.5"
              fill="none"
              viewBox="0 0 24 24"
              stroke-width="2"
              stroke="currentColor"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      {:else}
        <a
          href="/chat/work?view=inbox"
          class="shrink-0 mt-0.5 rounded-md bg-orange-500/20 px-2 py-0.5 text-[10px] font-semibold text-orange-700 dark:text-orange-300 hover:bg-orange-500/30 transition-colors"
        >
          去处理 &rarr;
        </a>
      {/if}
    </div>
  {:else if !isTerminal && currentActivity}
    <div
      class="mt-3 flex items-center gap-2 rounded-lg border border-amber-500/20 bg-amber-500/5 px-2.5 py-2 text-[10px] text-amber-700 dark:text-amber-300"
    >
      <span class="h-1.5 w-1.5 animate-pulse rounded-full bg-amber-500"></span>
      {#if currentActivity.detail}
        <span class="truncate font-medium">{currentActivity.detail}</span>
      {:else if currentActivity.toolName}
        <span class="truncate font-medium">正在{toolLabel(currentActivity.toolName)}</span>
      {:else if currentActivity.agentRole}
        <span class="truncate font-medium"
          >{friendlyRoleLabel(currentActivity.agentRole)} 正在执行</span
        >
      {:else}
        <span class="truncate font-medium">正在执行任务</span>
      {/if}
    </div>
  {:else if !isTerminal && resolvedSnapshot?.pendingApprovalCount}
    <div
      class="mt-3 flex items-center justify-between gap-2 rounded-lg border border-orange-500/30 bg-orange-500/10 px-2.5 py-2 text-[10px] text-orange-700 dark:text-orange-300"
    >
      <div class="flex items-center gap-2 min-w-0">
        <span class="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-orange-500"></span>
        {#if resolvedSnapshot.pendingAccessRoot}
          <span class="truncate font-medium">Agent 正在等待目录访问授权。</span>
        {:else if resolvedSnapshot.pendingApprovalCount === 1}
          <span class="truncate font-medium">Agent 正在等待你的授权。</span>
        {:else}
          <span class="truncate font-medium"
            >Agent 有 {resolvedSnapshot.pendingApprovalCount} 项操作等待你的授权。</span
          >
        {/if}
      </div>
      <a
        href="/chat/work?view=inbox"
        class="shrink-0 rounded-md bg-orange-500/20 px-2 py-0.5 text-[10px] font-semibold text-orange-700 dark:text-orange-300 hover:bg-orange-500/30 transition-colors"
      >
        去处理 &rarr;
      </a>
    </div>
  {:else if !isTerminal && resolvedSnapshot?.activeToolName}
    <div
      class="mt-3 flex items-center gap-2 rounded-lg border border-primary/20 bg-primary/5 px-2.5 py-2 text-[10px] text-primary"
    >
      <span class="h-1.5 w-1.5 animate-pulse rounded-full bg-primary"></span>
      <span class="truncate font-medium">正在{toolLabel(resolvedSnapshot.activeToolName)}</span>
    </div>
  {:else if !isTerminal && resolvedSnapshot?.pendingElicitation}
    <div
      class="mt-3 flex items-center justify-between gap-2 rounded-lg border border-orange-500/30 bg-orange-500/10 px-2.5 py-2 text-[10px] text-orange-700 dark:text-orange-300"
    >
      <div class="flex items-center gap-2 min-w-0">
        <span class="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-orange-500"></span>
        <span class="truncate font-medium">Agent 正在等待你补充信息。</span>
      </div>
      <a
        href="/chat/work?view=inbox"
        class="shrink-0 rounded-md bg-orange-500/20 px-2 py-0.5 text-[10px] font-semibold text-orange-700 dark:text-orange-300 hover:bg-orange-500/30 transition-colors"
      >
        去处理 &rarr;
      </a>
    </div>
  {/if}

  <!-- Runtime diagnostic alert card (when no active attention banner) -->
  {#if health && (healthStatus !== "healthy" || healthReason) && !isTerminal && !allTasksCompleted && !dismissedAnomaly && !visibleAttention && visiblePendingInteractions.length === 0}
    <div class="mt-3 rounded-lg border px-2.5 py-2 text-[10px] {healthStatusClass(healthStatus)}">
      <div class="flex items-center justify-between gap-2 font-semibold">
        <span class="flex items-center gap-1.5">
          <span
            class="h-1.5 w-1.5 animate-pulse rounded-full {healthStatus === 'stalled' ||
            healthStatus === 'needs_attention'
              ? 'bg-red-500'
              : 'bg-amber-500'}"
          ></span>
          <span>运行异常 · {healthStatusLabel(healthStatus)}</span>
        </span>
        <div class="flex items-center gap-1.5">
          {#if liveness && liveness !== "alive"}
            <span class="text-[9px] opacity-80 font-normal">({livenessLabel(liveness)})</span>
          {/if}
          <button
            type="button"
            onclick={() => {
              dismissedAnomaly = true;
            }}
            class="rounded-md p-0.5 opacity-70 hover:opacity-100 transition-opacity"
            title="忽略此提示"
            aria-label="忽略此提示"
          >
            <svg
              class="h-3.5 w-3.5"
              fill="none"
              viewBox="0 0 24 24"
              stroke-width="2"
              stroke="currentColor"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>
      {#if healthReason}
        <p class="mt-1 leading-4 opacity-90">{healthReason}</p>
      {/if}
      {#if health.stalledSince}
        <p class="mt-0.5 text-[9px] opacity-75">停滞起始：{health.stalledSince}</p>
      {/if}
    </div>
  {/if}

  <!-- Subagent summary is intentionally near the top of the inspector so a
       long step list cannot hide the evidence that child runs started. -->
  {#if showSubagentSummary}
    <div
      class="pointer-events-auto mt-3 rounded-lg border border-primary/20 bg-primary/[0.04] px-2.5 py-2"
      role="status"
      aria-live="polite"
    >
      <button
        type="button"
        class="flex w-full cursor-pointer items-center justify-between gap-2 text-left text-[10px]"
        aria-expanded={teamSectionOpen}
        aria-controls="work-subagent-details"
        onclick={() => (teamSectionOpen = !teamSectionOpen)}
      >
        <span class="flex min-w-0 items-center gap-1.5 font-semibold text-foreground">
          <svg
            class="h-3.5 w-3.5 shrink-0 text-primary"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            aria-hidden="true"
          >
            <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
            <circle cx="9" cy="7" r="4" />
            <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
            <path d="M16 3.13a4 4 0 0 1 0 7.75" />
          </svg>
          <span>团队协作</span>
        </span>
        <span class="flex shrink-0 items-center gap-1.5 text-[9px] font-medium text-primary">
          {#if subagents.length > 0}
            {subagentsHeaderSummary}
          {:else if subagentToolActive}
            等待子 Agent 注册
          {:else}
            未使用子 Agent
          {/if}
          <span
            class="text-muted-foreground transition-transform {teamSectionOpen ? 'rotate-90' : ''}"
            aria-hidden="true">›</span
          >
        </span>
      </button>
      <p class="mt-1 text-[10px] leading-4 text-muted-foreground">
        {#if subagents.length === 0 && subagentToolActive}
          正在启动子 Agent，注册成功后会显示每个子任务的运行状态。
        {:else if subagents.length === 0}
          本次运行没有记录到子 Agent，父任务独立执行。
        {:else if runningSubagentsCount > 0}
          {runningSubagentsCount} 个子 Agent 已启动并运行中，状态会持续同步。
        {:else if completedSubagentsCount === subagents.length}
          所有已启动的子 Agent 都已完成并回传结果。
        {:else}
          子 Agent 生命周期已记录，请展开下方“团队协作”查看失败或中断详情。
        {/if}
      </p>
    </div>
  {/if}

  <!-- Goal Display with Goal Card -->
  {#if shouldShowGoalCard}
    <div class="mt-4">
      <WorkGoalCard
        goal={goalSpec}
        statementFallback={goal}
        loading={verifyingGoal}
        onVerify={handleVerifyGoal}
        onTriggerRepair={handleTriggerRepair}
        compact
      />
    </div>
  {/if}

  <!-- Checkpoint Display -->
  {#if checkpoint}
    <div class="mt-3 rounded-lg border border-border/40 bg-card/30 px-2.5 py-1.5">
      <div class="flex items-center justify-between gap-2">
        <div class="flex min-w-0 items-center gap-1.5 text-[9px] font-medium text-muted-foreground">
          <svg
            class="h-3 w-3 shrink-0 text-primary/70"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />
          </svg>
          <span>最新检查点</span>
        </div>
        <button
          type="button"
          class="shrink-0 rounded px-1.5 py-0.5 text-[9px] font-medium text-primary transition-colors hover:bg-primary/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
          aria-label="查看完整检查点"
          onclick={() => (checkpointDetailsOpen = true)}
        >
          查看完整检查点
        </button>
      </div>
      <div class="relative mt-1">
        <p
          class="line-clamp-8 max-h-32 overflow-hidden break-words text-[10px] leading-4 text-muted-foreground"
        >
          {checkpoint.summary}
        </p>
        {#if checkpointSummaryOverflowing}
          <div
            class="pointer-events-none absolute inset-x-0 bottom-0 h-5 bg-gradient-to-t from-card/90 to-transparent"
            aria-hidden="true"
          ></div>
        {/if}
      </div>
    </div>

    <Modal bind:open={checkpointDetailsOpen} title="最新检查点">
      <div class="space-y-3">
        <div class="flex items-center justify-between gap-3 text-[11px] text-muted-foreground">
          <span>完整检查点内容</span>
          <button
            type="button"
            class="rounded-md px-2 py-1 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
            onclick={() => (checkpointDetailsOpen = false)}
          >
            关闭
          </button>
        </div>
        <pre
          class="max-h-[60vh] overflow-auto whitespace-pre-wrap break-words rounded-lg border border-border/60 bg-muted/30 p-3 font-mono text-[11px] leading-5 text-foreground/90 select-text">{checkpoint.summary}</pre>
      </div>
    </Modal>
  {/if}

  <!-- Acceptance Criteria & Deliverables (Goal → Acceptance → Delivery) -->
  {#if acceptanceCriteria.length > 0}
    <div class="mt-3 rounded-lg border border-border/60 bg-card/40 p-2.5 space-y-2">
      <div class="flex items-center justify-between text-[10px]">
        <span class="font-semibold text-foreground flex items-center gap-1.5">
          <svg
            class="h-3.5 w-3.5 text-primary"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M9 11l3 3L22 4" />
            <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
          </svg>
          验收标准与交付物
        </span>
        <span class="rounded bg-muted px-1.5 py-0.2 font-medium text-muted-foreground">
          {criteriaSummaryBadge}
        </span>
      </div>

      <div class="space-y-1.5">
        {#each acceptanceCriteria as criterion (criterion.path)}
          <div
            class="flex items-start justify-between gap-2 text-[10px] rounded bg-background/50 p-1.5 border border-border/30"
          >
            <div class="min-w-0 flex-1">
              <div class="font-mono font-medium text-foreground truncate flex items-center gap-1.5">
                <span class="truncate">{criterion.title}</span>
                {#if !criterion.required}
                  <span
                    class="shrink-0 rounded bg-muted/80 px-1 py-0.2 text-[8px] text-muted-foreground font-sans font-normal"
                    >可选</span
                  >
                {/if}
              </div>
              {#if criterion.message}
                <div class="text-[9px] text-muted-foreground truncate">{criterion.message}</div>
              {/if}
            </div>
            <span
              class="shrink-0 rounded border px-1.5 py-0.2 text-[9px] font-semibold {checkStatusClass(
                criterion.status,
              )}"
            >
              {checkStatusLabel(criterion.status)}
            </span>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Prominent Phase Banners (WaitingDelivery / Recoverable / Failed) -->
  {#if phase === "awaiting_delivery"}
    <div class="mt-3 rounded-xl border border-cyan-500/30 bg-cyan-500/10 p-3 space-y-2">
      <div class="flex items-center justify-between">
        <span
          class="text-xs font-semibold text-cyan-800 dark:text-cyan-200 flex items-center gap-1.5"
        >
          <span class="h-2 w-2 rounded-full bg-cyan-500 animate-pulse"></span>
          等待交付物验收
        </span>
        <span
          class="text-[10px] rounded bg-cyan-500/20 text-cyan-700 dark:text-cyan-300 px-1.5 py-0.5 font-semibold"
        >
          {deliveryProgressLabel}
        </span>
      </div>
      <p class="text-[11px] text-cyan-700 dark:text-cyan-300 leading-relaxed">
        当前任务步骤已执行完毕，正在等待交付物验收。请在成果列表中核验并完成交付。
      </p>
    </div>
  {:else if phase === "recoverable"}
    <div class="mt-3 rounded-xl border border-amber-500/30 bg-amber-500/10 p-3 space-y-1.5">
      <div
        class="text-xs font-semibold text-amber-700 dark:text-amber-300 flex items-center gap-1.5"
      >
        <span class="h-2 w-2 rounded-full bg-amber-500 animate-pulse"></span>
        任务已暂停（可恢复）
      </div>
      <p class="text-[11px] text-amber-700 dark:text-amber-300 leading-relaxed">
        任务执行遇到中断，请在顶部恢复卡片中选择恢复或重试。
      </p>
    </div>
  {:else if phase === "failed" && resolvedSnapshot?.error}
    <div class="mt-3 rounded-xl border border-red-500/30 bg-red-500/10 p-3 space-y-1.5">
      <div class="text-xs font-semibold text-red-700 dark:text-red-300 flex items-center gap-1.5">
        <svg
          class="h-3.5 w-3.5 shrink-0"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <circle cx="12" cy="12" r="10" />
          <line x1="12" y1="8" x2="12" y2="12" />
          <line x1="12" y1="16" x2="12.01" y2="16" />
        </svg>
        任务执行中断
      </div>
      <p
        class="text-[11px] text-red-600 dark:text-red-400 whitespace-pre-wrap break-all leading-relaxed"
      >
        {resolvedSnapshot.error}
      </p>
    </div>
  {/if}

  <!-- Steps List -->
  {#if tasks.length > 0}
    <div class="mt-4">
      <TaskChecklistCard {tasks} />
    </div>
  {/if}

  {#if piTodoSections.some((section) => section.tasks.length > 0)}
    <div class="mt-4">
      <TaskChecklistCard tasks={[]} sections={piTodoSections} title="Pi Todo" />
    </div>
  {/if}

  <!-- Tool Operations Summary -->
  {#if toolSummary && toolSummary.started > 0}
    <div
      class="mt-4 flex items-center justify-between text-[10px] text-muted-foreground border-t border-border/40 pt-2.5"
    >
      <span>工具执行</span>
      <span>
        {toolSummary.completed} 成功 · {toolSummary.failed} 失败
        {#if toolSummary.running > 0}
          · <span class="text-amber-600 dark:text-amber-400 font-medium"
            >{toolSummary.running} 运行中</span
          >
        {/if}
      </span>
    </div>
  {/if}

  <!-- Team Collaboration / Subagents Section -->
  {#if subagents.length > 0 && teamSectionOpen}
    <div id="work-subagent-details" class="mt-4 border-t border-border/50 pt-3">
      <div class="flex items-center justify-between gap-2 text-[10px]">
        <span class="font-medium text-foreground flex items-center gap-1.5">
          <svg
            class="h-3.5 w-3.5 text-primary"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
            <circle cx="9" cy="7" r="4" />
            <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
            <path d="M16 3.13a4 4 0 0 1 0 7.75" />
          </svg>
          团队协作
        </span>
        <span class="text-muted-foreground text-[9px]">
          {subagentsHeaderSummary}
        </span>
      </div>

      <div class="mt-2 space-y-1.5">
        {#each subagents as sa (sa.id)}
          {@const summaryText = formatSubagentSummary(sa.resultSummary || sa.task || "")}
          <details
            class="group rounded-lg border border-border/50 bg-card/60 transition-colors hover:border-border/80"
          >
            <summary
              class="w-full cursor-pointer list-none space-y-1 px-2.5 py-2 text-left text-[10px] select-none [&::-webkit-details-marker]:hidden"
            >
              <div class="flex items-center justify-between gap-1.5">
                <div class="flex items-center gap-1.5 font-medium">
                  <span
                    class="h-1.5 w-1.5 rounded-full {sa.status === 'running'
                      ? 'bg-amber-500 animate-pulse'
                      : sa.status === 'failed'
                        ? 'bg-red-500'
                        : sa.status === 'interrupted'
                          ? 'bg-muted-foreground'
                          : 'bg-emerald-500'}"
                  ></span>
                  <span class="tracking-wide text-[10px] font-semibold text-foreground/90">
                    {friendlyRoleLabel(sa.role)}
                  </span>
                </div>
                <div class="flex items-center gap-1.5">
                  <span
                    class="text-[9px] {sa.status === 'running'
                      ? 'text-amber-600 dark:text-amber-400 font-medium'
                      : 'text-muted-foreground'}"
                  >
                    {sa.statusText}
                  </span>
                  <svg
                    class="h-3 w-3 text-muted-foreground/60 transition-transform group-open:rotate-90"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    aria-hidden="true"><path d="m9 18 6-6-6-6" /></svg
                  >
                </div>
              </div>
              {#if summaryText}
                <p class="text-[10px] text-muted-foreground line-clamp-2 leading-relaxed">
                  {summaryText}
                </p>
              {/if}
            </summary>

            <div
              class="border-t border-border/40 bg-muted/20 px-2.5 py-2 text-[10px] space-y-2 rounded-b-lg"
            >
              {#if sa.task && sa.task !== sa.resultSummary}
                <div>
                  <div class="text-[9px] font-medium text-muted-foreground mb-0.5">指派任务</div>
                  <div
                    class="text-foreground/90 leading-relaxed break-words bg-background/60 p-1.5 rounded border border-border/30"
                  >
                    {sa.task}
                  </div>
                </div>
              {/if}

              {#if sa.error}
                <div
                  class="rounded border border-red-500/20 bg-red-500/5 p-1.5 text-red-600 dark:text-red-300"
                >
                  <div class="text-[9px] font-medium mb-0.5">执行失败原因</div>
                  <div class="font-mono text-[9px] leading-relaxed break-words">{sa.error}</div>
                </div>
              {/if}

              {#if sa.resultSummary}
                <div>
                  <div class="text-[9px] font-medium text-muted-foreground mb-0.5">
                    执行汇报 / 输出
                  </div>
                  <pre
                    class="max-h-48 overflow-auto rounded bg-background/80 p-2 font-mono text-[9px] leading-relaxed text-foreground/90 border border-border/40 whitespace-pre-wrap break-words">{sa.resultSummary}</pre>
                </div>
              {:else if sa.status === "running"}
                <div class="flex items-center gap-1.5 text-amber-600 dark:text-amber-400 py-1">
                  <span class="h-2 w-2 animate-ping rounded-full bg-amber-500"></span>
                  <span>子任务正在独立执行调研，完成后将在此同步结果…</span>
                </div>
              {/if}

              <div
                class="pt-0.5 flex items-center justify-between text-[10px] text-muted-foreground/75"
              >
                <span class="font-medium">{friendlyRoleLabel(sa.role)}</span>
                <span class="flex items-center gap-1.5">
                  <span class="max-w-40 truncate font-mono text-[8px]" title={sa.agentId}
                    >ID: {sa.agentId}</span
                  >
                  <span
                    class="text-[9px] px-1.5 py-0.2 rounded font-semibold {sa.status === 'running'
                      ? 'bg-blue-500/10 text-blue-600 dark:text-blue-400'
                      : sa.status === 'completed'
                        ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
                        : sa.status === 'failed'
                          ? 'bg-red-500/10 text-red-600 dark:text-red-400'
                          : 'bg-muted text-muted-foreground'}"
                  >
                    {subagentStatusLabel(sa.status)}
                  </span>
                </span>
              </div>
            </div>
          </details>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Artifacts Summary -->
  {#if showArtifactSummary}
    <div class="mt-4 border-t border-border/50 pt-3">
      <div class="flex items-center justify-between gap-3">
        <div>
          <div class="text-[10px] font-medium text-muted-foreground">成果交付</div>
          <div class="mt-1 text-xs font-semibold text-foreground">
            {deliveredArtifacts}/{artifacts.length} 已交付
            {#if validatedArtifacts > deliveredArtifacts}
              <span class="text-[11px] font-normal text-muted-foreground ml-1">
                ({validatedArtifacts} 个已验证)
              </span>
            {/if}
          </div>
        </div>
        {#if artifacts.length > 0}
          <div class="text-right text-[10px] text-muted-foreground">
            {#if deliveredArtifacts === artifacts.length}
              全部成果已完成验证与交付
            {:else}
              {artifacts.length - deliveredArtifacts} 个成果就绪/验证中
            {/if}
          </div>
        {:else}
          <span class="text-[10px] text-muted-foreground">尚无 Artifact</span>
        {/if}
      </div>
      {#if resolvedSnapshot?.error}
        <p
          class="mt-2 rounded-lg bg-red-500/5 px-2.5 py-2 text-[10px] leading-4 text-red-700 dark:text-red-300"
        >
          {resolvedSnapshot.error}
        </p>
      {/if}
    </div>
  {/if}
</section>
