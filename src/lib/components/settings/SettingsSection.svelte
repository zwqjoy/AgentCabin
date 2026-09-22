<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    description?: string;
    class?: string;
    children?: Snippet;
    action?: Snippet;
  }

  let { title, description = "", class: className = "", children, action }: Props = $props();
</script>

<section class="space-y-3 {className}">
  <div class="flex items-end justify-between gap-4 pb-1">
    <div>
      <h2 class="text-sm font-semibold text-foreground uppercase tracking-wider">{title}</h2>
      {#if description}
        <p class="mt-0.5 text-xs text-muted-foreground">{description}</p>
      {/if}
    </div>
    {#if action}
      <div class="shrink-0">
        {@render action()}
      </div>
    {/if}
  </div>

  {#if children}
    <div class="space-y-4">
      {@render children()}
    </div>
  {/if}
</section>
