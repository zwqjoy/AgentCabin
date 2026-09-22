<script lang="ts">
  import type { Snippet } from "svelte";

  type Props = {
    href?: string;
    active?: boolean;
    label: string;
    title?: string;
    icon?: Snippet;
    badge?: number | string;
    badgeColor?: "default" | "amber";
    dotIndicator?: boolean;
    onclick?: (e: MouseEvent) => void;
    class?: string;
  };

  let {
    href,
    active = false,
    label,
    title,
    icon,
    badge,
    badgeColor = "default",
    dotIndicator = false,
    onclick,
    class: className = "",
  }: Props = $props();

  const activeClasses = "bg-sidebar-accent text-sidebar-foreground font-semibold";
  const inactiveClasses =
    "text-sidebar-foreground/75 hover:bg-sidebar-accent/50 hover:text-sidebar-foreground";
  const baseClasses =
    "ui-sidebar-nav-item flex items-center justify-between rounded-lg px-2.5 py-1.5 text-xs font-medium transition-colors select-none";
</script>

{#if href}
  <a
    {href}
    {onclick}
    title={title ?? label}
    aria-label={title ?? label}
    class="{baseClasses} {active ? activeClasses : inactiveClasses} {className}"
  >
    <div class="flex min-w-0 items-center gap-2">
      {#if icon}
        {@render icon()}
      {/if}
      <span class="truncate">{label}</span>
    </div>
    <div class="flex shrink-0 items-center gap-1.5">
      {#if dotIndicator}
        <span class="h-1.5 w-1.5 rounded-full bg-emerald-500"></span>
      {/if}
      {#if badge !== undefined && (typeof badge === "number" ? badge > 0 : badge.length > 0)}
        {#if badgeColor === "amber"}
          <span
            class="rounded-full bg-amber-500/20 px-1.5 py-0.2 text-[10px] font-bold text-amber-500"
          >
            {badge}
          </span>
        {:else}
          <span
            class="rounded-full bg-sidebar-foreground/10 px-1.5 py-px text-[10px] font-medium text-sidebar-foreground/60"
          >
            {badge}
          </span>
        {/if}
      {/if}
    </div>
  </a>
{:else}
  <button
    type="button"
    {onclick}
    title={title ?? label}
    aria-label={title ?? label}
    class="{baseClasses} w-full {active ? activeClasses : inactiveClasses} {className}"
  >
    <div class="flex min-w-0 items-center gap-2">
      {#if icon}
        {@render icon()}
      {/if}
      <span class="truncate">{label}</span>
    </div>
    <div class="flex shrink-0 items-center gap-1.5">
      {#if dotIndicator}
        <span class="h-1.5 w-1.5 rounded-full bg-emerald-500"></span>
      {/if}
      {#if badge !== undefined && (typeof badge === "number" ? badge > 0 : badge.length > 0)}
        {#if badgeColor === "amber"}
          <span
            class="rounded-full bg-amber-500/20 px-1.5 py-0.2 text-[10px] font-bold text-amber-500"
          >
            {badge}
          </span>
        {:else}
          <span
            class="rounded-full bg-sidebar-foreground/10 px-1.5 py-px text-[10px] font-medium text-sidebar-foreground/60"
          >
            {badge}
          </span>
        {/if}
      {/if}
    </div>
  </button>
{/if}
