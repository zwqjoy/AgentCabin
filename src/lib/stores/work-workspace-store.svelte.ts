import { listArchivedWorkspaces, listStandaloneWorkSessions, listWorkspaces } from "$lib/api/work";
import { getTransport } from "$lib/transport";
import { withTimeout } from "$lib/utils/async-utils";
import type { TaskRun } from "$lib/types";
import type { WorkWorkspaceSummary } from "$lib/types/work";

const WORKSPACE_LOAD_TIMEOUT_MS = 15_000;
const STANDALONE_LOAD_TIMEOUT_MS = 15_000;

export class WorkWorkspaceStore {
  workspaces = $state<WorkWorkspaceSummary[]>([]);
  archivedWorkspaces = $state<WorkWorkspaceSummary[]>([]);
  standaloneSessions = $state<TaskRun[]>([]);

  workspacesLoaded = $state(false);
  standaloneLoaded = $state(false);

  loadingWorkspaces = $state(false);
  loadingStandalone = $state(false);
  error = $state("");

  private inFlightWorkspaces: Promise<[WorkWorkspaceSummary[], WorkWorkspaceSummary[]]> | null =
    null;
  private inFlightStandalone: Promise<TaskRun[]> | null = null;

  private isSupported(): boolean {
    return getTransport().isDesktop();
  }

  getWorkspaceById(id: string): WorkWorkspaceSummary | null {
    return (
      this.workspaces.find((w) => w.id === id) ??
      this.archivedWorkspaces.find((w) => w.id === id) ??
      null
    );
  }

  setWorkspaces(workspaces: WorkWorkspaceSummary[]): void {
    this.workspaces = workspaces;
  }

  setArchivedWorkspaces(archived: WorkWorkspaceSummary[]): void {
    this.archivedWorkspaces = archived;
  }

  setStandaloneSessions(sessions: TaskRun[]): void {
    this.standaloneSessions = sessions;
  }

  updateWorkspace(updated: WorkWorkspaceSummary): void {
    this.workspaces = this.workspaces.map((w) => (w.id === updated.id ? updated : w));
    this.archivedWorkspaces = this.archivedWorkspaces.map((w) =>
      w.id === updated.id ? updated : w,
    );
  }

  removeWorkspace(id: string): void {
    this.workspaces = this.workspaces.filter((w) => w.id !== id);
    this.archivedWorkspaces = this.archivedWorkspaces.filter((w) => w.id !== id);
  }

  addWorkspace(workspace: WorkWorkspaceSummary): void {
    if (workspace.archived) {
      this.archivedWorkspaces = [
        workspace,
        ...this.archivedWorkspaces.filter((w) => w.id !== workspace.id),
      ];
    } else {
      this.workspaces = [workspace, ...this.workspaces.filter((w) => w.id !== workspace.id)];
    }
  }

  async fetchWorkspaces(force = false): Promise<WorkWorkspaceSummary[]> {
    if (!this.isSupported()) {
      this.workspaces = [];
      this.archivedWorkspaces = [];
      this.workspacesLoaded = true;
      return [];
    }

    if (this.inFlightWorkspaces && !force) {
      const [active] = await this.inFlightWorkspaces;
      return active;
    }

    this.loadingWorkspaces = true;
    this.error = "";

    const request = (async (): Promise<[WorkWorkspaceSummary[], WorkWorkspaceSummary[]]> => {
      try {
        const [active, archived] = await Promise.all([
          withTimeout(listWorkspaces(), WORKSPACE_LOAD_TIMEOUT_MS, "读取工作空间列表超时"),
          withTimeout(listArchivedWorkspaces(), WORKSPACE_LOAD_TIMEOUT_MS, "读取归档工作空间超时"),
        ]);
        return [active, archived];
      } finally {
        this.inFlightWorkspaces = null;
      }
    })();

    this.inFlightWorkspaces = request;

    try {
      const [active, archived] = await request;
      this.workspaces = active;
      this.archivedWorkspaces = archived;
      this.workspacesLoaded = true;
      return active;
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      return this.workspaces;
    } finally {
      this.loadingWorkspaces = false;
    }
  }

  async fetchStandaloneSessions(force = false): Promise<TaskRun[]> {
    if (!this.isSupported()) {
      this.standaloneSessions = [];
      this.standaloneLoaded = true;
      return [];
    }

    if (this.inFlightStandalone && !force) {
      return await this.inFlightStandalone;
    }

    this.loadingStandalone = true;

    const request = (async (): Promise<TaskRun[]> => {
      try {
        return await withTimeout(
          listStandaloneWorkSessions(),
          STANDALONE_LOAD_TIMEOUT_MS,
          "读取独立任务超时",
        );
      } finally {
        this.inFlightStandalone = null;
      }
    })();

    this.inFlightStandalone = request;

    try {
      const sessions = await request;
      this.standaloneSessions = sessions;
      this.standaloneLoaded = true;
      return sessions;
    } catch (cause) {
      if (!this.error) {
        this.error = cause instanceof Error ? cause.message : String(cause);
      }
      return this.standaloneSessions;
    } finally {
      this.loadingStandalone = false;
    }
  }

  async fetchAll(force = false): Promise<void> {
    await Promise.allSettled([this.fetchWorkspaces(force), this.fetchStandaloneSessions(force)]);
  }
}

export const workWorkspaceStore = new WorkWorkspaceStore();
