import * as api from "$lib/api";
import {
  getWorkSession,
  resumeWorkSession,
  sendWorkMessage,
  cancelWorkTurn,
  startStandaloneWorkSession,
  startWorkSession,
  stopWorkSession,
} from "$lib/api/work";
import { getEventMiddleware } from "$lib/stores/event-middleware";
import { SessionStore } from "$lib/stores/session-store.svelte";
import { resolveElicitationOptimistic } from "$lib/utils/resolve-elicitation";
import { resolvePermissionOptimistic } from "$lib/utils/resolve-permission";
import { withTimeout } from "$lib/utils/async-utils";
import type { Attachment, PermissionSuggestion, TaskRun, TimelineEntry } from "$lib/types";
import type { WorkExecutionMode, WorkPreset } from "$lib/types/work";
import type { EffectiveAgentCapabilities } from "$lib/utils/agent-capabilities";
import {
  DEFAULT_WORK_RUNTIME,
  getWorkRuntimeClientOrReadOnly,
  type WorkRuntimeClient,
} from "$lib/work-runtime";

const WORK_LOAD_TIMEOUT_MS = 15_000;
// Stopping is a recovery action. Keep the composer/sidebar usable even when
// the desktop command is waiting on a stuck provider or actor teardown.
const WORK_STOP_TIMEOUT_MS = 10_000;

/** Work coordinator around the generic SessionStore and event reducer. */
export class WorkSessionStore {
  readonly session = new SessionStore();
  readonly middleware = getEventMiddleware();

  workspaceId = $state<string | null>(null);
  /** When true, the session is a standalone (workspace-less) task. */
  standalone = $state(false);
  loading = $state(false);
  starting = $state(false);
  sending = $state(false);
  stopping = $state(false);
  cancellingTurn = $state(false);
  error = $state("");

  private requestVersion = 0;

  constructor(defaultRuntime: string = DEFAULT_WORK_RUNTIME) {
    const normalized = defaultRuntime.trim();
    this.session.agent = normalized || DEFAULT_WORK_RUNTIME;
  }

  get runtimeClient(): WorkRuntimeClient {
    const agent = this.session.run?.agent ?? this.session.agent ?? DEFAULT_WORK_RUNTIME;
    return getWorkRuntimeClientOrReadOnly(agent);
  }

  private setCapabilityError(capability: "Steer" | "Follow-up"): void {
    const message = `Work 当前运行时尚未确认可用的 ${capability} 能力。`;
    this.error = message;
    this.session.error = message;
  }

  private canUseQueueMode(mode: "steer" | "followUp"): boolean {
    return mode === "steer" ? this.canSteer : this.canFollowUp;
  }

  get composerCapabilities(): EffectiveAgentCapabilities {
    return this.runtimeClient.getComposerCapabilities(this.session);
  }

  get canSteer(): boolean {
    return Boolean(this.session.run?.id && this.composerCapabilities.runtime.steer);
  }

  get canFollowUp(): boolean {
    return Boolean(this.session.run?.id && this.composerCapabilities.runtime.followUp);
  }

  get canCancelTurn(): boolean {
    return Boolean(
      this.session.run?.id &&
      this.session.isRunning &&
      this.composerCapabilities.protocol.sessionModeControl,
    );
  }

  get canFork(): boolean {
    return Boolean(this.session.run?.id && this.composerCapabilities.runtime.fork);
  }

  get canClone(): boolean {
    return this.runtimeClient.canClone(this.session);
  }

  get canOpenSessionTree(): boolean {
    return this.runtimeClient.canOpenSessionTree(this.session);
  }

  /** Work transcript projection: hide transport diagnostics before the UI sees them. */
  get visibleTimeline(): TimelineEntry[] {
    return this.session.timeline.filter(
      (entry) => !this.runtimeClient.isDiagnosticTimelineEntry(entry),
    );
  }

  /**
   * Expose Work activity invalidation based on runtime-specific subagent events.
   */
  subscribeSubagentActivity(callback: () => void): () => void {
    return this.middleware.subscribeEvents((event) => {
      if (this.runtimeClient.isSubagentActivityEvent(event)) callback();
    });
  }

  async startMiddleware(): Promise<void> {
    await this.middleware.start();
  }

  async load(workspaceId: string, runId?: string | null, includeLatest = true): Promise<void> {
    const version = ++this.requestVersion;
    this.workspaceId = workspaceId;
    this.standalone = !workspaceId;
    this.loading = true;
    this.error = "";
    this.middleware.subscribeCurrent("", this.session);

    try {
      // Keep the initial reset inside the guarded lifecycle. A reset normally
      // resolves synchronously, but if it ever throws, Work must still leave
      // the loading state so the surface can render its retry affordance.
      await this.session.loadRun("");
      if (runId) {
        const run = await withTimeout(
          api.getRun(runId),
          WORK_LOAD_TIMEOUT_MS,
          "加载 Work 会话超时，请点击“重试连接”。",
        );
        if (version !== this.requestVersion) return;
        if (run) {
          if (run.app_mode !== "work") {
            throw new Error("该会话不属于 Work 模式");
          }
          this.standalone = !run.workspace_id;
          this.middleware.subscribeCurrent(run.id, this.session);
          await withTimeout(
            this.session.loadRun(run.id),
            WORK_LOAD_TIMEOUT_MS,
            "加载 Work 会话超时，请点击“重试连接”。",
          );
        }
      } else if (workspaceId && includeLatest) {
        const run = await withTimeout(
          getWorkSession(workspaceId),
          WORK_LOAD_TIMEOUT_MS,
          "加载 Work 会话超时，请点击“重试连接”。",
        );
        if (version !== this.requestVersion) return;
        if (run) {
          this.middleware.subscribeCurrent(run.id, this.session);
          await withTimeout(
            this.session.loadRun(run.id),
            WORK_LOAD_TIMEOUT_MS,
            "加载 Work 会话超时，请点击“重试连接”。",
          );
        }
      }
    } catch (cause) {
      if (version === this.requestVersion) {
        this.error = cause instanceof Error ? cause.message : String(cause);
      }
    } finally {
      if (version === this.requestVersion) this.loading = false;
    }
  }

  async start(
    message: string,
    model?: string,
    attachments?: Attachment[],
    permissionMode?: WorkExecutionMode,
    preset: WorkPreset = "office",
  ): Promise<TaskRun> {
    this.starting = true;
    this.error = "";
    const optimisticId = this.session.pushOptimisticUser(message, attachments);
    try {
      let run: TaskRun;
      if (this.standalone || !this.workspaceId) {
        run = await startStandaloneWorkSession(
          message,
          model,
          attachments,
          permissionMode,
          preset,
          this.session.agent,
        );
      } else {
        run = await startWorkSession(
          this.workspaceId,
          message,
          model,
          attachments,
          preset,
          this.session.agent,
        );
      }
      this.session.adoptStartedRun(run, optimisticId);
      this.middleware.subscribeCurrent(run.id, this.session);
      // Start navigation immediately. The history catch-up is intentionally
      // detached so a slow event file or runtime handshake cannot block the
      // Work composer; live events remain subscribed during the catch-up.
      void this.session.hydrateStartedRun(run.id);
      return run;
    } catch (cause) {
      this.session.removeOptimisticUser(optimisticId);
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    } finally {
      this.starting = false;
    }
  }

  async send(message: string, attachments?: Attachment[]): Promise<void> {
    const runId = this.session.run?.id;
    if (!runId) throw new Error("Work 会话尚未启动");
    if (this.session.isRunning && !this.canFollowUp) {
      this.setCapabilityError("Follow-up");
      return;
    }
    this.sending = true;
    this.error = "";
    const optimisticId = this.session.pushOptimisticUser(message, attachments);
    try {
      await sendWorkMessage(runId, message, attachments);
    } catch (cause) {
      this.session.removeOptimisticUser(optimisticId);
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    } finally {
      this.sending = false;
    }
  }

  async sendQueuedMessage(
    message: string,
    attachments: Attachment[],
    mode: "steer" | "followUp",
  ): Promise<void> {
    if (!this.canUseQueueMode(mode)) {
      this.setCapabilityError(mode === "steer" ? "Steer" : "Follow-up");
      return;
    }
    await this.session.sendQueuedMessage(message, attachments, mode);
  }

  async steerQueuedMessage(id: string): Promise<void> {
    if (!this.canSteer) {
      this.setCapabilityError("Steer");
      return;
    }
    await this.session.steerQueuedMessage(id);
  }

  async resume(message?: string, attachments?: Attachment[]): Promise<TaskRun> {
    const runId = this.session.run?.id;
    if (!runId) throw new Error("Work 会话尚未创建");
    this.starting = true;
    this.error = "";
    const optimisticId = message ? this.session.pushOptimisticUser(message, attachments) : "";
    try {
      // Resume emits the new turn through the existing bus subscription before the
      // backend command resolves. Keep that subscription in place and preserve the
      // already-rendered transcript; loadRun() clears the timeline and can race the
      // live events with a stale terminal snapshot, making the same run look empty.
      this.middleware.subscribeCurrent(runId, this.session);
      const run = await resumeWorkSession(runId, message, attachments);
      if (this.session.run?.id === run.id) {
        this.session.run = { ...this.session.run, ...run };
      } else {
        this.session.run = run;
      }
      return run;
    } catch (cause) {
      if (optimisticId) this.session.removeOptimisticUser(optimisticId);
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    } finally {
      this.starting = false;
    }
  }

  async stop(): Promise<void> {
    const runId = this.session.run?.id;
    if (!runId || this.stopping) return;
    this.stopping = true;
    this.error = "";
    let stoppedRun: TaskRun | undefined;
    try {
      stoppedRun = await withTimeout(
        stopWorkSession(runId),
        WORK_STOP_TIMEOUT_MS,
        "停止 Work 会话超时，已解除界面占用；后台会继续完成清理。",
      );
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      // The stop command has already persisted the terminal state. Do not
      // clear the transcript and replay events.jsonl on the critical UI path;
      // that replay is only needed when a user later opens the run.
      this.session.adoptStoppedRun(stoppedRun);
      this.starting = false;
      this.sending = false;
      this.stopping = false;
    }
  }

  /** Cancel only the active DSH turn and keep the provider session alive. */
  async cancelTurn(): Promise<void> {
    const runId = this.session.run?.id;
    if (!runId || !this.canCancelTurn || this.cancellingTurn) return;
    this.cancellingTurn = true;
    this.error = "";
    try {
      await withTimeout(
        cancelWorkTurn(runId),
        WORK_STOP_TIMEOUT_MS,
        "取消当前 Work 回合超时，请稍后查看会话状态。",
      );
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    } finally {
      this.cancellingTurn = false;
    }
  }

  async respondPermission(
    requestId: string,
    behavior: "allow" | "deny",
    updatedPermissions?: PermissionSuggestion[],
    updatedInput?: Record<string, unknown>,
    denyMessage?: string,
    interrupt?: boolean,
  ): Promise<void> {
    const runId = this.session.run?.id;
    if (!runId) return;
    try {
      await api.respondPermission(
        runId,
        requestId,
        behavior,
        updatedPermissions,
        updatedInput,
        denyMessage,
        interrupt,
      );
      resolvePermissionOptimistic(this.session, runId, requestId, behavior);
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }
  }

  async respondElicitation(
    requestId: string,
    action: "accept" | "decline" | "cancel",
    content?: Record<string, unknown>,
  ): Promise<void> {
    const runId = this.session.run?.id;
    if (!runId) return;
    try {
      await api.respondElicitation(runId, requestId, action, content);
      resolveElicitationOptimistic(this.session, runId, requestId);
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }
  }

  dispose(): void {
    this.requestVersion += 1;
    const runId = this.session.run?.id;
    if (runId) this.middleware.unsubscribe(runId, this.session);
    this.session.unmountGuards();
  }
}
