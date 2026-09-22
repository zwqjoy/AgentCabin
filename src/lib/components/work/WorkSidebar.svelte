<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import { onMount, untrack } from "svelte";
  import { deleteRuns, getRun, setRunFlags, stopRun, stopSession } from "$lib/api";
  import {
    archiveWorkspace,
    deleteWorkspace,
    listWorkSessions,
    renameWorkspace,
    restoreWorkspace,
  } from "$lib/api/work";
  import { inboxStore } from "$lib/stores/inbox-store.svelte";
  import { workTaskStore } from "$lib/stores/work-task-store.svelte";
  import { workWorkspaceStore } from "$lib/stores/work-workspace-store.svelte";
  import Modal from "$lib/components/Modal.svelte";
  import ConversationItem from "$lib/components/ConversationItem.svelte";
  import SidebarNavItem from "$lib/components/SidebarNavItem.svelte";
  import SidebarSectionLabel from "$lib/components/SidebarSectionLabel.svelte";
  import SidebarFolderRow from "$lib/components/SidebarFolderRow.svelte";
  import type { TaskRun } from "$lib/types";
  import type { WorkWorkspaceSummary } from "$lib/types/work";
  import { stripExpertTag } from "$lib/utils/expert-context";
  import { conversationCanDelete, type ConversationGroup } from "$lib/utils/sidebar-groups";
  import {
    applyRunMutation,
    dispatchRunMutation,
    RUNS_CHANGED_EVENT,
    type RunMutation,
  } from "$lib/utils/run-mutations";
  import { deleteSnapshot } from "$lib/utils/snapshot-cache";
  import { dbgWarn } from "$lib/utils/debug";
  import { getTransport } from "$lib/transport";
  import { withTimeout } from "$lib/utils/async-utils";
  import { t } from "$lib/i18n/index.svelte";
  import {
    getWorkActiveRunId,
    getWorkspaceRowAction,
    isWorkHomeSelected,
  } from "$lib/utils/work-sidebar-navigation";
  import {
    getExpandedWorkSessionLoadTargets,
    getWorkSessionRefreshTargets,
  } from "$lib/utils/work-sidebar-refresh";
  import { getWorkSessionAttentionLabel } from "$lib/utils/work-sidebar-attention";

  let workspaces = $derived(workWorkspaceStore.workspaces);
  let archivedWorkspaces = $derived(workWorkspaceStore.archivedWorkspaces);
  let standaloneSessions = $derived(workWorkspaceStore.standaloneSessions);
  let loading = $derived(
    !workWorkspaceStore.workspacesLoaded && workWorkspaceStore.loadingWorkspaces,
  );
  let error = $derived(workWorkspaceStore.error);
  let sessionsByWorkspace = $state<Record<string, TaskRun[]>>({});
  let sessionsLoadingByWorkspace = $state<Record<string, boolean>>({});
  let sessionsRequestByWorkspace: Record<string, number> = {};
  let sessionsInFlight: Record<string, boolean> = {};
  let sessionsInFlightCount: Record<string, number> = {};
  let expandedWorkspaces = $state<Set<string>>(new Set());
  let expandedWorkspacesLoaded = $state(false);
  let startedRunId = $state("");
  let actionRunId = $state("");
  let deleteConfirmOpen = $state(false);
  let deleteTarget: ConversationGroup | null = $state(null);
  let workspaceMenuId = $state("");
  let workspaceActionBusyId = $state("");
  let workspaceActionError = $state("");
  const transport = getTransport();
  const workTransportSupported = transport.isDesktop();
  const WORK_SESSION_LIST_TIMEOUT_MS = 10_000;
  let renameModalOpen = $state(false);
  let renameTarget: WorkWorkspaceSummary | null = $state(null);
  let renameName = $state("");
  let archiveConfirmOpen = $state(false);
  let archiveTarget: WorkWorkspaceSummary | null = $state(null);
  let wsDeleteConfirmOpen = $state(false);
  let wsDeleteTarget: WorkWorkspaceSummary | null = $state(null);
  let archivedWorkspacesExpanded = $state(false);
  let standaloneExpanded = $state(true);
  let conversationSearch = $state("");
  let selectedId = $derived($page.url.searchParams.get("workspace") ?? "");
  let selectedRunId = $derived($page.url.searchParams.get("run") ?? "");
  let selectedView = $derived($page.url.searchParams.get("view") ?? "");
  let newConversation = $derived($page.url.searchParams.get("newSession") === "1");
  let isWorkHome = $derived(
    isWorkHomeSelected(selectedId, selectedRunId, selectedView, newConversation, startedRunId),
  );
  let normalizedConversationSearch = $derived(conversationSearch.trim().toLocaleLowerCase());

  function matchesConversation(session: TaskRun): boolean {
    const query = normalizedConversationSearch;
    if (!query) return true;
    return [session.name, session.prompt, session.cwd, session.id]
      .filter((value): value is string => Boolean(value))
      .some((value) => value.toLocaleLowerCase().includes(query));
  }

  function getWorkspaceSessions(workspaceId: string): TaskRun[] {
    return sessionsByWorkspace[workspaceId] ?? [];
  }

  function getSortedSessions(workspaceId: string): TaskRun[] {
    return [...getWorkspaceSessions(workspaceId)].sort((left, right) => {
      const leftPinned = left.pinned ?? false;
      const rightPinned = right.pinned ?? false;
      if (leftPinned !== rightPinned) return leftPinned ? -1 : 1;
      return (right.last_activity_at ?? right.started_at).localeCompare(
        left.last_activity_at ?? left.started_at,
      );
    });
  }

  function getVisibleSessions(workspaceId: string): TaskRun[] {
    return getSortedSessions(workspaceId).filter(
      (session) => !session.archived && matchesConversation(session),
    );
  }

  function getSortedStandaloneSessions(): TaskRun[] {
    return [...standaloneSessions].sort((left, right) => {
      const leftPinned = left.pinned ?? false;
      const rightPinned = right.pinned ?? false;
      if (leftPinned !== rightPinned) return leftPinned ? -1 : 1;
      return (right.last_activity_at ?? right.started_at).localeCompare(
        left.last_activity_at ?? left.started_at,
      );
    });
  }

  let visibleStandaloneSessions = $derived(
    getSortedStandaloneSessions().filter(
      (session) => !session.archived && matchesConversation(session),
    ),
  );
  let archivedStandaloneSessions = $derived(
    getSortedStandaloneSessions().filter(
      (session) => session.archived && matchesConversation(session),
    ),
  );
  $effect(() => {
    const query = normalizedConversationSearch;
    if (!query) return;
    const next = new Set(expandedWorkspaces);
    for (const workspace of workspaces) {
      if (getSortedSessions(workspace.id).some(matchesConversation)) {
        next.add(workspace.id);
      }
    }
    if (next.size !== expandedWorkspaces.size) expandedWorkspaces = next;
  });

  let activeRunId = $derived(getWorkActiveRunId(selectedRunId, startedRunId));

  $effect(() => {
    if (selectedRunId && selectedRunId === startedRunId) {
      startedRunId = "";
    }
  });

  async function loadWorkspaces(force = false) {
    if (!workTransportSupported) return;
    await workWorkspaceStore.fetchWorkspaces(force);
  }

  async function loadStandaloneSessions(force = false) {
    if (!workTransportSupported) return;
    await workWorkspaceStore.fetchStandaloneSessions(force);
  }

  async function loadSessions(workspaceId: string, force = false) {
    if (!workspaceId) return;
    if (sessionsInFlight[workspaceId] && !force) return;

    const reqId = (sessionsRequestByWorkspace[workspaceId] ?? 0) + 1;
    sessionsRequestByWorkspace[workspaceId] = reqId;
    sessionsInFlight[workspaceId] = true;
    sessionsInFlightCount[workspaceId] = (sessionsInFlightCount[workspaceId] ?? 0) + 1;
    sessionsLoadingByWorkspace = { ...sessionsLoadingByWorkspace, [workspaceId]: true };

    try {
      const nextSessions = await withTimeout(
        listWorkSessions(workspaceId),
        WORK_SESSION_LIST_TIMEOUT_MS,
        "读取 Work 对话列表超时",
      );
      if (sessionsRequestByWorkspace[workspaceId] !== reqId) return;
      sessionsByWorkspace = { ...sessionsByWorkspace, [workspaceId]: nextSessions };
    } catch (cause) {
      if (sessionsRequestByWorkspace[workspaceId] !== reqId) return;
      error = cause instanceof Error ? cause.message : String(cause);
      sessionsByWorkspace = { ...sessionsByWorkspace, [workspaceId]: [] };
    } finally {
      const remaining = Math.max(0, (sessionsInFlightCount[workspaceId] ?? 1) - 1);
      sessionsInFlightCount[workspaceId] = remaining;
      if (remaining === 0 || sessionsRequestByWorkspace[workspaceId] === reqId) {
        sessionsInFlight[workspaceId] = false;
        sessionsLoadingByWorkspace = { ...sessionsLoadingByWorkspace, [workspaceId]: false };
      }
    }
  }

  function toggleWorkspace(id: string) {
    const next = new Set(expandedWorkspaces);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
      void loadSessions(id);
    }
    expandedWorkspaces = next;
  }

  function handleWorkspaceRowClick(id: string) {
    const action = getWorkspaceRowAction(
      selectedId,
      selectedRunId,
      selectedView,
      newConversation,
      id,
    );
    if (action.kind === "stay") return;
    startedRunId = "";
    void goto(action.href);
  }

  function newWorkspace() {
    void goto("/chat/work?new=1");
  }

  function toggleWorkspaceMenu(id: string) {
    workspaceMenuId = workspaceMenuId === id ? "" : id;
  }

  function openRenameWorkspace(workspace: WorkWorkspaceSummary) {
    workspaceMenuId = "";
    renameTarget = workspace;
    renameName = workspace.name;
    workspaceActionError = "";
    renameModalOpen = true;
  }

  function closeRenameWorkspace() {
    renameModalOpen = false;
    renameTarget = null;
    renameName = "";
    workspaceActionError = "";
  }

  async function submitRenameWorkspace() {
    const target = renameTarget;
    const name = renameName.trim();
    if (!target) return;
    if (!name) {
      workspaceActionError = "请输入工作空间名称";
      return;
    }
    if (workspaceActionBusyId) return;

    workspaceActionBusyId = target.id;
    workspaceActionError = "";
    try {
      const updated = await renameWorkspace(target.id, name);
      workWorkspaceStore.updateWorkspace(updated);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
      closeRenameWorkspace();
    } catch (cause) {
      workspaceActionError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      workspaceActionBusyId = "";
    }
  }

  function requestArchiveWorkspace(workspace: WorkWorkspaceSummary) {
    workspaceMenuId = "";
    archiveTarget = workspace;
    workspaceActionError = "";
    archiveConfirmOpen = true;
  }

  function closeArchiveWorkspace() {
    archiveConfirmOpen = false;
    archiveTarget = null;
    workspaceActionError = "";
  }

  async function confirmArchiveWorkspace() {
    const target = archiveTarget;
    if (!target || workspaceActionBusyId) return;

    workspaceActionBusyId = target.id;
    workspaceActionError = "";
    try {
      await archiveWorkspace(target.id);
      workWorkspaceStore.removeWorkspace(target.id);
      archiveConfirmOpen = false;
      archiveTarget = null;

      if (selectedId === target.id) {
        const nextWorkspace = workspaces.find((w) => w.id !== target.id);
        const targetUrl = nextWorkspace
          ? `/chat/work?workspace=${encodeURIComponent(nextWorkspace.id)}`
          : "/chat/work";
        await goto(targetUrl, { replaceState: true });
      }
      void workWorkspaceStore.fetchWorkspaces(true);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
    } catch (cause) {
      workspaceActionError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      workspaceActionBusyId = "";
    }
  }

  async function restoreArchivedWorkspace(workspace: WorkWorkspaceSummary) {
    if (workspaceActionBusyId) return;
    workspaceActionBusyId = workspace.id;
    workspaceActionError = "";
    try {
      const restored = await restoreWorkspace(workspace.id);
      workWorkspaceStore.addWorkspace(restored);
      void workWorkspaceStore.fetchWorkspaces(true);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
    } catch (cause) {
      workspaceActionError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      workspaceActionBusyId = "";
    }
  }

  function requestDeleteWorkspace(workspace: WorkWorkspaceSummary) {
    workspaceMenuId = "";
    wsDeleteTarget = workspace;
    workspaceActionError = "";
    wsDeleteConfirmOpen = true;
  }

  function closeDeleteWorkspace() {
    wsDeleteConfirmOpen = false;
    wsDeleteTarget = null;
    workspaceActionError = "";
  }

  async function confirmDeleteWorkspace() {
    const target = wsDeleteTarget;
    if (!target || workspaceActionBusyId) return;

    workspaceActionBusyId = target.id;
    workspaceActionError = "";
    try {
      await deleteWorkspace(target.id);
      workWorkspaceStore.removeWorkspace(target.id);
      wsDeleteConfirmOpen = false;
      wsDeleteTarget = null;

      if (selectedId === target.id) {
        const nextWorkspace = workspaces.find((w) => w.id !== target.id);
        const targetUrl = nextWorkspace
          ? `/chat/work?workspace=${encodeURIComponent(nextWorkspace.id)}`
          : "/chat/work";
        await goto(targetUrl, { replaceState: true });
      }
      void workWorkspaceStore.fetchWorkspaces(true);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
    } catch (cause) {
      workspaceActionError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      workspaceActionBusyId = "";
    }
  }

  function newConversationForWorkspace(workspaceId?: string) {
    const target = workspaceId || selectedId;
    if (!target) return newWorkspace();
    startedRunId = "";
    if (typeof window !== "undefined") {
      window.dispatchEvent(
        new CustomEvent("agentcabin:work-new-chat", {
          detail: { workspaceId: target },
        }),
      );
    }
    void goto(`/chat/work?workspace=${encodeURIComponent(target)}&newSession=1`);
  }

  function selectStandaloneSession(runId: string) {
    startedRunId = "";
    void goto(`/chat/work?run=${encodeURIComponent(runId)}`);

    const session = standaloneSessions.find((candidate) => candidate.id === runId);
    if (session?.unread) {
      setRunFlags(runId, { unread: false })
        .then(() =>
          dispatchRunMutation({
            kind: "update",
            runId,
            patch: { unread: false },
          }),
        )
        .catch((cause) => dbgWarn("work-sidebar", "clear standalone unread failed", cause));
    }
  }

  function selectSession(workspaceId: string, runId: string) {
    startedRunId = "";
    void goto(
      `/chat/work?workspace=${encodeURIComponent(workspaceId)}&run=${encodeURIComponent(runId)}`,
    );

    const wsSessions = getWorkspaceSessions(workspaceId);
    const session = wsSessions.find((candidate) => candidate.id === runId);
    if (session?.unread) {
      setRunFlags(runId, { unread: false })
        .then(() =>
          dispatchRunMutation({
            kind: "update",
            runId,
            patch: { unread: false },
          }),
        )
        .catch((cause) => dbgWarn("work-sidebar", "clear unread failed", cause));
    }
  }

  function sessionTitle(session: TaskRun): string {
    const value = stripExpertTag(session.name?.trim() || session.prompt?.trim() || "新对话");
    return value.split("\n")[0].slice(0, 60);
  }

  function sessionConversation(
    session: TaskRun,
    workspaceName?: string,
    workspacePath?: string,
  ): ConversationGroup {
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
      projectName: workspaceName,
      projectPath: workspacePath,
    };
  }

  function sessionAttentionLabel(session: TaskRun): string {
    return getWorkSessionAttentionLabel(inboxStore.items, session);
  }

  function notifySessionsChanged(runId?: string, workspaceId?: string) {
    window.dispatchEvent(
      new CustomEvent("agentcabin:work-sessions-changed", {
        detail: { workspaceId: workspaceId || selectedId, runId },
      }),
    );
  }

  function handleRunMutation(event: Event) {
    const mutation = (event as CustomEvent<RunMutation>).detail;
    if (!mutation) return;
    const next: Record<string, TaskRun[]> = {};
    for (const [wsId, wsSessions] of Object.entries(sessionsByWorkspace)) {
      next[wsId] = applyRunMutation(wsSessions, mutation);
    }
    sessionsByWorkspace = next;
    workWorkspaceStore.setStandaloneSessions(applyRunMutation(standaloneSessions, mutation));
    void workWorkspaceStore.fetchArchivedCount();
  }

  function requestDeleteConversation(conversation: ConversationGroup) {
    if (!conversationCanDelete(conversation) || actionRunId) return;
    deleteTarget = conversation;
    deleteConfirmOpen = true;
  }

  function cancelDeleteConversation() {
    deleteConfirmOpen = false;
    deleteTarget = null;
  }

  async function confirmDeleteConversation() {
    const conversation = deleteTarget;
    cancelDeleteConversation();
    if (!conversation || actionRunId) return;

    const ids = conversation.runs.map((run) => run.id);
    actionRunId = conversation.latestRun.id;
    try {
      await deleteRuns(ids);
      try {
        await Promise.all(ids.map((id) => deleteSnapshot(id)));
      } catch (cause) {
        dbgWarn("work-sidebar", "delete snapshot cleanup failed", cause);
      }
      dispatchRunMutation({ kind: "delete", runIds: ids });
      workWorkspaceStore.setStandaloneSessions(
        standaloneSessions.filter((s) => !ids.includes(s.id)),
      );
      notifySessionsChanged();

      if (ids.includes(selectedRunId)) {
        if (selectedId) {
          const wsSessions = getVisibleSessions(selectedId);
          const fallback = wsSessions.find((session) => !ids.includes(session.id));
          const target = fallback
            ? `/chat/work?workspace=${encodeURIComponent(selectedId)}&run=${encodeURIComponent(fallback.id)}`
            : `/chat/work?workspace=${encodeURIComponent(selectedId)}`;
          await goto(target, { replaceState: true });
        } else {
          const fallback = visibleStandaloneSessions.find((session) => !ids.includes(session.id));
          const target = fallback
            ? `/chat/work?run=${encodeURIComponent(fallback.id)}`
            : `/chat/work`;
          await goto(target, { replaceState: true });
        }
      }
    } catch (cause) {
      dbgWarn("work-sidebar", "delete conversation failed", cause);
    } finally {
      actionRunId = "";
    }
  }

  async function endConversation(conversation: ConversationGroup) {
    const activeRuns = conversation.runs.filter(
      (run) => !["completed", "failed", "stopped"].includes(run.status),
    );
    if (activeRuns.length === 0 || actionRunId) return;

    actionRunId = conversation.latestRun.id;
    try {
      for (const run of activeRuns) {
        let needsFallback = false;
        try {
          await stopSession(run.id);
          const fresh = await getRun(run.id);
          needsFallback = !["completed", "failed", "stopped"].includes(fresh.status);
        } catch (cause) {
          needsFallback = true;
          dbgWarn("work-sidebar", "stop session fallback required", {
            runId: run.id,
            error: cause,
          });
        }
        if (needsFallback) await stopRun(run.id);
      }
      notifySessionsChanged(activeRuns[0]?.id);
    } catch (cause) {
      dbgWarn("work-sidebar", "end conversation failed", cause);
    } finally {
      actionRunId = "";
    }
  }

  let _prevSelectedId = "";
  $effect(() => {
    const workspaceId = selectedId;
    if (workspaceId === _prevSelectedId) return;
    _prevSelectedId = workspaceId;
    if (!workspaceId) return;

    untrack(() => {
      void loadSessions(workspaceId);
    });
  });

  // Restore the child conversation lists for workspaces that were already
  // expanded before Work mounted, even when there is no workspace in the URL.
  $effect(() => {
    if (!expandedWorkspacesLoaded) return;
    const targets = getExpandedWorkSessionLoadTargets(
      workspaces.map((workspace) => workspace.id),
      expandedWorkspaces,
      Object.keys(sessionsByWorkspace),
    );
    if (targets.length === 0) return;

    untrack(() => {
      for (const workspaceId of targets) {
        void loadSessions(workspaceId);
      }
    });
  });

  $effect(() => {
    if (!expandedWorkspacesLoaded) return;
    const validIds = new Set(workspaces.map((w) => w.id));
    const current = [...expandedWorkspaces];
    const pruned = workspaces.length > 0 ? current.filter((id) => validIds.has(id)) : current;
    localStorage.setItem("agentcabin:work-expanded-workspaces", JSON.stringify(pruned));
  });

  onMount(() => {
    try {
      const rawExpanded = localStorage.getItem("agentcabin:work-expanded-workspaces");
      if (rawExpanded) {
        const parsed = JSON.parse(rawExpanded);
        if (Array.isArray(parsed) && parsed.every((v: unknown) => typeof v === "string")) {
          expandedWorkspaces = new Set(parsed as string[]);
        }
      }
    } catch {
      /* ignore */
    }
    expandedWorkspacesLoaded = true;

    void workWorkspaceStore.fetchWorkspaces();
    void workWorkspaceStore.fetchStandaloneSessions();
    void workWorkspaceStore.fetchArchivedCount();
    inboxStore.fetch(false);
    workTaskStore.fetchTasks();

    let disposed = false;
    let unlistenStatus: (() => void) | undefined;
    let unlistenBusEvent: (() => void) | undefined;

    function refreshSessionForRun(runId: unknown) {
      if (typeof runId !== "string") return;
      const targets = getWorkSessionRefreshTargets(runId, sessionsByWorkspace, standaloneSessions);
      if (targets.workspaceIds.length === 0 && !targets.refreshStandalone) return;
      void inboxStore.fetch(false);
      for (const workspaceId of targets.workspaceIds) {
        // The selected/just-started run is already represented locally by the
        // authoritative TaskRun returned from start(). Re-reading every run
        // in the workspace on each run_state event only leaves the sidebar
        // showing "正在读取对话…" while the active transcript is healthy.
        if (workspaceId === selectedId && (runId === selectedRunId || runId === startedRunId)) {
          continue;
        }
        void loadSessions(workspaceId);
      }
      if (targets.refreshStandalone) {
        void loadStandaloneSessions();
      }
    }

    transport
      .listen<{ run_id?: unknown }>("agentcabin:status-changed", (payload) => {
        refreshSessionForRun(payload?.run_id);
      })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlistenStatus = fn;
      });

    transport
      .listen<{ type?: unknown; run_id?: unknown }>("bus-event", (payload) => {
        if (payload?.type !== "run_state") return;
        refreshSessionForRun(payload.run_id);
      })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlistenBusEvent = fn;
      });

    const onDocumentPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Element) || !target.closest("[data-workspace-menu]")) {
        workspaceMenuId = "";
      }
    };
    const onDocumentKeydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") workspaceMenuId = "";
    };
    const onChanged = () => {
      void workWorkspaceStore.fetchAll(true);
      inboxStore.fetch(false);
      workTaskStore.fetchTasks();
      if (selectedId) void loadSessions(selectedId);
    };
    const onSessionsChanged = (event: Event) => {
      const detail = (event as CustomEvent<{ workspaceId?: string; runId?: string; run?: TaskRun }>)
        .detail;
      const workspaceId = detail?.workspaceId;
      if (workspaceId) {
        if (detail?.runId) startedRunId = detail.runId;
        if (detail?.run) {
          // The start command already returned the authoritative TaskRun. Adopt
          // it immediately instead of launching another all-run metadata scan
          // while the chat is receiving its first streamed events.
          sessionsRequestByWorkspace[workspaceId] =
            (sessionsRequestByWorkspace[workspaceId] ?? 0) + 1;
          sessionsInFlight[workspaceId] = false;
          sessionsInFlightCount[workspaceId] = 0;
          sessionsLoadingByWorkspace = {
            ...sessionsLoadingByWorkspace,
            [workspaceId]: false,
          };
          const existing = sessionsByWorkspace[workspaceId] ?? [];
          sessionsByWorkspace = {
            ...sessionsByWorkspace,
            [workspaceId]: [detail.run, ...existing.filter((run) => run.id !== detail.run?.id)],
          };
        } else {
          void loadSessions(workspaceId, true);
        }
      } else if (detail?.runId) {
        startedRunId = detail.runId;
        if (detail.run) {
          standaloneSessions = [
            detail.run,
            ...standaloneSessions.filter((run) => run.id !== detail.run?.id),
          ];
        } else {
          void loadStandaloneSessions();
        }
      }
    };
    document.addEventListener("pointerdown", onDocumentPointerDown);
    document.addEventListener("keydown", onDocumentKeydown);
    window.addEventListener(RUNS_CHANGED_EVENT, handleRunMutation);
    window.addEventListener("agentcabin:workspaces-changed", onChanged);
    window.addEventListener("agentcabin:work-sessions-changed", onSessionsChanged);
    return () => {
      disposed = true;
      unlistenStatus?.();
      unlistenBusEvent?.();
      document.removeEventListener("pointerdown", onDocumentPointerDown);
      document.removeEventListener("keydown", onDocumentKeydown);
      window.removeEventListener(RUNS_CHANGED_EVENT, handleRunMutation);
      window.removeEventListener("agentcabin:workspaces-changed", onChanged);
      window.removeEventListener("agentcabin:work-sessions-changed", onSessionsChanged);
    };
  });
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <!-- Top Primary Navigation Group -->
  <div class="work-sidebar-nav space-y-0.5 border-b border-sidebar-border/50 p-2">
    <!-- Conversation home -->
    <SidebarNavItem
      href="/chat/work"
      label="对话"
      active={isWorkHome}
      onclick={() => {
        startedRunId = "";
      }}
    >
      {#snippet icon()}
        <svg
          viewBox="0 0 24 24"
          class="h-3.5 w-3.5 shrink-0"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <rect x="3" y="3" width="7" height="7" /><rect x="14" y="3" width="7" height="7" /><rect
            x="14"
            y="14"
            width="7"
            height="7"
          /><rect x="3" y="14" width="7" height="7" />
        </svg>
      {/snippet}
    </SidebarNavItem>

    <!-- Inbox: the canonical human-action queue -->
    <SidebarNavItem
      href="/chat/work?view=inbox"
      label="Inbox"
      active={selectedView === "inbox"}
      badge={inboxStore.pendingCount}
      badgeColor="amber"
      onclick={() => {
        startedRunId = "";
      }}
    >
      {#snippet icon()}
        <svg
          viewBox="0 0 24 24"
          class="h-3.5 w-3.5 shrink-0 text-amber-500/90"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="12" cy="12" r="10" /><circle cx="12" cy="12" r="3" />
        </svg>
      {/snippet}
    </SidebarNavItem>

    <!-- Tasks: schedules remain an optional task setting -->
    <SidebarNavItem
      href="/chat/work?view=tasks"
      label="任务"
      active={selectedView === "tasks" || selectedView === "automation"}
      dotIndicator={workTaskStore.tasks.some((t) => t.schedule?.enabled)}
      onclick={() => {
        startedRunId = "";
      }}
    >
      {#snippet icon()}
        <svg
          viewBox="0 0 24 24"
          class="h-3.5 w-3.5 shrink-0 text-blue-500/90"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" />
        </svg>
      {/snippet}
    </SidebarNavItem>

    <!-- Materials and artifacts -->
    <SidebarNavItem
      href="/chat/work?view=library"
      label="资料与产物"
      active={selectedView === "library"}
      onclick={() => {
        startedRunId = "";
      }}
    >
      {#snippet icon()}
        <svg
          viewBox="0 0 24 24"
          class="h-3.5 w-3.5 shrink-0 text-purple-500/90"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1-2.5-2.5Z" />
          <path d="M6 6h10" /><path d="M6 10h10" />
        </svg>
      {/snippet}
    </SidebarNavItem>

    <!-- Archived conversations -->
    <SidebarNavItem
      href="/chat/work?view=archived"
      label={t("settings_nav_archived")}
      active={selectedView === "archived"}
      badge={workWorkspaceStore.archivedSessionsCount}
      onclick={() => {
        startedRunId = "";
      }}
    >
      {#snippet icon()}
        <svg
          viewBox="0 0 24 24"
          class="h-3.5 w-3.5 shrink-0 text-muted-foreground"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <rect x="2" y="3" width="20" height="5" rx="1" />
          <path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8" />
          <path d="M10 12h4" />
        </svg>
      {/snippet}
    </SidebarNavItem>
  </div>

  <div class="work-sidebar-body flex-1 overflow-y-auto px-2 py-2">
    <!-- Standalone Tasks Header -->
    {#if !loading && !error && (visibleStandaloneSessions.length > 0 || archivedStandaloneSessions.length > 0)}
      <div class="mb-1">
        <SidebarSectionLabel
          label="任务"
          count={visibleStandaloneSessions.length}
          class="cursor-pointer transition-colors hover:text-sidebar-foreground/70"
        />
        {#if standaloneExpanded}
          <div class="mt-0.5 space-y-0.5">
            {#if visibleStandaloneSessions.length === 0}
              <div class="px-2.5 py-2 text-xs text-sidebar-foreground/45">暂无活跃任务</div>
            {:else}
              {#each visibleStandaloneSessions as session (session.id)}
                <ConversationItem
                  conversation={sessionConversation(session, "独立任务")}
                  selected={session.id === activeRunId && !selectedId}
                  statusLabel={sessionAttentionLabel(session)}
                  onclick={() => selectStandaloneSession(session.id)}
                  ondelete={requestDeleteConversation}
                  onend={endConversation}
                />
              {/each}
            {/if}
          </div>
        {/if}
      </div>
    {/if}

    <!-- Workspaces Header -->
    <SidebarSectionLabel label="工作空间" count={workspaces.length} class="mb-0.5 shrink-0" />

    {#if loading}
      <div class="flex items-center gap-2 px-2 py-3 text-xs text-sidebar-foreground/50">
        <span
          class="h-3 w-3 animate-spin rounded-full border-2 border-sidebar-foreground/20 border-t-sidebar-foreground/70"
        ></span>
        正在读取工作空间…
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
    {:else if error}
      <div
        class="flex items-center justify-between gap-2 rounded-lg border border-red-400/20 bg-red-400/5 px-2.5 py-2 text-xs leading-5 text-red-300"
        role="alert"
      >
        <span class="min-w-0 flex-1">{error}</span>
        <button
          type="button"
          class="shrink-0 rounded-md border border-red-400/30 px-2 py-1 text-[11px] font-semibold hover:bg-red-400/10 disabled:opacity-50"
          disabled={loading}
          onclick={() => void loadWorkspaces()}
        >
          {loading ? "重试中…" : "重试加载"}
        </button>
      </div>
    {:else if workspaces.length === 0}
      <button
        type="button"
        class="mt-1 w-full rounded-lg border border-dashed border-sidebar-border/70 px-3 py-3 text-left text-xs leading-5 text-sidebar-foreground/55 transition-colors hover:border-sidebar-foreground/30 hover:bg-sidebar-accent/35"
        onclick={newWorkspace}
      >
        <span class="block font-medium text-sidebar-foreground/75">创建第一个工作空间</span>
        <span class="mt-0.5 block text-[11px]">设置独立边界，开始协作。</span>
      </button>
    {:else}
      <div class="mt-1 space-y-0.5">
        {#each workspaces as workspace (workspace.id)}
          {@const isExpanded = expandedWorkspaces.has(workspace.id)}
          {@const wsSessions = getVisibleSessions(workspace.id)}
          {@const isLoadingSessions = sessionsLoadingByWorkspace[workspace.id] ?? false}
          {@const wsPendingCount = inboxStore.items.filter(
            (item) => item.workspaceId === workspace.id && item.status === "pending",
          ).length}
          <SidebarFolderRow
            label={workspace.name}
            title={workspace.primaryWorkRoot || workspace.root}
            expanded={isExpanded}
            selected={selectedId === workspace.id}
            hasMenu={true}
            menuOpen={workspaceMenuId === workspace.id}
            onToggle={() => toggleWorkspace(workspace.id)}
            onClick={() => handleWorkspaceRowClick(workspace.id)}
          >
            {#snippet badges()}
              {#if wsPendingCount > 0}
                <span
                  class="shrink-0 rounded-full bg-amber-500/20 px-1.5 py-0.2 text-[10px] font-bold text-amber-500"
                  title={`${wsPendingCount} 个待处理事项`}
                >
                  {wsPendingCount}
                </span>
              {/if}
              {#if workspace.artifactCount > 0}
                <span
                  class="shrink-0 rounded bg-sidebar-foreground/10 px-1 py-0.2 text-[9px] text-sidebar-foreground/50"
                >
                  {workspace.artifactCount}
                </span>
              {/if}
            {/snippet}

            {#snippet menu()}
              <div data-workspace-menu>
                <button
                  type="button"
                  class="absolute right-1 top-1 flex h-6 w-6 items-center justify-center rounded-md text-sidebar-foreground/55 transition-[opacity,background-color,color] hover:bg-sidebar-accent/80 hover:text-sidebar-foreground focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 {selectedId ===
                  workspace.id
                    ? 'opacity-100'
                    : 'opacity-0 group-hover:opacity-100 group-focus-within:opacity-100'}"
                  title="工作空间操作"
                  aria-label={`管理工作空间 ${workspace.name}`}
                  aria-expanded={workspaceMenuId === workspace.id}
                  onclick={(e) => {
                    e.stopPropagation();
                    toggleWorkspaceMenu(workspace.id);
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
                  newConversationForWorkspace(workspace.id);
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
                <span>新建对话</span>
              </button>

              {#if isLoadingSessions && wsSessions.length === 0}
                <div class="flex items-center gap-2 px-2.5 py-2 text-xs text-sidebar-foreground/50">
                  <span
                    class="h-3 w-3 animate-spin rounded-full border-2 border-sidebar-foreground/20 border-t-sidebar-foreground/70"
                  ></span>
                  正在读取对话…
                </div>
              {:else if wsSessions.length === 0}
                <div class="px-2.5 py-2 text-xs text-sidebar-foreground/45">暂无活跃对话</div>
              {:else}
                {#each wsSessions as session (session.id)}
                  <ConversationItem
                    conversation={sessionConversation(
                      session,
                      workspace.name,
                      workspace.primaryWorkRoot || workspace.root,
                    )}
                    selected={activeRunId === session.id}
                    statusLabel={sessionAttentionLabel(session)}
                    onclick={() => selectSession(workspace.id, session.id)}
                    ondelete={requestDeleteConversation}
                    onend={endConversation}
                  />
                {/each}
              {/if}
            {/snippet}
          </SidebarFolderRow>
        {/each}

        <!-- Add Workspace Link (unified border-dashed style) -->
        <button
          type="button"
          class="work-sidebar-add-workspace flex w-full items-center gap-2 rounded-lg border border-dashed border-sidebar-border/60 px-2.5 py-1.5 text-xs text-sidebar-foreground/50 transition-colors hover:border-sidebar-border hover:bg-sidebar-accent/40 hover:text-sidebar-foreground mt-1"
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
          <span>新建工作空间</span>
        </button>
      </div>
    {/if}

    <!-- Archived Workspaces: Collapsible by Default -->
    {#if !loading && !error && archivedWorkspaces.length > 0}
      <div class="mt-3 border-t border-sidebar-border/40 pt-2">
        <button
          type="button"
          class="work-sidebar-section-label flex w-full items-center justify-between px-2.5 py-1 font-semibold text-sidebar-foreground/40 hover:text-sidebar-foreground/70 transition-colors"
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
          <span>{archivedWorkspaces.length}</span>
        </button>
        {#if archivedWorkspacesExpanded}
          <div class="mt-1 space-y-0.5 opacity-80">
            {#each archivedWorkspaces as workspace (workspace.id)}
              <div
                class="flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-sidebar-foreground/55"
              >
                <svg
                  viewBox="0 0 24 24"
                  class="h-3.5 w-3.5 shrink-0"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.8"
                  aria-hidden="true"
                >
                  <path d="M4 7h16v13H4zM8 7V4h8v3M9 11h6" />
                </svg>
                <span class="min-w-0 flex-1 truncate text-xs" title={workspace.root}
                  >{workspace.name}</span
                >
                <button
                  type="button"
                  class="shrink-0 rounded-md px-1.5 py-0.5 text-[10px] text-primary hover:bg-sidebar-accent"
                  disabled={workspaceActionBusyId === workspace.id}
                  onclick={() => void restoreArchivedWorkspace(workspace)}
                >
                  {workspaceActionBusyId === workspace.id ? "…" : "恢复"}
                </button>
                <button
                  type="button"
                  class="shrink-0 rounded-md px-1.5 py-0.5 text-[10px] text-red-400 transition-colors hover:bg-red-400/10 disabled:opacity-50"
                  disabled={workspaceActionBusyId === workspace.id}
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
</div>

<Modal bind:open={deleteConfirmOpen} title={t("sidebar_deleteConfirm")}>
  <p class="mb-4 text-sm text-muted-foreground">{t("sidebar_deleteDesc")}</p>
  <div class="flex justify-end gap-2">
    <button
      type="button"
      class="rounded-md border border-border px-3 py-1.5 text-sm transition-colors hover:bg-accent"
      onclick={cancelDeleteConversation}
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

<Modal bind:open={renameModalOpen} title="重命名工作空间">
  <form
    onsubmit={(event) => {
      event.preventDefault();
      void submitRenameWorkspace();
    }}
  >
    <label class="block text-sm font-medium text-foreground" for="work-sidebar-rename"
      >工作空间名称</label
    >
    <input
      id="work-sidebar-rename"
      class="mt-2 min-h-11 w-full rounded-xl border border-border bg-background px-3.5 text-sm text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary focus:ring-2 focus:ring-primary/20"
      bind:value={renameName}
      maxlength="80"
    />
    {#if workspaceActionError}
      <p class="mt-2 text-xs leading-5 text-destructive" role="alert">{workspaceActionError}</p>
    {/if}
    <div class="mt-5 flex justify-end gap-2">
      <button
        type="button"
        class="min-h-10 rounded-lg px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={closeRenameWorkspace}
      >
        取消
      </button>
      <button
        type="submit"
        class="min-h-10 rounded-lg bg-primary px-3.5 py-2 text-sm font-semibold text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-60"
        disabled={workspaceActionBusyId !== ""}
      >
        {workspaceActionBusyId ? "保存中…" : "保存"}
      </button>
    </div>
  </form>
</Modal>

<Modal bind:open={archiveConfirmOpen} title="归档工作空间">
  <p class="text-sm leading-6 text-muted-foreground">
    确定要归档“{archiveTarget?.name ??
      "工作空间"}”吗？归档后它会从列表中隐藏，但其中的对话、文件和交付物仍会保留，不会物理删除。
  </p>
  {#if workspaceActionError}
    <p class="mt-2 text-xs leading-5 text-destructive" role="alert">{workspaceActionError}</p>
  {/if}
  <div class="mt-5 flex justify-end gap-2">
    <button
      type="button"
      class="min-h-10 rounded-lg px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      onclick={closeArchiveWorkspace}
    >
      取消
    </button>
    <button
      type="button"
      class="min-h-10 rounded-lg bg-destructive px-3.5 py-2 text-sm font-semibold text-destructive-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-60"
      disabled={workspaceActionBusyId !== ""}
      onclick={() => void confirmArchiveWorkspace()}
    >
      {workspaceActionBusyId ? "归档中…" : "确认归档"}
    </button>
  </div>
</Modal>

<Modal bind:open={wsDeleteConfirmOpen} title="永久删除工作空间">
  <div
    class="rounded-lg border border-red-400/30 bg-red-400/5 px-3 py-2.5 text-sm font-semibold leading-6 text-red-600 dark:text-red-400"
  >
    此操作不可恢复
  </div>
  <p class="mt-3 text-sm leading-6 text-muted-foreground">
    确定要永久删除「{wsDeleteTarget?.name ?? "工作空间"}」吗？这将删除以下数据：
  </p>
  <ul class="mt-2 space-y-1 text-xs leading-5 text-muted-foreground">
    <li>· 工作空间数据目录（{wsDeleteTarget?.root ?? ""}）</li>
    <li>· input/ 中的所有输入材料</li>
    <li>· output/ 中的所有交付成果和 Artifact</li>
    <li>· scratch/ 中的草稿文件</li>
    <li>· 该工作空间下的所有任务和对话记录</li>
  </ul>
  <p class="mt-3 text-xs leading-5 text-muted-foreground">
    如果只是暂时不用，建议选择「归档」而非删除——归档可以随时恢复。
  </p>
  {#if workspaceActionError}
    <p class="mt-2 text-xs leading-5 text-destructive" role="alert">{workspaceActionError}</p>
  {/if}
  <div class="mt-5 flex justify-end gap-2">
    <button
      type="button"
      class="min-h-10 rounded-lg px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      onclick={closeDeleteWorkspace}
    >
      取消
    </button>
    <button
      type="button"
      class="min-h-10 rounded-lg bg-destructive px-3.5 py-2 text-sm font-semibold text-destructive-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-60"
      disabled={workspaceActionBusyId !== ""}
      onclick={() => void confirmDeleteWorkspace()}
    >
      {workspaceActionBusyId ? "删除中…" : "永久删除"}
    </button>
  </div>
</Modal>
