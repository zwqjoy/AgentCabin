import { goto } from "$app/navigation";
import { page } from "$app/stores";
import { get } from "svelte/store";
import { getRun, setRunFlags, stopRun, stopSession, deleteRuns } from "$lib/api";
import {
  archiveWorkspace,
  deleteWorkspace,
  listArchivedWorkSessions,
  listRecentWorkSessions,
  listWorkSessions,
  renameWorkspace,
  restoreWorkspace,
} from "$lib/api/work";
import { inboxStore } from "$lib/stores/inbox-store.svelte";
import { workTaskStore } from "$lib/stores/work-task-store.svelte";
import { workWorkspaceStore } from "$lib/stores/work-workspace-store.svelte";
import { getTransport } from "$lib/transport";
import { dbgWarn } from "$lib/utils/debug";
import { withTimeout } from "$lib/utils/async-utils";
import {
  applyRunMutation,
  dispatchRunMutation,
  RUNS_CHANGED_EVENT,
  type RunMutation,
} from "$lib/utils/run-mutations";
import { deleteSnapshot } from "$lib/utils/snapshot-cache";
import {
  applyWorkArchiveProjectionMutation,
  filterArchivedWorkSessions,
  filterRecentWorkSessions,
} from "$lib/utils/work-archive-projection";
import type { TaskRun } from "$lib/types";
import type { WorkWorkspaceSummary } from "$lib/types/work";
import {
  getExpandedWorkSessionLoadTargets,
  getWorkSidebarRunStatus,
  getWorkSessionRefreshTargets,
} from "$lib/utils/work-sidebar-refresh";

const WORK_SESSION_LIST_TIMEOUT_MS = 10_000;
const RECENT_CONVERSATIONS_LIMIT = 20;
const ARCHIVED_CONVERSATIONS_LIMIT = 50;

/**
 * Owns every piece of Work sidebar lifecycle that is not presentation:
 * session loading with request de-duplication, workspace mutations,
 * conversation stop/delete, event-driven refresh, and expansion persistence.
 *
 * The WorkSidebar component reads derived state from here and forwards user
 * intent; it holds no backend calls of its own.
 */
export class WorkSidebarStore {
  sessionsByWorkspace = $state<Record<string, TaskRun[]>>({});
  sessionsLoadingByWorkspace = $state<Record<string, boolean>>({});
  expandedWorkspaces = $state<Set<string>>(new Set());
  /** Run id started in this app session, before the URL catches up. */
  startedRunId = $state("");

  workspaceActionBusyId = $state("");
  conversationActionRunId = $state("");
  error = $state("");

  private sessionsRequestByWorkspace: Record<string, number> = {};
  private sessionsInFlight: Record<string, boolean> = {};
  private sessionsInFlightCount: Record<string, number> = {};
  private expandedWorkspacesLoaded = false;
  private disposed = false;
  private unlisteners: Array<() => void> = [];

  workspaces = $derived(workWorkspaceStore.workspaces);
  archivedWorkspaces = $derived(workWorkspaceStore.archivedWorkspaces);
  standaloneSessions = $derived(workWorkspaceStore.standaloneSessions);
  recentSessions = $state<TaskRun[]>([]);
  recentLoading = $state(false);
  archivedSessions = $state<TaskRun[]>([]);
  archivedLoading = $state(false);
  loading = $derived(!workWorkspaceStore.workspacesLoaded && workWorkspaceStore.loadingWorkspaces);
  /** Tasks currently running or blocked on the user — drives the sidebar "任务" count. */
  activeTaskCount = $derived(
    workTaskStore.tasks.filter(
      (task) => task.status === "in_run" || task.status === "needs_attention",
    ).length,
  );

  // ── Derived lists ──────────────────────────────────────────────────────────

  getWorkspaceSessions(workspaceId: string): TaskRun[] {
    return this.sessionsByWorkspace[workspaceId] ?? [];
  }

  getSortedSessions(workspaceId: string): TaskRun[] {
    return [...this.getWorkspaceSessions(workspaceId)].sort((left, right) => {
      const leftPinned = left.pinned ?? false;
      const rightPinned = right.pinned ?? false;
      if (leftPinned !== rightPinned) return leftPinned ? -1 : 1;
      return (right.last_activity_at ?? right.started_at).localeCompare(
        left.last_activity_at ?? left.started_at,
      );
    });
  }

  getVisibleSessions(workspaceId: string, matches: (session: TaskRun) => boolean): TaskRun[] {
    return this.getSortedSessions(workspaceId).filter(
      (session) => !session.archived && matches(session),
    );
  }

  getArchivedSessions(workspaceId: string, matches: (session: TaskRun) => boolean): TaskRun[] {
    return this.getSortedSessions(workspaceId).filter(
      (session) => session.archived && matches(session),
    );
  }

  /** Newest Work conversations across standalone tasks and workspaces. */
  getRecentConversations(matches: (session: TaskRun) => boolean): TaskRun[] {
    return filterRecentWorkSessions(this.recentSessions, matches).slice(
      0,
      RECENT_CONVERSATIONS_LIMIT,
    );
  }

  getArchivedConversations(matches: (session: TaskRun) => boolean): TaskRun[] {
    return filterArchivedWorkSessions(this.archivedSessions, matches).slice(
      0,
      ARCHIVED_CONVERSATIONS_LIMIT,
    );
  }

  findSession(runId: string): { session: TaskRun; workspaceId: string | null } | null {
    const standalone = this.standaloneSessions.find((session) => session.id === runId);
    if (standalone) return { session: standalone, workspaceId: null };
    for (const [workspaceId, sessions] of Object.entries(this.sessionsByWorkspace)) {
      const session = sessions.find((candidate) => candidate.id === runId);
      if (session) return { session, workspaceId };
    }
    return null;
  }

  // ── Loading ────────────────────────────────────────────────────────────────

  async loadWorkspaces(force = false): Promise<void> {
    if (!getTransport().isDesktop()) return;
    await workWorkspaceStore.fetchWorkspaces(force);
  }

  async loadStandaloneSessions(force = false): Promise<void> {
    if (!getTransport().isDesktop()) return;
    await workWorkspaceStore.fetchStandaloneSessions(force);
  }

  async loadRecentSessions(): Promise<void> {
    if (!getTransport().isDesktop() || this.recentLoading) return;
    this.recentLoading = true;
    try {
      const sessions = await withTimeout(
        listRecentWorkSessions(RECENT_CONVERSATIONS_LIMIT),
        WORK_SESSION_LIST_TIMEOUT_MS,
        "读取最近 Work 对话超时",
      );
      this.recentSessions = filterRecentWorkSessions(sessions, () => true);
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      this.recentLoading = false;
    }
  }

  async loadArchivedSessions(): Promise<void> {
    if (!getTransport().isDesktop() || this.archivedLoading) return;
    this.archivedLoading = true;
    try {
      const sessions = await withTimeout(
        listArchivedWorkSessions(ARCHIVED_CONVERSATIONS_LIMIT),
        WORK_SESSION_LIST_TIMEOUT_MS,
        "读取已归档 Work 对话超时",
      );
      this.archivedSessions = filterArchivedWorkSessions(sessions, () => true);
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      this.archivedLoading = false;
    }
  }

  async loadSessions(workspaceId: string, force = false): Promise<void> {
    if (!workspaceId) return;
    if (this.sessionsInFlight[workspaceId] && !force) return;

    const reqId = (this.sessionsRequestByWorkspace[workspaceId] ?? 0) + 1;
    this.sessionsRequestByWorkspace[workspaceId] = reqId;
    this.sessionsInFlight[workspaceId] = true;
    this.sessionsInFlightCount[workspaceId] = (this.sessionsInFlightCount[workspaceId] ?? 0) + 1;
    this.sessionsLoadingByWorkspace = {
      ...this.sessionsLoadingByWorkspace,
      [workspaceId]: true,
    };

    try {
      const nextSessions = await withTimeout(
        listWorkSessions(workspaceId),
        WORK_SESSION_LIST_TIMEOUT_MS,
        "读取 Work 对话列表超时",
      );
      if (this.sessionsRequestByWorkspace[workspaceId] !== reqId) return;
      this.sessionsByWorkspace = {
        ...this.sessionsByWorkspace,
        [workspaceId]: nextSessions,
      };
      this.error = "";
    } catch (cause) {
      if (this.sessionsRequestByWorkspace[workspaceId] !== reqId) return;
      this.error = cause instanceof Error ? cause.message : String(cause);
      this.sessionsByWorkspace = { ...this.sessionsByWorkspace, [workspaceId]: [] };
    } finally {
      const remaining = Math.max(0, (this.sessionsInFlightCount[workspaceId] ?? 1) - 1);
      this.sessionsInFlightCount[workspaceId] = remaining;
      if (remaining === 0 || this.sessionsRequestByWorkspace[workspaceId] === reqId) {
        this.sessionsInFlight[workspaceId] = false;
        this.sessionsLoadingByWorkspace = {
          ...this.sessionsLoadingByWorkspace,
          [workspaceId]: false,
        };
      }
    }
  }

  toggleWorkspaceExpanded(id: string): void {
    const next = new Set(this.expandedWorkspaces);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
      void this.loadSessions(id);
    }
    this.expandedWorkspaces = next;
  }

  /** Restore child conversation lists for workspaces expanded before mount. */
  syncExpandedLoadTargets(): void {
    if (!this.expandedWorkspacesLoaded) return;
    const targets = getExpandedWorkSessionLoadTargets(
      this.workspaces.map((workspace) => workspace.id),
      this.expandedWorkspaces,
      Object.keys(this.sessionsByWorkspace),
    );
    for (const workspaceId of targets) {
      void this.loadSessions(workspaceId);
    }
  }

  // ── Conversation actions ───────────────────────────────────────────────────

  markStartedRun(runId: string): void {
    this.startedRunId = runId;
  }

  selectStandaloneSession(runId: string): void {
    this.startedRunId = "";
    void goto(`/chat/work?run=${encodeURIComponent(runId)}`);
    const session = this.standaloneSessions.find((candidate) => candidate.id === runId);
    this.clearUnread(session);
  }

  selectSession(workspaceId: string, runId: string): void {
    this.startedRunId = "";
    void goto(
      `/chat/work?workspace=${encodeURIComponent(workspaceId)}&run=${encodeURIComponent(runId)}`,
    );
    const session = this.getWorkspaceSessions(workspaceId).find(
      (candidate) => candidate.id === runId,
    );
    this.clearUnread(session);
  }

  private clearUnread(session?: TaskRun): void {
    if (!session?.unread) return;
    setRunFlags(session.id, { unread: false })
      .then(() =>
        dispatchRunMutation({
          kind: "update",
          runId: session.id,
          patch: { unread: false },
        }),
      )
      .catch((cause) => dbgWarn("work-sidebar", "clear unread failed", cause));
  }

  newConversationForWorkspace(workspaceId?: string): void {
    const target = workspaceId || "";
    this.startedRunId = "";
    if (typeof window !== "undefined") {
      window.dispatchEvent(
        new CustomEvent("agentcabin:work-new-chat", {
          detail: { workspaceId: target },
        }),
      );
    }
    void goto(
      target
        ? `/chat/work?workspace=${encodeURIComponent(target)}&newSession=1`
        : "/chat/work?newSession=1",
    );
  }

  async deleteConversation(conversationIds: string[], latestRunId: string): Promise<void> {
    if (this.conversationActionRunId) return;
    this.conversationActionRunId = latestRunId;
    const page_ = get(page);
    const selectedRunId = page_.url.searchParams.get("run") ?? "";
    const selectedId = page_.url.searchParams.get("workspace") ?? "";
    try {
      await deleteRuns(conversationIds);
      try {
        await Promise.all(conversationIds.map((id) => deleteSnapshot(id)));
      } catch (cause) {
        dbgWarn("work-sidebar", "delete snapshot cleanup failed", cause);
      }
      dispatchRunMutation({ kind: "delete", runIds: conversationIds });
      workWorkspaceStore.setStandaloneSessions(
        this.standaloneSessions.filter((s) => !conversationIds.includes(s.id)),
      );
      this.notifySessionsChanged();

      if (conversationIds.includes(selectedRunId)) {
        if (selectedId) {
          const remaining = this.getVisibleSessions(selectedId, () => true).filter(
            (session) => !conversationIds.includes(session.id),
          );
          const target = remaining[0]
            ? `/chat/work?workspace=${encodeURIComponent(selectedId)}&run=${encodeURIComponent(remaining[0].id)}`
            : `/chat/work?workspace=${encodeURIComponent(selectedId)}`;
          await goto(target, { replaceState: true });
        } else {
          const remaining = this.standaloneSessions.filter(
            (session) => !session.archived && !conversationIds.includes(session.id),
          );
          const target = remaining[0]
            ? `/chat/work?run=${encodeURIComponent(remaining[0].id)}`
            : "/chat/work";
          await goto(target, { replaceState: true });
        }
      }
    } catch (cause) {
      dbgWarn("work-sidebar", "delete conversation failed", cause);
    } finally {
      this.conversationActionRunId = "";
    }
  }

  async endConversation(activeRunIds: string[]): Promise<void> {
    if (activeRunIds.length === 0 || this.conversationActionRunId) return;
    this.conversationActionRunId = activeRunIds[0];
    try {
      for (const runId of activeRunIds) {
        let needsFallback = false;
        try {
          await stopSession(runId);
          const fresh = await getRun(runId);
          needsFallback = !["completed", "failed", "stopped"].includes(fresh.status);
        } catch (cause) {
          needsFallback = true;
          dbgWarn("work-sidebar", "stop session fallback required", { runId, error: cause });
        }
        if (needsFallback) await stopRun(runId);
      }
      this.notifySessionsChanged(activeRunIds[0]);
    } catch (cause) {
      dbgWarn("work-sidebar", "end conversation failed", cause);
    } finally {
      this.conversationActionRunId = "";
    }
  }

  // ── Workspace actions ──────────────────────────────────────────────────────

  async renameWorkspace(id: string, name: string): Promise<void> {
    if (this.workspaceActionBusyId) return;
    this.workspaceActionBusyId = id;
    this.error = "";
    try {
      const updated = await renameWorkspace(id, name);
      workWorkspaceStore.updateWorkspace(updated);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      this.workspaceActionBusyId = "";
    }
  }

  async archiveWorkspace(workspace: WorkWorkspaceSummary): Promise<void> {
    if (this.workspaceActionBusyId) return;
    this.workspaceActionBusyId = workspace.id;
    this.error = "";
    const page_ = get(page);
    const selectedId = page_.url.searchParams.get("workspace") ?? "";
    try {
      await archiveWorkspace(workspace.id);
      workWorkspaceStore.removeWorkspace(workspace.id);
      if (selectedId === workspace.id) {
        await goto("/chat/work", { replaceState: true });
      }
      void workWorkspaceStore.fetchWorkspaces(true);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      this.workspaceActionBusyId = "";
    }
  }

  async restoreArchivedWorkspace(workspace: WorkWorkspaceSummary): Promise<void> {
    if (this.workspaceActionBusyId) return;
    this.workspaceActionBusyId = workspace.id;
    this.error = "";
    try {
      const restored = await restoreWorkspace(workspace.id);
      workWorkspaceStore.addWorkspace(restored);
      void workWorkspaceStore.fetchWorkspaces(true);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      this.workspaceActionBusyId = "";
    }
  }

  async deleteWorkspace(workspace: WorkWorkspaceSummary): Promise<void> {
    if (this.workspaceActionBusyId) return;
    this.workspaceActionBusyId = workspace.id;
    this.error = "";
    const page_ = get(page);
    const selectedId = page_.url.searchParams.get("workspace") ?? "";
    try {
      await deleteWorkspace(workspace.id);
      workWorkspaceStore.removeWorkspace(workspace.id);
      if (selectedId === workspace.id) {
        await goto("/chat/work", { replaceState: true });
      }
      void workWorkspaceStore.fetchWorkspaces(true);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      this.workspaceActionBusyId = "";
    }
  }

  // ── Event wiring ───────────────────────────────────────────────────────────

  notifySessionsChanged(runOrId?: TaskRun | string): void {
    const run = typeof runOrId === "string" ? undefined : runOrId;
    const runId = typeof runOrId === "string" ? runOrId : runOrId?.id;
    window.dispatchEvent(
      new CustomEvent("agentcabin:work-sessions-changed", {
        detail: { workspaceId: run?.workspace_id ?? "", runId, run },
      }),
    );
  }

  start(): void {
    this.disposed = false;
    const transport = getTransport();

    void workWorkspaceStore.fetchWorkspaces();
    void workWorkspaceStore.fetchStandaloneSessions();
    void this.loadRecentSessions();
    void this.loadArchivedSessions();
    inboxStore.fetch(false);
    workTaskStore.fetchTasks();

    function refreshSessionForRun(this: WorkSidebarStore, runId: unknown) {
      if (typeof runId !== "string") return;
      const targets = getWorkSessionRefreshTargets(
        runId,
        this.sessionsByWorkspace,
        this.standaloneSessions,
      );
      if (targets.workspaceIds.length === 0 && !targets.refreshStandalone) return;
      void inboxStore.fetch(false);
      const page_ = get(page);
      const selectedRunId = page_.url.searchParams.get("run") ?? "";
      const selectedId = page_.url.searchParams.get("workspace") ?? "";
      for (const workspaceId of targets.workspaceIds) {
        // The selected/just-started run is already represented locally by the
        // authoritative TaskRun returned from start(). Re-reading every run
        // in the workspace on each run_state event only leaves the sidebar
        // showing "正在读取对话…" while the active transcript is healthy.
        if (
          workspaceId === selectedId &&
          (runId === selectedRunId || runId === this.startedRunId)
        ) {
          continue;
        }
        void this.loadSessions(workspaceId);
      }
      if (targets.refreshStandalone) {
        void this.loadStandaloneSessions();
      }
    }

    const syncRunStatus = (runId: unknown, state: unknown) => {
      if (typeof runId !== "string") return;
      const status = getWorkSidebarRunStatus(state);
      if (!status) return;
      dispatchRunMutation({ kind: "update", runId, patch: { status } });
    };

    transport
      .listen<{ run_id?: unknown; status?: unknown }>("agentcabin:status-changed", (payload) => {
        syncRunStatus(payload?.run_id, payload?.status);
        refreshSessionForRun.call(this, payload?.run_id);
      })
      .then((fn) => {
        if (this.disposed) fn();
        else this.unlisteners.push(fn);
      });

    transport
      .listen<{ type?: unknown; run_id?: unknown; state?: unknown }>("bus-event", (payload) => {
        if (payload?.type !== "run_state") return;
        syncRunStatus(payload.run_id, payload.state);
        refreshSessionForRun.call(this, payload.run_id);
      })
      .then((fn) => {
        if (this.disposed) fn();
        else this.unlisteners.push(fn);
      });

    const handleRunMutation = (event: Event) => {
      const mutation = (event as CustomEvent<RunMutation>).detail;
      if (!mutation) return;
      const next: Record<string, TaskRun[]> = {};
      for (const [wsId, wsSessions] of Object.entries(this.sessionsByWorkspace)) {
        next[wsId] = applyRunMutation(wsSessions, mutation);
      }
      this.sessionsByWorkspace = next;
      workWorkspaceStore.setStandaloneSessions(applyRunMutation(this.standaloneSessions, mutation));
      const projection = applyWorkArchiveProjectionMutation(
        { recentSessions: this.recentSessions, archivedSessions: this.archivedSessions },
        mutation,
        mutation.kind === "update" ? this.findSession(mutation.runId)?.session : undefined,
      );
      this.recentSessions = projection.recentSessions;
      this.archivedSessions = projection.archivedSessions;
      if (mutation.kind === "update" && mutation.patch.archived !== undefined) {
        void this.loadArchivedSessions();
        if (mutation.patch.archived === false) void this.loadRecentSessions();
      }
    };

    const onSessionsChanged = (event: Event) => {
      const detail = (event as CustomEvent<{ workspaceId?: string; runId?: string; run?: TaskRun }>)
        .detail;
      const workspaceId = detail?.workspaceId;
      if (workspaceId) {
        if (detail?.runId) this.startedRunId = detail.runId;
        if (detail?.run) {
          // The start command already returned the authoritative TaskRun. Adopt
          // it immediately instead of launching another all-run metadata scan
          // while the chat is receiving its first streamed events.
          this.sessionsRequestByWorkspace[workspaceId] =
            (this.sessionsRequestByWorkspace[workspaceId] ?? 0) + 1;
          this.sessionsInFlight[workspaceId] = false;
          this.sessionsInFlightCount[workspaceId] = 0;
          this.sessionsLoadingByWorkspace = {
            ...this.sessionsLoadingByWorkspace,
            [workspaceId]: false,
          };
          const existing = this.sessionsByWorkspace[workspaceId] ?? [];
          this.sessionsByWorkspace = {
            ...this.sessionsByWorkspace,
            [workspaceId]: [detail.run, ...existing.filter((run) => run.id !== detail.run?.id)],
          };
          const projection = applyWorkArchiveProjectionMutation(
            { recentSessions: this.recentSessions, archivedSessions: this.archivedSessions },
            { kind: "create", run: detail.run },
          );
          this.recentSessions = projection.recentSessions;
          this.archivedSessions = projection.archivedSessions;
        } else {
          void this.loadSessions(workspaceId, true);
        }
      } else if (detail?.runId) {
        this.startedRunId = detail.runId;
        if (detail.run) {
          workWorkspaceStore.setStandaloneSessions([
            detail.run,
            ...this.standaloneSessions.filter((run) => run.id !== detail.run?.id),
          ]);
          const projection = applyWorkArchiveProjectionMutation(
            { recentSessions: this.recentSessions, archivedSessions: this.archivedSessions },
            { kind: "create", run: detail.run },
          );
          this.recentSessions = projection.recentSessions;
          this.archivedSessions = projection.archivedSessions;
        } else {
          void this.loadStandaloneSessions();
        }
      }
    };

    const onChanged = () => {
      void workWorkspaceStore.fetchAll(true);
      inboxStore.fetch(false);
      workTaskStore.fetchTasks();
      const page_ = get(page);
      const selectedId = page_.url.searchParams.get("workspace") ?? "";
      if (selectedId) void this.loadSessions(selectedId);
      void this.loadRecentSessions();
      void this.loadArchivedSessions();
    };

    window.addEventListener(RUNS_CHANGED_EVENT, handleRunMutation);
    window.addEventListener("agentcabin:workspaces-changed", onChanged);
    window.addEventListener("agentcabin:work-sessions-changed", onSessionsChanged);

    this.unlisteners.push(() => {
      window.removeEventListener(RUNS_CHANGED_EVENT, handleRunMutation);
      window.removeEventListener("agentcabin:workspaces-changed", onChanged);
      window.removeEventListener("agentcabin:work-sessions-changed", onSessionsChanged);
    });

    // Restore persisted expansion state.
    try {
      const rawExpanded = localStorage.getItem("agentcabin:work-expanded-workspaces");
      if (rawExpanded) {
        const parsed = JSON.parse(rawExpanded);
        if (Array.isArray(parsed) && parsed.every((v: unknown) => typeof v === "string")) {
          this.expandedWorkspaces = new Set(parsed as string[]);
        }
      }
    } catch {
      /* ignore */
    }
    this.expandedWorkspacesLoaded = true;
  }

  stop(): void {
    this.disposed = true;
    for (const unlisten of this.unlisteners) unlisten();
    this.unlisteners = [];
    try {
      const pruned = [...this.expandedWorkspaces].filter((id) =>
        this.workspaces.some((workspace) => workspace.id === id),
      );
      localStorage.setItem("agentcabin:work-expanded-workspaces", JSON.stringify(pruned));
    } catch {
      /* ignore */
    }
  }
}
