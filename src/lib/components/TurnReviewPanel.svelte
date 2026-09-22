<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { parseTurnReviewDiff, type TurnReviewRow } from "$lib/utils/turn-review";
  import { getAppearance } from "$lib/stores/appearance.svelte";

  let {
    diffText,
    onClose,
  }: {
    diffText: string;
    onClose: () => void;
  } = $props();

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

  function fileName(path: string): string {
    return path.split("/").pop() ?? path;
  }

  function directory(path: string): string {
    const parts = path.split("/");
    return parts.length > 1 ? parts.slice(0, -1).join("/") : "";
  }

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

<svelte:window
  onkeydown={(event) => {
    if (event.key === "Escape") onClose();
  }}
/>

<aside
  class="relative z-20 flex h-full shrink-0 flex-col overflow-hidden border-l border-border bg-background shadow-[-8px_0_24px_rgba(0,0,0,0.06)] animate-in slide-in-from-right-2 duration-200"
  style="width: clamp(460px, 58vw, 980px)"
  aria-label={t("turnReview_title")}
  data-export-exclude
>
  <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-3">
    <div class="flex h-7 w-7 items-center justify-center rounded-md bg-muted text-foreground">
      <svg
        class="h-4 w-4"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <rect x="4" y="3" width="16" height="18" rx="3" />
        <path d="M12 8v8M8 12h8" />
      </svg>
    </div>
    <h2 class="text-sm font-semibold text-foreground">{t("turnReview_title")}</h2>
    <button
      type="button"
      class="ml-auto flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      onclick={onClose}
      title={t("common_close")}
      aria-label={t("common_close")}
    >
      <svg
        class="h-4 w-4"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        aria-hidden="true"
      >
        <path d="M18 6 6 18M6 6l12 12" />
      </svg>
    </button>
  </header>

  <div class="flex h-12 shrink-0 items-center gap-3 border-b border-border px-4">
    <span class="text-sm font-semibold text-foreground">{t("turnReview_previousTurn")}</span>
    <span class="font-mono text-sm tabular-nums text-emerald-600 dark:text-emerald-400"
      >+{totalInsertions}</span
    >
    <span class="font-mono text-sm tabular-nums text-red-500 dark:text-red-400"
      >-{totalDeletions}</span
    >
    <span class="ml-auto text-xs text-muted-foreground">
      {t("turnReview_fileCount", { count: String(files.length) })}
    </span>
  </div>

  <div class="flex min-h-0 flex-1">
    <section class="flex min-w-0 flex-1 flex-col overflow-hidden">
      {#if selectedFile}
        <div class="flex h-11 shrink-0 items-center gap-2 border-b border-border px-4">
          <svg
            class="h-4 w-4 shrink-0 text-muted-foreground"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.7"
            aria-hidden="true"
          >
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
            <path d="M14 2v6h6" />
          </svg>
          <span class="min-w-0 truncate text-sm font-medium text-foreground">
            {selectedFile.path}
          </span>
          <span class="ml-auto shrink-0 font-mono text-xs text-emerald-600 dark:text-emerald-400"
            >+{selectedFile.insertions}</span
          >
          <span class="shrink-0 font-mono text-xs text-red-500 dark:text-red-400"
            >-{selectedFile.deletions}</span
          >
        </div>

        <div class="min-h-0 flex-1 overflow-auto bg-muted/15">
          <div class="min-w-[660px]">
            <div
              class="sticky top-0 z-10 grid grid-cols-2 border-b border-border bg-background/95 text-[11px] font-medium text-muted-foreground backdrop-blur"
            >
              <div class="border-r border-border px-3 py-1.5">{t("turnReview_before")}</div>
              <div class="px-3 py-1.5">{t("turnReview_after")}</div>
            </div>
            {#each selectedFile.hunks as hunk}
              <div
                class="border-b border-blue-500/15 bg-blue-500/5 px-3 py-1.5 font-mono text-[11px] text-blue-600 dark:text-blue-300"
              >
                {hunk.header}
              </div>
              {#each hunk.rows as row}
                <div class="grid min-h-6 grid-cols-2 border-b border-border/35 font-mono text-xs">
                  <div
                    class="grid min-w-0 grid-cols-[48px_20px_1fr] border-r border-border {cellTone(
                      row,
                      'old',
                    )}"
                  >
                    <span
                      class="select-none px-2 py-1 text-right tabular-nums {lineTone(row, 'old')}"
                      >{row.oldLine ?? ""}</span
                    >
                    <span class="select-none py-1 text-center text-red-500/80"
                      >{marker(row, "old")}</span
                    >
                    <code class="overflow-visible whitespace-pre px-1 py-1 text-foreground/90"
                      >{row.oldText}</code
                    >
                  </div>
                  <div class="grid min-w-0 grid-cols-[48px_20px_1fr] {cellTone(row, 'new')}">
                    <span
                      class="select-none px-2 py-1 text-right tabular-nums {lineTone(row, 'new')}"
                      >{row.newLine ?? ""}</span
                    >
                    <span class="select-none py-1 text-center text-emerald-600/80"
                      >{marker(row, "new")}</span
                    >
                    <code class="overflow-visible whitespace-pre px-1 py-1 text-foreground/90"
                      >{row.newText}</code
                    >
                  </div>
                </div>
              {/each}
            {/each}
          </div>
        </div>
      {:else}
        <div class="flex flex-1 items-center justify-center text-sm text-muted-foreground">
          {t("diff_noChanges")}
        </div>
      {/if}
    </section>

    <nav
      class="flex w-56 shrink-0 flex-col border-l border-border bg-muted/15"
      aria-label={t("turnReview_files")}
    >
      <div class="p-3">
        <label
          class="flex h-9 items-center gap-2 rounded-lg border border-border bg-background px-2.5 focus-within:ring-2 focus-within:ring-ring"
        >
          <svg
            class="h-4 w-4 shrink-0 text-muted-foreground"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            aria-hidden="true"
          >
            <circle cx="11" cy="11" r="7" />
            <path d="m20 20-3.5-3.5" />
          </svg>
          <input
            bind:value={filter}
            class="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground"
            placeholder={t("turnReview_filterFiles")}
            aria-label={t("turnReview_filterFiles")}
          />
        </label>
      </div>
      <div class="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        {#each filteredFiles as file (file.path)}
          <button
            type="button"
            class="group flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left transition-colors {selectedFile?.path ===
            file.path
              ? 'bg-accent text-foreground'
              : 'text-muted-foreground hover:bg-muted hover:text-foreground'}"
            onclick={() => (selectedPath = file.path)}
            title={file.path}
          >
            <svg
              class="h-4 w-4 shrink-0 text-amber-600 dark:text-amber-400"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.7"
              aria-hidden="true"
            >
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
              <path d="M14 2v6h6" />
            </svg>
            <span class="min-w-0 flex-1">
              <span class="block truncate text-xs font-medium">{fileName(file.path)}</span>
              {#if directory(file.path)}
                <span class="mt-0.5 block truncate text-[10px] text-muted-foreground/70"
                  >{directory(file.path)}</span
                >
              {/if}
            </span>
            <span class="h-2 w-2 shrink-0 rounded-sm border border-amber-600/70"></span>
          </button>
        {/each}
        {#if filteredFiles.length === 0}
          <p class="px-2 py-6 text-center text-xs text-muted-foreground">
            {t("turnReview_noMatchingFiles")}
          </p>
        {/if}
      </div>
    </nav>
  </div>
</aside>
