import { goto } from "$app/navigation";
import {
  createWorkspace,
  createWorkspaceFromFolder,
  relinkWorkspaceFolder,
  renameWorkspace,
  setWorkArtifactStorageMode,
} from "$lib/api/work";
import { getTransport } from "$lib/transport";
import { workWorkspaceStore } from "$lib/stores/work-workspace-store.svelte";
import { workProjectionStore } from "$lib/stores/work-projection-store.svelte";
import { withTimeout } from "$lib/utils/async-utils";
import * as workResources from "$lib/work/work-resource-service";
import { standaloneScope, workspaceScope, type WorkScope } from "$lib/work/work-scope";
import type {
  WorkAccessRoot,
  WorkArtifactSummary,
  WorkArtifactStorageMode,
  WorkFileSummary,
  WorkProfile,
  WorkRecoveryAction,
  WorkRunProgressView,
  WorkRunRecovery,
  WorkWorkspaceSummary,
} from "$lib/types/work";

const PROFILE_LOAD_TIMEOUT_MS = 15_000;

/** Route scope identity: which conversation the page is showing. */
export interface WorkRouteScopeInput {
  workspaceId: string;
  runId: string;
  newConversation: boolean;
  legacyView: string;
}

/** Live session context supplied by the mounted conversation surface. */
export interface WorkRuntimeContext {
  sessionRunId: string | null;
  progressView: WorkRunProgressView | null;
}

export interface WorkPageRouteInput extends WorkRouteScopeInput {
  conversationRunId: string;
}

function recoveryQuery(conversationRunId: string, runtime: WorkRuntimeContext) {
  return {
    run: runtime.sessionRunId
      ? {
          id: runtime.sessionRunId,
          work_task_id: runtime.progressView?.taskId,
          work_run_id: runtime.progressView?.workRunId,
        }
      : null,
    progressView: runtime.progressView,
  };
}

/**
 * Page-level orchestrator for the Work route.
 *
 * Owns the conversation-adjacent resources (artifacts, input files, access
 * roots, profile, recovery) and every backend call behind them, including the
 * load/refresh lifecycle that previously lived inline in +page.svelte. The
 * route component only parses the URL, calls `syncRoute`, and renders.
 */
export class WorkPageController {
  artifacts = $state<WorkArtifactSummary[]>([]);
  inputFiles = $state<WorkFileSummary[]>([]);
  accessRoots = $state<WorkAccessRoot[]>([]);
  profile = $state<WorkProfile | null>(null);
  recovery = $state<WorkRunRecovery | null>(null);
  loading = $state(true);
  error = $state("");

  private input: WorkPageRouteInput = {
    workspaceId: "",
    runId: "",
    newConversation: false,
    legacyView: "",
    conversationRunId: "",
  };

  private loadVersion = 0;
  private artifactRefreshVersion = 0;
  private lastLoadedScope: string | null = null;
  private lastArtifactScope: string | null = null;
  private lastConversationRunId = "";

  private isDesktop(): boolean {
    return getTransport().isDesktop();
  }

  // ── Scope helpers ──────────────────────────────────────────────────────────

  isStandalone(): boolean {
    return !this.input.workspaceId && !this.input.legacyView;
  }

  hasActiveConversation(): boolean {
    return this.input.workspaceId
      ? Boolean(this.input.runId || this.input.newConversation)
      : !this.input.legacyView;
  }

  scope(): WorkScope {
    return this.input.workspaceId ? workspaceScope(this.input.workspaceId) : standaloneScope();
  }

  // ── Route lifecycle ────────────────────────────────────────────────────────

  /**
   * Reconcile the route with loaded resources. Returns true when the route
   * was an adopted just-started run (the caller should clear its adoption
   * marker); the conversation surface must not remount for those.
   *
   * Call from a `$effect` that tracks only the URL-derived scope inputs; pass
   * everything else inside so it is evaluated untracked, mirroring the
   * original page semantics.
   */
  syncRoute(
    scope: WorkRouteScopeInput,
    adoptedRunScope: string,
    conversationRunId: string,
    runtime: WorkRuntimeContext,
  ): boolean {
    this.input = { ...scope, conversationRunId };
    const routeScope = `${scope.workspaceId}:${scope.runId || "workspace"}`;
    const artifactScope = `${scope.workspaceId}:${scope.runId || (scope.newConversation ? "new" : "workspace")}`;

    // Clear the previous conversation's results as soon as navigation changes.
    // A newly started run can intentionally skip the metadata fan-out while
    // its transcript is streaming, so leaving this state in place makes an
    // older task's artifact appear to belong to the current task.
    if (artifactScope !== this.lastArtifactScope) {
      this.lastArtifactScope = artifactScope;
      this.artifactRefreshVersion += 1;
      this.loadVersion += 1;
      this.artifacts = [];
    }

    // WorkChatSurface adopts a newly started run in-place and shallowly
    // updates the URL. Its bound sessionInfo already points at that run, so
    // do not kick off the workspace metadata/files/artifacts fan-out while
    // the first long response is streaming. Selecting an existing run still
    // reloads normally because sessionInfo has not caught up yet.
    const adoptedByEvent = Boolean(scope.runId) && adoptedRunScope === routeScope;
    const adoptedByLiveSession =
      Boolean(scope.runId) &&
      runtime.sessionRunId === scope.runId &&
      this.lastLoadedScope !== null &&
      this.lastLoadedScope.startsWith(`${scope.workspaceId}:`);
    if (adoptedByEvent || adoptedByLiveSession) {
      this.lastLoadedScope = routeScope;
      if (scope.runId) void this.refreshArtifacts();
      return true;
    }

    if (routeScope !== this.lastLoadedScope || workWorkspaceStore.workspaces.length === 0) {
      this.lastLoadedScope = routeScope;
      void this.loadScope();
    }
    return false;
  }

  /** Refresh artifacts once a "新对话" adopts its first run id (deduped). */
  syncConversationRun(conversationRunId: string): void {
    this.input = { ...this.input, conversationRunId };
    if (!this.input.workspaceId || !this.hasActiveConversation() || Boolean(this.input.runId)) {
      this.lastConversationRunId = conversationRunId;
      return;
    }
    if (conversationRunId && conversationRunId !== this.lastConversationRunId) {
      this.lastConversationRunId = conversationRunId;
      void this.refreshArtifacts();
    }
  }

  private async loadScope(): Promise<void> {
    const version = ++this.loadVersion;
    const input = this.input;
    if (workWorkspaceStore.workspaces.length === 0 && !workWorkspaceStore.workspacesLoaded) {
      this.loading = true;
    }
    this.error = "";
    if (!this.isDesktop()) {
      this.loading = false;
      return;
    }
    try {
      const conversationActive = Boolean(
        input.runId || input.newConversation || (!input.workspaceId && !input.legacyView),
      );
      const standalone = !input.workspaceId && !input.legacyView;
      const scope = standalone ? standaloneScope() : workspaceScope(input.workspaceId);
      const [, nextProfile, nextArtifacts, nextInputFiles, nextAccessRoots] = await Promise.all([
        workWorkspaceStore.fetchWorkspaces(),
        withTimeout(workResources.getProfile(), PROFILE_LOAD_TIMEOUT_MS, "读取 Work 配置超时"),
        conversationActive
          ? input.runId
            ? workResources.listArtifacts(scope, input.runId)
            : Promise.resolve<WorkArtifactSummary[]>([])
          : input.workspaceId
            ? workResources.listArtifacts(scope, null)
            : Promise.resolve<WorkArtifactSummary[]>([]),
        workResources.listInputFiles(scope),
        workResources.listAccessRoots(scope),
      ]);
      if (version !== this.loadVersion) return;
      this.profile = nextProfile;
      this.artifacts = nextArtifacts;
      this.inputFiles = nextInputFiles;
      this.accessRoots = nextAccessRoots;
    } catch (cause) {
      if (version !== this.loadVersion) return;
      this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (version === this.loadVersion) this.loading = false;
    }
  }

  // ── Artifacts ──────────────────────────────────────────────────────────────

  private removeArtifactFromState(artifactId: string): void {
    // Invalidate an in-flight refresh before removing the card locally. A
    // refresh triggered by the task stream must not put the old card back.
    this.artifactRefreshVersion += 1;
    this.artifacts = this.artifacts.filter((item) => item.id !== artifactId);
  }

  async refreshArtifacts(): Promise<void> {
    const version = ++this.artifactRefreshVersion;
    const input = this.input;
    let nextArtifacts: WorkArtifactSummary[];
    if (this.isStandalone()) {
      nextArtifacts = input.conversationRunId
        ? await workResources.listArtifacts(standaloneScope(), input.conversationRunId)
        : [];
    } else if (!input.workspaceId) {
      return;
    } else if (this.hasActiveConversation()) {
      nextArtifacts = input.conversationRunId
        ? await workResources.listArtifacts(
            workspaceScope(input.workspaceId),
            input.conversationRunId,
          )
        : [];
    } else {
      nextArtifacts = await workResources.listArtifacts(workspaceScope(input.workspaceId), null);
    }
    // A terminal/tool event can refresh while the user is deleting an artifact.
    // Do not let an older response overwrite the newer local state.
    if (version === this.artifactRefreshVersion) this.artifacts = nextArtifacts;
  }

  async deleteArtifact(artifactId: string): Promise<void> {
    const artifact = this.artifacts.find((item) => item.id === artifactId);
    if (!artifact) return;
    const { confirm } = await import("$lib/platform/dialog");
    if (this.isStandalone()) {
      const ok = await confirm(`确定删除成果「${artifact.title}」吗？对应文件会一并删除。`, {
        title: "删除成果",
        kind: "warning",
      });
      if (!ok) return;
      await workResources.deleteArtifact(
        { scope: standaloneScope(), runId: this.input.conversationRunId, artifactId },
        artifact,
      );
      this.removeArtifactFromState(artifactId);
      return;
    }
    if (!this.input.workspaceId) return;
    const shared = this.artifacts.some(
      (item) => item.id !== artifactId && item.path === artifact.path,
    );
    const message = shared
      ? `确定删除成果「${artifact.title}」吗？output/ 中的文件仍被其他成果引用，将会保留。`
      : `确定删除成果「${artifact.title}」吗？output/ 中的对应文件会一并删除。`;
    const ok = await confirm(message, { title: "删除成果", kind: "warning" });
    if (!ok) return;
    await workResources.deleteArtifact({
      scope: workspaceScope(this.input.workspaceId),
      runId: this.input.conversationRunId,
      artifactId,
    });
    this.removeArtifactFromState(artifactId);
  }

  async validateArtifact(artifactId: string): Promise<void> {
    const artifact = this.artifacts.find((item) => item.id === artifactId);
    if (!artifact) throw new Error("成果不存在或已被移除。");
    const updated = await workResources.validateArtifact({
      scope: this.scope(),
      runId: this.input.conversationRunId,
      artifactId,
      artifactRunId: artifact.runId || null,
    });
    this.artifacts = this.artifacts.map((item) => (item.id === artifactId ? updated : item));
  }

  async deliverArtifact(artifactId: string, runtime: WorkRuntimeContext): Promise<void> {
    const artifact = this.artifacts.find((item) => item.id === artifactId);
    if (!artifact) throw new Error("成果不存在或已被移除。");
    const updated = await workResources.deliverArtifact({
      scope: this.scope(),
      runId: this.input.conversationRunId,
      artifactId,
      artifactRunId: artifact.runId || null,
    });
    this.artifacts = this.artifacts.map((item) => (item.id === artifactId ? updated : item));

    // WaitingDelivery is a durable run state. Once the user explicitly
    // delivers the file, run the existing Verify gate so the WorkRun can move
    // to Completed only after the full current-run acceptance check passes.
    const progressView = runtime.progressView;
    if (
      !this.isStandalone() &&
      progressView?.runStatus === "waiting_delivery" &&
      progressView.taskId &&
      progressView.workRunId
    ) {
      await this.handleRecoveryAction("verify", undefined, runtime);
    }
  }

  async exportArtifact(artifactId: string): Promise<void> {
    const artifact = this.artifacts.find((item) => item.id === artifactId);
    const { save } = await import("$lib/platform/dialog");
    const destination = await save({
      defaultPath: artifact?.title || "work-artifact",
      filters: artifact?.artifactType
        ? [{ name: artifact.artifactType.toUpperCase(), extensions: [artifact.artifactType] }]
        : undefined,
    });
    if (!destination) return;
    await workResources.exportArtifact(
      this.scope(),
      this.input.conversationRunId,
      artifactId,
      destination,
      artifact?.runId || null,
    );
  }

  async copyArtifactToPrimary(artifactId: string, rootKind?: string | null): Promise<string> {
    const artifact = this.artifacts.find((item) => item.id === artifactId);
    if (!artifact) throw new Error("成果不存在或已被移除。");
    if (!this.input.workspaceId || rootKind !== "local_folder") {
      throw new Error("当前 Workspace 没有关联可复制的本地工作目录。");
    }
    return await workResources.copyArtifactToPrimary(
      workspaceScope(this.input.workspaceId),
      artifactId,
      artifact.runId || null,
    );
  }

  async saveOfficeArtifact(artifactId: string, contentBase64: string): Promise<void> {
    await workResources.saveOfficeArtifact(
      this.scope(),
      this.input.conversationRunId,
      artifactId,
      contentBase64,
    );
    await this.refreshArtifacts();
  }

  async openArtifact(artifactId: string): Promise<void> {
    const artifact = this.artifacts.find((item) => item.id === artifactId);
    if (!artifact) return;
    if (this.isStandalone() && !this.input.conversationRunId) return;
    await workResources.openFile(this.scope(), this.input.conversationRunId, artifact.path);
  }

  async openArtifactDirectory(): Promise<void> {
    if (this.isStandalone() && !this.input.conversationRunId) return;
    await workResources.openDirectory(this.scope(), this.input.conversationRunId, "output");
  }

  async updateArtifactStorageMode(mode: WorkArtifactStorageMode): Promise<void> {
    if (!this.input.workspaceId) return;
    if (mode === "primary_work_root") {
      const { confirm } = await import("$lib/platform/dialog");
      const ok = await confirm(
        "启用后，新成果会直接写入所选本地文件夹的 output/。已有成果不会迁移。继续吗？",
        { title: "直接保存成果", kind: "warning" },
      );
      if (!ok) return;
    }
    try {
      const updated = await setWorkArtifactStorageMode(this.input.workspaceId, mode);
      workWorkspaceStore.updateWorkspace(updated);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }
  }

  // ── Workspace files ────────────────────────────────────────────────────────

  async importInputFile(sourcePath: string): Promise<void> {
    if (!this.input.workspaceId) return;
    const imported = await workResources.importInputFile(
      workspaceScope(this.input.workspaceId),
      sourcePath,
    );
    if (!imported) return;
    this.inputFiles = [imported, ...this.inputFiles.filter((file) => file.path !== imported.path)];
  }

  async openWorkspaceFile(relativePath: string): Promise<void> {
    if (!this.input.workspaceId) return;
    await workResources.openFile(
      workspaceScope(this.input.workspaceId),
      this.input.conversationRunId,
      relativePath,
    );
  }

  async removeInputFile(relativePath: string): Promise<void> {
    if (!this.input.workspaceId) return;
    await workResources.removeInputFile(workspaceScope(this.input.workspaceId), relativePath);
    this.inputFiles = this.inputFiles.filter((file) => file.path !== relativePath);
  }

  // ── Access roots ───────────────────────────────────────────────────────────

  async addAccessRoot(): Promise<void> {
    const workspaceId = this.input.workspaceId;
    if (!workspaceId) return;
    const { open } = await import("$lib/platform/dialog");
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择 Work 可访问的目录",
    });
    if (typeof selected !== "string") return;
    try {
      this.accessRoots = await workResources.addAccessRoot(
        workspaceScope(workspaceId),
        selected,
        false,
      );
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async toggleAccessRoot(root: WorkAccessRoot): Promise<void> {
    if (!this.input.workspaceId) return;
    try {
      this.accessRoots = await workResources.setAccessRootWritable(
        workspaceScope(this.input.workspaceId),
        root.path,
        !root.writable,
      );
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async removeAccessRoot(root: WorkAccessRoot): Promise<void> {
    if (!this.input.workspaceId) return;
    const { confirm } = await import("$lib/platform/dialog");
    const ok = await confirm(`移除目录"${root.path}"？`, {
      title: "移除目录",
      kind: "warning",
    });
    if (!ok) return;
    try {
      this.accessRoots = await workResources.removeAccessRoot(
        workspaceScope(this.input.workspaceId),
        root.path,
      );
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  // ── Receipt & recovery ─────────────────────────────────────────────────────

  /** 任务回执（Ledger 投影）：Workspace run 用 task/run id，standalone 用会话 run id。 */
  loadRunReceipt(runtime: WorkRuntimeContext) {
    return workResources.getReceipt({
      scope: this.scope(),
      conversationRunId: this.input.conversationRunId,
      ...recoveryQuery(this.input.conversationRunId, runtime),
    });
  }

  async refreshRecovery(runtime: WorkRuntimeContext): Promise<void> {
    if (this.isStandalone() || !this.input.workspaceId) {
      this.recovery = null;
      return;
    }
    try {
      this.recovery = await workResources.getRecovery({
        scope: workspaceScope(this.input.workspaceId),
        conversationRunId: this.input.conversationRunId,
        ...recoveryQuery(this.input.conversationRunId, runtime),
      });
    } catch (cause) {
      // A transient projection race while a new session is attaching should
      // not replace the conversation with a global error banner.
      this.recovery = null;
      const status = runtime.progressView?.runStatus;
      if (status === "recoverable" || status === "waiting_delivery") {
        this.error = cause instanceof Error ? cause.message : String(cause);
      }
    }
  }

  async handleRecoveryAction(
    action: WorkRecoveryAction,
    subagentId?: string,
    runtime?: WorkRuntimeContext,
  ): Promise<void> {
    if (this.isStandalone() || !this.input.workspaceId) return;
    const context: WorkRuntimeContext = runtime ?? { sessionRunId: null, progressView: null };
    const nextRun = await workResources.recoverRun(
      {
        scope: workspaceScope(this.input.workspaceId),
        conversationRunId: this.input.conversationRunId,
        ...recoveryQuery(this.input.conversationRunId, context),
      },
      action,
      subagentId,
    );
    if (!nextRun) return;
    const nextRecovery =
      nextRun.status === "recoverable" || nextRun.status === "waiting_delivery"
        ? workResources.getRecovery({
            scope: workspaceScope(this.input.workspaceId),
            conversationRunId: this.input.conversationRunId,
            run: {
              id: nextRun.sessionId || this.input.conversationRunId,
              work_task_id: nextRun.taskId,
              work_run_id: nextRun.id,
            },
          })
        : Promise.resolve(null);
    await Promise.allSettled([
      workProjectionStore.fetch(nextRun.id),
      nextRecovery.then((value) => (this.recovery = value)),
      this.refreshArtifacts(),
      this.loadScope(),
    ]);
    if (action === "from_scratch" && nextRun.sessionId) {
      void goto(
        `/chat/work?workspace=${encodeURIComponent(this.input.workspaceId)}&run=${encodeURIComponent(nextRun.sessionId)}`,
        { replaceState: true },
      );
    }
  }

  // ── Workspace mutations ────────────────────────────────────────────────────

  /** Force a full scope reload (profile, artifacts, files, access roots). */
  async reload(): Promise<void> {
    await this.loadScope();
  }

  async createWorkspace(name: string): Promise<WorkWorkspaceSummary> {
    const workspace = await createWorkspace(name);
    workWorkspaceStore.addWorkspace(workspace);
    window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
    return workspace;
  }

  async createWorkspaceFromFolder(folderPath: string): Promise<WorkWorkspaceSummary> {
    const workspace = await createWorkspaceFromFolder(folderPath);
    workWorkspaceStore.addWorkspace(workspace);
    window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
    return workspace;
  }

  async renameWorkspace(id: string, name: string): Promise<void> {
    const updated = await renameWorkspace(id, name);
    workWorkspaceStore.updateWorkspace(updated);
    window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
  }

  async relinkWorkspaceFolder(id: string, folderPath: string): Promise<void> {
    const updated = await relinkWorkspaceFolder(id, folderPath);
    workWorkspaceStore.updateWorkspace(updated);
    window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
  }
}
