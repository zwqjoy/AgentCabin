<script lang="ts">
  import {
    formatAutomationRunStatus,
    formatAutomationTrigger,
    formatRunDuration,
  } from "$lib/utils/work-automation";
  import type { WorkTaskRunSummary } from "$lib/types/work";

  interface Props {
    taskTitle: string;
    runs: WorkTaskRunSummary[];
    loading?: boolean;
    onClose: () => void;
    onOpenRun?: (workspaceId: string, runId: string) => void;
    onRetry?: (run: WorkTaskRunSummary) => void | Promise<void>;
  }

  let { taskTitle, runs, loading = false, onClose, onOpenRun, onRetry }: Props = $props();

  function goalSummary(run: WorkTaskRunSummary): { text: string; cls: string } {
    if (run.goalPassed) {
      return { text: "✓ 已达成", cls: "text-emerald-600 dark:text-emerald-400" };
    }
    switch (run.goalStatus) {
      case "not_applicable":
        return { text: "— 不适用", cls: "text-slate-400 dark:text-slate-500" };
      case "failed":
        return { text: "✗ 未达成", cls: "text-red-600 dark:text-red-400" };
      case "insufficient_evidence":
        return { text: "△ 证据不足", cls: "text-amber-600 dark:text-amber-400" };
      default:
        return { text: "-", cls: "text-muted-foreground" };
    }
  }
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-end bg-black/30 backdrop-blur-[2px]"
  role="dialog"
  aria-modal="true"
  aria-labelledby="history-title"
  tabindex="-1"
  onclick={(e) => e.target === e.currentTarget && onClose()}
>
  <div
    class="flex h-full w-full max-w-xl flex-col border-l border-border bg-card shadow-2xl animate-in slide-in-from-right duration-200"
  >
    <!-- Header -->
    <div class="flex items-center justify-between border-b border-border/80 px-5 py-4">
      <div class="min-w-0 flex-1">
        <h2 id="history-title" class="text-sm font-semibold text-foreground truncate">
          执行历史流水
        </h2>
        <p class="mt-0.5 text-xs text-muted-foreground truncate" title={taskTitle}>
          任务：{taskTitle}
        </p>
      </div>
      <button
        type="button"
        class="rounded-lg p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
        aria-label="关闭"
        onclick={onClose}
      >
        ✕
      </button>
    </div>

    <!-- Content List -->
    <div class="flex-1 overflow-y-auto p-5 space-y-3">
      {#if loading}
        <div class="flex h-40 items-center justify-center text-xs text-muted-foreground">
          <span class="animate-pulse">正在加载执行记录…</span>
        </div>
      {:else if runs.length === 0}
        <div
          class="flex h-48 flex-col items-center justify-center rounded-xl border border-dashed border-border/70 p-6 text-center text-xs text-muted-foreground"
        >
          <span class="text-xl">⏱</span>
          <span class="mt-2 font-medium">暂无历史执行记录</span>
          <span class="mt-1 text-[11px] opacity-75">
            任务将在到达定时触发时间或手动点击「立即执行」后产生运行记录。
          </span>
        </div>
      {:else}
        {#each runs as run (run.runId)}
          {@const statusMeta = formatAutomationRunStatus(run.status)}
          {@const triggerMeta = formatAutomationTrigger(run.trigger)}
          {@const goalMeta = goalSummary(run)}
          <div
            class="rounded-xl border border-border/60 bg-card/60 p-3.5 transition-all hover:border-border hover:bg-card/90"
          >
            <div class="flex items-start justify-between gap-3">
              <div class="flex items-center gap-2">
                <span class="h-2 w-2 rounded-full {statusMeta.dotClass}"></span>
                <span
                  class="rounded-full border px-2 py-0.5 text-[10px] font-semibold {statusMeta.badgeClass}"
                >
                  {statusMeta.label}
                </span>
                <span
                  class="rounded border px-1.5 py-0.5 text-[9px] font-medium {triggerMeta.badgeClass}"
                >
                  {triggerMeta.label}
                </span>
              </div>

              <div class="text-right text-[10px] text-muted-foreground font-mono">
                {run.startedAt ? run.startedAt.slice(0, 19).replace("T", " ") : "-"}
              </div>
            </div>

            <!-- Run metadata summary -->
            <div
              class="mt-2.5 grid grid-cols-3 gap-2 rounded-lg bg-muted/30 p-2 text-center text-[11px]"
            >
              <div>
                <span class="text-[9px] text-muted-foreground block">执行耗时</span>
                <span class="font-semibold text-foreground font-mono"
                  >{formatRunDuration(run.durationMs)}</span
                >
              </div>
              <div>
                <span class="text-[9px] text-muted-foreground block">交付成果物</span>
                <span class="font-semibold text-foreground">{run.deliveredArtifactsCount} 件</span>
              </div>
              <div>
                <span class="text-[9px] text-muted-foreground block">目标达成</span>
                <span class="font-semibold {goalMeta.cls}">{goalMeta.text}</span>
              </div>
            </div>

            {#if run.errorMessage}
              <div
                class="mt-2 rounded bg-red-500/10 p-2 text-[11px] text-red-600 dark:text-red-400 border border-red-500/20"
              >
                {run.errorMessage}
              </div>
            {/if}

            <div
              class="mt-3 flex items-center justify-between border-t border-border/40 pt-2 text-[11px]"
            >
              <span class="font-mono text-[10px] text-muted-foreground/70 truncate">
                记录编号: {run.runId.slice(0, 16)}…
              </span>

              <div class="flex items-center gap-3">
                {#if onRetry && (run.status === "failed" || run.status === "cancelled")}
                  <button
                    type="button"
                    class="font-semibold text-orange-600 hover:underline dark:text-orange-400"
                    onclick={() => void onRetry?.(run)}
                  >
                    重试
                  </button>
                {/if}
                {#if onOpenRun}
                  <button
                    type="button"
                    class="font-semibold text-primary hover:underline"
                    onclick={() => onOpenRun(run.workspaceId, run.runId)}
                  >
                    查看对话与回执 &rarr;
                  </button>
                {/if}
              </div>
            </div>
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>
