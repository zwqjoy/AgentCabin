/**
 * TeamStore unit tests.
 *
 * Tests computed getters, watcher event handlers, and cooldown logic.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock Tauri API
vi.mock("$lib/api", () => ({
  getRun: vi.fn(),
  listTeams: vi.fn(),
  getTeamConfig: vi.fn(),
  listTeamTasks: vi.fn(),
  getTeamInbox: vi.fn(),
  getAllTeamInboxes: vi.fn().mockResolvedValue([]),
}));

vi.mock("$lib/utils/debug", () => ({
  dbg: vi.fn(),
  dbgWarn: vi.fn(),
}));

// Import after mocks
import { TeamStore } from "./team-store.svelte";
import * as api from "$lib/api";

describe("TeamStore", () => {
  let store: TeamStore;

  beforeEach(() => {
    store = new TeamStore();
    vi.clearAllMocks();
  });

  describe("initial state", () => {
    it("starts with empty teams and no selection", () => {
      expect(store.teams).toEqual([]);
      expect(store.selectedTeam).toBe("");
      expect(store.teamConfig).toBeNull();
      expect(store.tasks).toEqual([]);
      expect(store.inbox).toEqual([]);
      expect(store.inboxAgent).toBe("");
      expect(store.loading).toBe(false);
    });
  });

  describe("computed getters filter by status", () => {
    it("pendingTasks returns only pending tasks", () => {
      store.tasks = [
        {
          id: "1",
          subject: "A",
          description: "",
          activeForm: "",
          owner: "",
          status: "pending",
          blocks: [],
          blockedBy: [],
        },
        {
          id: "2",
          subject: "B",
          description: "",
          activeForm: "",
          owner: "",
          status: "in_progress",
          blocks: [],
          blockedBy: [],
        },
        {
          id: "3",
          subject: "C",
          description: "",
          activeForm: "",
          owner: "",
          status: "completed",
          blocks: [],
          blockedBy: [],
        },
      ];
      expect(store.pendingTasks).toHaveLength(1);
      expect(store.pendingTasks[0].id).toBe("1");
    });

    it("inProgressTasks returns only in_progress tasks", () => {
      store.tasks = [
        {
          id: "1",
          subject: "A",
          description: "",
          activeForm: "",
          owner: "",
          status: "pending",
          blocks: [],
          blockedBy: [],
        },
        {
          id: "2",
          subject: "B",
          description: "",
          activeForm: "",
          owner: "w",
          status: "in_progress",
          blocks: [],
          blockedBy: [],
        },
        {
          id: "3",
          subject: "C",
          description: "",
          activeForm: "",
          owner: "",
          status: "in_progress",
          blocks: [],
          blockedBy: [],
        },
      ];
      expect(store.inProgressTasks).toHaveLength(2);
    });

    it("completedTasks returns only completed tasks", () => {
      store.tasks = [
        {
          id: "1",
          subject: "A",
          description: "",
          activeForm: "",
          owner: "",
          status: "completed",
          blocks: [],
          blockedBy: [],
        },
        {
          id: "2",
          subject: "B",
          description: "",
          activeForm: "",
          owner: "",
          status: "pending",
          blocks: [],
          blockedBy: [],
        },
      ];
      expect(store.completedTasks).toHaveLength(1);
      expect(store.completedTasks[0].id).toBe("1");
    });
  });

  describe("Claude ordinary subagents", () => {
    it("shows Claude Task/Agent calls as collaboration tasks", async () => {
      vi.mocked(api.getRun).mockResolvedValue({
        agent: "claude",
        model: "sonnet",
        cwd: "/tmp/project",
        prompt: "review the project",
      } as never);

      store.handleBusEvent({
        type: "tool_start",
        run_id: "run-claude",
        tool_use_id: "tool-1",
        tool_name: "Task",
        input: { subagent_type: "Explore", prompt: "Inspect the frontend" },
      });

      await vi.waitFor(() => expect(store.collaborationTasks).toHaveLength(1));
      expect(store.collaborationTasks[0]).toMatchObject({
        agent: "claude-code",
        role: "Explore",
        description: "Inspect the frontend",
        status: "running",
      });
      expect(store.teams[0].name).toBe("Claude · run-clau");

      store.handleBusEvent({
        type: "tool_end",
        run_id: "run-claude",
        tool_use_id: "tool-1",
        tool_name: "Task",
        output: {},
        status: "success",
      });
      expect(store.collaborationTasks[0].status).toBe("completed");
    });

    it("handles Claude's streamed Agent input that starts as null", async () => {
      vi.mocked(api.getRun).mockResolvedValue({
        agent: "claude",
        model: "sonnet",
        cwd: "/tmp/project",
        prompt: "review the project",
      } as never);

      store.handleBusEvent({
        type: "tool_start",
        run_id: "run-streamed-agent",
        tool_use_id: "tool-1",
        tool_name: "Agent",
        input: null,
      });

      await vi.waitFor(() => expect(store.collaborationTasks).toHaveLength(1));
      expect(store.collaborationTasks[0].status).toBe("running");

      store.handleBusEvent({
        type: "tool_input_delta",
        run_id: "run-streamed-agent",
        tool_use_id: "tool-1",
        partial_json: '{"agent":"Explore","description":"只读检查 Rust 代码"}',
      });

      expect(store.collaborationTasks[0]).toMatchObject({
        role: "Explore",
        description: "只读检查 Rust 代码",
      });
      expect(store.teams).toHaveLength(1);
    });

    it("does not show Pi or Codex delegation tools", async () => {
      vi.mocked(api.getRun).mockResolvedValue({
        agent: "pi",
        model: "gpt",
        cwd: "/tmp/project",
        prompt: "delegate work",
      } as never);

      store.handleBusEvent({
        type: "tool_start",
        run_id: "run-pi",
        tool_use_id: "tool-1",
        tool_name: "subagent",
        input: { agent: "scout", task: "Inspect files" },
      });
      store.handleBusEvent({
        type: "tool_start",
        run_id: "run-codex",
        tool_use_id: "tool-2",
        tool_name: "spawn_agent",
        input: { task: "Inspect files" },
      });

      await Promise.resolve();
      expect(store.collaborationTasks).toEqual([]);
      expect(store.teams).toEqual([]);
    });

    it("does not read native Claude Team APIs", async () => {
      await store.loadTeams();
      await store.forceRefresh();
      expect(api.listTeams).not.toHaveBeenCalled();
      expect(api.getTeamConfig).not.toHaveBeenCalled();
      expect(api.listTeamTasks).not.toHaveBeenCalled();
      expect(api.getAllTeamInboxes).not.toHaveBeenCalled();
    });
  });
});
