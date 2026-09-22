<script lang="ts">
  import type { BusToolItem } from "$lib/types";
  import { aggregateBatchStatus } from "$lib/utils/tool-rendering";
  import { dbg } from "$lib/utils/debug";
  import { t } from "$lib/i18n/index.svelte";

  let { tools }: { tools: BusToolItem[] } = $props();

  // Single-pass aggregation
  let stats = $derived(aggregateBatchStatus(tools));
  let allDone = $derived(stats.total > 0 && stats.completed + stats.failed === stats.total);
  let hasActive = $derived(!allDone);
  let hasFailure = $derived(stats.failed > 0);

  // Debug: signature-based dedup
  let _lastDbgSig = "";
  $effect(() => {
    const sig = `${stats.total}:${stats.completed}:${stats.running}:${stats.failed}`;
    if (sig !== _lastDbgSig) {
      _lastDbgSig = sig;
      dbg("batch", "progress", stats);
    }
  });
</script>

<div class="w-full rounded-md px-2 py-1">
  <div class="flex min-h-5 items-center gap-1.5 text-xs text-muted-foreground">
    {#if hasFailure}
      <svg
        class="h-3.5 w-3.5 shrink-0 text-destructive"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"><circle cx="12" cy="12" r="9" /><path d="M12 8v4M12 16h.01" /></svg
      >
    {:else if allDone}
      <svg
        class="h-3 w-3 text-emerald-500 shrink-0"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2.5"
        stroke-linecap="round"
        stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
      >
    {:else if hasActive}
      <span class="inline-block h-1.5 w-1.5 rounded-full bg-blue-400 animate-pulse shrink-0"></span>
    {/if}
    <span>{t(hasFailure ? "batch_failed" : hasActive ? "batch_running" : "batch_done")}</span>
  </div>
</div>
