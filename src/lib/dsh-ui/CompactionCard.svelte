<script lang="ts">
  import { formatExactTokens } from "$lib/utils/dsh-format";
  import { currentLocale } from "$lib/i18n/index.svelte";

  let {
    shadowedItemCount = 0,
    shadowedTokenCount = 0,
    summary = "",
  }: {
    shadowedItemCount?: number;
    shadowedTokenCount?: number;
    summary?: string;
  } = $props();

  let expanded = $state(false);
  let isEn = $derived(Boolean(currentLocale() && currentLocale().startsWith("en")));
</script>

<div class="w-full my-2 select-none">
  <div
    class="rounded-xl border border-border/60 bg-muted/30 px-3 py-2 text-xs text-muted-foreground/80 transition-colors"
  >
    <div
      class="flex items-center justify-between cursor-pointer"
      onclick={() => {
        if (summary) expanded = !expanded;
      }}
      role="button"
      tabindex="0"
      onkeydown={(e) => {
        if ((e.key === "Enter" || e.key === " ") && summary) {
          e.preventDefault();
          expanded = !expanded;
        }
      }}
    >
      <div class="flex items-center gap-2">
        <svg
          class="h-3.5 w-3.5 text-muted-foreground/70 shrink-0"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M21 8v13H3V8" />
          <path d="M1 3h22v5H1z" />
          <path d="M10 12h4" />
        </svg>
        <span class="font-medium text-foreground/90">
          {isEn ? "Context compacted" : "上下文已压缩"}
        </span>
        {#if shadowedItemCount > 0 || shadowedTokenCount > 0}
          <span class="text-muted-foreground/50">·</span>
          <span class="text-[11px] text-muted-foreground/70 font-mono">
            {isEn
              ? `${shadowedItemCount} items, ${formatExactTokens(shadowedTokenCount)} tokens`
              : `${shadowedItemCount} 项，${formatExactTokens(shadowedTokenCount)} tokens`}
          </span>
        {/if}
      </div>

      {#if summary}
        <svg
          class="h-3.5 w-3.5 text-muted-foreground/50 transition-transform duration-150 {expanded
            ? 'rotate-180'
            : ''}"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      {/if}
    </div>

    {#if expanded && summary}
      <div
        class="mt-2 pt-2 border-t border-border/40 text-[11px] leading-relaxed text-muted-foreground whitespace-pre-wrap font-mono"
      >
        {summary}
      </div>
    {/if}
  </div>
</div>
