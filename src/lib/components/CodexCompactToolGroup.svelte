<script lang="ts">
  import type { BusToolItem, TimelineEntry, PermissionSuggestion } from "$lib/types";
  import type { TaskNotificationItem } from "$lib/stores/session-store.svelte";
  import {
    getToolGroupSemanticSummary,
    getToolActionCategory,
    extractOutputText,
    copyToClipboard,
    friendlyToolName,
    getToolDetail,
    type ToolGroupSemanticSummary,
  } from "$lib/utils/tool-rendering";
  import InlineToolCard from "$lib/components/InlineToolCard.svelte";

  let {
    tools,
    subTimelineByToolId = new Map(),
    runId = "",
    fetchToolResult,
    onAnswer,
    onApprove,
    onPermissionRespond,
    onExitPlanClearContext,
    taskNotifications,
    planContentByToolId = new Map(),
    showPermissionInPanel = false,
    agentDisplayName,
    onPreviewFile,
  }: {
    tools: BusToolItem[];
    subTimelineByToolId?: Map<string, TimelineEntry[]>;
    runId?: string;
    fetchToolResult?: (runId: string, toolUseId: string) => Promise<Record<string, unknown> | null>;
    onAnswer?: (answer: string) => void;
    onApprove?: (toolName: string) => void;
    onPermissionRespond?: (
      requestId: string,
      behavior: "allow" | "deny",
      updatedPermissions?: PermissionSuggestion[],
      updatedInput?: Record<string, unknown>,
      denyMessage?: string,
      interrupt?: boolean,
    ) => void | Promise<void>;
    onExitPlanClearContext?: () => void | Promise<void>;
    taskNotifications?: Map<string, TaskNotificationItem>;
    planContentByToolId?: Map<string, { content: string; fileName: string } | null>;
    showPermissionInPanel?: boolean;
    agentDisplayName?: string;
    onPreviewFile?: (path: string) => void;
  } = $props();

  let expanded = $state(false);
  let showFullCards = $state(false);
  let expandedOutputs = $state<Record<string, boolean>>({});
  let lazyResults = $state<Record<string, string>>({});
  let lazyLoading = $state<Record<string, boolean>>({});
  let copiedToolId = $state<string | null>(null);

  let summary = $derived<ToolGroupSemanticSummary>(getToolGroupSemanticSummary(tools));

  // Determine if any tool requires user action/permission
  function isInteractiveTool(tool: BusToolItem): boolean {
    return (
      tool.status === "permission_prompt" ||
      tool.status === "ask_pending" ||
      tool.tool_name === "AskUserQuestion" ||
      (tool.tool_name === "ExitPlanMode" && Boolean(planContentByToolId.get(tool.tool_use_id)))
    );
  }

  let interactiveTools = $derived(tools.filter(isInteractiveTool));
  let regularTools = $derived(tools.filter((t) => !isInteractiveTool(t)));

  interface ToolItemDisplay {
    iconKind: "wrench" | "pencil" | "search" | "book" | "globe" | "bot" | "terminal";
    prefix: string;
    target: string;
    filePath?: string;
    command?: string;
    pattern?: string;
    output?: string;
  }

  function formatToolItemDisplay(tool: BusToolItem): ToolItemDisplay {
    const input = (tool.input ?? tool.tool_input ?? {}) as Record<string, unknown>;
    const toolName = tool.tool_name || "";
    const cat = getToolActionCategory(toolName);

    // Extract output preview (prefer lazy fetched or tool.output / tool_use_result)
    const rawOutput = extractOutputText(tool.output ?? tool.tool_use_result);
    const outputText = lazyResults[tool.tool_use_id] || rawOutput;

    if (cat === "skill") {
      const skillName = (input.skill ??
        input.name ??
        input.skill_name ??
        input.SkillName ??
        "") as string;
      return {
        iconKind: "wrench",
        prefix: "已读取",
        target: skillName ? `${skillName} 技能` : "技能",
        output: outputText,
      };
    }

    if (cat === "read") {
      const rawPath = (input.file_path ??
        input.path ??
        input.notebook_path ??
        input.filePath ??
        input.filename ??
        input.AbsolutePath ??
        input.target_file ??
        "") as string;
      const baseName = rawPath ? rawPath.split("/").pop() || rawPath : "";
      return {
        iconKind: "book",
        prefix: "已读取",
        target: baseName || rawPath || "文件",
        filePath: rawPath,
        output: outputText,
      };
    }

    if (cat === "edit") {
      const rawPath = (input.file_path ??
        input.path ??
        input.notebook_path ??
        input.filePath ??
        input.TargetFile ??
        input.target_file ??
        input.filename ??
        "") as string;
      const baseName = rawPath ? rawPath.split("/").pop() || rawPath : "";
      return {
        iconKind: "pencil",
        prefix: "已编辑",
        target: baseName || rawPath || "文件",
        filePath: rawPath,
        output: outputText,
      };
    }

    if (cat === "command") {
      const rawCmd = (input.command ??
        input.cmd ??
        input.CommandLine ??
        input.commandLine ??
        input.instruction ??
        input.exec ??
        tool.summary ??
        getToolDetail(input) ??
        "") as string;
      const cmd = typeof rawCmd === "string" ? rawCmd.trim() : "";
      return {
        iconKind: "terminal",
        prefix: "已运行",
        target: cmd || (toolName ? friendlyToolName(toolName) : "命令"),
        command: cmd,
        output: outputText,
      };
    }

    if (cat === "search") {
      const pattern = (input.pattern ??
        input.query ??
        input.Query ??
        input.Pattern ??
        input.regex ??
        input.search_term ??
        "") as string;
      const rawPath = (input.path ??
        input.SearchPath ??
        input.SearchDirectory ??
        input.directory ??
        input.dir ??
        "") as string;
      const scope = rawPath ? rawPath.split("/").pop() || rawPath : "";
      return {
        iconKind: "search",
        prefix: scope ? `已在 ${scope} 中搜索` : "已搜索",
        target: pattern ? `"${pattern}"` : scope || "代码",
        pattern,
        output: outputText,
      };
    }

    if (cat === "web") {
      const query = (input.query ?? input.url ?? input.Url ?? input.targetUrl ?? "") as string;
      return {
        iconKind: "globe",
        prefix: "已搜索网页：",
        target: query || "网页",
        output: outputText,
      };
    }

    if (cat === "agent") {
      const prompt = (input.prompt ??
        input.task ??
        input.Prompt ??
        input.subagent_type ??
        "") as string;
      return {
        iconKind: "bot",
        prefix: "已委派子代理",
        target: prompt ? (prompt.length > 50 ? prompt.slice(0, 50) + "…" : prompt) : "子代理",
        output: outputText,
      };
    }

    return {
      iconKind: "terminal",
      prefix: "已执行",
      target: getToolDetail(input) || (toolName ? friendlyToolName(toolName) : "操作"),
      output: outputText,
    };
  }

  async function toggleToolOutput(tool: BusToolItem) {
    const toolId = tool.tool_use_id;
    const isCurrentlyExpanded = Boolean(expandedOutputs[toolId]);

    if (isCurrentlyExpanded) {
      expandedOutputs[toolId] = false;
      return;
    }

    expandedOutputs[toolId] = true;

    // If output is not cached and not currently present, fetch from backend
    const existing = lazyResults[toolId] || extractOutputText(tool.output ?? tool.tool_use_result);
    if (!existing && fetchToolResult && runId && !lazyLoading[toolId]) {
      lazyLoading[toolId] = true;
      try {
        const res = await fetchToolResult(runId, toolId);
        if (res) {
          lazyResults[toolId] = extractOutputText(res);
        }
      } catch {
        // graceful ignore
      } finally {
        lazyLoading[toolId] = false;
      }
    }
  }

  async function handleItemClick(tool: BusToolItem, item: ToolItemDisplay) {
    if (item.filePath && onPreviewFile) {
      onPreviewFile(item.filePath);
      return;
    }
    await toggleToolOutput(tool);
  }

  function handleCopyOutput(toolId: string, text: string) {
    copyToClipboard(text);
    copiedToolId = toolId;
    setTimeout(() => {
      if (copiedToolId === toolId) copiedToolId = null;
    }, 1500);
  }
</script>

<div class="w-full py-0.5">
  <div class="chat-content-width pl-7">
    {#if regularTools.length > 0}
      <!-- Codex style compact semantic row -->
      <div class="flex flex-col">
        <button
          type="button"
          class="group/tool-row inline-flex items-center gap-2 rounded-md px-1.5 py-1 text-left text-xs text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground cursor-pointer select-none"
          aria-expanded={expanded}
          onclick={() => (expanded = !expanded)}
        >
          <!-- Icon -->
          <span
            class="flex h-4 w-4 shrink-0 items-center justify-center text-muted-foreground/80 group-hover/tool-row:text-foreground"
          >
            {#if summary.iconKind === "wrench"}
              <!-- Wrench icon -->
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path
                  d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"
                />
              </svg>
            {:else if summary.iconKind === "book"}
              <!-- Book icon -->
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
            {:else if summary.iconKind === "pencil"}
              <!-- Pencil icon -->
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
            {:else if summary.iconKind === "search"}
              <!-- Magnifying glass icon -->
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
            {:else if summary.iconKind === "globe"}
              <!-- Globe icon -->
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <circle cx="12" cy="12" r="10" />
                <path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20" />
                <path d="M2 12h20" />
              </svg>
            {:else if summary.iconKind === "bot"}
              <!-- Bot icon -->
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <rect width="18" height="14" x="3" y="6" rx="2" />
                <circle cx="9" cy="13" r="1" /><circle cx="15" cy="13" r="1" />
                <path d="M12 2v4" />
              </svg>
            {:else}
              <!-- Terminal icon -->
              <span class="font-mono text-[10px] text-muted-foreground/80 font-bold select-none"
                >&gt;_</span
              >
            {/if}
          </span>

          <!-- Label -->
          <span class="font-normal text-xs {summary.hasActive ? 'text-blue-400 font-medium' : ''}">
            {summary.hasActive ? summary.activeLabel : summary.label}
          </span>

          <!-- Pulse indicator if running -->
          {#if summary.hasActive}
            <span class="h-1.5 w-1.5 rounded-full bg-blue-500 animate-pulse"></span>
          {/if}

          <!-- Chevron -->
          <svg
            class="h-3.5 w-3.5 text-muted-foreground/70 transition-transform duration-150 {expanded
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
        </button>

        <!-- Expanded Items -->
        {#if expanded}
          <div class="my-1 pl-4 space-y-1 animate-fade-in border-l border-border/40 ml-2">
            {#if showFullCards}
              <div class="space-y-2 pt-1">
                <div class="flex justify-end pb-1">
                  <button
                    type="button"
                    class="text-[11px] text-muted-foreground/70 hover:text-foreground underline underline-offset-2 cursor-pointer"
                    onclick={() => (showFullCards = false)}
                  >
                    收起为简洁列表
                  </button>
                </div>
                {#each regularTools as tool (tool.tool_use_id)}
                  <InlineToolCard
                    {tool}
                    subTimeline={subTimelineByToolId.get(tool.tool_use_id)}
                    {runId}
                    {fetchToolResult}
                    {onApprove}
                    {onPermissionRespond}
                    {onExitPlanClearContext}
                    {taskNotifications}
                    {showPermissionInPanel}
                    {agentDisplayName}
                    {onPreviewFile}
                  />
                {/each}
              </div>
            {:else}
              <!-- Compact items list matching Codex Screenshot 4 -->
              {#each regularTools as tool (tool.tool_use_id)}
                {@const item = formatToolItemDisplay(tool)}
                {@const isOutputExpanded = Boolean(expandedOutputs[tool.tool_use_id])}
                {@const currentOutput = lazyResults[tool.tool_use_id] ?? item.output ?? ""}
                {@const isLoading = Boolean(lazyLoading[tool.tool_use_id])}
                {@const hasOutputText = Boolean(currentOutput && currentOutput.trim().length > 0)}

                <div
                  class="group/item flex flex-col rounded-md transition-colors hover:bg-muted/35 px-2 py-1 -mx-1"
                >
                  <!-- Row click target -->
                  <div
                    role="button"
                    tabindex="0"
                    class="flex items-center gap-2 text-xs leading-relaxed text-muted-foreground cursor-pointer select-none"
                    onclick={() => handleItemClick(tool, item)}
                    onkeydown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        handleItemClick(tool, item);
                      }
                    }}
                  >
                    <!-- Icon -->
                    <span
                      class="flex h-4 w-4 shrink-0 items-center justify-center text-muted-foreground/70 group-hover/item:text-foreground"
                    >
                      {#if item.iconKind === "wrench"}
                        <svg
                          class="h-3.5 w-3.5"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1.8"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        >
                          <path
                            d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"
                          />
                        </svg>
                      {:else if item.iconKind === "globe"}
                        <svg
                          class="h-3.5 w-3.5"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1.8"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        >
                          <circle cx="12" cy="12" r="10" />
                          <path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20" />
                          <path d="M2 12h20" />
                        </svg>
                      {:else if item.iconKind === "terminal"}
                        <span
                          class="font-mono text-[10px] text-muted-foreground/70 group-hover/item:text-foreground font-semibold select-none"
                          >&gt;_</span
                        >
                      {:else if item.iconKind === "book"}
                        <svg
                          class="h-3.5 w-3.5"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1.8"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        >
                          <path
                            d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1-2.5-2.5Z"
                          />
                          <path d="M6 6h10" /><path d="M6 10h10" />
                        </svg>
                      {:else if item.iconKind === "search"}
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
                      {:else if item.iconKind === "pencil"}
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
                      {:else}
                        <svg
                          class="h-3.5 w-3.5"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1.8"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        >
                          <circle cx="12" cy="12" r="10" />
                        </svg>
                      {/if}
                    </span>

                    <!-- Prefix (e.g. 已读取 / 已运行 / 已搜索) -->
                    <span class="text-muted-foreground/85 shrink-0 select-none font-normal">
                      {item.prefix}
                    </span>

                    <!-- Target (Highlighted mono text) -->
                    <span
                      class="min-w-0 truncate font-mono text-foreground/90 font-normal {item.filePath
                        ? 'hover:underline text-blue-400'
                        : ''}"
                      title={item.command || item.filePath || item.pattern || item.target}
                    >
                      {item.target}
                    </span>

                    <!-- Right side affordance -->
                    <div class="ml-auto flex items-center gap-1.5 shrink-0">
                      {#if item.filePath && onPreviewFile}
                        <span
                          class="text-[10px] text-muted-foreground/50 group-hover/item:text-foreground flex items-center gap-0.5 transition-colors"
                        >
                          <span>预览</span>
                          <svg
                            class="h-3 w-3"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                          >
                            <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                            <polyline points="15 3 21 3 21 9" />
                            <line x1="10" y1="14" x2="21" y2="3" />
                          </svg>
                        </span>
                      {:else}
                        <!-- Expand / Collapse chevron for command output / search results -->
                        <span
                          class="text-[10px] text-muted-foreground/50 group-hover/item:text-foreground flex items-center gap-1 transition-colors"
                        >
                          {#if isLoading}
                            <span
                              class="h-3 w-3 animate-spin rounded-full border border-current border-t-transparent"
                            ></span>
                          {:else}
                            <span class="opacity-0 group-hover/item:opacity-100 transition-opacity">
                              {isOutputExpanded ? "收起" : "输出"}
                            </span>
                            <svg
                              class="h-3.5 w-3.5 transition-transform duration-150 {isOutputExpanded
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
                        </span>
                      {/if}
                    </div>
                  </div>

                  <!-- Output drawer if expanded -->
                  {#if isOutputExpanded}
                    <div
                      class="mt-1.5 ml-6 overflow-hidden rounded-md border border-border/50 bg-muted/60 dark:bg-[#16161e] p-2.5 font-mono text-[11px] text-foreground/90 animate-fade-in shadow-xs"
                    >
                      {#if isLoading}
                        <div
                          class="flex items-center gap-2 py-1 text-muted-foreground text-xs font-sans"
                        >
                          <span
                            class="h-3.5 w-3.5 animate-spin rounded-full border border-current border-t-transparent"
                          ></span>
                          <span>正在获取输出...</span>
                        </div>
                      {:else if hasOutputText}
                        <div
                          class="flex items-center justify-between pb-1.5 mb-1.5 border-b border-border/30 text-[10px] text-muted-foreground font-sans"
                        >
                          <span>{item.command ? "终端输出" : "输出详情"}</span>
                          <button
                            type="button"
                            class="hover:text-foreground transition-colors cursor-pointer px-1.5 py-0.5 rounded hover:bg-muted/80"
                            onclick={(e) => {
                              e.stopPropagation();
                              handleCopyOutput(tool.tool_use_id, currentOutput);
                            }}
                          >
                            {copiedToolId === tool.tool_use_id ? "已复制 ✓" : "复制"}
                          </button>
                        </div>
                        <div class="max-h-60 overflow-y-auto pr-1">
                          <pre
                            class="m-0 whitespace-pre-wrap break-all leading-relaxed">{currentOutput}</pre>
                        </div>
                      {:else}
                        <div
                          class="py-1 text-muted-foreground/60 italic text-center text-xs font-sans"
                        >
                          (已执行完毕，无文本输出)
                        </div>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/each}

              <!-- Subtle link to switch to full card view -->
              <div class="pt-1.5 flex justify-end">
                <button
                  type="button"
                  class="text-[10px] text-muted-foreground/40 hover:text-muted-foreground transition-colors cursor-pointer"
                  onclick={() => (showFullCards = true)}
                >
                  切换为调试详情卡片
                </button>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}

    <!-- Interactive Tools (Always rendered as full cards so user can approve/answer) -->
    {#if interactiveTools.length > 0}
      <div class="space-y-2 py-1">
        {#each interactiveTools as tool (tool.tool_use_id)}
          <InlineToolCard
            {tool}
            subTimeline={subTimelineByToolId.get(tool.tool_use_id)}
            {runId}
            {fetchToolResult}
            onAnswer={tool.tool_name === "AskUserQuestion" &&
            (tool.status === "running" || tool.status === "ask_pending")
              ? (answer) => onAnswer?.(answer)
              : undefined}
            {onApprove}
            {onPermissionRespond}
            {onExitPlanClearContext}
            {taskNotifications}
            planContent={planContentByToolId.get(tool.tool_use_id)}
            {showPermissionInPanel}
            {agentDisplayName}
            {onPreviewFile}
          />
        {/each}
      </div>
    {/if}
  </div>
</div>
