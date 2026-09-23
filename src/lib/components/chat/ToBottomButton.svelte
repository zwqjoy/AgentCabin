<script lang="ts">
  let {
    visible = false,
    onClick,
  }: {
    visible?: boolean;
    onClick?: () => void;
  } = $props();
</script>

{#if visible}
  <div
    class="pointer-events-none sticky bottom-4 z-20 flex h-0 w-full justify-end pr-6 select-none animate-fade-in"
  >
    <button
      type="button"
      class="pointer-events-auto -mt-9 flex h-8 w-8 items-center justify-center rounded-full border border-border/80 bg-background/95 text-foreground/80 shadow-md backdrop-blur-md transition-all duration-150 hover:bg-muted hover:text-foreground hover:scale-105 active:scale-95 cursor-pointer"
      onclick={() => onClick?.()}
      aria-label="回到底部"
      title="回到底部"
    >
      <svg
        class="h-4 w-4"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <polyline points="6 9 12 15 18 9" />
      </svg>
    </button>
  </div>
{/if}
