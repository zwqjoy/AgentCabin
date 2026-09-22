import {
  addWorkStandingRule,
  createWorkTask,
  deleteWorkTask,
  finishWorkRun,
  getWorkTask,
  listWorkRuns,
  listWorkTasks,
  setWorkspaceDefaultPolicy,
  setWorkspaceModelPreferences,
  startWorkRun,
  updateWorkTask,
} from "$lib/api/work";
import type {
  TaskStandingRule,
  WorkPolicy,
  WorkRun,
  WorkRunStatus,
  WorkRunTrigger,
  WorkTask,
  WorkTaskState,
  WorkWorkspaceSummary,
} from "$lib/types/work";

export class WorkTaskStore {
  tasks = $state<WorkTask[]>([]);
  private taskCache = $state<Record<string, WorkTask>>({});
  activeTaskId = $state<string | null>(null);
  runs = $state<WorkRun[]>([]);
  loading = $state(false);
  error = $state("");

  private inFlightTasks = new Map<string, Promise<WorkTask>>();
  private failedTaskIds = new Set<string>();

  get activeTask(): WorkTask | null {
    if (!this.activeTaskId) return null;
    return this.getTaskById(this.activeTaskId);
  }

  getTaskById(taskId: string): WorkTask | null {
    return this.tasks.find((task) => task.id === taskId) ?? this.taskCache[taskId] ?? null;
  }

  async fetchTasks(workspaceId?: string): Promise<void> {
    this.loading = true;
    this.error = "";
    this.failedTaskIds.clear();
    try {
      const all = await listWorkTasks(workspaceId);
      for (const task of all) this.taskCache[task.id] = task;
      this.tasks = all.filter((task) => task.source === "manual");
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      this.loading = false;
    }
  }

  async fetchTask(taskId: string): Promise<WorkTask | null> {
    if (!taskId) return null;
    const cached = this.getTaskById(taskId);
    if (cached) return cached;
    if (this.failedTaskIds.has(taskId)) return null;

    const inFlight = this.inFlightTasks.get(taskId);
    if (inFlight) return inFlight;

    const promise = (async () => {
      try {
        const task = await getWorkTask(taskId);
        this.taskCache = {
          ...this.taskCache,
          [task.id]: task,
        };
        this.failedTaskIds.delete(taskId);
        return task;
      } catch (cause) {
        this.failedTaskIds.add(taskId);
        this.error = cause instanceof Error ? cause.message : String(cause);
        throw cause;
      } finally {
        this.inFlightTasks.delete(taskId);
      }
    })();

    this.inFlightTasks.set(taskId, promise);
    return promise;
  }

  async selectTask(taskId: string): Promise<void> {
    this.activeTaskId = taskId;
    await this.fetchRuns(taskId);
  }

  async fetchRuns(taskId: string): Promise<void> {
    try {
      this.runs = await listWorkRuns(taskId);
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  private async refreshAfterRunFailure(taskId: string, workspaceId?: string): Promise<void> {
    const refreshes: Promise<void>[] = [this.fetchRuns(taskId)];
    if (workspaceId) refreshes.push(this.fetchTasks(workspaceId));
    await Promise.allSettled(refreshes);
  }

  async createTask(
    workspaceId: string,
    title: string,
    instructions: string,
    policy?: WorkPolicy,
    schedule?: WorkTask["schedule"],
    requiredArtifacts?: string[],
  ): Promise<WorkTask> {
    this.error = "";
    try {
      const task = await createWorkTask(
        workspaceId,
        title,
        instructions,
        policy,
        schedule,
        requiredArtifacts,
      );
      this.taskCache[task.id] = task;
      this.tasks = [task, ...this.tasks];
      this.activeTaskId = task.id;
      return task;
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }
  }

  async deleteTask(taskId: string): Promise<void> {
    this.error = "";
    try {
      await deleteWorkTask(taskId);
      delete this.taskCache[taskId];
      this.tasks = this.tasks.filter((task) => task.id !== taskId);
      if (this.activeTaskId === taskId) {
        this.activeTaskId = null;
        this.runs = [];
      }
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }
  }

  async updateTask(task: WorkTask): Promise<void> {
    this.error = "";
    try {
      await updateWorkTask(task);
      this.taskCache[task.id] = task;
      this.tasks = this.tasks.map((t) => (t.id === task.id ? task : t));
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }
  }

  async updateWorkspaceDefaultPolicy(
    workspaceId: string,
    policy: WorkPolicy,
  ): Promise<WorkWorkspaceSummary> {
    this.error = "";
    try {
      return await setWorkspaceDefaultPolicy(workspaceId, policy);
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }
  }

  async updateWorkspaceModelPreferences(
    workspaceId: string,
    model?: string,
    effort?: string,
  ): Promise<WorkWorkspaceSummary> {
    this.error = "";
    try {
      return await setWorkspaceModelPreferences(workspaceId, model, effort);
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }
  }

  async addStandingRule(taskId: string, rule: TaskStandingRule): Promise<WorkTask> {
    this.error = "";
    try {
      const updated = await addWorkStandingRule(taskId, rule);
      this.taskCache[updated.id] = updated;
      this.tasks = this.tasks.map((t) => (t.id === taskId ? updated : t));
      return updated;
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }
  }

  async startRun(taskId: string, sessionId?: string, trigger?: WorkRunTrigger): Promise<WorkRun> {
    this.error = "";
    const workspaceId = this.tasks.find((task) => task.id === taskId)?.workspaceId;
    try {
      const run = await startWorkRun(taskId, sessionId, trigger);
      this.runs = [run, ...this.runs];
      await this.fetchTasks(workspaceId);
      return run;
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      await this.refreshAfterRunFailure(taskId, workspaceId);
      this.error = message;
      throw cause;
    }
  }

  async finishRun(
    taskId: string,
    runId: string,
    status: WorkRunStatus,
    errorMessage?: string,
    taskState?: WorkTaskState,
  ): Promise<WorkRun> {
    this.error = "";
    const workspaceId = this.tasks.find((task) => task.id === taskId)?.workspaceId;
    try {
      const finished = await finishWorkRun(taskId, runId, status, errorMessage, taskState);
      this.runs = this.runs.map((r) => (r.id === runId ? finished : r));
      await this.fetchTasks(workspaceId);
      return finished;
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      await this.refreshAfterRunFailure(taskId, workspaceId);
      this.error = message;
      throw cause;
    }
  }
}

export const workTaskStore = new WorkTaskStore();
