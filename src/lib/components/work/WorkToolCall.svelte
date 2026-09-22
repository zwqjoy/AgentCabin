<script lang="ts">
  import {
    formatDirectoryPath,
    getInteractionDescription,
    getInteractionTitle,
    isAccessRootRequest,
    isQuestionInteraction,
  } from "$lib/utils/work-interactions";
  import { workToolLabel } from "$lib/utils/work-activity";
  import { classifyWorkExecutionFailure } from "$lib/utils/work-execution-failure";
  import type { TimelineEntry } from "$lib/types";
  import type { InboxItem, InboxItemStatus } from "$lib/types/work";
  import WorkPendingActionItem from "$lib/components/work/WorkPendingActionItem.svelte";

  type ToolEntry = Extract<TimelineEntry, { kind: "tool" }>;

  let {
    entry,
    nested = false,
    pendingInteraction = null,
    resolvedInteraction = null,
    onResolveInteraction,
  }: {
    entry: ToolEntry;
    nested?: boolean;
    pendingInteraction?: InboxItem | null;
    resolvedInteraction?: InboxItem | null;
    onResolveInteraction?: (
      item: InboxItem,
      status: InboxItemStatus,
      response?: unknown,
    ) => Promise<void>;
  } = $props();

  function stringify(value: unknown): string {
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return String(value);
    }
  }

  function preview(value: unknown, maxChars = 12000): string {
    const text = stringify(value);
    return text.length > maxChars ? `${text.slice(0, maxChars)}\n…（结果已截断）` : text;
  }

  function isBrowserTool(name: string): boolean {
    return name.startsWith("web_") || name.startsWith("browser_");
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

  function isToolExecutionFailed(status: string, output: unknown): boolean {
    if (
      status === "error" ||
      status === "failed" ||
      status === "denied" ||
      status === "permission_denied" ||
      status === "timeout" ||
      status === "timed_out" ||
      status === "cancelled" ||
      status === "canceled" ||
      status === "rejected"
    ) {
      return true;
    }
    if (output && typeof output === "object") {
      const rec = output as Record<string, unknown>;
      if (rec.ok === false) return true;
      if (rec.details && typeof rec.details === "object") {
        if ((rec.details as Record<string, unknown>).ok === false) return true;
      }
    }
    return false;
  }

  let browserFailed = $derived(
    isBrowserTool(entry.tool.tool_name) && browserToolFailed(entry.tool.output),
  );
  let isToolFailed = $derived(
    browserFailed || isToolExecutionFailed(entry.tool.status, entry.tool.output),
  );
  let executionFailure = $derived(
    isToolFailed ? classifyWorkExecutionFailure(entry.tool.status, entry.tool.output) : null,
  );

  let isDirAccess = $derived(entry.tool.tool_name === "work_request_directory_access");
  let inputPath = $derived(
    typeof entry.tool.input?.path === "string"
      ? (entry.tool.input.path as string)
      : typeof entry.tool.input?.target === "string"
        ? (entry.tool.input.target as string)
        : "",
  );
  let displayPath = $derived(formatDirectoryPath(inputPath));

  function getActionSummary(
    toolName: string,
    input: unknown,
    status: string,
    isFailed = false,
    failureLabel = "",
  ): { verb: string; target: string; isRunning: boolean; isFailed: boolean } {
    const isRunning = status === "running";
    const inp = (typeof input === "object" && input !== null ? input : {}) as Record<
      string,
      unknown
    >;

    // 1. File reading / exploration
    if (toolName === "work_read_file" || toolName === "read_file" || toolName === "view_file") {
      const rawPath = String(inp.path || inp.target || inp.AbsolutePath || "");
      const fileName = rawPath ? rawPath.split("/").pop() || rawPath : "文件";
      return {
        verb: isRunning ? "正在读取" : isFailed ? "读取失败" : "已读取",
        target: fileName,
        isRunning,
        isFailed,
      };
    }

    // 2. File list / directory exploration
    if (toolName === "work_list_files" || toolName === "list_dir" || toolName === "find_by_name") {
      const rawPath = String(
        inp.path || inp.target || inp.DirectoryPath || inp.SearchDirectory || "",
      );
      const dirName = rawPath ? rawPath.split("/").filter(Boolean).pop() || "/" : "目录";
      return {
        verb: isRunning ? "正在探索" : isFailed ? "探索失败" : "已探索",
        target: dirName,
        isRunning,
        isFailed,
      };
    }

    // 3. Search / Grep
    if (toolName === "grep_search" || toolName === "work_grep" || toolName === "search_web") {
      const query = String(inp.query || inp.Query || inp.pattern || "");
      return {
        verb: isRunning ? "正在检索" : isFailed ? "检索失败" : "已检索",
        target: query ? `"${query}"` : "代码",
        isRunning,
        isFailed,
      };
    }

    // 4. Browser search / open
    if (toolName === "web_search") {
      const query = String(inp.query || "");
      return {
        verb: isRunning ? "正在搜索" : isFailed ? "搜索失败" : "已搜索",
        target: query ? `"${query}"` : "网页",
        isRunning,
        isFailed,
      };
    }
    if (
      toolName === "web_open" ||
      toolName === "browser_navigate" ||
      toolName === "read_url_content"
    ) {
      const url = String(inp.url || inp.Url || "");
      let domain = url;
      try {
        domain = new URL(url).hostname;
      } catch {
        // fallback to url if parsing fails
      }
      return {
        verb: isRunning ? "正在访问" : isFailed ? "访问失败" : "已访问",
        target: domain || "网页",
        isRunning,
        isFailed,
      };
    }

    // 5. File writing / editing
    if (toolName === "work_write_file" || toolName === "write_to_file") {
      const rawPath = String(inp.path || inp.TargetFile || "");
      const fileName = rawPath ? rawPath.split("/").pop() || rawPath : "文件";
      return {
        verb: isRunning ? "正在写入" : isFailed ? "写入失败" : "已写入",
        target: fileName,
        isRunning,
        isFailed,
      };
    }
    if (
      toolName === "work_edit_file" ||
      toolName === "replace_file_content" ||
      toolName === "multi_replace_file_content"
    ) {
      const rawPath = String(inp.path || inp.TargetFile || "");
      const fileName = rawPath ? rawPath.split("/").pop() || rawPath : "文件";
      return {
        verb: isRunning ? "正在编辑" : isFailed ? "编辑失败" : "已更新",
        target: fileName,
        isRunning,
        isFailed,
      };
    }

    // 6. Command run
    if (
      toolName === "work_run_command" ||
      toolName === "run_command" ||
      toolName === "bash" ||
      toolName === "Bash"
    ) {
      const cmd = String(
        inp.CommandLine ||
          inp.command ||
          (Array.isArray(inp.args) ? inp.args.join(" ") : "") ||
          "命令",
      );
      const shortCmd = cmd.length > 50 ? cmd.slice(0, 48) + "…" : cmd;
      return {
        verb: isRunning ? "正在执行" : isFailed ? failureLabel || "执行失败" : "已执行",
        target: shortCmd,
        isRunning,
        isFailed,
      };
    }

    // 7. Subagents / Delegation
    if (toolName === "work_delegate" || toolName === "invoke_subagent") {
      const role = String(inp.role || "助手");
      const friendlyRole =
        role === "researcher"
          ? "研究助手"
          : role === "worker"
            ? "执行助手"
            : role === "reviewer"
              ? "审阅助手"
              : role;
      return {
        verb: isRunning ? "正在委派" : isFailed ? "委派失败" : "已委派",
        target: friendlyRole,
        isRunning,
        isFailed,
      };
    }

    // 8. Work plan / state
    if (
      toolName === "work_replace_plan" ||
      toolName === "work_update_step" ||
      toolName === "work_set_goal"
    ) {
      return {
        verb: isRunning ? "正在更新" : isFailed ? "更新失败" : "已更新",
        target: "任务计划",
        isRunning,
        isFailed,
      };
    }

    // 9. Work artifact
    if (
      toolName === "work_register_artifact" ||
      toolName === "work_deliver" ||
      toolName === "work_validate_artifact"
    ) {
      const title = String(inp.title || inp.artifact_id || inp.path || "成果");
      return {
        verb: isRunning ? "正在登记" : isFailed ? "登记失败" : "已登记",
        target: title,
        isRunning,
        isFailed,
      };
    }

    // Default fallback
    return {
      verb: isRunning ? "正在执行" : isFailed ? failureLabel || "执行失败" : "已执行",
      target: workToolLabel(toolName),
      isRunning,
      isFailed,
    };
  }

  let actionSummary = $derived(
    getActionSummary(
      entry.tool.tool_name,
      entry.tool.input,
      entry.tool.status,
      isToolFailed,
      executionFailure?.label,
    ),
  );

  let isPending = $derived(
    Boolean(pendingInteraction) ||
      (isDirAccess &&
        entry.tool.status === "running" &&
        !resolvedInteraction &&
        !entry.tool.output),
  );
  let isRecovery = $derived(
    Boolean(pendingInteraction?.payload.recoveryKey || pendingInteraction?.payload.recoveryAction),
  );

  let isDenied = $derived(
    resolvedInteraction?.status === "rejected" ||
      resolvedInteraction?.status === "cancelled" ||
      entry.tool.status === "denied" ||
      (typeof entry.tool.output === "object" &&
        entry.tool.output !== null &&
        "confirmed" in (entry.tool.output as Record<string, unknown>) &&
        (entry.tool.output as Record<string, unknown>).confirmed === false),
  );

  let isApproved = $derived(
    resolvedInteraction?.status === "approved" ||
      (isDirAccess && entry.tool.status === "success") ||
      (typeof entry.tool.output === "object" &&
        entry.tool.output !== null &&
        "ok" in (entry.tool.output as Record<string, unknown>) &&
        (entry.tool.output as Record<string, unknown>).ok === true),
  );

  let isDelegate = $derived(entry.tool.tool_name === "work_delegate");
  let delegateInput = $derived((entry.tool.input ?? {}) as Record<string, unknown>);
  let delegateRole = $derived(String(delegateInput.role ?? "researcher").toLowerCase());
  let delegateTask = $derived(String(delegateInput.task ?? ""));
  let delegateOutput = $derived(
    entry.tool.output && typeof entry.tool.output === "object"
      ? (entry.tool.output as Record<string, unknown>)
      : {},
  );
  let delegateDetails = $derived(
    delegateOutput.details && typeof delegateOutput.details === "object"
      ? (delegateOutput.details as Record<string, unknown>)
      : delegateOutput,
  );
  let delegateAgentId = $derived(String(delegateDetails.agent_id ?? ""));
  let isDelegateRunning = $derived(entry.tool.status === "running");
  let isDelegateFailed = $derived(entry.tool.status === "error" || delegateDetails.ok === false);
</script>

{#if isDelegate && !isPending}
  <!-- Subagent Activity Item (Flat Process Timeline Row) -->
  <div
    class="{nested
      ? 'w-full py-0.5'
      : 'chat-content-width py-0.5'} animate-fade-in text-[var(--chat-secondary-size,13px)]"
  >
    <details class="group/work-delegate">
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

        <span class="font-medium text-foreground/80 text-[13px]">
          {delegateRole === "researcher"
            ? "Researcher 调研助手"
            : delegateRole === "worker"
              ? "Worker 执行助手"
              : delegateRole === "reviewer"
                ? "Reviewer 审阅助手"
                : `${delegateRole} 助手`}
        </span>

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
          class="h-3 w-3 text-muted-foreground/40 transition-transform group-open/work-delegate:rotate-90"
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
      </div>
    </details>
  </div>
{:else if isPending && pendingInteraction && isQuestionInteraction(pendingInteraction)}
  <div class={nested ? "w-full my-1" : "chat-content-width py-2"}>
    <WorkPendingActionItem item={pendingInteraction} onResolve={onResolveInteraction} />
  </div>
{:else if isPending && pendingInteraction}
  <!-- Approvals, authorizations, and recovery stay in Inbox; questions render inline below. -->
  {@const title = getInteractionTitle(pendingInteraction)}
  {@const desc = getInteractionDescription(pendingInteraction)}
  <div class="{nested ? 'w-full my-1' : 'chat-content-width py-2'} animate-fade-in">
    <div
      class="flex items-center gap-3 rounded-xl border border-orange-500/30 bg-orange-500/[0.06] px-3 py-2.5"
    >
      <span
        class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-orange-500/10 text-orange-600 dark:text-orange-400"
      >
        {#if isDirAccess || isAccessRootRequest(pendingInteraction)}
          <svg
            viewBox="0 0 24 24"
            class="h-4 w-4"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            aria-hidden="true"
          >
            <path d="M3.5 6.5h6l2 2h9v9a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z" />
          </svg>
        {:else}
          <svg
            viewBox="0 0 24 24"
            class="h-4 w-4"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            aria-hidden="true"
          >
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
        {/if}
      </span>
      <div class="min-w-0 flex-1">
        <div class="flex flex-wrap items-center gap-2">
          <h4 class="text-xs font-semibold text-foreground">{title}</h4>
          <span
            class="rounded-full bg-orange-500/15 px-2 py-0.5 text-[10px] font-semibold text-orange-600 dark:text-orange-400"
          >
            {isRecovery ? "等待恢复确认" : "等待你的处理"}
          </span>
        </div>
        <p class="mt-0.5 truncate text-[11px] text-muted-foreground">{desc}</p>
        {#if displayPath}
          <p class="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">{displayPath}</p>
        {/if}
      </div>
      <a
        href="/chat/work?view=inbox"
        class="shrink-0 rounded-lg bg-orange-500/15 px-2.5 py-1.5 text-[11px] font-semibold text-orange-700 transition-colors hover:bg-orange-500/25 dark:text-orange-300"
      >
        打开 Inbox →
      </a>
    </div>
  </div>
{:else if isDirAccess && isApproved}
  <!-- Resolved: Approved directory access card -->
  <details class="{nested ? 'w-full py-0.5' : 'chat-content-width py-1.5'} text-sm">
    <summary
      class="group flex min-h-8 cursor-pointer select-none list-none items-center gap-3 rounded-lg px-0.5 py-1 text-muted-foreground transition-colors hover:bg-muted/45 [&::-webkit-details-marker]:hidden"
    >
      <span
        class="flex h-5 w-5 shrink-0 items-center justify-center text-emerald-600 dark:text-emerald-400"
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
          <path d="m5 12 4 4L19 6" />
        </svg>
      </span>
      <span class="min-w-0 flex-1 truncate font-medium text-foreground">
        已允许访问 {displayPath || inputPath}
      </span>
      <svg
        class="h-4 w-4 shrink-0 text-emerald-600 dark:text-emerald-400"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-label="已完成"
      >
        <path d="m5 12 4 4L19 6" />
      </svg>
    </summary>
    <div class="ml-8 mt-1 space-y-2 rounded-xl border border-border/45 bg-muted/20 px-3 py-2.5">
      <div class="text-[10px] text-muted-foreground/70">
        执行详情：<code class="font-mono">{entry.tool.tool_name}</code>
      </div>
      <div>
        <div class="mb-0.5 text-[10px] font-medium text-muted-foreground">授权路径</div>
        <div class="font-mono text-xs text-foreground">{inputPath}</div>
      </div>
      {#if entry.tool.output !== undefined}
        <div>
          <div class="mb-0.5 text-[10px] font-medium text-muted-foreground">工具结果</div>
          <pre
            class="max-h-56 overflow-auto rounded-md bg-background/70 px-2 py-1.5 font-mono text-[10px] leading-4 text-muted-foreground">{preview(
              entry.tool.output,
            )}</pre>
        </div>
      {/if}
    </div>
  </details>
{:else if isDirAccess && isDenied}
  <!-- Resolved: Denied directory access card -->
  <details
    class="{nested
      ? 'w-full py-0.5'
      : 'chat-content-width py-1.5'} text-sm text-red-600 dark:text-red-400"
  >
    <summary
      class="group flex min-h-8 cursor-pointer select-none list-none items-center gap-3 rounded-lg px-0.5 py-1 text-muted-foreground transition-colors hover:bg-muted/45 [&::-webkit-details-marker]:hidden"
    >
      <span class="flex h-5 w-5 shrink-0 items-center justify-center text-red-500">
        <svg
          class="h-4 w-4"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </span>
      <span class="min-w-0 flex-1 truncate font-medium text-foreground">
        已拒绝访问 {displayPath || inputPath}
      </span>
      <span class="shrink-0 text-xs font-medium text-red-500">已拒绝</span>
    </summary>
    <div class="ml-8 mt-1 space-y-2 rounded-xl border border-border/45 bg-muted/20 px-3 py-2.5">
      <div class="text-[10px] text-muted-foreground/70">
        执行详情：<code class="font-mono">{entry.tool.tool_name}</code>
      </div>
      <div>
        <div class="mb-0.5 text-[10px] font-medium text-muted-foreground">申请路径</div>
        <div class="font-mono text-xs text-foreground">{inputPath}</div>
      </div>
    </div>
  </details>
{:else}
  <!-- Standard Tool Call Entry (Cursor-style action chip) -->
  <details class="{nested ? 'w-full py-0.5' : 'chat-content-width py-0.5'} text-xs group">
    <summary
      class="flex min-h-6 cursor-pointer select-none list-none items-center gap-2 rounded-md px-1 py-0.5 text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground [&::-webkit-details-marker]:hidden"
    >
      <span class="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
        {#if isToolFailed}
          <span class="h-1.5 w-1.5 rounded-full bg-rose-500"></span>
        {:else if entry.tool.status === "running"}
          <span
            class="h-2.5 w-2.5 animate-spin rounded-full border-[1.5px] border-muted-foreground/40 border-t-transparent"
          ></span>
        {:else}
          <svg
            class="h-3 w-3 text-muted-foreground/50 transition-transform group-open:rotate-90 group-hover:text-foreground"
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
        {/if}
      </span>
      <span
        class="truncate font-medium {isToolFailed
          ? 'text-red-600 dark:text-red-400'
          : 'text-muted-foreground'} group-hover:text-foreground"
      >
        <span>{actionSummary.verb}</span>
        <span class="ml-1 font-mono text-foreground/80 font-normal">{actionSummary.target}</span>
      </span>
      {#if entry.tool.duration_ms != null && entry.tool.duration_ms > 0}
        <span class="shrink-0 text-[10px] text-muted-foreground/40">{entry.tool.duration_ms}ms</span
        >
      {/if}
    </summary>
    <div class="ml-5 mt-1 space-y-2 rounded-lg border border-border/40 bg-muted/20 p-2 text-[11px]">
      {#if executionFailure}
        <div
          class="rounded-md border border-red-500/25 bg-red-500/5 px-2.5 py-2 text-red-700 dark:text-red-300"
          role="alert"
        >
          <div class="flex items-center justify-between gap-2">
            <span class="text-[10px] font-semibold">失败类型</span>
            <span class="rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] font-semibold"
              >{executionFailure.label}</span
            >
          </div>
          <p class="mt-1 text-[10px] leading-4 text-red-700/80 dark:text-red-300/80">
            {executionFailure.description}
          </p>
          {#if executionFailure.exitCode !== undefined}
            <p class="mt-1 font-mono text-[10px] text-red-700/70 dark:text-red-300/70">
              exit code: {executionFailure.exitCode}
            </p>
          {/if}
        </div>
      {/if}
      <div>
        <span class="text-[10px] text-muted-foreground">工具：</span>
        <code class="font-mono text-foreground">{entry.tool.tool_name}</code>
      </div>
      {#if entry.tool.input}
        <div>
          <div class="text-[10px] text-muted-foreground mb-0.5">参数：</div>
          <pre
            class="max-h-32 overflow-auto rounded bg-background/80 p-2 font-mono text-[10px] leading-4 text-foreground/90 border border-border/40">{preview(
              entry.tool.input,
            )}</pre>
        </div>
      {/if}
      {#if entry.tool.output !== undefined}
        <div>
          {#if isBrowserTool(entry.tool.tool_name)}
            {@const details = browserDetails(entry.tool.output)}
            <div class="mb-0.5 text-[10px] font-medium text-muted-foreground">网络访问来源</div>
            {#if browserFailed}
              <div
                class="rounded-md border border-red-500/20 bg-red-500/5 px-2 py-1.5 text-[10px] leading-4 text-red-600 dark:text-red-300"
                role="alert"
              >
                {browserFailureMessage(entry.tool.output)}
              </div>
            {:else if entry.tool.tool_name === "web_search"}
              <div class="space-y-1.5">
                {#each browserArray(details.results) as source}
                  <a
                    class="block rounded-md border border-border/50 bg-background/70 px-2 py-1.5 hover:border-primary/50"
                    href={String(source.url ?? "#")}
                    target="_blank"
                    rel="noreferrer"
                  >
                    <div class="text-[11px] font-medium text-foreground">
                      {String(source.title ?? source.url ?? "来源")}
                    </div>
                    <div class="mt-0.5 line-clamp-2 text-[10px] leading-4 text-muted-foreground">
                      {String(source.snippet ?? "")}
                    </div>
                  </a>
                {/each}
              </div>
            {:else if entry.tool.tool_name === "web_cite"}
              <div
                class="rounded-md border border-emerald-500/20 bg-emerald-500/5 px-2 py-1.5 text-[11px] leading-4 text-foreground"
              >
                {browserCitation(details)}
              </div>
            {:else if entry.tool.tool_name === "web_extract"}
              <div class="space-y-1.5">
                {#each browserArray(details.passages) as passage}
                  <div
                    class="rounded-md border border-border/50 bg-background/70 px-2 py-1.5 text-[10px] leading-4 text-muted-foreground"
                  >
                    {String(passage.text ?? "")}
                  </div>
                {/each}
              </div>
            {:else}
              <div
                class="rounded-md border border-border/50 bg-background/70 px-2 py-1.5 text-[10px] leading-4 text-muted-foreground"
              >
                {String(details.title ?? details.url ?? details.preview ?? "页面已保存到来源账本")}
              </div>
            {/if}
            <details class="mt-1.5">
              <summary class="cursor-pointer text-[10px] text-muted-foreground/70"
                >查看结构化结果</summary
              >
              <pre
                class="mt-1 max-h-56 overflow-auto rounded-md bg-background/70 px-2 py-1.5 font-mono text-[10px] leading-4 text-muted-foreground">{preview(
                  entry.tool.output,
                )}</pre>
            </details>
          {:else}
            <div class="mb-0.5 text-[10px] font-medium text-muted-foreground">工具结果</div>
            <pre
              class="max-h-56 overflow-auto rounded-md bg-background/70 px-2 py-1.5 font-mono text-[10px] leading-4 text-muted-foreground">{preview(
                entry.tool.output,
              )}</pre>
          {/if}
        </div>
      {/if}
    </div>
  </details>
{/if}
