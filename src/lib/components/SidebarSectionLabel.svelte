<script lang="ts">
  import type { Snippet } from "svelte";

  type Props = {
    label: string;
    count?: number;
    actions?: Snippet;
    class?: string;
  };

  let { label, count, actions, class: className = "" }: Props = $props();
</script>

<div class="flex items-center justify-between px-2.5 py-1 {className}">
  <div class="flex items-center gap-1.5">
    <span class="text-xs font-semibold text-sidebar-foreground/45">{label}</span>
    {#if count !== undefined && count > 0}
      <span class="text-[10px] font-medium text-sidebar-foreground/45">{count}</span>
    {/if}
  </div>
  {#if actions}
    <div class="flex items-center gap-0.5">
      {@render actions()}
    </div>
  {/if}
</div>
