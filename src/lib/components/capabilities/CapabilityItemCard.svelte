<script lang="ts">
  import type { CapabilityCenterItem } from "$lib/types/work";
  import CapabilityReadinessBadge from "./CapabilityReadinessBadge.svelte";

  interface Props {
    item: CapabilityCenterItem;
    onAction?: (actionType: string, item: CapabilityCenterItem) => void;
    onSelect?: (item: CapabilityCenterItem) => void;
    actionBusy?: boolean;
  }

  let { item, onAction, onSelect, actionBusy = false }: Props = $props();

  const categoryIcon = $derived.by(() => {
    switch (item.category) {
      case "browser":
        return "globe";
      case "app":
        return "grid";
      case "connector":
        return "link";
      case "mcp":
        return "cpu";
      case "skill":
      default:
        return "zap";
    }
  });

  const isReady = $derived(item.readiness === "ready");
</script>

<div
  class="group relative flex flex-col justify-between rounded-xl border border-border/70 bg-card p-4 transition-all hover:border-border hover:shadow-md"
  onclick={() => onSelect?.(item)}
>
  <!-- Card Header -->
  <div>
    <div class="flex items-start justify-between gap-2 mb-2">
      <div class="flex items-center gap-2 min-w-0">
        <div
          class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary"
        >
          {#if categoryIcon === "globe"}
            <svg
              class="h-4 w-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle cx="12" cy="12" r="10" /><path
                d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"
              /><path d="M2 12h20" />
            </svg>
          {:else if categoryIcon === "grid"}
            <svg
              class="h-4 w-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <rect width="7" height="7" x="3" y="3" rx="1" /><rect
                width="7"
                height="7"
                x="14"
                y="3"
                rx="1"
              /><rect width="7" height="7" x="14" y="14" rx="1" /><rect
                width="7"
                height="7"
                x="3"
                y="14"
                rx="1"
              />
            </svg>
          {:else if categoryIcon === "link"}
            <svg
              class="h-4 w-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" /><path
                d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"
              />
            </svg>
          {:else if categoryIcon === "cpu"}
            <svg
              class="h-4 w-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <rect width="16" height="16" x="4" y="4" rx="2" /><rect
                width="6"
                height="6"
                x="9"
                y="9"
                rx="1"
              /><path d="M15 2v2" /><path d="M15 20v2" /><path d="M2 15h2" /><path
                d="M2 9h2"
              /><path d="M20 15h2" /><path d="M20 9h2" /><path d="M9 2v2" /><path d="M9 20v2" />
            </svg>
          {:else}
            <svg
              class="h-4 w-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
            </svg>
          {/if}
        </div>
        <div class="min-w-0">
          <h3
            class="truncate text-sm font-semibold text-foreground group-hover:text-primary transition-colors"
          >
            {item.name}
          </h3>
          <span class="text-[11px] font-mono text-muted-foreground uppercase tracking-wider">
            {item.category}
          </span>
        </div>
      </div>

      <CapabilityReadinessBadge readiness={item.readiness} size="sm" />
    </div>

    <!-- Description -->
    <p class="line-clamp-2 text-xs text-muted-foreground mb-3 leading-relaxed">
      {item.description || "暂无描述"}
    </p>

    <!-- State Distinction Matrix: Installed / Enabled / Ready -->
    <div class="rounded-lg bg-muted/40 p-2 text-[11px] space-y-1 mb-3">
      <div class="flex items-center justify-between text-muted-foreground">
        <span>Installed</span>
        {#if item.installed}
          <span class="text-emerald-600 dark:text-emerald-400 font-medium">✓ 已安装</span>
        {:else}
          <span class="text-zinc-400">✗ 未安装</span>
        {/if}
      </div>

      <div class="flex items-center justify-between text-muted-foreground">
        <span>Enabled</span>
        {#if item.enabled}
          <span class="text-emerald-600 dark:text-emerald-400 font-medium">✓ 已启用</span>
        {:else}
          <span class="text-amber-500 font-medium">○ 未启用</span>
        {/if}
      </div>

      <div class="flex items-center justify-between text-muted-foreground">
        <span>Ready</span>
        {#if isReady}
          <span class="text-emerald-600 dark:text-emerald-400 font-medium">✓ 就绪</span>
        {:else}
          <span class="text-rose-500 font-medium">✗ 待处理</span>
        {/if}
      </div>

      {#if item.auth}
        <div
          class="flex items-center justify-between text-muted-foreground pt-0.5 border-t border-border/30"
        >
          <span>Auth</span>
          {#if item.auth.status === "connected" || item.auth.status === "authenticated"}
            <span class="text-emerald-600 dark:text-emerald-400 truncate max-w-[130px]">
              ✓ {item.auth.accounts?.[0] || "已连接"}
            </span>
          {:else}
            <span class="text-amber-500">✗ 需认证</span>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Readiness Reason Note -->
    {#if !isReady}
      <div
        class="mb-3 rounded-md bg-amber-500/10 px-2 py-1 text-[11px] text-amber-700 dark:text-amber-300"
      >
        {item.readinessReason}
      </div>
    {/if}
  </div>

  <!-- Card Actions -->
  <div class="flex items-center justify-between gap-2 pt-2 border-t border-border/50">
    <button
      type="button"
      class="text-xs text-muted-foreground hover:text-foreground transition-colors"
      onclick={(e) => {
        e.stopPropagation();
        onSelect?.(item);
      }}
    >
      详情
    </button>

    {#if item.actions.length > 0}
      <div class="flex items-center gap-1.5">
        {#each item.actions as action}
          <button
            type="button"
            class="rounded-lg px-2.5 py-1 text-xs font-medium transition-colors {action.actionType ===
              'enable' ||
            action.actionType === 'connect' ||
            action.actionType === 'install_chromium'
              ? 'bg-primary text-primary-foreground hover:bg-primary/90'
              : 'bg-muted text-muted-foreground hover:text-foreground hover:bg-muted/80'}"
            disabled={actionBusy}
            onclick={(e) => {
              e.stopPropagation();
              onAction?.(action.actionType, item);
            }}
          >
            {action.label}
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>
