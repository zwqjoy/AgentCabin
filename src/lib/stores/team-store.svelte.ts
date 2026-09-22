/**
 * CollaborationStore: reactive state for the collaboration-tasks panel.
 *
 * Only Claude Code's ordinary subagent tool calls are normalized here. The
 * native Claude Team filesystem/API and Pi/Codex delegation protocols are not
 * part of this view.
 */
import * as api from "$lib/api";
import type {
  BusEvent,
  CollaborationTask,
  CollaborationTaskStatus,
  TeamSummary,
  TeamConfig,
  TeamTask,
  TeamInboxMessage,
} from "$lib/types";
import { dbg, dbgWarn } from "$lib/utils/debug";

export class TeamStore {
  teams = $state<TeamSummary[]>([]);
  /** Normalized work from Claude Code's ordinary subagents. */
  collaborationTasks = $state<CollaborationTask[]>([]);
  selectedTeam = $state("");
  teamConfig = $state<TeamConfig | null>(null);
  tasks = $state<TeamTask[]>([]);
  inbox = $state<TeamInboxMessage[]>([]);
  allInbox = $state<TeamInboxMessage[]>([]);
  inboxAgent = $state("");
  loading = $state(false);

  /** Currently expanded task id (for description view) */
  expandedTaskId = $state<string | null>(null);

  private _runMeta = new Map<
    string,
    { agent: string; model: string; cwd: string; prompt: string }
  >();
  private _collaborationTeamNames = new Map<string, string>();
  private _pendingToolEndStatuses = new Map<string, CollaborationTaskStatus>();
  private _pendingToolInputJson = new Map<string, string>();

  get pendingTasks(): TeamTask[] {
    return this.tasks.filter((t) => t.status === "pending");
  }

  get inProgressTasks(): TeamTask[] {
    return this.tasks.filter((t) => t.status === "in_progress");
  }

  get completedTasks(): TeamTask[] {
    return this.tasks.filter((t) => t.status === "completed");
  }

  private _collaborationName(runId: string): string {
    const existing = this._collaborationTeamNames.get(runId);
    if (existing) return existing;
    const name = `Claude · ${runId.slice(0, 8)}`;
    this._collaborationTeamNames.set(runId, name);
    return name;
  }

  private _agentLabel(agent: string): string {
    switch (agent.toLowerCase()) {
      case "claude":
      case "claude-code":
        return "Claude";
      default:
        return agent || "Agent";
    }
  }

  private _collaborationRunId(name: string): string | null {
    for (const [runId, teamName] of this._collaborationTeamNames) {
      if (teamName === name) return runId;
    }
    return null;
  }

  private _collaborationTasksForRun(runId: string): CollaborationTask[] {
    return this.collaborationTasks.filter((task) => task.run_id === runId);
  }

  private _rebuildVisibleTeams(): void {
    const runIds = [...new Set(this.collaborationTasks.map((task) => task.run_id))];
    const collaborationSummaries: TeamSummary[] = runIds.map((runId) => {
      const tasks = this._collaborationTasksForRun(runId);
      const meta = this._runMeta.get(runId);
      const firstDescription = meta?.prompt?.trim() || "Agent 协作任务";
      return {
        name: this._collaborationName(runId),
        description: firstDescription.slice(0, 120),
        member_count: tasks.length,
        task_count: tasks.length,
        created_at: Math.min(...tasks.map((task) => task.started_at)),
      };
    });
    this.teams = collaborationSummaries;
  }

  private _isClaudeSubagentTool(toolName: string): boolean {
    const normalized = toolName.toLowerCase().replace(/[\s-]/g, "_");
    // Claude Code's ordinary subagent entry points. Do not match Pi's
    // subagent/subagent_spawn tools or generic task-like payloads.
    return ["task", "agent"].includes(normalized);
  }

  private _stringInput(input: Record<string, unknown>, ...keys: string[]): string {
    for (const key of keys) {
      const value = input[key];
      if (typeof value === "string" && value.trim()) return value.trim();
    }
    return "";
  }

  private _toolInput(input: Record<string, unknown> | null): Record<string, unknown> {
    return input && typeof input === "object" ? input : {};
  }

  private _applyToolInput(
    task: CollaborationTask,
    input: Record<string, unknown>,
  ): CollaborationTask {
    const role = this._stringInput(
      input,
      "subagent_type",
      "subagentType",
      "agent_type",
      "agent",
      "role",
    );
    const description = this._stringInput(
      input,
      "description",
      "prompt",
      "task",
      "subject",
      "instruction",
    );
    return {
      ...task,
      role: role || task.role,
      description: description || task.description,
      model: this._stringInput(input, "model") || task.model,
    };
  }

  private _taskStatusFromToolEnd(status: string): CollaborationTaskStatus {
    const normalized = status.toLowerCase();
    if (["error", "failed", "denied", "permission_denied"].includes(normalized)) return "failed";
    if (["stopped", "cancelled", "canceled"].includes(normalized)) return "stopped";
    return "completed";
  }

  private _taskStatusFromRunState(state: string): CollaborationTaskStatus | null {
    switch (state.toLowerCase()) {
      case "failed":
        return "failed";
      case "stopped":
        return "stopped";
      case "completed":
        return "completed";
      default:
        return null;
    }
  }

  private _upsertCollaborationTask(task: CollaborationTask): void {
    const index = this.collaborationTasks.findIndex((item) => item.id === task.id);
    if (index < 0) {
      this.collaborationTasks = [...this.collaborationTasks, task];
    } else {
      const next = [...this.collaborationTasks];
      next[index] = { ...next[index], ...task };
      this.collaborationTasks = next;
    }
    this._rebuildVisibleTeams();
    if (this.selectedTeam && this._collaborationRunId(this.selectedTeam) === task.run_id) {
      void this.selectTeam(this.selectedTeam);
    }
  }

  private async _ensureRunMeta(runId: string): Promise<void> {
    if (this._runMeta.has(runId)) return;
    try {
      const run = await api.getRun(runId);
      this._runMeta.set(runId, {
        agent: run.agent || "agent",
        model: run.model || "",
        cwd: run.cwd || "",
        prompt: run.prompt || "",
      });
      this._rebuildVisibleTeams();
    } catch (error) {
      dbgWarn("teams", "collaboration run metadata unavailable", { runId, error });
    }
  }

  private async _handleClaudeSubagentStart(
    event: Extract<BusEvent, { type: "tool_start" }>,
  ): Promise<void> {
    await this._ensureRunMeta(event.run_id);
    const meta = this._runMeta.get(event.run_id);
    const agent = meta?.agent.toLowerCase() ?? "";
    if (agent !== "claude" && agent !== "claude-code") return;

    const input = this._toolInput(event.input);
    const taskId = `${event.run_id}:${event.tool_use_id}`;
    const role =
      this._stringInput(input, "subagent_type", "subagentType", "agent_type", "agent", "role") ||
      event.tool_name;
    const description = this._stringInput(
      input,
      "description",
      "prompt",
      "task",
      "subject",
      "instruction",
    );
    this._upsertCollaborationTask({
      id: `${event.run_id}:${event.tool_use_id}`,
      run_id: event.run_id,
      agent: "claude-code",
      role,
      description,
      model: this._stringInput(input, "model") || meta?.model || "",
      cwd: meta?.cwd || "",
      tool_name: event.tool_name,
      status: "running",
      started_at: Date.now(),
    });
    const pendingInput = this._pendingToolInputJson.get(taskId);
    if (pendingInput) {
      try {
        const parsed = JSON.parse(pendingInput);
        if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
          const current = this.collaborationTasks.find((task) => task.id === taskId);
          if (current) this._upsertCollaborationTask(this._applyToolInput(current, parsed));
        }
      } catch {
        // The streamed JSON is incomplete; the regular delta handler will retry.
      }
    }
    const pendingStatus = this._pendingToolEndStatuses.get(taskId);
    if (pendingStatus) {
      this._pendingToolEndStatuses.delete(taskId);
      const current = this.collaborationTasks.find((task) => task.id === taskId);
      if (current) {
        this._upsertCollaborationTask({ ...current, status: pendingStatus, ended_at: Date.now() });
      }
    }
  }

  /** Consume normalized bus events and expose sub-agent work as collaboration tasks. */
  handleBusEvent(event: BusEvent): void {
    if (event.type === "session_init") {
      void this._ensureRunMeta(event.run_id);
      return;
    }

    if (event.type === "tool_start" && this._isClaudeSubagentTool(event.tool_name)) {
      this._pendingToolInputJson.set(`${event.run_id}:${event.tool_use_id}`, "");
      void this._handleClaudeSubagentStart(event);
      return;
    }

    if (event.type === "tool_input_delta") {
      const id = `${event.run_id}:${event.tool_use_id}`;
      const previous = this._pendingToolInputJson.get(id);
      if (previous === undefined) return;
      const next = previous + event.partial_json;
      this._pendingToolInputJson.set(id, next);
      try {
        const parsed = JSON.parse(next);
        if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
          const current = this.collaborationTasks.find((task) => task.id === id);
          if (current) this._upsertCollaborationTask(this._applyToolInput(current, parsed));
        }
      } catch {
        // Wait for the next partial JSON chunk.
      }
      return;
    }

    if (event.type === "tool_end") {
      const id = `${event.run_id}:${event.tool_use_id}`;
      const current = this.collaborationTasks.find((task) => task.id === id);
      if (current) {
        this._pendingToolInputJson.delete(id);
        this._upsertCollaborationTask({
          ...current,
          status: this._taskStatusFromToolEnd(event.status),
          ended_at: Date.now(),
        });
      } else if (this._isClaudeSubagentTool(event.tool_name)) {
        // Run metadata lookup is async; remember a fast tool completion so the
        // eventual task projection cannot remain incorrectly stuck at running.
        this._pendingToolEndStatuses.set(id, this._taskStatusFromToolEnd(event.status));
      }
      return;
    }

    if (event.type === "run_state") {
      const status = this._taskStatusFromRunState(event.state);
      if (!status) return;
      for (const task of this.collaborationTasks.filter(
        (item) => item.run_id === event.run_id && item.status === "running",
      )) {
        this._upsertCollaborationTask({ ...task, status, ended_at: Date.now() });
      }
    }
  }

  /** Rebuild the list from observed Claude subagent events only. */
  async loadTeams(): Promise<void> {
    this.loading = true;
    try {
      this._rebuildVisibleTeams();
      dbg("teams", "loadTeams", { count: this.teams.length });
    } catch (e) {
      dbgWarn("teams", "loadTeams error", e);
    } finally {
      this.loading = false;
    }
  }

  /** Rebuild visible collaboration tasks and selected state. */
  async forceRefresh(): Promise<void> {
    dbg("teams", "forceRefresh");
    try {
      this._rebuildVisibleTeams();
      if (this.selectedTeam) await this.selectTeam(this.selectedTeam);
    } catch (e) {
      dbgWarn("teams", "forceRefresh error", e);
    }
  }

  /** Select a team and fetch its config + tasks + all inboxes. Always fetches fresh data. */
  async selectTeam(name: string): Promise<void> {
    this.selectedTeam = name;
    if (!name) {
      this.teamConfig = null;
      this.tasks = [];
      this.inbox = [];
      this.allInbox = [];
      this.inboxAgent = "";
      this.expandedTaskId = null;
      return;
    }
    dbg("teams", "selectTeam", name);
    const collaborationRunId = this._collaborationRunId(name);
    if (collaborationRunId) {
      const collaborationTasks = this._collaborationTasksForRun(collaborationRunId);
      const meta = this._runMeta.get(collaborationRunId);
      const colors = ["blue", "green", "purple", "orange", "cyan", "pink"];
      const members = collaborationTasks.map((task, index) => ({
        agentId: task.id,
        name: task.role || `子 Agent ${index + 1}`,
        agentType: this._agentLabel(task.agent),
        model: task.model,
        color: colors[index % colors.length],
        planModeRequired: false,
        joinedAt: task.started_at,
        tmuxPaneId: "",
        cwd: task.cwd || meta?.cwd || "",
        subscriptions: [],
        backendType: task.agent,
        prompt: task.description,
        isActive: task.status === "running",
      }));
      this.teamConfig = {
        name,
        description: meta?.prompt || "Agent 协作任务",
        createdAt: Math.min(...collaborationTasks.map((task) => task.started_at)),
        leadAgentId: `lead:${collaborationRunId}`,
        leadSessionId: collaborationRunId,
        members,
      };
      this.tasks = collaborationTasks.map((task) => ({
        id: task.id,
        subject: task.role || task.tool_name,
        description: task.description,
        activeForm: task.role,
        owner: task.role,
        status:
          task.status === "running"
            ? "in_progress"
            : task.status === "completed"
              ? "completed"
              : task.status,
        blocks: [],
        blockedBy: [],
        metadata: { agent: task.agent, tool_name: task.tool_name },
      }));
      this.inbox = [];
      this.allInbox = [];
      return;
    }
    this.teamConfig = null;
    this.tasks = [];
    this.inbox = [];
    this.allInbox = [];
  }

  /** Load inbox messages for a specific agent in the current team. */
  async loadInbox(team: string, agent: string): Promise<void> {
    this.inboxAgent = agent;
    this.inbox = [];
  }

  /** Load all inboxes merged for the current team. */
  async loadAllInbox(team: string): Promise<void> {
    if (this.selectedTeam === team) this.allInbox = [];
  }

  /** Remove the local collaboration-task projection. */
  async deleteTeam(name: string): Promise<void> {
    dbg("teams", "deleteTeam", name);
    const collaborationRunId = this._collaborationRunId(name);
    if (collaborationRunId) {
      this.collaborationTasks = this.collaborationTasks.filter(
        (task) => task.run_id !== collaborationRunId,
      );
      this._rebuildVisibleTeams();
    }
    // Clear selection if the deleted team was selected
    if (this.selectedTeam === name) {
      this.selectedTeam = "";
      this.teamConfig = null;
      this.tasks = [];
      this.inbox = [];
      this.allInbox = [];
      this.inboxAgent = "";
      this.expandedTaskId = null;
    }
    // Refresh visible collaboration-task list
    this.teams = this.teams.filter((t) => t.name !== name);
  }

  /** Legacy no-op: native Claude Team watcher events are intentionally ignored. */
  handleTeamUpdate(payload: { team_name: string; change: string }): void {
    dbg("teams", "ignored native team-update", payload);
  }

  /** Legacy no-op: native Claude Team task events are intentionally ignored. */
  handleTaskUpdate(payload: { team_name: string; task_id: string; change: string }): void {
    dbg("teams", "ignored native task-update", payload);
  }
}
