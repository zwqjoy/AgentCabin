<script lang="ts">
  import { onMount } from "svelte";
  import { getInboxSessionRun, listWorkTasks } from "$lib/api/work";
  import { inboxStore } from "$lib/stores/inbox-store.svelte";
  import WorkPendingActionItem from "./WorkPendingActionItem.svelte";
  import {
    formatDirectoryPath,
    getDirectoryPathFromItem,
    getInteractionDescription,
    getInteractionTitle,
    isAccessRootRequest,
  } from "$lib/utils/work-interactions";
  import type {
    InboxItem,
    InboxItemStatus,
    InboxItemType,
    WorkTask,
    WorkWorkspaceSummary,
  } from "$lib/types/work";

  let {
    workspaceId,
    workspaces = [],
  }: { workspaceId?: string; workspaces?: WorkWorkspaceSummary[] } = $props();

  let filterStatus = $state<"all" | "pending">("pending");
  let deletingId = $state<string | null>(null);
  let isClearing = $state(false);
  let showConfirmClear = $state(false);
  let itemErrors = $state<Record<string, string>>({});
  let detailsOpenMap = $state<Record<string, boolean>>({});
  let tasks = $state<WorkTask[]>([]);
  let sessionRunIds = $state<Record<string, string>>({});

  onMount(() => {
    void loadInboxContext();
  });

  async function loadInboxContext(): Promise<void> {
    await inboxStore.fetch(false);
    try {
      tasks = await listWorkTasks(workspaceId || undefined);
    } catch {
      tasks = [];
    }
    await loadSessionRunIds(inboxStore.items);
  }

  async function loadSessionRunIds(items: InboxItem[]): Promise<void> {
    const candidates = items.filter((item) => item.status === "pending");
    const resolved = await Promise.all(
      candidates.map(async (item) => {
        try {
          const sessionRunId = await getInboxSessionRun(item.id);
          return sessionRunId ? ([item.id, sessionRunId] as const) : null;
        } catch {
          return null;
        }
      }),
    );
    const next = { ...sessionRunIds };
    for (const entry of resolved) {
      if (entry) next[entry[0]] = entry[1];
    }
    sessionRunIds = next;
  }

  $effect(() => {
    const pendingItems = inboxStore.pendingItems;
    if (pendingItems.length > 0) void loadSessionRunIds(pendingItems);
  });

  const displayedItems = $derived(
    filterStatus === "pending"
      ? inboxStore.pendingItems.filter((i) => !workspaceId || i.workspaceId === workspaceId)
      : inboxStore.items.filter((i) => !workspaceId || i.workspaceId === workspaceId),
  );

  const scopedPendingCount = $derived(
    inboxStore.pendingItems.filter((i) => !workspaceId || i.workspaceId === workspaceId).length,
  );

  const clearableItems = $derived(displayedItems.filter((item) => item.status !== "pending"));

  function shortId(value: string): string {
    return value ? value.slice(0, 8) : "未知";
  }

  function taskLabel(item: InboxItem): string {
    return tasks.find((task) => task.id === item.taskId)?.title || `任务 ${shortId(item.taskId)}`;
  }

  function workspaceLabel(item: InboxItem): string {
    return (
      workspaces.find((workspace) => workspace.id === item.workspaceId)?.name ||
      (item.workspaceId ? `工作区 ${shortId(item.workspaceId)}` : "独立任务")
    );
  }

  function taskHref(item: InboxItem): string | null {
    const sessionRunId = sessionRunIds[item.id];
    if (!sessionRunId) return null;
    const params = new URLSearchParams({ run: sessionRunId });
    if (item.workspaceId) params.set("workspace", item.workspaceId);
    return `/chat/work?${params.toString()}`;
  }

  function formatItemType(type: InboxItemType): string {
    switch (type) {
      case "permission_request":
        return "权限申请";
      case "question_elicitation":
        return "问题回答";
      case "plan_approval":
        return "方案审批";
      case "artifact_validation":
        return "成果验证";
      case "access_root_request":
        return "目录授权";
      case "app_connection_request":
        return "应用连接";
      case "connector_auth_request":
        return "服务授权";
      default:
        return type;
    }
  }

  function formatStatus(status: InboxItemStatus): { label: string; cls: string } {
    switch (status) {
      case "pending":
        return { label: "待处理", cls: "bg-amber-500/20 text-amber-600 dark:text-amber-400" };
      case "approved":
        return { label: "已批准", cls: "bg-emerald-500/20 text-emerald-600 dark:text-emerald-400" };
      case "rejected":
        return { label: "已拒绝", cls: "bg-red-500/20 text-red-600 dark:text-red-400" };
      case "answered":
        return { label: "已回答", cls: "bg-blue-500/20 text-blue-600 dark:text-blue-400" };
      case "cancelled":
        return { label: "已取消", cls: "bg-muted text-muted-foreground" };
      case "expired":
        return { label: "已过期", cls: "bg-muted text-muted-foreground" };
      default:
        return { label: status, cls: "bg-muted text-muted-foreground" };
    }
  }

  async function handleResolve(item: InboxItem, status: InboxItemStatus, response?: unknown) {
    itemErrors[item.id] = "";
    try {
      await inboxStore.resolve(item.id, status, response);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      itemErrors = { ...itemErrors, [item.id]: msg };
      throw err;
    }
  }

  async function handleDeleteItem(id: string) {
    deletingId = id;
    try {
      await inboxStore.deleteItem(id);
      const newErrors = { ...itemErrors };
      delete newErrors[id];
      itemErrors = newErrors;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      itemErrors = { ...itemErrors, [id]: msg };
    } finally {
      deletingId = null;
    }
  }

  async function executeClearAll() {
    isClearing = true;
    showConfirmClear = false;
    try {
      await inboxStore.clearAll(workspaceId, true);
      itemErrors = {};
    } finally {
      isClearing = false;
    }
  }

  function toggleDetails(id: string) {
    detailsOpenMap = {
      ...detailsOpenMap,
      [id]: !detailsOpenMap[id],
    };
  }

  function retryLoad() {
    void inboxStore.fetch(false);
  }
</script>

<div class="flex h-full flex-col bg-background text-foreground">
  <!-- Header -->
  <div class="flex items-center justify-between border-b border-border/40 px-4 py-3">
    <div class="flex items-center gap-2">
      <h2 class="text-sm font-semibold tracking-wide">待处理</h2>
      {#if scopedPendingCount > 0}
        <span
          class="rounded-full bg-amber-500/20 px-2 py-0.5 text-xs font-bold text-amber-600 dark:text-amber-400"
        >
          {scopedPendingCount} 待处理
        </span>
      {/if}
    </div>

    <!-- Actions & Filter Toggle -->
    <div class="flex items-center gap-3">
      {#if filterStatus === "all" && clearableItems.length > 0}
        <button
          type="button"
          disabled={isClearing}
          class="flex items-center gap-1.5 rounded-lg border border-red-500/30 bg-red-500/10 px-2.5 py-1 text-xs font-semibold text-red-600 dark:text-red-400 hover:bg-red-500/20 transition-colors disabled:opacity-50"
          onclick={() => (showConfirmClear = true)}
          title="清空已处理记录"
        >
          <svg
            class="w-3.5 h-3.5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
            />
          </svg>
          <span>{isClearing ? "清空中…" : `清空已处理 (${clearableItems.length})`}</span>
        </button>
      {/if}

      <div class="flex rounded-md border border-border/50 p-0.5 text-xs">
        <button
          class="rounded px-2.5 py-1 font-medium transition-colors {filterStatus === 'pending'
            ? 'bg-primary/10 text-primary'
            : 'text-muted-foreground hover:text-foreground'}"
          aria-pressed={filterStatus === "pending"}
          onclick={() => (filterStatus = "pending")}
        >
          待处理 ({scopedPendingCount})
        </button>
        <button
          class="rounded px-2.5 py-1 font-medium transition-colors {filterStatus === 'all'
            ? 'bg-primary/10 text-primary'
            : 'text-muted-foreground hover:text-foreground'}"
          aria-pressed={filterStatus === "all"}
          onclick={() => (filterStatus = "all")}
        >
          全部记录
        </button>
      </div>
    </div>
  </div>

  <!-- Clear Confirmation Banner/Modal -->
  {#if showConfirmClear}
    <div
      class="mx-4 mt-3 rounded-xl border border-red-500/30 bg-red-500/10 p-3.5 text-xs text-red-700 dark:text-red-300 space-y-2"
    >
      <div class="font-semibold flex items-center gap-1.5">
        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
          />
        </svg>
        <span>确定清空已处理记录吗？</span>
      </div>
      <p class="text-[11px] text-muted-foreground">
        此操作只会清除所选范围下已处理的历史记录，未处理事项会保留。
      </p>
      <div class="flex justify-end gap-2 pt-1">
        <button
          type="button"
          class="rounded-lg border border-border/70 bg-card px-3 py-1 text-xs font-medium text-foreground hover:bg-accent"
          onclick={() => (showConfirmClear = false)}
        >
          取消
        </button>
        <button
          type="button"
          class="rounded-lg bg-red-600 px-3 py-1 text-xs font-semibold text-white hover:bg-red-700 shadow-sm"
          onclick={executeClearAll}
        >
          确认清空
        </button>
      </div>
    </div>
  {/if}

  {#if inboxStore.error}
    <div
      class="mx-4 mt-3 flex items-center justify-between gap-3 rounded-xl border border-red-500/30 bg-red-500/10 p-3 text-xs text-red-700 dark:text-red-300"
      role="alert"
    >
      <span class="min-w-0 flex-1">{inboxStore.error}</span>
      <button
        type="button"
        class="shrink-0 rounded-lg border border-red-500/30 px-2.5 py-1 font-semibold hover:bg-red-500/10 disabled:opacity-50"
        disabled={inboxStore.loading}
        onclick={retryLoad}
      >
        {inboxStore.loading ? "重试中…" : "重试加载"}
      </button>
    </div>
  {/if}

  <!-- Content List -->
  <div class="flex-1 overflow-y-auto p-4 space-y-3">
    {#if inboxStore.loading && displayedItems.length === 0}
      <div class="py-8 text-center text-xs text-muted-foreground">正在加载事项…</div>
    {:else if displayedItems.length === 0}
      <div
        class="flex items-center justify-between rounded-xl border border-dashed border-border/60 bg-card/30 p-4 text-xs text-muted-foreground"
      >
        <div class="flex items-center gap-2">
          <svg
            viewBox="0 0 24 24"
            class="h-4 w-4 text-emerald-500 shrink-0"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <polyline points="20 6 9 17 4 12" />
          </svg>
          <span class="font-medium text-foreground/90">暂无待处理事项</span>
          <span class="text-muted-foreground/70"
            >· Agent
            在执行任务遇到目录授权、提问、方案审批或异常恢复时会在此暂停等待，处理后自动恢复执行</span
          >
        </div>
      </div>
    {:else}
      {#each displayedItems as item (item.id)}
        {#if item.status === "pending"}
          <WorkPendingActionItem
            {item}
            taskLabel={taskLabel(item)}
            workspaceLabel={workspaceLabel(item)}
            taskHref={taskHref(item)}
            onResolve={handleResolve}
          />
        {:else}
          {@const st = formatStatus(item.status)}
          {@const isDir = isAccessRootRequest(item)}
          {@const dirPath = isDir ? getDirectoryPathFromItem(item) : ""}
          {@const displayPath = isDir ? formatDirectoryPath(dirPath) : ""}
          {@const title = getInteractionTitle(item)}
          {@const description = getInteractionDescription(item)}
          {@const showDetails = Boolean(detailsOpenMap[item.id])}
          {@const itemError = itemErrors[item.id]}

          <div
            class="group relative rounded-xl border border-border/60 bg-card/60 p-4 shadow-sm transition-all hover:border-border space-y-3 opacity-90"
          >
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2">
                  <span
                    class="rounded bg-muted px-2 py-0.5 text-[10px] font-semibold text-muted-foreground"
                  >
                    {formatItemType(item.itemType)}
                  </span>
                  <span class="text-xs font-semibold text-foreground">{title}</span>
                </div>
                <div class="mt-1 flex items-center gap-1.5 text-[10px] text-muted-foreground">
                  <span class="font-semibold text-foreground/80">来自任务</span>
                  <span class="truncate">{taskLabel(item)}</span>
                  <span class="truncate text-muted-foreground/70">· {workspaceLabel(item)}</span>
                </div>
                {#if displayPath}
                  <div
                    class="mt-1 font-mono text-xs text-foreground bg-muted/50 rounded px-2 py-0.5 inline-block"
                  >
                    {displayPath}
                  </div>
                {/if}
                <p class="mt-1.5 text-xs text-muted-foreground leading-relaxed">{description}</p>
              </div>

              <div class="flex items-center gap-2 shrink-0">
                {#if taskHref(item)}
                  <a
                    href={taskHref(item) || undefined}
                    class="rounded px-2 py-1 text-[10px] font-semibold text-primary hover:bg-primary/10"
                    >回到任务</a
                  >
                {/if}
                <span class="rounded px-2 py-0.5 text-[10px] font-bold {st.cls}">
                  {st.label}
                </span>

                <button
                  type="button"
                  disabled={deletingId === item.id}
                  class="opacity-60 hover:opacity-100 p-1 text-muted-foreground hover:text-destructive hover:bg-destructive/10 rounded transition-all"
                  title="删除记录"
                  onclick={() => handleDeleteItem(item.id)}
                >
                  {#if deletingId === item.id}
                    <span class="text-[10px]">…</span>
                  {:else}
                    <svg
                      class="w-3.5 h-3.5"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                    >
                      <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                      />
                    </svg>
                  {/if}
                </button>
              </div>
            </div>

            {#if item.payload.toolName || item.payload.parameters}
              <div>
                <button
                  type="button"
                  class="text-[10px] text-muted-foreground/80 hover:text-foreground"
                  onclick={() => toggleDetails(item.id)}
                >
                  {showDetails ? "收起详情" : "查看详情"}
                </button>
                {#if showDetails}
                  <div
                    class="mt-1 space-y-1 rounded bg-muted/30 p-2 text-[10px] text-muted-foreground font-mono"
                  >
                    {#if item.payload.toolName}
                      <div>工具: {item.payload.toolName}</div>
                    {/if}
                    {#if item.payload.parameters}
                      <pre class="max-h-24 overflow-auto">{JSON.stringify(
                          item.payload.parameters,
                          null,
                          2,
                        )}</pre>
                    {/if}
                  </div>
                {/if}
              </div>
            {/if}

            {#if itemError}
              <div
                class="flex items-center justify-between rounded-lg bg-destructive/10 border border-destructive/20 px-3 py-2 text-xs text-destructive"
              >
                <span class="truncate">{itemError}</span>
                <button
                  type="button"
                  class="underline ml-2 shrink-0 text-[11px] font-semibold hover:text-destructive/80"
                  onclick={() => handleDeleteItem(item.id)}
                >
                  重试删除
                </button>
              </div>
            {/if}
          </div>
        {/if}
      {/each}
    {/if}
  </div>
</div>
