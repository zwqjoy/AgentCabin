<script lang="ts">
  import type { WorkArtifactAcceptance } from "$lib/types/work";

  interface Props {
    acceptance: WorkArtifactAcceptance | null;
    loading?: boolean;
    onRefresh?: () => void;
    onViewArtifact?: (artifactId: string) => void;
  }

  let { acceptance, loading = false, onRefresh, onViewArtifact }: Props = $props();
</script>

<div class="rounded-xl border border-amber-500/30 bg-amber-500/5 p-4 shadow-sm">
  <div class="flex items-center justify-between pb-3 border-b border-amber-500/20">
    <div class="flex items-center gap-2.5">
      <div
        class="flex h-7 w-7 items-center justify-center rounded-lg bg-amber-500 text-white shadow-sm"
      >
        <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="12" cy="12" r="10" /><line x1="12" y1="8" x2="12" y2="12" /><line
            x1="12"
            y1="16"
            x2="12.01"
            y2="16"
          />
        </svg>
      </div>
      <div>
        <h3 class="text-sm font-bold text-foreground flex items-center gap-2">
          <span>交付验收检查 (Waiting Delivery)</span>
          {#if loading}
            <span
              class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-primary border-t-transparent"
            ></span>
          {/if}
        </h3>
        <p class="text-xs text-muted-foreground mt-0.5">
          Agent 已完成执行，正等待所有必需成果通过验收检查
        </p>
      </div>
    </div>

    {#if acceptance}
      <div class="flex items-center gap-2">
        <span
          class="rounded-md bg-background px-2.5 py-1 text-xs font-mono font-bold border border-border/60 {acceptance.satisfied
            ? 'text-emerald-600 dark:text-emerald-400'
            : 'text-amber-600 dark:text-amber-400'}"
        >
          {acceptance.satisfiedCount} / {acceptance.requiredCount} 完成
        </span>
        {#if onRefresh}
          <button
            type="button"
            class="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
            onclick={onRefresh}
            title="刷新验收状态"
          >
            <svg
              class="h-4 w-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path
                d="M3 3v5h5"
              /><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" /><path
                d="M16 21h5v-5"
              />
            </svg>
          </button>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Acceptance Checks List -->
  {#if acceptance}
    <div class="mt-3 space-y-2 text-xs">
      {#each acceptance.checks as check (check.requirement.path)}
        <div
          class="flex items-center justify-between rounded-lg border border-border/60 bg-background/70 px-3 py-2 transition-colors"
        >
          <div class="flex items-center gap-2.5 min-w-0">
            {#if check.status === "satisfied"}
              <span
                class="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 font-bold"
              >
                ✓
              </span>
            {:else if check.status === "invalid"}
              <span
                class="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-rose-500/10 text-rose-600 dark:text-rose-400 font-bold"
              >
                ⚠
              </span>
            {:else}
              <span
                class="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-amber-500/10 text-amber-600 dark:text-amber-400 font-bold"
              >
                ✗
              </span>
            {/if}

            <div class="min-w-0">
              <span class="font-mono font-medium text-foreground truncate block">
                {check.requirement.path}
              </span>
              <span class="text-[11px] text-muted-foreground block truncate">
                {check.message}
              </span>
            </div>
          </div>

          <div class="flex items-center gap-2 shrink-0">
            {#if check.status === "satisfied"}
              <span
                class="rounded bg-emerald-500/10 px-2 py-0.5 text-[11px] font-medium text-emerald-600 dark:text-emerald-400 border border-emerald-500/20"
              >
                已就绪
              </span>
              {#if check.artifactId && onViewArtifact}
                <button
                  type="button"
                  class="text-[11px] text-primary hover:underline"
                  onclick={() => onViewArtifact(check.artifactId!)}
                >
                  查看
                </button>
              {/if}
            {:else if check.status === "invalid"}
              <span
                class="rounded bg-rose-500/10 px-2 py-0.5 text-[11px] font-medium text-rose-600 dark:text-rose-400 border border-rose-500/20"
              >
                验收未通过
              </span>
            {:else}
              <span
                class="rounded bg-amber-500/10 px-2 py-0.5 text-[11px] font-medium text-amber-600 dark:text-amber-400 border border-amber-500/20"
              >
                未生成 (Missing)
              </span>
            {/if}
          </div>
        </div>
      {/each}
    </div>

    {#if !acceptance.satisfied}
      <div
        class="mt-3 rounded-lg bg-amber-500/10 p-2.5 text-xs text-amber-800 dark:text-amber-200 border border-amber-500/20 flex items-center gap-2"
      >
        <svg
          class="h-4 w-4 shrink-0"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <circle cx="12" cy="12" r="10" /><line x1="12" y1="8" x2="12" y2="12" /><line
            x1="12"
            y1="16"
            x2="12.01"
            y2="16"
          />
        </svg>
        <span>
          尚缺 {acceptance.missingCount} 个交付物，{acceptance.invalidCount} 个未通过校验。任务暂不能标记为完成。
        </span>
      </div>
    {/if}
  {/if}
</div>
