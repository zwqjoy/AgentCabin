<script lang="ts">
  import type { TurnUsage } from "$lib/stores/types";
  import type { TimelineEntry } from "$lib/types";
  import { formatTokens, formatCacheHitPercent, formatRunDuration } from "$lib/utils/chat-format";
  import { currentLocale } from "$lib/i18n/index.svelte";

  let {
    turnUsages = [],
    timeline = [],
  }: {
    turnUsages?: TurnUsage[];
    timeline?: TimelineEntry[];
  } = $props();

  let isEn = $derived(Boolean(currentLocale() && currentLocale().startsWith("en")));

  // Derive conversation-level aggregated metrics (aligned with DSH StatsLine)
  const stats = $derived.by(() => {
    // 1. Turns & Steps
    const turns = timeline.filter((e) => e.kind === "user").length;
    const toolSteps = timeline.filter((e) => e.kind === "tool").length;
    const assistantSteps = timeline.filter((e) => e.kind === "assistant").length;
    const totalSteps = toolSteps + assistantSteps;

    // 2. Tokens & Cache
    let totalInput = 0;
    let totalOutput = 0;
    let totalCacheRead = 0;
    let totalLlmMs = 0;
    let totalWallMs = 0;

    for (const tu of turnUsages) {
      totalInput += tu.inputTokens || 0;
      totalOutput += tu.outputTokens || 0;
      totalCacheRead += tu.cacheReadTokens || 0;
      if (tu.durationApiMs && tu.durationApiMs > 0) {
        totalLlmMs += tu.durationApiMs;
      }
      if (tu.durationMs && tu.durationMs > 0) {
        totalWallMs += tu.durationMs;
      }
    }

    // Tool execution duration estimated as wall - LLM time (or from tool timestamps)
    const totalToolMs = Math.max(0, totalWallMs - totalLlmMs);

    // Speed calculation
    const overallTps =
      totalOutput > 0 && totalLlmMs > 0 ? (totalOutput / (totalLlmMs / 1000)).toFixed(0) : null;

    const promptTokens = totalInput + totalCacheRead;
    const cacheHit =
      promptTokens > 0 ? formatCacheHitPercent(totalCacheRead, promptTokens, 0) : null;

    return {
      turns,
      totalSteps,
      totalInput,
      totalOutput,
      totalCacheRead,
      totalLlmMs,
      totalToolMs,
      overallTps,
      cacheHit,
    };
  });
</script>

{#if stats.turns > 0 || stats.totalSteps > 0 || turnUsages.length > 0}
  <div
    class="w-full flex items-center justify-center gap-2 py-1.5 px-3 text-[11px] text-muted-foreground/60 select-none overflow-x-auto scrollbar-none font-mono"
  >
    <!-- Group 1: Turns & Steps -->
    <span>
      {isEn
        ? `${stats.turns} turns · ${stats.totalSteps} steps`
        : `${stats.turns} 轮 · ${stats.totalSteps} 步`}
    </span>

    <!-- Separator -->
    <span class="text-muted-foreground/30">|</span>

    <!-- Group 2: LLM & Tool duration -->
    {#if stats.totalLlmMs > 0 || stats.totalToolMs > 0}
      <span>
        {isEn
          ? `LLM ${formatRunDuration(stats.totalLlmMs, true)} · Tools ${formatRunDuration(stats.totalToolMs, true)}`
          : `LLM ${formatRunDuration(stats.totalLlmMs, false)} · 工具调用 ${formatRunDuration(stats.totalToolMs, false)}`}
      </span>
      <span class="text-muted-foreground/30">|</span>
    {/if}

    <!-- Group 3: Throughput -->
    {#if stats.overallTps}
      <span>
        {isEn ? `${stats.overallTps} tok/s` : `${stats.overallTps} tok/s`}
      </span>
      <span class="text-muted-foreground/30">|</span>
    {/if}

    <!-- Group 4: Cache hit & Token totals -->
    <span>
      {#if stats.cacheHit !== null}
        {isEn ? `Cache hit ${stats.cacheHit}%` : `缓存命中 ${stats.cacheHit}%`}
        {#if stats.totalInput > 0 || stats.totalOutput > 0}
          <span> · </span>
        {/if}
      {/if}
      {#if stats.totalInput > 0 || stats.totalOutput > 0}
        {isEn
          ? `In ${formatTokens(stats.totalInput)} · Out ${formatTokens(stats.totalOutput)}`
          : `输入 ${formatTokens(stats.totalInput)} · 输出 ${formatTokens(stats.totalOutput)}`}
      {/if}
    </span>
  </div>
{/if}
