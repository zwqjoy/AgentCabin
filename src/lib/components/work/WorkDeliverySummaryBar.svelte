<script lang="ts">
  import type { WorkArtifactSummary, WorkResultPresentation } from "$lib/types/work";

  interface Props {
    presentation: WorkResultPresentation;
    primaryArtifact?: WorkArtifactSummary;
    artifactCount?: number;
    onPreview?: (artifact: WorkArtifactSummary) => void;
    onToggleExpand: () => void;
    isExpanded?: boolean;
  }

  let {
    presentation,
    primaryArtifact,
    artifactCount = 0,
    onPreview,
    onToggleExpand,
    isExpanded = false,
  }: Props = $props();

  let effectiveArtifactCount = $derived(
    artifactCount > 0 ? artifactCount : presentation.artifactCount || 0,
  );
  let effectivePrimary = $derived(primaryArtifact ?? presentation.primaryArtifact);
  let isCompleted = $derived(presentation.outcome === "completed");
  let isFailed = $derived(presentation.outcome === "failed");
  let isCancelled = $derived(presentation.outcome === "cancelled");
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="group flex w-full cursor-pointer select-none items-center justify-between gap-3 rounded-xl border px-3.5 py-2 text-xs shadow-2xs backdrop-blur-xs transition-all duration-200 {isCompleted
    ? 'border-emerald-500/30 bg-emerald-500/[0.04] hover:bg-emerald-500/[0.07] dark:border-emerald-500/25'
    : isFailed
      ? 'border-amber-500/30 bg-amber-500/[0.04] hover:bg-amber-500/[0.07] dark:border-amber-500/25'
      : 'border-border/80 bg-card/60 hover:bg-accent/40'}"
  onclick={onToggleExpand}
  title={isExpanded ? "点击收起交付卡片" : "点击展开完整成果卡片"}
>
  <!-- Left: Status Icon & Concise Info -->
  <div class="flex min-w-0 flex-1 items-center gap-2.5">
    {#if isCompleted}
      <span
        class="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
        aria-hidden="true"
      >
        <svg
          class="h-3 w-3"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2.5"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <polyline points="20 6 9 17 4 12" />
        </svg>
      </span>
    {:else if isFailed}
      <span
        class="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-amber-500/15 text-amber-600 dark:text-amber-400"
        aria-hidden="true"
      >
        <svg
          class="h-3 w-3"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2.5"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="12" cy="12" r="10" />
          <line x1="12" y1="8" x2="12" y2="12" />
          <line x1="12" y1="16" x2="12.01" y2="16" />
        </svg>
      </span>
    {:else}
      <span
        class="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground"
        aria-hidden="true"
      >
        <svg
          class="h-3 w-3"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="12" cy="12" r="10" />
          <line x1="4.93" y1="4.93" x2="19.07" y2="19.07" />
        </svg>
      </span>
    {/if}

    <div class="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-0.5">
      <span
        class="font-semibold {isCompleted
          ? 'text-emerald-700 dark:text-emerald-300'
          : isFailed
            ? 'text-amber-700 dark:text-amber-300'
            : 'text-foreground'}"
      >
        {presentation.title || "任务已完成"}
      </span>

      {#if effectiveArtifactCount > 0}
        <span class="text-muted-foreground/60">·</span>
        <span class="text-muted-foreground">
          已交付 {effectiveArtifactCount} 个成果文件
        </span>
      {/if}

      {#if effectivePrimary}
        <span class="hidden text-muted-foreground/60 sm:inline">·</span>
        <span
          class="hidden max-w-[200px] truncate font-medium text-foreground/90 sm:inline lg:max-w-[320px]"
          title={effectivePrimary.title}
        >
          {effectivePrimary.title}
        </span>
      {/if}
    </div>
  </div>

  <!-- Right: Quick Actions -->
  <div class="flex shrink-0 items-center gap-1.5" onclick={(e) => e.stopPropagation()}>
    {#if effectivePrimary && onPreview}
      <button
        type="button"
        class="inline-flex items-center gap-1 rounded-md border border-border/80 bg-background/80 px-2 py-1 text-[11px] font-medium text-foreground shadow-2xs transition-colors hover:bg-accent hover:text-accent-foreground"
        onclick={() => onPreview(effectivePrimary)}
        title="快速预览主要成果文件"
      >
        <svg
          class="h-3 w-3 text-muted-foreground"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
          <circle cx="12" cy="12" r="3" />
        </svg>
        预览
      </button>
    {/if}

    <button
      type="button"
      class="inline-flex items-center gap-1 rounded-md px-1.5 py-1 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      onclick={onToggleExpand}
      aria-label={isExpanded ? "收起" : "展开"}
    >
      <span>{isExpanded ? "收起" : "详情"}</span>
      <svg
        class="h-3 w-3 transition-transform duration-200 {isExpanded ? 'rotate-180' : ''}"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2.5"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <polyline points="6 9 12 15 18 9" />
      </svg>
    </button>
  </div>
</div>
