<script lang="ts">
  import type {
    WorkArtifactCheckStatus,
    WorkRecoveryAction,
    WorkRunRecovery,
  } from "$lib/types/work";

  interface Props {
    recovery: WorkRunRecovery;
    readOnly?: boolean;
    onAction: (action: WorkRecoveryAction, subagentId?: string) => Promise<void>;
  }

  let { recovery, readOnly = false, onAction }: Props = $props();
  let busy = $state<WorkRecoveryAction | null>(null);
  let error = $state("");

  function actionLabel(action: WorkRecoveryAction): string {
    switch (action) {
      case "continue":
        return "确认已完成，继续";
      case "retry":
        return "未完成，重新执行";
      case "verify":
        return "验证交付物";
      case "retry_subagent":
        return "重试中断子任务";
      case "from_scratch":
        return "从头开始";
      case "cancel":
        return "取消 Run";
    }
  }

  function checkClass(status: WorkArtifactCheckStatus): string {
    if (status === "satisfied") return "text-emerald-600 dark:text-emerald-300";
    if (status === "invalid") return "text-red-600 dark:text-red-300";
    return "text-amber-600 dark:text-amber-300";
  }

  function checkLabel(status: WorkArtifactCheckStatus): string {
    if (status === "satisfied") return "已满足";
    if (status === "invalid") return "需修复";
    return "缺失";
  }

  async function handleAction(action: WorkRecoveryAction) {
    if (busy || readOnly) return;
    busy = action;
    error = "";
    try {
      await onAction(action, recovery.interruptedSubagentIds[0]);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = null;
    }
  }
</script>

<section
  class="rounded-xl border border-amber-500/35 bg-amber-500/[0.06] p-4 shadow-sm"
  aria-label="Work Run 恢复"
>
  <div class="flex items-start gap-3">
    <div
      class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-amber-500/15 text-amber-600 dark:text-amber-300"
    >
      <svg viewBox="0 0 24 24" class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="1.8">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M12 8v4l2.5 2.5M21 12a9 9 0 1 1-2.64-6.36M21 4v5h-5"
        />
      </svg>
    </div>
    <div class="min-w-0 flex-1">
      <div class="flex flex-wrap items-center gap-2">
        <span
          class="rounded bg-amber-500/15 px-1.5 py-0.5 text-[10px] font-semibold text-amber-700 dark:text-amber-300"
        >
          {recovery.status === "waiting_delivery" ? "等待交付物验收" : "执行中断，可恢复"}
        </span>
        <h3 class="text-sm font-semibold text-foreground">
          {recovery.stepTitle || recovery.toolName || "Work Run"}
        </h3>
      </div>
      <p class="mt-1 text-[11px] text-muted-foreground">
        中断时间：{new Date(recovery.detectedAt).toLocaleString("zh-CN")}
      </p>
    </div>
  </div>

  <div class="mt-3 space-y-2 text-xs leading-5">
    <p class="text-foreground">{recovery.reason}</p>
    <p class="text-muted-foreground">建议：{recovery.recommendation}</p>
    {#if recovery.sideEffectClass === "external_mutating"}
      <p
        class="rounded-lg border border-red-500/25 bg-red-500/[0.06] px-2.5 py-2 text-red-700 dark:text-red-300"
      >
        该操作可能已经产生外部副作用。请先确认“已完成”还是“未完成”，系统不会盲目重放旧调用。
      </p>
    {/if}
  </div>

  {#if recovery.acceptance && recovery.acceptance.checks.length > 0}
    <div class="mt-3 rounded-lg border border-border/60 bg-background/45 p-2.5">
      <div class="text-[11px] font-semibold text-foreground">
        当前 Run 的交付物（{recovery.acceptance.satisfiedCount}/{recovery.acceptance
          .requiredCount}）
      </div>
      <div class="mt-1.5 space-y-1">
        {#each recovery.acceptance.checks as check (check.requirement.path)}
          <div class="flex items-start justify-between gap-2 text-[10px]">
            <span class="min-w-0 break-all font-mono text-muted-foreground"
              >{check.requirement.path}</span
            >
            <span class="shrink-0 font-semibold {checkClass(check.status)}"
              >{checkLabel(check.status)}</span
            >
          </div>
          {#if check.status !== "satisfied"}
            <div class="text-[10px] text-muted-foreground">{check.message}</div>
          {/if}
        {/each}
      </div>
    </div>
  {/if}

  {#if error}
    <p
      class="mt-3 rounded-lg border border-red-500/25 bg-red-500/[0.06] px-2.5 py-2 text-xs text-red-600 dark:text-red-300"
      role="alert"
    >
      {error}
    </p>
  {/if}

  <div class="mt-3 flex flex-wrap justify-end gap-2 border-t border-amber-500/20 pt-3">
    {#each recovery.availableActions as action (action)}
      <button
        type="button"
        disabled={readOnly ||
          busy !== null ||
          (action === "retry_subagent" && recovery.interruptedSubagentIds.length === 0)}
        class="rounded-lg border px-3 py-1.5 text-[11px] font-semibold transition-colors disabled:opacity-50 {action ===
        'cancel'
          ? 'border-border/70 text-muted-foreground hover:bg-accent'
          : action === 'from_scratch'
            ? 'border-orange-500/35 bg-orange-500/10 text-orange-700 hover:bg-orange-500/20 dark:text-orange-300'
            : 'border-primary/35 bg-primary/10 text-primary hover:bg-primary/20'}"
        onclick={() => void handleAction(action)}
      >
        {busy === action ? "处理中…" : actionLabel(action)}
      </button>
    {/each}
  </div>
</section>
