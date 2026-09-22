<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import { platform } from "$lib/platform";
  import { getTransport } from "$lib/transport";
  import { workProjectionStore } from "$lib/stores/work-projection-store.svelte";
  import { onMount, untrack } from "svelte";
  import { trapFocus } from "$lib/utils/focus-trap";
  import { withTimeout } from "$lib/utils/async-utils";
  import {
    getWorkConversationSurfaceKey,
    getWorkConversationSurfaceKeyAfterRouteChange,
  } from "$lib/utils/work-sidebar-navigation";
  import {
    createWorkspace,
    createWorkspaceFromFolder,
    relinkWorkspaceFolder,
    getWorkProfile,
    renameWorkspace,
    setWorkArtifactStorageMode,
  } from "$lib/api/work";
  import * as workResources from "$lib/work/work-resource-service";
  import { workspaceScope, standaloneScope, type WorkScope } from "$lib/work/work-scope";
  import { workWorkspaceStore } from "$lib/stores/work-workspace-store.svelte";
  import WorkChatSurface from "$lib/components/work/WorkChatSurface.svelte";
  import WorkConversationInspectorAside from "$lib/components/work/WorkConversationInspectorAside.svelte";
  import WorkWorkspaceView from "$lib/components/work/WorkWorkspaceView.svelte";
  import WorkHomeView from "$lib/components/work/WorkHomeView.svelte";
  import WorkInboxPanel from "$lib/components/work/WorkInboxPanel.svelte";
  import WorkAutomationCenter from "$lib/components/work/WorkAutomationCenter.svelte";
  import WorkMaterialsCenter from "$lib/components/work/WorkMaterialsCenter.svelte";
  import ArchivedChatsView from "$lib/components/ArchivedChatsView.svelte";
  import type { SessionInfoData } from "$lib/types";
  import type {
    InboxItem,
    WorkArtifactSummary,
    WorkArtifactStorageMode,
    WorkAccessRoot,
    WorkFileSummary,
    WorkProfile,
    WorkProgressSnapshot,
    WorkRecoveryAction,
    WorkRunRecovery,
    WorkRunProgressView,
    WorkWorkspaceSummary,
  } from "$lib/types/work";

  let workspaces = $derived(workWorkspaceStore.workspaces);
  let artifacts = $state<WorkArtifactSummary[]>([]);
  let inputFiles = $state<WorkFileSummary[]>([]);
  let accessRoots = $state<WorkAccessRoot[]>([]);
  let profile = $state<WorkProfile | null>(null);
  let sessionInfo = $state<SessionInfoData | null>(null);
  let progress = $state<WorkProgressSnapshot | null>(null);
  let progressView = $state<WorkRunProgressView | null>(null);
  let recovery = $state<WorkRunRecovery | null>(null);
  let pendingInteractions = $state<InboxItem[]>([]);
  let conversationArchived = $state(false);
  let artifactRefreshVersion = 0;
  // The default surface is the goal, approval, and result. Progress, run, and
  // artifact details stay in the adjacent task panel while a run is active.
  let showConversationInspector = $state(false);

  // Pending interactions are rendered inline in the chat surface and highlighted
  // via badges without intrusively forcing the right sidebar open.
  let loading = $state(true);
  let saving = $state(false);
  let error = $state("");
  let loadVersion = 0;
  let workspaceName = $state("");
  let createError = $state("");
  let workspaceNameInput = $state<HTMLInputElement>();
  let createDialog = $state<HTMLDivElement>();
  let accessBusyPath = $state("");
  const workTransportSupported = getTransport().isDesktop();

  let selectedId = $derived($page.url.searchParams.get("workspace") ?? "");
  let selectedRunId = $derived($page.url.searchParams.get("run") ?? "");
  let selectedView = $derived($page.url.searchParams.get("view") ?? "");
  let newConversation = $derived($page.url.searchParams.get("newSession") === "1");
  let createOpen = $derived($page.url.searchParams.get("new") === "1");
  let selectedWorkspace = $derived(
    workspaces.find((workspace) => workspace.id === selectedId) ?? null,
  );
  let isStandalone = $derived(!selectedId && !selectedView);
  let hasActiveConversation = $derived(
    selectedWorkspace ? Boolean(selectedRunId || newConversation) : !selectedView,
  );
  // Artifacts shown next to an active conversation belong to that conversation's
  // run. A brand-new conversation has no run yet, so it starts with none.
  let conversationRunId = $derived(selectedRunId || sessionInfo?.runId || "");
  // Stable surface key: prevent {#key} from remounting WorkChatSurface when
  // adoptStartedRun() transitions the same conversation from no-run → run-id.
  // When a new conversation gets its first run (empty runId → runId, same workspace),
  // WorkChatSurface already holds the live session from workSession.start().
  // Remounting at that moment discards the live state and re-enters booting=true
  // while the AI is streaming events, causing a reactive update storm that
  // freezes the entire app.
  let adoptedRunScope = $state("");
  let conversationSurfaceKey = $state(
    untrack(() => getWorkConversationSurfaceKey(selectedId, selectedRunId)),
  );
  let _surfaceKeyWsId = $state(untrack(() => selectedId));
  let _surfaceKeyRunId = $state(untrack(() => selectedRunId));

  $effect(() => {
    const wsId = selectedId;
    const runId = selectedRunId;
    const isAdopted =
      Boolean(runId) && (adoptedRunScope === `${wsId}:${runId}` || sessionInfo?.runId === runId);
    untrack(() => {
      const nextSurfaceKey = getWorkConversationSurfaceKeyAfterRouteChange(
        conversationSurfaceKey,
        _surfaceKeyWsId,
        _surfaceKeyRunId,
        wsId,
        runId,
        isAdopted,
      );

      _surfaceKeyWsId = wsId;
      _surfaceKeyRunId = runId;
      conversationSurfaceKey = nextSurfaceKey;
    });
  });
  let currentWorkspaceRoot = $derived(selectedWorkspace?.root || sessionInfo?.cwd || "");

  $effect(() => {
    if (!createOpen) return;
    requestAnimationFrame(() => workspaceNameInput?.focus());
  });
  async function loadWorkspaces(workspaceId = selectedId, runId = selectedRunId) {
    const version = ++loadVersion;
    if (workspaces.length === 0 && !workWorkspaceStore.workspacesLoaded) {
      loading = true;
    }
    error = "";
    if (!workTransportSupported) {
      loading = false;
      return;
    }
    try {
      const conversationActive = Boolean(
        runId || newConversation || (!workspaceId && !selectedView),
      );
      const standalone = !workspaceId && !selectedView;
      const scope = standalone ? standaloneScope() : workspaceScope(workspaceId);
      const [, nextProfile, nextArtifacts, nextInputFiles, nextAccessRoots] = await Promise.all([
        workWorkspaceStore.fetchWorkspaces(),
        withTimeout(getWorkProfile(), 15_000, "读取 Work 配置超时"),
        conversationActive
          ? runId
            ? workResources.listArtifacts(scope, runId)
            : Promise.resolve<WorkArtifactSummary[]>([])
          : workspaceId
            ? workResources.listArtifacts(scope, null)
            : Promise.resolve<WorkArtifactSummary[]>([]),
        workResources.listInputFiles(scope),
        workResources.listAccessRoots(scope),
      ]);
      if (version !== loadVersion) return;
      profile = nextProfile;
      artifacts = nextArtifacts;
      inputFiles = nextInputFiles;
      accessRoots = nextAccessRoots;
    } catch (cause) {
      if (version !== loadVersion) return;
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (version === loadVersion) loading = false;
    }
  }

  function removeArtifactFromState(artifactId: string): void {
    // Invalidate an in-flight refresh before removing the card locally. A
    // refresh triggered by the task stream must not put the old card back.
    artifactRefreshVersion += 1;
    artifacts = artifacts.filter((item) => item.id !== artifactId);
  }

  function currentScope(): WorkScope {
    return isStandalone ? standaloneScope() : workspaceScope(selectedId);
  }

  async function refreshArtifacts() {
    const version = ++artifactRefreshVersion;
    let nextArtifacts: WorkArtifactSummary[];
    if (isStandalone) {
      nextArtifacts = conversationRunId
        ? await workResources.listArtifacts(standaloneScope(), conversationRunId)
        : [];
    } else if (!selectedId) {
      return;
    } else if (hasActiveConversation) {
      nextArtifacts = conversationRunId
        ? await workResources.listArtifacts(workspaceScope(selectedId), conversationRunId)
        : [];
    } else {
      nextArtifacts = await workResources.listArtifacts(workspaceScope(selectedId), null);
    }
    // A terminal/tool event can refresh while the user is deleting an artifact.
    // Do not let an older response overwrite the newer local state.
    if (version === artifactRefreshVersion) artifacts = nextArtifacts;
  }

  async function refreshRecovery() {
    if (isStandalone || !selectedId) {
      recovery = null;
      return;
    }
    try {
      recovery = await workResources.getRecovery({
        scope: workspaceScope(selectedId),
        conversationRunId,
        run: sessionInfo?.runId
          ? {
              id: sessionInfo.runId,
              work_task_id: progressView?.taskId,
              work_run_id: progressView?.workRunId,
            }
          : null,
        progressView,
      });
    } catch (cause) {
      // A transient projection race while a new session is attaching should
      // not replace the conversation with a global error banner.
      recovery = null;
      if (
        progressView?.runStatus === "recoverable" ||
        progressView?.runStatus === "waiting_delivery"
      ) {
        error = cause instanceof Error ? cause.message : String(cause);
      }
    }
  }

  async function handleRecoveryAction(action: WorkRecoveryAction, subagentId?: string) {
    if (isStandalone || !selectedId) return;
    const nextRun = await workResources.recoverRun(
      {
        scope: workspaceScope(selectedId),
        conversationRunId,
        run: sessionInfo?.runId
          ? {
              id: sessionInfo.runId,
              work_task_id: progressView?.taskId,
              work_run_id: progressView?.workRunId,
            }
          : null,
        progressView,
      },
      action,
      subagentId,
    );
    if (!nextRun) return;
    const nextRecovery =
      nextRun.status === "recoverable" || nextRun.status === "waiting_delivery"
        ? workResources.getRecovery({
            scope: workspaceScope(selectedId),
            conversationRunId,
            run: {
              id: nextRun.sessionId || conversationRunId,
              work_task_id: nextRun.taskId,
              work_run_id: nextRun.id,
            },
          })
        : Promise.resolve(null);
    await Promise.allSettled([
      workProjectionStore.fetch(nextRun.id),
      nextRecovery.then((value) => (recovery = value)),
      refreshArtifacts(),
      loadWorkspaces(selectedId, selectedRunId),
    ]);
    if (action === "from_scratch" && nextRun.sessionId) {
      void goto(
        `/chat/work?workspace=${encodeURIComponent(selectedId)}&run=${encodeURIComponent(nextRun.sessionId)}`,
        { replaceState: true },
      );
    }
  }

  async function deleteArtifact(artifactId: string) {
    const artifact = artifacts.find((item) => item.id === artifactId);
    if (!artifact) return;
    if (isStandalone) {
      const { confirm } = await import("$lib/platform/dialog");
      const ok = await confirm(`确定删除成果「${artifact.title}」吗？对应文件会一并删除。`, {
        title: "删除成果",
        kind: "warning",
      });
      if (!ok) return;
      await workResources.deleteArtifact(
        { scope: standaloneScope(), runId: conversationRunId, artifactId },
        artifact,
      );
      removeArtifactFromState(artifactId);
      return;
    }
    if (!selectedId) return;
    const shared = artifacts.some((item) => item.id !== artifactId && item.path === artifact.path);
    const message = shared
      ? `确定删除成果「${artifact.title}」吗？output/ 中的文件仍被其他成果引用，将会保留。`
      : `确定删除成果「${artifact.title}」吗？output/ 中的对应文件会一并删除。`;
    const { confirm } = await import("$lib/platform/dialog");
    const ok = await confirm(message, {
      title: "删除成果",
      kind: "warning",
    });
    if (!ok) return;
    await workResources.deleteArtifact({
      scope: workspaceScope(selectedId),
      runId: conversationRunId,
      artifactId,
    });
    removeArtifactFromState(artifactId);
  }

  async function validateArtifact(artifactId: string) {
    const artifact = artifacts.find((item) => item.id === artifactId);
    if (!artifact) throw new Error("成果不存在或已被移除。");
    const updated = await workResources.validateArtifact({
      scope: currentScope(),
      runId: conversationRunId,
      artifactId,
      artifactRunId: artifact.runId || null,
    });
    artifacts = artifacts.map((item) => (item.id === artifactId ? updated : item));
  }

  async function deliverArtifact(artifactId: string) {
    const artifact = artifacts.find((item) => item.id === artifactId);
    if (!artifact) throw new Error("成果不存在或已被移除。");
    const updated = await workResources.deliverArtifact({
      scope: currentScope(),
      runId: conversationRunId,
      artifactId,
      artifactRunId: artifact.runId || null,
    });
    artifacts = artifacts.map((item) => (item.id === artifactId ? updated : item));

    // WaitingDelivery is a durable run state. Once the user explicitly
    // delivers the file, run the existing Verify gate so the WorkRun can move
    // to Completed only after the full current-run acceptance check passes.
    if (
      !isStandalone &&
      progressView?.runStatus === "waiting_delivery" &&
      progressView.taskId &&
      progressView.workRunId
    ) {
      await handleRecoveryAction("verify");
    }
  }

  async function exportArtifact(artifactId: string) {
    const artifact = artifacts.find((item) => item.id === artifactId);
    const { save } = await import("$lib/platform/dialog");
    const destination = await save({
      defaultPath: artifact?.title || "work-artifact",
      filters: artifact?.artifactType
        ? [{ name: artifact.artifactType.toUpperCase(), extensions: [artifact.artifactType] }]
        : undefined,
    });
    if (!destination) return;
    await workResources.exportArtifact(
      currentScope(),
      conversationRunId,
      artifactId,
      destination,
      artifact?.runId || null,
    );
  }

  async function copyArtifactToPrimary(artifactId: string): Promise<string> {
    const artifact = artifacts.find((item) => item.id === artifactId);
    if (!artifact) throw new Error("成果不存在或已被移除。");
    if (!selectedId || selectedWorkspace?.rootKind !== "local_folder") {
      throw new Error("当前 Workspace 没有关联可复制的本地工作目录。");
    }
    return await workResources.copyArtifactToPrimary(
      workspaceScope(selectedId),
      artifactId,
      artifact.runId || null,
    );
  }

  async function updateArtifactStorageMode(mode: WorkArtifactStorageMode): Promise<void> {
    if (!selectedId) return;
    if (mode === "primary_work_root") {
      const { confirm } = await import("$lib/platform/dialog");
      const ok = await confirm(
        "启用后，新成果会直接写入所选本地文件夹的 output/。已有成果不会迁移。继续吗？",
        { title: "直接保存成果", kind: "warning" },
      );
      if (!ok) return;
    }
    try {
      const updated = await setWorkArtifactStorageMode(selectedId, mode);
      workWorkspaceStore.updateWorkspace(updated);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }
  }

  async function importInputFile(sourcePath: string) {
    if (!selectedId) return;
    const imported = await workResources.importInputFile(workspaceScope(selectedId), sourcePath);
    if (!imported) return;
    inputFiles = [imported, ...inputFiles.filter((file) => file.path !== imported.path)];
  }

  async function openWorkspaceFile(relativePath: string) {
    if (!selectedId) return;
    await workResources.openFile(workspaceScope(selectedId), conversationRunId, relativePath);
  }

  async function removeInputFile(relativePath: string) {
    if (!selectedId) return;
    await workResources.removeInputFile(workspaceScope(selectedId), relativePath);
    inputFiles = inputFiles.filter((file) => file.path !== relativePath);
  }

  async function addAccessRoot() {
    if (!selectedId || accessBusyPath) return;
    const { open } = await import("$lib/platform/dialog");
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择 Work 可访问的目录",
    });
    if (typeof selected !== "string") return;
    accessBusyPath = selected;
    try {
      accessRoots = await workResources.addAccessRoot(workspaceScope(selectedId), selected, false);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      accessBusyPath = "";
    }
  }

  async function toggleAccessRoot(root: WorkAccessRoot) {
    if (!selectedId || accessBusyPath) return;
    accessBusyPath = root.path;
    try {
      accessRoots = await workResources.setAccessRootWritable(
        workspaceScope(selectedId),
        root.path,
        !root.writable,
      );
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      accessBusyPath = "";
    }
  }

  async function removeAccessRoot(root: WorkAccessRoot) {
    if (!selectedId || accessBusyPath) return;
    const { confirm } = await import("$lib/platform/dialog");
    const ok = await confirm(`移除目录"${root.path}"？`, {
      title: "移除目录",
      kind: "warning",
    });
    if (!ok) return;
    accessBusyPath = root.path;
    try {
      accessRoots = await workResources.removeAccessRoot(workspaceScope(selectedId), root.path);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      accessBusyPath = "";
    }
  }

  async function openArtifact(artifactId: string) {
    const artifact = artifacts.find((item) => item.id === artifactId);
    if (!artifact) return;
    if (isStandalone && !conversationRunId) return;
    await workResources.openFile(currentScope(), conversationRunId, artifact.path);
  }

  async function openArtifactDirectory() {
    if (isStandalone && !conversationRunId) return;
    await workResources.openDirectory(currentScope(), conversationRunId, "output");
  }

  /** 任务回执（Ledger 投影）：Workspace run 用 task/run id，standalone 用会话 run id。 */
  async function loadRunReceipt() {
    return await workResources.getReceipt({
      scope: currentScope(),
      conversationRunId,
      run: sessionInfo?.runId
        ? {
            id: sessionInfo.runId,
            work_task_id: progressView?.taskId,
            work_run_id: progressView?.workRunId,
          }
        : null,
      progressView,
    });
  }

  async function saveOfficeArtifact(artifactId: string, contentBase64: string): Promise<void> {
    await workResources.saveOfficeArtifact(
      currentScope(),
      conversationRunId,
      artifactId,
      contentBase64,
    );
    await refreshArtifacts();
  }

  function openCreate() {
    workspaceName = "";
    createError = "";
    void goto("/chat/work?new=1");
  }

  function openNewConversation() {
    if (!selectedId) return;
    if (typeof window !== "undefined") {
      window.dispatchEvent(
        new CustomEvent("agentcabin:work-new-chat", {
          detail: { workspaceId: selectedId },
        }),
      );
    }
    void goto(`/chat/work?workspace=${encodeURIComponent(selectedId)}&newSession=1`);
  }

  function closeCreate() {
    const target = selectedId
      ? `/chat/work?workspace=${encodeURIComponent(selectedId)}`
      : "/chat/work";
    void goto(target, { replaceState: true });
  }

  async function openFolderWorkspace() {
    try {
      const { open } = await import("$lib/platform/dialog");
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择要作为工作区的本地文件夹",
      });
      if (typeof selected !== "string" || !selected.trim()) return;
      saving = true;
      error = "";
      const workspace = await createWorkspaceFromFolder(selected);
      workWorkspaceStore.addWorkspace(workspace);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
      await goto(`/chat/work?workspace=${encodeURIComponent(workspace.id)}&newSession=1`, {
        replaceState: true,
      });
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      saving = false;
    }
  }

  async function relinkPrimaryFolder() {
    if (!selectedId) return;
    try {
      const { open } = await import("$lib/platform/dialog");
      const selected = await open({
        directory: true,
        multiple: false,
        title: "重新选择关联的本地文件夹",
      });
      if (typeof selected !== "string" || !selected.trim()) return;
      accessBusyPath = selected;
      const updated = await relinkWorkspaceFolder(selectedId, selected);
      workWorkspaceStore.updateWorkspace(updated);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      accessBusyPath = "";
    }
  }

  async function openPrimaryFolder(folderPath: string) {
    if (!selectedId) return;
    try {
      await workResources.openDirectory(workspaceScope(selectedId), conversationRunId, folderPath);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function submitCreate() {
    const name = workspaceName.trim();
    if (!name) {
      createError = "请输入工作空间名称";
      return;
    }
    saving = true;
    createError = "";
    error = "";
    try {
      const workspace = await createWorkspace(name);
      workWorkspaceStore.addWorkspace(workspace);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
      await goto(`/chat/work?workspace=${encodeURIComponent(workspace.id)}`, {
        replaceState: true,
      });
    } catch (cause) {
      createError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      saving = false;
    }
  }

  // ── Fresh-workspace hint (shown once right after a quick-start creation) ──
  let freshWorkspace = $derived($page.url.searchParams.get("fresh") === "1");
  let freshRenaming = $state(false);
  let freshRenameValue = $state("");

  function startFreshRename() {
    freshRenameValue = selectedWorkspace?.name ?? "";
    freshRenaming = true;
  }

  async function confirmFreshRename() {
    const name = freshRenameValue.trim();
    if (name && selectedWorkspace && name !== selectedWorkspace.name) {
      await renameWorkspaceInList(name);
    }
    freshRenaming = false;
  }

  function dismissFreshHint() {
    const url = new URL($page.url);
    url.searchParams.delete("fresh");
    void goto(url.pathname + url.search, { replaceState: true });
  }

  async function renameWorkspaceInList(name: string) {
    if (!selectedId) return;
    try {
      const updated = await renameWorkspace(selectedId, name);
      workWorkspaceStore.updateWorkspace(updated);
      window.dispatchEvent(new CustomEvent("agentcabin:workspaces-changed"));
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  onMount(() => {
    const onChanged = () => void loadWorkspaces();
    const onSessionStarted = (event: Event) => {
      const detail = (event as CustomEvent<{ workspaceId?: string; run?: { id?: string } }>).detail;
      if (detail?.run?.id) {
        adoptedRunScope = `${detail.workspaceId ?? ""}:${detail.run.id}`;
      }
    };
    window.addEventListener("agentcabin:workspaces-changed", onChanged);
    window.addEventListener("agentcabin:work-sessions-changed", onSessionStarted);
    return () => {
      window.removeEventListener("agentcabin:workspaces-changed", onChanged);
      window.removeEventListener("agentcabin:work-sessions-changed", onSessionStarted);
    };
  });

  let lastLoadedScope: string | null = null;
  let lastArtifactScope: string | null = null;
  $effect(() => {
    const workspaceId = selectedId;
    const runId = selectedRunId;
    const scope = `${workspaceId}:${runId || "workspace"}`;
    const artifactScope = `${workspaceId}:${runId || (newConversation ? "new" : "workspace")}`;
    untrack(() => {
      // Clear the previous conversation's results as soon as navigation changes.
      // A newly started run can intentionally skip the metadata fan-out while
      // its transcript is streaming, so leaving this state in place makes an
      // older task's artifact appear to belong to the current task.
      if (artifactScope !== lastArtifactScope) {
        lastArtifactScope = artifactScope;
        artifactRefreshVersion += 1;
        loadVersion += 1;
        artifacts = [];
      }
      // WorkChatSurface adopts a newly started run in-place and shallowly
      // updates the URL. Its bound sessionInfo already points at that run, so
      // do not kick off the workspace metadata/files/artifacts fan-out while
      // the first long response is streaming. Selecting an existing run still
      // reloads normally because sessionInfo has not caught up yet.
      if (
        adoptedRunScope === scope ||
        (runId &&
          sessionInfo?.runId === runId &&
          lastLoadedScope !== null &&
          lastLoadedScope.startsWith(`${workspaceId}:`))
      ) {
        adoptedRunScope = "";
        lastLoadedScope = scope;
        if (runId) void refreshArtifacts();
        return;
      }
      if (scope !== lastLoadedScope || workspaces.length === 0) {
        lastLoadedScope = scope;
        void loadWorkspaces(workspaceId, runId);
      }
    });
  });

  let lastRecoveryScope = "";
  $effect(() => {
    const scope = `${selectedId}:${progressView?.taskId ?? ""}:${progressView?.workRunId ?? ""}:${progressView?.runStatus ?? ""}`;
    untrack(() => {
      if (scope === lastRecoveryScope) return;
      lastRecoveryScope = scope;
      void refreshRecovery();
    });
  });

  // A conversation opened via "新对话" has no run in the URL. Once its run
  // starts, reload Artifacts scoped to that run instead of the whole Workspace.
  let lastConversationRunId = "";
  $effect(() => {
    const runId = conversationRunId;
    if (!selectedId || !hasActiveConversation || selectedRunId) {
      lastConversationRunId = runId;
      return;
    }
    if (runId && runId !== lastConversationRunId) {
      lastConversationRunId = runId;
      untrack(() => void refreshArtifacts());
    }
  });
</script>

<svelte:head>
  <title>Work · AgentCabin</title>
</svelte:head>

<div
  class="h-full min-h-0 bg-background {hasActiveConversation
    ? 'px-0 py-0'
    : 'px-4 py-4 sm:px-5 lg:px-6'}"
>
  <div class="mx-auto flex h-full min-h-0 max-w-[1600px] flex-col">
    {#if !workTransportSupported}
      <section
        class="flex min-h-0 flex-1 items-center justify-center rounded-2xl border border-primary/20 bg-gradient-to-br from-primary/[0.08] via-card/70 to-card/30 p-6 sm:p-10"
        aria-labelledby="work-desktop-only-title"
      >
        <div class="max-w-lg text-center">
          <div
            class="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-primary/10 text-primary"
            aria-hidden="true"
          >
            <svg
              viewBox="0 0 24 24"
              class="h-7 w-7"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <rect x="3" y="4" width="18" height="13" rx="2" />
              <path d="M8 21h8M12 17v4M7 8h10M7 11h6" />
            </svg>
          </div>
          <h1
            id="work-desktop-only-title"
            class="mt-5 text-xl font-bold text-foreground sm:text-2xl"
          >
            Work 需要桌面 App
          </h1>
          <p class="mx-auto mt-2 max-w-md text-sm leading-6 text-muted-foreground">
            Work
            会在本机读取授权目录、执行任务并保存交付成果。当前浏览器预览不会启动本地运行时，请回到桌面
            App 使用完整功能。
          </p>
          <div class="mt-5 flex flex-wrap justify-center gap-2">
            <a
              href="/chat"
              class="inline-flex min-h-10 items-center rounded-xl border border-border/80 bg-card px-4 py-2 text-xs font-semibold text-foreground transition-colors hover:bg-accent"
            >
              返回 Code
            </a>
            <span
              class="inline-flex min-h-10 items-center rounded-xl bg-primary/10 px-4 py-2 text-xs font-medium text-primary"
            >
              本地 Work 工作模式
            </span>
          </div>
        </div>
      </section>
    {:else}
      {#if error}
        <div
          class="mb-3 shrink-0 rounded-xl border border-red-400/25 bg-red-400/5 px-4 py-3 text-sm text-red-300"
          role="alert"
        >
          {error}
        </div>
      {/if}

      {#if loading}
        <div class="flex min-h-0 flex-1 items-center justify-center text-sm text-muted-foreground">
          <span
            class="mr-2 h-4 w-4 animate-spin rounded-full border-2 border-muted-foreground/20 border-t-primary"
          ></span>
          正在准备 Work…
        </div>
      {:else if isStandalone}
        <div class="flex min-h-0 min-w-0 flex-1 flex-col">
          <div class="relative mt-0 flex min-h-0 min-w-0 flex-1 gap-0">
            <section class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
              {#key conversationSurfaceKey}
                <WorkChatSurface
                  workspace={undefined}
                  {workspaces}
                  onCreateWorkspace={openCreate}
                  onOpenFolderWorkspace={() => void openFolderWorkspace()}
                  onSelectWorkspace={(id: string | null) =>
                    void goto(
                      id
                        ? `/chat/work?workspace=${encodeURIComponent(id)}&newSession=1`
                        : `/chat/work?newSession=1`,
                    )}
                  {profile}
                  runId={selectedRunId || null}
                  newConversation={newConversation || !selectedRunId}
                  {artifacts}
                  onArtifactsChanged={refreshArtifacts}
                  bind:sessionInfo
                  bind:progress
                  bind:progressView
                  bind:conversationArchived
                  bind:pendingInteractions
                  onToggleInspector={() => (showConversationInspector = !showConversationInspector)}
                  inspectorOpen={showConversationInspector}
                />
              {/key}
            </section>

            <WorkConversationInspectorAside
              open={showConversationInspector}
              onClose={() => (showConversationInspector = false)}
              {sessionInfo}
              {progress}
              {progressView}
              readOnly={conversationArchived}
              {artifacts}
              {pendingInteractions}
              workspaceRoot={currentWorkspaceRoot}
              primaryWorkRoot={selectedWorkspace?.primaryWorkRoot ?? ""}
              artifactStorageMode={selectedWorkspace?.artifactStorageMode ?? "managed"}
              onExportArtifact={exportArtifact}
              onOpenArtifact={openArtifact}
              onDeleteArtifact={deleteArtifact}
              onCopyArtifactToPrimary={copyArtifactToPrimary}
              onValidateArtifact={validateArtifact}
              onDeliverArtifact={deliverArtifact}
              onOpenArtifactDirectory={openArtifactDirectory}
              onSaveOfficeArtifact={saveOfficeArtifact}
              getReceipt={loadRunReceipt}
              onArtifactsChanged={refreshArtifacts}
            />
          </div>
        </div>
      {:else if selectedWorkspace}
        {#if !hasActiveConversation}
          <div class="flex min-h-0 min-w-0 flex-1 flex-col">
            <WorkWorkspaceView
              workspace={selectedWorkspace}
              {inputFiles}
              {accessRoots}
              {artifacts}
              {accessBusyPath}
              onImportInputFile={importInputFile}
              onOpenWorkspaceFile={openWorkspaceFile}
              onRemoveInputFile={removeInputFile}
              onAddAccessRoot={() => void addAccessRoot()}
              onToggleAccessRoot={(root) => void toggleAccessRoot(root)}
              onRemoveAccessRoot={(root) => void removeAccessRoot(root)}
              onOpenPrimaryFolder={(path) => void openPrimaryFolder(path)}
              onRelinkPrimaryFolder={() => void relinkPrimaryFolder()}
              onExportArtifact={exportArtifact}
              onCopyArtifactToPrimary={copyArtifactToPrimary}
              onSetArtifactStorageMode={updateArtifactStorageMode}
              onOpenArtifact={openArtifact}
              onDeleteArtifact={deleteArtifact}
              onValidateArtifact={validateArtifact}
              onDeliverArtifact={deliverArtifact}
              onOpenArtifactDirectory={openArtifactDirectory}
              onSaveOfficeArtifact={saveOfficeArtifact}
              onNewConversation={openNewConversation}
              onRenameWorkspace={renameWorkspaceInList}
            />
          </div>
        {:else}
          <div class="flex min-h-0 min-w-0 flex-1 flex-col">
            {#if freshWorkspace && selectedWorkspace}
              <div
                class="shrink-0 border-b border-blue-500/20 bg-blue-500/5 px-4 py-2.5 text-xs sm:px-5"
                role="status"
              >
                {#if freshRenaming}
                  <div class="flex flex-wrap items-center gap-2">
                    <span class="text-foreground/90">给这个工作区起个正式名字：</span>
                    <input
                      bind:value={freshRenameValue}
                      onkeydown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          void confirmFreshRename();
                        }
                        if (e.key === "Escape") freshRenaming = false;
                      }}
                      class="min-h-7 w-56 rounded-lg border border-border bg-background px-2 py-1 text-xs text-foreground outline-none focus:border-primary focus:ring-1 focus:ring-primary/20"
                      maxlength="80"
                    />
                    <button
                      type="button"
                      class="rounded-lg bg-primary px-2.5 py-1 text-[11px] font-semibold text-primary-foreground hover:opacity-90"
                      onclick={() => void confirmFreshRename()}>确定</button
                    >
                    <button
                      type="button"
                      class="rounded-lg px-2.5 py-1 text-[11px] text-muted-foreground hover:bg-accent"
                      onclick={() => (freshRenaming = false)}>取消</button
                    >
                  </div>
                {:else}
                  <div class="flex flex-wrap items-center gap-2">
                    <svg
                      class="h-3.5 w-3.5 shrink-0 text-blue-500"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      aria-hidden="true"
                    >
                      <circle cx="12" cy="12" r="10" />
                      <path d="M12 16v-4M12 8h.01" />
                    </svg>
                    <span class="text-foreground/90">
                      已自动创建工作区「{selectedWorkspace.name}」——完成工作后给它起个正式名字，方便以后找回。
                    </span>
                    <button
                      type="button"
                      class="rounded-lg bg-primary/10 px-2.5 py-1 text-[11px] font-semibold text-primary hover:bg-primary/20"
                      onclick={startFreshRename}>起个名字</button
                    >
                    <button
                      type="button"
                      class="ml-auto rounded-lg px-2 py-1 text-[11px] text-muted-foreground hover:bg-accent"
                      onclick={dismissFreshHint}>知道了</button
                    >
                  </div>
                {/if}
              </div>
            {/if}
            <div class="relative flex min-h-0 min-w-0 flex-1">
              <section class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
                {#key conversationSurfaceKey}
                  <WorkChatSurface
                    workspace={selectedWorkspace}
                    {workspaces}
                    onCreateWorkspace={openCreate}
                    onOpenFolderWorkspace={() => void openFolderWorkspace()}
                    onSelectWorkspace={(id: string | null) =>
                      void goto(
                        id
                          ? `/chat/work?workspace=${encodeURIComponent(id)}&newSession=1`
                          : `/chat/work?newSession=1`,
                      )}
                    {profile}
                    runId={selectedRunId || null}
                    {newConversation}
                    {artifacts}
                    onArtifactsChanged={refreshArtifacts}
                    onWorkspaceChanged={(ws: WorkWorkspaceSummary) => {
                      workWorkspaceStore.updateWorkspace(ws);
                    }}
                    bind:sessionInfo
                    bind:progress
                    bind:progressView
                    bind:conversationArchived
                    bind:pendingInteractions
                    onToggleInspector={() =>
                      (showConversationInspector = !showConversationInspector)}
                    inspectorOpen={showConversationInspector}
                    onExportArtifact={exportArtifact}
                  />
                {/key}
              </section>

              <WorkConversationInspectorAside
                open={showConversationInspector}
                onClose={() => (showConversationInspector = false)}
                {sessionInfo}
                {progress}
                {progressView}
                {recovery}
                readOnly={conversationArchived}
                {artifacts}
                {pendingInteractions}
                workspaceRoot={currentWorkspaceRoot}
                primaryWorkRoot={selectedWorkspace?.primaryWorkRoot ?? ""}
                artifactStorageMode={selectedWorkspace?.artifactStorageMode ?? "managed"}
                onExportArtifact={exportArtifact}
                onOpenArtifact={openArtifact}
                onDeleteArtifact={deleteArtifact}
                onCopyArtifactToPrimary={copyArtifactToPrimary}
                onValidateArtifact={validateArtifact}
                onDeliverArtifact={deliverArtifact}
                onOpenArtifactDirectory={openArtifactDirectory}
                onSaveOfficeArtifact={saveOfficeArtifact}
                getReceipt={loadRunReceipt}
                onArtifactsChanged={refreshArtifacts}
              />
            </div>
          </div>
        {/if}
      {:else if selectedView === "inbox"}
        <div
          class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-2xl border border-border/70 bg-card/50 shadow-sm p-4 sm:p-5"
        >
          <WorkInboxPanel workspaceId="" {workspaces} />
        </div>
      {:else if selectedView === "tasks" || selectedView === "automation"}
        <div
          class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-2xl border border-border/70 bg-card/50 shadow-sm p-4 sm:p-5"
        >
          <WorkAutomationCenter workspaceId="" {workspaces} />
        </div>
      {:else if selectedView === "library"}
        <div
          class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-2xl border border-border/70 bg-card/50 shadow-sm"
        >
          <WorkMaterialsCenter {workspaces} />
        </div>
      {:else if selectedView === "archived"}
        <div
          class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto rounded-2xl border border-border/70 bg-card/50 shadow-sm"
        >
          <ArchivedChatsView realm="work" />
        </div>
      {:else}
        <div class="flex min-h-0 min-w-0 flex-1 flex-col">
          <WorkHomeView
            {workspaces}
            onSelectWorkspace={(id) => void goto(`/chat/work?workspace=${encodeURIComponent(id)}`)}
            onCreateWorkspace={openCreate}
            onOpenFolderWorkspace={() => void openFolderWorkspace()}
          />
        </div>
      {/if}
    {/if}
  </div>
</div>

{#if createOpen && workTransportSupported}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/30 px-4 backdrop-blur-[2px]"
    role="dialog"
    aria-modal="true"
    aria-labelledby="workspace-create-title"
    tabindex="-1"
    onclick={(event) => event.target === event.currentTarget && closeCreate()}
    onkeydown={(event) => {
      if (event.key === "Escape" && !saving) {
        event.preventDefault();
        event.stopPropagation();
        closeCreate();
      }
      trapFocus(event, createDialog ?? null);
    }}
  >
    <div
      bind:this={createDialog}
      class="w-full max-w-lg rounded-2xl border border-border bg-card p-6 shadow-2xl space-y-6"
    >
      <div class="flex items-start justify-between gap-4">
        <div>
          <h2 id="workspace-create-title" class="text-lg font-semibold text-foreground">
            新建任务 / 工作空间
          </h2>
          <p class="mt-1 text-xs text-muted-foreground">
            选择本地文件夹作为主工作目录，或创建独立工作空间
          </p>
        </div>
        <button
          type="button"
          class="rounded-lg p-2 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          aria-label="关闭"
          onclick={closeCreate}
        >
          <svg
            viewBox="0 0 24 24"
            class="h-4 w-4"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"><path d="m6 6 12 12M18 6 6 18" /></svg
          >
        </button>
      </div>

      <!-- Option 1: Primary Action - Open Local Folder -->
      <div class="space-y-3">
        <button
          type="button"
          class="group w-full flex items-start gap-4 rounded-xl border border-primary/40 bg-primary/5 p-4 text-left transition-all hover:border-primary hover:bg-primary/10 hover:shadow-sm"
          onclick={async () => {
            closeCreate();
            await openFolderWorkspace();
          }}
        >
          <span
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-primary text-primary-foreground shadow-xs"
          >
            <svg
              viewBox="0 0 24 24"
              class="h-5 w-5"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path
                d="M3 7.5A2.5 2.5 0 0 1 5.5 5h4l2 2h7A2.5 2.5 0 0 1 21 9.5v7A2.5 2.5 0 0 1 18.5 19h-13A2.5 2.5 0 0 1 3 16.5Z"
              />
            </svg>
          </span>
          <div class="min-w-0 flex-1">
            <div class="flex items-center justify-between">
              <span class="text-sm font-semibold text-foreground group-hover:text-primary">
                打开本地文件夹
              </span>
              <span class="rounded bg-primary/20 px-2 py-0.5 text-[10px] font-semibold text-primary"
                >推荐</span
              >
            </div>
            <p class="mt-1 text-xs text-muted-foreground">
              选择本地代码库或项目文件夹，任务将直接以此目录为基础开展工作（默认读写）。
            </p>
          </div>
        </button>

        <!-- Option 2: Quick Task (Standalone) -->
        <button
          type="button"
          class="group w-full flex items-start gap-4 rounded-xl border border-border/80 bg-card/60 p-3.5 text-left transition-all hover:border-border hover:bg-accent/50"
          onclick={() => {
            closeCreate();
            if (typeof window !== "undefined") {
              window.dispatchEvent(
                new CustomEvent("agentcabin:work-new-chat", {
                  detail: { workspaceId: "" },
                }),
              );
            }
            void goto("/chat/work?newSession=1");
          }}
        >
          <span
            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground group-hover:text-foreground"
          >
            <svg
              viewBox="0 0 24 24"
              class="h-4 w-4"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
            >
              <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
            </svg>
          </span>
          <div class="min-w-0 flex-1">
            <span class="text-xs font-semibold text-foreground group-hover:text-primary">
              快速任务
            </span>
            <p class="mt-0.5 text-[11px] text-muted-foreground">
              不关联特定本地目录，快速开启一次性协作或探索对话。
            </p>
          </div>
        </button>
      </div>

      <!-- Option 3: Create Managed Workspace with Name -->
      <div class="border-t border-border/60 pt-4">
        <form
          onsubmit={(event) => {
            event.preventDefault();
            void submitCreate();
          }}
        >
          <label class="block text-xs font-medium text-muted-foreground" for="workspace-name">
            或新建独立工作空间（无需绑定本地文件夹）
          </label>
          <div class="mt-2 flex gap-2">
            <input
              id="workspace-name"
              class="min-h-10 flex-1 rounded-xl border border-border bg-background px-3 text-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary focus:ring-1 focus:ring-primary/20"
              placeholder="输入工作空间名称，例如：数据分析"
              maxlength="80"
              bind:value={workspaceName}
              bind:this={workspaceNameInput}
            />
            <button
              type="submit"
              class="min-h-10 shrink-0 rounded-xl bg-secondary px-4 text-xs font-semibold text-secondary-foreground transition-all hover:bg-secondary/80 disabled:opacity-60"
              disabled={saving || !workspaceName.trim()}
            >
              {saving ? "创建中…" : "创建"}
            </button>
          </div>
          {#if createError}
            <p
              class="mt-2 rounded-lg border border-red-400/30 bg-red-400/5 px-3 py-1.5 text-xs text-red-500"
              role="alert"
            >
              {createError}
            </p>
          {/if}
        </form>
      </div>
    </div>
  </div>
{/if}
