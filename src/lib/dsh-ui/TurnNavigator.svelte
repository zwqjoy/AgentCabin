<script module lang="ts">
  export interface TurnRailItem {
    turnIndex: number;
    promptText?: string;
    responseSnippet?: string;
    anchorId?: string;
    isRunning?: boolean;
  }
</script>

<script lang="ts">
  let {
    items = [],
    activeTurn = 0,
    busyTurn = null,
    onNavigate,
  }: {
    items: TurnRailItem[];
    activeTurn?: number;
    busyTurn?: number | null;
    onNavigate?: (turnIndex: number, anchorId?: string) => void;
  } = $props();

  let hoveredTurn = $state<number | null>(null);

  function handleMarkClick(item: TurnRailItem, event: MouseEvent) {
    event.stopPropagation();
    onNavigate?.(item.turnIndex, item.anchorId);
  }
</script>

{#if items.length > 1}
  <aside
    class="pointer-events-none sticky top-0 z-20 h-0 w-full select-none"
    aria-label="Turn navigation rail"
  >
    <div
      class="pointer-events-auto absolute right-3 top-1/2 -translate-y-1/2 flex flex-col items-end py-2 px-1"
    >
      <div class="relative flex flex-col items-end gap-2">
        {#each items as item (item.turnIndex)}
          {@const isActive = activeTurn === item.turnIndex}
          {@const isBusy = busyTurn === item.turnIndex || Boolean(item.isRunning)}
          {@const isHovered = hoveredTurn === item.turnIndex}
          {@const tickStyle = isActive
            ? "w-[20px] bg-foreground"
            : isHovered
              ? "w-[16px] bg-foreground/70"
              : "w-[10px] bg-muted-foreground/35"}

          <div
            class="relative flex items-center justify-end h-4 w-7 cursor-pointer group"
            onmouseenter={() => (hoveredTurn = item.turnIndex)}
            onmouseleave={() => {
              if (hoveredTurn === item.turnIndex) hoveredTurn = null;
            }}
            onclick={(e) => handleMarkClick(item, e)}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onNavigate?.(item.turnIndex, item.anchorId);
              }
            }}
            role="button"
            tabindex="0"
            aria-label={`第 ${item.turnIndex + 1} 轮对话`}
          >
            <!-- Turn Mark Tick (DSH: 2px height, 10px normal, 16px hover, 20px active) -->
            <span
              class="h-[2px] rounded-xs transition-all duration-150 ease-out {tickStyle} {isBusy
                ? 'animate-[dsh-turn-mark-busy_1s_infinite]'
                : ''}"
            ></span>

            <!-- Turn Preview Floating Card (DSH style: left offset, blur, prompt & response) -->
            {#if isHovered && (item.promptText || item.responseSnippet)}
              <div
                class="pointer-events-none absolute right-[calc(100%+8px)] top-1/2 -translate-y-1/2 w-64 max-w-[calc(100vw-120px)] rounded-xl border border-border/80 bg-background/95 p-2.5 shadow-xl backdrop-blur-md animate-[dsh-turn-preview-enter_0.12s_ease-out] z-30"
              >
                <div
                  class="flex items-center justify-between text-[11px] text-muted-foreground/70 pb-1 mb-1 border-b border-border/40 font-mono"
                >
                  <span>第 {item.turnIndex + 1} 轮</span>
                  {#if isBusy}
                    <span class="text-blue-500 font-medium animate-pulse">执行中</span>
                  {/if}
                </div>
                {#if item.promptText}
                  <p class="text-xs font-medium text-foreground line-clamp-1 leading-snug">
                    {item.promptText}
                  </p>
                {/if}
                {#if item.responseSnippet}
                  <p class="text-[11px] text-muted-foreground/80 line-clamp-2 leading-relaxed mt-1">
                    {item.responseSnippet}
                  </p>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  </aside>
{/if}
