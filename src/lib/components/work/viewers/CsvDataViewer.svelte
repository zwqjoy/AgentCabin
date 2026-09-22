<script lang="ts">
  import { filterCsvRows, parseCsv, sortCsvRows, type ParsedCsv } from "$lib/utils/csv-parser";

  interface Props {
    content: string;
  }

  let { content }: Props = $props();

  let searchQuery = $state("");
  let sortCol = $state<number | null>(null);
  let sortDir = $state<"asc" | "desc">("asc");
  let copied = $state(false);

  let parsed = $derived<ParsedCsv>(parseCsv(content));

  let displayedRows = $derived.by(() => {
    let rows = parsed.rows;
    if (searchQuery.trim()) {
      rows = filterCsvRows(rows, searchQuery);
    }
    if (sortCol !== null) {
      rows = sortCsvRows(rows, sortCol, sortDir);
    }
    return rows;
  });

  function handleSort(colIndex: number) {
    if (sortCol === colIndex) {
      if (sortDir === "asc") {
        sortDir = "desc";
      } else {
        sortCol = null;
        sortDir = "asc";
      }
    } else {
      sortCol = colIndex;
      sortDir = "asc";
    }
  }

  async function copyToClipboard() {
    try {
      await navigator.clipboard.writeText(content);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      // ignore
    }
  }
</script>

<div class="flex h-full w-full flex-col overflow-hidden bg-background">
  <!-- Toolbar -->
  <div
    class="flex flex-wrap items-center justify-between gap-3 border-b border-border/70 bg-card/60 px-4 py-2.5"
  >
    <div class="flex items-center gap-2.5">
      <div class="relative w-56 sm:w-64">
        <input
          type="text"
          placeholder="搜索表格内容..."
          bind:value={searchQuery}
          class="h-8 w-full rounded-lg border border-border/80 bg-background px-2.5 pl-8 text-xs text-foreground placeholder-muted-foreground outline-none transition-colors focus:border-primary focus:ring-1 focus:ring-primary/20"
        />
        <svg
          class="pointer-events-none absolute left-2.5 top-2.5 h-3.5 w-3.5 text-muted-foreground"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <circle cx="11" cy="11" r="8"></circle>
          <path d="m21 21-4.3-4.3"></path>
        </svg>
        {#if searchQuery}
          <button
            type="button"
            class="absolute right-2 top-2 text-muted-foreground hover:text-foreground"
            onclick={() => (searchQuery = "")}
            aria-label="清空搜索"
          >
            ✕
          </button>
        {/if}
      </div>

      <span class="rounded-md bg-muted px-2 py-1 text-[11px] font-medium text-muted-foreground">
        {#if searchQuery}
          筛选出 {displayedRows.length} / {parsed.totalRows} 行
        {:else}
          共 {parsed.totalRows} 行 · {parsed.totalCols} 列
        {/if}
      </span>
    </div>

    <div class="flex items-center gap-2">
      <button
        type="button"
        class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-2.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={() => void copyToClipboard()}
      >
        {#if copied}
          <svg
            class="h-3.5 w-3.5 text-emerald-500"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <polyline points="20 6 9 17 4 12"></polyline>
          </svg>
          <span class="text-emerald-600 dark:text-emerald-400">已复制</span>
        {:else}
          <svg
            class="h-3.5 w-3.5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
          </svg>
          <span>复制原始内容</span>
        {/if}
      </button>
    </div>
  </div>

  <!-- Table Container -->
  <div class="flex-1 overflow-auto overscroll-contain">
    {#if parsed.headers.length === 0}
      <div class="flex h-full items-center justify-center p-8 text-xs text-muted-foreground">
        表格内容为空
      </div>
    {:else}
      <table class="w-full border-collapse text-left text-xs">
        <thead
          class="sticky top-0 z-10 border-b border-border bg-muted/90 shadow-sm backdrop-blur-sm"
        >
          <tr>
            <th
              class="w-12 px-3 py-2.5 text-center font-mono text-[10px] font-semibold text-muted-foreground/70"
            >
              #
            </th>
            {#each parsed.headers as header, colIdx}
              <th
                class="group cursor-pointer select-none px-3 py-2.5 font-semibold text-foreground transition-colors hover:bg-accent/60"
                onclick={() => handleSort(colIdx)}
              >
                <div class="flex items-center justify-between gap-1.5">
                  <span class="truncate">{header || `列 ${colIdx + 1}`}</span>
                  <span
                    class="text-[10px] text-muted-foreground/60 transition-opacity group-hover:opacity-100"
                  >
                    {#if sortCol === colIdx}
                      {sortDir === "asc" ? "▲" : "▼"}
                    {:else}
                      <span class="opacity-0 group-hover:opacity-40">↕</span>
                    {/if}
                  </span>
                </div>
              </th>
            {/each}
          </tr>
        </thead>
        <tbody class="divide-y divide-border/40 font-mono text-[11px]">
          {#each displayedRows as row, rowIdx}
            <tr class="transition-colors hover:bg-accent/40">
              <td class="select-none px-3 py-2 text-center text-[10px] text-muted-foreground/60">
                {rowIdx + 1}
              </td>
              {#each row as cell}
                <td class="max-w-md truncate px-3 py-2 text-foreground/90" title={cell}>
                  {cell}
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
</div>
