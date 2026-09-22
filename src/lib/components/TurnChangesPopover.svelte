<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "$lib/i18n/index.svelte";
  import type { UnifiedDiffSummary } from "$lib/utils/diff-stats";

  type Props = {
    summary: UnifiedDiffSummary;
    onViewDiff?: () => void;
  };

  let { summary, onViewDiff }: Props = $props();

  let open = $state(false);
  let wrapperEl: HTMLDivElement | undefined = $state();

  let filesLabel = $derived(
    summary.files.length === 1
      ? t("sidebar_changedFile", { count: "1" })
      : t("sidebar_changedFiles", { count: String(summary.files.length) }),
  );

  function toggle() {
    open = !open;
  }

  function viewDiff() {
    open = false;
    onViewDiff?.();
  }

  onMount(() => {
    function onDocumentPointerDown(event: MouseEvent) {
      if (open && wrapperEl && !wrapperEl.contains(event.target as Node)) open = false;
    }

    function onDocumentKeydown(event: KeyboardEvent) {
      if (open && event.key === "Escape") open = false;
    }

    document.addEventListener("mousedown", onDocumentPointerDown, true);
    document.addEventListener("keydown", onDocumentKeydown);
    return () => {
      document.removeEventListener("mousedown", onDocumentPointerDown, true);
      document.removeEventListener("keydown", onDocumentKeydown);
    };
  });
</script>

<div bind:this={wrapperEl} class="relative">
  <button
    type="button"
    class="flex max-w-[min(22rem,calc(100vw-1.5rem))] items-center gap-1.5 rounded-full border border-border bg-background/95 px-2.5 py-1 text-xs font-medium text-foreground shadow-sm backdrop-blur transition-colors hover:bg-muted/70 {open
      ? 'bg-muted/70'
      : ''}"
    aria-expanded={open}
    aria-haspopup="dialog"
    title={t("diff_turnDiff")}
    onclick={toggle}
  >
    <svg
      class="h-3.5 w-3.5 shrink-0 text-muted-foreground"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.7"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path d="M9 3v18M3 9h18M3 15h18" />
    </svg>
    <span class="truncate">{filesLabel}</span>
    <span class="shrink-0 font-mono text-[11px] tabular-nums text-emerald-600 dark:text-emerald-400"
      >+{summary.totalInsertions}</span
    >
    <span class="shrink-0 font-mono text-[11px] tabular-nums text-red-500 dark:text-red-400"
      >-{summary.totalDeletions}</span
    >
    <svg
      class="h-3 w-3 shrink-0 text-muted-foreground/70 transition-transform {open
        ? 'rotate-180'
        : ''}"
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
  </button>

  {#if open}
    <div
      class="absolute right-0 top-full z-50 mt-2 w-[min(24rem,calc(100vw-1.5rem))] overflow-hidden rounded-xl border border-border/80 bg-popover text-popover-foreground shadow-xl backdrop-blur-md animate-in fade-in zoom-in-95 duration-150"
      role="dialog"
      aria-label={t("diff_turnDiff")}
      tabindex="-1"
    >
      <div class="flex items-center gap-2 border-b border-border/70 px-3 py-2.5">
        <span class="text-xs font-semibold">{t("diff_turnDiff")}</span>
        <span class="ml-auto flex items-center gap-2 font-mono text-[11px] tabular-nums">
          <span class="text-emerald-600 dark:text-emerald-400">+{summary.totalInsertions}</span>
          <span class="text-red-500 dark:text-red-400">-{summary.totalDeletions}</span>
        </span>
        {#if onViewDiff}
          <button
            type="button"
            class="rounded-md px-1.5 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            onclick={viewDiff}
            title={t("diff_viewFull")}
          >
            {t("diff_viewFull")}
          </button>
        {/if}
      </div>

      <div class="max-h-[min(22rem,60vh)] overflow-y-auto py-1">
        {#each summary.files as file (file.path)}
          <div
            class="flex items-center gap-2 px-3 py-2 text-xs transition-colors hover:bg-accent/50"
            title={file.path}
          >
            <svg
              class="h-3.5 w-3.5 shrink-0 text-muted-foreground/70"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.7"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
              <path d="M14 2v6h6M8 13h8M8 17h6" />
            </svg>
            <span class="min-w-0 flex-1 truncate font-mono text-[11px]">{file.path}</span>
            <span
              class="shrink-0 font-mono text-[11px] tabular-nums text-emerald-600 dark:text-emerald-400"
              >+{file.insertions}</span
            >
            <span class="shrink-0 font-mono text-[11px] tabular-nums text-red-500 dark:text-red-400"
              >-{file.deletions}</span
            >
          </div>
        {/each}
      </div>

      <div
        class="flex items-center justify-between border-t border-border/70 px-3 py-2 text-[11px] text-muted-foreground"
      >
        <span>{filesLabel}</span>
        <span class="font-mono tabular-nums"
          >+{summary.totalInsertions} -{summary.totalDeletions}</span
        >
      </div>
    </div>
  {/if}
</div>
