<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import {
    editOfficeDocument,
    MAX_OFFICE_EDITOR_COLUMNS,
    MAX_OFFICE_EDITOR_ROWS,
    type OfficeDocumentExtension,
  } from "$lib/utils/office-edit";
  import { parseOfficePreview, type OfficePreview } from "$lib/utils/office-preview";

  interface Props {
    base64: string;
    fileName?: string;
    fileExt: OfficeDocumentExtension;
    onSave: (base64: string) => Promise<void>;
    onCancel: () => void;
  }

  let { base64, fileName = "document", fileExt, onSave, onCancel }: Props = $props();

  let preview = $state<OfficePreview | null>(null);
  let loading = $state(false);
  let saving = $state(false);
  let error = $state("");
  let saved = $state(false);
  let activeSheetIndex = $state(0);
  let findText = $state("");
  let replaceText = $state("");
  let replaceAll = $state(false);
  let cellEdits = $state<Record<string, string>>({});
  let loadSequence = 0;

  let activeSheet = $derived(
    preview?.kind === "xlsx" ? (preview.sheets[activeSheetIndex] ?? null) : null,
  );
  let editorRowCount = $derived(
    activeSheet ? Math.min(activeSheet.rowCount, MAX_OFFICE_EDITOR_ROWS) : 0,
  );
  let editorColumnCount = $derived(
    activeSheet ? Math.min(activeSheet.columnCount, MAX_OFFICE_EDITOR_COLUMNS) : 0,
  );
  let canSave = $derived(
    preview?.kind === "xlsx" ? Object.keys(cellEdits).length > 0 : findText.trim().length > 0,
  );

  $effect(() => {
    const encoded = base64;
    const extension = fileExt;
    const sequence = ++loadSequence;
    preview = null;
    error = "";
    saved = false;
    activeSheetIndex = 0;
    findText = "";
    replaceText = "";
    replaceAll = false;
    cellEdits = {};

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
        error = editorError(cause);
      })
      .finally(() => {
        if (sequence === loadSequence) loading = false;
      });

    return () => {
      loadSequence += 1;
    };
  });

  function cellKey(sheet: string, row: number, column: number): string {
    return `${sheet}\u0000${row}\u0000${column}`;
  }

  function cellValue(sheet: string, row: number, column: number): string {
    const key = cellKey(sheet, row, column);
    const edited = cellEdits[key];
    if (edited !== undefined) return edited;
    if (preview?.kind !== "xlsx") return "";
    return preview.sheets.find((item) => item.name === sheet)?.rows[row - 1]?.[column - 1] ?? "";
  }

  function setCellValue(sheet: string, row: number, column: number, value: string): void {
    cellEdits[cellKey(sheet, row, column)] = value;
    saved = false;
  }

  function selectSheet(index: number): void {
    activeSheetIndex = index;
  }

  function editorError(cause: unknown): string {
    const message = cause instanceof Error ? cause.message : String(cause);
    switch (message) {
      case "office_editor_format_mismatch":
        return t("officeEditor_formatMismatch");
      case "office_editor_find_required":
        return t("officeEditor_findRequired");
      case "office_editor_text_not_found":
        return t("officeEditor_textNotFound");
      case "office_document_too_large":
        return t("officeEditor_tooLarge");
      default:
        return message;
    }
  }

  async function save(): Promise<void> {
    if (!preview || !canSave || saving) return;
    saving = true;
    error = "";
    saved = false;
    try {
      let operation;
      if (preview.kind === "xlsx") {
        const edits = Object.entries(cellEdits).map(([key, value]) => {
          const [sheet, row, column] = key.split("\u0000");
          return {
            sheet,
            row: Number(row),
            column: Number(column),
            value,
          };
        });
        operation = { kind: "xlsx_cells" as const, edits };
      } else {
        operation = {
          kind: "text" as const,
          edit: { find: findText, replace: replaceText, replaceAll },
        };
      }
      const edited = await editOfficeDocument(base64, fileExt, operation);
      await onSave(edited);
      saved = true;
      cellEdits = {};
      findText = "";
      replaceText = "";
    } catch (cause) {
      error = editorError(cause);
    } finally {
      saving = false;
    }
  }
</script>

<div class="flex h-full min-h-0 flex-col overflow-hidden bg-background">
  <div
    class="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border/70 bg-card/60 px-4 py-3"
  >
    <div class="min-w-0">
      <p class="truncate text-xs font-semibold text-foreground">{t("officeEditor_title")}</p>
      <p class="mt-1 truncate font-mono text-[11px] text-muted-foreground">{fileName}</p>
    </div>
    <div class="flex items-center gap-2">
      <button
        type="button"
        class="min-h-8 rounded-lg border border-border px-3 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={onCancel}
        disabled={saving}
      >
        {t("officeEditor_cancel")}
      </button>
      <button
        type="button"
        class="min-h-8 rounded-lg bg-primary px-3 text-xs font-semibold text-primary-foreground transition-colors hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
        onclick={() => void save()}
        disabled={loading || saving || !canSave}
      >
        {saving ? t("officeEditor_saving") : t("officeEditor_save")}
      </button>
    </div>
  </div>

  {#if loading}
    <div class="flex min-h-0 flex-1 items-center justify-center text-sm text-muted-foreground">
      {t("officeEditor_loading")}
    </div>
  {:else if error}
    <div class="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
      <p class="text-sm font-semibold text-red-500">{t("officeEditor_errorTitle")}</p>
      <p class="max-w-lg text-xs leading-5 text-muted-foreground">{error}</p>
      <button
        type="button"
        class="rounded-lg border border-border px-3 py-1.5 text-xs font-medium hover:bg-accent"
        onclick={() => (error = "")}
      >
        {t("officeEditor_dismissError")}
      </button>
    </div>
  {:else if preview?.kind === "xlsx"}
    <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div class="shrink-0 border-b border-border/70 bg-card/50 px-3 py-2">
        <div class="flex min-w-0 items-center gap-1 overflow-x-auto" role="tablist">
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
        <p class="mt-2 text-[11px] leading-5 text-muted-foreground">
          {t("officeEditor_xlsxModeNote", {
            rows: String(MAX_OFFICE_EDITOR_ROWS),
            columns: String(MAX_OFFICE_EDITOR_COLUMNS),
          })}
        </p>
      </div>

      {#if activeSheet}
        <div class="min-h-0 flex-1 overflow-auto p-3 sm:p-5">
          <table class="min-w-full border-separate border-spacing-0 text-xs">
            <thead>
              <tr>
                <th
                  class="sticky left-0 top-0 z-20 w-12 border-b border-r border-border bg-muted px-2 py-2 text-center font-mono text-[10px] text-muted-foreground"
                  >#</th
                >
                {#each Array.from({ length: editorColumnCount }) as _, column}
                  <th
                    class="sticky top-0 z-10 min-w-32 border-b border-r border-border bg-muted px-2 py-2 text-left font-medium text-muted-foreground"
                  >
                    {t("officeEditor_column", { number: String(column + 1) })}
                  </th>
                {/each}
              </tr>
            </thead>
            <tbody>
              {#each Array.from({ length: editorRowCount }) as _, row}
                <tr>
                  <th
                    class="sticky left-0 z-10 border-b border-r border-border bg-muted/70 px-2 py-1.5 text-center font-mono text-[10px] font-normal text-muted-foreground"
                    >{row + 1}</th
                  >
                  {#each Array.from({ length: editorColumnCount }) as _, column}
                    <td class="border-b border-r border-border/70 bg-background p-0">
                      <input
                        class="min-h-9 w-full min-w-32 bg-transparent px-2 py-1.5 text-xs text-foreground outline-none focus:bg-primary/5 focus:ring-1 focus:ring-inset focus:ring-primary"
                        value={cellValue(activeSheet.name, row + 1, column + 1)}
                        aria-label={`${activeSheet.name} ${row + 1}, ${column + 1}`}
                        oninput={(event) =>
                          setCellValue(
                            activeSheet?.name ?? "",
                            row + 1,
                            column + 1,
                            (event.currentTarget as HTMLInputElement).value,
                          )}
                      />
                    </td>
                  {/each}
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <div
          class="flex min-h-0 flex-1 items-center justify-center p-6 text-xs text-muted-foreground"
        >
          {t("officeEditor_empty")}
        </div>
      {/if}
    </div>
  {:else if preview?.kind === "docx" || preview?.kind === "pptx"}
    <div class="min-h-0 flex-1 overflow-auto p-4 sm:p-6">
      <div class="mx-auto max-w-2xl space-y-5">
        <div
          class="rounded-xl border border-primary/20 bg-primary/5 px-4 py-3 text-xs leading-5 text-muted-foreground"
        >
          {t("officeEditor_textModeNote")}
        </div>

        <div class="grid gap-4 sm:grid-cols-2">
          <label class="block text-xs font-medium text-foreground">
            {t("officeEditor_findLabel")}
            <input
              class="mt-1.5 min-h-10 w-full rounded-lg border border-border bg-background px-3 text-sm text-foreground outline-none placeholder:text-muted-foreground/60 focus:border-primary focus:ring-2 focus:ring-primary/20"
              bind:value={findText}
              placeholder={t("officeEditor_findPlaceholder")}
            />
          </label>
          <label class="block text-xs font-medium text-foreground">
            {t("officeEditor_replaceLabel")}
            <input
              class="mt-1.5 min-h-10 w-full rounded-lg border border-border bg-background px-3 text-sm text-foreground outline-none placeholder:text-muted-foreground/60 focus:border-primary focus:ring-2 focus:ring-primary/20"
              bind:value={replaceText}
              placeholder={t("officeEditor_replacePlaceholder")}
            />
          </label>
        </div>

        <label class="flex items-center gap-2 text-xs text-muted-foreground">
          <input type="checkbox" bind:checked={replaceAll} class="h-4 w-4 rounded border-border" />
          {t("officeEditor_replaceAll")}
        </label>

        {#if preview.kind === "pptx"}
          <div class="space-y-3">
            {#each preview.slides as slide (slide.number)}
              <div class="rounded-xl border border-border/70 bg-card/60 p-4">
                <p class="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                  {t("officeEditor_slideLabel", { number: String(slide.number) })}
                </p>
                <div class="mt-2 space-y-1 text-sm leading-6 text-foreground">
                  {#each slide.paragraphs as paragraph}
                    <p>{paragraph}</p>
                  {/each}
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <p class="text-xs leading-5 text-muted-foreground">{t("officeEditor_docxContext")}</p>
        {/if}
      </div>
    </div>
  {:else}
    <div class="flex min-h-0 flex-1 items-center justify-center p-6 text-xs text-muted-foreground">
      {t("officeEditor_empty")}
    </div>
  {/if}

  {#if saved}
    <div
      class="shrink-0 border-t border-emerald-500/20 bg-emerald-500/5 px-4 py-2 text-xs text-emerald-700 dark:text-emerald-300"
      role="status"
    >
      {t("officeEditor_saved")}
    </div>
  {/if}
</div>
