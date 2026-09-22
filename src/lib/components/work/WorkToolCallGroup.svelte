<script lang="ts">
  import type { TimelineEntry } from "$lib/types";
  import type { InboxItem, InboxItemStatus } from "$lib/types/work";
  import { classifyWorkExecutionFailure } from "$lib/utils/work-execution-failure";
  import WorkToolCall from "$lib/components/work/WorkToolCall.svelte";

  type ToolEntry = Extract<TimelineEntry, { kind: "tool" }>;
  let {
    entries,
    findPendingInteraction,
    findResolvedInteraction,
    onResolveInteraction,
  }: {
    entries: ToolEntry[];
    findPendingInteraction?: (toolEntry: ToolEntry) => InboxItem | null;
    findResolvedInteraction?: (toolEntry: ToolEntry) => InboxItem | null;
    onResolveInteraction?: (
      item: InboxItem,
      status: InboxItemStatus,
      response?: unknown,
    ) => Promise<void>;
  } = $props();

  let expanded = $state(false);

  let pendingEntries = $derived(
    findPendingInteraction ? entries.filter((entry) => Boolean(findPendingInteraction(entry))) : [],
  );
  let pendingCount = $derived(pendingEntries.length);

  function isStatusFailed(status: string): boolean {
    return (
      status === "error" ||
      status === "failed" ||
      status === "denied" ||
      status === "permission_denied" ||
      status === "timeout" ||
      status === "timed_out" ||
      status === "cancelled" ||
      status === "canceled" ||
      status === "rejected"
    );
  }

  let failedCount = $derived(
    entries.filter((entry) => {
      if (isStatusFailed(entry.tool.status)) {
        return true;
      }
      if (entry.tool.output && typeof entry.tool.output === "object") {
        const rec = entry.tool.output as Record<string, unknown>;
        if (rec.ok === false) return true;
        if (
          rec.details &&
          typeof rec.details === "object" &&
          (rec.details as Record<string, unknown>).ok === false
        ) {
          return true;
        }
      }
      return false;
    }).length,
  );
  let failureSummary = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const entry of entries) {
      const output = entry.tool.output;
      const details =
        output?.details && typeof output.details === "object"
          ? (output.details as Record<string, unknown>)
          : output;
      const failed =
        isStatusFailed(entry.tool.status) || output?.ok === false || details?.ok === false;
      if (!failed) continue;
      const failure = classifyWorkExecutionFailure(entry.tool.status, entry.tool.output);
      if (failure) counts.set(failure.label, (counts.get(failure.label) ?? 0) + 1);
    }
    return [...counts.entries()].map(([label, count]) => `${label} ${count}`).join("，");
  });
  let runningCount = $derived(entries.filter((entry) => entry.tool.status === "running").length);
  let completedCount = $derived(entries.filter((entry) => entry.tool.status === "success").length);
  let groupActionSummary = $derived.by(() => {
    if (entries.length === 0) return { title: "执行详情", subtitle: "" };

    const fileReads = entries.filter((e) =>
      ["work_read_file", "read_file", "view_file"].includes(e.tool.tool_name),
    ).length;
    const dirExplores = entries.filter((e) =>
      ["work_list_files", "list_dir", "find_by_name"].includes(e.tool.tool_name),
    ).length;
    const searches = entries.filter((e) =>
      ["grep_search", "work_grep", "search_web", "web_search"].includes(e.tool.tool_name),
    ).length;
    const commands = entries.filter((e) =>
      ["work_run_command", "run_command", "bash", "Bash"].includes(e.tool.tool_name),
    ).length;
    const edits = entries.filter((e) =>
      [
        "work_write_file",
        "write_to_file",
        "work_edit_file",
        "replace_file_content",
        "multi_replace_file_content",
      ].includes(e.tool.tool_name),
    ).length;

    // Pure files/dirs
    if (fileReads + dirExplores === entries.length) {
      const parts: string[] = [];
      if (dirExplores > 0) parts.push(`${dirExplores} 个目录`);
      if (fileReads > 0) parts.push(`${fileReads} 个文件`);
      const verb = runningCount > 0 ? "正在探索" : "已探索";
      return {
        title: `${verb} ${parts.join("，")}`,
        subtitle: `${completedCount}/${entries.length} 完成`,
      };
    }

    // Pure searches
    if (searches === entries.length) {
      const verb = runningCount > 0 ? "正在检索" : "已检索";
      return {
        title: `${verb} ${searches} 处代码/网页`,
        subtitle: `${completedCount}/${entries.length} 完成`,
      };
    }

    // Pure commands
    if (commands === entries.length) {
      const verb = runningCount > 0 ? "正在执行" : "已执行";
      return {
        title: `${verb} ${commands} 条命令`,
        subtitle: `${completedCount}/${entries.length} 完成`,
      };
    }

    // Pure edits
    if (edits === entries.length) {
      const verb = runningCount > 0 ? "正在修改" : "已修改";
      return {
        title: `${verb} ${edits} 个文件`,
        subtitle: `${completedCount}/${entries.length} 完成`,
      };
    }

    // Mixed actions
    const parts: string[] = [];
    if (dirExplores > 0) parts.push(`${dirExplores} 目录`);
    if (fileReads > 0) parts.push(`${fileReads} 文件`);
    if (searches > 0) parts.push(`${searches} 检索`);
    if (commands > 0) parts.push(`${commands} 命令`);
    if (edits > 0) parts.push(`${edits} 编辑`);

    const verb = runningCount > 0 ? "正在执行" : "已执行";
    const detail = parts.length > 0 ? ` (${parts.join("，")})` : "";
    return {
      title: `${verb} ${entries.length} 个动作${detail}`,
      subtitle: `${completedCount}/${entries.length} 完成`,
    };
  });

  let statusLabel = $derived(
    pendingCount > 0
      ? `${pendingCount} 项等待处理`
      : failedCount > 0
        ? `${failedCount} 项失败${failureSummary ? ` · ${failureSummary}` : ""}`
        : runningCount > 0
          ? "进行中"
          : `${completedCount}/${entries.length} 完成`,
  );
  let statusClass = $derived(
    pendingCount > 0
      ? "text-orange-600 dark:text-orange-400 font-medium"
      : failedCount > 0
        ? "text-rose-500"
        : "text-muted-foreground/70 font-normal",
  );
</script>

<div class="w-full py-0.5">
  <div class="chat-content-width">
    <div class="text-[var(--chat-secondary-size,13px)]">
      <button
        type="button"
        class="group flex min-h-6 w-full items-center gap-2 rounded px-1 py-0.5 text-left text-[var(--chat-secondary-size,13px)] text-muted-foreground/80 transition-colors hover:bg-muted/30 hover:text-foreground"
        aria-expanded={expanded}
        onclick={() => (expanded = !expanded)}
      >
        <svg
          class="h-3 w-3 shrink-0 text-muted-foreground/50 transition-transform {expanded
            ? 'rotate-90'
            : ''}"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"><path d="m9 18 6-6-6-6" /></svg
        >
        <span class="min-w-0 truncate font-normal text-foreground/80"
          >{groupActionSummary.title}</span
        >
        <span class="ml-auto flex shrink-0 items-center gap-1.5 {statusClass}" title={statusLabel}>
          {#if pendingCount > 0}
            <span class="h-1.5 w-1.5 rounded-full bg-orange-500"></span>
          {:else if failedCount > 0}
            <span class="h-1.5 w-1.5 rounded-full bg-rose-500"></span>
          {:else if runningCount > 0}
            <span
              class="h-2.5 w-2.5 animate-spin rounded-full border-[1.5px] border-muted-foreground/40 border-t-transparent"
            ></span>
          {/if}
          <span class="text-[11px] text-muted-foreground/50">{statusLabel}</span>
        </span>
      </button>

      {#if !expanded && pendingCount > 0}
        <div class="mt-1.5 space-y-1.5">
          {#each pendingEntries as entry (entry.id)}
            <WorkToolCall
              {entry}
              nested
              pendingInteraction={findPendingInteraction ? findPendingInteraction(entry) : null}
              resolvedInteraction={findResolvedInteraction ? findResolvedInteraction(entry) : null}
              {onResolveInteraction}
            />
          {/each}
        </div>
      {/if}

      {#if expanded}
        <div class="ml-3 mt-0.5 space-y-0.5 border-l border-border/40 pl-2">
          {#each entries as entry (entry.id)}
            <WorkToolCall
              {entry}
              nested
              pendingInteraction={findPendingInteraction ? findPendingInteraction(entry) : null}
              resolvedInteraction={findResolvedInteraction ? findResolvedInteraction(entry) : null}
              {onResolveInteraction}
            />
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>
