<script lang="ts">
  import DOMPurify from "dompurify";
  import { t } from "$lib/i18n/index.svelte";
  import {
    OFFICE_PREVIEW_TOO_LARGE,
    parseOfficePreview,
    type OfficePreview as OfficePreviewData,
  } from "$lib/utils/office-preview";

  interface Props {
    base64: string;
    fileName?: string;
    fileExt: string;
  }

  let { base64, fileName = "document", fileExt }: Props = $props();

  let preview = $state<OfficePreviewData | null>(null);
  let loading = $state(false);
  let error = $state("");
  let activeSheetIndex = $state(0);
  let searchQuery = $state("");
  let loadSequence = 0;

  let activeSheet = $derived(
    preview?.kind === "xlsx" ? (preview.sheets[activeSheetIndex] ?? null) : null,
  );
  let activeSheetColumns = $derived(
    activeSheet ? Array.from({ length: activeSheet.columnCount }, (_, index) => index) : [],
  );
  let displayedRows = $derived.by(() => {
    if (!activeSheet) return [];
    const [, ...bodyRows] = activeSheet.rows;
    const query = searchQuery.trim().toLowerCase();
    if (!query) return bodyRows;
    return bodyRows.filter((row) => row.some((cell) => cell.toLowerCase().includes(query)));
  });
  let safeDocxHtml = $derived(
    preview?.kind === "docx"
      ? DOMPurify.sanitize(preview.html, { ADD_ATTR: ["class", "style"] })
      : "",
  );

  $effect(() => {
    const encoded = base64;
    const extension = fileExt.toLowerCase();
    const sequence = ++loadSequence;
    preview = null;
    error = "";
    activeSheetIndex = 0;
    searchQuery = "";

    if (!encoded) {
      loading = false;
      return;
    }

    loading = true;
    void parseOfficePreview(encoded, extension)
      .then((result) => {
        if (sequence === loadSequence) preview = result;
      })
      .catch((cause: unknown) => {
        if (sequence !== loadSequence) return;
        const message = cause instanceof Error ? cause.message : String(cause);
        error = message === OFFICE_PREVIEW_TOO_LARGE ? t("officePreview_tooLarge") : message;
      })
      .finally(() => {
        if (sequence === loadSequence) loading = false;
      });

    return () => {
      loadSequence += 1;
    };
  });

  function selectSheet(index: number): void {
    activeSheetIndex = index;
    searchQuery = "";
  }
</script>

<div class="flex h-full w-full flex-col overflow-hidden bg-background">
  {#if loading}
    <div class="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground">
      <div
        class="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent"
      ></div>
      <span class="text-xs">{t("officePreview_loading")}</span>
    </div>
  {:else if error}
    <div class="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
      <div class="rounded-full bg-amber-500/10 p-3 text-amber-500" aria-hidden="true">⚠</div>
      <p class="text-sm font-medium text-foreground">{t("officePreview_errorTitle")}</p>
      <p class="max-w-lg text-xs leading-5 text-muted-foreground">{error}</p>
      <p class="max-w-lg text-[11px] leading-5 text-muted-foreground/80">
        {t("officePreview_errorDesc")}
      </p>
    </div>
  {:else if preview?.kind === "docx"}
    <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div
        class="flex flex-wrap items-center justify-between gap-2 border-b border-border/70 bg-card/60 px-4 py-2"
      >
        <div class="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          <span
            class="rounded bg-blue-500/10 px-2 py-0.5 text-[10px] font-medium text-blue-600 dark:text-blue-300"
          >
            {t("officePreview_docxBadge")}
          </span>
          <span class="truncate font-mono">{fileName}</span>
        </div>
        <span class="text-[11px] text-muted-foreground">{t("officePreview_docxNote")}</span>
      </div>

      <div class="office-docx flex-1 overflow-auto bg-muted/20 p-4 sm:p-8">
        <article
          class="mx-auto min-h-full max-w-3xl rounded-xl border border-border/70 bg-card px-5 py-6 shadow-sm sm:px-10 sm:py-9"
        >
          {@html safeDocxHtml}
        </article>
      </div>

      {#if preview.messages.length > 0}
        <details
          class="shrink-0 border-t border-border/60 bg-card/60 px-4 py-2 text-[11px] text-muted-foreground"
        >
          <summary class="cursor-pointer select-none">
            {t("officePreview_conversionNotes", { count: String(preview.messages.length) })}
          </summary>
          <ul class="mt-2 list-disc space-y-1 pl-4">
            {#each preview.messages as message}
              <li>{message}</li>
            {/each}
          </ul>
        </details>
      {/if}
    </div>
  {:else if preview?.kind === "xlsx"}
    <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div class="shrink-0 border-b border-border/70 bg-card/60 px-3 py-2">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div class="flex min-w-0 items-center gap-1 overflow-x-auto" role="tablist">
            <span
              class="mr-1 rounded bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-300"
            >
              {t("officePreview_xlsxBadge")}
            </span>
            {#each preview.sheets as sheet, index (sheet.name)}
              <button
                type="button"
                class="min-h-8 max-w-48 truncate rounded-lg px-2.5 text-xs transition-colors {activeSheetIndex ===
                index
                  ? 'bg-primary/10 font-semibold text-primary'
                  : 'text-muted-foreground hover:bg-accent hover:text-foreground'}"
                role="tab"
                aria-selected={activeSheetIndex === index}
                onclick={() => selectSheet(index)}
                title={sheet.name}
              >
                {sheet.name}
              </button>
            {/each}
          </div>
          <input
            type="search"
            bind:value={searchQuery}
            class="h-8 w-full rounded-lg border border-border/80 bg-background px-2.5 text-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground focus:border-primary focus:ring-1 focus:ring-primary/20 sm:w-56"
            placeholder={t("officePreview_searchPlaceholder")}
            aria-label={t("officePreview_searchPlaceholder")}
          />
        </div>
        {#if activeSheet}
          <div
            class="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted-foreground"
          >
            <span
              >{t("officePreview_sheetSummary", {
                rows: String(activeSheet.rowCount),
                columns: String(activeSheet.columnCount),
              })}</span
            >
            {#if activeSheet.truncated}
              <span class="text-amber-600 dark:text-amber-300">
                {t("officePreview_sheetTruncated")}
              </span>
            {/if}
            {#if searchQuery.trim()}
              <span
                >{t("officePreview_searchSummary", {
                  shown: String(displayedRows.length),
                  total: String(Math.max(activeSheet.rowCount - 1, 0)),
                })}</span
              >
            {/if}
          </div>
        {/if}
      </div>

      <div class="min-h-0 flex-1 overflow-auto overscroll-contain">
        {#if activeSheet && activeSheet.rows.length > 0}
          {@const header = activeSheet.rows[0] ?? []}
          <table class="min-w-full border-collapse text-left text-xs">
            <thead
              class="sticky top-0 z-10 border-b border-border bg-muted/95 shadow-sm backdrop-blur-sm"
            >
              <tr>
                <th
                  class="sticky left-0 z-20 w-12 border-r border-border/60 bg-muted px-3 py-2 text-center font-mono text-[10px] text-muted-foreground/70"
                >
                  #
                </th>
                {#each activeSheetColumns as columnIndex}
                  <th
                    class="min-w-32 max-w-64 whitespace-nowrap px-3 py-2.5 font-semibold text-foreground"
                  >
                    {header[columnIndex] ||
                      t("officePreview_column", { number: String(columnIndex + 1) })}
                  </th>
                {/each}
              </tr>
            </thead>
            <tbody class="divide-y divide-border/40 font-mono text-[11px]">
              {#each displayedRows as row, rowIndex}
                <tr class="transition-colors hover:bg-accent/40">
                  <td
                    class="sticky left-0 border-r border-border/40 bg-background px-3 py-2 text-center text-[10px] text-muted-foreground/60"
                  >
                    {rowIndex + 2}
                  </td>
                  {#each activeSheetColumns as columnIndex}
                    <td
                      class="max-w-80 truncate px-3 py-2 text-foreground/90"
                      title={row[columnIndex] ?? ""}
                    >
                      {row[columnIndex] ?? ""}
                    </td>
                  {/each}
                </tr>
              {/each}
            </tbody>
          </table>
          {#if displayedRows.length === 0 && searchQuery.trim()}
            <div class="flex justify-center p-8 text-xs text-muted-foreground">
              {t("officePreview_noMatches")}
            </div>
          {/if}
        {:else}
          <div class="flex h-full items-center justify-center p-8 text-xs text-muted-foreground">
            {t("officePreview_empty")}
          </div>
        {/if}
      </div>
    </div>
  {:else if preview?.kind === "pptx"}
    <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div
        class="flex flex-wrap items-center justify-between gap-2 border-b border-border/70 bg-card/60 px-4 py-2"
      >
        <div class="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          <span
            class="rounded bg-violet-500/10 px-2 py-0.5 text-[10px] font-medium text-violet-600 dark:text-violet-300"
          >
            {t("officePreview_pptxBadge")}
          </span>
          <span class="truncate font-mono">{fileName}</span>
        </div>
        <span class="text-[11px] text-muted-foreground">
          {t("officePreview_slideSummary", { count: String(preview.slides.length) })}
        </span>
      </div>
      <div class="flex-1 overflow-auto bg-muted/20 p-4 sm:p-6">
        <div class="mx-auto max-w-4xl space-y-4">
          <div
            class="rounded-xl border border-violet-500/20 bg-violet-500/5 px-4 py-3 text-xs leading-5 text-muted-foreground"
          >
            {t("officePreview_pptxNote")}
          </div>
          {#each preview.slides as slide (slide.number)}
            <article class="overflow-hidden rounded-xl border border-border/70 bg-card shadow-sm">
              <div
                class="flex items-center gap-3 border-b border-border/60 bg-muted/40 px-4 py-2.5"
              >
                <span
                  class="flex h-7 w-7 items-center justify-center rounded-lg bg-violet-500/10 text-xs font-semibold text-violet-600 dark:text-violet-300"
                >
                  {slide.number}
                </span>
                <span class="text-xs font-medium text-muted-foreground">
                  {t("officePreview_slideLabel", { number: String(slide.number) })}
                </span>
              </div>
              <div class="space-y-3 px-5 py-5 sm:px-8">
                {#if slide.paragraphs.length > 0}
                  <h3 class="text-lg font-semibold leading-snug text-foreground">
                    {slide.paragraphs[0]}
                  </h3>
                  {#each slide.paragraphs.slice(1) as paragraph}
                    <p class="text-sm leading-6 text-muted-foreground">{paragraph}</p>
                  {/each}
                {:else}
                  <p class="text-sm text-muted-foreground">{t("officePreview_emptySlide")}</p>
                {/if}
              </div>
            </article>
          {/each}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .office-docx :global(h1),
  .office-docx :global(h2),
  .office-docx :global(h3),
  .office-docx :global(h4) {
    color: hsl(var(--foreground));
    font-weight: 650;
    line-height: 1.25;
    margin: 1.25em 0 0.55em;
  }

  .office-docx :global(h1) {
    font-size: 1.65rem;
  }

  .office-docx :global(h2) {
    font-size: 1.3rem;
  }

  .office-docx :global(h3) {
    font-size: 1.1rem;
  }

  .office-docx :global(p),
  .office-docx :global(li) {
    color: hsl(var(--foreground) / 0.9);
    font-size: 0.9rem;
    line-height: 1.75;
  }

  .office-docx :global(ul),
  .office-docx :global(ol) {
    margin: 0.75rem 0;
    padding-left: 1.5rem;
  }

  .office-docx :global(table) {
    border-collapse: collapse;
    margin: 1rem 0;
    width: 100%;
  }

  .office-docx :global(th),
  .office-docx :global(td) {
    border: 1px solid hsl(var(--border));
    padding: 0.5rem 0.65rem;
    text-align: left;
    vertical-align: top;
  }

  .office-docx :global(th) {
    background: hsl(var(--muted));
    font-weight: 600;
  }

  .office-docx :global(img) {
    height: auto;
    max-width: 100%;
  }

  .office-docx :global(a) {
    color: hsl(var(--primary));
    text-decoration: underline;
    text-underline-offset: 2px;
  }
</style>
