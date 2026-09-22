<script lang="ts">
  import WorkChangesPanel from "./WorkChangesPanel.svelte";
  import WorkSourcesPanel from "./WorkSourcesPanel.svelte";
  import { formatDurationMs } from "$lib/utils/work-result";
  import type { WorkRunReceipt } from "$lib/types/work";

  interface Props {
    receipt: WorkRunReceipt | null;
    loading?: boolean;
    error?: string;
  }

  let { receipt, loading = false, error = "" }: Props = $props();

  let showSources = $state(true);
  let showChanges = $state(true);

  const accessedSourceCount = $derived(
    receipt ? receipt.sources.filter((source) => source.accessed).length : 0,
  );
  const createdCount = $derived(
    receipt ? receipt.changedFiles.filter((c) => c.changeKind === "created").length : 0,
  );
  const modifiedCount = $derived(
    receipt ? receipt.changedFiles.filter((c) => c.changeKind === "modified").length : 0,
  );
  const deletedCount = $derived(
    receipt ? receipt.changedFiles.filter((c) => c.changeKind === "deleted").length : 0,
  );

  const effectiveDurationMs = $derived.by(() => {
    if (receipt?.durationMs != null && receipt.durationMs >= 0) return receipt.durationMs;
    if (receipt?.startedAt && receipt?.finishedAt) {
      const start = new Date(receipt.startedAt).getTime();
      const end = new Date(receipt.finishedAt).getTime();
      if (!isNaN(start) && !isNaN(end) && end >= start) {
        return end - start;
      }
    }
    return null;
  });

  const runLabel = $derived.by(() => {
    if (!receipt) return "";
    const parts = [receipt.runtime || "运行时"];
    if (receipt.model) parts.push(receipt.model);
    if (effectiveDurationMs != null) parts.push(`耗时 ${formatDurationMs(effectiveDurationMs)}`);
    return parts.join(" · ");
  });

  const cells = $derived(
    receipt
      ? [
          { label: "成果", value: String(receipt.artifacts.length) },
          { label: "已读来源", value: String(accessedSourceCount) },
          { label: "输入文件", value: String(receipt.inputFiles.length) },
          { label: "变更文件", value: String(receipt.changedFiles.length) },
          {
            label: "工具调用",
            value: String(receipt.toolSummary.completed + receipt.toolSummary.failed),
          },
          { label: "耗时", value: formatDurationMs(effectiveDurationMs) },
        ]
      : [],
  );
</script>

<div class="space-y-3">
  {#if error}
    <div
      class="rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
      role="alert"
    >
      {error}
    </div>
  {:else if loading && !receipt}
    <div class="flex items-center gap-2 px-1 py-3 text-xs text-muted-foreground">
      <span
        class="h-4 w-4 animate-spin rounded-full border-2 border-muted-foreground/20 border-t-primary"
      ></span>
      正在汇总任务回执…
    </div>
  {:else if receipt}
    {#if !receipt.ledgerAvailable && receipt.artifacts.length === 0}
      <p
        class="rounded-lg border border-dashed border-border/70 px-3 py-2 text-[11px] text-muted-foreground"
      >
        暂无运行记录。
      </p>
    {/if}

    <div class="grid grid-cols-3 gap-1.5 text-center">
      {#each cells as cell (cell.label)}
        <div class="rounded-lg border border-border/60 bg-card/40 px-1 py-1.5">
          <p class="text-[13px] font-semibold text-foreground">{cell.value}</p>
          <p class="text-[10px] text-muted-foreground">{cell.label}</p>
        </div>
      {/each}
    </div>

    {#if receipt.sources.length > 0}
      <div class="flex items-center gap-3 text-[10px] text-muted-foreground/80">
        <span>来源合计 {receipt.sources.length}</span>
        <span>新建 {createdCount}</span>
        <span>修改 {modifiedCount}</span>
        {#if deletedCount > 0}
          <span class="text-red-500/80">删除 {deletedCount}</span>
        {/if}
      </div>
    {/if}

    <section>
      <button
        type="button"
        class="flex w-full items-center justify-between rounded-lg px-1 py-1 text-left"
        aria-expanded={showSources}
        onclick={() => (showSources = !showSources)}
      >
        <span class="text-xs font-semibold text-foreground">来源</span>
        <span class="text-[10px] text-muted-foreground">来自真实运行事件</span>
      </button>
      {#if showSources}
        <div class="mt-1">
          <WorkSourcesPanel sources={receipt.sources} searchQueries={receipt.searchQueries} />
        </div>
      {/if}
    </section>

    <section>
      <button
        type="button"
        class="flex w-full items-center justify-between rounded-lg px-1 py-1 text-left"
        aria-expanded={showChanges}
        onclick={() => (showChanges = !showChanges)}
      >
        <span class="text-xs font-semibold text-foreground">变更</span>
        <span class="text-[10px] text-muted-foreground">Ledger 记录的文件变化</span>
      </button>
      {#if showChanges}
        <div class="mt-1">
          <WorkChangesPanel changes={receipt.changedFiles} inputFiles={receipt.inputFiles} />
        </div>
      {/if}
    </section>

    <p class="border-t border-border/50 pt-2 text-[10px] text-muted-foreground/70">
      {runLabel}
    </p>
  {:else}
    <p
      class="rounded-lg border border-dashed border-border/70 px-3 py-2 text-[11px] text-muted-foreground"
    >
      暂无任务回执。
    </p>
  {/if}
</div>
