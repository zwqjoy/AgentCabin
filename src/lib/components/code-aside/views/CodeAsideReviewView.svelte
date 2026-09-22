<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { parseTurnReviewDiff, type TurnReviewRow } from "$lib/utils/turn-review";
  import { getAppearance } from "$lib/stores/appearance.svelte";

  interface Props {
    diffText: string;
    showFileTree?: boolean;
    onToggleFileTree?: () => void;
  }

  let { diffText, showFileTree = $bindable(true), onToggleFileTree }: Props = $props();

  let selectedPath = $state("");
  let filter = $state("");
  let files = $derived(parseTurnReviewDiff(diffText));
  let filteredFiles = $derived(
    filter.trim()
      ? files.filter((file) => file.path.toLowerCase().includes(filter.trim().toLowerCase()))
      : files,
  );
  let selectedFile = $derived(files.find((file) => file.path === selectedPath) ?? files[0] ?? null);
  let totalInsertions = $derived(files.reduce((total, file) => total + file.insertions, 0));
  let totalDeletions = $derived(files.reduce((total, file) => total + file.deletions, 0));

  $effect(() => {
    if (!files.some((file) => file.path === selectedPath)) {
      selectedPath = files[0]?.path ?? "";
    }
  });

  function cellTone(row: TurnReviewRow, side: "old" | "new"): string {
    if (side === "old" && (row.kind === "change" || row.kind === "deletion")) {
      return "bg-red-500/10";
    }
    if (side === "new" && (row.kind === "change" || row.kind === "addition")) {
      return "bg-emerald-500/10";
    }
    return "bg-background";
  }

  function lineTone(row: TurnReviewRow, side: "old" | "new"): string {
    if (side === "old" && (row.kind === "change" || row.kind === "deletion")) {
      return "bg-red-500/15 text-red-700 dark:text-red-300";
    }
    if (side === "new" && (row.kind === "change" || row.kind === "addition")) {
      return "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300";
    }
    return "bg-muted/35 text-muted-foreground";
  }

  function marker(row: TurnReviewRow, side: "old" | "new"): string {
    if (getAppearance().diffMarkerStyle === "color") return " ";
    if (side === "old" && (row.kind === "change" || row.kind === "deletion")) return "−";
    if (side === "new" && (row.kind === "change" || row.kind === "addition")) return "+";
    return " ";
  }
</script>

<div class="flex h-full flex-col overflow-hidden bg-background">
  <!-- Sub-header: summary metrics -->
  <div
    class="flex h-10 shrink-0 items-center justify-between border-b border-border/60 px-4 bg-muted/10 text-xs"
  >
    <div class="flex items-center gap-3">
      <span class="font-medium text-foreground">{t("turnReview_previousTurn")}</span>
      <span class="font-mono tabular-nums text-emerald-600 dark:text-emerald-400"
        >+{totalInsertions}</span
      >
      <span class="font-mono tabular-nums text-red-500 dark:text-red-400">-{totalDeletions}</span>
    </div>
    <div class="flex items-center gap-2">
      <span class="text-xs text-muted-foreground">
        {t("turnReview_fileCount", { count: String(files.length) })}
      </span>
      <button
        type="button"
        class="rounded p-1 transition-colors {showFileTree
          ? 'text-foreground hover:bg-muted/60'
          : 'text-muted-foreground/60 hover:bg-muted/40 hover:text-foreground'}"
        title={showFileTree ? "收起文件列表" : "展开文件列表"}
        onclick={() => {
          if (onToggleFileTree) {
            onToggleFileTree();
          } else {
            showFileTree = !showFileTree;
          }
        }}
      >
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <rect x="3" y="3" width="18" height="18" rx="2" />
          <path d="M15 3v18" />
        </svg>
      </button>
    </div>
  </div>

  <!-- Body: Diff on left, file list on right -->
  <div class="flex min-h-0 flex-1">
    <section class="flex min-w-0 flex-1 flex-col overflow-hidden">
      {#if selectedFile}
        <div
          class="flex h-9 shrink-0 items-center gap-2 border-b border-border/60 px-4 bg-muted/5 text-xs"
        >
          <svg
            class="h-3.5 w-3.5 shrink-0 text-muted-foreground"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
            <path d="M14 2v6h6" />
          </svg>
          <span class="min-w-0 truncate font-mono font-medium text-foreground">
            {selectedFile.path}
          </span>
          <span
            class="ml-auto shrink-0 font-mono text-[11px] text-emerald-600 dark:text-emerald-400"
            >+{selectedFile.insertions}</span
          >
          <span class="shrink-0 font-mono text-[11px] text-red-500 dark:text-red-400"
            >-{selectedFile.deletions}</span
          >
        </div>

        <div class="min-h-0 flex-1 overflow-auto bg-muted/15">
          <div class="min-w-[500px]">
            <div
              class="sticky top-0 z-10 grid grid-cols-2 border-b border-border bg-background/95 text-[11px] font-medium text-muted-foreground backdrop-blur"
            >
              <div class="border-r border-border px-3 py-1.5">{t("turnReview_before")}</div>
              <div class="px-3 py-1.5">{t("turnReview_after")}</div>
            </div>
            {#each selectedFile.hunks as hunk}
              <div
                class="border-b border-blue-500/15 bg-blue-500/5 px-3 py-1 font-mono text-[11px] text-blue-600 dark:text-blue-300"
              >
                {hunk.header}
              </div>
              {#each hunk.rows as row}
                <div class="grid min-h-6 grid-cols-2 border-b border-border/35 font-mono text-xs">
                  <div
                    class="grid min-w-0 grid-cols-[44px_18px_1fr] border-r border-border {cellTone(
                      row,
                      'old',
                    )}"
                  >
                    <span
                      class="select-none px-1.5 py-0.5 text-right tabular-nums {lineTone(
                        row,
                        'old',
                      )}">{row.oldLine ?? ""}</span
                    >
                    <span class="select-none py-0.5 text-center text-red-500/80"
                      >{marker(row, "old")}</span
                    >
                    <code class="overflow-visible whitespace-pre px-1 py-0.5 text-foreground/90"
                      >{row.oldText}</code
                    >
                  </div>
                  <div class="grid min-w-0 grid-cols-[44px_18px_1fr] {cellTone(row, 'new')}">
                    <span
                      class="select-none px-1.5 py-0.5 text-right tabular-nums {lineTone(
                        row,
                        'new',
                      )}">{row.newLine ?? ""}</span
                    >
                    <span class="select-none py-0.5 text-center text-emerald-600/80"
                      >{marker(row, "new")}</span
                    >
                    <code class="overflow-visible whitespace-pre px-1 py-0.5 text-foreground/90"
                      >{row.newText}</code
                    >
                  </div>
                </div>
              {/each}
            {/each}
          </div>
        </div>
      {:else}
        <div class="flex flex-1 items-center justify-center text-xs text-muted-foreground">
          {t("diff_noChanges")}
        </div>
      {/if}
    </section>

    {#if showFileTree}
      <nav class="flex w-52 shrink-0 flex-col border-l border-border/60 bg-muted/10 text-xs">
        <div class="p-2 border-b border-border/60">
          <label
            class="flex h-7 items-center gap-1.5 rounded-md border border-border/70 bg-background px-2 focus-within:ring-1 focus-within:ring-ring"
          >
            <svg
              class="h-3 w-3 shrink-0 text-muted-foreground"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" />
            </svg>
            <input
              bind:value={filter}
              class="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground"
              placeholder={t("turnReview_filterFiles")}
            />
          </label>
        </div>
        <div class="min-h-0 flex-1 overflow-y-auto p-1.5 space-y-0.5">
          {#each filteredFiles as file (file.path)}
            <button
              type="button"
              class="group flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left transition-colors {selectedFile?.path ===
              file.path
                ? 'bg-accent text-foreground font-medium'
                : 'text-muted-foreground hover:bg-muted/60 hover:text-foreground'}"
              onclick={() => (selectedPath = file.path)}
            >
              <svg
                class="h-3 w-3 shrink-0 opacity-60"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
              </svg>
              <span class="min-w-0 flex-1 truncate font-mono text-[11px]">{file.path}</span>
              <span class="shrink-0 font-mono text-[10px] text-emerald-600">+{file.insertions}</span
              >
              <span class="shrink-0 font-mono text-[10px] text-red-500">-{file.deletions}</span>
            </button>
          {/each}
        </div>
      </nav>
    {/if}
  </div>
</div>
