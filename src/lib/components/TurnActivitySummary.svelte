<script lang="ts">
  import { formatDuration } from "$lib/utils/format";
  import WorkToolCall from "$lib/components/work/WorkToolCall.svelte";
  import WorkToolCallGroup from "$lib/components/work/WorkToolCallGroup.svelte";
  import type { TimelineEntry } from "$lib/types";
  import { summarizeWorkActivity } from "$lib/utils/work-activity";

  type ToolEntry = Extract<TimelineEntry, { kind: "tool" }>;

  let {
    durationMs = 0,
    stepCount = 0,
    running = false,
    label = "思考过程",
    thinkingText = "",
    expanded = $bindable(false),
    toolGroups,
    workMode = false,
  }: {
    durationMs?: number;
    stepCount?: number;
    running?: boolean;
    label?: string;
    thinkingText?: string;
    expanded?: boolean;
    toolGroups?: ToolEntry[][];
    workMode?: boolean;
  } = $props();

  const activeToolGroups = $derived(toolGroups ?? []);
  const workTools = $derived(activeToolGroups.flatMap((group) => group.map((entry) => entry.tool)));
  const workActionCount = $derived(workTools.length > 0 ? workTools.length : stepCount);
  const workActivity = $derived(summarizeWorkActivity(workTools, thinkingText, running));
  let thinkingDetailsExpanded = $state(false);

  function formatElapsed(ms: number): string {
    const value = formatDuration(ms);
    return value.replace(/h/g, "小时").replace(/m/g, "分").replace(/s/g, "秒");
  }

  let summary = $derived(
    workMode
      ? running
        ? workActionCount > 0
          ? `正在处理 · ${workActionCount} 个动作`
          : "正在处理"
        : workActionCount > 0
          ? `已处理 · ${workActionCount} 个动作`
          : durationMs > 0
            ? `本轮处理 · 耗时 ${formatElapsed(durationMs)}`
            : "本轮处理"
      : running
        ? stepCount > 0
          ? `正在执行 · ${stepCount} 个步骤`
          : "正在执行"
        : durationMs > 0
          ? `耗时 ${formatElapsed(durationMs)}${stepCount > 0 ? ` · ${stepCount} 个步骤` : ""}`
          : stepCount > 0
            ? `${label} · ${stepCount} 个步骤`
            : label,
  );

  function workStatusDotClass(): string {
    if (workActivity.status === "error") return "bg-red-500";
    if (workActivity.status === "waiting") return "bg-orange-500 animate-pulse";
    if (workActivity.status === "running" || workActivity.status === "thinking") {
      return "bg-amber-500 animate-pulse";
    }
    if (workActivity.status === "completed") return "bg-emerald-500";
    return "bg-muted-foreground/50";
  }

  function workStatusTextClass(): string {
    if (workActivity.status === "error") return "text-red-600 dark:text-red-400";
    if (workActivity.status === "waiting") return "text-orange-600 dark:text-orange-400";
    if (workActivity.status === "completed") return "text-emerald-600 dark:text-emerald-400";
    return "text-muted-foreground";
  }
</script>

<div class="w-full py-1.5" data-export-exclude>
  <div class="chat-content-width">
    <div class="flex items-center border-b border-border/60">
      <button
        type="button"
        class="group inline-flex min-w-0 items-center gap-2 py-2 text-sm text-muted-foreground transition-colors hover:text-foreground {workMode
          ? 'flex-1'
          : ''}"
        aria-expanded={expanded}
        onclick={() => (expanded = !expanded)}
      >
        <svg
          class="h-3.5 w-3.5 shrink-0 transition-transform {expanded ? 'rotate-90' : ''}"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"><path d="m9 18 6-6-6-6" /></svg
        >
        <span class="shrink-0">{summary}</span>
        {#if workMode && workActivity.currentLabel}
          <span
            class="hidden min-w-0 items-center gap-1.5 truncate border-l border-border/60 pl-2 text-xs text-muted-foreground sm:inline-flex"
          >
            <span
              class="h-1.5 w-1.5 shrink-0 rounded-full {workStatusDotClass()}"
              aria-hidden="true"
            ></span>
            <span class="truncate">{workActivity.currentLabel}</span>
          </span>
        {/if}
      </button>
      {#if workMode && workActivity.statusLabel}
        <span class="ml-2 shrink-0 text-[11px] font-medium {workStatusTextClass()}"
          >{workActivity.statusLabel}</span
        >
      {/if}
      <span class="ml-3 h-px flex-1 bg-border/35"></span>
    </div>
    {#if expanded}
      {#if workMode}
        <div class="pb-3 pt-2 pl-7">
          <div
            class="flex items-center gap-2.5 rounded-lg border border-border/60 bg-card/45 px-3 py-2.5"
          >
            <span class="h-2 w-2 shrink-0 rounded-full {workStatusDotClass()}" aria-hidden="true"
            ></span>
            <div class="min-w-0 flex-1">
              <div class="text-xs font-medium text-foreground">{workActivity.currentLabel}</div>
              {#if workActivity.status === "waiting"}
                <div
                  class="mt-0.5 text-[11px] leading-4 text-orange-700/80 dark:text-orange-300/80"
                >
                  需要你处理后，任务才会继续。
                </div>
              {:else if workActivity.status === "thinking"}
                <div class="mt-0.5 text-[11px] leading-4 text-muted-foreground">
                  模型正在整理执行方案，暂时不需要你的操作。
                </div>
              {/if}
            </div>
            {#if workActivity.totalCount > 0}
              <span class="shrink-0 text-[11px] text-muted-foreground"
                >{workActivity.completedCount}/{workActivity.totalCount} 项</span
              >
            {/if}
          </div>

          {#if thinkingText.trim()}
            <div class="mt-2" data-exclude-search="true">
              <button
                type="button"
                class="inline-flex min-h-8 items-center gap-1.5 rounded-md px-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
                aria-expanded={thinkingDetailsExpanded}
                onclick={() => (thinkingDetailsExpanded = !thinkingDetailsExpanded)}
              >
                <svg
                  class="h-3 w-3 transition-transform {thinkingDetailsExpanded ? 'rotate-90' : ''}"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  aria-hidden="true"><path d="m9 18 6-6-6-6" /></svg
                >
                <span>模型过程</span>
                <span class="text-[10px] text-muted-foreground/70"
                  >{thinkingDetailsExpanded ? "收起" : "查看详情"}</span
                >
              </button>
              {#if thinkingDetailsExpanded}
                <div
                  class="mt-1 max-h-48 overflow-y-auto rounded-lg border border-border/50 bg-muted/25 px-3 py-2 text-xs leading-relaxed text-muted-foreground/80"
                >
                  <pre
                    class="m-0 whitespace-pre-wrap break-words font-mono">{thinkingText.trimEnd()}</pre>
                </div>
              {/if}
            </div>
          {/if}

          {#if activeToolGroups.length > 0}
            <div class="mt-3">
              <div class="mb-1 flex items-center justify-between gap-2 px-1">
                <span class="text-[11px] font-medium text-muted-foreground">执行详情</span>
                <span class="text-[10px] text-muted-foreground/70">{workActionCount} 项</span>
              </div>
              <div class="space-y-0.5">
                {#each activeToolGroups as group (group[0]?.id)}
                  {#if group.length === 1}
                    <WorkToolCall entry={group[0]} nested />
                  {:else}
                    <WorkToolCallGroup entries={group} />
                  {/if}
                {/each}
              </div>
            </div>
          {/if}
        </div>
      {:else}
        <div class="pb-2 pt-1">
          <!-- Thinking text -->
          {#if thinkingText.trim()}
            <div class="pl-7 pb-2" data-exclude-search="true">
              <div
                class="border-l-2 border-muted-foreground/15 pl-3 text-xs leading-relaxed text-muted-foreground/75"
              >
                <div class="max-h-80 overflow-y-auto whitespace-pre-line break-words">
                  {thinkingText.trimEnd()}
                </div>
              </div>
            </div>
          {/if}
          <!-- All tool groups in a unified list -->
          {#if activeToolGroups.length > 0}
            <div class="space-y-0.5">
              {#each activeToolGroups as group (group[0]?.id)}
                {#if group.length === 1}
                  <WorkToolCall entry={group[0]} nested />
                {:else}
                  <WorkToolCallGroup entries={group} />
                {/if}
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    {/if}
  </div>
</div>
