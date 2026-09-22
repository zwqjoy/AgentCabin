<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import type { UnifiedDiffSummary } from "$lib/utils/diff-stats";

  type Props = {
    summary: UnifiedDiffSummary;
    onUndo?: () => void;
    onReview?: () => void;
    onDismiss?: () => void;
    undoBusy?: boolean;
  };

  let { summary, onUndo, onReview, onDismiss, undoBusy = false }: Props = $props();

  let expanded = $state(false);

  const COLLAPSED_LIMIT = 3;
  const visibleFiles = $derived(expanded ? summary.files : summary.files.slice(0, COLLAPSED_LIMIT));
  const hiddenCount = $derived(expanded ? 0 : Math.max(0, summary.files.length - COLLAPSED_LIMIT));

  const filesLabel = $derived(
    summary.files.length === 1
      ? t("turnSummary_editedFile", { count: "1" })
      : t("turnSummary_editedFiles", { count: String(summary.files.length) }),
  );
</script>

<div
  class="turn-summary-banner chat-content-width animate-in fade-in slide-in-from-bottom-2 duration-300"
  data-export-exclude
>
  <div
    class="overflow-hidden rounded-xl border border-border/80 bg-card/95 shadow-sm backdrop-blur-sm"
  >
    <!-- Header row -->
    <div class="flex min-h-16 items-center gap-3 px-4 py-3 sm:px-5">
      <!-- File edit icon -->
      <div
        class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground"
      >
        <svg
          class="h-5 w-5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.7"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <rect x="4" y="3" width="16" height="18" rx="3" />
          <path d="M12 8v8M8 12h8" />
        </svg>
      </div>

      <!-- Label + stats -->
      <div class="flex-1 min-w-0">
        <div class="truncate text-sm font-semibold text-foreground">{filesLabel}</div>
        <div class="mt-0.5 font-mono text-xs tabular-nums">
          <span class="text-emerald-600 dark:text-emerald-400">+{summary.totalInsertions}</span>
          <span class="mx-1 text-muted-foreground/40">−</span>
          <span class="text-red-500 dark:text-red-400">-{summary.totalDeletions}</span>
        </div>
      </div>

      <div class="flex shrink-0 items-center gap-1.5">
        {#if onUndo}
          <button
            type="button"
            class="inline-flex h-9 items-center gap-1.5 rounded-lg px-2.5 text-sm font-medium text-foreground transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
            onclick={onUndo}
            disabled={undoBusy}
            title={t("turnSummary_undo")}
          >
            <span>{undoBusy ? t("turnSummary_undoing") : t("turnSummary_undo")}</span>
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
              <path d="M9 14 4 9l5-5" />
              <path d="M4 9h10a6 6 0 0 1 6 6v1" />
            </svg>
          </button>
        {/if}

        {#if onReview}
          <button
            type="button"
            class="inline-flex h-9 items-center rounded-lg border border-border bg-background px-3 text-sm font-semibold text-foreground shadow-sm transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            onclick={onReview}
            title={t("toolbar_reviewTitle")}
          >
            {t("toolbar_review")}
          </button>
        {/if}

        <!-- Dismiss button -->
        {#if onDismiss}
          <button
            type="button"
            class="flex h-9 w-9 items-center justify-center rounded-lg text-muted-foreground/60 transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            onclick={onDismiss}
            title={t("turnSummary_dismiss")}
            aria-label={t("turnSummary_dismiss")}
          >
            <svg
              class="h-4 w-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="M18 6 6 18M6 6l12 12" />
            </svg>
          </button>
        {/if}
      </div>
    </div>

    <!-- File list -->
    <div class="border-t border-border/60">
      {#each visibleFiles as file (file.path)}
        <div
          class="flex min-h-11 items-center gap-3 px-4 py-2 text-sm transition-colors hover:bg-muted/25 sm:px-5"
          title={file.path}
        >
          <span class="min-w-0 flex-1 truncate text-foreground/80">
            {file.path}
          </span>
          <span
            class="shrink-0 font-mono text-sm tabular-nums text-emerald-600 dark:text-emerald-400"
          >
            +{file.insertions}
          </span>
          <span class="shrink-0 font-mono text-sm tabular-nums text-red-500 dark:text-red-400">
            -{file.deletions}
          </span>
        </div>
      {/each}

      {#if hiddenCount > 0}
        <button
          type="button"
          class="mx-4 mb-2 mt-1 flex min-h-9 items-center gap-1 rounded-md px-2 text-xs text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground sm:mx-5"
          onclick={() => (expanded = true)}
        >
          <svg
            class="h-3 w-3"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="m6 9 6 6 6-6" />
          </svg>
          {t("turnSummary_showMore", { count: String(hiddenCount) })}
        </button>
      {/if}
    </div>
  </div>
</div>
