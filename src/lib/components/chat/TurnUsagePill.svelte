<script lang="ts">
  import type { TurnUsage } from "$lib/stores/types";
  import {
    formatTokens,
    formatExactTokens,
    formatCacheHitPercent,
    formatRunDuration,
    formatLatencySeconds,
  } from "$lib/utils/chat-format";
  import { currentLocale } from "$lib/i18n/index.svelte";

  let {
    usage,
    model = "",
    durationMs = 0,
    timestamp = "",
  }: {
    usage?: TurnUsage | null;
    model?: string;
    durationMs?: number;
    timestamp?: string;
  } = $props();

  let isEn = $derived(Boolean(currentLocale() && currentLocale().startsWith("en")));

  // Popover state
  let usageOpen = $state(false);
  let timeOpen = $state(false);

  let usageRootRef: HTMLElement | null = $state(null);
  let timeRootRef: HTMLElement | null = $state(null);
  let usagePanelRef: HTMLElement | null = $state(null);
  let timePanelRef: HTMLElement | null = $state(null);

  let usagePos = $state<{ bottom: number; left: number } | null>(null);
  let timePos = $state<{ bottom: number; left: number } | null>(null);

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() {
        if (node.parentNode) {
          node.parentNode.removeChild(node);
        }
      },
    };
  }

  function updateUsagePos() {
    if (!usageRootRef) return;
    const rect = usageRootRef.getBoundingClientRect();
    const bottom = Math.max(8, window.innerHeight - rect.top + 6);
    const panelWidth = 320;
    const left = Math.max(16, Math.min(rect.left, window.innerWidth - panelWidth - 16));
    usagePos = { bottom, left };
  }

  function updateTimePos() {
    if (!timeRootRef) return;
    const rect = timeRootRef.getBoundingClientRect();
    const bottom = Math.max(8, window.innerHeight - rect.top + 6);
    const panelWidth = 280;
    const left = Math.max(16, Math.min(rect.left, window.innerWidth - panelWidth - 16));
    timePos = { bottom, left };
  }

  function toggleUsage(e: MouseEvent) {
    e.stopPropagation();
    timeOpen = false;
    if (!usageOpen) {
      updateUsagePos();
      usageOpen = true;
    } else {
      usageOpen = false;
    }
  }

  function toggleTime(e: MouseEvent) {
    e.stopPropagation();
    usageOpen = false;
    if (!timeOpen) {
      updateTimePos();
      timeOpen = true;
    } else {
      timeOpen = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      usageOpen = false;
      timeOpen = false;
    }
  }

  function handleClickOutside(e: MouseEvent | PointerEvent) {
    const target = e.target as Node | null;
    if (usageOpen) {
      if (usageRootRef?.contains(target) || usagePanelRef?.contains(target)) return;
      usageOpen = false;
    }
    if (timeOpen) {
      if (timeRootRef?.contains(target) || timePanelRef?.contains(target)) return;
      timeOpen = false;
    }
  }

  $effect(() => {
    if (typeof window === "undefined") return;
    if (usageOpen || timeOpen) {
      const handleScrollOrResize = () => {
        if (usageOpen) updateUsagePos();
        if (timeOpen) updateTimePos();
      };
      window.addEventListener("scroll", handleScrollOrResize, true);
      window.addEventListener("resize", handleScrollOrResize);
      window.addEventListener("keydown", handleKeydown);
      window.addEventListener("pointerdown", handleClickOutside);
      return () => {
        window.removeEventListener("scroll", handleScrollOrResize, true);
        window.removeEventListener("resize", handleScrollOrResize);
        window.removeEventListener("keydown", handleKeydown);
        window.removeEventListener("pointerdown", handleClickOutside);
      };
    }
  });

  // Derived usage breakdown
  const inputTokens = $derived(usage?.inputTokens ?? 0);
  const outputTokens = $derived(usage?.outputTokens ?? 0);
  const cacheReadTokens = $derived(usage?.cacheReadTokens ?? 0);
  const cacheWriteTokens = $derived(usage?.cacheWriteTokens ?? 0);
  const totalTokens = $derived(inputTokens + outputTokens + cacheReadTokens);
  const uncachedInput = $derived(Math.max(0, inputTokens));
  const promptTotal = $derived(inputTokens + cacheReadTokens);

  const cacheHit = $derived.by(() => {
    if (!usage || promptTotal <= 0) return null;
    return formatCacheHitPercent(cacheReadTokens, promptTotal, 1);
  });

  const effectiveDurationMs = $derived.by(() => {
    if (usage?.durationMs && usage.durationMs > 0) return usage.durationMs;
    if (durationMs > 0) return durationMs;
    return 0;
  });

  const tps = $derived.by(() => {
    if (outputTokens > 0 && effectiveDurationMs > 0) {
      const speed = outputTokens / (effectiveDurationMs / 1000);
      return speed >= 10 ? String(Math.round(speed)) : speed.toFixed(1);
    }
    return null;
  });

  const ttftDisplay = $derived.by(() => {
    if (usage?.ttftMs && usage.ttftMs > 0) {
      return formatLatencySeconds(usage.ttftMs, isEn);
    }
    // Estimated TTFT if API duration available
    if (
      usage?.durationApiMs &&
      usage.durationApiMs > 0 &&
      effectiveDurationMs > usage.durationApiMs
    ) {
      return formatLatencySeconds(usage.durationApiMs, isEn);
    }
    return null;
  });

  const timeFormatted = $derived(formatRunDuration(effectiveDurationMs, isEn));

  const formattedTimestamp = $derived.by(() => {
    if (!timestamp) return "";
    try {
      const d = new Date(timestamp);
      if (isNaN(d.getTime())) return "";
      return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
    } catch {
      return "";
    }
  });
</script>

<div
  class="inline-flex items-center gap-1.5 text-[12px] text-[var(--chat-text-tertiary,#9298a1)] select-none"
>
  <!-- Turn Usage Inline Metadata -->
  {#if totalTokens > 0}
    <span bind:this={usageRootRef} class="relative inline-flex">
      <button
        type="button"
        class="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[12px] text-[var(--chat-text-tertiary,#9298a1)] transition-colors hover:bg-muted/50 hover:text-foreground cursor-pointer {usageOpen
          ? 'bg-muted text-foreground'
          : ''}"
        aria-haspopup="dialog"
        aria-expanded={usageOpen}
        onclick={toggleUsage}
        title={isEn ? "Turn Usage" : "本轮用量"}
      >
        <span
          >{isEn
            ? `Usage ${formatTokens(totalTokens)} tok`
            : `用量 ${formatTokens(totalTokens)} tok`}</span
        >
      </button>

      <!-- Anchored Dialog: portaled to document.body at outermost layer -->
      {#if usageOpen}
        <div
          bind:this={usagePanelRef}
          use:portal
          role="dialog"
          aria-label={isEn ? "Turn Usage" : "本轮用量"}
          class="fixed z-[9999] min-w-[300px] max-w-[calc(100vw-32px)] sm:max-w-[420px] rounded-xl border border-border/60 bg-popover/95 p-4 text-xs shadow-xl backdrop-blur-md animate-fade-in"
          style="bottom: {usagePos?.bottom ?? 0}px; left: {usagePos?.left ?? 0}px;"
        >
          <!-- Header -->
          <div class="flex items-center justify-between font-medium text-foreground mb-2">
            <span class="inline-flex items-center gap-1.5 font-medium text-foreground">
              <svg
                class="h-3.5 w-3.5 text-foreground/80"
                viewBox="0 0 16 16"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
              >
                <ellipse
                  cx="8"
                  cy="3.6"
                  rx="5.75"
                  ry="2.4"
                  stroke="currentColor"
                  stroke-width="1.25"
                />
                <path
                  d="M2.25 3.6V12.3A5.75 2.4 0 0 0 13.75 12.3V3.6"
                  stroke="currentColor"
                  stroke-width="1.25"
                />
                <path
                  d="M2.25 7.95A5.75 2.4 0 0 0 13.75 7.95"
                  stroke="currentColor"
                  stroke-width="1.25"
                />
              </svg>
              {isEn ? "Turn Usage" : "本轮用量"}
            </span>
            <span class="font-mono tabular-nums text-foreground/80">
              {formatExactTokens(totalTokens)} tok
            </span>
          </div>

          <!-- Divider Rule -->
          <div class="border-t border-border/40 my-2.5"></div>

          <!-- Details Grid -->
          <dl
            class="grid grid-cols-[minmax(76px,auto)_minmax(0,1fr)] gap-x-4 gap-y-2 text-muted-foreground"
          >
            {#if model}
              <dt>{isEn ? "Provider / Model" : "提供方 / 模型"}</dt>
              <dd
                class="font-mono tabular-nums text-right text-foreground truncate max-w-[200px]"
                title={model}
              >
                {model}
              </dd>
            {/if}

            {#if cacheHit !== null}
              <dt>{isEn ? "Cache Hit" : "缓存命中"}</dt>
              <dd class="font-mono tabular-nums text-right text-foreground">{cacheHit}%</dd>
            {/if}

            <dt>{isEn ? "Uncached Input" : "未缓存输入"}</dt>
            <dd class="font-mono tabular-nums text-right text-foreground">
              {formatExactTokens(uncachedInput)} tok
            </dd>

            {#if cacheReadTokens > 0}
              <dt>{isEn ? "Cached Input" : "缓存读取"}</dt>
              <dd class="font-mono tabular-nums text-right text-foreground">
                {formatExactTokens(cacheReadTokens)} tok
              </dd>
            {/if}

            {#if cacheWriteTokens > 0}
              <dt>{isEn ? "Cache Write" : "缓存写入"}</dt>
              <dd class="font-mono tabular-nums text-right text-foreground">
                {formatExactTokens(cacheWriteTokens)} tok
              </dd>
            {/if}

            <dt>{isEn ? "Output" : "输出"}</dt>
            <dd class="font-mono tabular-nums text-right text-foreground">
              {formatExactTokens(outputTokens)} tok
              {#if usage?.reasoningTokens}
                <span class="text-muted-foreground font-normal ml-1">
                  ({isEn ? "reasoning" : "其中推理"}
                  {formatExactTokens(usage.reasoningTokens)})
                </span>
              {/if}
            </dd>
          </dl>
        </div>
      {/if}
    </span>
  {/if}

  <!-- Turn Time Inline Metadata -->
  {#if effectiveDurationMs > 0}
    <span bind:this={timeRootRef} class="relative inline-flex">
      <button
        type="button"
        class="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[12px] text-[var(--chat-text-tertiary,#9298a1)] transition-colors hover:bg-muted/50 hover:text-foreground cursor-pointer {timeOpen
          ? 'bg-muted text-foreground'
          : ''}"
        aria-haspopup="dialog"
        aria-expanded={timeOpen}
        onclick={toggleTime}
        title={isEn ? "Turn time and speed" : "本轮用时和速度"}
      >
        <span>{isEn ? `Ran for ${timeFormatted}` : `用时 ${timeFormatted}`}</span>
      </button>

      <!-- Anchored Time Dialog: portaled to document.body at outermost layer -->
      {#if timeOpen}
        <div
          bind:this={timePanelRef}
          use:portal
          role="dialog"
          aria-label={isEn ? "Turn time and speed" : "本轮用时和速度"}
          class="fixed z-[9999] min-w-[260px] max-w-[calc(100vw-32px)] sm:max-w-[340px] rounded-xl border border-border/60 bg-popover/95 p-4 text-xs shadow-xl backdrop-blur-md animate-fade-in"
          style="bottom: {timePos?.bottom ?? 0}px; left: {timePos?.left ?? 0}px;"
        >
          <!-- Header -->
          <div class="flex items-center justify-between font-medium text-foreground mb-2">
            <span class="inline-flex items-center gap-1.5 font-medium text-foreground">
              <svg
                class="h-3.5 w-3.5 text-foreground/80"
                viewBox="0 0 16 16"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
              >
                <circle cx="8" cy="8" r="6.375" stroke="currentColor" stroke-width="1.25" />
                <path d="M8 4.4V8.3L10.7 9.85" stroke="currentColor" stroke-width="1.25" />
              </svg>
              {isEn ? "Turn time and speed" : "本轮用时和速度"}
            </span>
          </div>

          <!-- Divider Rule -->
          <div class="border-t border-border/40 my-2.5"></div>

          <!-- Details Grid -->
          <dl
            class="grid grid-cols-[minmax(110px,auto)_minmax(0,1fr)] gap-x-4 gap-y-2 text-muted-foreground"
          >
            <dt>{isEn ? "Total Duration" : "本轮总用时"}</dt>
            <dd class="font-mono tabular-nums text-right text-foreground">{timeFormatted}</dd>

            {#if tps}
              <dt>{isEn ? "Output Speed (TPS)" : "输出速度（TPS）"}</dt>
              <dd class="font-mono tabular-nums text-right text-foreground">{tps} tok/s</dd>
            {/if}

            {#if ttftDisplay}
              <dt>{isEn ? "First Token Latency (TTFT)" : "首 token 用时（TTFT）"}</dt>
              <dd class="font-mono tabular-nums text-right text-foreground">{ttftDisplay}</dd>
            {/if}
          </dl>
        </div>
      {/if}
    </span>
  {/if}

  <!-- Timestamp -->
  {#if formattedTimestamp}
    <span class="text-[12px] text-muted-foreground/60 ml-1 font-mono tabular-nums"
      >{formattedTimestamp}</span
    >
  {/if}
</div>
