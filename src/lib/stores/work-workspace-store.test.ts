import { beforeEach, describe, expect, it, vi } from "vitest";
import { WorkWorkspaceStore } from "./work-workspace-store.svelte";
import * as workApi from "$lib/api/work";
import * as transportModule from "$lib/transport";

vi.mock("$lib/api/work", () => ({
  listWorkspaces: vi.fn(),
  listArchivedWorkspaces: vi.fn(),
  listStandaloneWorkSessions: vi.fn(),
}));

vi.mock("$lib/transport", () => ({
  getTransport: vi.fn(),
}));

describe("WorkWorkspaceStore", () => {
  let store: WorkWorkspaceStore;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(transportModule.getTransport).mockReturnValue({
      isDesktop: () => true,
    } as any);
    store = new WorkWorkspaceStore();
  });

  it("initializes with default empty state", () => {
    expect(store.workspaces).toEqual([]);
    expect(store.archivedWorkspaces).toEqual([]);
    expect(store.standaloneSessions).toEqual([]);
    expect(store.workspacesLoaded).toBe(false);
    expect(store.loadingWorkspaces).toBe(false);
  });

  it("fetches workspaces and archived workspaces concurrently", async () => {
    const mockActive = [{ id: "ws-1", name: "Project 1" }] as any;
    const mockArchived = [{ id: "ws-2", name: "Archived 1" }] as any;

    vi.mocked(workApi.listWorkspaces).mockResolvedValue(mockActive);
    vi.mocked(workApi.listArchivedWorkspaces).mockResolvedValue(mockArchived);

    await store.fetchWorkspaces();

    expect(store.workspaces).toEqual(mockActive);
    expect(store.archivedWorkspaces).toEqual(mockArchived);
    expect(store.workspacesLoaded).toBe(true);
    expect(store.loadingWorkspaces).toBe(false);
  });

  it("deduplicates concurrent in-flight fetchWorkspaces calls", async () => {
    let resolveActive: any;
    const activePromise = new Promise((res) => {
      resolveActive = res;
    });
    vi.mocked(workApi.listWorkspaces).mockReturnValue(activePromise as any);
    vi.mocked(workApi.listArchivedWorkspaces).mockResolvedValue([]);

    const call1 = store.fetchWorkspaces();
    const call2 = store.fetchWorkspaces();

    expect(workApi.listWorkspaces).toHaveBeenCalledTimes(1);

    resolveActive([{ id: "ws-1" }]);
    await Promise.all([call1, call2]);

    expect(store.workspaces).toEqual([{ id: "ws-1" }]);
  });

  it("fetches standalone sessions independently", async () => {
    const mockSessions = [{ id: "run-1", name: "Quick Task" }] as any;
    vi.mocked(workApi.listStandaloneWorkSessions).mockResolvedValue(mockSessions);

    await store.fetchStandaloneSessions();

    expect(store.standaloneSessions).toEqual(mockSessions);
    expect(store.standaloneLoaded).toBe(true);
    expect(store.loadingStandalone).toBe(false);
  });

  it("updates and removes workspace properly", () => {
    store.setWorkspaces([
      { id: "ws-1", name: "Old Name" } as any,
      { id: "ws-2", name: "Project 2" } as any,
    ]);

    store.updateWorkspace({ id: "ws-1", name: "New Name" } as any);
    expect(store.getWorkspaceById("ws-1")?.name).toBe("New Name");

    store.removeWorkspace("ws-1");
    expect(store.getWorkspaceById("ws-1")).toBeNull();
    expect(store.workspaces.length).toBe(1);
  });
});
