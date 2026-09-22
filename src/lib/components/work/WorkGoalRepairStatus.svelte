<script lang="ts">
  import type { GoalSpec } from "$lib/types/work";

  interface Props {
    goal: GoalSpec;
    onRetryVerify?: () => void;
    onTriggerRepair?: () => void;
    loading?: boolean;
  }

  let { goal, onRetryVerify, onTriggerRepair, loading = false }: Props = $props();

  const isRepairing = $derived(
    goal.repairRound > 0 && goal.status !== "passed" && goal.status !== "failed",
  );
  const isFailed = $derived(goal.status === "failed" || goal.repairRound >= goal.maxRepairRounds);
</script>

{#if isRepairing || isFailed || goal.repairInstruction}
  <div
    class="rounded-lg border {isFailed
      ? 'border-red-500/30 bg-red-500/5'
      : 'border-amber-500/30 bg-amber-500/5'} p-2.5 text-xs"
  >
    <div class="flex items-center justify-between gap-2">
      <div
        class="flex items-center gap-1.5 font-medium {isFailed
          ? 'text-red-600 dark:text-red-400'
          : 'text-amber-600 dark:text-amber-400'}"
      >
        {#if !isFailed}
          <span class="h-2 w-2 rounded-full bg-amber-500 animate-pulse"></span>
          <span>正在自动修复（第 {goal.repairRound}/{goal.maxRepairRounds} 轮）…</span>
        {:else}
          <span class="h-2 w-2 rounded-full bg-red-500"></span>
          <span>已达到最大修复轮次（{goal.maxRepairRounds} 轮），需人工确认</span>
        {/if}
      </div>

      <div class="flex items-center gap-1.5">
        {#if onRetryVerify}
          <button
            type="button"
            class="rounded border border-border px-2 py-1 text-[11px] font-medium text-foreground hover:bg-accent disabled:opacity-50"
            disabled={loading}
            onclick={() => onRetryVerify?.()}
          >
            重新验收
          </button>
        {/if}
        {#if onTriggerRepair && !isFailed}
          <button
            type="button"
            class="rounded border border-amber-500/40 bg-amber-500/10 px-2 py-1 text-[11px] font-medium text-amber-700 dark:text-amber-300 hover:bg-amber-500/20 disabled:opacity-50"
            disabled={loading}
            onclick={() => onTriggerRepair?.()}
          >
            继续修复
          </button>
        {/if}
      </div>
    </div>

    {#if goal.repairInstruction}
      <div
        class="mt-2 whitespace-pre-wrap rounded bg-card/40 p-2 font-mono text-[11px] text-muted-foreground border border-border/40"
      >
        {goal.repairInstruction}
      </div>
    {/if}
  </div>
{/if}
