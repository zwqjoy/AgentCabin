<script lang="ts">
  import type { WorkArtifactSummary, WorkResultPresentation } from "$lib/types/work";
  import { formatArtifactStatus, formatArtifactType, formatFileSize } from "$lib/utils/work-result";

  interface Props {
    presentation: WorkResultPresentation;
    artifacts?: WorkArtifactSummary[];
    readOnly?: boolean;
    onPreview: (artifact: WorkArtifactSummary) => void;
    onExport: (artifactId: string) => Promise<void>;
    onOpenAllArtifacts?: () => void;
    /** 提供后在失败/取消态显示「继续任务」按钮。 */
    onContinue?: () => void;
    continueBusy?: boolean;
    onShowDetails?: (artifact: WorkArtifactSummary) => void;
    /** 提供后在卡片右上角显示「收起」按钮 */
    onCollapse?: () => void;
  }

  let {
    presentation,
    artifacts = [],
    readOnly = false,
    onPreview,
    onExport,
    onOpenAllArtifacts,
    onContinue,
    continueBusy = false,
    onShowDetails,
    onCollapse,
  }: Props = $props();

  let exportingId = $state("");
  let exportError = $state("");

  async function handleExport(artifactId: string) {
    if (exportingId) return;
    exportingId = artifactId;
    exportError = "";
    try {
      await onExport(artifactId);
    } catch (cause) {
      exportError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      exportingId = "";
    }
  }

  let primary = $derived(presentation.primaryArtifact);
  let canExportPrimary = $derived(Boolean(primary?.status === "delivered"));
  let isCompleted = $derived(presentation.outcome === "completed");
  let isFailed = $derived(presentation.outcome === "failed");
  let isCancelled = $derived(presentation.outcome === "cancelled");
</script>

<div
  class="w-full rounded-2xl border bg-card/60 p-4.5 sm:p-5 shadow-xs transition-colors backdrop-blur-xs {isCompleted
    ? 'border-emerald-500/30 dark:border-emerald-500/20 bg-emerald-500/[0.02]'
    : isFailed
      ? 'border-amber-500/30 dark:border-amber-500/20 bg-amber-500/[0.02]'
      : 'border-border/80'}"
>
  <!-- Header: Outcome badge & Title -->
  <div class="flex flex-wrap items-center justify-between gap-2.5">
    <div class="flex items-center gap-2">
      {#if isCompleted}
        <span
          class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
          aria-hidden="true"
        >
          <svg
            class="h-3.5 w-3.5"
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
          class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-amber-500/15 text-amber-600 dark:text-amber-400"
          aria-hidden="true"
        >
          <svg
            class="h-3.5 w-3.5"
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
      {:else if isCancelled}
        <span
          class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground"
          aria-hidden="true"
        >
          <svg
            class="h-3.5 w-3.5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <circle cx="12" cy="12" r="10" />
            <line x1="4.93" y1="4.93" x2="19.07" y2="19.07" />
          </svg>
        </span>
      {/if}
      <h3
        class="text-sm font-semibold {isCompleted
          ? 'text-emerald-700 dark:text-emerald-300'
          : isFailed
            ? 'text-amber-700 dark:text-amber-300'
            : 'text-foreground'}"
      >
        {presentation.title}
      </h3>
    </div>

    <!-- Badges / Metrics / Collapse -->
    <div class="flex flex-wrap items-center gap-1.5 text-[11px]">
      {#if presentation.totalSteps > 0}
        <span class="rounded-full bg-muted/80 px-2.5 py-0.5 font-medium text-muted-foreground">
          {presentation.completedSteps}/{presentation.totalSteps} 步骤完成
        </span>
      {/if}
      {#if presentation.artifactCount > 0}
        <span class="rounded-full bg-primary/10 px-2.5 py-0.5 font-medium text-primary">
          {presentation.artifactCount} 个成果文件{#if presentation.durationFormatted}
            · {presentation.durationFormatted}{/if}
        </span>
      {:else if presentation.durationFormatted}
        <span class="rounded-full bg-muted/80 px-2.5 py-0.5 font-medium text-muted-foreground">
          耗时 {presentation.durationFormatted}
        </span>
      {/if}
      {#if onCollapse}
        <button
          type="button"
          class="inline-flex items-center gap-1 rounded-md border border-border/60 bg-background/80 px-2 py-0.5 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          onclick={onCollapse}
          title="收起为轻量摘要"
        >
          <span>收起</span>
          <svg
            class="h-3 w-3"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <polyline points="18 15 12 9 6 15" />
          </svg>
        </button>
      {/if}
    </div>
  </div>

  <!-- Goal / Subtitle -->
  {#if presentation.goal}
    <div class="mt-2 text-sm font-medium text-foreground/95">
      {presentation.goal}
    </div>
  {/if}

  {#if presentation.subtitle}
    <p class="mt-1 text-xs leading-5 text-muted-foreground">
      {presentation.subtitle}
    </p>
  {/if}

  <!-- Collaboration summary (if available) -->
  {#if presentation.collaborationSummary}
    <div
      class="mt-2.5 inline-flex items-center gap-1.5 rounded-md bg-accent/60 px-2.5 py-1 text-[11px] text-muted-foreground"
    >
      <svg
        class="h-3.5 w-3.5 text-primary/80"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
        <circle cx="9" cy="7" r="4" />
        <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
        <path d="M16 3.13a4 4 0 0 1 0 7.75" />
      </svg>
      <span>{presentation.collaborationSummary}</span>
    </div>
  {/if}

  <!-- Failure / interruption reason (factual summary only) -->
  {#if (isFailed || isCancelled) && (presentation.error || presentation.failedStep)}
    <div
      class="mt-2.5 rounded-lg border {isFailed
        ? 'border-red-400/20 bg-red-400/5'
        : 'border-border/60 bg-muted/40'} px-3 py-2"
    >
      {#if presentation.failedStep}
        <div
          class="text-[11px] font-medium {isFailed
            ? 'text-red-600 dark:text-red-400'
            : 'text-muted-foreground'}"
        >
          中断于：{presentation.failedStep}
        </div>
      {/if}
      {#if presentation.error}
        <p
          class="mt-1 max-h-20 overflow-y-auto whitespace-pre-wrap break-all text-[11px] leading-5 {isFailed
            ? 'text-red-600/90 dark:text-red-300/90'
            : 'text-muted-foreground'}"
        >
          {presentation.error}
        </p>
      {/if}
    </div>
  {/if}

  <!-- Continue / retry entry for non-completed terminal states -->
  {#if (isFailed || isCancelled) && onContinue && !readOnly && !continueBusy}
    <div class="mt-3 flex justify-end">
      <button
        type="button"
        class="inline-flex items-center gap-1.5 rounded-lg border border-primary/40 bg-primary/10 px-3 py-1.5 text-xs font-semibold text-primary transition-colors hover:bg-primary/20"
        onclick={onContinue}
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
          <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" />
        </svg>
        继续任务
      </button>
    </div>
  {:else if (isFailed || isCancelled) && continueBusy}
    <div class="mt-3 flex justify-end">
      <span
        class="inline-flex items-center gap-1.5 rounded-lg border border-primary/40 bg-primary/10 px-3 py-1.5 text-xs font-semibold text-primary opacity-60"
      >
        <span class="h-3 w-3 animate-spin rounded-full border-2 border-primary/20 border-t-primary"
        ></span>
        继续中…
      </span>
    </div>
  {/if}

  <!-- Primary Artifact Section -->
  {#if primary}
    <div class="mt-3.5">
      <div
        class="mb-1.5 flex items-center justify-between text-[11px] font-medium text-muted-foreground"
      >
        {#if isFailed || isCancelled}
          <span class="font-semibold text-amber-600 dark:text-amber-400">
            部分成果（任务中断前生成）
          </span>
        {:else}
          <span>主要成果</span>
        {/if}
        {#if artifacts.length > 1 && onOpenAllArtifacts}
          <button type="button" class="text-primary hover:underline" onclick={onOpenAllArtifacts}>
            查看全部 {artifacts.length} 个成果 ›
          </button>
        {/if}
      </div>

      {#if isFailed || isCancelled}
        <p class="mb-2 text-[11px] text-amber-600/90 dark:text-amber-400/90">
          注意：以下文件为任务中断前生成的中间产物，未通过全量交付验收，不可视为完整交付结果。
        </p>
      {/if}

      <div
        class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 rounded-xl border border-border/80 bg-background/90 p-3 shadow-2xs hover:border-border transition-colors"
      >
        <!-- Artifact Details -->
        <button
          type="button"
          class="flex min-w-0 flex-1 items-start gap-2.5 text-left focus:outline-hidden"
          onclick={() => onPreview(primary)}
        >
          <span
            class="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary"
            aria-hidden="true"
          >
            <svg
              class="h-4 w-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
              <polyline points="14 2 14 8 20 8" />
              <line x1="16" y1="13" x2="8" y2="13" />
              <line x1="16" y1="17" x2="8" y2="17" />
            </svg>
          </span>

          <div class="min-w-0 flex-1">
            <div
              class="truncate text-xs font-semibold text-foreground hover:text-primary transition-colors"
            >
              {primary.title}
            </div>
            <div
              class="mt-0.5 flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground"
            >
              <span>{formatArtifactType(primary.artifactType, primary.path)}</span>
              <span>·</span>
              <span>{formatFileSize(primary.size)}</span>
              <span>·</span>
              <span
                class="rounded px-1.5 py-0.2 text-[10px] font-medium {primary.status ===
                  'delivered' || primary.status === 'validated'
                  ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
                  : 'bg-muted text-muted-foreground'}"
                title={formatArtifactStatus(primary.status).tooltip}
              >
                {formatArtifactStatus(primary.status).label}
              </span>
            </div>
          </div>
        </button>

        <!-- Action Buttons -->
        <div class="flex shrink-0 items-center gap-2 self-end sm:self-center">
          <button
            type="button"
            class="inline-flex items-center gap-1 rounded-lg border border-border/80 bg-card px-2.5 py-1.5 text-xs font-medium text-foreground shadow-2xs hover:bg-accent hover:text-accent-foreground transition-colors"
            onclick={() => onPreview(primary)}
          >
            <svg
              class="h-3.5 w-3.5 text-muted-foreground"
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

          {#if onShowDetails}
            <button
              type="button"
              class="inline-flex items-center gap-1 rounded-lg border border-border/80 bg-card px-2.5 py-1.5 text-xs font-medium text-foreground shadow-2xs hover:bg-accent hover:text-accent-foreground transition-colors"
              onclick={() => onShowDetails?.(primary)}
              title="查看产物溯源与验证详情"
            >
              <svg
                class="h-3.5 w-3.5 text-muted-foreground"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <circle cx="12" cy="12" r="10" />
                <line x1="12" y1="16" x2="12" y2="12" />
                <line x1="12" y1="8" x2="12.01" y2="8" />
              </svg>
              详情
            </button>
          {/if}

          {#if canExportPrimary}
            <button
              type="button"
              class="inline-flex items-center gap-1 rounded-lg border border-border/80 bg-card px-2.5 py-1.5 text-xs font-medium text-foreground shadow-2xs hover:bg-accent hover:text-accent-foreground disabled:opacity-50 transition-colors"
              disabled={exportingId === primary.id}
              onclick={() => handleExport(primary.id)}
            >
              <svg
                class="h-3.5 w-3.5 text-muted-foreground"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                <polyline points="7 10 12 15 17 10" />
                <line x1="12" y1="15" x2="12" y2="3" />
              </svg>
              {exportingId === primary.id ? "导出中…" : "导出"}
            </button>
          {/if}
        </div>
      </div>

      {#if exportError}
        <div
          class="mt-2 rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
          role="alert"
        >
          导出失败：{exportError}
        </div>
      {/if}
    </div>
  {:else if presentation.artifactCount > 0 && onOpenAllArtifacts}
    <!-- If there are artifacts but none eligible as primary (e.g. invalid) -->
    <div
      class="mt-3 flex items-center justify-between rounded-xl border border-border/70 bg-background/60 p-3"
    >
      <span class="text-xs text-muted-foreground">
        已生成 {presentation.artifactCount} 个文件（{presentation.problematicCount} 个异常）
      </span>
      <button
        type="button"
        class="text-xs font-medium text-primary hover:underline"
        onclick={onOpenAllArtifacts}
      >
        查看成果列表 ›
      </button>
    </div>
  {/if}
</div>
