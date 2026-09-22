<script lang="ts">
  import type { CapabilityCenterItem, CapabilityCenterProjection } from "$lib/types/work";
  import CapabilityReadinessBadge from "./CapabilityReadinessBadge.svelte";

  interface Props {
    projection: CapabilityCenterProjection | null;
    loading?: boolean;
    onAction?: (actionType: string, item: CapabilityCenterItem) => void;
    onSelectItem?: (item: CapabilityCenterItem) => void;
    onFilterReadiness?: (readiness: string | null) => void;
    activeFilter?: string | null;
  }

  let {
    projection,
    loading = false,
    onAction,
    onSelectItem,
    onFilterReadiness,
    activeFilter = null,
  }: Props = $props();

  let overview = $derived(projection?.overview);

  // Filter items that require attention first, or highlight top items
  let attentionItems = $derived.by(() => {
    if (!projection?.items) return [];
    return projection.items.filter(
      (item) => item.readiness !== "ready" && item.readiness !== "not_installed",
    );
  });

  let showAllAttention = $state(false);
  let visibleAttentionItems = $derived(
    showAllAttention ? attentionItems : attentionItems.slice(0, 6),
  );
</script>

<div class="rounded-xl border border-border/70 bg-card/60 p-4 shadow-sm backdrop-blur-sm">
  <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
    <div>
      <h2 class="text-base font-semibold tracking-tight text-foreground flex items-center gap-2">
        <span>能力就绪概览</span>
        {#if loading}
          <span
            class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-primary border-t-transparent"
          ></span>
        {/if}
      </h2>
      <p class="text-xs text-muted-foreground mt-0.5">
        清楚掌握当前环境可用能力、未就绪原因与快速配置入口
      </p>
    </div>

    <!-- Overview Counters -->
    {#if overview}
      <div class="flex flex-wrap items-center gap-2 text-xs">
        <button
          type="button"
          class="flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 transition-all {activeFilter ===
          'ready'
            ? 'border-emerald-500 bg-emerald-500/15 font-semibold text-emerald-600 dark:text-emerald-400 ring-1 ring-emerald-500/30'
            : 'border-border/60 bg-muted/30 hover:bg-muted/60 text-muted-foreground'}"
          onclick={() => onFilterReadiness?.(activeFilter === "ready" ? null : "ready")}
        >
          <span class="h-2 w-2 rounded-full bg-emerald-500"></span>
          <span>Ready</span>
          <span class="font-mono font-medium text-foreground">{overview.readyCount}</span>
        </button>

        <button
          type="button"
          class="flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 transition-all {activeFilter ===
          'needs_setup'
            ? 'border-orange-500 bg-orange-500/15 font-semibold text-orange-600 dark:text-orange-400 ring-1 ring-orange-500/30'
            : 'border-border/60 bg-muted/30 hover:bg-muted/60 text-muted-foreground'}"
          onclick={() => onFilterReadiness?.(activeFilter === "needs_setup" ? null : "needs_setup")}
        >
          <span class="h-2 w-2 rounded-full bg-orange-500"></span>
          <span>Needs Setup</span>
          <span class="font-mono font-medium text-foreground">{overview.needsSetupCount}</span>
        </button>

        <button
          type="button"
          class="flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 transition-all {activeFilter ===
          'needs_auth'
            ? 'border-amber-500 bg-amber-500/15 font-semibold text-amber-600 dark:text-amber-400 ring-1 ring-amber-500/30'
            : 'border-border/60 bg-muted/30 hover:bg-muted/60 text-muted-foreground'}"
          onclick={() => onFilterReadiness?.(activeFilter === "needs_auth" ? null : "needs_auth")}
        >
          <span class="h-2 w-2 rounded-full bg-amber-500"></span>
          <span>Needs Login</span>
          <span class="font-mono font-medium text-foreground">{overview.needsAuthCount}</span>
        </button>

        <button
          type="button"
          class="flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 transition-all {activeFilter ===
          'unavailable'
            ? 'border-zinc-500 bg-zinc-500/15 font-semibold text-zinc-600 dark:text-zinc-400 ring-1 ring-zinc-500/30'
            : 'border-border/60 bg-muted/30 hover:bg-muted/60 text-muted-foreground'}"
          onclick={() => onFilterReadiness?.(activeFilter === "unavailable" ? null : "unavailable")}
        >
          <span class="h-2 w-2 rounded-full bg-zinc-400"></span>
          <span>Unavailable</span>
          <span class="font-mono font-medium text-foreground">{overview.unavailableCount}</span>
        </button>
      </div>
    {/if}
  </div>

  <!-- Needs Attention Quick Fix Bar (if any items need auth or setup) -->
  {#if attentionItems.length > 0 && !activeFilter}
    <div class="mt-3.5 border-t border-border/50 pt-3">
      <div class="flex items-center justify-between mb-2">
        <span class="text-xs font-medium text-muted-foreground"
          >待处理能力 ({attentionItems.length})</span
        >
        {#if attentionItems.length > 6}
          <button
            type="button"
            class="text-[11px] font-medium text-primary hover:underline transition-colors"
            onclick={() => (showAllAttention = !showAllAttention)}
          >
            {showAllAttention ? "收起" : `展开全部 (${attentionItems.length})`}
          </button>
        {/if}
      </div>
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
        {#each visibleAttentionItems as item (item.id)}
          <div
            class="flex items-center justify-between gap-2 rounded-lg border border-border/60 bg-background/50 px-3 py-2 text-xs transition-colors hover:border-border hover:bg-background/80"
          >
            <div class="min-w-0 flex-1 cursor-pointer" onclick={() => onSelectItem?.(item)}>
              <div class="flex items-center gap-1.5">
                <span class="font-medium text-foreground truncate">{item.name}</span>
                <CapabilityReadinessBadge readiness={item.readiness} size="sm" showLabel={false} />
              </div>
              <p class="truncate text-[11px] text-muted-foreground mt-0.5">
                {item.readinessReason}
              </p>
            </div>

            {#if item.actions.length > 0}
              <button
                type="button"
                class="shrink-0 rounded bg-primary/10 px-2 py-1 text-[11px] font-medium text-primary hover:bg-primary/20 transition-colors"
                onclick={(e) => {
                  e.stopPropagation();
                  onAction?.(item.actions[0].actionType, item);
                }}
              >
                {item.actions[0].label}
              </button>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>
