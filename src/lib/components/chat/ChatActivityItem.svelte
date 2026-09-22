<script lang="ts">
  import { onDestroy } from "svelte";
  import type { ActivityItem } from "$lib/utils/tool-activity-adapter";
  import { copyToClipboard, extractOutputText } from "$lib/utils/tool-rendering";
  import { classifyWorkExecutionFailure } from "$lib/utils/work-execution-failure";

  let {
    activity,
    runId = "",
    fetchToolResult,
    onPreviewFile,
    onOpenDetails,
  }: {
    activity: ActivityItem;
    runId?: string;
    fetchToolResult?: (runId: string, toolUseId: string) => Promise<Record<string, unknown> | null>;
    onPreviewFile?: (path: string) => void;
    onOpenDetails?: (
      toolName: string,
      args: unknown,
      result: unknown,
      isError: boolean,
      isRunning: boolean,
    ) => void;
  } = $props();

  let isDrawerOpen = $state(false);
  let lazyOutput = $state<string | null>(null);
  let isLoadingOutput = $state(false);
  let isCopied = $state(false);
  let copyResetTimer: ReturnType<typeof setTimeout> | undefined;

  onDestroy(() => {
    clearTimeout(copyResetTimer);
  });

  const displayOutput = $derived(
    lazyOutput ??
      activity.detail.output ??
      extractOutputText(activity.tool.output ?? activity.tool.tool_use_result),
  );

  const hasOutput = $derived(
    Boolean(
      (displayOutput && displayOutput.trim().length > 0) ||
      activity.detail.diff ||
      (activity.detail.matches && activity.detail.matches.length > 0) ||
      activity.detail.command,
    ),
  );

  // Subagent specialized handling
  const isDelegate = $derived(
    activity.tool.tool_name === "work_delegate" ||
      (activity.category === "agent" && activity.tool.tool_name !== "task"),
  );
  const delegateInput = $derived(
    (activity.tool.input ?? activity.tool.tool_input ?? {}) as Record<string, unknown>,
  );
  const rawRole = $derived(
    String(
      delegateInput.role ??
        delegateInput.subagent_type ??
        activity.detail.subagentType ??
        "researcher",
    ).toLowerCase(),
  );
  const delegateRoleLabel = $derived(
    rawRole === "researcher"
      ? "Researcher 调研助手"
      : rawRole === "worker"
        ? "Worker 执行助手"
        : rawRole === "reviewer"
          ? "Reviewer 审阅助手"
          : `${rawRole.toUpperCase()} 助手`,
  );
  const delegateTask = $derived(
    String(delegateInput.task ?? delegateInput.prompt ?? activity.detail.prompt ?? ""),
  );
  const delegateOutput = $derived(
    activity.tool.output && typeof activity.tool.output === "object"
      ? (activity.tool.output as Record<string, unknown>)
      : {},
  );
  const delegateDetails = $derived(
    delegateOutput.details && typeof delegateOutput.details === "object"
      ? (delegateOutput.details as Record<string, unknown>)
      : delegateOutput,
  );
  const delegateAgentId = $derived(
    String(delegateDetails.agent_id ?? delegateDetails.conversationId ?? ""),
  );
  const isDelegateRunning = $derived(activity.state === "running");
  const isDelegateFailed = $derived(activity.state === "failed" || delegateDetails.ok === false);

  // Failure classification
  const executionFailure = $derived(
    activity.state === "failed" || activity.tool.status === "error"
      ? classifyWorkExecutionFailure(activity.tool.status, activity.tool.output)
      : null,
  );

  const errorSummary = $derived.by(() => {
    if (activity.state !== "failed" && activity.tool.status !== "error") return null;
    const raw: unknown = activity.tool.output ?? activity.tool.tool_use_result;
    if (typeof raw === "string") return raw.trim().split("\n")[0];
    if (raw && typeof raw === "object") {
      const text = extractOutputText(raw as Record<string, unknown>);
      if (text) return text.trim().split("\n")[0];
    }
    return executionFailure?.description ?? "工具执行失败";
  });

  const dshTitle = $derived.by(() => {
    if (activity.state === "failed") return "工具调用";
    if (activity.category === "command") return "Bash";
    if (activity.category === "read") return "读取";
    if (activity.category === "edit") return "写入";
    if (activity.category === "search") return "搜索";
    return activity.verb || "工具调用";
  });

  const inputJsonString = $derived.by(() => {
    const inp = activity.tool.input ?? activity.tool.tool_input;
    if (!inp) return null;
    if (typeof inp === "string") return inp;
    try {
      return JSON.stringify(inp, null, 2);
    } catch {
      return String(inp);
    }
  });

  // Default open if failed (like DSH Image 3)
  $effect(() => {
    if (activity.state === "failed") {
      isDrawerOpen = true;
    }
  });

  // Browser tool helpers
  function isBrowserTool(name: string): boolean {
    return name.startsWith("web_") || name.startsWith("browser_") || name === "read_url_content";
  }

  function browserDetails(output: Record<string, unknown> | undefined): Record<string, unknown> {
    if (!output) return {};
    return output.details && typeof output.details === "object"
      ? (output.details as Record<string, unknown>)
      : output;
  }

  function browserArray(value: unknown): Array<Record<string, unknown>> {
    return Array.isArray(value)
      ? value.filter(
          (item): item is Record<string, unknown> => Boolean(item) && typeof item === "object",
        )
      : [];
  }

  function browserCitation(details: Record<string, unknown>): string {
    if (typeof details.markdown === "string") return details.markdown;
    const citation = details.citation;
    if (
      citation &&
      typeof citation === "object" &&
      typeof (citation as Record<string, unknown>).markdown === "string"
    ) {
      return String((citation as Record<string, unknown>).markdown);
    }
    return "已生成引用";
  }

  function browserToolFailed(output: unknown): boolean {
    if (!output || typeof output !== "object") return false;
    return browserDetails(output as Record<string, unknown>).ok === false;
  }

  function browserFailureMessage(output: unknown): string {
    if (!output || typeof output !== "object") return "网络访问工具执行失败";
    const content = (output as Record<string, unknown>).content;
    if (!Array.isArray(content)) return "网络访问工具执行失败";
    const firstText = content.find(
      (item) =>
        item &&
        typeof item === "object" &&
        typeof (item as Record<string, unknown>).text === "string",
    ) as Record<string, unknown> | undefined;
    return firstText ? String(firstText.text) : "网络访问工具执行失败";
  }

  async function toggleDrawer() {
    if (isDrawerOpen) {
      isDrawerOpen = false;
      return;
    }

    isDrawerOpen = true;

    // Fetch tool output if missing and lazy loader available
    if (
      !displayOutput &&
      fetchToolResult &&
      runId &&
      !isLoadingOutput &&
      activity.tool.tool_use_id
    ) {
      isLoadingOutput = true;
      try {
        const res = await fetchToolResult(runId, activity.tool.tool_use_id);
        if (res) {
          lazyOutput = extractOutputText(res);
        }
      } catch {
        // silent ignore
      } finally {
        isLoadingOutput = false;
      }
    }
  }

  function handleRowClick(event: MouseEvent) {
    // If target has preview action and clicked directly
    const previewPath = activity.detail.filePath || activity.detail.path;
    if (previewPath && onPreviewFile) {
      const target = event.target as HTMLElement;
      if (target.closest(".preview-btn") || target.closest(".file-target")) {
        onPreviewFile(previewPath);
        return;
      }
    }
    void toggleDrawer();
  }

  function handleCopy(textToCopy: string, event: MouseEvent) {
    event.stopPropagation();
    copyToClipboard(textToCopy);
    isCopied = true;
    // Clear any pending reset so rapid re-copies don't get reverted by a stale timer,
    // and never leak a timer past component teardown.
    clearTimeout(copyResetTimer);
    copyResetTimer = setTimeout(() => {
      isCopied = false;
    }, 1600);
  }
</script>

{#if isDelegate}
  <!-- Subagent Activity Item (Flat Process Timeline Row) -->
  <div
    id="tool-{activity.id}"
    class="group/delegate w-full py-0.5 animate-fade-in text-[var(--chat-secondary-size,13px)]"
  >
    <details class="group/delegate-details">
      <summary
        class="flex min-h-6 cursor-pointer select-none list-none items-center gap-2 rounded px-1 py-0.5 text-muted-foreground/80 hover:text-foreground transition-colors hover:bg-muted/30 [&::-webkit-details-marker]:hidden"
      >
        <span class="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
          {#if isDelegateRunning}
            <span
              class="h-2.5 w-2.5 animate-spin rounded-full border-[1.5px] border-muted-foreground/50 border-t-transparent"
            ></span>
          {:else if isDelegateFailed}
            <span class="text-rose-500 font-bold text-xs">✕</span>
          {:else}
            <span class="text-muted-foreground/60 text-xs">✓</span>
          {/if}
        </span>

        <span class="font-medium text-foreground/80 text-[13px]">{delegateRoleLabel}</span>

        {#if delegateTask}
          <span class="text-muted-foreground/40 font-mono select-none">·</span>
          <span class="min-w-0 truncate text-muted-foreground/65 font-normal text-[12.5px]"
            >{delegateTask}</span
          >
        {/if}

        <span class="ml-auto shrink-0 text-[11px] text-muted-foreground/50">
          {isDelegateRunning ? "运行中" : isDelegateFailed ? "失败" : "已完成"}
        </span>

        <svg
          class="h-3 w-3 text-muted-foreground/40 transition-transform group-open/delegate-details:rotate-90"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="m9 18 6-6-6-6" />
        </svg>
      </summary>

      <!-- Expandable Detail Surface -->
      <div
        class="mt-1 mb-1.5 ml-5 rounded-lg border border-border/30 bg-[var(--chat-surface-code)] p-2.5 text-xs space-y-1.5"
      >
        {#if delegateTask}
          <div class="text-muted-foreground leading-relaxed">
            <span class="text-muted-foreground/50 select-none">任务：</span>{delegateTask}
          </div>
        {/if}

        {#if delegateAgentId}
          <div
            class="flex items-center justify-between text-[10.5px] text-muted-foreground/50 pt-1 border-t border-border/20"
          >
            <span class="font-mono">ID: {delegateAgentId.slice(0, 16)}</span>
            <span>独立沙箱运行</span>
          </div>
        {/if}

        {#if hasOutput}
          <div class="pt-1">
            <span class="text-[10.5px] text-muted-foreground/60 block mb-1 select-none"
              >输出详情：</span
            >
            <pre
              class="max-h-52 overflow-auto rounded bg-background/50 p-2 font-mono text-[10.5px] leading-4 text-muted-foreground border border-border/20">{displayOutput}</pre>
          </div>
        {/if}
      </div>
    </details>
  </div>
{:else}
  <!-- Standard Tool Activity Row and Drawer -->
  <div id="tool-{activity.id}" class="group/item flex flex-col py-0.5">
    <!-- DSH DisclosureRow: clean flat row with icon, title, separator, and target -->
    <div
      role="button"
      tabindex="0"
      class="flex items-center h-6 w-full min-w-0 rounded px-1 text-[13px] leading-6 transition-colors hover:bg-muted/30 cursor-pointer select-none text-muted-foreground/80 hover:text-foreground group/item {activity.state ===
      'running'
        ? 'bg-muted/25'
        : ''}"
      onclick={handleRowClick}
      onkeydown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          void toggleDrawer();
        }
      }}
    >
      <!-- Leading Icon / Chevron (16px box, 14px icon, smoothly switches to chevron on hover or open) -->
      <span
        class="w-4 h-4 mr-1.5 flex items-center justify-center shrink-0 text-muted-foreground/60 group-hover/item:text-foreground"
      >
        {#if isDrawerOpen}
          <svg
            class="h-3.5 w-3.5 text-muted-foreground"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <polyline points="6 9 12 15 18 9" />
          </svg>
        {:else if activity.state === "running"}
          <span
            class="h-3 w-3 animate-spin rounded-full border-[1.5px] border-muted-foreground/50 border-t-transparent"
          ></span>
        {:else}
          <span class="inline-flex items-center justify-center group-hover/item:hidden">
            {#if activity.iconKind === "terminal"}
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <polyline points="4 17 10 11 4 5" /><line x1="12" y1="19" x2="20" y2="19" />
              </svg>
            {:else if activity.iconKind === "book" || activity.category === "read"}
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1-2.5-2.5Z" />
                <path d="M6 6h10" /><path d="M6 10h10" />
              </svg>
            {:else if activity.iconKind === "pencil" || activity.category === "edit"}
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
                <path d="m15 5 4 4" />
              </svg>
            {:else if activity.iconKind === "search" || activity.category === "search"}
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" />
              </svg>
            {:else}
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
              >
                <circle cx="12" cy="12" r="10" />
              </svg>
            {/if}
          </span>
          <svg
            class="h-3.5 w-3.5 hidden group-hover/item:block -rotate-90 text-muted-foreground/80"
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
      </span>

      <!-- Title (Bash / 读取 / 写入 / 搜索 / 工具调用) -->
      <span
        class="shrink-0 font-normal select-none {activity.state === 'failed'
          ? 'text-rose-500 font-medium'
          : 'text-muted-foreground/80 group-hover/item:text-foreground'}"
      >
        {dshTitle}
      </span>

      <!-- Dot Separator -->
      <span class="mx-1.5 text-muted-foreground/40 font-mono text-xs select-none shrink-0">·</span>

      <!-- Target or Error Summary -->
      <span
        class="min-w-0 flex-1 truncate text-[13px] font-normal {activity.state === 'failed'
          ? 'text-rose-500'
          : activity.detail.filePath || activity.detail.path
            ? 'text-muted-foreground/80 hover:text-foreground hover:underline underline-offset-2'
            : 'text-muted-foreground/70 group-hover/item:text-muted-foreground/90'}"
        title={activity.detail.command ||
          activity.detail.filePath ||
          activity.detail.path ||
          activity.target}
      >
        {errorSummary || activity.target}
      </span>

      <!-- Actions (Preview file / DSH Details drawer): appear on hover, stay single-line without wrapping -->
      {#if ((activity.detail.filePath || activity.detail.path) && onPreviewFile) || onOpenDetails}
        <div
          class="ml-2 flex items-center gap-1 shrink-0 opacity-0 group-hover/item:opacity-100 transition-opacity"
        >
          <!-- Preview file button if path is present -->
          {#if (activity.detail.filePath || activity.detail.path) && onPreviewFile}
            <button
              type="button"
              class="preview-btn shrink-0 whitespace-nowrap inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground/60 hover:bg-muted hover:text-foreground transition-colors cursor-pointer"
              onclick={(e) => {
                e.stopPropagation();
                onPreviewFile(activity.detail.filePath || activity.detail.path!);
              }}
              title="预览"
            >
              <span>预览</span>
            </button>
          {/if}

          <!-- DSH Details Drawer button -->
          {#if onOpenDetails}
            <button
              type="button"
              class="preview-btn shrink-0 whitespace-nowrap inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground/60 hover:bg-muted hover:text-foreground transition-colors cursor-pointer"
              onclick={(e) => {
                e.stopPropagation();
                onOpenDetails(
                  activity.target || activity.tool.tool_name,
                  activity.tool.input ?? activity.tool.tool_input,
                  displayOutput || errorSummary,
                  activity.state === "failed",
                  activity.state === "running",
                );
              }}
              title="查看详情抽屉"
            >
              <span>详情</span>
              <svg
                class="size-2.5 opacity-70 shrink-0"
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
              >
                <path d="M6 3l5 5-5 5" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </button>
          {/if}
        </div>
      {/if}
    </div>

    <!-- DSH ioCard & Drawer -->
    {#if isDrawerOpen}
      <div
        class="mt-1 mb-1.5 ml-4 overflow-hidden rounded-xl border border-border/60 bg-[var(--chat-surface-code)] text-xs font-mono shadow-xs animate-fade-in"
      >
        {#if inputJsonString}
          <div
            class="grid grid-cols-[max-content_1fr] items-baseline gap-x-3.5 max-h-36 p-3 overflow-y-auto"
          >
            <span class="text-muted-foreground/60 text-xs sticky top-0 font-sans select-none"
              >输入</span
            >
            <span class="text-muted-foreground/90 whitespace-pre-wrap break-words"
              >{inputJsonString}</span
            >
          </div>
        {/if}
        {#if inputJsonString && (displayOutput || errorSummary)}
          <div class="h-[0.5px] bg-border/40"></div>
        {/if}
        {#if displayOutput || errorSummary}
          <div
            class="grid grid-cols-[max-content_1fr] items-baseline gap-x-3.5 max-h-56 p-3 overflow-y-auto"
          >
            <span class="text-muted-foreground/60 text-xs sticky top-0 font-sans select-none"
              >输出</span
            >
            <span
              class="whitespace-pre-wrap break-words {activity.state === 'failed'
                ? 'text-rose-500'
                : 'text-muted-foreground/90'}"
            >
              {displayOutput || errorSummary}
            </span>
          </div>
        {/if}
      </div>
    {/if}
  </div>
{/if}
