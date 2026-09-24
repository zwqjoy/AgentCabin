<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import { getContext, onMount, untrack } from "svelte";
  import * as api from "$lib/api";
  import {
    canCloneWorkSession,
    canOpenWorkSessionTree,
    getWorkAgentDisplayName,
    getWorkModels,
    getWorkSlashCommands,
    getWorkStartingLabel,
    isLegacyWorkRun,
    isMissingPiWorkSessionError,
    isPiWorkRuntimeConfirmation,
    loadWorkModels,
    loadWorkModelsLive,
    loadWorkPreferences,
    normalizeWorkQueuedText,
    persistWorkEffort,
    type WorkRuntimePreferences,
  } from "$lib/work/pi-work-runtime";
  import ConversationMessage from "$lib/components/ConversationMessage.svelte";
  import AssistantTurnHeader from "$lib/components/AssistantTurnHeader.svelte";
  import type { SelectedExpert } from "$lib/components/WorkBuddyCascadingMenu.svelte";
  import {
    parseExpertFromText,
    isExpertSkill,
    type ActiveExpertContext,
  } from "$lib/utils/expert-context";
  import SessionStatusBar from "$lib/components/SessionStatusBar.svelte";
  import WorkToolCall from "$lib/components/work/WorkToolCall.svelte";
  import WorkToolCallGroup from "$lib/components/work/WorkToolCallGroup.svelte";
  import ElicitationDialog from "$lib/components/ElicitationDialog.svelte";
  import WorkRuntimeSpecificSurface from "$lib/components/work/WorkRuntimeSpecificSurface.svelte";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";
  import ConversationMarkdown from "$lib/components/ConversationMarkdown.svelte";
  import PermissionPanel from "$lib/components/PermissionPanel.svelte";
  import PromptInput from "$lib/components/PromptInput.svelte";
  import TodoPanel from "$lib/components/TodoPanel.svelte";
  import ChatSearchToolbar from "$lib/components/ChatSearchToolbar.svelte";
  import ConversationTurnRail from "$lib/components/ConversationTurnRail.svelte";
  import ChatReasoningBlock from "$lib/components/chat/ChatReasoningBlock.svelte";
  import ChatAssistantMessage from "$lib/components/chat/ChatAssistantMessage.svelte";
  import ChatProcessStream from "$lib/components/chat/ChatProcessStream.svelte";
  import ChatSessionStatsLine from "$lib/components/chat/ChatSessionStatsLine.svelte";
  import ToBottomButton from "$lib/components/chat/ToBottomButton.svelte";
  import DetailsDrawer from "$lib/components/chat/DetailsDrawer.svelte";
  import ContextInjectionRow from "$lib/components/chat/ContextInjectionRow.svelte";
  import ChatInteractionBlock from "$lib/components/chat/ChatInteractionBlock.svelte";
  import WorkPendingActionItem from "$lib/components/work/WorkPendingActionItem.svelte";
  import WorkInlineInteraction from "$lib/components/work/WorkInlineInteraction.svelte";
  import {
    buildChatPresentationTurns,
    type ChatPresentationTurn,
    type ChatInteractionItem,
    extractLatestThinkingLine,
  } from "$lib/utils/chat-presentation";
  import { isInteractionTool } from "$lib/utils/tool-activity-adapter";
  import type { BusToolItem } from "$lib/types";
  import {
    assignSessionWorkspace,
    continueWorkSession,
    getRunEffectiveCapabilities,
    getWorkArtifactAcceptance,
    listWorkResources,
    listWorkSessions,
  } from "$lib/api/work";
  import * as workResources from "$lib/work/work-resource-service";
  import { workspaceScope, standaloneScope } from "$lib/work/work-scope";
  import { inboxStore } from "$lib/stores/inbox-store.svelte";
  import {
    filterCurrentRunInteractions,
    isQuestionInteraction,
  } from "$lib/utils/work-interactions";
  import type {
    Attachment,
    CliCommand,
    CliModelInfo,
    PermissionSuggestion,
    ScreenshotPayload,
    SessionInfoData,
    TaskRun,
    UserSettings,
  } from "$lib/types";
  import { resolveManagedProviderDefaultModel } from "$lib/utils/provider-routing";
  import { ALL_RUNTIME_PROVIDERS, CONVERSATION_ASSISTANT_NAME } from "$lib/utils/agent-metadata";
  import type {
    InboxItem,
    InboxItemStatus,
    RunEffectiveCapabilitiesView,
    WorkArtifactAcceptance,
    WorkArtifactSummary,
    WorkProfile,
    WorkProgressSnapshot,
    WorkResultPresentation,
    WorkResourceSummary,
    WorkRunProgressView,
    WorkTask,
    WorkWorkspaceSummary,
  } from "$lib/types/work";
  import { WorkSessionStore } from "$lib/stores/work-session-store.svelte";
  import { KeybindingStore } from "$lib/stores/keybindings.svelte";
  import { getTransport } from "$lib/transport";
  import { workProjectionStore } from "$lib/stores/work-projection-store.svelte";
  import { workTaskStore } from "$lib/stores/work-task-store.svelte";
  import { WORK_PRESETS, type WorkExecutionMode, type WorkPreset } from "$lib/types/work";
  import { isNearChatBottom } from "$lib/utils/chat-scroll";
  import { trapFocus } from "$lib/utils/focus-trap";
  import { withTimeout } from "$lib/utils/async-utils";
  import { resolveContextWindow } from "$lib/utils/context-window";
  import {
    isToolChatterAssistant,
    formatCodexDuration,
    getToolGroupSemanticSummary,
  } from "$lib/utils/tool-rendering";
  import { classifyWorkExecutionFailure } from "$lib/utils/work-execution-failure";
  import { canResumeWorkSession } from "$lib/utils/work-resume";
  import {
    findTurnAssistantHeaderIndex,
    shouldRenderLiveAssistantTurnHeader,
  } from "$lib/utils/work-chat-activity";
  import ArtifactPreviewModal from "./ArtifactPreviewModal.svelte";
  import WorkResultCard from "./WorkResultCard.svelte";
  import WorkDeliverySummaryBar from "./WorkDeliverySummaryBar.svelte";
  import CapabilityRunInspector from "../capabilities/CapabilityRunInspector.svelte";
  import WorkArtifactAcceptancePanel from "./WorkArtifactAcceptancePanel.svelte";
  import WorkArtifactProvenanceDrawer from "./WorkArtifactProvenanceDrawer.svelte";
  import {
    deriveWorkResultPresentation,
    hasOperationalWorkEvidence,
    hasWorkCompletionEvidence,
  } from "$lib/utils/work-result";
  import {
    deriveWorkProgressPhase,
    mapProgressViewToStatusLabel,
    workRunProgressToSnapshot,
  } from "$lib/utils/work-progress";
  import { RUNS_CHANGED_EVENT, type RunMutation } from "$lib/utils/run-mutations";
  import { t } from "$lib/i18n/index.svelte";
  import { WORK_STARTERS } from "$lib/data/work-starters";

  const WORK_SESSION_LOOKUP_TIMEOUT_MS = 5_000;

  interface Props {
    workspace?: WorkWorkspaceSummary;
    profile: WorkProfile | null;
    runId?: string | null;
    newConversation?: boolean;
    sessionInfo?: SessionInfoData | null;
    progress?: WorkProgressSnapshot | null;
    progressView?: WorkRunProgressView | null;
    artifacts?: WorkArtifactSummary[];
    onArtifactsChanged?: () => Promise<void>;
    onWorkspaceChanged?: (ws: WorkWorkspaceSummary) => void;
    statusLabel?: string;
    sessionAlive?: boolean;
    conversationArchived?: boolean;
    pendingInteractions?: InboxItem[];
    workspaces?: WorkWorkspaceSummary[];
    onCreateWorkspace?: () => void;
    onOpenFolderWorkspace?: () => void;
    onSelectWorkspace?: (wsId: string | null) => void;
    onStopSession?: () => void;
    onToggleInspector?: () => void;
    inspectorOpen?: boolean;
    onExportArtifact?: (artifactId: string) => Promise<void>;
  }

  let {
    workspace,
    profile,
    runId = null,
    newConversation = false,
    sessionInfo = $bindable(null),
    progress = $bindable<WorkProgressSnapshot | null>(null),
    progressView = $bindable<WorkRunProgressView | null>(null),
    artifacts = [],
    workspaces = [],
    onCreateWorkspace,
    onOpenFolderWorkspace,
    onSelectWorkspace,
    onArtifactsChanged,
    onWorkspaceChanged,
    statusLabel = $bindable(""),
    sessionAlive = $bindable(false),
    conversationArchived = $bindable(false),
    pendingInteractions = $bindable<InboxItem[]>([]),
    onStopSession = $bindable(() => {}),
    onToggleInspector,
    inspectorOpen = false,
    onExportArtifact,
  }: Props = $props();

  const toggleAppSidebar = getContext<(() => void) | undefined>("toggleSidebar");
  const getAppSidebarOpen = getContext<(() => boolean) | undefined>("isSidebarOpen");
  let appSidebarOpen = $derived(getAppSidebarOpen?.() ?? true);

  let isStandalone = $derived(!workspace);

  const workSession = new WorkSessionStore();
  const keybindingStore = getContext<KeybindingStore>("keybindings");
  let promptRef: PromptInput | undefined = $state();
  let statusBarRef: SessionStatusBar | undefined = $state();
  let booting = $state(true);
  let disposed = false;
  let transcript = $state<HTMLDivElement>();
  let loadedKey = $state("");
  let previousRunId = $state<string | null>(null);
  let isAutoScroll = $state(true);
  let showScrollHint = $state(false);
  let continuationBusy = $state(false);
  let assignModalOpen = $state(false);
  let assigningWorkspace = $state(false);
  let assignTargetWsId = $state("");
  let assignError = $state("");
  let assignDialog = $state<HTMLDivElement>();
  let historySearchOpen = $state(false);
  let searchToolbarRef = $state<ReturnType<typeof ChatSearchToolbar>>();
  let inspectorOpenedForRun = $state("");
  let selectedExpert = $state<SelectedExpert | null>(null);
  let userDismissedExpert = $state(false);
  let lastRestoredRunId = $state<string | null | undefined>(undefined);

  $effect(() => {
    const currentRun = runId || "new";
    if (lastRestoredRunId !== currentRun) {
      lastRestoredRunId = currentRun;
      userDismissedExpert = false;
    }
    if (userDismissedExpert) return;
    if (untrack(() => selectedExpert)) return;

    for (let i = visibleTimeline.length - 1; i >= 0; i--) {
      const entry = visibleTimeline[i];
      if (entry.kind === "user") {
        const parsed = parseExpertFromText(entry.content).expert;
        if (parsed) {
          selectedExpert = {
            id: parsed.id || parsed.title,
            name: parsed.id || parsed.title,
            title: parsed.title,
            isTeam: parsed.isTeam,
            icon: parsed.avatarChar || (parsed.title || "专").slice(0, 1),
          };
          break;
        }
      }
    }
  });

  $effect(() => {
    if (selectedExpert) {
      userDismissedExpert = false;
    }
  });

  function handleExpertClear() {
    userDismissedExpert = true;
    selectedExpert = null;
  }

  $effect(() => {
    if (!assignModalOpen || !assignDialog) return;
    requestAnimationFrame(() => {
      assignDialog
        ?.querySelector<HTMLElement>(
          "select:not([disabled]), button:not([disabled]), input:not([disabled])",
        )
        ?.focus();
    });
  });

  async function handleAssignToWorkspace() {
    if (!session.run?.id || !assignTargetWsId || assigningWorkspace || standaloneAssignmentBlocked)
      return;
    assigningWorkspace = true;
    assignError = "";
    try {
      await assignSessionWorkspace(session.run.id, assignTargetWsId);
      assignModalOpen = false;
      void goto(
        `/chat/work?workspace=${encodeURIComponent(assignTargetWsId)}&run=${encodeURIComponent(session.run.id)}`,
      );
    } catch (err) {
      assignError = err instanceof Error ? err.message : String(err);
    } finally {
      assigningWorkspace = false;
    }
  }

  function applyTemplateToPrompt(promptText: string) {
    promptRef?.setValue(promptText);
    requestAnimationFrame(() => promptRef?.focus());
  }

  const session = workSession.session;

  function parseWorkPreset(value: string | null): WorkPreset {
    return WORK_PRESETS.some((preset) => preset.id === value) ? (value as WorkPreset) : "office";
  }

  // Presets describe the user's work intent; the underlying Work Harness and
  // shared Browser/Web Runtime remain unchanged. A loaded run owns its preset
  // so reopening a conversation cannot silently switch its behavior.
  let selectedWorkPreset = $state<WorkPreset>(
    parseWorkPreset($page.url.searchParams.get("preset")),
  );
  let activeWorkPreset = $derived(session.run?.work_preset ?? selectedWorkPreset);
  let standaloneAssignmentBlocked = $derived(
    session.run?.status === "pending" || session.run?.status === "running",
  );

  // ── Permission mode (execution_mode) ──────────────────────────────────────
  // The task's execution_mode is the conversation-level override; the
  // workspace default_policy.execution_mode is both the new-task default and
  // the autonomy ceiling. Before any task/run exists, the selector edits the
  // workspace default so the mode can be chosen up front (pre-conversation).
  let activeTask = $state<WorkTask | null>(null);

  $effect(() => {
    const taskId = session.run?.work_task_id;
    if (isStandalone || !taskId) {
      activeTask = null;
      return;
    }

    const cached = workTaskStore.getTaskById(taskId);
    if (cached) {
      activeTask = cached;
      return;
    }

    // Keep the first live turn on the same path as a standalone Work task.
    // Workspace task metadata is durable bookkeeping, not a prerequisite for
    // rendering the session transcript; load it after the turn is idle.
    if (session.isRunning) {
      activeTask = null;
      return;
    }

    let current = true;
    void workTaskStore
      .fetchTask(taskId)
      .then((task) => {
        if (current && session.run?.work_task_id === taskId) activeTask = task;
      })
      .catch((cause) => {
        if (current) console.error("Failed to load active Work task:", cause);
      });
    return () => {
      current = false;
    };
  });

  // A task is only editable once its run is live; before that we fall back to
  // the workspace default. Read session.run directly (not hasRun, declared
  // further below) to keep this derived self-contained.
  let hasActiveTask = $derived(Boolean(activeTask && session.run?.id));

  // Standalone task execution mode state (defaults to auto for quick tasks)
  let standaloneExecutionMode = $state<WorkExecutionMode>("auto");

  const CLI_PERMISSION_MODE_BY_WORK_MODE: Record<WorkExecutionMode, string> = {
    auto: "auto",
    direct: "default",
    plan_first: "plan",
    full_access: "bypassPermissions",
  };

  let currentExecutionMode = $derived.by<WorkExecutionMode>(() => {
    if (isStandalone) {
      const pm = session.run?.permission_mode;
      if (pm) {
        if (pm === "plan" || pm === "plan_first") return "plan_first";
        if (pm === "ask" || pm === "direct" || pm === "default") return "direct";
        if (pm === "auto" || pm === "auto_all") return "auto";
        if (
          pm === "full_access" ||
          pm === "fullAccess" ||
          pm === "bypass" ||
          pm === "bypassPermissions"
        ) {
          return "full_access";
        }
      }
      return standaloneExecutionMode;
    }
    return workspace?.defaultPolicy?.executionMode ?? activeTask?.policy.executionMode ?? "auto";
  });

  function visibleExecutionMode(mode: WorkExecutionMode): WorkExecutionMode {
    return mode === "direct" || mode === "plan_first" ? "auto" : mode;
  }

  let permissionModeBusy = $state(false);

  async function setExecutionMode(mode: WorkExecutionMode) {
    if (permissionModeBusy || mode === currentExecutionMode) return;
    permissionModeBusy = true;
    try {
      if (isStandalone) {
        standaloneExecutionMode = mode;
        if (session.run?.id) {
          const cliPermMode = CLI_PERMISSION_MODE_BY_WORK_MODE[mode];
          await api.updateRunPermissionMode(session.run.id, cliPermMode);
          if (session.run) {
            session.run = { ...session.run, permission_mode: cliPermMode };
          }
        }
        showChatToast(executionModeChangeNotice(mode));
        return;
      }
      if (workspace) {
        const updated = await workTaskStore.updateWorkspaceDefaultPolicy(workspace.id, {
          ...workspace.defaultPolicy,
          executionMode: mode,
        });
        onWorkspaceChanged?.(updated);
      }
      if (hasActiveTask && activeTask) {
        const updatedTask = {
          ...activeTask,
          policy: { ...activeTask.policy, executionMode: mode },
        };
        await workTaskStore.updateTask(updatedTask);
        activeTask = updatedTask;
      }
      if (session.run?.id) {
        const cliPermMode = CLI_PERMISSION_MODE_BY_WORK_MODE[mode];
        await api.updateRunPermissionMode(session.run.id, cliPermMode);
        if (session.run) {
          session.run = { ...session.run, permission_mode: cliPermMode };
        }
      }
      showChatToast(executionModeChangeNotice(mode));
    } catch (cause) {
      console.error("Failed to update execution mode:", cause);
      showChatToast("权限模式切换失败，请稍后重试。", "error");
    } finally {
      permissionModeBusy = false;
    }
  }

  let WORK_MODE_OPTIONS = $derived.by(
    (): Array<{
      value: WorkExecutionMode;
      label: string;
      description: string;
      cls: string;
    }> => [
      {
        value: "auto",
        label: t("work_permissionAutoLabel"),
        description: t("work_permissionAutoDescription"),
        cls: "text-amber-600 dark:text-amber-400",
      },
      {
        value: "full_access",
        label: t("work_permissionFullAccessLabel"),
        description: t("work_permissionFullAccessDescription"),
        cls: "text-red-600 dark:text-red-400",
      },
    ],
  );

  function executionModeLabel(mode: WorkExecutionMode): string {
    return (
      WORK_MODE_OPTIONS.find((option) => option.value === visibleExecutionMode(mode))?.label ?? mode
    );
  }

  function hasPendingPermissionNow(): boolean {
    return (
      session.hasPendingPermission ||
      filterCurrentRunInteractions(inboxStore.items, session.run, true).length > 0
    );
  }

  function executionModeChangeNotice(mode: WorkExecutionMode): string {
    const label = executionModeLabel(mode);
    if (!isStandalone) {
      return `已将工作空间权限切换为${label}。`;
    }
    if (!session.run?.id) {
      return `已设置为${label}，首次发送消息时生效。`;
    }

    if (mode === "full_access") {
      return hasPendingPermissionNow()
        ? "已切换为完全访问；当前待处理的审批不会自动通过，后续文件读写和命令执行将不再受 Work 审批与路径沙箱限制。"
        : "已切换为完全访问；后续文件读写和命令执行将不再受 Work 审批与路径沙箱限制，请谨慎使用。";
    }

    if (hasPendingPermissionNow()) {
      return `已切换为${label}；当前待处理的审批不会自动通过，后续新发起的操作按${label}执行。`;
    }

    return `已切换为${label}；后续尚未发起的操作立即按此模式执行。`;
  }

  let workspacePickerConfig = $derived.by(() => {
    return {
      currentWorkspaceId: workspace?.id ?? null,
      currentWorkspaceName: workspace?.name || "选择工作空间",
      workspaces: workspaces.map((ws) => ({ id: ws.id, name: ws.name })),
      onSelect: (wsId: string | null) => {
        if (onSelectWorkspace) {
          onSelectWorkspace(wsId);
        } else if (wsId) {
          void goto(`/chat/work?workspace=${encodeURIComponent(wsId)}&newSession=1`);
        } else {
          void goto(`/chat/work?newSession=1`);
        }
      },
      onCreateWorkspace,
      onOpenFolderWorkspace,
      // 对话已开始后只读显示当前工作空间：输入文件与成果已绑定 workspace，
      // 中途切换会导致数据归属错乱，需回到首页或新会话再切换。
      readOnly: hasRun,
    };
  });

  let workPermissionPicker = $derived.by(() => {
    if (!workspace && !isStandalone) return null;
    return {
      value: visibleExecutionMode(currentExecutionMode),
      busy: permissionModeBusy,
      title: isStandalone
        ? session.run?.id
          ? t("work_permissionModeNextActionsTitle")
          : t("work_permissionModeFirstSendTitle")
        : t("work_permissionModeWorkspaceTitle"),
      options: WORK_MODE_OPTIONS.map((opt) => ({
        ...opt,
      })),
      onSelect: (value: string) => void setExecutionMode(value as WorkExecutionMode),
    };
  });

  let currentPendingInteractions = $derived.by(() => {
    return filterCurrentRunInteractions(inboxStore.items, session.run, true);
  });
  let currentAllInteractions = $derived.by(() => {
    return filterCurrentRunInteractions(inboxStore.items, session.run, false);
  });

  let unmirroredPendingTools = $derived(
    session.pendingToolPermissions.filter(
      ({ tool, requestId }) =>
        !currentPendingInteractions.some(
          (item) =>
            item.payload.requestId === requestId || item.payload.toolUseId === tool.tool_use_id,
        ),
    ),
  );
  // Work pipeline interactions remain backed by durable Inbox items. Questions
  // render inline in the originating task; the generic runtime prompt remains
  // only as a compatibility fallback when no durable Inbox item exists.
  let unmirroredElicitations = $derived.by(() => {
    const inboxRequestIds = new Set(
      currentPendingInteractions
        .map((item) => item.payload.requestId)
        .filter((requestId): requestId is string => Boolean(requestId)),
    );
    return new Map(
      [...session.pendingElicitations].filter(([requestId]) => !inboxRequestIds.has(requestId)),
    );
  });

  function findPendingInteractionForTool(toolEntry: ToolEntry): InboxItem | null {
    return (
      currentPendingInteractions.find(
        (item) =>
          (item.payload.toolCallId &&
            (item.payload.toolCallId === toolEntry.tool.tool_use_id ||
              item.payload.toolCallId === toolEntry.id)) ||
          (item.payload.toolUseId && item.payload.toolUseId === toolEntry.tool.tool_use_id) ||
          (item.payload.requestId && item.payload.requestId === toolEntry.id) ||
          (toolEntry.tool.tool_name === "ask_questions" && isQuestionInteraction(item)) ||
          (toolEntry.tool.tool_name === "work_request_directory_access" &&
            (item.itemType === "access_root_request" ||
              item.payload.toolName === "work_request_directory_access")),
      ) ?? null
    );
  }

  function findResolvedInteractionForTool(toolEntry: ToolEntry): InboxItem | null {
    return (
      currentAllInteractions.find(
        (item) =>
          item.status !== "pending" &&
          ((item.payload.toolCallId &&
            (item.payload.toolCallId === toolEntry.tool.tool_use_id ||
              item.payload.toolCallId === toolEntry.id)) ||
            (item.payload.toolUseId && item.payload.toolUseId === toolEntry.tool.tool_use_id) ||
            (item.payload.requestId && item.payload.requestId === toolEntry.id) ||
            (toolEntry.tool.tool_name === "ask_questions" && isQuestionInteraction(item)) ||
            (toolEntry.tool.tool_name === "work_request_directory_access" &&
              (item.itemType === "access_root_request" ||
                item.payload.toolName === "work_request_directory_access"))),
      ) ?? null
    );
  }

  async function resolveInlineInteraction(
    item: InboxItem,
    status: InboxItemStatus,
    response?: unknown,
  ): Promise<void> {
    await inboxStore.resolve(item.id, status, response);
  }
  // Work capabilities are global to the Work runtime. The conversation surface
  // reads them for the pre-session skill picker; it never stores them on a Workspace.
  let globalResources = $state<WorkResourceSummary[]>([]);
  let loadedResourceRuntime = $state("");
  let userSettings = $state<UserSettings | null>(null);
  let effectiveWorkAgent = $derived(session.run?.agent ?? session.agent ?? "pi");
  let isLegacyRun = $derived(isLegacyWorkRun(session.run));
  let runtimeClientError = $derived(
    isLegacyRun
      ? `当前 Work 会话属于已退役的 Runtime (${getWorkAgentDisplayName(session.run?.agent)})`
      : null,
  );
  let activeProviderBinding = $derived.by(() => {
    return userSettings?.agent_provider_bindings?.[
      effectiveWorkAgent as "claude" | "codex" | "pi" | "grok" | "dsh"
    ];
  });
  let activeGlobalProvider = $derived.by(() => {
    if (activeProviderBinding?.mode !== "custom" || !activeProviderBinding.provider_id) {
      return undefined;
    }
    return userSettings?.global_providers?.find(
      (item) => item.id === activeProviderBinding?.provider_id,
    );
  });
  let managedProviderDefaultModel = $derived.by(() => {
    if (activeProviderBinding?.mode !== "custom") return "";
    return resolveManagedProviderDefaultModel(
      activeProviderBinding,
      activeGlobalProvider?.models?.map((item) => item.id) ?? [],
    );
  });
  let globalProviderModelOptions = $derived.by((): CliModelInfo[] => {
    const list: CliModelInfo[] = [];
    for (const provider of userSettings?.global_providers ?? []) {
      for (const m of provider.models ?? []) {
        list.push({
          value: `${provider.id}/${m.id}`,
          displayName: m.name?.trim() || m.id,
          description: provider.name,
          providerId: provider.id,
          providerName: provider.name,
          contextWindow: m.context_window,
          supportsEffort: m.supports_reasoning,
          supportedEffortLevels: m.supported_effort_levels,
        });
      }
    }
    return list;
  });

  let modelCatalog = $derived.by((): CliModelInfo[] => {
    const nativeModels =
      session.run?.id && session.sessionModels.length > 0 ? session.sessionModels : getWorkModels();
    // When global providers are configured, use them exclusively.
    // Native models are a fallback for when no custom providers exist.
    if (globalProviderModelOptions.length > 0) {
      return globalProviderModelOptions;
    }
    return nativeModels;
  });

  let modelOptions = $derived.by((): CliModelInfo[] => {
    const catalog = modelCatalog;
    const currentModel = session.model.trim();
    if (!currentModel) return catalog;

    const getCatalogBareId = (m: CliModelInfo) => {
      if (m.providerId && m.value.startsWith(`${m.providerId}/`)) {
        return m.value.slice(m.providerId.length + 1);
      }
      const s = m.value.indexOf("/");
      return s >= 0 ? m.value.slice(s + 1) : m.value;
    };

    // Exact match: model is already in catalog
    if (catalog.some((model) => model.value === currentModel)) return catalog;

    // Bare/partial model name: check if catalog has a matching entry
    const matchingCatalogItem = catalog.find((m) => {
      return getCatalogBareId(m) === currentModel || m.displayName === currentModel;
    });
    if (matchingCatalogItem) return catalog;

    // No match at all: prepend a fallback entry for the running session model.
    // Only extract provider if the prefix before '/' is actually a known provider ID or name.
    const slash = currentModel.indexOf("/");
    let fallbackProviderName: string | undefined;
    let fallbackDisplayName = currentModel;
    if (slash >= 0) {
      const candidatePrefix = currentModel.slice(0, slash);
      const matchedProvider = userSettings?.global_providers?.find(
        (p) => p.id === candidatePrefix || p.name === candidatePrefix,
      );
      if (matchedProvider) {
        fallbackProviderName = matchedProvider.name || matchedProvider.id;
        fallbackDisplayName = currentModel.slice(slash + 1);
      }
    }
    return [
      {
        value: currentModel,
        displayName: fallbackDisplayName,
        providerName: fallbackProviderName,
        description: "当前会话",
        supportsEffort: false,
      },
      ...catalog,
    ];
  });

  // Reconcile a bare/partial model string to the canonical catalog model value.
  $effect(() => {
    const cur = session.model.trim();
    if (!cur || modelOptions.length === 0) return;
    if (modelOptions.some((m) => m.value === cur)) return;
    const match = modelOptions.find((m) => {
      const bareId =
        m.providerId && m.value.startsWith(`${m.providerId}/`)
          ? m.value.slice(m.providerId.length + 1)
          : m.value.includes("/")
            ? m.value.slice(m.value.indexOf("/") + 1)
            : m.value;
      return bareId === cur || m.displayName === cur;
    });
    if (match && match.value !== cur) {
      session.model = match.value;
      if (session.run) {
        session.run = { ...session.run, model: match.value };
      }
    }
  });
  let currentEffort = $state("");
  let chatToast = $state<string | null>(null);
  let chatToastTone = $state<"neutral" | "error">("neutral");
  let chatToastTimeout: ReturnType<typeof setTimeout> | null = null;

  function showChatToast(msg: string, tone: "neutral" | "error" = "neutral") {
    chatToast = msg;
    chatToastTone = tone;
    if (chatToastTimeout) clearTimeout(chatToastTimeout);
    chatToastTimeout = setTimeout(() => {
      chatToast = null;
    }, 2500);
  }

  let workspaceModelPreferenceWrite: Promise<void> = Promise.resolve();
  let workspaceModelPreferenceDraft: {
    workspaceId: string;
    model?: string;
    effort?: string;
  } = { workspaceId: "" };

  function persistWorkspaceModelPreference(patch: {
    model?: string;
    effort?: string;
  }): Promise<void> {
    if (!workspace) return Promise.resolve();
    const workspaceId = workspace.id;
    if (workspaceModelPreferenceDraft.workspaceId !== workspaceId) {
      workspaceModelPreferenceDraft = {
        workspaceId,
        model: workspace.defaultModel,
        effort: workspace.defaultEffort,
      };
    }
    workspaceModelPreferenceDraft = { ...workspaceModelPreferenceDraft, ...patch };
    const nextModel = workspaceModelPreferenceDraft.model;
    const nextEffort = workspaceModelPreferenceDraft.effort;

    workspaceModelPreferenceWrite = workspaceModelPreferenceWrite
      .catch(() => {})
      .then(async () => {
        try {
          const updated = await workTaskStore.updateWorkspaceModelPreferences(
            workspaceId,
            nextModel,
            nextEffort,
          );
          onWorkspaceChanged?.(updated);
        } catch (cause) {
          workSession.error = cause instanceof Error ? cause.message : String(cause);
          throw cause;
        }
      });

    return workspaceModelPreferenceWrite;
  }

  let archivedConversation = $derived(Boolean(session.run?.archived));
  let runtimeReadOnly = $derived(Boolean(runtimeClientError));
  let conversationReadOnly = $derived(archivedConversation || runtimeReadOnly);
  let canSend = $derived(Boolean(session.run?.id && session.sessionAlive) && !conversationReadOnly);
  let isInteractionBlocked = $derived(
    conversationReadOnly || session.hasPendingPermission || session.hasElicitation,
  );
  let hasRun = $derived(Boolean(session.run?.id));
  let canResume = $derived(
    canResumeWorkSession(session.run, session.sessionAlive, !isLegacyRun, conversationReadOnly),
  );
  let canContinueWork = $derived(
    Boolean(
      session.run?.id &&
      !continuationBusy &&
      ["idle", "completed", "stopped", "failed"].includes(session.run.status) &&
      !session.isRunning &&
      !session.hasPendingPermission &&
      !session.hasElicitation &&
      !conversationReadOnly,
    ),
  );
  let workSlashCommands = $derived(getWorkSlashCommands(session));
  let skillItems = $derived.by((): Array<{ name: string; description: string }> => {
    const byName = new Map<string, { name: string; description: string }>();

    for (const resource of globalResources) {
      if (resource.kind !== "skill" || !resource.enabled || !resource.active) continue;
      if (isExpertSkill(resource)) continue;
      const name = resource.name || resource.id;
      byName.set(name, {
        name,
        description: resource.description || "Work 全局技能",
      });
    }

    for (const command of session.sessionCommands as CliCommand[]) {
      if (!command.name.toLowerCase().startsWith("skill:")) continue;
      const name = command.name.slice("skill:".length).trim();
      if (!name || byName.has(name)) continue;
      if (isExpertSkill({ name, description: command.description })) continue;
      byName.set(name, {
        name,
        description: command.description || "当前运行时技能",
      });
    }

    return Array.from(byName.values()).sort((left, right) => left.name.localeCompare(right.name));
  });
  let workComposerCapabilities = $derived(workSession.composerCapabilities);
  let visibleTimeline = $derived(workSession.visibleTimeline);
  function focusHistorySearch() {
    historySearchOpen = true;
    requestAnimationFrame(() => {
      searchToolbarRef?.focus();
    });
  }

  function toggleHistorySearch() {
    if (historySearchOpen) {
      historySearchOpen = false;
      return;
    }
    focusHistorySearch();
  }

  function scrollToHistoryEntry(entryId: string) {
    const node = Array.from(
      transcript?.querySelectorAll<HTMLElement>("[data-work-timeline-id]") ?? [],
    ).find((element) => element.dataset.workTimelineId === entryId);
    node?.scrollIntoView({ behavior: "smooth", block: "center" });
  }

  function expandAllIntermediateTurns() {
    const next: Record<string, boolean> = { ...expandedIntermediateTurns };
    for (const turn of presentationTurns) {
      if (turn.processBlocks.length > 0) next[turn.id] = true;
    }
    expandedIntermediateTurns = next;
  }

  function collapseAllIntermediateTurns() {
    const next: Record<string, boolean> = { ...expandedIntermediateTurns };
    for (const turn of presentationTurns) {
      if (turn.processBlocks.length > 0) next[turn.id] = false;
    }
    expandedIntermediateTurns = next;
  }
  let turnHeaderIndices = $derived.by(() => {
    const headers = new Set<number>();
    let awaitingAssistant = false;
    for (let index = 0; index < visibleTimeline.length; index += 1) {
      const entry = visibleTimeline[index];
      if (entry.kind === "user") {
        awaitingAssistant = true;
        continue;
      }
      const startsAssistantTurn =
        entry.kind === "tool" ||
        entry.kind === "command_output" ||
        (entry.kind === "assistant" && !isToolChatterAssistant(entry));
      if (awaitingAssistant && startsAssistantTurn) {
        headers.add(index);
        awaitingAssistant = false;
      }
    }
    return headers;
  });
  let latestUserIndex = $derived.by(() => {
    for (let index = visibleTimeline.length - 1; index >= 0; index -= 1) {
      if (visibleTimeline[index]?.kind === "user") return index;
    }
    return -1;
  });
  let hasLatestAssistantTurnHeader = $derived.by(() => {
    if (latestUserIndex < 0) return false;
    const latestTurn = presentationTurns[presentationTurns.length - 1];
    if (latestTurn && (latestTurn.processBlocks.length > 0 || latestTurn.finalMessage)) {
      return true;
    }
    for (const index of turnHeaderIndices) {
      if (index > latestUserIndex) return true;
    }
    return false;
  });
  let showLiveAssistantTurnHeader = $derived(
    shouldRenderLiveAssistantTurnHeader({
      isRunning: session.isRunning,
      hasLatestAssistantTurnHeader,
      hasThinkingText: Boolean(session.thinkingText),
      hasStreamingText: Boolean(session.streamingText),
    }),
  );

  function isToolChatterAt(index: number): boolean {
    const entry = visibleTimeline[index];
    if (!entry || !isToolChatterAssistant(entry)) return false;
    return (
      visibleTimeline[index - 1]?.kind === "tool" || visibleTimeline[index + 1]?.kind === "tool"
    );
  }

  type ToolEntry = Extract<(typeof visibleTimeline)[number], { kind: "tool" }>;
  let toolGroups = $derived.by(() => {
    const groups = new Map<number, ToolEntry[]>();
    const hidden = new Set<number>();
    let start = -1;
    let current: ToolEntry[] = [];
    let currentIndices: number[] = [];
    const flush = () => {
      if (start < 0 || current.length === 0) return;
      groups.set(start, current);
      for (const index of currentIndices.slice(1)) hidden.add(index);
      start = -1;
      current = [];
      currentIndices = [];
    };
    visibleTimeline.forEach((entry, index) => {
      if (entry.kind === "tool") {
        const isInteractionTool =
          entry.tool.tool_name === "work_request_directory_access" ||
          Boolean(findPendingInteractionForTool(entry)) ||
          Boolean(findResolvedInteractionForTool(entry));
        if (isInteractionTool) {
          flush();
          groups.set(index, [entry]);
          return;
        }
        if (start < 0) start = index;
        current.push(entry);
        currentIndices.push(index);
      } else if (start >= 0 && isToolChatterAt(index)) {
        // Tool-adjacent assistant filler is hidden from the transcript and
        // should not split one user-facing execution group into many cards.
        return;
      } else {
        flush();
      }
    });
    flush();
    return { groups, hidden };
  });
  let toolGroupStarts = $derived(toolGroups.groups);
  let toolGroupHiddenIndices = $derived(toolGroups.hidden);
  const artifactToolNames = new Set([
    "work_write_file",
    "work_register_artifact",
    "work_validate_artifact",
    "work_deliver",
  ]);
  let lastArtifactRefreshKey = "";
  $effect(() => {
    let latest: ToolEntry | undefined;
    for (let index = visibleTimeline.length - 1; index >= 0; index -= 1) {
      const candidate = visibleTimeline[index];
      if (candidate.kind === "tool" && artifactToolNames.has(candidate.tool.tool_name)) {
        latest = candidate;
        break;
      }
    }
    if (!latest || !["success", "error"].includes(latest.tool.status)) {
      return;
    }
    const key = `${latest.id}:${latest.tool.status}:${latest.tool.output ? "output" : "empty"}`;
    if (key === lastArtifactRefreshKey) return;
    lastArtifactRefreshKey = key;
    void onArtifactsChanged?.();
  });

  function formatWorkedDuration(ms: number): string {
    return formatCodexDuration(ms);
  }

  /** 为回合生成动作摘要：对齐 Codex 语义文案 */
  function formatTurnActionSummary(
    intermediateIndices: number[],
    trailingIndices: number[],
  ): string {
    const allIndices = [...intermediateIndices, ...trailingIndices];
    const toolEntries = allIndices
      .map((idx) => visibleTimeline[idx])
      .filter((entry): entry is Extract<(typeof visibleTimeline)[number], { kind: "tool" }> =>
        Boolean(entry && entry.kind === "tool"),
      );
    if (toolEntries.length === 0) return "";
    const summary = getToolGroupSemanticSummary(toolEntries.map((e) => e.tool));
    return summary.label;
  }

  /** 工具名 → 简短中文动作描述（仅收录常见 work 工具，其余原样显示）。 */
  function formatToolNameForSummary(toolName: string): string {
    const labels: Record<string, string> = {
      work_read_file: "读取文件",
      work_write_file: "写入文件",
      work_edit_file: "编辑文件",
      work_list_files: "列出文件",
      work_execute: "执行能力",
      work_run_command: "运行命令",
      work_register_artifact: "注册成果",
      work_validate_artifact: "验证成果",
      work_deliver: "交付成果",
      work_delegate: "委派子代理",
      work_request_directory_access: "请求目录授权",
    };
    const base = toolName.split("__").pop() || toolName;
    return labels[base] || labels[toolName] || base;
  }

  interface ConversationTurn {
    id: string;
    userIndex: number;
    assistantHeaderIndex: number | null;
    assistantTimestamp?: string;
    intermediateIndices: number[];
    finalAssistantIndex: number | null;
    trailingIndices: number[];
    durationFormatted: string;
    actionSummary: string;
    hasIntermediate: boolean;
    expert: ActiveExpertContext | null;
  }

  let expandedIntermediateTurns = $state<Record<string, boolean>>({});

  let presentationTurns = $derived(
    buildChatPresentationTurns(
      visibleTimeline,
      {
        isRunning: session.isRunning,
        thinkingText: session.thinkingText,
        streamingText: session.streamingText,
        durationMs: session.durationMs,
      },
      expandedIntermediateTurns,
      undefined,
      undefined,
      (tool) => {
        const dummyToolEntry: ToolEntry = {
          id: tool.tool_use_id,
          anchorId: tool.tool_use_id,
          kind: "tool",
          ts: "",
          tool,
        };
        return (
          isInteractionTool(tool) ||
          Boolean(findPendingInteractionForTool(dummyToolEntry)) ||
          Boolean(findResolvedInteractionForTool(dummyToolEntry)) ||
          tool.tool_name === "ask_questions" ||
          tool.tool_name === "work_request_directory_access"
        );
      },
      (tool) => {
        const dummyToolEntry: ToolEntry = {
          id: tool.tool_use_id,
          anchorId: tool.tool_use_id,
          kind: "tool",
          ts: "",
          tool,
        };
        return Boolean(findPendingInteractionForTool(dummyToolEntry));
      },
    ),
  );

  let conversationRailEntries = $derived.by(() => {
    const result: Array<{
      id: string;
      anchorId: string;
      preview: string;
      responsePreview?: string;
      isRunning?: boolean;
      turnIndex: number;
    }> = [];
    for (const turn of presentationTurns) {
      if (!turn.userMessage) continue;
      result.push({
        id: turn.userMessage.id,
        anchorId: turn.userMessage.anchorId,
        preview: turn.userMessage.content,
        responsePreview: turn.finalMessage?.content?.slice(0, 140),
        isRunning: turn.isRunning,
        turnIndex: turn.turnIndex + 1,
      });
    }
    return result;
  });

  let usageByTurn = $derived(new Map(session.turnUsages.map((tu) => [tu.turnIndex, tu])));

  let allIntermediateTurnsExpanded = $derived.by(() => {
    const turnsWithProcess = presentationTurns.filter((turn) => turn.processBlocks.length > 0);
    if (turnsWithProcess.length === 0) return false;
    return turnsWithProcess.every((turn) => !turn.isCollapsed);
  });
  let resolvedContextWindow = $derived.by(() =>
    resolveContextWindow({
      reportedWindow: session.contextWindow,
      model: session.run?.model?.trim() || session.model.trim(),
      models: modelOptions,
      customProvider: true,
    }),
  );
  let internalSessionInfo = $derived.by(() => {
    if (!session.run) return null;
    const contextWindow = resolvedContextWindow.contextWindow;
    const contextTokens =
      session.contextTokens ||
      session.usage.inputTokens + session.usage.cacheReadTokens + session.usage.cacheWriteTokens;
    const contextUtilization =
      session.contextUtilization ||
      (contextWindow > 0 ? Math.min(contextTokens / contextWindow, 1) : 0);
    return {
      sessionId: session.run.session_id,
      runId: session.run.id,
      runName: session.run.name,
      cwd: session.sessionCwd || session.run.cwd,
      numTurns: session.userTurnCount,
      status: session.run.status,
      startedAt: session.run.started_at ?? null,
      endedAt: session.run.ended_at ?? null,
      lastTurnDurationMs: session.durationMs,
      tokensEstimated:
        !session.usage.modelUsage || Object.keys(session.usage.modelUsage).length === 0,
      model: session.run.model ?? session.model,
      agent: session.run.agent ?? "pi",
      cliVersion: session.cliVersion,
      permissionMode: "",
      fastModeState: session.fastModeState,
      cost: session.usage.cost,
      inputTokens: session.usage.inputTokens,
      outputTokens: session.usage.outputTokens,
      cacheReadTokens: session.usage.cacheReadTokens,
      cacheWriteTokens: session.usage.cacheWriteTokens,
      contextWindow,
      contextWindowEstimated: resolvedContextWindow.estimated,
      contextUtilization,
      contextTokens,
      contextBreakdown: session.contextBreakdown,
      compactCount: session.compactCount,
      microcompactCount: session.microcompactCount,
      mcpServers: session.mcpServers,
      remoteHostName: session.remoteHostName,
      platformId: session.platformId,
      cliUsageIncomplete: session.run.cli_usage_incomplete ?? false,
      runSource: session.run.source,
    };
  });
  let hasDirectoryAccessPending = $derived(
    currentPendingInteractions.some(
      (item) =>
        item.itemType === "access_root_request" ||
        item.payload.toolName === "work_request_directory_access",
    ),
  );
  let latestExecutionFailure = $derived.by(() => {
    for (let index = visibleTimeline.length - 1; index >= 0; index -= 1) {
      const entry = visibleTimeline[index];
      if (entry.kind !== "tool") continue;
      const failure = classifyWorkExecutionFailure(entry.tool.status, entry.tool.output);
      if (failure) return failure;
    }
    return null;
  });
  let calculatedStatusLabel = $derived.by(() => {
    if (workSession.starting) return getWorkStartingLabel();
    if (currentPendingInteractions.length > 0) {
      return currentPendingInteractions.some(isQuestionInteraction) ? "等待你的回答" : "等待你处理";
    }
    if (session.hasElicitation) return "等待用户确认";
    if (session.hasPendingPermission) return "等待权限确认";
    if (session.isRunning) return "正在执行";
    if (session.run?.status === "stopped") return "会话已停止";
    if (session.run?.status === "failed") return latestExecutionFailure?.label || "执行失败";
    if (session.run) return "等待下一条消息";
    return "尚未启动会话";
  });
  let calculatedSessionAlive = $derived(Boolean(session.run && session.sessionAlive));
  let progressTasks = $derived.by(() => {
    if (session.workTaskState !== null) return session.workTaskState.plan;
    if (session.structuredTaskState !== null) return session.structuredTaskState;
    return [];
  });
  let pendingWorkConfirmations = $derived(
    [...session.pendingElicitations.values()].filter((elicitation) =>
      isPiWorkRuntimeConfirmation(elicitation.mode),
    ).length,
  );
  let hasWorkInputRequest = $derived(
    [...session.pendingElicitations.values()].some(
      (elicitation) => !isPiWorkRuntimeConfirmation(elicitation.mode),
    ),
  );
  let progressSnapshot = $derived.by(
    (): WorkProgressSnapshot => ({
      phase: authoritativeProgressView?.phase
        ? authoritativeProgressView.phase === "waiting_user"
          ? session.hasPendingPermission || pendingWorkConfirmations > 0
            ? "waiting_approval"
            : "waiting_input"
          : (authoritativeProgressView.phase as import("$lib/types/work").WorkProgressPhase)
        : deriveWorkProgressPhase({
            sessionPhase: session.phase,
            runStatus: session.run?.status ?? null,
            workRunStatus: authoritativeProgressView?.runStatus ?? null,
            tasks: progressTasks,
            artifacts,
            hasPendingPermission:
              session.hasPendingPermission ||
              pendingWorkConfirmations > 0 ||
              currentPendingInteractions.length > 0,
            hasElicitation:
              hasWorkInputRequest ||
              currentPendingInteractions.some((item) => item.itemType === "question_elicitation"),
          }),
      sessionPhase: session.phase,
      runStatus: session.run?.status ?? null,
      tasks: progressTasks,
      taskState: session.workTaskState,
      activeToolName: currentPendingInteractions.length > 0 ? "" : session.activeToolName,
      pendingApprovalCount:
        session.pendingToolPermissions.length +
        pendingWorkConfirmations +
        currentPendingInteractions.length,
      pendingAccessRoot: hasDirectoryAccessPending,
      pendingElicitation: hasWorkInputRequest,
      error: session.error || workSession.error,
      toolCallCount: visibleTimeline.filter((e) => e.kind === "tool").length,
    }),
  );
  let currentWorkTaskId = $derived(session.run?.work_task_id ?? null);
  let currentWorkRunId = $derived(session.run?.work_run_id ?? session.run?.id ?? runId ?? null);

  $effect(() => {
    // A running workspace session must use the generic session event stream
    // just like a standalone task. The durable WorkRun projection is loaded
    // once the turn settles, so it cannot compete with the first long stream.
    // Once the session is locally stopped, there is no reason to keep a live
    // WorkRun watcher around while the backend finishes its terminal cleanup.
    // A later resume will subscribe again for the new turn.
    if (!session.isRunning && session.run?.status !== "stopped" && currentWorkRunId) {
      const unsubscribe = workProjectionStore.subscribe(currentWorkRunId);
      return unsubscribe;
    }
  });

  let authoritativeProgressView = $derived(
    currentWorkRunId ? workProjectionStore.getProjection(currentWorkRunId) : null,
  );

  let resultPresentation = $derived.by(() => {
    let durationMs: number | null = null;
    if (session.durationMs && session.durationMs > 0) {
      durationMs = session.durationMs;
    }
    const startedAt = session.run?.started_at;
    const completedAt = session.run?.ended_at;

    return deriveWorkResultPresentation({
      progressView: authoritativeProgressView,
      progressSnapshot: !authoritativeProgressView ? progressSnapshot : null,
      artifacts,
      allowSessionFallback: !authoritativeProgressView,
      durationMs,
      startedAt,
      completedAt,
    });
  });

  let hasCurrentOperationalEvidence = $derived.by(() =>
    hasOperationalWorkEvidence({
      progressView: authoritativeProgressView,
      progressSnapshot: !authoritativeProgressView ? progressSnapshot : null,
      artifacts,
      pendingInteractions: currentPendingInteractions,
    }),
  );
  let hasCompletionEvidence = $derived.by(() =>
    hasWorkCompletionEvidence({
      progressView: authoritativeProgressView,
      progressSnapshot: !authoritativeProgressView ? progressSnapshot : null,
      artifacts,
    }),
  );
  // Promotion is sticky for the lifetime of the run. A later refresh can
  // temporarily omit a progress projection, but must not turn an already
  // operational task back into an answer-only conversation.
  let operationalRunKey = $state("");
  $effect(() => {
    const runKey = session.run?.id ?? "";
    if (!runKey) {
      operationalRunKey = "";
      return;
    }
    if (hasCurrentOperationalEvidence && operationalRunKey !== runKey) {
      operationalRunKey = runKey;
    }
  });
  let hasOperationalWork = $derived(
    hasCurrentOperationalEvidence && Boolean(session.run?.id)
      ? true
      : Boolean(session.run?.id && operationalRunKey === session.run.id),
  );
  let transcriptRecoveryRunId = $state("");
  $effect(() => {
    const run = session.run;
    if (
      !run?.id ||
      run.app_mode !== "work" ||
      !["completed", "failed", "stopped"].includes(run.status) ||
      booting ||
      workSession.loading ||
      workSession.starting ||
      session.phase === "loading" ||
      transcriptRecoveryRunId === run.id
    ) {
      return;
    }
    if (
      visibleTimeline.some((entry) => entry.kind === "assistant") ||
      session.streamingText ||
      session.thinkingText
    ) {
      return;
    }

    // A terminal Work snapshot can predate the assistant event when recovery
    // stopped the run during app restart. Re-read the authoritative event log
    // once so the already persisted answer is restored without user action.
    transcriptRecoveryRunId = run.id;
    untrack(() => {
      void session.loadRun(run.id);
    });
  });
  let shouldRenderResultCard = $derived(
    resultPresentation.isTerminal && hasOperationalWork && hasCompletionEvidence,
  );

  interface TurnDeliveryResult {
    turnId: string;
    presentation: WorkResultPresentation;
    artifacts: WorkArtifactSummary[];
    primaryArtifact?: WorkArtifactSummary;
    isLatest: boolean;
  }

  let expandedDeliveryTurns = $state<Record<string, boolean>>({});

  function toggleDeliveryTurnExpand(turnId: string, defaultExpanded: boolean) {
    const current = expandedDeliveryTurns[turnId] ?? defaultExpanded;
    expandedDeliveryTurns = {
      ...expandedDeliveryTurns,
      [turnId]: !current,
    };
  }

  let turnDeliveryMap = $derived.by(() => {
    const map = new Map<string, TurnDeliveryResult>();
    if (!hasOperationalWork || !hasCompletionEvidence || !resultPresentation.isTerminal) {
      return map;
    }
    if (presentationTurns.length === 0) {
      return map;
    }

    const userIndices: number[] = [];
    visibleTimeline.forEach((entry, idx) => {
      if (entry.kind === "user") userIndices.push(idx);
    });

    const turnArtifactsMap = new Map<string, WorkArtifactSummary[]>();
    const turnHasToolsMap = new Map<string, boolean>();

    for (let turnIdx = 0; turnIdx < presentationTurns.length; turnIdx++) {
      const turn = presentationTurns[turnIdx];
      const startIdx = userIndices[turnIdx] !== undefined ? userIndices[turnIdx] : 0;
      const nextIdx =
        turnIdx + 1 < userIndices.length ? userIndices[turnIdx + 1] : visibleTimeline.length;
      const entries = visibleTimeline.slice(startIdx, nextIdx);

      const hasTools = entries.some((e) => e.kind === "tool");
      turnHasToolsMap.set(turn.id, hasTools);

      const registeredTargets = new Set<string>();
      entries.forEach((e) => {
        if (e.kind === "tool") {
          const tool = e.tool;
          if (artifactToolNames.has(tool.tool_name)) {
            if (tool.input && typeof tool.input === "object") {
              const inp = tool.input as Record<string, unknown>;
              if (inp.path && typeof inp.path === "string")
                registeredTargets.add(inp.path.toLowerCase());
              if (inp.title && typeof inp.title === "string")
                registeredTargets.add(inp.title.toLowerCase());
            }
          }
        }
      });

      const turnStart = turn.userMessage?.timestamp
        ? new Date(turn.userMessage.timestamp).getTime()
        : 0;
      const nextTurn = presentationTurns[turnIdx + 1];
      const turnEnd = nextTurn?.userMessage?.timestamp
        ? new Date(nextTurn.userMessage.timestamp).getTime()
        : Infinity;

      const matched = artifacts.filter((art) => {
        const artPath = (art.path || "").toLowerCase();
        const artTitle = (art.title || "").toLowerCase();
        if (
          (artPath && registeredTargets.has(artPath)) ||
          (artTitle && registeredTargets.has(artTitle)) ||
          registeredTargets.has(art.id.toLowerCase())
        ) {
          return true;
        }
        if (art.createdAt) {
          const artTs = new Date(art.createdAt).getTime();
          if (!isNaN(artTs) && artTs >= turnStart && artTs <= turnEnd) {
            return true;
          }
        }
        return false;
      });

      if (matched.length > 0) {
        turnArtifactsMap.set(turn.id, matched);
      }
    }

    let totalMatchedCount = 0;
    for (const arts of turnArtifactsMap.values()) {
      totalMatchedCount += arts.length;
    }

    if (totalMatchedCount === 0 && artifacts.length > 0) {
      let fallbackTurn = presentationTurns[0];
      for (let i = presentationTurns.length - 1; i >= 0; i--) {
        if (turnHasToolsMap.get(presentationTurns[i].id)) {
          fallbackTurn = presentationTurns[i];
          break;
        }
      }
      turnArtifactsMap.set(fallbackTurn.id, artifacts);
    }

    for (let turnIdx = 0; turnIdx < presentationTurns.length; turnIdx++) {
      const turn = presentationTurns[turnIdx];
      const isLatest = turnIdx === presentationTurns.length - 1;
      const turnArtifacts = turnArtifactsMap.get(turn.id);

      if (turnArtifacts && turnArtifacts.length > 0) {
        const presentation =
          isLatest && resultPresentation.isTerminal
            ? resultPresentation
            : deriveWorkResultPresentation({
                progressView: null,
                progressSnapshot: null,
                artifacts: turnArtifacts,
                allowSessionFallback: true,
                durationMs: turn.durationMs > 0 ? turn.durationMs : null,
                startedAt: turn.userMessage?.timestamp,
                completedAt: turn.finalMessage?.timestamp,
              });

        map.set(turn.id, {
          turnId: turn.id,
          presentation,
          artifacts: turnArtifacts,
          primaryArtifact: presentation.primaryArtifact ?? turnArtifacts[0],
          isLatest,
        });
      }
    }

    return map;
  });

  let runEffectiveCapabilities = $state<RunEffectiveCapabilitiesView | null>(null);
  let loadingRunCapabilities = $state(false);
  let capabilityInspectorOpen = $state(false);
  let provenanceArtifact = $state<WorkArtifactSummary | null>(null);
  let artifactAcceptance = $state<WorkArtifactAcceptance | null>(null);
  let loadingAcceptance = $state(false);

  let isWaitingDelivery = $derived(
    authoritativeProgressView?.runStatus === "waiting_delivery" ||
      authoritativeProgressView?.phase === "awaiting_delivery" ||
      (session.run?.status as string) === "waiting_delivery",
  );

  async function loadRunCapabilities(runId: string) {
    if (!runId) return;
    loadingRunCapabilities = true;
    try {
      runEffectiveCapabilities = await getRunEffectiveCapabilities(runId);
    } catch (err) {
      console.warn("Failed to load run effective capabilities:", err);
    } finally {
      loadingRunCapabilities = false;
    }
  }

  async function loadArtifactAcceptance(runId: string) {
    if (!runId) return;
    loadingAcceptance = true;
    try {
      artifactAcceptance = await getWorkArtifactAcceptance(runId);
    } catch (err) {
      console.warn("Failed to load artifact acceptance:", err);
    } finally {
      loadingAcceptance = false;
    }
  }

  $effect(() => {
    const runId = session.run?.id || currentWorkRunId;
    if (runId) {
      void loadRunCapabilities(runId);
      if (isWaitingDelivery) {
        void loadArtifactAcceptance(runId);
      }
    } else {
      runEffectiveCapabilities = null;
      artifactAcceptance = null;
    }
  });

  // Open the inspector once a run shows real task activity. A workspace-backed
  // session may have WorkTask/WorkRun ids for an ordinary conversational answer,
  // so the run id itself must not trigger the task-oriented panel.
  $effect(() => {
    const runKey = session.run?.id ?? "";
    if (!hasOperationalWork || !runKey || runKey === inspectorOpenedForRun) return;
    inspectorOpenedForRun = runKey;
    untrack(() => {
      if (!inspectorOpen) onToggleInspector?.();
    });
  });

  let lastRefreshedTerminalRunId = "";
  $effect(() => {
    const isTerminal = resultPresentation.isTerminal;
    const currentRunId = currentWorkRunId || session.run?.id || "";
    if (isTerminal && currentRunId && lastRefreshedTerminalRunId !== currentRunId) {
      lastRefreshedTerminalRunId = currentRunId;
      void onArtifactsChanged?.();
    }
  });

  let previewArtifact = $state<WorkArtifactSummary | null>(null);

  function handlePreviewArtifact(artifact: WorkArtifactSummary) {
    previewArtifact = artifact;
  }

  async function handleExportArtifact(artifactId: string) {
    if (onExportArtifact) {
      await onExportArtifact(artifactId);
      return;
    }
    const artifact = artifacts.find((item) => item.id === artifactId);
    const { save } = await import("$lib/platform/dialog");
    const destination = await save({
      defaultPath: artifact?.title || "work-artifact",
      filters: artifact?.artifactType
        ? [{ name: artifact.artifactType.toUpperCase(), extensions: [artifact.artifactType] }]
        : undefined,
    });
    if (!destination) return;
    if (!workspace && !session.run?.id) return;
    const scope = !workspace ? standaloneScope() : workspaceScope(workspace.id);
    await workResources.exportArtifact(
      scope,
      session.run?.id ?? "",
      artifactId,
      destination,
      artifact?.runId || null,
    );
  }

  async function handleExportHtml() {
    if (!session.run) {
      showChatToast(t("export_noConversation"), "error");
      return;
    }

    try {
      const rootEl = transcript;
      if (!rootEl) {
        showChatToast(t("export_noConversation"), "error");
        return;
      }

      const { exportConversationToHtml, buildExportFilename } =
        await import("$lib/utils/html-export");
      const title = session.run.name ?? session.run.prompt?.slice(0, 80) ?? "Untitled";
      const html = await exportConversationToHtml(rootEl, {
        title,
        sessionInfo: {
          model: session.run.model ?? session.model,
          cwd: session.sessionCwd || session.run.cwd,
          startedAt: session.run.started_at,
          turnCount:
            session.userTurnCount ||
            visibleTimeline.filter((entry) => entry.kind === "user").length,
        },
      });

      const { save } = await import("$lib/platform/dialog");
      const path = await save({
        defaultPath: buildExportFilename(title),
        filters: [{ name: "HTML", extensions: ["html"] }],
      });
      if (!path) return;

      await api.writeHtmlExport(path, html);
      showChatToast(t("export_htmlSuccess"));
    } catch (cause) {
      console.warn("Work conversation HTML export failed:", cause);
      showChatToast(t("export_htmlFailed"), "error");
    }
  }

  async function handleOpenArtifactExternal(artifactId: string) {
    const artifact = artifacts.find((item) => item.id === artifactId);
    if (!artifact) return;
    // 独立会话下以 session run id 作为 workspaceId 打开；否则用当前 workspace。
    if (!workspace && !session.run?.id) {
      showChatToast("当前会话还没有关联工作区，无法用系统应用打开。", "error");
      return;
    }
    const scope = !workspace ? standaloneScope() : workspaceScope(workspace.id);
    await workResources.openFile(scope, session.run?.id ?? "", artifact.path);
  }

  function handleOpenAllArtifacts() {
    if (!inspectorOpen && onToggleInspector) {
      onToggleInspector();
    }
  }

  let hasTranscript = $derived(
    visibleTimeline.length > 0 || Boolean(session.streamingText) || Boolean(session.thinkingText),
  );
  // A newly started run can legitimately have no assistant event for a short
  // time while the provider performs its first request. Keep the empty state
  // busy, but describe that state accurately instead of claiming the workspace
  // connection is still being established.
  let waitingForFirstResponse = $derived(
    !hasTranscript &&
      (workSession.starting ||
        workSession.sending ||
        Boolean(session.run?.id && session.isRunning)),
  );
  let emptyTranscriptStatus = $derived.by(() => {
    if (booting || workSession.loading) return "正在连接工作空间…";
    if (workSession.starting) return "正在启动 Work 任务…";
    if (waitingForFirstResponse) return "任务已启动，正在等待 Work 首条响应…";
    return "正在加载会话…";
  });

  $effect(() => {
    sessionInfo = internalSessionInfo;
    progressView = authoritativeProgressView;
    if (authoritativeProgressView && session.run?.status !== "stopped") {
      progress = workRunProgressToSnapshot(
        authoritativeProgressView,
        artifacts,
        session.error || workSession.error,
      );
      statusLabel = mapProgressViewToStatusLabel(authoritativeProgressView, artifacts);
    } else {
      progress = progressSnapshot;
      statusLabel = calculatedStatusLabel;
    }
    sessionAlive = calculatedSessionAlive;
    conversationArchived = archivedConversation;
    pendingInteractions = currentPendingInteractions;
    onStopSession = () => void stopSession();
  });

  async function loadModels() {
    await loadWorkModels();
  }

  async function loadGlobalResources(runtime = effectiveWorkAgent) {
    const requestedRuntime = runtime.trim();
    if (!requestedRuntime || requestedRuntime === loadedResourceRuntime) return;
    loadedResourceRuntime = requestedRuntime;
    try {
      const nextResources = await listWorkResources(requestedRuntime);
      if (!disposed && requestedRuntime === effectiveWorkAgent.trim()) {
        globalResources = nextResources;
      }
    } catch {
      // The Work runtime still starts with its built-in tools and runtime commands.
      if (loadedResourceRuntime === requestedRuntime) loadedResourceRuntime = "";
    }
  }

  $effect(() => {
    void loadGlobalResources(effectiveWorkAgent);
  });

  let runtimePreferences = $state<WorkRuntimePreferences | null>(null);

  function getGlobalDefaultModel(): string {
    const configured =
      runtimePreferences?.model?.trim() ||
      managedProviderDefaultModel ||
      userSettings?.pi_provider?.model?.trim() ||
      userSettings?.default_model?.trim() ||
      "";
    if (configured) {
      return configured;
    }
    // Don't eagerly pick modelCatalog[0] before preferences or settings have loaded.
    if (!runtimePreferences && !userSettings) return "";
    return modelCatalog[0]?.value || modelOptions[0]?.value || "";
  }

  async function resolveWorkspaceLatestModel(wsId: string, currentPref?: string): Promise<string> {
    if (
      currentPref &&
      (modelCatalog.length === 0 || modelCatalog.some((item) => item.value === currentPref))
    ) {
      return currentPref;
    }
    try {
      const runs = await withTimeout(
        listWorkSessions(wsId),
        WORK_SESSION_LOOKUP_TIMEOUT_MS,
        "读取 Work 对话列表超时",
      );
      const sorted = [...runs].sort((a, b) => {
        const timeA = a.last_activity_at ?? a.started_at ?? "";
        const timeB = b.last_activity_at ?? b.started_at ?? "";
        return timeB.localeCompare(timeA);
      });
      const latestWithModel = sorted.find((r) => r.model && r.model.trim());
      if (latestWithModel?.model) {
        const model = latestWithModel.model.trim();
        if (modelCatalog.length > 0 && !modelCatalog.some((item) => item.value === model)) {
          return "";
        }
        void persistWorkspaceModelPreference({ model }).catch(() => {});
        return model;
      }
    } catch {
      // ignore error
    }
    return "";
  }

  async function applyWorkspaceDefaultModelIfNeeded() {
    if (disposed || session.run?.id) return;
    // Don't overwrite a model the user has already explicitly selected.
    if (session.model.trim()) return;
    if (!userSettings || !runtimePreferences) {
      await loadAgentPreferences();
    }
    if (disposed || session.run?.id) return;
    // Re-check after async: user may have selected a model while we were loading preferences.
    if (session.model.trim()) return;
    if (workspace) {
      let targetModel = workspace.defaultModel?.trim() || "";
      if (
        targetModel &&
        modelCatalog.length > 0 &&
        !modelCatalog.some((item) => item.value === targetModel)
      ) {
        targetModel = "";
      }
      if (!targetModel && workspace.id) {
        targetModel = await resolveWorkspaceLatestModel(
          workspace.id,
          workspace.defaultModel?.trim(),
        );
      }
      if (!targetModel) {
        targetModel = getGlobalDefaultModel();
      }
      if (targetModel) {
        session.model = targetModel;
        if (
          workspace.defaultEffort &&
          (!workspace.defaultModel || workspace.defaultModel.trim() === targetModel)
        ) {
          currentEffort = workspace.defaultEffort;
        }
      }
    } else {
      const globalDefault = getGlobalDefaultModel();
      if (globalDefault) {
        session.model = globalDefault;
      }
    }
  }

  function ensureModelSelection() {
    if (disposed || session.run?.id || modelCatalog.length === 0) return;
    // Only apply a default when there is no current model selection.
    // Never overwrite a model the user explicitly chose, even if it isn't
    // yet present in the loaded catalog (catalog may still be loading, or
    // the model uses the provider/model composite format).
    if (session.model.trim()) return;
    if (workspace) {
      const workspacePreferredModel = workspace.defaultModel?.trim();
      if (
        workspacePreferredModel &&
        modelCatalog.some((m) => m.value === workspacePreferredModel)
      ) {
        session.model = workspacePreferredModel;
        if (workspace.defaultEffort) currentEffort = workspace.defaultEffort;
        return;
      }
    }
    const fallback = getGlobalDefaultModel() || modelOptions[0]?.value;
    if (fallback) session.model = fallback;
  }

  let pendingPreferencesPromise: Promise<void> | null = null;
  function loadAgentPreferences(): Promise<void> {
    if (!pendingPreferencesPromise) {
      pendingPreferencesPromise = (async () => {
        try {
          const fetchedUserSettings = await api.getUserSettings().catch(() => null);
          if (!disposed) {
            if (fetchedUserSettings) {
              userSettings = fetchedUserSettings;
            }
            const configuredRuntime =
              fetchedUserSettings?.work_default_runtime?.trim() || profile?.runtime?.trim();
            if (!session.run?.id && session.agent === "pi" && configuredRuntime) {
              session.agent = configuredRuntime;
            }
            const preferences = await loadWorkPreferences().catch(() => null);
            if (preferences) runtimePreferences = preferences;
            currentEffort = preferences?.effort?.trim() || "medium";
            if (!session.run?.id) {
              await applyWorkspaceDefaultModelIfNeeded();
            }
          }
        } catch {
          // The picker remains usable when runtime preferences are unavailable.
        } finally {
          pendingPreferencesPromise = null;
        }
      })();
    }
    return pendingPreferencesPromise;
  }

  async function handleModelChange(model: string) {
    if (conversationReadOnly) return;
    const previousModel = session.model;
    session.model = model;

    const runId = session.run?.id;
    if (!runId) return;

    if (session.run) {
      session.run = { ...session.run, model };
    }

    try {
      await api.updateRunModel(runId, model);
      if (session.sessionAlive && session.sessionCapabilities?.protocol.sessionSetModel !== false) {
        await api.setSessionModel(runId, model);
      }
    } catch (cause) {
      session.model = previousModel;
      workSession.error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function handleEffortChange(effort: string) {
    if (conversationReadOnly) return;
    currentEffort = effort;

    if (workspace) {
      void persistWorkspaceModelPreference({ effort }).catch(() => {});
    } else {
      try {
        await persistWorkEffort(effort);
      } catch (cause) {
        workSession.error = cause instanceof Error ? cause.message : String(cause);
        return;
      }
    }

    if (
      session.sessionAlive &&
      session.run?.id &&
      session.sessionCapabilities?.protocol.effortControl !== false
    ) {
      try {
        await api.setEffort(session.run.id, effort);
      } catch {
        // The preference is already persisted and will be used by the next Work session.
      }
    }
  }

  async function handleRenameRun(name: string) {
    if (conversationReadOnly || !session.run?.id) return;
    try {
      await api.renameRun(session.run.id, name);
      session.run = { ...session.run, name };
      notifySessionsChanged(session.run.id);
    } catch (cause) {
      workSession.error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  function handleResultCardContinue() {
    if (!canContinueWork || continuationBusy || conversationReadOnly) return;
    if (isStandalone) {
      void continueAssistantReply();
      return;
    }
    // Continue from the last assistant entry (fallback: any assistant entry).
    const entry = visibleTimeline
      .slice()
      .reverse()
      .find((item) => item.kind === "assistant");
    if (!entry) return;
    void continueAssistantReply(entry.id);
  }

  async function continueAssistantReply(entryId?: string) {
    if (conversationReadOnly || !session.run?.id || continuationBusy || !canContinueWork) return;
    continuationBusy = true;
    try {
      const workspaceId = workspace?.id ?? "";
      const continued = await continueWorkSession(
        workspaceId,
        session.run.id,
        entryId,
        session.run.model,
      );
      notifySessionsChanged(continued.id);
      const targetUrl = workspaceId
        ? `/chat/work?workspace=${encodeURIComponent(workspaceId)}&run=${encodeURIComponent(continued.id)}`
        : standaloneRunUrl(continued.id);
      await goto(targetUrl);
    } catch (cause) {
      workSession.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      continuationBusy = false;
    }
  }

  async function load() {
    booting = true;
    try {
      await workSession.startMiddleware();
      if (!disposed) await workSession.load(workspace?.id ?? "", runId, !newConversation);
      if (!disposed && session.run?.id && session.sessionAlive) {
        loadWorkModelsLive(session.run.id);
      }
      if (!disposed) {
        await applyWorkspaceDefaultModelIfNeeded();
        if (newConversation) {
          const urlPrompt = $page.url.searchParams.get("prompt");
          if (urlPrompt) {
            requestAnimationFrame(() => {
              promptRef?.setValue(urlPrompt);
              promptRef?.focus();
            });
          }
        }
      }
    } catch (cause) {
      workSession.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (!disposed) booting = false;
    }
  }

  function clearSessionError() {
    workSession.error = "";
    session.error = "";
  }

  function retrySessionLoad() {
    clearSessionError();
    void load();
  }

  function notifySessionsChanged(runOrId?: TaskRun | string) {
    const run = typeof runOrId === "string" ? undefined : runOrId;
    const runId = typeof runOrId === "string" ? runOrId : runOrId?.id;
    window.dispatchEvent(
      new CustomEvent("agentcabin:work-sessions-changed", {
        detail: { workspaceId: workspace?.id ?? "", runId, run },
      }),
    );
  }

  function standaloneRunUrl(runId: string): string {
    return `/chat/work?run=${encodeURIComponent(runId)}`;
  }

  function workspaceRunUrl(wsId: string, runId: string): string {
    return `/chat/work?workspace=${encodeURIComponent(wsId)}&run=${encodeURIComponent(runId)}`;
  }

  function adoptStartedRun(run: TaskRun, wsKey: string): void {
    // Notify sidebar and parent route listeners before navigating so adoptedRunScope
    // is recorded and the sidebar task item is populated immediately.
    notifySessionsChanged(run);
    void goto(isStandalone ? standaloneRunUrl(run.id) : workspaceRunUrl(wsKey, run.id), {
      replaceState: true,
      keepFocus: true,
      noScroll: true,
    });
  }

  async function handlePromptSend(message: string, attachments: Attachment[] = []) {
    const text = message.trim();
    if (
      (!text && attachments.length === 0) ||
      booting ||
      workSession.loading ||
      workSession.starting ||
      workSession.sending ||
      isInteractionBlocked
    ) {
      return;
    }

    try {
      const prompt = text || "请处理这些附件。";
      const wsKey = workspace?.id ?? "standalone";
      if (canSend) {
        await workSession.send(prompt, attachments);
      } else if (canResume) {
        try {
          const run = await workSession.resume(prompt, attachments);
          adoptStartedRun(run, wsKey);
        } catch (cause) {
          if (!isMissingPiWorkSessionError(cause)) throw cause;
          const run = await workSession.start(
            prompt,
            session.model || undefined,
            attachments,
            currentExecutionMode,
            activeWorkPreset,
          );
          adoptStartedRun(run, wsKey);
        }
      } else {
        const run = await workSession.start(
          prompt,
          session.model || undefined,
          attachments,
          currentExecutionMode,
          activeWorkPreset,
        );
        adoptStartedRun(run, wsKey);
      }
    } catch {
      // The store keeps the user-visible error; keep the composer available for retry.
    }
  }

  async function handleEditAndResend(
    turnIndex: number,
    newContent: string,
    attachments?: Attachment[],
  ) {
    if (session.isRunning || isInteractionBlocked) return;
    if (!newContent.trim()) return;

    const totalUserTurns = session.timeline.filter((e) => e.kind === "user").length;
    const numTurns = Math.max(1, totalUserTurns - turnIndex);

    try {
      const activeAgent = session.run?.agent ?? session.agent ?? "pi";
      if (activeAgent === "codex" && session.run?.id) {
        try {
          await api.rollbackTurns(session.run.id, numTurns);
        } catch (e) {
          console.warn("[work/ui] rollbackTurns failed:", e);
        }
      }
      session.truncateToTurn(turnIndex);
      await handlePromptSend(newContent, attachments || []);
    } catch (err) {
      console.warn("[work/ui] edit and resend failed:", err);
    }
  }

  function handleQueueSend(text: string, attachments: Attachment[]) {
    const queuedText = normalizeWorkQueuedText(text, workSlashCommands);
    void workSession.sendQueuedMessage(queuedText, attachments, "followUp").catch((error) => {
      workSession.error = (error as Error)?.message ?? String(error);
    });
  }

  function editQueuedMessage(id: string) {
    const queued = session.takeQueuedMessage(id);
    if (!queued) return;
    promptRef?.restoreSnapshot({
      text: queued.text,
      attachments: queued.attachments.map((attachment) => ({
        id: crypto.randomUUID(),
        ...attachment,
      })),
      pastedBlocks: [],
      pathRefs: [],
    });
  }

  function handleQueueSteer(id: string) {
    void workSession.steerQueuedMessage(id).catch((error) => {
      workSession.error = (error as Error)?.message ?? String(error);
    });
  }

  async function respondPermission(
    requestId: string,
    behavior: "allow" | "deny",
    updatedPermissions?: PermissionSuggestion[],
    updatedInput?: Record<string, unknown>,
    denyMessage?: string,
    interrupt?: boolean,
  ) {
    if (archivedConversation) return;
    await workSession.respondPermission(
      requestId,
      behavior,
      updatedPermissions,
      updatedInput,
      denyMessage,
      interrupt,
    );
  }

  async function respondElicitation(
    requestId: string,
    action: "accept" | "decline" | "cancel",
    content?: Record<string, unknown>,
  ) {
    if (archivedConversation) return;
    await workSession.respondElicitation(requestId, action, content);
  }

  async function stopSession() {
    if (archivedConversation) return;
    // Stop can be invoked from the composer while a shell popover is open.
    // Close those transient click-catchers before the bounded IPC request so
    // the rest of the app remains clickable even if backend teardown is slow.
    window.dispatchEvent(new Event("agentcabin:close-transient-overlays"));
    await workSession.stop();
    notifySessionsChanged(session.run ?? undefined);
  }

  function handleTranscriptScroll() {
    if (!transcript) return;
    isAutoScroll = isNearChatBottom(transcript);
    if (isAutoScroll) showScrollHint = false;
  }

  function scrollTranscriptToBottom() {
    if (!transcript) return;
    transcript.scrollTop = transcript.scrollHeight;
    isAutoScroll = true;
    showScrollHint = false;
  }

  // ── DSH Details Drawer State ──
  let toolDetailsDrawerState = $state<{
    open: boolean;
    toolName: string;
    args?: unknown;
    result?: unknown;
    isError?: boolean;
    isRunning?: boolean;
  }>({
    open: false,
    toolName: "",
  });

  function openToolDetails(
    toolName: string,
    args?: unknown,
    result?: unknown,
    isError = false,
    isRunning = false,
  ) {
    toolDetailsDrawerState = {
      open: true,
      toolName,
      args,
      result,
      isError,
      isRunning,
    };
  }

  function closeToolDetails() {
    toolDetailsDrawerState.open = false;
  }

  $effect(() => {
    const currentRunId = session.run?.id ?? null;
    if (currentRunId === previousRunId) return;
    previousRunId = currentRunId;
    isAutoScroll = true;
    showScrollHint = false;
  });

  $effect(() => {
    // Thinking text has its own optional panel and must not force a layout read
    // on the main transcript for every token. Reading scrollHeight here while
    // the whole Work transcript is rendering is enough to make a busy WebView
    // appear frozen; timeline/answer changes are the only events that need the
    // main transcript to follow the bottom.
    const _activity = `${session.timeline.length}:${session.streamingText.length}`;
    if (!_activity || !transcript) return;
    requestAnimationFrame(() => {
      if (!transcript) return;
      if (isAutoScroll) {
        transcript.scrollTop = transcript.scrollHeight;
      } else {
        showScrollHint = true;
      }
    });
  });

  $effect(() => {
    const wsKey = workspace?.id ?? "standalone";
    const key = `${wsKey}:${runId ?? "latest"}:${newConversation ? "new" : "resume"}`;
    if (key === loadedKey) return;
    // If the session already has this run loaded (via workSession.start() or .resume()),
    // skip the redundant load(). With the parent's stable {#key}, the same
    // WorkChatSurface instance survives the adoptStartedRun URL transition, so
    // session.run reflects the live run started by handlePromptSend — no reload needed.
    if (runId && session.run?.id === runId) {
      loadedKey = key;
      return;
    }
    loadedKey = key;
    void load();
  });

  $effect(() => {
    if (
      runId ||
      session.run ||
      session.model ||
      workSession.loading ||
      !managedProviderDefaultModel
    ) {
      return;
    }
    if (!workspace) {
      session.model = managedProviderDefaultModel;
    }
  });

  // Runtime extensions can write text into the host editor without opening a modal.
  // Keep this Work-only binding explicit so the extension cannot silently send
  // a message: set_editor_text only updates the draft, and the user still
  // submits it through PromptInput.
  $effect(() => {
    const editorText = session.pendingEditorText;
    if (editorText === null || !promptRef) return;
    session.pendingEditorText = null;
    promptRef.setValue(editorText);
  });

  onMount(() => {
    function handleRunMutation(event: Event) {
      const mutation = (event as CustomEvent<RunMutation>).detail;
      if (mutation?.kind === "update" && mutation.runId === session.run?.id && session.run) {
        session.run = { ...session.run, ...mutation.patch };
      }
    }

    function handleBrowserSessionStopped(event: Event) {
      const runId = (event as CustomEvent<{ runId?: unknown }>).detail?.runId;
      const current = session.run;
      if (
        typeof runId !== "string" ||
        !current ||
        (runId !== current.id && runId !== current.work_run_id)
      ) {
        return;
      }

      // BrowserInspector already asked the backend to stop the actor. Adopt
      // the terminal state locally as well, because the selected sidebar row
      // deliberately avoids a full metadata scan during active streaming.
      const stopped = { ...current, status: "stopped" as const };
      workSession.session.adoptStoppedRun(stopped);
      notifySessionsChanged(stopped);
    }

    function onWorkNewChat(event: Event) {
      const detail = (event as CustomEvent<{ workspaceId?: string; prompt?: string }>).detail;
      const wsId = workspace?.id ?? "";
      if (detail?.workspaceId && detail.workspaceId !== wsId) return;
      const wsKey = workspace?.id ?? "standalone";
      loadedKey = `${wsKey}:latest:new`;
      promptRef?.clearAll();
      void workSession.load(wsId, null, false).then(async () => {
        await applyWorkspaceDefaultModelIfNeeded();
        if (detail?.prompt) {
          promptRef?.setValue(detail.prompt);
          requestAnimationFrame(() => promptRef?.focus());
        }
      });
      requestAnimationFrame(() => {
        if (detail?.prompt) {
          promptRef?.setValue(detail.prompt);
        }
        promptRef?.focus();
      });
    }

    function handleVisibilityOrFocus() {
      if (disposed) return;
      if (document.visibilityState === "visible") {
        void inboxStore.fetch(false);
        if (workspace && !session.isRunning) {
          void workTaskStore.fetchTasks(workspace.id);
        }
        if (!session.isRunning && session.run?.id) {
          workProjectionStore.invalidate(session.run.id);
        }
      }
    }

    function handleHistorySearchShortcut(event: KeyboardEvent) {
      if (
        (event.metaKey || event.ctrlKey) &&
        event.key.toLocaleLowerCase() === "f" &&
        !event.altKey
      ) {
        event.preventDefault();
        focusHistorySearch();
      }
    }

    const hasOpenDialog = () => Boolean(document.activeElement?.closest("[role='dialog']"));
    keybindingStore?.registerCallback("chat:interrupt", () => {
      if (hasOpenDialog() || !session.isRunning) return;
      void workSession.stop();
    });
    keybindingStore?.registerCallback("chat:sendGlobal", () => {
      if (hasOpenDialog() || session.isRunning) return;
      promptRef?.triggerSend();
    });
    keybindingStore?.registerCallback("app:modelPicker", () => {
      if (!hasOpenDialog()) statusBarRef?.openModelDropdown();
    });
    keybindingStore?.registerCallback("chat:cyclePermission", () => {
      if (hasOpenDialog() || permissionModeBusy) return;
      const currentIndex = WORK_MODE_OPTIONS.findIndex(
        (option) => option.value === currentExecutionMode,
      );
      for (let offset = 1; offset <= WORK_MODE_OPTIONS.length; offset += 1) {
        const next = WORK_MODE_OPTIONS[(currentIndex + offset) % WORK_MODE_OPTIONS.length];
        if (next) {
          void setExecutionMode(next.value);
          break;
        }
      }
    });
    keybindingStore?.registerCallback("app:toggleBottomPanel", () => {
      if (!hasOpenDialog()) onToggleInspector?.();
    });
    window.addEventListener("keydown", handleHistorySearchShortcut);

    window.addEventListener(RUNS_CHANGED_EVENT, handleRunMutation);
    window.addEventListener("agentcabin:work-new-chat", onWorkNewChat);
    window.addEventListener("agentcabin:browser-session-stopped", handleBrowserSessionStopped);
    function onUserSettingsChanged(event: Event) {
      const next = (event as CustomEvent<UserSettings>).detail;
      if (next) userSettings = next;
      if (disposed || session.run?.id) return;
      void Promise.all([loadModels(), loadAgentPreferences()]).then(async () => {
        if (!disposed) {
          await applyWorkspaceDefaultModelIfNeeded();
          ensureModelSelection();
        }
      });
    }
    window.addEventListener("agentcabin:user-settings-changed", onUserSettingsChanged);
    document.addEventListener("visibilitychange", handleVisibilityOrFocus);
    window.addEventListener("focus", handleVisibilityOrFocus);

    let screenshotUnlisten: (() => void) | undefined;
    void getTransport()
      .listen<ScreenshotPayload>("screenshot-taken", (payload) => {
        const { contentBase64, mediaType, filename } = payload;
        const bytes = Uint8Array.from(atob(contentBase64), (c) => c.charCodeAt(0));
        const file = new File([bytes], filename, { type: mediaType });
        promptRef?.addFiles([file]);
      })
      .then((unlisten) => {
        if (disposed) {
          unlisten();
        } else {
          screenshotUnlisten = unlisten;
        }
      });

    void (async () => {
      await Promise.all([
        loadModels(),
        loadAgentPreferences(),
        loadGlobalResources(),
        inboxStore.fetch(false),
      ]);
      await applyWorkspaceDefaultModelIfNeeded();
      ensureModelSelection();
    })();

    return () => {
      screenshotUnlisten?.();
      window.removeEventListener(RUNS_CHANGED_EVENT, handleRunMutation);
      window.removeEventListener("agentcabin:work-new-chat", onWorkNewChat);
      window.removeEventListener("agentcabin:browser-session-stopped", handleBrowserSessionStopped);
      window.removeEventListener("agentcabin:user-settings-changed", onUserSettingsChanged);
      document.removeEventListener("visibilitychange", handleVisibilityOrFocus);
      window.removeEventListener("focus", handleVisibilityOrFocus);
      window.removeEventListener("keydown", handleHistorySearchShortcut);
      keybindingStore?.unregisterCallback("chat:interrupt");
      keybindingStore?.unregisterCallback("chat:sendGlobal");
      keybindingStore?.unregisterCallback("app:modelPicker");
      keybindingStore?.unregisterCallback("chat:cyclePermission");
      keybindingStore?.unregisterCallback("app:toggleBottomPanel");
      disposed = true;
      // The event middleware is a module-level singleton shared by Code and
      // Work. Dispose this run subscription, but keep the global listeners
      // alive while the keyed chat surface is being replaced.
      workSession.dispose();
    };
  });
</script>

<section
  class="chat-canvas relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background"
>
  <SessionStatusBar
    bind:this={statusBarRef}
    run={session.run}
    agent={effectiveWorkAgent}
    model={session.model}
    running={session.sessionAlive}
    onToggleSidebar={toggleAppSidebar}
    sidebarOpen={appSidebarOpen}
    cwd={isStandalone ? "" : workspace?.name || session.sessionCwd || workspace?.root || ""}
    statusText={session.sessionAlive ? "正在执行" : ""}
    {modelOptions}
    onModelChange={(model) => void handleModelChange(model)}
    effort={currentEffort}
    onEffortChange={(effort) => void handleEffortChange(effort)}
    onRename={session.run ? handleRenameRun : undefined}
    onStatusClick={onToggleInspector}
    onToggleRightSidebar={onToggleInspector}
    rightSidebarOpen={inspectorOpen}
    toolsCount={runEffectiveCapabilities
      ? (runEffectiveCapabilities.enabledSkills?.length ?? 0) +
        (runEffectiveCapabilities.mcpServers?.length ?? 0) +
        (runEffectiveCapabilities.connectors?.length ?? 0)
      : 0}
    onToolsClick={() => {
      capabilityInspectorOpen = true;
    }}
    onSearch={toggleHistorySearch}
    searchOpen={historySearchOpen}
  />

  {#if isStandalone && session.run?.id && hasOperationalWork}
    <div
      class="flex items-center justify-between border-b border-primary/20 bg-primary/[0.04] px-4 py-2 text-xs sm:px-5"
    >
      <div class="flex items-center gap-2 text-foreground/80">
        <span class="rounded bg-primary/10 px-1.5 py-0.5 text-[10px] font-semibold text-primary"
          >快速任务</span
        >
        <span class="text-muted-foreground"
          >{standaloneAssignmentBlocked
            ? "当前回合仍在执行，完成后即可收编。"
            : "独立会话的产物会复制到 Workspace，原始记录会保留。"}</span
        >
      </div>
      {#if workspaces.length > 0}
        <button
          type="button"
          disabled={standaloneAssignmentBlocked}
          class="rounded-lg border border-primary/40 bg-primary/10 px-2.5 py-1 text-xs font-semibold text-primary transition-colors hover:bg-primary/20 disabled:cursor-not-allowed disabled:opacity-50"
          onclick={() => {
            assignTargetWsId = workspaces[0]?.id ?? "";
            assignError = "";
            assignModalOpen = true;
          }}
        >
          {standaloneAssignmentBlocked ? "等待本轮完成" : "收编至 Workspace"}
        </button>
      {:else}
        <button
          type="button"
          class="rounded-lg border border-primary/40 bg-primary/10 px-2.5 py-1 text-xs font-semibold text-primary transition-colors hover:bg-primary/20"
          onclick={() => onCreateWorkspace?.()}
        >
          创建 Workspace
        </button>
      {/if}
    </div>
  {/if}

  {#if assignModalOpen}
    <div
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="assign-workspace-title"
      tabindex="-1"
      onkeydown={(event) => {
        if (event.key === "Escape" && !assigningWorkspace) {
          event.preventDefault();
          event.stopPropagation();
          assignModalOpen = false;
        }
        trapFocus(event, assignDialog ?? null);
      }}
    >
      <div
        bind:this={assignDialog}
        class="w-full max-w-sm rounded-xl border border-border bg-background p-5 shadow-xl space-y-4"
      >
        <h3 id="assign-workspace-title" class="text-sm font-bold text-foreground">
          收编任务至 Workspace
        </h3>
        <p class="text-xs text-muted-foreground">
          产物会复制到目标工作区，原始独立会话目录会保留，避免收编失败时丢失数据。
        </p>
        {#if assignError}
          <p
            class="rounded-lg border border-red-400/30 bg-red-400/5 px-3 py-2 text-xs text-red-500"
            role="alert"
          >
            {assignError}
          </p>
        {/if}
        <div>
          <label
            class="block text-xs font-medium text-muted-foreground mb-1.5"
            for="assign-ws-select">选择目标工作区</label
          >
          <select
            id="assign-ws-select"
            class="w-full rounded-lg border border-border bg-card px-3 py-2 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
            bind:value={assignTargetWsId}
          >
            {#each workspaces as ws (ws.id)}
              <option value={ws.id}>{ws.name}</option>
            {/each}
          </select>
        </div>
        <div class="flex justify-end gap-2 pt-2">
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-1.5 text-xs font-medium hover:bg-accent"
            onclick={() => {
              assignModalOpen = false;
              assignError = "";
            }}
          >
            取消
          </button>
          <button
            type="button"
            disabled={assigningWorkspace || !assignTargetWsId}
            class="flex items-center gap-1.5 rounded-lg bg-primary px-3.5 py-1.5 text-xs font-semibold text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            onclick={handleAssignToWorkspace}
          >
            {#if assigningWorkspace}
              <span
                class="h-3 w-3 animate-spin rounded-full border-2 border-primary-foreground/20 border-t-primary-foreground"
              ></span>
              <span>收编中…</span>
            {:else}
              <span>确认收编</span>
            {/if}
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if workSession.error || session.error}
    <div
      class="flex items-center justify-between gap-3 border-b border-red-400/20 bg-red-400/5 px-4 py-3 text-xs leading-5 text-red-500 sm:px-5"
      role="alert"
    >
      <span class="min-w-0 flex-1">{workSession.error || session.error}</span>
      <div class="flex shrink-0 items-center gap-2">
        <button
          type="button"
          class="rounded-md border border-red-400/30 px-2 py-1 text-[11px] font-semibold text-red-500 hover:bg-red-400/10 disabled:opacity-50"
          disabled={booting || workSession.loading}
          onclick={retrySessionLoad}
        >
          重试连接
        </button>
        <button
          type="button"
          class="rounded p-1 text-red-500/70 hover:bg-red-400/10 hover:text-red-500"
          aria-label="关闭错误提示"
          onclick={clearSessionError}
        >
          <svg
            class="h-3.5 w-3.5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            aria-hidden="true"
          >
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>
  {/if}

  {#if archivedConversation}
    <div
      class="flex shrink-0 items-center gap-2 border-b border-amber-500/20 bg-amber-500/5 px-4 py-2.5 text-xs text-amber-600 dark:text-amber-300 sm:px-5"
      role="status"
    >
      <svg
        class="h-3.5 w-3.5 shrink-0"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <rect x="3" y="5" width="18" height="14" rx="2" />
        <path d="M7 9h10M7 13h6" />
      </svg>
      <span>已归档 · 该对话为只读，不能发送消息或上传文件。</span>
    </div>
  {/if}

  {#if conversationRailEntries.length > 1}
    <ConversationTurnRail
      container={transcript ?? null}
      entries={conversationRailEntries}
      entryAttribute="data-work-timeline-id"
      onSelect={(entry: import("$lib/components/ConversationTurnRail.svelte").RailEntry) =>
        scrollToHistoryEntry(entry.id)}
    />
  {/if}

  <div
    bind:this={transcript}
    class="min-h-0 flex-1 overflow-y-auto overscroll-contain bg-background"
    style="overflow-anchor: auto"
    onscroll={handleTranscriptScroll}
  >
    {#if (booting || workSession.loading || waitingForFirstResponse) && !hasTranscript}
      <div class="flex min-h-56 items-center justify-center text-sm text-muted-foreground">
        <span
          class="mr-2 h-4 w-4 animate-spin rounded-full border-2 border-muted-foreground/20 border-t-primary"
        ></span>
        {emptyTranscriptStatus}
      </div>
    {:else if !hasTranscript}
      <div
        class="work-home-empty-state flex min-h-full flex-1 flex-col items-center justify-center px-4 py-6 sm:px-8 sm:py-10 lg:px-12"
      >
        <div class="flex w-full max-w-4xl flex-col items-center">
          <img
            src="/logo.png?v=2"
            alt=""
            aria-hidden="true"
            class="work-home-logo mb-4 h-10 w-10 opacity-90"
          />

          <h1
            class="work-home-title text-center text-2xl font-bold tracking-tight text-foreground select-none sm:text-3xl"
          >
            AgentCabin, 我帮你
          </h1>

          <p
            class="work-home-subtitle mt-2 max-w-md text-center text-sm leading-6 text-muted-foreground"
          >
            描述你想完成的事，我帮你拆解、执行并整理结果。
          </p>

          <!-- Scene mode pills (WorkBuddy style) -->
          <div
            class="work-home-secondary-actions mt-3 flex flex-wrap items-center justify-center gap-2 animate-fade-in"
            aria-hidden="true"
          >
            <button
              type="button"
              aria-pressed="true"
              class="inline-flex items-center gap-1.5 rounded-full bg-foreground px-3.5 py-1 text-xs font-medium text-background shadow-2xs focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M18 8h1a4 4 0 0 1 0 8h-1" />
                <path d="M2 8h16v9a4 4 0 0 1-4 4H6a4 4 0 0 1-4-4V8z" />
                <line x1="6" y1="1" x2="6" y2="4" />
                <line x1="10" y1="1" x2="10" y2="4" />
                <line x1="14" y1="1" x2="14" y2="4" />
              </svg>
              日常办公
            </button>
            <button
              type="button"
              aria-pressed="false"
              class="inline-flex items-center gap-1.5 rounded-full border border-border/60 bg-card/40 px-3.5 py-1 text-xs font-medium text-muted-foreground transition-colors hover:border-border hover:bg-accent/50 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              onclick={() => goto("/chat/pi")}
            >
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <polyline points="16 18 22 12 16 6" />
                <polyline points="8 6 2 12 8 18" />
              </svg>
              代码开发
            </button>
            <button
              type="button"
              aria-pressed="false"
              class="inline-flex items-center gap-1.5 rounded-full border border-border/60 bg-card/40 px-3.5 py-1 text-xs font-medium text-muted-foreground transition-colors hover:border-border hover:bg-accent/50 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              onclick={() => applyTemplateToPrompt("请帮我进行一项创意思维发散与设计方案构想：")}
            >
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <circle cx="12" cy="12" r="10" />
                <path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20" />
                <path d="M2 12h20" />
              </svg>
              设计创意
            </button>
          </div>

          <!-- Quick Starter Pills Bar (Above the composer, exactly like WorkBuddy) -->
          <div
            class="work-home-secondary-actions mt-6 flex w-full flex-wrap items-center justify-center gap-2 px-1"
            aria-hidden="true"
          >
            <button
              type="button"
              class="group inline-flex items-center gap-1.5 rounded-full border border-border/70 bg-card/60 px-3 py-1.5 text-xs font-medium text-foreground/80 shadow-2xs transition-all duration-150 hover:border-border hover:bg-accent/50 hover:text-foreground"
              onclick={() =>
                applyTemplateToPrompt(
                  "请帮我整理这份文档：提炼关键结论和待办，指出需要补充的信息，并输出一份结构清晰的可交付版本。",
                )}
            >
              <svg
                class="h-3.5 w-3.5 text-muted-foreground group-hover:text-foreground transition-colors"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                <polyline points="14 2 14 8 20 8" />
                <line x1="16" y1="13" x2="8" y2="13" />
                <line x1="16" y1="17" x2="8" y2="17" />
              </svg>
              <span>文档处理</span>
            </button>

            <button
              type="button"
              class="group inline-flex items-center gap-1.5 rounded-full border border-border/70 bg-card/60 px-3 py-1.5 text-xs font-medium text-foreground/80 shadow-2xs transition-all duration-150 hover:border-border hover:bg-accent/50 hover:text-foreground"
              onclick={() =>
                applyTemplateToPrompt(
                  "请帮我分析这份数据：先检查并说明明显问题，再提炼关键指标、生成合适的可视化图表，并输出一份分析报告。",
                )}
            >
              <svg
                class="h-3.5 w-3.5 text-muted-foreground group-hover:text-foreground transition-colors"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <line x1="18" y1="20" x2="18" y2="10" />
                <line x1="12" y1="20" x2="12" y2="4" />
                <line x1="6" y1="20" x2="6" y2="14" />
              </svg>
              <span>数据分析及可视化</span>
            </button>

            <button
              type="button"
              class="group inline-flex items-center gap-1.5 rounded-full border border-border/70 bg-card/60 px-3 py-1.5 text-xs font-medium text-foreground/80 shadow-2xs transition-all duration-150 hover:border-border hover:bg-accent/50 hover:text-foreground"
              onclick={() =>
                applyTemplateToPrompt(
                  "请根据我提供的材料制作一份结构清晰的 PPT，包含页面大纲、关键要点、图表建议和演讲备注。",
                )}
            >
              <svg
                class="h-3.5 w-3.5 text-muted-foreground group-hover:text-foreground transition-colors"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <rect width="20" height="14" x="2" y="3" rx="2" />
                <line x1="8" y1="21" x2="16" y2="21" />
                <line x1="12" y1="17" x2="12" y2="21" />
              </svg>
              <span>幻灯片 PPT</span>
            </button>

            <button
              type="button"
              class="group inline-flex items-center gap-1.5 rounded-full border border-border/70 bg-card/60 px-3 py-1.5 text-xs font-medium text-foreground/80 shadow-2xs transition-all duration-150 hover:border-border hover:bg-accent/50 hover:text-foreground"
              onclick={() =>
                applyTemplateToPrompt(
                  "请研究这个主题：列出可靠来源，比较关键观点，说明不确定性，并输出一份有结论和引用的研究报告。",
                )}
            >
              <svg
                class="h-3.5 w-3.5 text-muted-foreground group-hover:text-foreground transition-colors"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <circle cx="11" cy="11" r="8" />
                <path d="m21 21-4.3-4.3" />
              </svg>
              <span>深度调研</span>
            </button>

            <button
              type="button"
              class="group inline-flex items-center gap-1.5 rounded-full border border-border/70 bg-card/60 px-3 py-1.5 text-xs font-medium text-foreground/80 shadow-2xs transition-all duration-150 hover:border-border hover:bg-accent/50 hover:text-foreground"
              onclick={() =>
                applyTemplateToPrompt(
                  "请整理这个工作目录中的文件：先分析现状并提出归类和命名规则，等我确认后再执行变更，并汇总变更结果。",
                )}
            >
              <svg
                class="h-3.5 w-3.5 text-muted-foreground group-hover:text-foreground transition-colors"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path
                  d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
                />
              </svg>
              <span>文件整理</span>
            </button>
          </div>

          <!-- Prompt Composer (Floating workbench island) -->
          <div class="work-home-composer relative mt-8 w-full">
            {#if chatToast}
              <div
                class="pointer-events-none absolute bottom-full left-1/2 z-50 mb-1.5 flex w-max max-w-[min(720px,calc(100vw-2rem))] -translate-x-1/2 items-center justify-center rounded-lg border px-4 py-2 text-center text-sm shadow-lg backdrop-blur-sm animate-in fade-in slide-in-from-bottom-2 duration-200 {chatToastTone ===
                'error'
                  ? 'border-red-400/30 bg-red-400/10 text-red-600 dark:text-red-300'
                  : 'border-border bg-background/95'}"
                role="status"
                aria-live="polite"
              >
                {chatToast}
              </div>
            {/if}
            <PromptInput
              bind:this={promptRef}
              bind:selectedExpert
              onExpertClear={handleExpertClear}
              harness="work"
              agent="pi"
              capabilities={workComposerCapabilities}
              queueAvailable={workSession.canFollowUp}
              queueActionAvailable={workSession.canSteer}
              planModeActive={false}
              permissionPicker={workPermissionPicker}
              workspacePicker={workspacePickerConfig}
              customPlaceholder="今天帮你做些什么？ @ 引用对话文件，/ 调用技能与指令"
              disabled={booting ||
                workSession.loading ||
                workSession.starting ||
                workSession.sending ||
                workSession.stopping ||
                isInteractionBlocked}
              {hasRun}
              running={session.isRunning}
              sessionAlive={session.sessionAlive}
              {canResume}
              useStreamSession={true}
              isRemote={false}
              cliCommands={workSlashCommands}
              models={modelOptions}
              currentModel={session.model}
              {sessionInfo}
              {currentEffort}
              onSend={handlePromptSend}
              queuedMessages={session.queuedMessages}
              onQueueSend={workSession.canFollowUp ? handleQueueSend : undefined}
              onQueueSteer={workSession.canSteer ? handleQueueSteer : undefined}
              onQueueEdit={editQueuedMessage}
              onQueueDelete={(id) => session.deleteQueuedMessage(id)}
              onInterrupt={() => void stopSession()}
              onModelSwitch={(model) => void handleModelChange(model)}
              onEffortChange={(effort) => void handleEffortChange(effort)}
              availableSkills={skillItems.map((skill) => skill.name)}
              {skillItems}
              skillSelectorEmptyMessage={t("skillSelector_workEmpty")}
              cwd={workspace?.root || ""}
              runId={session.run?.id ?? ""}
              showAuthBadge={false}
              pendingPermission={session.hasPendingPermission}
            />
          </div>

          <!-- Bottom subtle footer note -->
          <div
            class="mt-3 flex items-center justify-center gap-2 text-[11px] text-muted-foreground/60"
          >
            <span class="h-1.5 w-1.5 rounded-full bg-emerald-500/70"></span>
            <span>本机执行</span>
            <span>·</span>
            <span>敏感操作按权限执行</span>
          </div>
        </div>
      </div>
    {:else}
      {#if historySearchOpen}
        <div
          class="sticky top-0 z-20 flex items-center justify-between gap-2 border-b border-border/30 bg-background/95 px-4 py-1.5 backdrop-blur-sm sm:px-6"
          data-export-exclude
        >
          <div class="flex-1 flex justify-end min-w-0">
            <ChatSearchToolbar
              bind:this={searchToolbarRef}
              container={transcript ?? null}
              bind:open={historySearchOpen}
            />
          </div>
          {#if !isAutoScroll}
            <button
              type="button"
              class="shrink-0 rounded-md px-2 py-1 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
              onclick={scrollTranscriptToBottom}
            >
              回到最新
            </button>
          {/if}
        </div>
      {/if}
      {#snippet renderWorkInteraction(tool: BusToolItem, item: ChatInteractionItem)}
        {@const dummyToolEntry: ToolEntry = {
          id: item.id,
          anchorId: item.id,
          kind: "tool",
          ts: "",
          tool,
        }}
        {@const pending = findPendingInteractionForTool(dummyToolEntry)}
        {@const resolved = findResolvedInteractionForTool(dummyToolEntry)}
        {#if pending}
          {#if isQuestionInteraction(pending)}
            <WorkInlineInteraction item={pending} onResolve={resolveInlineInteraction} />
          {:else}
            <WorkToolCall
              entry={dummyToolEntry}
              pendingInteraction={pending}
              resolvedInteraction={resolved}
              onResolveInteraction={resolveInlineInteraction}
            />
          {/if}
        {:else if resolved}
          <WorkToolCall
            entry={dummyToolEntry}
            pendingInteraction={null}
            resolvedInteraction={resolved}
            onResolveInteraction={resolveInlineInteraction}
          />
        {:else}
          <ChatInteractionBlock
            {tool}
            subTimeline={item.subTimeline}
            runId={session.run?.id ?? ""}
            agentDisplayName={CONVERSATION_ASSISTANT_NAME}
            onAnswer={(answer) => void session.answerToolQuestion(tool.tool_use_id, answer)}
          />
        {/if}
      {/snippet}

      {#each presentationTurns as turn, turnIdx (turn.id)}
        {@const isLatestTurn = turnIdx === presentationTurns.length - 1}

        <!-- User Message -->
        {#if turn.userMessage}
          <div
            id="msg-{turn.userMessage.anchorId}"
            data-work-timeline-id={turn.userMessage.id}
            class="work-timeline-entry scroll-mt-4"
          >
            <ConversationMessage
              message={{
                id: turn.userMessage.id,
                role: "user",
                content: turn.userMessage.content,
                timestamp: turn.userMessage.timestamp,
              }}
              attachments={turn.userMessage.attachments}
              agent="pi"
              isRunning={session.isRunning}
              onEdit={!session.isRunning && !conversationReadOnly
                ? (newContent) =>
                    handleEditAndResend(turn.turnIndex, newContent, turn.userMessage?.attachments)
                : undefined}
            />
          </div>
        {/if}

        <!-- Separators -->
        {#if turn.separators}
          {#each turn.separators as sep (sep.id)}
            <div class="w-full py-3">
              <div class="chat-content-width">
                <div class="flex items-center gap-3">
                  <span class="h-px flex-1 bg-border/40"></span>
                  <span class="whitespace-nowrap text-xs text-muted-foreground/60"
                    >{sep.content}</span
                  >
                  <span class="h-px flex-1 bg-border/40"></span>
                </div>
              </div>
            </div>
          {/each}
        {/if}

        <!-- Context Injection (Work Workspace) -->
        {#if turnIdx === 0}
          <div class="chat-content-width pb-1.5">
            <ContextInjectionRow
              type="injection"
              title="上下文"
              source={workspace?.name || "Work"}
              summary={workspace?.name ? `工作空间: ${workspace.name}` : undefined}
              content={workspace?.root
                ? `Workspace: ${workspace.name}\nRoot: ${workspace.root}`
                : "Work session active."}
            />
          </div>
        {/if}

        <!-- Assistant Turn Header -->
        {#if turn.processBlocks.length > 0 || turn.finalMessage || (turn.isRunning && isLatestTurn)}
          <AssistantTurnHeader
            timestamp={turn.finalMessage?.timestamp || turn.userMessage?.timestamp}
            agent="pi"
            displayName={CONVERSATION_ASSISTANT_NAME}
            expert={turn.userMessage
              ? parseExpertFromText(turn.userMessage.content).expert
              : undefined}
          />
        {/if}

        <!-- Process Stream (Reasoning -> Activity Group -> Interactions interleaved) -->
        {#if turn.processBlocks.length > 0 || turn.interactionBlocks.length > 0 || (turn.isRunning && isLatestTurn)}
          <div class="chat-content-width">
            <ChatProcessStream
              {turn}
              runId={session.run?.id ?? ""}
              agentDisplayName={CONVERSATION_ASSISTANT_NAME}
              onToggleCollapse={() => {
                expandedIntermediateTurns[turn.id] = turn.isCollapsed;
              }}
              renderCustomInteraction={renderWorkInteraction}
              onOpenDetails={openToolDetails}
              error={isLatestTurn && session.run?.status === "failed"
                ? { message: "运行中断或失败" }
                : undefined}
              onContinue={() => void handlePromptSend("继续")}
            />
          </div>
        {/if}

        <!-- Command Outputs -->
        {#if turn.commandOutputs}
          {#each turn.commandOutputs as cmdOut (cmdOut.id)}
            <div class="w-full py-2">
              <div class="chat-content-width">
                <div
                  class="command-output overflow-x-auto rounded-[var(--chat-radius-md,10px)] border border-[var(--chat-border,rgba(17,24,39,0.07))] bg-[var(--chat-surface-code)] px-4 py-3 text-[var(--chat-code-size,13px)] text-muted-foreground"
                >
                  <MarkdownContent
                    text={cmdOut.content}
                    workspaceId={workspace?.id}
                    basePath={workspace?.root}
                  />
                </div>
              </div>
            </div>
          {/each}
        {/if}

        <!-- Final Assistant Response -->
        {#if turn.finalMessage}
          {@const currentTurnUsage =
            usageByTurn.get(turn.turnIndex + 1) ??
            usageByTurn.get(turn.turnIndex) ??
            // DSH ACP usage notifications may arrive without the backend turn index.
            // Keep historical messages exact, but let the latest answer show the usage
            // that is already present in the conversation-level stats line.
            (isLatestTurn ? session.turnUsages.at(-1) : undefined)}
          <div
            id="msg-{turn.finalMessage.anchorId}"
            data-work-timeline-id={turn.finalMessage.id}
            class="work-timeline-entry scroll-mt-4"
          >
            <div class="chat-content-width">
              <ChatAssistantMessage
                content={turn.finalMessage.content}
                isStreaming={turn.finalMessage.isStreaming}
                model={turn.finalMessage.model}
                durationFormatted={turn.durationFormatted}
                durationMs={turn.durationMs}
                timestamp={turn.finalMessage.timestamp}
                usage={currentTurnUsage}
                isLatest={isLatestTurn}
                workspaceId={workspace?.id}
                basePath={workspace?.root}
                onExport={session.run ? () => void handleExportHtml() : undefined}
                onContinueFromMessage={canContinueWork
                  ? () => void continueAssistantReply(turn.finalMessage!.id)
                  : undefined}
              />
            </div>
          </div>
        {/if}

        <!-- Turn Delivery Result (Full card on latest turn; compact summary bar on historical turns) -->
        {#if turnDeliveryMap.has(turn.id)}
          {@const delivery = turnDeliveryMap.get(turn.id)!}
          {@const isHistoricalTurn = turnIdx < presentationTurns.length - 1}
          {@const isExpanded = expandedDeliveryTurns[turn.id] ?? !isHistoricalTurn}
          <div class="chat-content-width pt-3 pb-1">
            {#if isExpanded}
              <div class="space-y-2">
                <WorkResultCard
                  presentation={delivery.presentation}
                  artifacts={delivery.artifacts}
                  readOnly={archivedConversation}
                  onPreview={handlePreviewArtifact}
                  onExport={handleExportArtifact}
                  onOpenAllArtifacts={handleOpenAllArtifacts}
                  onContinue={isLatestTurn && canContinueWork
                    ? handleResultCardContinue
                    : undefined}
                  continueBusy={continuationBusy}
                  onShowDetails={(art) => (provenanceArtifact = art)}
                  onCollapse={isHistoricalTurn
                    ? () => toggleDeliveryTurnExpand(turn.id, !isHistoricalTurn)
                    : undefined}
                />
              </div>
            {:else}
              <WorkDeliverySummaryBar
                presentation={delivery.presentation}
                primaryArtifact={delivery.primaryArtifact}
                artifactCount={delivery.artifacts.length}
                onPreview={handlePreviewArtifact}
                isExpanded={false}
                onToggleExpand={() => toggleDeliveryTurnExpand(turn.id, !isHistoricalTurn)}
              />
            {/if}
          </div>
        {/if}
      {/each}

      {#if isWaitingDelivery}
        <div class="mx-auto w-full max-w-4xl px-4 py-4 sm:px-6">
          <WorkArtifactAcceptancePanel
            acceptance={artifactAcceptance}
            loading={loadingAcceptance}
            onRefresh={() => {
              const runId = session.run?.id || currentWorkRunId;
              if (runId) void loadArtifactAcceptance(runId);
            }}
            onViewArtifact={(artifactId) => {
              const art = artifacts.find((a) => a.id === artifactId);
              if (art) previewArtifact = art;
            }}
          />
        </div>
      {/if}

      {#if shouldRenderResultCard && presentationTurns.length === 0}
        <div class="mx-auto w-full max-w-4xl space-y-4 px-4 py-4 sm:px-6">
          <WorkResultCard
            presentation={resultPresentation}
            {artifacts}
            readOnly={archivedConversation}
            onPreview={handlePreviewArtifact}
            onExport={handleExportArtifact}
            onOpenAllArtifacts={handleOpenAllArtifacts}
            onContinue={canContinueWork ? handleResultCardContinue : undefined}
            continueBusy={continuationBusy}
            onShowDetails={(art) => (provenanceArtifact = art)}
          />
        </div>
      {/if}
    {/if}
    <ToBottomButton visible={!isAutoScroll} onClick={scrollTranscriptToBottom} />
    <DetailsDrawer
      open={toolDetailsDrawerState.open}
      toolName={toolDetailsDrawerState.toolName}
      args={toolDetailsDrawerState.args}
      result={toolDetailsDrawerState.result}
      isError={toolDetailsDrawerState.isError}
      isRunning={toolDetailsDrawerState.isRunning}
      onClose={closeToolDetails}
    />
  </div>

  {#if showScrollHint}
    <button
      type="button"
      class="absolute bottom-[5.5rem] left-1/2 z-10 flex -translate-x-1/2 items-center gap-1.5 rounded-full bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground shadow-lg transition-all duration-200 hover:bg-primary/90"
      onclick={scrollTranscriptToBottom}
    >
      新消息
      <svg
        class="h-3 w-3"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2.5"
        stroke-linecap="round"
        stroke-linejoin="round"><path d="m6 9 6 6 6-6" /></svg
      >
    </button>
  {/if}

  <WorkRuntimeSpecificSurface {session} provider="pi" />

  {#if unmirroredPendingTools.length > 0}
    <div
      class="shrink-0 px-4 pt-2 sm:px-5"
      class:pointer-events-none={conversationReadOnly}
      class:opacity-60={conversationReadOnly}
      aria-disabled={conversationReadOnly}
    >
      <PermissionPanel
        pendingTools={unmirroredPendingTools}
        agentDisplayName="Work"
        onPermissionRespond={respondPermission}
      />
    </div>
  {/if}

  {#if unmirroredElicitations.size > 0}
    <div
      class="shrink-0 px-4 pt-2 sm:px-5"
      class:pointer-events-none={conversationReadOnly}
      class:opacity-60={conversationReadOnly}
      aria-disabled={conversationReadOnly}
    >
      <ElicitationDialog
        elicitations={unmirroredElicitations}
        onRespond={respondElicitation}
        surface="work"
      />
    </div>
  {/if}

  {#if hasTranscript}
    <TodoPanel
      tasks={session.todoPanelVisible ? session.panelTasks : []}
      piTodoState={session.todoPanelVisible ? session.piTodoState : null}
    />
    <div
      class="shrink-0 bg-background px-4 pb-3 pt-2 sm:px-6"
      class:pointer-events-none={conversationReadOnly}
      class:opacity-70={conversationReadOnly}
      aria-disabled={conversationReadOnly}
    >
      <div class="mx-auto w-full max-w-4xl">
        <div class="relative">
          {#if chatToast}
            <div
              class="pointer-events-none absolute bottom-full left-1/2 z-50 mb-1.5 flex w-max max-w-[min(720px,calc(100vw-2rem))] -translate-x-1/2 items-center justify-center rounded-lg border px-4 py-2 text-center text-sm shadow-lg backdrop-blur-sm animate-in fade-in slide-in-from-bottom-2 duration-200 {chatToastTone ===
              'error'
                ? 'border-red-400/30 bg-red-400/10 text-red-600 dark:text-red-300'
                : 'border-border bg-background/95'}"
              role="status"
              aria-live="polite"
            >
              {chatToast}
            </div>
          {/if}

          {#if runtimeClientError}
            <div
              class="mb-3 rounded-2xl border border-destructive/30 bg-destructive/10 p-3 text-xs text-destructive flex items-center gap-2 shadow-sm"
              role="alert"
            >
              <svg
                class="h-4 w-4 shrink-0"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <circle cx="12" cy="12" r="10" />
                <line x1="12" y1="8" x2="12" y2="12" />
                <line x1="12" y1="16" x2="12.01" y2="16" />
              </svg>
              <span>{runtimeClientError}（历史会话处于只读模式）</span>
            </div>
          {/if}

          <PromptInput
            bind:this={promptRef}
            harness="work"
            bind:selectedExpert
            onExpertClear={handleExpertClear}
            agent="pi"
            capabilities={workComposerCapabilities}
            queueAvailable={workSession.canFollowUp}
            queueActionAvailable={workSession.canSteer}
            planModeActive={false}
            permissionPicker={workPermissionPicker}
            workspacePicker={workspacePickerConfig}
            customPlaceholder="继续描述下一步，Work 会沿用当前任务上下文…"
            disabled={booting ||
              workSession.loading ||
              workSession.starting ||
              workSession.sending ||
              workSession.stopping ||
              isInteractionBlocked}
            {hasRun}
            running={session.isRunning}
            sessionAlive={session.sessionAlive}
            {canResume}
            useStreamSession={true}
            isRemote={false}
            cliCommands={workSlashCommands}
            models={modelOptions}
            currentModel={session.model}
            {sessionInfo}
            {currentEffort}
            onSend={handlePromptSend}
            queuedMessages={session.queuedMessages}
            onQueueSend={workSession.canFollowUp ? handleQueueSend : undefined}
            onQueueSteer={workSession.canSteer ? handleQueueSteer : undefined}
            onQueueEdit={editQueuedMessage}
            onQueueDelete={(id) => session.deleteQueuedMessage(id)}
            onInterrupt={() => void stopSession()}
            onModelSwitch={(model) => void handleModelChange(model)}
            onEffortChange={(effort) => void handleEffortChange(effort)}
            availableSkills={skillItems.map((skill) => skill.name)}
            {skillItems}
            skillSelectorEmptyMessage={t("skillSelector_workEmpty")}
            cwd={workspace?.root || ""}
            runId={session.run?.id ?? ""}
            showAuthBadge={false}
            pendingPermission={session.hasPendingPermission}
            interruptDisabled={booting ||
              workSession.loading ||
              workSession.starting ||
              workSession.stopping ||
              conversationReadOnly}
          />
          <ChatSessionStatsLine turnUsages={session.turnUsages} timeline={visibleTimeline} />
        </div>
      </div>
    </div>
  {/if}

  <ArtifactPreviewModal
    artifact={previewArtifact}
    open={previewArtifact !== null}
    onClose={() => (previewArtifact = null)}
    onExport={handleExportArtifact}
    onOpenExternal={handleOpenArtifactExternal}
    onShowDetails={(art) => (provenanceArtifact = art)}
    workspaceRoot={workspace?.root || sessionInfo?.cwd || ""}
  />

  <CapabilityRunInspector
    runId={session.run?.id || currentWorkRunId || ""}
    capabilities={runEffectiveCapabilities}
    loading={loadingRunCapabilities}
    open={capabilityInspectorOpen}
    onClose={() => (capabilityInspectorOpen = false)}
  />

  <WorkArtifactProvenanceDrawer
    artifact={provenanceArtifact}
    open={provenanceArtifact !== null}
    onClose={() => (provenanceArtifact = null)}
  />
</section>
