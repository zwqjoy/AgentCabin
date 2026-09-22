<script module lang="ts">
  export type RailEntry = {
    id: string;
    anchorId: string;
    preview: string;
    responsePreview?: string;
    isRunning?: boolean;
    turnIndex: number;
  };
</script>

<script lang="ts">
  type MeasuredMarker = {
    entry: RailEntry;
    percent: number;
    scrollY: number;
    active: boolean;
  };

  type Props = {
    container: HTMLElement | null;
    entries: RailEntry[];
    entryAttribute?: string;
    side?: "left" | "right";
    onSelect?: (entry: RailEntry) => void | Promise<void>;
    class?: string;
  };

  let {
    container = null,
    entries = [],
    entryAttribute = "data-entry-id",
    side = "right",
    onSelect,
    class: className = "",
  }: Props = $props();

  let railRoot = $state<HTMLElement | null>(null);
  let track = $state<HTMLDivElement | null>(null);
  let trackLayout = $state({ top: 0, height: 0 });
  let markers = $state<MeasuredMarker[]>([]);
  let hoveredMarkerId = $state<string | null>(null);
  let focusedMarkerId = $state<string | null>(null);
  let measureFrame: number | null = null;

  const clamp = (value: number, min: number, max: number): number =>
    Math.min(max, Math.max(min, value));

  function scheduleMeasure() {
    if (measureFrame !== null || typeof window === "undefined") return;
    measureFrame = window.requestAnimationFrame(() => {
      measureFrame = null;
      measure();
    });
  }

  function getMountedElements(): Map<string, HTMLElement> {
    if (!container) return new Map();
    const mounted = new Map<string, HTMLElement>();
    for (const element of container.querySelectorAll<HTMLElement>(`[${entryAttribute}]`)) {
      const id = element.getAttribute(entryAttribute);
      if (id) mounted.set(id, element);
    }
    return mounted;
  }

  function measure() {
    if (!container || !railRoot || !track || entries.length === 0) {
      markers = [];
      return;
    }

    const scrollHeight = Math.max(container.scrollHeight, 1);
    const clientHeight = Math.max(container.clientHeight, 1);
    const containerRect = container.getBoundingClientRect();
    const railRect = railRoot.getBoundingClientRect();
    const stickyHeader = container.querySelector<HTMLElement>(".sticky.top-0");
    const stickyHeaderBottom = stickyHeader?.getBoundingClientRect().bottom ?? containerRect.top;
    const trackInset = 16;
    const trackTop = Math.max(trackInset, stickyHeaderBottom - railRect.top + 8);
    trackLayout = {
      top: trackTop,
      height: Math.max(0, clientHeight - trackTop - trackInset),
    };
    const mounted = getMountedElements();

    const measured = entries.map((entry, index) => {
      const element = mounted.get(entry.id);
      const fallbackY =
        entries.length > 1 ? (index / (entries.length - 1)) * Math.max(scrollHeight - 1, 0) : 0;
      const scrollY = element
        ? clamp(
            element.getBoundingClientRect().top - containerRect.top + container.scrollTop,
            0,
            scrollHeight,
          )
        : fallbackY;

      return {
        entry,
        scrollY,
        percent: clamp((scrollY / scrollHeight) * 100, 0, 100),
      };
    });

    const activeY = container.scrollTop + clientHeight * 0.28;
    let closestIndex = 0;
    let closestDistance = Number.POSITIVE_INFINITY;
    measured.forEach((marker, index) => {
      const distance = Math.abs(marker.scrollY - activeY);
      if (distance < closestDistance) {
        closestDistance = distance;
        closestIndex = index;
      }
    });

    markers = measured.map((marker, index) => ({
      ...marker,
      active: index === closestIndex,
    }));
  }

  $effect(() => {
    const node = container;
    if (!node) {
      markers = [];
      return;
    }

    const onScroll = () => scheduleMeasure();
    node.addEventListener("scroll", onScroll, { passive: true });

    const resizeObserver =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(scheduleMeasure);
    resizeObserver?.observe(node);
    if (railRoot) resizeObserver?.observe(railRoot);
    if (track) resizeObserver?.observe(track);

    const mutationObserver =
      typeof MutationObserver === "undefined" ? null : new MutationObserver(scheduleMeasure);
    mutationObserver?.observe(node, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["class", "style"],
    });

    scheduleMeasure();

    return () => {
      node.removeEventListener("scroll", onScroll);
      resizeObserver?.disconnect();
      mutationObserver?.disconnect();
      if (measureFrame !== null && typeof window !== "undefined") {
        window.cancelAnimationFrame(measureFrame);
        measureFrame = null;
      }
    };
  });
</script>

{#if entries.length > 1}
  <nav
    bind:this={railRoot}
    class="pointer-events-none absolute inset-y-0 z-30 hidden w-8 select-none sm:block {side ===
    'right'
      ? 'right-0'
      : 'left-0'} {className}"
    aria-label="历史对话轮次导航"
    data-export-exclude
  >
    <div
      bind:this={track}
      class="absolute inset-x-0"
      style={`top: ${trackLayout.top}px; height: ${trackLayout.height}px`}
    >
      {#each markers as marker (marker.entry.id)}
        {@const isHovered =
          hoveredMarkerId === marker.entry.id || focusedMarkerId === marker.entry.id}
        {@const isBusy = Boolean(marker.entry.isRunning)}
        {@const tickStyle = marker.active
          ? "w-[20px] bg-foreground"
          : isHovered
            ? "w-[16px] bg-foreground/75"
            : "w-[10px] bg-muted-foreground/35"}

        <button
          type="button"
          class="group pointer-events-auto absolute h-5 w-7 -translate-y-1/2 outline-none cursor-pointer flex items-center justify-end pr-1 {side ===
          'right'
            ? 'right-0'
            : 'left-0'}"
          style={`top: ${marker.percent}%`}
          aria-label={`第 ${marker.entry.turnIndex} 轮对话`}
          aria-current={marker.active ? "location" : undefined}
          onmouseenter={() => (hoveredMarkerId = marker.entry.id)}
          onmouseleave={() => (hoveredMarkerId = null)}
          onfocus={() => (focusedMarkerId = marker.entry.id)}
          onblur={() => (focusedMarkerId = null)}
          onclick={() => onSelect?.(marker.entry)}
        >
          <!-- DSH Turn Mark Tick -->
          <span
            class="h-[2px] rounded-xs transition-all duration-150 ease-out {tickStyle} {isBusy
              ? 'animate-[dsh-turn-mark-busy_1s_infinite]'
              : ''}"
          ></span>

          <!-- DSH Hover Preview Card -->
          {#if isHovered}
            <div
              class="pointer-events-none absolute top-1/2 -translate-y-1/2 z-50 w-64 max-w-[calc(100vw-120px)] rounded-xl border border-border/80 bg-popover/95 p-3 text-left shadow-xl backdrop-blur-md animate-[dsh-turn-preview-enter_0.12s_ease-out] {side ===
              'right'
                ? 'right-[calc(100%+8px)]'
                : 'left-[calc(100%+8px)]'}"
            >
              <div
                class="flex items-center justify-between text-[11px] text-muted-foreground/70 pb-1 mb-1 border-b border-border/40 font-mono"
              >
                <span>第 {marker.entry.turnIndex} 轮</span>
                {#if isBusy}
                  <span class="text-blue-500 font-medium animate-pulse">执行中</span>
                {/if}
              </div>
              <p class="text-xs font-medium text-foreground line-clamp-1 leading-snug">
                {marker.entry.preview.replace(/\s+/g, " ").trim() || "用户消息"}
              </p>
              {#if marker.entry.responsePreview}
                <p class="text-[11px] text-muted-foreground/80 line-clamp-2 leading-relaxed mt-1">
                  {marker.entry.responsePreview.replace(/\s+/g, " ").trim()}
                </p>
              {/if}
            </div>
          {/if}
        </button>
      {/each}
    </div>
  </nav>
{/if}
