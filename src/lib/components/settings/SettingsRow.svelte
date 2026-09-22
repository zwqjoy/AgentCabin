<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    description?: string;
    class?: string;
    disabled?: boolean;
    clickable?: boolean;
    onclick?: () => void;
    children?: Snippet;
    icon?: Snippet;
    badge?: Snippet;
    id?: string;
  }

  let {
    title,
    description = "",
    class: className = "",
    disabled = false,
    clickable = false,
    onclick,
    children,
    icon,
    badge,
    id,
  }: Props = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  {id}
  class="flex items-center justify-between gap-4 px-4 py-3.5 sm:px-5 sm:py-4 transition-colors min-h-[64px] {clickable &&
  !disabled
    ? 'cursor-pointer hover:bg-muted/30 active:bg-muted/50'
    : ''} {disabled ? 'opacity-50 pointer-events-none' : ''} {className}"
  onclick={clickable && !disabled ? onclick : undefined}
>
  <div class="flex items-start gap-3 min-w-0 flex-1 pr-4">
    {#if icon}
      <div class="mt-0.5 shrink-0 text-muted-foreground">
        {@render icon()}
      </div>
    {/if}
    <div class="min-w-0">
      <div class="flex items-center gap-2">
        <span class="text-[13px] font-medium text-foreground tracking-tight">{title}</span>
        {#if badge}
          {@render badge()}
        {/if}
      </div>
      {#if description}
        <p class="mt-0.5 text-xs text-muted-foreground leading-relaxed">{description}</p>
      {/if}
    </div>
  </div>

  {#if children}
    <div class="shrink-0 flex items-center gap-2">
      {@render children()}
    </div>
  {/if}
</div>
