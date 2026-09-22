<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title?: string;
    description?: string;
    class?: string;
    headerClass?: string;
    children?: Snippet;
    action?: Snippet;
  }

  let {
    title = "",
    description = "",
    class: className = "",
    headerClass = "",
    children,
    action,
  }: Props = $props();
</script>

<div class="space-y-2 {className}">
  {#if title || description || action}
    <div class="flex items-start justify-between gap-4 px-1 pb-0.5 {headerClass}">
      <div class="min-w-0">
        {#if title}
          <h3 class="text-sm font-semibold text-foreground tracking-tight">{title}</h3>
        {/if}
        {#if description}
          <p class="mt-0.5 text-xs text-muted-foreground leading-normal">{description}</p>
        {/if}
      </div>
      {#if action}
        <div class="shrink-0">
          {@render action()}
        </div>
      {/if}
    </div>
  {/if}

  <div
    class="rounded-xl border border-border/70 bg-card shadow-xs overflow-hidden divide-y divide-border/40"
  >
    {#if children}
      {@render children()}
    {/if}
  </div>
</div>
