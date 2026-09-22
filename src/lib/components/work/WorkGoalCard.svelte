<script lang="ts">
  import WorkAcceptanceItem from "./WorkAcceptanceItem.svelte";
  import WorkGoalRepairStatus from "./WorkGoalRepairStatus.svelte";
  import { computeGoalProgress, formatGoalStatus } from "$lib/utils/work-goal";
  import type { GoalSpec } from "$lib/types/work";

  interface Props {
    goal: GoalSpec | null;
    statementFallback?: string | null;
    loading?: boolean;
    onVerify?: () => void;
    onTriggerRepair?: () => void;
    compact?: boolean;
    initiallyExpanded?: boolean;
  }

  let {
    goal,
    statementFallback = "",
    loading = false,
    onVerify,
    onTriggerRepair,
    compact = false,
    initiallyExpanded = true,
  }: Props = $props();

  let expanded = $state(initiallyExpanded);

  const statement = $derived(goal?.statement || statementFallback || "执行工作任务");
  const criteria = $derived(goal?.criteria ?? []);
  const progress = $derived(computeGoalProgress(criteria));
  const statusMeta = $derived(formatGoalStatus(goal?.status ?? "pending"));
  const hasAcceptanceContract = $derived(criteria.length > 0);
</script>

<div
  class="work-goal-card rounded-xl border border-border/70 bg-card/50 backdrop-blur-sm transition-all {compact
    ? 'p-3'
    : 'p-3.5'}"
>
  <div class="flex items-start justify-between gap-3">
    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-2">
        <span class="flex h-2 w-2 rounded-full {statusMeta.dotClass}"></span>
        <span class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
          {hasAcceptanceContract ? "目标与验收契约" : "任务目标"}
        </span>
        {#if hasAcceptanceContract}
          <span
            class="rounded-full border px-2 py-0.5 text-[10px] font-semibold {statusMeta.badgeClass}"
          >
            {statusMeta.label}
          </span>
        {/if}
      </div>

      <h3
        class="mt-1 text-sm font-semibold text-foreground leading-snug truncate"
        title={statement}
      >
        {statement}
      </h3>
    </div>

    {#if criteria.length > 0}
      <div class="flex shrink-0 items-center gap-2">
        <div class="text-right">
          <p class="text-xs font-bold text-foreground">
            {progress.passed}
            <span class="text-muted-foreground font-normal">/ {progress.total}</span>
          </p>
          <p class="text-[10px] text-muted-foreground">
            {progress.percent}% 完成
          </p>
        </div>

        <button
          type="button"
          class="rounded-lg p-1 text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
          aria-expanded={expanded}
          title={expanded ? "收起目标详情" : "展开目标详情"}
          onclick={() => (expanded = !expanded)}
        >
          <span class="inline-block transition-transform {expanded ? 'rotate-90' : ''}">›</span>
        </button>
      </div>
    {/if}
  </div>

  {#if criteria.length > 0}
    <!-- 进度条 -->
    <div class="mt-2.5 h-1.5 w-full overflow-hidden rounded-full bg-muted/60">
      <div
        class="h-full rounded-full transition-all duration-300 {progress.percent === 100
          ? 'bg-emerald-500'
          : progress.failed > 0
            ? 'bg-red-500'
            : 'bg-primary'}"
        style="width: {progress.percent}%"
      ></div>
    </div>
  {/if}

  {#if expanded && criteria.length > 0}
    <div class="mt-3 space-y-2 border-t border-border/40 pt-3">
      <div class="flex items-center justify-between">
        <span class="text-[11px] font-medium text-muted-foreground">
          验收标准（系统机器证明）
        </span>
        {#if onVerify}
          <button
            type="button"
            class="text-[11px] text-primary hover:underline disabled:opacity-50"
            disabled={loading}
            onclick={() => onVerify?.()}
          >
            {loading ? "校验中…" : "触发系统核验"}
          </button>
        {/if}
      </div>

      <div class="space-y-1.5">
        {#each criteria as criterion, idx (criterion.id)}
          <WorkAcceptanceItem {criterion} index={idx} />
        {/each}
      </div>
    </div>
  {/if}

  {#if goal}
    <div class="mt-3">
      <WorkGoalRepairStatus {goal} onRetryVerify={onVerify} {onTriggerRepair} {loading} />
    </div>
  {/if}
</div>
