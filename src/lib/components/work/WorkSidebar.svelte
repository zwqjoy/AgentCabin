<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import { onMount } from "svelte";
  import { inboxStore } from "$lib/stores/inbox-store.svelte";
  import { WorkSidebarStore } from "$lib/stores/work-sidebar-store.svelte";
  import { getTransport } from "$lib/transport";
  import Modal from "$lib/components/Modal.svelte";
  import ConversationItem from "$lib/components/ConversationItem.svelte";
  import SidebarSectionLabel from "$lib/components/SidebarSectionLabel.svelte";
  import SidebarFolderRow from "$lib/components/SidebarFolderRow.svelte";
  import type { TaskRun } from "$lib/types";
  import type { WorkWorkspaceSummary } from "$lib/types/work";
  import { stripExpertTag } from "$lib/utils/expert-context";
  import { conversationCanDelete, type ConversationGroup } from "$lib/utils/sidebar-groups";
  import { getWorkActiveRunId, getWorkspaceRowAction } from "$lib/utils/work-sidebar-navigation";
  import { getWorkSessionAttentionLabel } from "$lib/utils/work-sidebar-attention";
  import { t } from "$lib/i18n/index.svelte";

  // ── Lifecycle store: all loading/mutation logic lives here ─────────────────
  const sidebar = new WorkSidebarStore();
  const workTransportSupported = getTransport().isDesktop();

  let pageUrl = $derived($page.url);
  let selectedId = $derived(pageUrl.searchParams.get("workspace") ?? "");
  let selectedRunId = $derived(pageUrl.searchParams.get("run") ?? "");
  let activeRunId = $derived(getWorkActiveRunId(selectedRunId, sidebar.startedRunId));
  let activePanel = $derived(pageUrl.searchParams.get("panel") ?? "");

  // Work uses the shared Code shell; this state only controls workspace search.
  let conversationSearch = $state("");
  let workspaceSearchOpen = $state(false);
  let normalizedSearch = $derived(conversationSearch.trim().toLocaleLowerCase());

  function matchesConversation(session: TaskRun): boolean {
    const query = normalizedSearch;
    if (!query) return true;
    return [session.name, session.prompt, session.cwd, session.id]
      .filter((value): value is string => Boolean(value))
      .some((value) => value.toLocaleLowerCase().includes(query));
  }

  let recentConversations = $derived(sidebar.getRecentConversations(matchesConversation));

  $effect(() => {
    // Searching should reveal matching conversations in collapsed workspaces.
    if (!normalizedSearch) return;
    const next = new Set(sidebar.expandedWorkspaces);
    for (const workspace of sidebar.workspaces) {
      if (sidebar.getSortedSessions(workspace.id).some(matchesConversation)) {
        next.add(workspace.id);
      }
    }
    if (next.size !== sidebar.expandedWorkspaces.size) sidebar.expandedWorkspaces = next;
  });

  $effect(() => {
    if (selectedRunId && selectedRunId === sidebar.startedRunId) {
      sidebar.startedRunId = "";
    }
  });

  $effect(() => {
    const workspaceId = selectedId;
    if (!workspaceId) return;
    void sidebar.loadSessions(workspaceId);
  });

  $effect(() => {
    sidebar.syncExpandedLoadTargets();
  });

  onMount(() => {
    sidebar.start();
    return () => sidebar.stop();
  });

  // ── Presentation helpers ───────────────────────────────────────────────────

  function sessionTitle(session: TaskRun): string {
    const value = stripExpertTag(session.name?.trim() || session.prompt?.trim() || "新对话");
    return value.split("\n")[0].slice(0, 60);
  }

  function sessionConversation(session: TaskRun): ConversationGroup {
    const workspace = session.workspace_id
      ? sidebar.workspaces.find((ws) => ws.id === session.workspace_id)
      : undefined;
    return {
      groupKey: `r:${session.id}`,
      runs: [session],
      title: sessionTitle(session),
      latestRun: session,
      isFavorite: false,
      totalMessages: session.message_count ?? 0,
      pinned: session.pinned ?? false,
      archived: session.archived ?? false,
      unread: session.unread ?? false,
      projectName: workspace?.name,
      projectPath: workspace ? workspace.primaryWorkRoot || workspace.root : undefined,
    };
  }

  function sessionAttentionLabel(session: TaskRun): string {
    return getWorkSessionAttentionLabel(inboxStore.items, session);
  }

  function handleWorkspaceRowClick(id: string) {
    const action = getWorkspaceRowAction(selectedId, selectedRunId, false, id);
    if (action.kind === "stay") return;
    sidebar.startedRunId = "";
    void goto(action.href);
  }

  function newWorkspace() {
    void goto("/chat/work?new=1");
  }

  function openPendingPanel() {
    void goto("/chat/work?panel=pending");
  }

  // ── Dialog state (presentation-only) ───────────────────────────────────────
  let deleteConfirmOpen = $state(false);
  let deleteTarget: ConversationGroup | null = $state(null);
  let renameModalOpen = $state(false);
  let renameTarget: WorkWorkspaceSummary | null = $state(null);
  let renameName = $state("");
  let archiveConfirmOpen = $state(false);
  let archiveTarget: WorkWorkspaceSummary | null = $state(null);
  let wsDeleteConfirmOpen = $state(false);
  let wsDeleteTarget: WorkWorkspaceSummary | null = $state(null);
  let workspaceMenuId = $state("");
  let archivedWorkspacesExpanded = $state(false);

  function requestDeleteConversation(conversation: ConversationGroup) {
    if (!conversationCanDelete(conversation) || sidebar.conversationActionRunId) return;
    deleteTarget = conversation;
    deleteConfirmOpen = true;
  }

  async function confirmDeleteConversation() {
    const conversation = deleteTarget;
    deleteConfirmOpen = false;
    deleteTarget = null;
    if (!conversation) return;
    await sidebar.deleteConversation(
      conversation.runs.map((run) => run.id),
      conversation.latestRun.id,
    );
  }

  function endConversation(conversation: ConversationGroup) {
    void sidebar.endConversation(
      conversation.runs
        .filter((run) => !["completed", "failed", "stopped"].includes(run.status))
        .map((run) => run.id),
    );
  }

  function openRenameWorkspace(workspace: WorkWorkspaceSummary) {
    workspaceMenuId = "";
    renameTarget = workspace;
    renameName = workspace.name;
    renameModalOpen = true;
  }

  async function submitRenameWorkspace() {
    const target = renameTarget;
    const name = renameName.trim();
    if (!target || !name || sidebar.workspaceActionBusyId) return;
    await sidebar.renameWorkspace(target.id, name);
    renameModalOpen = false;
    renameTarget = null;
    renameName = "";
  }

  function requestArchiveWorkspace(workspace: WorkWorkspaceSummary) {
    workspaceMenuId = "";
    archiveTarget = workspace;
    archiveConfirmOpen = true;
  }

  async function confirmArchiveWorkspace() {
    const target = archiveTarget;
    archiveConfirmOpen = false;
    archiveTarget = null;
    if (!target) return;
    await sidebar.archiveWorkspace(target);
  }

  function requestDeleteWorkspace(workspace: WorkWorkspaceSummary) {
    workspaceMenuId = "";
    wsDeleteTarget = workspace;
    wsDeleteConfirmOpen = true;
  }

  async function confirmDeleteWorkspace() {
    const target = wsDeleteTarget;
    wsDeleteConfirmOpen = false;
    wsDeleteTarget = null;
    if (!target) return;
    await sidebar.deleteWorkspace(target);
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="work-sidebar-body flex min-h-0 flex-1 flex-col overflow-y-auto px-2 py-2">
    <!-- The recent task list is mode-wide; workspace trees below provide context. -->
    <div class="mb-2 shrink-0">
      <SidebarSectionLabel label="任务" count={recentConversations.length} class="mb-0.5" />
      {#if recentConversations.length > 0}
        <div class="space-y-0.5">
          {#each recentConversations as session (session.id)}
            <ConversationItem
              conversation={sessionConversation(session)}
              selected={session.id === activeRunId}
              statusLabel={sessionAttentionLabel(session)}
              onclick={() =>
                session.workspace_id
                  ? sidebar.selectSession(session.workspace_id, session.id)
                  : sidebar.selectStandaloneSession(session.id)}
              ondelete={requestDeleteConversation}
              onend={endConversation}
            />
          {/each}
        </div>
      {:else}
        <div class="px-2.5 py-2 text-xs text-sidebar-foreground/45">
          {normalizedSearch ? "没有匹配的对话" : "暂无最近对话"}
        </div>
      {/if}
    </div>

    <div class="mb-2 border-t border-sidebar-border/50"></div>

    <!-- Workspaces: same section header/actions as Code. -->
    <div class="shrink-0">
      <SidebarSectionLabel label="工作区" count={sidebar.workspaces.length} class="mb-0.5">
        {#snippet actions()}
          <button
            type="button"
            class="flex h-6 w-6 items-center justify-center rounded-md text-sidebar-foreground/45 transition-colors hover:bg-sidebar-accent/60 hover:text-sidebar-foreground"
            class:bg-sidebar-accent={workspaceSearchOpen}
            onclick={() => (workspaceSearchOpen = !workspaceSearchOpen)}
            title="搜索 Work 对话"
            aria-label="搜索 Work 对话"
          >
            <svg
              class="h-3.5 w-3.5"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <circle cx="11" cy="11" r="7" /><path d="m20 20-4-4" />
            </svg>
          </button>
          <button
            type="button"
            class="flex h-6 w-6 items-center justify-center rounded-md text-sidebar-foreground/45 transition-colors hover:bg-sidebar-accent/60 hover:text-sidebar-foreground"
            onclick={newWorkspace}
            title="新建工作区"
            aria-label="新建工作区"
          >
            <svg
              class="h-3.5 w-3.5"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M12 5v14M5 12h14" />
            </svg>
          </button>
        {/snippet}
      </SidebarSectionLabel>

      {#if workspaceSearchOpen}
        <input
          type="search"
          class="mb-1 min-h-8 w-full rounded-lg border border-transparent bg-sidebar-accent/40 px-2.5 py-1 text-xs text-sidebar-foreground outline-none transition-colors placeholder:text-sidebar-foreground/40 focus:border-sidebar-border focus:bg-background"
          placeholder="搜索 Work 对话"
          bind:value={conversationSearch}
        />
      {/if}

      {#if sidebar.loading}
        <div class="flex items-center gap-2 px-2 py-3 text-xs text-sidebar-foreground/50">
          <span
            class="h-3 w-3 animate-spin rounded-full border-2 border-sidebar-foreground/20 border-t-sidebar-foreground/70"
          ></span>
          正在读取工作区…
        </div>
      {:else if !workTransportSupported}
        <div
          class="mt-1 rounded-lg border border-primary/20 bg-primary/5 px-2.5 py-2 text-xs leading-5 text-sidebar-foreground/70"
        >
          <span class="block font-medium text-sidebar-foreground/85">桌面 App 才能执行 Work</span>
          <span class="mt-0.5 block text-[11px] text-sidebar-foreground/55"
            >本地文件和任务运行只在桌面 App 中可用。</span
          >
        </div>
      {:else if sidebar.error}
        <div
          class="flex items-center justify-between gap-2 rounded-lg border border-red-400/20 bg-red-400/5 px-2.5 py-2 text-xs leading-5 text-red-300"
          role="alert"
        >
          <span class="min-w-0 flex-1">{sidebar.error}</span>
          <button
            type="button"
            class="shrink-0 rounded-md border border-red-400/30 px-2 py-1 text-[11px] font-semibold hover:bg-red-400/10"
            onclick={() => void sidebar.loadWorkspaces(true)}
          >
            重试
          </button>
        </div>
      {:else if sidebar.workspaces.length === 0}
        <button
          type="button"
          class="mt-1 w-full rounded-lg border border-dashed border-sidebar-border/70 px-3 py-3 text-left text-xs leading-5 text-sidebar-foreground/55 transition-colors hover:border-sidebar-foreground/30 hover:bg-sidebar-accent/35"
          onclick={newWorkspace}
        >
          <span class="block font-medium text-sidebar-foreground/75">创建第一个工作区</span>
          <span class="mt-0.5 block text-[11px]">关联本地文件夹，开始协作。</span>
        </button>
      {:else}
        <div class="space-y-0.5">
          {#each sidebar.workspaces as workspace (workspace.id)}
            {@const isExpanded = sidebar.expandedWorkspaces.has(workspace.id)}
            {@const wsSessions = sidebar.getVisibleSessions(workspace.id, matchesConversation)}
            {@const isLoadingSessions = sidebar.sessionsLoadingByWorkspace[workspace.id] ?? false}
            <SidebarFolderRow
              label={workspace.name}
              title={workspace.primaryWorkRoot || workspace.root}
              expanded={isExpanded}
              selected={selectedId === workspace.id}
              hasMenu={true}
              menuOpen={workspaceMenuId === workspace.id}
              onToggle={() => sidebar.toggleWorkspaceExpanded(workspace.id)}
              onClick={() => handleWorkspaceRowClick(workspace.id)}
            >
              {#snippet menu()}
                <div data-workspace-menu>
                  <button
                    type="button"
                    class="absolute right-1 top-1 flex h-6 w-6 items-center justify-center rounded-md text-sidebar-foreground/55 transition-[opacity,background-color,color] hover:bg-sidebar-accent/80 hover:text-sidebar-foreground focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 {selectedId ===
                    workspace.id
                      ? 'opacity-100'
                      : 'opacity-0 group-hover:opacity-100 group-focus-within:opacity-100'}"
                    title="工作区操作"
                    aria-label={`管理工作区 ${workspace.name}`}
                    aria-expanded={workspaceMenuId === workspace.id}
                    onclick={(e) => {
                      e.stopPropagation();
                      workspaceMenuId = workspaceMenuId === workspace.id ? "" : workspace.id;
                    }}
                  >
                    <svg
                      viewBox="0 0 24 24"
                      class="h-3.5 w-3.5"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2.2"
                      stroke-linecap="round"
                    >
                      <path d="M5 12h.01M12 12h.01M19 12h.01" />
                    </svg>
                  </button>
                  {#if workspaceMenuId === workspace.id}
                    <div
                      class="absolute right-1 top-8 z-20 min-w-32 overflow-hidden rounded-lg border border-sidebar-border bg-sidebar shadow-xl"
                      role="menu"
                    >
                      <button
                        type="button"
                        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-sidebar-foreground transition-colors hover:bg-sidebar-accent"
                        role="menuitem"
                        onclick={() => openRenameWorkspace(workspace)}
                      >
                        <svg
                          viewBox="0 0 24 24"
                          class="h-3.5 w-3.5"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1.8"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        >
                          <path d="M12 20h9" /><path
                            d="M16.5 3.5a2.12 2.12 0 0 1 3 3L8 18l-4 1 1-4Z"
                          />
                        </svg>
                        重命名
                      </button>
                      <button
                        type="button"
                        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-sidebar-foreground transition-colors hover:bg-sidebar-accent"
                        role="menuitem"
                        onclick={() => requestArchiveWorkspace(workspace)}
                      >
                        <svg
                          viewBox="0 0 24 24"
                          class="h-3.5 w-3.5"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1.8"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        >
                          <path d="M3 7h18M5 7v13h14V7M9 7V4h6v3" /><path d="M10 11v5M14 11v5" />
                        </svg>
                        归档
                      </button>
                      <button
                        type="button"
                        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-red-400 transition-colors hover:bg-red-400/10"
                        role="menuitem"
                        onclick={() => requestDeleteWorkspace(workspace)}
                      >
                        <svg
                          viewBox="0 0 24 24"
                          class="h-3.5 w-3.5"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1.8"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        >
                          <path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6M10 11v5M14 11v5" />
                        </svg>
                        删除
                      </button>
                    </div>
                  {/if}
                </div>
              {/snippet}

              {#snippet children()}
                <button
                  type="button"
                  class="work-sidebar-conversation-action chat-project-new-chat flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-xs text-sidebar-foreground/60 transition-colors hover:bg-sidebar-accent/50 hover:text-sidebar-foreground"
                  onclick={(e) => {
                    e.stopPropagation();
                    sidebar.newConversationForWorkspace(workspace.id);
                  }}
                >
                  <svg
                    viewBox="0 0 24 24"
                    class="h-3.5 w-3.5 text-sidebar-foreground/45"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                  >
                    <path d="M12 5v14M5 12h14" />
                  </svg>
                  <span>新对话</span>
                </button>

                {#if isLoadingSessions && wsSessions.length === 0}
                  <div
                    class="flex items-center gap-2 px-2.5 py-2 text-xs text-sidebar-foreground/50"
                  >
                    <span
                      class="h-3 w-3 animate-spin rounded-full border-2 border-sidebar-foreground/20 border-t-sidebar-foreground/70"
                    ></span>
                    正在读取对话…
                  </div>
                {:else if wsSessions.length === 0}
                  <div class="px-2.5 py-2 text-xs text-sidebar-foreground/45">暂无对话</div>
                {:else}
                  {#each wsSessions as session (session.id)}
                    <ConversationItem
                      conversation={sessionConversation(session)}
                      selected={activeRunId === session.id}
                      statusLabel={sessionAttentionLabel(session)}
                      onclick={() => sidebar.selectSession(workspace.id, session.id)}
                      ondelete={requestDeleteConversation}
                      onend={endConversation}
                    />
                  {/each}
                {/if}
              {/snippet}
            </SidebarFolderRow>
          {/each}

          <button
            type="button"
            class="work-sidebar-add-workspace mt-1 flex w-full items-center gap-2 rounded-lg border border-dashed border-sidebar-border/60 px-2.5 py-1.5 text-xs text-sidebar-foreground/50 transition-colors hover:border-sidebar-border hover:bg-sidebar-accent/40 hover:text-sidebar-foreground"
            onclick={newWorkspace}
          >
            <svg
              viewBox="0 0 24 24"
              class="h-3.5 w-3.5 text-sidebar-foreground/40"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
            >
              <path d="M12 5v14M5 12h14" />
            </svg>
            <span>新建工作区</span>
          </button>
        </div>
      {/if}

      <!-- Archived workspaces are management-only and stay out of the Code-like primary tree. -->
      {#if !sidebar.loading && sidebar.archivedWorkspaces.length > 0}
        <div class="mt-3 border-t border-sidebar-border/40 pt-2">
          <button
            type="button"
            class="flex w-full items-center justify-between px-2.5 py-1 font-semibold text-sidebar-foreground/40 transition-colors hover:text-sidebar-foreground/70"
            aria-expanded={archivedWorkspacesExpanded}
            onclick={() => (archivedWorkspacesExpanded = !archivedWorkspacesExpanded)}
          >
            <div class="flex items-center gap-1.5">
              <svg
                viewBox="0 0 24 24"
                class="h-3 w-3 transition-transform duration-150 {archivedWorkspacesExpanded
                  ? 'rotate-90'
                  : ''}"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"><path d="m9 18 6-6-6-6" /></svg
              >
              <span>已归档工作区</span>
            </div>
            <span>{sidebar.archivedWorkspaces.length}</span>
          </button>
          {#if archivedWorkspacesExpanded}
            <div class="mt-1 space-y-0.5 opacity-80">
              {#each sidebar.archivedWorkspaces as workspace (workspace.id)}
                <div
                  class="flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-sidebar-foreground/55"
                >
                  <span class="min-w-0 flex-1 truncate text-xs" title={workspace.root}
                    >{workspace.name}</span
                  >
                  <button
                    type="button"
                    class="shrink-0 rounded-md px-1.5 py-0.5 text-[10px] text-primary hover:bg-sidebar-accent"
                    disabled={sidebar.workspaceActionBusyId === workspace.id}
                    onclick={() => void sidebar.restoreArchivedWorkspace(workspace)}
                  >
                    {sidebar.workspaceActionBusyId === workspace.id ? "…" : "恢复"}
                  </button>
                  <button
                    type="button"
                    class="shrink-0 rounded-md px-1.5 py-0.5 text-[10px] text-red-400 transition-colors hover:bg-red-400/10 disabled:opacity-50"
                    disabled={sidebar.workspaceActionBusyId === workspace.id}
                    title="永久删除数据目录"
                    onclick={() => requestDeleteWorkspace(workspace)}
                  >
                    删除
                  </button>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>

    {#if workTransportSupported}
      <footer class="mt-auto shrink-0 border-t border-sidebar-border/40 px-0.5 pt-2">
        {#if inboxStore.pendingCount > 0}
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-xs transition-colors {activePanel ===
            'pending'
              ? 'bg-sidebar-accent text-sidebar-foreground'
              : 'text-sidebar-foreground/60 hover:bg-sidebar-accent/50 hover:text-sidebar-foreground'}"
            onclick={openPendingPanel}
          >
            <span>待处理</span>
            <span
              class="flex h-4 min-w-4 items-center justify-center rounded-full bg-amber-500/25 px-1 text-[10px] font-bold text-amber-600 dark:text-amber-400"
              >{inboxStore.pendingCount}</span
            >
          </button>
        {/if}
      </footer>
    {/if}
  </div>
</div>

<Modal bind:open={deleteConfirmOpen} title={t("sidebar_deleteConfirm")}>
  <p class="mb-4 text-sm text-muted-foreground">{t("sidebar_deleteDesc")}</p>
  <div class="flex justify-end gap-2">
    <button
      type="button"
      class="rounded-md border border-border px-3 py-1.5 text-sm transition-colors hover:bg-accent"
      onclick={() => {
        deleteConfirmOpen = false;
        deleteTarget = null;
      }}
    >
      {t("sidebar_deleteCancel")}
    </button>
    <button
      type="button"
      class="rounded-md bg-destructive px-3 py-1.5 text-sm text-destructive-foreground transition-colors hover:bg-destructive/90"
      onclick={() => void confirmDeleteConversation()}
    >
      {t("sidebar_deleteOk")}
    </button>
  </div>
</Modal>

<Modal bind:open={renameModalOpen} title="重命名工作区">
  <form
    onsubmit={(event) => {
      event.preventDefault();
      void submitRenameWorkspace();
    }}
  >
    <label class="block text-sm font-medium text-foreground" for="work-sidebar-rename"
      >工作区名称</label
    >
    <input
      id="work-sidebar-rename"
      class="mt-2 min-h-11 w-full rounded-xl border border-border bg-background px-3.5 text-sm text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary focus:ring-2 focus:ring-primary/20"
      bind:value={renameName}
      maxlength="80"
    />
    {#if sidebar.error}
      <p class="mt-2 text-xs leading-5 text-destructive" role="alert">{sidebar.error}</p>
    {/if}
    <div class="mt-5 flex justify-end gap-2">
      <button
        type="button"
        class="min-h-10 rounded-lg px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={() => {
          renameModalOpen = false;
          renameTarget = null;
          renameName = "";
        }}
      >
        取消
      </button>
      <button
        type="submit"
        class="min-h-10 rounded-lg bg-primary px-3.5 py-2 text-sm font-semibold text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-60"
        disabled={sidebar.workspaceActionBusyId !== ""}
      >
        {sidebar.workspaceActionBusyId ? "保存中…" : "保存"}
      </button>
    </div>
  </form>
</Modal>

<Modal bind:open={archiveConfirmOpen} title="归档工作区">
  <p class="text-sm leading-6 text-muted-foreground">
    确定要归档“{archiveTarget?.name ??
      "工作区"}”吗？归档后它会从列表中隐藏，但其中的对话、文件和成果仍会保留，不会物理删除。
  </p>
  {#if sidebar.error}
    <p class="mt-2 text-xs leading-5 text-destructive" role="alert">{sidebar.error}</p>
  {/if}
  <div class="mt-5 flex justify-end gap-2">
    <button
      type="button"
      class="min-h-10 rounded-lg px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      onclick={() => {
        archiveConfirmOpen = false;
        archiveTarget = null;
      }}
    >
      取消
    </button>
    <button
      type="button"
      class="min-h-10 rounded-lg bg-destructive px-3.5 py-2 text-sm font-semibold text-destructive-foreground transition-opacity hover:opacity-90 disabled:opacity-90"
      disabled={sidebar.workspaceActionBusyId !== ""}
      onclick={() => void confirmArchiveWorkspace()}
    >
      {sidebar.workspaceActionBusyId ? "归档中…" : "确认归档"}
    </button>
  </div>
</Modal>

<Modal bind:open={wsDeleteConfirmOpen} title="永久删除工作区">
  <div
    class="rounded-lg border border-red-400/30 bg-red-400/5 px-3 py-2.5 text-sm font-semibold leading-6 text-red-600 dark:text-red-400"
  >
    此操作不可恢复
  </div>
  <p class="mt-3 text-sm leading-6 text-muted-foreground">
    确定要永久删除「{wsDeleteTarget?.name ?? "工作区"}」吗？这将删除以下数据：
  </p>
  <ul class="mt-2 space-y-1 text-xs leading-5 text-muted-foreground">
    <li>· 工作区数据目录（{wsDeleteTarget?.root ?? ""}）</li>
    <li>· input/ 中的所有输入材料</li>
    <li>· output/ 中的所有成果</li>
    <li>· 该工作区下的所有对话记录</li>
  </ul>
  <p class="mt-3 text-xs leading-5 text-muted-foreground">
    如果只是暂时不用，建议选择「归档」而非删除——归档可以随时恢复。
  </p>
  {#if sidebar.error}
    <p class="mt-2 text-xs leading-5 text-destructive" role="alert">{sidebar.error}</p>
  {/if}
  <div class="mt-5 flex justify-end gap-2">
    <button
      type="button"
      class="min-h-10 rounded-lg px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      onclick={() => {
        wsDeleteConfirmOpen = false;
        wsDeleteTarget = null;
      }}
    >
      取消
    </button>
    <button
      type="button"
      class="min-h-10 rounded-lg bg-destructive px-3.5 py-2 text-sm font-semibold text-destructive-foreground transition-opacity hover:opacity-90 disabled:opacity-90"
      disabled={sidebar.workspaceActionBusyId !== ""}
      onclick={() => void confirmDeleteWorkspace()}
    >
      {sidebar.workspaceActionBusyId ? "删除中…" : "永久删除"}
    </button>
  </div>
</Modal>
