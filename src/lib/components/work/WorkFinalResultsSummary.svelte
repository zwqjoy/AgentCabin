<script lang="ts">
  import type { WorkArtifactSummary } from "$lib/types/work";
  import WorkArtifactCard from "./WorkArtifactCard.svelte";
  import { formatDurationMs } from "$lib/utils/work-result";

  interface Props {
    projection?: any;
    runId?: string;
    status?: string;
    artifacts: WorkArtifactSummary[];
    onPreview: (artifact: WorkArtifactSummary) => void;
    onOpen?: (artifact: WorkArtifactSummary) => void;
    onExport?: (artifact: WorkArtifactSummary) => void;
    onDeliver?: (artifact: WorkArtifactSummary) => void;
    onShowDetails?: (artifact: WorkArtifactSummary) => void;
    workspaceRoot?: string;
  }

  let {
    projection,
    runId,
    status = "completed",
    artifacts = [],
    onPreview,
    onOpen,
    onExport,
    onDeliver,
    onShowDetails,
    workspaceRoot: _workspaceRoot,
  }: Props = $props();

  let isCompleted = $derived(
    status === "completed" ||
      projection?.runStatus === "completed" ||
      projection?.phase === "completed",
  );
  let isPartial = $derived(
    !isCompleted &&
      (status === "failed" ||
        status === "stopped" ||
        status === "recoverable" ||
        projection?.runStatus === "failed" ||
        projection?.runStatus === "stopped" ||
        projection?.runStatus === "recoverable"),
  );

  // Filter only artifacts belonging to this run
  let runArtifacts = $derived.by(() => {
    const targetId =
      runId ||
      projection?.work_run_id ||
      projection?.workRunId ||
      projection?.run_id ||
      projection?.runId;
    if (!targetId) return artifacts;
    const filtered = artifacts.filter(
      (a) => a.runId === targetId || (a as any).run_id === targetId,
    );
    return filtered.length > 0 ? filtered : artifacts;
  });

  let deliveredCount = $derived(runArtifacts.filter((a) => a.status === "delivered").length);

  let totalSources = $derived.by(() => {
    const set = new Set<string>();
    for (const a of runArtifacts) {
      for (const s of a.sources || []) {
        set.add(
          s.resultDigest ||
            (s as any).result_digest ||
            s.sourceToolCallId ||
            (s as any).source_tool_call_id,
        );
      }
    }
    return set.size;
  });

  let formattedDuration = $derived.by(() => {
    if (projection?.durationMs != null && projection.durationMs > 0) {
      return formatDurationMs(projection.durationMs);
    }
    const started = projection?.started_at || projection?.startedAt;
    const completed =
      projection?.completed_at ||
      projection?.completedAt ||
      projection?.finished_at ||
      projection?.finishedAt ||
      projection?.ended_at ||
      projection?.endedAt;
    if (started && completed) {
      const start = new Date(started).getTime();
      const end = new Date(completed).getTime();
      if (!isNaN(start) && !isNaN(end) && end >= start) {
        return formatDurationMs(end - start);
      }
    }
    return null;
  });
</script>

<div
  class="rounded-xl border p-4 shadow-sm {isPartial
    ? 'border-amber-500/30 bg-amber-500/5'
    : 'border-emerald-500/30 bg-emerald-500/5'}"
>
  <!-- Header -->
  <div
    class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between pb-3 border-b {isPartial
      ? 'border-amber-500/20'
      : 'border-emerald-500/20'}"
  >
    <div class="flex items-center gap-2.5">
      <div
        class="flex h-8 w-8 items-center justify-center rounded-lg text-white shadow-sm {isPartial
          ? 'bg-amber-500'
          : 'bg-emerald-500'}"
      >
        {#if isPartial}
          <svg
            class="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <circle cx="12" cy="12" r="10" /><line x1="12" y1="8" x2="12" y2="12" /><line
              x1="12"
              y1="16"
              x2="12.01"
              y2="16"
            />
          </svg>
        {:else}
          <svg
            class="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
          >
            <polyline points="20 6 9 17 4 12" />
          </svg>
        {/if}
      </div>
      <div>
        <h3 class="text-sm font-bold text-foreground">
          {isPartial ? "部分产物清单 (Partial Deliverables)" : "工作完成交付总结 (Work Completed)"}
        </h3>
        <p class="text-xs text-muted-foreground mt-0.5">
          {isPartial
            ? "执行未全量完成，以下仅为中断前生成的中间或部分产物"
            : "所有主要成果已生成并通过系统验证"}
        </p>
      </div>
    </div>

    <!-- Metrics -->
    <div class="flex items-center gap-3 text-xs">
      <div
        class="flex items-center gap-1.5 rounded-md bg-background/80 px-2.5 py-1 border border-border/60"
      >
        <span class="text-muted-foreground">交付物:</span>
        <span
          class="font-mono font-bold {isPartial
            ? 'text-amber-600 dark:text-amber-400'
            : 'text-emerald-600 dark:text-emerald-400'}"
        >
          {deliveredCount} / {runArtifacts.length}
        </span>
      </div>

      {#if totalSources > 0}
        <div
          class="flex items-center gap-1.5 rounded-md bg-background/80 px-2.5 py-1 border border-border/60"
        >
          <span class="text-muted-foreground">来源数:</span>
          <span class="font-mono font-medium text-foreground">{totalSources}</span>
        </div>
      {/if}

      {#if formattedDuration}
        <div
          class="flex items-center gap-1.5 rounded-md bg-background/80 px-2.5 py-1 border border-border/60"
        >
          <span class="text-muted-foreground">耗时:</span>
          <span class="font-mono font-medium text-foreground">{formattedDuration}</span>
        </div>
      {/if}
    </div>
  </div>

  <!-- Deliverables List -->
  <div class="mt-3">
    <h4 class="text-xs font-semibold text-foreground mb-2 flex items-center justify-between">
      <span>{isPartial ? "已生成产物明细" : "正式交付成果 (Deliverables)"}</span>
      <span class="text-[11px] text-muted-foreground font-normal"
        >共 {runArtifacts.length} 个文件</span
      >
    </h4>

    {#if runArtifacts.length === 0}
      <div
        class="rounded-lg border border-dashed border-border p-4 text-center text-xs text-muted-foreground"
      >
        本次运行未登记成果文件
      </div>
    {:else}
      <div class="grid grid-cols-1 md:grid-cols-2 gap-2.5">
        {#each runArtifacts as artifact (artifact.id)}
          <WorkArtifactCard
            {artifact}
            {onPreview}
            onOpen={onOpen || ((a) => onPreview(a))}
            onExport={onExport || (() => {})}
            onDeliver={onDeliver || (() => {})}
            {onShowDetails}
          />
        {/each}
      </div>
    {/if}
  </div>
</div>
