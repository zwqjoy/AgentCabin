<script lang="ts">
  import type { ContextUsageCategory, SessionInfoData } from "$lib/types";
  import type { MessageKey } from "$lib/i18n/types";
  import { t } from "$lib/i18n/index.svelte";
  import { formatTokenCount } from "$lib/utils/format";

  let { info, onClose }: { info: SessionInfoData; onClose?: () => void } = $props();

  const contextWindow = $derived(
    Math.max(0, info.contextBreakdown?.contextWindow || info.contextWindow || 0),
  );
  const contextTokens = $derived(
    Math.max(
      0,
      info.contextBreakdown?.usedTokens ??
        info.contextTokens ??
        info.inputTokens + info.cacheReadTokens + info.cacheWriteTokens,
    ),
  );
  const usedTokens = $derived(
    contextWindow > 0 ? Math.min(contextWindow, contextTokens) : contextTokens,
  );
  const percentage = $derived(
    contextWindow > 0 ? Math.round(Math.min(1, contextTokens / contextWindow) * 100) : 0,
  );

  const categoryLabels: Record<string, MessageKey> = {
    system_prompt: "prompt_contextUsageSystemPrompt",
    instructions: "prompt_contextUsageInstructions",
    skills: "prompt_contextUsageSkills",
    tool_prompt: "prompt_contextUsageToolPrompt",
    system_tools: "prompt_contextUsageSystemTools",
    custom_tools: "prompt_contextUsageCustomTools",
    mcp_tools: "prompt_contextUsageMcpTools",
    user_messages: "prompt_contextUsageUserMessages",
    assistant_text: "prompt_contextUsageAssistantText",
    assistant_thinking: "prompt_contextUsageAssistantThinking",
    tool_calls: "prompt_contextUsageToolCalls",
    tool_results: "prompt_contextUsageToolResults",
    extension_messages: "prompt_contextUsageExtensionMessages",
    other_messages: "prompt_contextUsageOtherMessages",
    free_space: "prompt_contextUsageFreeSpace",
  };
  const categoryColors: Record<string, string> = {
    system_prompt: "#73777c",
    instructions: "#8b5cf6",
    skills: "#c026d3",
    tool_prompt: "#2576b9",
    system_tools: "#2563eb",
    custom_tools: "#0f766e",
    mcp_tools: "#0891b2",
    user_messages: "#e0441e",
    assistant_text: "#f59e0b",
    assistant_thinking: "#d97706",
    tool_calls: "#65a30d",
    tool_results: "#16a34a",
    extension_messages: "#9c1671",
    other_messages: "#73777c",
    free_space: "#d7d9dc",
  };

  function categoryLabel(id: string): string {
    const key = categoryLabels[id];
    return key ? t(key) : id;
  }

  function flattenCategories(categories: ContextUsageCategory[]): ContextUsageCategory[] {
    return categories.flatMap((category) => {
      const parent = { ...category, children: undefined };
      return category.children?.length
        ? [parent, ...flattenCategories(category.children)]
        : [parent];
    });
  }

  const usageParts = $derived.by(() => {
    const breakdown = info.contextBreakdown;
    if (breakdown?.categories?.length) {
      return flattenCategories(breakdown.categories)
        .filter((part) => part.tokens > 0)
        .map((part) => ({
          id: part.id,
          label: categoryLabel(part.id),
          tokens: part.tokens,
          color: categoryColors[part.id] ?? "#73777c",
        }));
    }
    const rawParts = [
      {
        id: "input",
        label: t("prompt_contextUsageInput"),
        tokens: Math.max(0, info.inputTokens),
        color: "#e0441e",
      },
      {
        id: "cache-read",
        label: t("prompt_contextUsageCacheRead"),
        tokens: Math.max(0, info.cacheReadTokens),
        color: "#9c1671",
      },
      {
        id: "cache-write",
        label: t("prompt_contextUsageCacheWrite"),
        tokens: Math.max(0, info.cacheWriteTokens),
        color: "#2576b9",
      },
    ];
    const rawTotal = rawParts.reduce((sum, part) => sum + part.tokens, 0);
    const visibleTotal = Math.min(rawTotal, usedTokens);
    const parts = rawParts
      .map((part) => ({
        ...part,
        tokens: rawTotal > 0 ? Math.round((part.tokens / rawTotal) * visibleTotal) : 0,
      }))
      .filter((part) => part.tokens > 0);
    const classified = parts.reduce((sum, part) => sum + part.tokens, 0);
    const other = Math.max(0, usedTokens - classified);
    if (other > 0) {
      parts.push({
        id: "other",
        label: t("prompt_contextUsageOther"),
        tokens: other,
        color: "#73777c",
      });
    }
    return parts;
  });

  const barTotal = $derived(contextWindow > 0 ? contextWindow : usedTokens);
  const isStructured = $derived(!!info.contextBreakdown);

  const hasUsage = $derived(usedTokens > 0 || contextWindow > 0);

  function formatTotal(value: number): string {
    return formatTokenCount(value);
  }
</script>

<div
  class="w-[min(22rem,calc(100vw-1rem))] max-w-full overflow-hidden rounded-xl border border-border/80 bg-popover text-popover-foreground shadow-2xl animate-in fade-in zoom-in-95 duration-150"
  role="dialog"
  aria-label={t("prompt_contextUsageTitle")}
>
  <div class="flex items-center justify-between border-b border-border/60 px-3 py-2">
    <span class="text-xs font-medium">{t("prompt_contextUsageTitle")}</span>
    <button
      type="button"
      class="rounded-md p-0.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      aria-label={t("common_close")}
      onclick={() => onClose?.()}
    >
      <svg
        class="h-3.5 w-3.5"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        aria-hidden="true"
      >
        <path d="M6 6l12 12M18 6 6 18" stroke-linecap="round" />
      </svg>
    </button>
  </div>

  <div class="px-3 pb-3 pt-2.5">
    {#if hasUsage}
      <div class="flex items-end justify-between gap-3">
        <div>
          <div class="flex items-baseline gap-1">
            <span class="text-xl font-semibold tracking-tight tabular-nums">{percentage}%</span>
            <span class="text-xs text-muted-foreground">{t("prompt_contextUsageFull")}</span>
          </div>
          <div class="mt-0.5 text-[11px] text-muted-foreground">
            {formatTotal(usedTokens)} / {contextWindow > 0 ? formatTotal(contextWindow) : "—"} tokens
          </div>
        </div>
        <span class="text-[11px] text-muted-foreground tabular-nums">
          {t("prompt_contextUsageCurrent", { value: formatTotal(usedTokens) })}
        </span>
      </div>

      <div
        class="mt-2 flex h-1.5 overflow-hidden rounded-full bg-muted"
        aria-label={`${percentage}%`}
      >
        {#each usageParts as part}
          <span
            class="h-full first:rounded-l-full last:rounded-r-full"
            style:width={`${barTotal > 0 ? (part.tokens / barTotal) * 100 : 0}%`}
            style:background-color={part.color}
            title={`${part.label}: ${formatTotal(part.tokens)}`}
          ></span>
        {/each}
      </div>

      <div class="mt-3 space-y-1.5">
        {#each usageParts as part}
          <div class="flex items-center gap-2 text-xs">
            <span class="h-2.5 w-2.5 shrink-0 rounded-[3px]" style:background-color={part.color}
            ></span>
            <span class="min-w-0 flex-1 truncate">{part.label}</span>
            <span class="shrink-0 text-muted-foreground tabular-nums"
              >{formatTotal(part.tokens)}</span
            >
          </div>
        {/each}
      </div>

      <p class="mt-3 border-t border-border/50 pt-2.5 text-[10px] leading-4 text-muted-foreground">
        {#if isStructured}
          {t("prompt_contextUsageStructuredNote")}
        {:else}
          {t("prompt_contextUsageBreakdownNote")}
        {/if}
      </p>
    {:else}
      <div class="py-6 text-center text-xs text-muted-foreground">
        {t("prompt_contextUsageNoData")}
      </div>
    {/if}
  </div>
</div>
