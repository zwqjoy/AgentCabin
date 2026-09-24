<script lang="ts">
  import { page } from "$app/stores";
  import { goto, replaceState } from "$app/navigation";
  import { tick, onMount, untrack, getContext } from "svelte";
  import { platform } from "$lib/platform";
  import { getTransport } from "$lib/transport";
  import { dispatchRunMutation } from "$lib/utils/run-mutations";
  import * as api from "$lib/api";
  import {
    SessionStore,
    KeybindingStore,
    getEventMiddleware,
    loadCliInfo,
    getCliCurrentModel,
    getCliCommands,
    getCliModels,
    getModelsForAgent,
    getCodexDefaultModel,
    loadCodexModels,
    loadCodexModelsLive,
    loadPiModelsLive,
    canResumeNow,
    TERMINAL_PHASES,
    getResumeWarning,
    loadCliVersionInfo,
    getCliVersionInfo_cached,
    getCodexVersion,
    loadPiModels,
    getGrokModels,
    loadGrokModels,
  } from "$lib/stores";
  import type {
    Attachment,
    UserSettings,
    AgentSettings,
    SessionMode,
    CliModelInfo,
    ScreenshotPayload,
    SessionInfoData,
    TimelineEntry,
    GlobalProviderModel,
    GitProjectInfo,
    GitWorktreeInfo,
    GoalStatus,
    TaskRun,
    ProjectModelPreference,
    AgentProviderBinding,
    AgentProviderBindings,
  } from "$lib/types";
  import { PLATFORM_PRESETS, findCredential } from "$lib/utils/platform-presets";
  import { getPiModelEffortLevels, getPiModelOptions } from "$lib/utils/pi-provider-presets";
  import {
    filterCustomProviderModels,
    isProviderCompatible,
    resolveManagedProviderDefaultModel,
    type ProviderAgent,
  } from "$lib/utils/provider-routing";
  import {
    getProjectModelPreferenceKey,
    shouldApplyProjectModelPreference,
  } from "$lib/utils/project-model-preferences";
  import {
    ALL_RUNTIME_PROVIDERS,
    VISIBLE_RUNTIME_PROVIDERS,
    AGENT_ORDER,
    NATIVE_AGENT_ORDER,
    isKnownAgent,
    getAgentDisplayName,
    getAssistantDisplayName,
    CONVERSATION_ASSISTANT_NAME,
  } from "$lib/utils/agent-metadata";
  import {
    getAgentTarget,
    getRunRoute,
    isWorkRun,
    isNativeTarget,
    isPiTarget,
    getTargetRoute,
  } from "$lib/utils/agent-target";
  import {
    getSavedProjectCwd,
    setSavedProjectCwd,
    getSavedPinnedCwds,
  } from "$lib/utils/project-cwd";
  import { buildProjectFolders, normalizeCwd } from "$lib/utils/sidebar-groups";
  import { loadRemovedCwds } from "$lib/utils/removed-cwds";
  import { getAppRealm, type AppRealm } from "$lib/stores/app-mode.svelte";
  import { IS_WEBKIT } from "$lib/utils/platform";
  import {
    getEnabledCapabilitySkills,
    toCapabilitySkillItems,
    toCodexCapabilitySkillItems,
  } from "$lib/utils/chat-skills";
  import { isExpertSkill } from "$lib/utils/expert-context";
  import {
    detectBatchGroups,
    detectToolBursts,
    isToolChatterAssistant,
    detectToolProgressGroups,
    isPlanFilePath,
    planFileName,
    extractPlanContent,
  } from "$lib/utils/tool-rendering";

  import XTerminal from "$lib/components/XTerminal.svelte";
  import ConversationMessage from "$lib/components/ConversationMessage.svelte";
  import AssistantTurnHeader from "$lib/components/AssistantTurnHeader.svelte";
  import ChatProcessStream from "$lib/components/chat/ChatProcessStream.svelte";
  import ChatAssistantMessage from "$lib/components/chat/ChatAssistantMessage.svelte";
  import ChatSessionStatsLine from "$lib/components/chat/ChatSessionStatsLine.svelte";
  import { buildChatPresentationTurns } from "$lib/utils/chat-presentation";
  import SessionStatusBar from "$lib/components/SessionStatusBar.svelte";
  import CapabilityRunInspector from "$lib/components/capabilities/CapabilityRunInspector.svelte";
  import { getRunEffectiveCapabilities } from "$lib/api/work";
  import type { RunEffectiveCapabilitiesView } from "$lib/types/work";
  import McpStatusPanel from "$lib/components/McpStatusPanel.svelte";
  import DiffModal from "$lib/components/DiffModal.svelte";
  import TurnReviewPanel from "$lib/components/TurnReviewPanel.svelte";
  import CodeAsideWorkspace from "$lib/components/code-aside/CodeAsideWorkspace.svelte";
  import type { CodeAsideTabType } from "$lib/types/code-aside";
  import PromptInput from "$lib/components/PromptInput.svelte";
  import ScheduledTasksChip from "$lib/components/ScheduledTasksChip.svelte";
  import TodoPanel from "$lib/components/TodoPanel.svelte";
  import PermissionPanel from "$lib/components/PermissionPanel.svelte";
  import ElicitationDialog from "$lib/components/ElicitationDialog.svelte";
  import PiExtensionPanel from "$lib/components/PiExtensionPanel.svelte";
  import PiSessionTreePanel from "$lib/components/PiSessionTreePanel.svelte";
  import GitWorktreePanel from "$lib/components/GitWorktreePanel.svelte";
  import ArchivedChatsView from "$lib/components/ArchivedChatsView.svelte";
  import CodeTasksCenter from "$lib/components/CodeTasksCenter.svelte";
  import ChatSearchToolbar from "$lib/components/ChatSearchToolbar.svelte";
  import ConversationTurnRail from "$lib/components/ConversationTurnRail.svelte";
  import ToBottomButton from "$lib/components/chat/ToBottomButton.svelte";
  import DetailsDrawer from "$lib/components/chat/DetailsDrawer.svelte";
  import ContextInjectionRow from "$lib/components/chat/ContextInjectionRow.svelte";

  import ToolActivity from "$lib/components/ToolActivity.svelte";
  import ShortcutHelpPanel from "$lib/components/ShortcutHelpPanel.svelte";
  import type { PromptInputSnapshot } from "$lib/types";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";
  import HookReviewCard from "$lib/components/HookReviewCard.svelte";
  import HookExecutionCard from "$lib/components/HookExecutionCard.svelte";
  import ContextUsageGrid from "$lib/components/ContextUsageGrid.svelte";
  import CostSummaryView from "$lib/components/CostSummaryView.svelte";
  import ReleaseNotesCard from "$lib/components/ReleaseNotesCard.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { yieldToMain } from "$lib/utils/yield";
  import { setLastTarget, getStoredRemoteCwd } from "$lib/utils/remote-cwd";
  import { derivePiShellTitle, shouldAutoName } from "$lib/utils/auto-name";
  import { resolvePermissionOptimistic } from "$lib/utils/resolve-permission";
  import { ansiToHtml, hasAnsiCodes } from "$lib/utils/ansi";
  import { type TurnUsage, classifyError } from "$lib/stores/types";
  import {
    mergeNativeSlashCommands,
    filterNativeSlashCommands,
    buildHelpText,
    CONTEXT_CLEARED_MARKER,
    parseRalphArgs,
    parseSlashCommand,
    normalizePiSlashText,
    resolvePiCommandName,
    getPiColdStartNativeCommands,
    getCodexColdStartNativeCommands,
    getGrokColdStartNativeCommands,
    VIRTUAL_COMMANDS,
    parseVirtualAction,
    resolveVirtualCommand,
  } from "$lib/utils/slash-commands";
  import { executeAddDir } from "$lib/utils/add-dir";
  import { CODEX_INIT_PROMPT } from "$lib/utils/codex-init-prompt";
  import {
    CODEX_REVIEW_UNCOMMITTED_PROMPT,
    codexReviewBasePrompt,
    codexReviewCommitPrompt,
    codexReviewCustomPrompt,
  } from "$lib/utils/codex-review-prompt";
  import CodexReviewModal from "$lib/components/CodexReviewModal.svelte";
  import type { CodexReviewKind } from "$lib/components/CodexReviewModal.svelte";
  import RewindCodexModal from "$lib/components/RewindCodexModal.svelte";
  import type { RewindTurn } from "$lib/components/RewindCodexModal.svelte";
  import GoalPanel from "$lib/components/GoalPanel.svelte";
  import { buildDoctorReport } from "$lib/utils/doctor";
  import type { RewindCandidate, RewindMarker } from "$lib/utils/rewind";
  import { truncate, cwdDisplayLabel, formatTokenCount } from "$lib/utils/format";
  import { mapSettled } from "$lib/utils/async-utils";
  import { uuid } from "$lib/utils/uuid";
  import {
    getCurrentTurnEntries,
    extractTurnModifiedPaths,
    parseUnifiedDiffStats,
    summarizeTurnChanges,
  } from "$lib/utils/diff-stats";
  import RewindModal from "$lib/components/RewindModal.svelte";
  import TurnSummaryBanner from "$lib/components/TurnSummaryBanner.svelte";
  import FolderPicker from "$lib/components/FolderPicker.svelte";
  import type { ElementSelection } from "$lib/types";
  import { isElementSelection } from "$lib/types";
  import {
    classifyPiGoalCommand,
    coercePiGoalStateForComposer,
    isPiFeatureAvailable,
    planEntryNeedsGoalPause,
    shouldBlockPiGoalCommand,
  } from "$lib/utils/pi-feature-coordinator";
  import { applyPiPermissionMode, normalizePiPermissionMode } from "$lib/utils/pi-permission";
  import { getComposerClearAction, isPiGoalChipActive } from "$lib/utils/composer-features";
  import { startContinuationSession } from "$lib/utils/continuation";
  import { getAgentCapabilities } from "$lib/utils/agent-capabilities";
  import { resolveContextWindow } from "$lib/utils/context-window";
  import { resolveNewSessionStartMode } from "$lib/utils/new-session-start";

  // ── Helpers ──

  /** Sanitize Codex run.model: if it looks like a Claude model name, return empty. */
  function codexDisplayModel(model?: string): string {
    if (!model) return "";
    return /claude|opus|sonnet|haiku/i.test(model) ? "" : model;
  }

  // ── Layout context ──
  const toggleLayoutSidebar = getContext<() => void>("toggleSidebar");
  const isLayoutSidebarOpen = getContext<() => boolean>("isSidebarOpen");
  const keybindingStore = getContext<KeybindingStore>("keybindings");

  // ── Store + Middleware ──
  const store = new SessionStore();
  const middleware = getEventMiddleware();

  // ── UI-only state (not in store) ──

  let currentRealm = $derived<AppRealm>(getAppRealm($page.url));
  let currentHarness = $derived.by<"code" | "work">(() => {
    if (store.run && ((store.run as any).is_work || (store.run as any).mode === "work"))
      return "work";
    if ($page.url.pathname.includes("/work") || $page.url.searchParams.get("mode") === "work")
      return "work";
    return "code";
  });
  let isArchivedView = $derived($page.url.searchParams.get("view") === "archived");
  let isTaskView = $derived($page.url.searchParams.get("view") === "tasks");
  let codeStandaloneTask = $state(false);
  let middlewareReady = $state(false);
  let settings = $state<UserSettings | null>(null);
  let xtermRef: XTerminal | undefined = $state();
  let promptRef: PromptInput | undefined = $state();
  let sidebarCollapsed = $state(true);
  /** Reactive cwd override for new-chat-in-folder (cleared when a run is loaded) */
  let folderCwdOverride = $state("");
  let vscodeAvailable = $state<boolean | null>(null);
  let chatAreaRef: HTMLDivElement | undefined = $state();
  let chatSearchOpen = $state(false);
  let chatSearchToolbarRef = $state<ReturnType<typeof ChatSearchToolbar>>();

  function focusChatSearch() {
    chatSearchOpen = true;
    requestAnimationFrame(() => {
      chatSearchToolbarRef?.focus();
    });
  }

  function toggleChatSearch() {
    if (chatSearchOpen) {
      chatSearchOpen = false;
      return;
    }
    focusChatSearch();
  }
  let isChatAutoScroll = $state(true);
  /** Non-reactive flag: suppresses auto-scroll reset during search scroll-to navigation. */
  let _scrollToInFlight = false;
  let showChatScrollHint = $state(false);
  let agentSettings = $state<AgentSettings | null>(null);
  let resuming = $state(false);
  /** Suppress "Session ended" flash during tool approval restart cycle. */
  let approving = $state(false);
  // (pendingResumeText removed — auto-resume uses atomic resume+send via initialMessage)
  /** Most recent run with a session_id — for "Continue last session" on welcome screen. */
  let lastContinuableRun = $state<import("$lib/types").TaskRun | null>(null);
  /** Target host dropdown in hero meta. */
  let targetDropdownOpen = $state(false);
  /** Auth overview for AuthSourceBadge. */
  let authOverview = $state<import("$lib/types").AuthOverview | null>(null);

  type CodeProjectOption = { cwd: string; name: string };
  /** Local Code project folders kept in sync with the sidebar's visible project set. */
  let knownCodeProjectCwds = $state<string[]>([]);
  let removedCwdsVersion = $state(0);

  function codeProjectCwdsFromRuns(runs: TaskRun[], realm: AppRealm): string[] {
    const folders = buildProjectFolders(
      runs.filter((run) => !isWorkRun(run) && !run.remote_host_name && !run.code_standalone_task),
      new Set<string>(),
      getSavedPinnedCwds(realm),
      typeof localStorage !== "undefined" ? loadRemovedCwds() : [],
      false,
    );
    return folders.filter((folder) => !folder.isUncategorized).map((folder) => folder.cwd);
  }

  async function refreshKnownCodeProjects() {
    const realm = currentRealm;
    try {
      const runs = await api.listRuns();
      if (currentRealm === realm) {
        knownCodeProjectCwds = codeProjectCwdsFromRuns(runs, realm);
      }
    } catch (e) {
      dbgWarn("chat", "failed to load Code projects", e);
    }
  }

  let codeProjectCwd = $derived.by(() => {
    void removedCwdsVersion;
    if (codeStandaloneTask || store.run?.code_standalone_task) return "";
    const persistedCwd = store.remoteHostName
      ? getStoredRemoteCwd(store.remoteHostName)
      : getSavedProjectCwd(currentRealm);
    const candidate = normalizeCwd(store.effectiveCwd || folderCwdOverride || persistedCwd || "");
    if (!candidate) return "";
    const removedSet = new Set(
      (typeof localStorage !== "undefined" ? loadRemovedCwds() : []).map(normalizeCwd),
    );
    removedSet.delete("");
    return removedSet.has(candidate) ? "" : candidate;
  });

  let codeProjectOptions = $derived.by((): CodeProjectOption[] => {
    void removedCwdsVersion;
    const removedSet = new Set(
      (typeof localStorage !== "undefined" ? loadRemovedCwds() : []).map(normalizeCwd),
    );
    removedSet.delete("");

    const seen = new Set<string>();
    const result: CodeProjectOption[] = [];
    const add = (cwd: string) => {
      const normalized = normalizeCwd(cwd);
      if (!normalized || seen.has(normalized) || removedSet.has(normalized)) return;
      seen.add(normalized);
      result.push({ cwd: normalized, name: cwdDisplayLabel(normalized) });
    };

    if (codeProjectCwd) add(codeProjectCwd);
    for (const cwd of knownCodeProjectCwds) add(cwd);
    return result;
  });

  let codeProjectReadOnly = $derived(
    Boolean(store.run?.id || store.sessionAlive || store.timeline.length > 0),
  );

  let codeProjectPickerConfig = $derived.by(() => {
    if (currentHarness !== "code") return null;
    return {
      currentProjectCwd: codeProjectCwd || null,
      currentProjectName: codeProjectCwd ? cwdDisplayLabel(codeProjectCwd) : "",
      currentStandaloneTask: codeStandaloneTask || store.run?.code_standalone_task === true,
      projects: codeProjectOptions,
      onSelect: selectCodeTaskScope,
      readOnly: codeProjectReadOnly,
    };
  });

  /** Folder picker state — resolves a Promise on confirm/cancel. */
  let folderPickerOpen = $state(false);
  let folderPickerInitialHost = $state<string | null>(null);
  let folderPickerInitialPath = $state("");
  let folderPickerHideTarget = $state(false);
  let folderPickerResolve: ((v: { hostName: string | null; path: string } | null) => void) | null =
    null;
  function openFolderPicker(opts: {
    initialHost?: string | null;
    initialPath?: string;
    hideTargetSelector?: boolean;
  }): Promise<{ hostName: string | null; path: string } | null> {
    folderPickerInitialHost = opts.initialHost ?? null;
    folderPickerInitialPath = opts.initialPath ?? "";
    folderPickerHideTarget = opts.hideTargetSelector ?? false;
    folderPickerOpen = true;
    return new Promise((resolve) => {
      folderPickerResolve = resolve;
    });
  }
  /** Globally enabled skills loaded from the Capability Center. */
  let preloadedCapabilitySkills = $state<import("$lib/types").StandaloneSkill[]>([]);
  /** Pi extension and prompt commands statically discovered before session_init. */
  let preloadedPiCommands = $state<import("$lib/types").CliCommand[]>([]);
  /** Skills the live Codex agent actually loaded this session (name + path), from the app-server
   *  `skills/list` runtime query. `path` is required to send a skill as a structured
   *  {type:"skill"} UserInput. Empty unless there's a live Codex session — the composer skill
   *  picker is gated on this being non-empty so we never offer a skill we can't actually send. */
  let codexRuntimeSkills = $state<{ name: string; path: string; description: string }[]>([]);
  /** Generation counter for reloadProjectData race guard. */
  let preloadGen = 0;
  /** Local proxy running statuses for AuthSourceBadge. */
  let localProxyStatuses = $state<Record<string, { running: boolean; needsAuth: boolean }>>({});

  // ── Codex auth warning ──
  let codexWarning = $state<string | null>(null);
  let agentChangeSeq = 0;

  // ── Preview state ──
  let previewInstanceId = $state("");
  let previewOpen = $derived(previewInstanceId !== "");
  let previewUrlBarOpen = $state(false);
  let previewUrlInput = $state(
    localStorage.getItem("agentcabin:preview-url") ?? "http://localhost:",
  );
  let detectedPreviewUrl = $state<string | null>(null);
  let previewProbeSequence = 0;
  let previewProbeTimer: ReturnType<typeof setTimeout> | undefined;

  const LOCAL_PREVIEW_URL_RE =
    /https?:\/\/(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\])(?::\d{1,5})?(?:\/[^\s"'<>()[\]]*)?/gi;
  const DEV_SERVER_COMMAND_RE =
    /(?:\b(?:npm|pnpm|yarn|bun)\s+(?:run\s+)?(?:dev|start|preview)\b|\b(?:vite|next\s+dev|astro\s+dev|webpack-dev-server|parcel)\b)/i;
  const COMMON_DEV_SERVER_PORTS = [3000, 3001, 4173, 4200, 4321, 5000, 5173, 5174, 8000, 8080];

  function previewTimelineEvidenceText(): string {
    const parts: string[] = [];
    for (const entry of store.timeline) {
      if (entry.kind === "user" || entry.kind === "assistant" || entry.kind === "command_output") {
        parts.push(entry.content);
      } else if (entry.kind === "tool") {
        try {
          parts.push(JSON.stringify(entry.tool));
        } catch {
          // Tool payloads are normally plain JSON; ignore an unexpected non-serializable value.
        }
      }
    }
    return parts.join("\n");
  }

  let previewTimelineText = $derived(previewTimelineEvidenceText());

  let localPreviewEvidence = $derived.by(() => {
    const text = `${previewTimelineText}\n${store.streamingText ?? ""}`;
    const urls = new Set<string>();
    for (const match of text.matchAll(LOCAL_PREVIEW_URL_RE)) {
      const candidate = match[0].replace(/[),.;!?]+$/, "");
      if (isLocalhostUrl(candidate)) urls.add(candidate);
    }
    return {
      urls: [...urls],
      shouldProbeCommonPorts: DEV_SERVER_COMMAND_RE.test(text),
    };
  });

  let localPreviewReady = $derived(previewOpen || detectedPreviewUrl !== null);

  async function isLocalPreviewReachable(url: string): Promise<boolean> {
    if (!isLocalhostUrl(url)) return false;
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 1200);
    try {
      // no-cors lets us detect a reachable local server without requiring its CORS headers.
      await fetch(url, {
        method: "GET",
        mode: "no-cors",
        cache: "no-store",
        signal: controller.signal,
      });
      return true;
    } catch {
      return false;
    } finally {
      clearTimeout(timeout);
    }
  }

  async function refreshLocalPreviewAvailability() {
    const evidence = localPreviewEvidence;
    const candidates = new Set<string>(evidence.urls);
    const storedUrl = previewUrlInput.trim();
    if (isLocalhostUrl(storedUrl)) candidates.add(storedUrl);
    if (evidence.shouldProbeCommonPorts) {
      for (const port of COMMON_DEV_SERVER_PORTS) candidates.add(`http://localhost:${port}`);
    }

    const sequence = ++previewProbeSequence;
    if (candidates.size === 0) {
      detectedPreviewUrl = null;
      return;
    }

    const results = await Promise.all(
      [...candidates].map(async (url) => ({ url, reachable: await isLocalPreviewReachable(url) })),
    );
    if (sequence !== previewProbeSequence) return;
    const reachable = results.find((result) => result.reachable)?.url ?? null;
    detectedPreviewUrl = reachable;
    if (reachable) previewUrlInput = reachable;
  }

  $effect(() => {
    // Re-check when a tool prints a localhost URL or a dev-server command appears.
    const evidence = localPreviewEvidence;
    const storedUrl = previewUrlInput;
    const signature = `${evidence.urls.join("|")}::${evidence.shouldProbeCommonPorts}::${storedUrl}`;
    if (!signature) return;
    clearTimeout(previewProbeTimer);
    previewProbeTimer = setTimeout(() => void refreshLocalPreviewAvailability(), 250);
  });

  // ── Model contamination helpers ──

  /** Cache of last confirmed-clean Anthropic model, used as final fallback. */
  let lastKnownGoodAnthropicModel: string | undefined;

  /** Detect if default_model was contaminated by a third-party platform model.
   *  Returns:
   *  - true  = confirmed contaminated (in third-party models, not in CLI models)
   *  - false = confirmed clean (in CLI known models)
   *  - null  = unknown (CLI not loaded, or model not found in any list)
   */
  function isContaminatedDefaultModel(dm: string): boolean | null {
    const cliModels = getCliModels();
    if (!cliModels.length) return null; // CLI models not loaded yet
    if (cliModels.some((m) => m.value === dm)) return false; // in CLI model list = clean

    const inThirdParty =
      PLATFORM_PRESETS.some(
        (p) => p.id !== "anthropic" && p.id !== "custom" && p.models?.includes(dm),
      ) ||
      (settings?.platform_credentials ?? []).some(
        (c) => c.platform_id !== "anthropic" && c.models?.includes(dm),
      );
    return inThirdParty ? true : null; // not in CLI + not in third-party = unknown
  }

  // ── Project init detection ──
  let projectInitStatus = $state<import("$lib/types").ProjectInitStatus | null>(null);
  let initCheckSeq = 0;

  // ── Welcome Task Cards state (Codex style non-auto-sending templates) ──
  let activeWelcomeCard = $state<string | null>(null);
  let activeWelcomeSub = $state<string | null>(null);

  function applyTemplateToPrompt(promptText: string) {
    activeWelcomeCard = null;
    activeWelcomeSub = null;
    if (promptRef) {
      promptRef.setValue(promptText);
      requestAnimationFrame(() => promptRef?.focus());
    }
  }

  // ── Task notification banner ──
  let notificationVisible = $state(false);
  let latestNotification = $state<{ task_id: string; status: string } | null>(null);

  // ── Rewind modal ──
  let rewindModalOpen = $state(false);
  let codexReviewPickerOpen = $state(false);

  // ── Turn summary banner ──
  let finalizedTurnDiff = $state("");
  let turnSnapshot = $state<{
    cwd: string;
    beforeTree: string;
    generation: number;
  } | null>(null);
  let turnTrackingGeneration = 0;
  let turnFinalizePromise: Promise<void> | null = null;
  let dismissedTurnSummaryIds = $state(new Set<string>());
  let undoingTurnSummaryIds = $state(new Set<string>());
  let undoneTurnSummaryIds = $state(new Set<string>());
  let turnReviewOpen = $state(false);
  let turnReviewDiff = $state("");
  let codeAsideOpen = $state(false);
  let codeAsideRequestedTab = $state<CodeAsideTabType | null>(null);
  let codeAsideRequestedPath = $state<string | null>(null);
  let turnSummaryRunId = "";

  function openFileInCodeAside(filePath: string) {
    if (!filePath) return;
    codeAsideRequestedTab = null;
    codeAsideRequestedPath = null;
    requestAnimationFrame(() => {
      codeAsideRequestedTab = "file";
      codeAsideRequestedPath = filePath;
      setCodeAsideOpen(true);
    });
  }

  onMount(() => {
    function onOpenFile(e: Event) {
      const detail = (e as CustomEvent<{ path?: string; filePath?: string }>).detail;
      const target = typeof detail === "string" ? detail : detail?.path || detail?.filePath;
      if (target) {
        openFileInCodeAside(target);
      }
    }
    window.addEventListener("agentcabin:open-file", onOpenFile);
    window.addEventListener("agentcabin:open-aside-file", onOpenFile);
    return () => {
      window.removeEventListener("agentcabin:open-file", onOpenFile);
      window.removeEventListener("agentcabin:open-aside-file", onOpenFile);
    };
  });

  function localTurnCwd(): string {
    if (store.isRemote || store.remoteHostName) return "";
    return (
      store.effectiveCwd ||
      folderCwdOverride ||
      (typeof window !== "undefined" ? getSavedProjectCwd(currentRealm) : "") ||
      ""
    );
  }

  async function beginTurnFileTracking() {
    // A fast follow-up send can arrive while the preceding turn's Git diff is still being
    // materialized. Finish that summary first so its event stays before the next user message.
    if (turnSnapshot && !turnFinalizePromise) queueTurnFileFinalization();
    if (turnFinalizePromise) await turnFinalizePromise;

    const generation = ++turnTrackingGeneration;
    turnSnapshot = null;
    finalizedTurnDiff = "";
    turnReviewOpen = false;
    turnReviewDiff = "";
    // Do not let the preceding Codex turn diff flash while the next send is being dispatched.
    store.turnDiff = "";

    // Codex natively supplies turn/diff/updated (thread- and turn-scoped). It does not need
    // or want workspace-wide git tree snapshots which can pick up concurrent edits from other chats.
    if (effectiveAgent === "codex") return;

    const cwd = localTurnCwd();
    if (!cwd) return;
    try {
      const beforeTree = await api.createGitTurnSnapshot(cwd);
      if (generation !== turnTrackingGeneration) return;
      turnSnapshot = { cwd, beforeTree, generation };
      dbg("turn-summary", "captured pre-turn git tree", { cwd, beforeTree });
    } catch (error) {
      // Non-git folders and remote sessions keep using structured Edit/Write events.
      dbgWarn("turn-summary", "pre-turn git tree unavailable", error);
    }
  }

  async function finalizeTurnFileTracking() {
    const snapshot = turnSnapshot;
    turnSnapshot = null;
    const runId = store.run?.id ?? "";

    // 1. If Codex, persist the authoritative turn diff pushed by Codex App Server.
    if (effectiveAgent === "codex") {
      const diff = store.turnDiff.trim();
      if (runId && diff) {
        finalizedTurnDiff = diff;
        try {
          const cwd = localTurnCwd() || store.effectiveCwd || "";
          await api.persistTurnFileSummary(runId, uuid(), cwd, diff);
        } catch (error) {
          dbgWarn("turn-summary", "failed to persist codex turn diff", error);
        }
      }
      return;
    }

    // 2. If non-Codex, only diff the specific files touched by tools in THIS turn.
    if (!snapshot) return;
    try {
      const currentTurn = getCurrentTurnEntries(store.timeline);
      const touchedPaths = extractTurnModifiedPaths(currentTurn);

      // Crucial: If no file modification tools were executed in this turn, NO files were changed
      // by this turn. Do NOT run git diff across the repo (which would catch edits from other chats).
      if (touchedPaths.length === 0) {
        dbg("turn-summary", "no files mutated in current turn, skipping git diff");
        return;
      }

      const diff = await api.diffGitTurnSnapshot(snapshot.cwd, snapshot.beforeTree, touchedPaths);
      if (store.run?.id === runId && snapshot.generation === turnTrackingGeneration) {
        finalizedTurnDiff = diff;
      }
      dbg("turn-summary", "captured completed turn diff for touched files", {
        cwd: snapshot.cwd,
        touchedPaths,
        len: diff.length,
      });
      if (runId && diff.trim()) {
        await api.persistTurnFileSummary(runId, uuid(), snapshot.cwd, diff);
      }
    } catch (error) {
      dbgWarn("turn-summary", "failed to diff completed turn", error);
    }
  }

  function queueTurnFileFinalization() {
    if (turnFinalizePromise) return;
    const pending = finalizeTurnFileTracking();
    turnFinalizePromise = pending;
    void pending.finally(() => {
      if (turnFinalizePromise === pending) turnFinalizePromise = null;
    });
  }

  let _prevIsRunning = $state(false);
  $effect(() => {
    const running = store.isRunning;
    if (_prevIsRunning && !running) {
      queueTurnFileFinalization();
    }
    _prevIsRunning = running;
  });

  $effect(() => {
    const id = store.run?.id ?? "";
    if (turnSummaryRunId && id !== turnSummaryRunId) {
      ++turnTrackingGeneration;
      turnSnapshot = null;
      finalizedTurnDiff = "";
      dismissedTurnSummaryIds = new Set();
      undoingTurnSummaryIds = new Set();
      undoneTurnSummaryIds = new Set();
      turnReviewOpen = false;
      turnReviewDiff = "";
    }
    turnSummaryRunId = id;
  });

  // ── Codex Wave-3: turn-based rewind + goal modals ──
  let codexRewindOpen = $state(false);
  let codexRewindBusy = $state(false);
  let goalPanelOpen = $state(false);
  let piTreeOpen = $state(false);
  let piTreeActionBusy = $state(false);
  let piPermissionBusy = $state(false);
  let piFeaturePending = $state<
    | "plan_entering"
    | "plan_exiting"
    | "goal_starting"
    | "goal_pausing"
    | "goal_resuming"
    | "goal_clearing"
    | null
  >(null);
  let piFeatureStartupPending = $state(false);
  let worktreeOpen = $state(false);
  let worktreeBusy = $state(false);
  let worktreeProject = $state<GitProjectInfo | null>(null);
  let worktrees = $state<GitWorktreeInfo[]>([]);
  let worktreeCheckCwd = $state("");
  type ContinuationAgent = "claude" | "codex" | "pi" | "grok" | "dsh";
  type ContinuationMenuState = {
    entryId: string | null;
    step: "location" | "agent";
    useWorktree?: boolean;
  };
  const continuationAgentIds: ContinuationAgent[] = [...VISIBLE_RUNTIME_PROVIDERS];
  let continuationMenu = $state<ContinuationMenuState | null>(null);
  let continuationBusy = $state(false);

  // ── Bottom Terminal Panel state (Cmd+J) ──
  interface PtyTab {
    id: string;
    label: string;
    unlisten: (() => void) | null;
    exitUnlisten: (() => void) | null;
  }

  let bottomPanelOpen = $state(false);
  let ptyTabs = $state<PtyTab[]>([]);
  let activeTabId = $state("");
  let termRefs = $state<Record<string, XTerminal | undefined>>({});
  let bottomPanelHeight = $state(256);
  let isResizingPanel = $state(false);

  function toggleBottomPanel() {
    if (bottomPanelOpen) {
      killAllPtySessions();
    } else {
      // Open: auto-create first terminal tab
      createNewTerminal();
    }
    bottomPanelOpen = !bottomPanelOpen;
  }

  function closeBottomPanel() {
    killAllPtySessions();
    bottomPanelOpen = false;
  }

  function getTermCwd(): string {
    return store.effectiveCwd || getSavedProjectCwd(currentRealm) || "";
  }

  function getTermLabel(): string {
    const cwd = getTermCwd();
    return cwd ? cwd.split("/").pop() || "Terminal" : "Terminal";
  }

  /** Create a new terminal tab + PTY session. */
  function createNewTerminal() {
    const id = `pty-${Date.now()}`;
    ptyTabs = [...ptyTabs, { id, label: getTermLabel(), unlisten: null, exitUnlisten: null }];
    activeTabId = id;
  }

  /** Close a specific terminal tab: kill PTY, remove listeners, remove tab. */
  function closeTerminalTab(tabId: string) {
    const tab = ptyTabs.find((t) => t.id === tabId);
    if (!tab) return;

    tab.unlisten?.();
    tab.exitUnlisten?.();
    api.ptyKill(tabId).catch(() => {});
    delete termRefs[tabId];

    const idx = ptyTabs.findIndex((t) => t.id === tabId);
    ptyTabs = ptyTabs.filter((t) => t.id !== tabId);

    // If closing the active tab, switch to neighbor
    if (activeTabId === tabId) {
      if (ptyTabs.length === 0) {
        closeBottomPanel();
      } else {
        const nextIdx = Math.min(idx, ptyTabs.length - 1);
        activeTabId = ptyTabs[nextIdx].id;
      }
    }
  }

  /** Kill all PTY sessions and clear tabs (panel close / component destroy). */
  function killAllPtySessions() {
    for (const tab of ptyTabs) {
      tab.unlisten?.();
      tab.exitUnlisten?.();
      api.ptyKill(tab.id).catch(() => {});
    }
    ptyTabs = [];
    activeTabId = "";
    termRefs = {};
  }

  /** XTerminal onReady: create PTY session for this tab. */
  async function handleTabReady(tabId: string, cols: number, rows: number) {
    const ref = termRefs[tabId];
    if (!ref) return;
    ref.clear();

    const cwd = getTermCwd() || "/";
    const transport = getTransport();

    const unlisten = await transport.listen<api.PtyDataEvent>("pty_data", (event) => {
      if (event.session_id === tabId) {
        termRefs[tabId]?.writeText(event.data);
      }
    });
    const exitUnlisten = await transport.listen<api.PtyExitEvent>("pty_exit", (event) => {
      if (event.session_id === tabId) {
        termRefs[tabId]?.writeText("\r\n\x1b[90m[process exited]\x1b[0m\r\n");
      }
    });

    // Store listeners on the tab object
    const tab = ptyTabs.find((t) => t.id === tabId);
    if (tab) {
      tab.unlisten = unlisten;
      tab.exitUnlisten = exitUnlisten;
    }

    try {
      await api.ptyCreate(tabId, cwd, cols, rows);
    } catch (e) {
      ref.writeText(`\x1b[31mFailed to start terminal: ${e}\x1b[0m\r\n`);
    }
  }

  /** Forward raw keystrokes to the correct PTY. */
  async function handleTabData(tabId: string, data: string) {
    await api.ptyWrite(tabId, data);
  }

  /** Resize the correct PTY. */
  async function handleTabResize(tabId: string, cols: number, rows: number) {
    await api.ptyResize(tabId, cols, rows);
  }

  // ── Panel drag-to-resize ──

  function startPanelResize(e: MouseEvent) {
    e.preventDefault();
    isResizingPanel = true;
    const startY = e.clientY;
    const startHeight = bottomPanelHeight;

    const onMove = (ev: MouseEvent) => {
      const delta = startY - ev.clientY;
      bottomPanelHeight = Math.max(120, Math.min(800, startHeight + delta));
    };
    const onUp = () => {
      isResizingPanel = false;
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      termRefs[activeTabId]?.fit();
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }

  // Top-level user messages → selectable turns for the Codex rewind modal.
  // Built lazily (only when the modal opens) rather than on every stream event:
  // it's the only consumer, and the per-turn regex/slice work is O(turns). The
  // cheap `store.userTurnCount` gates whether the entry point is shown/enabled.
  let codexRewindTurns = $state<RewindTurn[]>([]);
  function buildCodexRewindTurns(): RewindTurn[] {
    const turns: RewindTurn[] = [];
    let idx = 0;
    for (const e of store.timeline) {
      if (e.kind === "user") {
        turns.push({ index: idx, preview: e.content.replace(/\s+/g, " ").trim().slice(0, 80) });
        idx++;
      }
    }
    return turns;
  }
  function openCodexRewind() {
    codexRewindTurns = buildCodexRewindTurns();
    codexRewindOpen = true;
  }

  async function runCodexRewind(opts: { dropFromTurnIndex: number; numTurns: number }) {
    if (!store.run || effectiveAgent !== "codex") return;
    codexRewindBusy = true;
    try {
      await api.rollbackTurns(store.run.id, opts.numTurns);
      // Backend rolled back history; mirror it locally (files unchanged).
      const dropped = store.truncateToTurn(opts.dropFromTurnIndex);
      appendCommandOutput(t("codexRewind_done", { n: String(dropped) }));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      dbgWarn("chat", "codex rewind failed", err);
      appendCommandOutput(t("codexRewind_failed", { error: msg }));
    } finally {
      codexRewindBusy = false;
    }
  }

  // Build the review prompt for the picker choice and send it as a Codex turn.
  function runCodexReview(choice: { kind: CodexReviewKind; value: string }) {
    if (store.isRunning) {
      appendCommandOutput(t("codexReview_busy"));
      return;
    }
    const prompt =
      choice.kind === "base"
        ? codexReviewBasePrompt(choice.value)
        : choice.kind === "commit"
          ? codexReviewCommitPrompt(choice.value)
          : choice.kind === "custom"
            ? codexReviewCustomPrompt(choice.value)
            : CODEX_REVIEW_UNCOMMITTED_PROMPT;
    void sendMessage(prompt, []);
  }
  let rewindDirectTarget = $state<RewindCandidate | null>(null);
  let rewindMarkers = $state<RewindMarker[]>([]);

  // Clear direct target on modal close
  $effect(() => {
    if (!rewindModalOpen) rewindDirectTarget = null;
  });

  // Auto-name one-shot latch: reset only on actual run ID change
  let prevAutoNameRunId = "";
  let autoNameDone = false;
  $effect(() => {
    const id = store.run?.id ?? "";
    if (id !== prevAutoNameRunId) {
      prevAutoNameRunId = id;
      autoNameDone = false;
    }
  });

  // Clear markers on run switch (explicit prev-value check)
  let prevRewindRunId = "";
  $effect(() => {
    const id = store.run?.id ?? "";
    if (id !== prevRewindRunId) {
      prevRewindRunId = id;
      rewindMarkers = [];
    }
  });

  // Lazy: only compute when rewind modal is open (avoids 3 array allocations per timeline change)
  let rewindCandidates = $derived(
    rewindModalOpen
      ? store.timeline
          .map((e, i) => ({ entry: e, idx: i }))
          .filter(
            (
              x,
            ): x is {
              entry: Extract<TimelineEntry, { kind: "user" }> & { cliUuid: string };
              idx: number;
            } => x.entry.kind === "user" && !!x.entry.cliUuid,
          )
          .reverse()
          .map(
            ({ entry, idx }): RewindCandidate => ({
              cliUuid: entry.cliUuid,
              content: entry.content,
              ts: entry.ts,
              timelineIndex: idx,
            }),
          )
      : [],
  );

  // ── BTW side question ──
  let btwState = $state<{
    active: boolean;
    btwId: string | null;
    question: string;
    answer: string;
    error: string | null;
    loading: boolean;
  }>({ active: false, btwId: null, question: "", answer: "", error: null, loading: false });

  // ── Shortcut help panel ──
  let shortcutHelpOpen = $state(false);
  let statusBarRef: SessionStatusBar | undefined = $state();

  // ── Run effective capabilities inspector ──
  let capabilityInspectorOpen = $state(false);
  let runEffectiveCapabilities = $state<RunEffectiveCapabilitiesView | null>(null);
  let loadingRunCapabilities = $state(false);

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

  $effect(() => {
    const runId = store.run?.id;
    if (runId) {
      void loadRunCapabilities(runId);
    } else {
      runEffectiveCapabilities = null;
    }
  });

  let stashedInput: PromptInputSnapshot | null = $state(null);
  let sidebarRequestedTab = $state<"tools" | "files" | "info" | "tasks" | "browser" | null>(null);
  let requestedPreviewPath = $state<string | null>(null);

  function openPreviewForPath(path: string) {
    if (!path) return;
    requestedPreviewPath = path;
    sidebarRequestedTab = "files";
    if (sidebarCollapsed) sidebarCollapsed = false;
  }

  // Clear preview when run changes (defense-in-depth; ToolActivity also clears via its runId effect)
  let _lastPreviewClearRunId = "__unset__";
  $effect(() => {
    const id = store.run?.id ?? "";
    if (id !== _lastPreviewClearRunId) {
      _lastPreviewClearRunId = id;
      requestedPreviewPath = null;
    }
  });

  // ── Verbose state (chat page level) ──
  let verboseEnabled = $state(false);
  let verboseSeq = 0;
  let lastSyncedRunId = "__unset__"; // sentinel ≠ "__no_run__", ensures first-screen trigger
  let verboseRetryTick = $state(0);
  let verboseRetryCount = 0;
  let verboseRetryTimer: ReturnType<typeof setTimeout> | null = null;
  const VERBOSE_MAX_RETRIES = 3;

  // ── Tool result lazy-load cache (Phase 2) ──
  let toolResultCache = new Map<string, Record<string, unknown>>();
  let toolResultInflight = new Map<string, Promise<Record<string, unknown> | null>>();
  // Clear cache on run switch
  $effect(() => {
    const _ = store.run?.id;
    toolResultCache = new Map();
    toolResultInflight = new Map();
  });

  async function fetchToolResult(
    runId: string,
    toolUseId: string,
  ): Promise<Record<string, unknown> | null> {
    const key = `${runId}:${toolUseId}`;
    const cached = toolResultCache.get(key);
    if (cached) return cached;
    let pending = toolResultInflight.get(key);
    if (!pending) {
      pending = api.getToolResult(runId, toolUseId);
      toolResultInflight.set(key, pending);
    }
    try {
      const result = await pending;
      // Run-gen check: don't write stale results into a different run's cache
      if (result && store.run?.id === runId) {
        toolResultCache.set(key, result);
      }
      return result;
    } finally {
      toolResultInflight.delete(key);
    }
  }

  // ── Timeline rendering ──
  // Progressive render: start with the most recent N entries, grow on upward scroll.
  // Switching to a multi-thousand-entry run with `Infinity` would mount thousands of
  // ChatMessage components in one frame and freeze the WebView (issue #119).
  const INITIAL_RENDER_LIMIT = 100;
  const RENDER_GROWTH_STEP = 100;
  let renderLimit = $state(INITIAL_RENDER_LIMIT);
  let progressiveGen = 0; // generation counter for stale-callback protection
  let loadingMore = $state(false);
  let loadMoreArmed = $state(true); // throttle: re-armed by handleChatScroll
  let _suppressLoadMoreRearm = false; // raised during programmatic scrollTop adjustment

  async function syncVerboseState(runId: string | undefined) {
    const key = runId ?? "__no_run__";
    if (key === lastSyncedRunId) return; // same run — skip
    const seq = ++verboseSeq;
    // New run resets retry counter
    verboseRetryCount = 0;
    try {
      const cfg = await api.getCliConfig();
      if (seq !== verboseSeq) return; // stale response
      lastSyncedRunId = key; // mark synced on success only
      verboseEnabled = cfg.verbose === true;
      dbg("chat", "verbose state synced", { verbose: verboseEnabled, runId, seq });
    } catch {
      // Don't mark synced — retry via tick++ after 3s (up to max)
      if (seq === verboseSeq && verboseRetryCount < VERBOSE_MAX_RETRIES) {
        verboseRetryCount++;
        verboseRetryTimer = setTimeout(() => {
          verboseRetryTimer = null;
          verboseRetryTick++;
        }, 3000);
      }
    }
  }

  // ── MCP panel ──
  let mcpPanelOpen = $state(false);

  // Keep the last live Codex run discoverable to the settings page so it can
  // show the installed CLI's feature catalog without exposing low-level flags in chat.
  $effect(() => {
    if (effectiveAgent === "codex" && store.sessionAlive && store.run?.id) {
      localStorage.setItem("agentcabin:last-codex-run-id", store.run.id);
    }
  });

  // ── Codex turn diff (live aggregated diff for the current turn) ──
  let turnDiffOpen = $state(false);
  // Agent-neutral current-turn file summary. Codex supplies turnDiff; Claude/Pi fall back to
  // the Edit/Write data already retained on their timeline entries.
  let effectiveTurnDiff = $derived(finalizedTurnDiff.trim() ? finalizedTurnDiff : store.turnDiff);
  let turnChanges = $derived(summarizeTurnChanges(store.timeline, effectiveTurnDiff));
  type TurnSummaryEntry = Extract<TimelineEntry, { kind: "turn_summary" }>;

  async function undoTurnChanges(entry: TurnSummaryEntry) {
    if (undoingTurnSummaryIds.has(entry.id) || undoneTurnSummaryIds.has(entry.id)) return;
    undoingTurnSummaryIds = new Set(undoingTurnSummaryIds).add(entry.id);
    try {
      await api.applyGitTurnPatch(entry.cwd, entry.diff, true);
      undoneTurnSummaryIds = new Set(undoneTurnSummaryIds).add(entry.id);
      turnReviewOpen = false;
      turnReviewDiff = "";
      const summaries = store.timeline.filter(
        (item): item is TurnSummaryEntry => item.kind === "turn_summary",
      );
      if (summaries.at(-1)?.id === entry.id) {
        finalizedTurnDiff = "";
        store.turnDiff = "";
      }
      showChatToast(t("turnSummary_undoSuccess"));
    } catch (error) {
      dbgWarn("turn-summary", "undo failed", error);
      showChatToast(t("turnSummary_undoFailed"));
    } finally {
      const next = new Set(undoingTurnSummaryIds);
      next.delete(entry.id);
      undoingTurnSummaryIds = next;
    }
  }

  function reviewTurnChanges(entry: TurnSummaryEntry) {
    if (!entry.diff.trim()) return;
    turnReviewDiff = entry.diff;
    codeAsideRequestedTab = "review";
    setCodeAsideOpen(true);
    turnReviewOpen = false;
  }

  function dismissTurnSummary(id: string) {
    dismissedTurnSummaryIds = new Set(dismissedTurnSummaryIds).add(id);
  }

  // ── CLI session browser ──

  // ── Input history (most recent first) ──
  let userHistory = $derived.by(() =>
    store.timeline
      .filter((e): e is Extract<TimelineEntry, { kind: "user" }> => e.kind === "user")
      .map((e) => e.content)
      .reverse(),
  );

  let filteredTimeline = $derived(store.timeline);

  let visibleTimeline = $derived.by(() => {
    const ft = filteredTimeline;
    if (renderLimit >= ft.length) return ft;
    return ft.slice(ft.length - renderLimit);
  });

  let latestUserIndex = $derived.by(() => {
    for (let index = visibleTimeline.length - 1; index >= 0; index -= 1) {
      if (visibleTimeline[index]?.kind === "user") return index;
    }
    return -1;
  });

  /** Agents may emit empty/ellipsis assistant messages as transport chatter between tools. */
  function isToolChatterAt(index: number): boolean {
    const entry = visibleTimeline[index];
    if (!entry || !isToolChatterAssistant(entry)) return false;
    return (
      visibleTimeline[index - 1]?.kind === "tool" || visibleTimeline[index + 1]?.kind === "tool"
    );
  }

  let expandedCodeTurns = $state<Record<string, boolean>>({});

  function expandAllCodeTurns() {
    const next: Record<string, boolean> = { ...expandedCodeTurns };
    for (const turn of presentationTurns) {
      if (turn.processBlocks.length > 0) next[turn.id] = true;
    }
    expandedCodeTurns = next;
  }

  function collapseAllCodeTurns() {
    const next: Record<string, boolean> = { ...expandedCodeTurns };
    for (const turn of presentationTurns) {
      if (turn.processBlocks.length > 0 && turn.interactionBlocks.length === 0)
        next[turn.id] = false;
    }
    expandedCodeTurns = next;
  }

  let planContentMap = $derived.by(() => {
    const map = new Map<string, { content: string; fileName: string } | null>();
    for (const entry of visibleTimeline) {
      if (entry.kind === "tool" && entry.tool && entry.tool.tool_name === "ExitPlanMode") {
        map.set(entry.tool.tool_use_id, getPlanContentForExitPlan(entry.id));
      }
    }
    return map;
  });

  let presentationTurns = $derived(
    buildChatPresentationTurns(
      visibleTimeline,
      {
        isRunning: store.isRunning,
        thinkingText: store.thinkingText,
        streamingText: store.streamingText,
        durationMs: store.durationMs,
      },
      expandedCodeTurns,
      planContentMap,
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

  /** Merge tool-driven assistant narration into one collapsible block per user turn. */
  let progressGroups = $derived(detectToolProgressGroups(visibleTimeline));
  let progressHiddenIndices = $derived.by(() => {
    const hidden = new Set<number>();
    for (const group of progressGroups.values()) {
      for (const index of group.assistantIndices) {
        if (index !== group.startIndex) hidden.add(index);
      }
    }
    return hidden;
  });

  // ── Batch groups (consecutive ≥3 Task tools) ──
  let batchGroups = $derived(detectBatchGroups(visibleTimeline));

  let _lastBatchSig = "";
  $effect(() => {
    const size = batchGroups.size;
    const agents = size > 0 ? [...batchGroups.values()].reduce((s, g) => s + g.length, 0) : 0;
    const sig = `${size}:${agents}`;
    if (sig !== _lastBatchSig) {
      _lastBatchSig = sig;
      if (size > 0) dbg("chat", "batchGroups", { groupCount: size, totalAgents: agents });
    }
  });

  // ── Tool activity groups (excludes Task — handled by BatchProgressBar) ──
  // Even a single Bash/Read should become a quiet semantic row. Clicking the row still reveals
  // the original card, so the threshold is about default density, not loss of detail.
  let toolBursts = $derived(detectToolBursts(visibleTimeline, 1));

  // Layer 1: Auto-collapse — quiet bursts and large active bursts stay compact. A running
  // burst still exposes its semantic activity in ToolBurstHeader; users can expand it when
  // they need the individual cards.
  let autoCollapsed = $derived.by(() => {
    const keys = new Set<string>();
    for (const [, burst] of toolBursts) {
      const needsInteraction = burst.tools.some(
        (t) => t.status === "permission_prompt" || t.status === "ask_pending",
      );
      const finished = burst.stats.completed + burst.stats.failed === burst.stats.total;
      if (burst.stats.total > 0 && !needsInteraction && (burst.stats.running > 0 || finished)) {
        keys.add(burst.key);
      }
    }
    return keys;
  });

  // Layer 2: Manual overrides — user explicitly toggled (state, survives re-renders)
  // true = user forced expand, false = user forced collapse, absent = follow auto
  let manualOverrides = $state(new Map<string, boolean>());

  function toggleBurst(key: string) {
    const next = new Map(manualOverrides);
    const currentlyCollapsed = effectiveCollapsed.has(key);
    next.set(key, currentlyCollapsed); // if collapsed → override to expanded (true), vice versa
    manualOverrides = next;
  }

  // Layer 3: Effective collapsed set — merge auto + manual (derived)
  // Priority: needsInteraction (force expand) > manual > auto
  let effectiveCollapsed = $derived.by(() => {
    const result = new Set<string>();
    for (const [, burst] of toolBursts) {
      // Highest priority: interaction needed → always expand, ignore everything else
      const needsInteraction = burst.tools.some(
        (t) => t.status === "permission_prompt" || t.status === "ask_pending",
      );
      if (needsInteraction) continue;

      const manual = manualOverrides.get(burst.key);
      if (manual === true) continue; // user forced expand → skip
      if (manual === false) {
        // user forced collapse → add
        result.add(burst.key);
        continue;
      }
      if (autoCollapsed.has(burst.key)) {
        // no override → follow auto
        result.add(burst.key);
      }
    }
    return result;
  });

  // Indices hidden by collapsed bursts (for skipping render)
  let burstHiddenIndices = $derived.by(() => {
    const hidden = new Set<number>();
    for (const [, burst] of toolBursts) {
      if (effectiveCollapsed.has(burst.key)) {
        for (let j = burst.startIndex; j <= burst.endIndex; j++) hidden.add(j);
      }
    }
    return hidden;
  });

  // ── Per-run prompt drafts (issue #156) ──
  // PromptInput unmounts/remounts on run switch (it's gated behind a phase condition, and loadRun
  // flips phase to "running" mid-replay). So instead of stashing at transition time (unreliable —
  // promptRef is mid-churn), PromptInput reports every draft change via onDraftChange, and we seed
  // the freshly-mounted instance from this map via initialDraft (the `{#key}` forces a clean
  // remount per run). Keyed by run id; "" = the not-yet-started "new chat".
  let promptDraftsByRun = new Map<string, PromptInputSnapshot>();

  function savePromptDraft(snap: PromptInputSnapshot) {
    const rid = store.run?.id ?? "";
    const hasContent =
      snap.text.trim() ||
      snap.attachments.length ||
      snap.pastedBlocks.length ||
      (snap.pathRefs?.length ?? 0) > 0;
    if (hasContent) promptDraftsByRun.set(rid, snap);
    else promptDraftsByRun.delete(rid);
  }

  // ── Cumulative session token totals (from modelUsage, which is session-cumulative) ──
  // status bar shows session totals; per-turn values are in the turn separator annotations.
  let cumulativeTokens = $derived.by(() => {
    const mu = store.usage.modelUsage;
    if (!mu || Object.keys(mu).length === 0) {
      // No modelUsage yet — fall back to per-turn values (better than zero)
      return {
        input: store.usage.inputTokens,
        output: store.usage.outputTokens,
        cacheRead: store.usage.cacheReadTokens,
        cacheWrite: store.usage.cacheWriteTokens,
      };
    }
    let input = 0,
      output = 0,
      cacheRead = 0,
      cacheWrite = 0;
    for (const entry of Object.values(mu)) {
      input += entry.input_tokens;
      output += entry.output_tokens;
      cacheRead += entry.cache_read_tokens;
      cacheWrite += entry.cache_write_tokens;
    }
    return { input, output, cacheRead, cacheWrite };
  });

  // ── Agent-aware display helpers ──
  let isPiCodeRoute = $derived($page.url.pathname.startsWith("/chat/pi"));
  let computedEnabledAgents = $derived.by(() => {
    const raw = settings?.enabled_agents ?? ["pi"];
    const enabledSet = new Set(raw);
    enabledSet.add("pi");
    const result = VISIBLE_RUNTIME_PROVIDERS.filter((a) => enabledSet.has(a));
    return result.length > 0 ? result : ["pi"];
  });

  let isLegacyDshRun = $derived(store.run?.agent === "dsh");

  let effectiveAgent = $derived.by(() => {
    if (store.run?.agent) return store.run.agent;
    return "pi";
  });
  let effectiveCapabilities = $derived(store.capabilities);
  let goalPanelAvailable = $derived(
    effectiveCapabilities.ui.goalPanel ||
      effectiveCapabilities.protocol.goalState ||
      (effectiveAgent === "pi" && isPiFeatureAvailable(store.piCapabilities, "goalAvailable")),
  );
  let goalActionAvailable = $derived(
    effectiveAgent === "pi"
      ? isPiFeatureAvailable(store.piCapabilities, "goalAvailable")
      : effectiveCapabilities.ui.goalPanel || effectiveCapabilities.protocol.goalState,
  );
  let planActionAvailable = $derived(
    effectiveAgent === "pi"
      ? isPiFeatureAvailable(store.piCapabilities, "planAvailable")
      : effectiveCapabilities.ui.planModeToggle && effectiveCapabilities.protocol.planMode,
  );
  let piFeatureDiscoveryPending = $derived(
    effectiveAgent === "pi" && store.piCapabilities === null,
  );
  let effortAvailable = $derived(
    effectiveCapabilities.ui.effortSelector && effectiveCapabilities.protocol.effortControl,
  );
  let agentDisplayName = $derived(getAssistantDisplayName(effectiveAgent, t("chat_claude")));

  // Pi goal state is emitted by the extension's session entry. Keep the existing
  // GoalPanel reusable, but never invent an active state before Pi confirms it.
  let piGoalForPanel = $derived.by(() => {
    const state = store.piGoalState;
    if (state.phase === "unavailable") return null;
    const status: GoalStatus | undefined =
      state.phase === "budget_limited"
        ? "budgetLimited"
        : state.phase === "active" || state.phase === "paused" || state.phase === "complete"
          ? state.phase
          : undefined;
    return {
      objective: state.objective,
      status,
      tokenBudget: state.tokenBudget,
      tokensUsed: state.tokensUsed,
      timeUsedSeconds: state.timeUsedSeconds,
    };
  });
  // Capability discovery can beat the first structured goal-state refresh.
  // Treat that short-lived initial value as an idle, usable Goal control.
  let piGoalForComposer = $derived.by(() => {
    if (effectiveAgent !== "pi" || !store.piCapabilities?.goalAvailable) return null;
    return coercePiGoalStateForComposer(store.piGoalState, store.piCapabilities);
  });
  let goalActiveForComposer = $derived(
    effectiveAgent === "pi"
      ? isPiGoalChipActive(piGoalForComposer?.phase)
      : !!store.goal?.objective,
  );

  let resolvedContextWindow = $derived.by(() => {
    const binding = settings?.agent_provider_bindings?.[effectiveAgent as ProviderAgent];
    const model = store.run?.model?.trim() || store.model?.trim() || binding?.model?.trim() || "";
    const provider =
      binding?.mode === "custom" && binding.provider_id
        ? settings?.global_providers?.find((item) => item.id === binding.provider_id)
        : undefined;
    return resolveContextWindow({
      reportedWindow: store.contextWindow,
      model,
      models: [
        ...store.sessionModels,
        ...getModelsForAgent(effectiveAgent),
        ...globalModelsToCliModels(provider?.models),
      ],
      customProvider: binding?.mode === "custom",
    });
  });

  // ── Session info for InfoPanel ──
  let currentSessionInfo: SessionInfoData | null = $derived.by(() => {
    if (!store.run) return null;
    const { contextWindow, estimated: contextWindowEstimated } = resolvedContextWindow;
    const reportedContextTokens =
      store.contextTokens ||
      store.usage.inputTokens + store.usage.cacheReadTokens + store.usage.cacheWriteTokens;
    const contextUtilization =
      store.contextUtilization ||
      (contextWindow > 0 ? Math.min(reportedContextTokens / contextWindow, 1) : 0);
    return {
      sessionId: store.run.session_id,
      runId: store.run.id,
      runName: store.run.name,
      cwd: store.sessionCwd || store.run.cwd,
      numTurns: store.userTurnCount,
      status: store.run.status ?? "pending",
      startedAt: store.run.started_at ?? null,
      endedAt: store.run.ended_at ?? null,
      lastTurnDurationMs: store.durationMs,
      tokensEstimated: !store.usage.modelUsage || Object.keys(store.usage.modelUsage).length === 0,
      model:
        effectiveAgent === "codex"
          ? codexDisplayModel(store.run.model)
          : (store.run.model ?? store.model),
      agent: store.run.agent ?? store.agent,
      cliVersion: effectiveAgent === "codex" ? (getCodexVersion() ?? "") : store.cliVersion,
      // Pi has an extension-owned permission policy. The generic CLI mode can be
      // stale or unrelated (for example, bypassPermissions from a previous Claude
      // session), so the info panel must use the same source as the Pi composer.
      permissionMode: effectiveAgent === "pi" ? store.piPermissionState.mode : store.permissionMode,
      fastModeState: store.fastModeState,
      cost: store.usage.cost,
      inputTokens: cumulativeTokens.input,
      outputTokens: cumulativeTokens.output,
      cacheReadTokens: cumulativeTokens.cacheRead,
      cacheWriteTokens: cumulativeTokens.cacheWrite,
      contextWindow,
      contextWindowEstimated,
      contextUtilization,
      contextTokens: reportedContextTokens,
      contextBreakdown: store.contextBreakdown,
      compactCount: store.compactCount,
      microcompactCount: store.microcompactCount,
      mcpServers: store.mcpServers,
      remoteHostName: store.remoteHostName,
      platformId: store.platformId,
      cliUsageIncomplete: store.run.cli_usage_incomplete ?? false,
      runSource: store.run.source,
      authSourceLabel: store.authSourceLabel || undefined,
      platformName: platformDisplayName || undefined,
      cliUpdateAvailable:
        store.cliVersion && channelLatest && channelLatest !== store.cliVersion
          ? channelLatest
          : undefined,
    };
  });

  // ── Sidebar data ──
  let sidebarToolsCount = $derived(
    store.timeline.some((e) => e.kind === "tool")
      ? store.timeline.filter((e) => e.kind === "tool").length
      : store.tools.filter((e) => e.tool_name).length,
  );
  let hasSidebarData = $derived(
    !!(store.run || store.tools.length > 0 || store.timeline.some((e) => e.kind === "tool")),
  );

  // ── CLI version info (reactive — ensures heroMetaFooter re-renders after async load) ──
  let cliVersionInfo = $derived(getCliVersionInfo_cached());

  // ── CLI update channel ──
  let channelLatest = $derived.by(() => {
    if (!cliVersionInfo?.installed) return undefined;
    return cliVersionInfo.channel === "stable" ? cliVersionInfo.stable : cliVersionInfo.latest;
  });

  // ── Hero meta version (agent-correct) ──
  // Codex → codex-cli version (strip the "codex-cli " prefix + leading "v" so chat_cliVersion's
  // "v{version}" renders cleanly); Claude → its CLI version. The update-check is Claude-only.
  let heroCliVersion = $derived(
    effectiveAgent === "codex"
      ? (getCodexVersion() ?? "").replace(/^codex-cli\s*/i, "").replace(/^v/i, "")
      : (cliVersionInfo?.installed ?? ""),
  );
  let heroHasUpdate = $derived(
    effectiveAgent !== "codex" &&
      !!cliVersionInfo?.installed &&
      !!channelLatest &&
      cliVersionInfo.installed !== channelLatest,
  );

  // ── Platform display name ──
  let platformDisplayName = $derived.by(() => {
    const pid = store.platformId;
    if (!pid) return undefined;
    const preset = PLATFORM_PRESETS.find((p) => p.id === pid);
    return preset?.name ?? authOverview?.app_platform_name ?? pid;
  });

  // ── Provider-aware model list ──
  // Pi uses its own managed/current model and must never inherit Claude platform models.
  // Claude priority: credential.models (user-configured) > preset.models (static defaults).
  let activeProviderBinding = $derived.by(() => {
    if (!settings || !["claude", "codex", "pi", "grok", "dsh"].includes(effectiveAgent))
      return undefined;
    return settings.agent_provider_bindings?.[effectiveAgent as ProviderAgent];
  });

  let activeGlobalProvider = $derived.by(() => {
    const providerId = activeProviderBinding?.provider_id;
    if (providerId) {
      return settings?.global_providers?.find((item) => item.id === providerId);
    }
    if (["claude", "codex", "pi", "grok", "dsh"].includes(effectiveAgent)) {
      return settings?.global_providers?.find((item) =>
        isProviderCompatible(effectiveAgent as ProviderAgent, item.protocol),
      );
    }
    return undefined;
  });

  function globalModelsToCliModels(
    models: GlobalProviderModel[] | undefined,
    providerId?: string,
    providerName?: string,
  ): CliModelInfo[] {
    return (models ?? []).map((item, index) => ({
      value: providerId ? `${providerId}/${item.id}` : item.id,
      displayName: item.name?.trim() || item.id,
      providerId,
      providerName,
      description: index === 0 ? "Provider 默认模型" : providerName || "全局 Provider",
      contextWindow: item.context_window,
      maxTokens: item.max_tokens,
      supportsEffort: item.supports_reasoning,
      supportedEffortLevels: item.supports_reasoning
        ? effectiveAgent === "pi" || effectiveAgent === "dsh"
          ? getPiModelEffortLevels(true, item.supports_xhigh, item.supported_effort_levels)
          : ["off", "minimal", "low", "medium", "high", "xhigh"]
        : undefined,
    }));
  }

  let platformModels = $derived.by((): CliModelInfo[] => {
    if (activeGlobalProvider?.models?.length) {
      return globalModelsToCliModels(
        activeGlobalProvider.models,
        activeGlobalProvider.id,
        activeGlobalProvider.name,
      );
    }
    if (effectiveAgent === "pi") {
      return getPiModelOptions(settings?.pi_provider, store.model);
    }
    const pid = store.platformId;
    if (!pid || pid === "anthropic") return [];
    const cred = findCredential(settings?.platform_credentials ?? [], pid);
    const preset = PLATFORM_PRESETS.find((p) => p.id === pid);
    const models = cred?.models?.length ? cred.models : preset?.models;
    if (!models?.length) return [];
    return models.map((m, i) => ({
      value: m,
      displayName: m,
      description: i === 0 ? "Default" : "",
    }));
  });

  let effectiveModels = $derived.by(() => {
    // Gather all compatible global providers' models
    const globalOptions: CliModelInfo[] = [];
    const targetAgent = (
      ["claude", "codex", "pi", "grok", "dsh"].includes(effectiveAgent) ? effectiveAgent : "pi"
    ) as ProviderAgent;

    for (const provider of settings?.global_providers ?? []) {
      if (!isProviderCompatible(targetAgent, provider.protocol)) {
        continue;
      }
      for (const m of provider.models ?? []) {
        globalOptions.push({
          value: `${provider.id}/${m.id}`,
          displayName: m.name?.trim() || m.id,
          description: provider.name,
          providerId: provider.id,
          providerName: provider.name,
          contextWindow: m.context_window,
          maxTokens: m.max_tokens,
          supportsEffort: m.supports_reasoning,
          supportedEffortLevels: m.supports_reasoning
            ? effectiveAgent === "pi" || effectiveAgent === "dsh"
              ? getPiModelEffortLevels(true, m.supports_xhigh, m.supported_effort_levels)
              : m.supported_effort_levels
            : undefined,
        });
      }
    }

    const nativeModels = getModelsForAgent(effectiveAgent, {
      platformModels,
      liveModels: store.sessionModels,
    });

    const catalog = globalOptions.length > 0 ? globalOptions : nativeModels;
    const currentModel = (store.model ?? "").trim();
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
      const matchedProvider = settings?.global_providers?.find(
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

  // In managed-provider mode the provider binding is the source of truth for new chats without
  // an existing run or project preference, so seed the composer from its real default model.
  let managedProviderDefaultModel = $derived.by(() => {
    if (activeProviderBinding?.mode !== "custom") return "";
    const bare = resolveManagedProviderDefaultModel(
      activeProviderBinding,
      activeGlobalProvider?.models?.map((item) => item.id) ?? [],
    );
    if (!bare) return "";
    if (bare.includes("/")) return bare;
    const pid = activeProviderBinding?.provider_id;
    return pid ? `${pid}/${bare}` : bare;
  });

  $effect(() => {
    if (
      runId ||
      store.run ||
      store.model ||
      store.phase === "loading" ||
      !managedProviderDefaultModel
    ) {
      return;
    }
    store.model = managedProviderDefaultModel;
  });

  // If the currently selected model in an idle draft is no longer in the allowed effectiveModels list
  // (e.g. after provider settings were changed and models unchecked), auto-correct it to the default model.
  $effect(() => {
    if (
      store.phase === "loading" ||
      store.sessionAlive ||
      store.run ||
      effectiveModels.length === 0 ||
      !store.model
    ) {
      return;
    }
    if (!effectiveModels.some((m) => m.value === store.model)) {
      const fallback = managedProviderDefaultModel || effectiveModels[0]?.value;
      if (fallback && fallback !== store.model) {
        dbg("chat", "auto-correct model to valid effective model", {
          agent: effectiveAgent,
          prev: store.model,
          next: fallback,
        });
        store.model = fallback;
      }
    }
  });

  // Reconcile a bare/partial model string (e.g. "zhanlu/kimi-k3") to the canonical
  // catalog model value (e.g. "provider-1788769121841/zhanlu/kimi-k3").
  $effect(() => {
    const cur = (store.model ?? "").trim();
    if (!cur || effectiveModels.length === 0) return;
    if (cur.includes("/") && effectiveModels.some((m) => m.value === cur)) return;
    const match = effectiveModels.find((m) => {
      const bareId =
        m.providerId && m.value.startsWith(`${m.providerId}/`)
          ? m.value.slice(m.providerId.length + 1)
          : m.value.includes("/")
            ? m.value.slice(m.value.indexOf("/") + 1)
            : m.value;
      return (
        bareId === cur ||
        bareId.toLowerCase() === cur.toLowerCase() ||
        m.displayName === cur ||
        m.displayName.toLowerCase() === cur.toLowerCase()
      );
    });
    if (match && match.value !== cur) {
      dbg("chat", "reconcile model to canonical catalog value", {
        prev: cur,
        next: match.value,
      });
      store.model = match.value;
      if (store.run) {
        store.run = { ...store.run, model: match.value };
      }
    }
  });

  let currentEffort = $state("");
  // Async settings/session hydration must not overwrite an explicit effort choice made by the
  // user while that hydration is still in flight.
  let effortSelectionVersion = 0;

  function conversationEffortStorageKey(runId: string): string {
    return `agentcabin:conversation-effort:${runId}`;
  }

  function loadConversationEffort(runId: string): string | undefined {
    if (typeof window === "undefined" || !runId) return undefined;
    try {
      const value = window.localStorage.getItem(conversationEffortStorageKey(runId));
      return value?.trim() || undefined;
    } catch {
      return undefined;
    }
  }

  function saveConversationEffort(runId: string, effort: string): void {
    if (typeof window === "undefined" || !runId) return;
    try {
      window.localStorage.setItem(conversationEffortStorageKey(runId), effort);
    } catch {
      // Local storage is an enhancement; the runtime/project preference remains authoritative.
    }
  }

  // An explicit project model/thinking preference is used as the default for new conversations
  // in the same left-sidebar project and Agent. Existing conversations keep their own choice.
  let projectPreferenceCwd = $derived.by(() => {
    if (codeStandaloneTask || store.run?.code_standalone_task) return "";
    return (
      store.effectiveCwd ||
      folderCwdOverride ||
      (typeof window !== "undefined" ? getSavedProjectCwd(currentRealm) || "" : "")
    );
  });
  let projectModelPreferenceKey = $derived.by(() => {
    return projectPreferenceCwd
      ? getProjectModelPreferenceKey(
          projectPreferenceCwd,
          store.remoteHostName ?? "",
          effectiveAgent,
        )
      : "";
  });
  let projectModelPreference = $state<ProjectModelPreference | undefined>();
  let projectModelPreferenceAppliedToken = "";
  let projectModelPreferenceWrite: Promise<void> = Promise.resolve();

  // Existing conversations own their effort. The project preference is only the fallback for
  // conversations that have not selected a value yet; never let another conversation overwrite
  // the value displayed for this run.
  let conversationEffortAppliedToken = "";
  $effect(() => {
    const runId = store.run?.id ?? "";
    const runEffort = store.run?.effort?.trim() ?? "";
    const projectEffort = projectModelPreference?.effort ?? "";
    const token = `${runId}\u0002${runEffort}\u0002${projectEffort}`;
    if (!runId || token === conversationEffortAppliedToken) return;
    conversationEffortAppliedToken = token;
    const saved = loadConversationEffort(runId);
    currentEffort = saved || runEffort || projectEffort;
  });

  $effect(() => {
    const key = projectModelPreferenceKey;
    const cwd = projectPreferenceCwd;
    const remoteHostName = store.remoteHostName ?? "";
    const agent = effectiveAgent;
    projectModelPreference = undefined;
    projectModelPreferenceAppliedToken = "";
    if (!key || !cwd) return;
    void api
      .getProjectPreferences(cwd, remoteHostName, agent)
      .then((preference) => {
        if (projectModelPreferenceKey === key) projectModelPreference = preference;
      })
      .catch((error) => dbgWarn("chat", "failed to load project preferences", error));
  });

  function persistProjectModelPreference(patch: {
    model?: string;
    effort?: string;
    permission_mode?: string;
  }): Promise<void> {
    const cwd = projectPreferenceCwd;
    const remoteHostName = store.remoteHostName ?? "";
    const agent = effectiveAgent;
    if (!cwd) return Promise.resolve();

    projectModelPreference = {
      ...(projectModelPreference ?? {}),
      ...patch,
    };
    projectModelPreferenceAppliedToken = "";

    projectModelPreferenceWrite = projectModelPreferenceWrite
      .catch(() => {})
      .then(async () => {
        try {
          const saved = await api.updateProjectPreferences(cwd, remoteHostName, agent, patch);
          if (projectPreferenceCwd === cwd && effectiveAgent === agent) {
            projectModelPreference = saved;
          }
        } catch (e) {
          dbgWarn("chat", "failed to persist project preferences", e);
          throw e;
        }
      });

    return projectModelPreferenceWrite;
  }

  // Project preferences (model, effort, permission mode) apply across all conversations in the same
  // project so that switching between chats or starting a new chat maintains consistent settings.
  $effect(() => {
    const key = projectModelPreferenceKey;
    const runId = store.run?.id ?? "draft";
    const phase = store.phase;
    const catalog = effectiveModels.map((model) => model.value).join("\u0001");
    const preference = projectModelPreference;
    const token = [
      key,
      runId,
      phase,
      catalog,
      preference?.model ?? "",
      preference?.effort ?? "",
      preference?.permission_mode ?? "",
    ].join("\u0002");

    if (
      !key ||
      !preference ||
      phase === "loading" ||
      token === projectModelPreferenceAppliedToken
    ) {
      return;
    }
    projectModelPreferenceAppliedToken = token;
    if (preference.permission_mode) {
      if (effectiveAgent === "pi") {
        const normalized = normalizePiPermissionMode(preference.permission_mode);
        if (store.piPermissionState.mode !== normalized) {
          store.piPermissionState = {
            ...store.piPermissionState,
            mode: normalized,
            restartRequired: false,
          };
          if (
            store.sessionAlive &&
            store.run?.id &&
            effectiveCapabilities.protocol.permissionModeControl
          ) {
            void api.setPermissionMode(store.run.id, normalized).catch(() => {});
          }
        }
      } else {
        if (store.permissionMode !== preference.permission_mode) {
          store.permissionMode = preference.permission_mode;
          store.permissionModeSetByUser = true;
          if (
            store.sessionAlive &&
            store.run?.id &&
            effectiveCapabilities.protocol.permissionModeControl
          ) {
            void api.setPermissionMode(store.run.id, preference.permission_mode).catch(() => {});
          }
        }
      }
    }
  });

  // Effort guard: auto-clear effort when model doesn't support it;
  // also auto-populate default effort ("high") when empty and model supports it.
  $effect(() => {
    if (!effortAvailable) return;

    // Codex, Pi, DSH and Grok keep an agent-scoped startup preference. Empty means
    // "use the CLI/model default" (the spawn path skips the flag).
    if (
      effectiveAgent === "codex" ||
      effectiveAgent === "pi" ||
      effectiveAgent === "dsh" ||
      effectiveAgent === "grok"
    )
      return;

    const pid = store.platformId;
    // Third-party platform: don't touch effort
    if (pid && pid !== "anthropic") return;

    const modelInfo = effectiveModels.find((m) => m.value === store.model);
    if (!modelInfo) return; // models not loaded yet

    if (currentEffort && modelInfo.supportsEffort === false) {
      // Model doesn't support effort → clear
      dbg("chat", "effort-guard: clearing for unsupported model", { model: store.model });
      currentEffort = "";
      api.updateCliConfig({ effortLevel: null }).catch((e) => {
        dbgWarn("chat", "effort-guard: CLI config clear failed", e);
      });
    } else if (modelInfo.supportsEffort === true) {
      const allowed = modelInfo.supportedEffortLevels;
      // If no effort is set, or current effort is not supported by this model, correct it
      if (!currentEffort || (allowed && allowed.length > 0 && !allowed.includes(currentEffort))) {
        const fallback = allowed?.includes("high") ? "high" : (allowed?.[0] ?? "high");
        dbg("chat", "effort-guard: defaulting or correcting effort", {
          model: store.model,
          from: currentEffort,
          to: fallback,
        });
        currentEffort = fallback;
        api.updateCliConfig({ effortLevel: fallback }).catch((e) => {
          dbgWarn("chat", "effort-guard: CLI config default failed", e);
        });
      }
    }
  });

  // Auto-focus the input when the run changes
  $effect(() => {
    const _ = store.run?.id;
    // Auto-focus the prompt input when entering a session
    requestAnimationFrame(() => promptRef?.focus());
  });

  // Sync verbose state from CLI config when run changes (or on retry)
  $effect(() => {
    const _tick = verboseRetryTick; // extra dep: drives retry on failure
    syncVerboseState(store.run?.id);
  });

  // ── Progressive timeline rendering ── helpers

  /** Invalidate any in-flight async load so its post-await side effects bail out. */
  function cancelProgressive() {
    progressiveGen++;
  }

  /** Bump the load generation and return it — caller compares against `progressiveGen`. */
  function nextProgressiveGen(): number {
    return ++progressiveGen;
  }

  /**
   * Expand `renderLimit` enough that `targetIndex` (in `filteredTimeline`) is mounted,
   * with `margin` extra entries above for context. No-op if already covered.
   */
  function expandRenderLimitTo(targetIndex: number, margin = 50) {
    const ft = filteredTimeline;
    if (targetIndex < 0 || targetIndex >= ft.length) return;
    if (renderLimit === Infinity) return;
    const needed = ft.length - targetIndex + margin;
    if (renderLimit < needed) renderLimit = Math.min(needed, ft.length);
  }

  /**
   * If `visibleIdx` (visibleTimeline-local) sits inside a collapsed tool burst,
   * force-expand that burst via `manualOverrides` so the entry's DOM mounts.
   * Caller must pass an index in the visibleTimeline namespace, not filteredTimeline.
   */
  async function ensureBurstExpandedFor(visibleIdx: number) {
    if (!burstHiddenIndices.has(visibleIdx)) return;
    for (const [, burst] of toolBursts) {
      if (visibleIdx >= burst.startIndex && visibleIdx <= burst.endIndex) {
        const next = new Map(manualOverrides);
        next.set(burst.key, true);
        manualOverrides = next;
        await tick();
        return;
      }
    }
  }

  // Sentinel above the visible list — when it intersects the chat viewport, grow renderLimit.
  let topSentinel = $state<HTMLDivElement | null>(null);
  let _topObserver: IntersectionObserver | null = null;

  $effect(() => {
    if (!topSentinel || !chatAreaRef) {
      _topObserver?.disconnect();
      _topObserver = null;
      return;
    }
    _topObserver?.disconnect();
    _topObserver = new IntersectionObserver(
      (entries) => {
        const entry = entries[0];
        if (!entry?.isIntersecting) return;
        if (!loadMoreArmed || loadingMore) return;
        const hidden = filteredTimeline.length - renderLimit;
        if (hidden <= 0) return;
        dbg("chat", "progressive-load-more", { renderLimit, hidden });
        void loadMoreEarlier();
      },
      { root: chatAreaRef, rootMargin: "200px 0px 0px 0px", threshold: 0 },
    );
    _topObserver.observe(topSentinel);
    return () => {
      _topObserver?.disconnect();
      _topObserver = null;
    };
  });

  /**
   * Grow `renderLimit` by `RENDER_GROWTH_STEP` and preserve the user's scroll position.
   * Browser-native scroll anchoring is unreliable on WKWebView with content-visibility,
   * so we measure the first rendered entry's offset before/after and adjust scrollTop
   * by the delta.
   */
  async function loadMoreEarlier() {
    if (loadingMore || !loadMoreArmed) return;
    loadingMore = true;
    loadMoreArmed = false; // re-armed by handleChatScroll on next user scroll
    try {
      const anchor = chatAreaRef?.querySelector<HTMLElement>("[data-entry-id]") ?? null;
      const anchorId = anchor?.dataset.entryId ?? null;
      const beforeTop = anchor?.getBoundingClientRect().top ?? 0;
      const beforeScroll = chatAreaRef?.scrollTop ?? 0;

      renderLimit = Math.min(renderLimit + RENDER_GROWTH_STEP, filteredTimeline.length);
      await tick();
      await yieldToMain();

      if (anchorId && chatAreaRef) {
        let after: HTMLElement | null = null;
        try {
          after = chatAreaRef.querySelector<HTMLElement>(
            `[data-entry-id="${CSS.escape(anchorId)}"]`,
          );
        } catch {
          after =
            Array.from(chatAreaRef.querySelectorAll<HTMLElement>("[data-entry-id]")).find(
              (el) => el.dataset.entryId === anchorId,
            ) ?? null;
        }
        if (after) {
          const afterTop = after.getBoundingClientRect().top;
          // Suppress re-arm so the programmatic scrollTop write doesn't immediately
          // rearm the observer — the sentinel may still be in view post-prepend.
          _suppressLoadMoreRearm = true;
          chatAreaRef.scrollTop = beforeScroll + (afterTop - beforeTop);
          // Clear suppression after the scroll event has dispatched + been handled.
          // yieldToMain has a 50ms timeout fallback so a backgrounded WebView with
          // throttled rAF can't strand the suppression flag.
          await yieldToMain();
          _suppressLoadMoreRearm = false;
        }
      }
    } finally {
      loadingMore = false;
    }

    // IntersectionObserver only refires on state changes. If anchor compensation
    // produced ~zero delta (collapsed/unmeasured entries above the viewport, or
    // a stable layout where the prepended content all sits within rootMargin),
    // the sentinel stays "still intersecting" with no new state to deliver — and
    // the user-scroll re-arm path (handleChatScroll) never fires either because
    // a same-value scrollTop write may dispatch no scroll event. Re-observing
    // forces a fresh callback delivery: if the sentinel exited the viewport,
    // the callback returns early; if still intersecting, it kicks off another
    // batch, naturally terminating once `filteredTimeline.length <= renderLimit`
    // or the sentinel exits.
    if (_topObserver && topSentinel && filteredTimeline.length > renderLimit) {
      loadMoreArmed = true;
      _topObserver.unobserve(topSentinel);
      _topObserver.observe(topSentinel);
    }
  }

  /**
   * Load a run and render its timeline progressively.
   * Starts with the most recent `INITIAL_RENDER_LIMIT` entries; the top sentinel
   * grows `renderLimit` as the user scrolls up.
   */
  async function loadRunProgressive(
    id: string,
    xtermRef?: { clear(): void; writeText(s: string): void },
  ) {
    // Reset progressive state so a previous run's expanded renderLimit (e.g. after
    // an anchor jump) doesn't leak into the new run.
    renderLimit = INITIAL_RENDER_LIMIT;
    loadingMore = false;
    loadMoreArmed = true;
    const gen = nextProgressiveGen();

    // Capture scrollTo BEFORE loadRun — URL may change during async load
    const scrollTo = $page.url.searchParams.get("scrollTo");

    // Flag suppresses auto-scroll reset during loadRun (non-reactive, won't trigger effects)
    if (scrollTo) _scrollToInFlight = true;

    await store.loadRun(id, xtermRef);
    if (store.run) {
      const target = getAgentTarget(store.run);
      if (isWorkRun(store.run)) {
        const params = new URLSearchParams();
        if (store.run.workspace_id) params.set("workspace", store.run.workspace_id);
        params.set("run", store.run.id);
        await goto(`/chat/work?${params.toString()}`, { replaceState: true });
        return;
      }
      if (target === "pi:code" && !$page.url.pathname.startsWith("/chat/pi")) {
        const params = new URLSearchParams();
        params.set("run", store.run.id);
        if (scrollTo) params.set("scrollTo", scrollTo);
        await goto(`/chat/pi?${params.toString()}`, { replaceState: true });
        return;
      }
      if (isNativeTarget(target) && $page.url.pathname.startsWith("/chat/pi")) {
        const params = new URLSearchParams();
        params.set("run", store.run.id);
        if (scrollTo) params.set("scrollTo", scrollTo);
        await goto(`/chat?${params.toString()}`, { replaceState: true });
        return;
      }
    }
    if (id) folderCwdOverride = ""; // clear folder override when a real run loads

    // Reload project data with the run's cwd
    if (id && store.effectiveCwd) {
      reloadProjectData(store.effectiveCwd);
    }

    // Cross-reference MCP servers with config disabled state
    // (session_init event carries stale status from session start time)
    if (store.mcpServers.length > 0) {
      try {
        const disabledNames = await api.getDisabledMcpServers();
        if (disabledNames.length > 0) {
          const disabledSet = new Set(disabledNames);
          const patched = store.mcpServers.map((s) =>
            disabledSet.has(s.name) && s.status !== "disabled" ? { ...s, status: "disabled" } : s,
          );
          if (patched.some((s, i) => s !== store.mcpServers[i])) {
            store.updateMcpServers(patched);
            dbg("chat", "patched MCP disabled state", { disabledNames });
          }
        }
      } catch {
        // non-critical, ignore
      }
    }

    if (gen !== progressiveGen) return;
    dbg("chat", "loadRun complete", {
      timeline: filteredTimeline.length,
      renderLimit,
      gen,
    });

    if (scrollTo) {
      await tick();
      scrollToMessage(scrollTo);
      _scrollToInFlight = false;
      // Clean scrollTo from URL — safe because $effect depends on
      // runId and hasResumeParam (primitives), not the full $page.url.
      const clean = new URL($page.url);
      clean.searchParams.delete("scrollTo");
      replaceState(clean, {});
    } else {
      // Scroll to bottom after DOM update — ensures content-visibility triggers re-layout
      await tick();
      requestAnimationFrame(() => {
        if (chatAreaRef) chatAreaRef.scrollTop = chatAreaRef.scrollHeight;
      });
    }
  }

  let welcomeVisible = $derived(
    store.timeline.length === 0 && !store.streamingText && !store.run && store.phase !== "loading",
  );

  // Welcome page project name (derived for template use)
  let welcomeProjectName = $derived.by(() => {
    if (codeStandaloneTask || store.run?.code_standalone_task) return "独立任务";
    const cwd = store.effectiveCwd || folderCwdOverride || getSavedProjectCwd(currentRealm) || "";
    return cwd ? cwdDisplayLabel(cwd) : "";
  });

  let inputBlockedByPermission = $derived(store.hasPendingPermission || store.hasElicitation);
  let pendingToolPermissions = $derived(store.pendingToolPermissions);
  let showPermissionPanel = $derived(pendingToolPermissions.length > 0 && store.sessionAlive);

  /** Skill info for SkillSelector: merge preloaded details with session skill names. */
  let skillItems = $derived.by(() => {
    const detailMap = new Map(preloadedCapabilitySkills.map((s) => [s.name, s]));
    const capabilitySkillNames = preloadedCapabilitySkills.map((skill) => skill.name);
    const isBlankConversation = !store.run && store.timeline.length === 0 && !store.sessionAlive;
    const names = isBlankConversation
      ? capabilitySkillNames
      : effectiveAgent === "pi"
        ? [...new Set([...store.availableSkills, ...capabilitySkillNames])]
        : store.availableSkills;
    // Before a session exists, the Capability Center list is authoritative. Once Claude has
    // initialized, its negotiated skill list is the live runtime source.
    if (names.length > 0 || (effectiveAgent === "claude" && store.sessionInitReceived)) {
      return names
        .map((name) => ({
          name,
          description: detailMap.get(name)?.description ?? "",
        }))
        .filter((item) => !isExpertSkill(item));
    }
    return toCapabilitySkillItems(preloadedCapabilitySkills).filter((item) => !isExpertSkill(item));
  });

  let piAvailableSkillNames = $derived(
    (() => {
      const isBlankConversation = !store.run && store.timeline.length === 0 && !store.sessionAlive;
      if (effectiveAgent !== "pi") return store.availableSkills;
      if (isBlankConversation) return preloadedCapabilitySkills.map((skill) => skill.name);
      return [
        ...new Set([
          ...store.availableSkills,
          ...preloadedCapabilitySkills.map((skill) => skill.name),
        ]),
      ];
    })(),
  );

  /**
   * Native provider command catalog used by both cold-start and live sessions.
   * AgentCabin virtual actions and skill entries are intentionally not added here.
   */
  let nativeSlashCommands = $derived.by(() => {
    const baseline =
      effectiveAgent === "claude"
        ? getCliCommands()
        : effectiveAgent === "pi"
          ? [...getPiColdStartNativeCommands(), ...preloadedPiCommands]
          : effectiveAgent === "codex"
            ? getCodexColdStartNativeCommands()
            : effectiveAgent === "grok"
              ? getGrokColdStartNativeCommands()
              : [];
    const discovered = store.sessionInitReceived
      ? store.sessionCommands
      : (store.nativeSlashCommandsByAgent[effectiveAgent] ?? []);
    return mergeNativeSlashCommands(baseline, discovered);
  });

  /** Race guard for the runtime skills query (run can switch mid-flight). */
  let codexSkillsGen = 0;
  /** Source Codex skills from the live runtime or the global Capability Center list. */
  $effect(() => {
    const runId = store.run?.id;
    const live = effectiveAgent === "codex" && store.sessionAlive && !!runId;
    if (effectiveAgent !== "codex") {
      if (codexRuntimeSkills.length > 0) codexRuntimeSkills = [];
      return;
    }
    const gen = ++codexSkillsGen;
    if (live) {
      dbg("skills", "fetch runtime skills", { runId });
      api
        .listCodexSkillsRuntime(runId!)
        .then((res) => {
          if (gen !== codexSkillsGen) return; // run/session changed while in flight
          // Flatten every cwd entry's enabled skills; only enabled ones can actually trigger.
          const flat = res.data.flatMap((entry) =>
            entry.skills
              .filter((s) => s.enabled)
              .map((s) => ({
                name: s.name,
                path: s.path,
                description: s.shortDescription ?? s.description ?? "",
              })),
          );
          codexRuntimeSkills = flat;
          dbg("skills", "runtime skills loaded", { count: flat.length });
        })
        .catch((e) => dbgWarn("skills", "listCodexSkillsRuntime failed", e));
    } else {
      // There is no Codex process before the first message. Reuse the global
      // Capability Center preload instead of scanning Codex's historical roots
      // (for example ~/.agents/skills) and presenting skills that cannot be sent.
      const flat = toCodexCapabilitySkillItems(preloadedCapabilitySkills);
      codexRuntimeSkills = flat;
      dbg("skills", "pre-session capability skills loaded", {
        mode: currentHarness,
        count: flat.length,
      });
    }
  });

  /** Fill the Pi picker before the first session; live RPC upgrades it later. */
  $effect(() => {
    if (effectiveAgent !== "pi" || store.sessionAlive) return;
    dbg("models", "preloading pi catalog before session");
    void loadPiModels();
  });

  /** Grok owns its model registry; resolve its concrete native default before the first prompt. */
  $effect(() => {
    if (effectiveAgent !== "grok" || store.sessionAlive) return;
    dbg("models", "preloading grok catalog before session");
    void loadGrokModels();
  });

  $effect(() => {
    if (effectiveAgent !== "grok" || store.sessionAlive || store.model) return;
    const configuredModel =
      activeProviderBinding?.model?.trim() || activeProviderBinding?.models?.[0]?.trim();
    const defaultModel = configuredModel || getGrokModels()[0]?.value;
    if (defaultModel) {
      dbg("models", "selecting Grok native default model", { model: defaultModel });
      store.model = defaultModel;
    }
  });

  /** DSH SDK initialization requires an explicit provider/model route. */
  $effect(() => {
    if (effectiveAgent !== "dsh" || store.sessionAlive || store.model) return;
    const defaultModel = effectiveModels[0]?.value;
    if (defaultModel) {
      dbg("models", "selecting DSH default provider model", { model: defaultModel });
      store.model = defaultModel;
    }
  });

  /** Upgrade live model catalogs once the agent actor is ready. */
  $effect(() => {
    const runId = store.run?.id;
    if (!store.sessionAlive || !runId) return;
    if (effectiveAgent === "codex") {
      dbg("models", "live session up, refreshing codex catalog", { runId });
      void loadCodexModelsLive(runId);
    } else if (effectiveAgent === "pi") {
      dbg("models", "live session up, refreshing pi catalog", { runId });
      void loadPiModelsLive(runId);
    }
  });

  // ── Per-turn usage annotations in timeline ──

  let usageByTurn = $derived(new Map(store.turnUsages.map((tu) => [tu.turnIndex, tu])));

  /** Prefix-sum of user message count across filteredTimeline (for progressive rendering offset). */
  let userCountPrefix = $derived.by(() => {
    const ft = filteredTimeline;
    const arr = new Int32Array(ft.length + 1);
    for (let i = 0; i < ft.length; i++) {
      arr[i + 1] = arr[i] + (ft[i].kind === "user" ? 1 : 0);
    }
    return arr;
  });

  /** Map of visibleTimeline index → TurnUsage to show BEFORE this entry (turn boundary). */
  let usageAnnotations = $derived.by(() => {
    const map = new Map<number, TurnUsage>();
    if (usageByTurn.size === 0) return map;
    const vt = visibleTimeline;
    const hidden = filteredTimeline.length - vt.length;
    let userCount = userCountPrefix[hidden];
    for (let i = 0; i < vt.length; i++) {
      if (vt[i].kind === "user") {
        if (userCount > 0) {
          const tu = usageByTurn.get(userCount);
          if (tu) map.set(i, tu);
        }
        userCount++;
      }
    }
    return map;
  });

  // ── Fork overlay ──
  let forkOverlay = $state<{
    active: boolean;
    sourceRunId: string;
    startedAt: number;
    error: string | null;
  } | null>(null);
  let forkElapsed = $state(0);

  /** Slash command processing indicator. */
  let processingSlashCmd = $state<string | null>(null);
  let slashCmdSeenRunning = $state(false);

  $effect(() => {
    if (!processingSlashCmd) return;
    // Track: phase was "running" at some point since flag was set
    if (store.isRunning) slashCmdSeenRunning = true;
    // Clear when content arrives, error set, or turn completed (idle after running)
    if (
      store.streamingText ||
      store.thinkingText ||
      store.error ||
      store.phase === "failed" ||
      store.phase === "completed" ||
      store.phase === "stopped" ||
      store.phase === "idle" ||
      !store.isRunning
    ) {
      processingSlashCmd = null;
      slashCmdSeenRunning = false;
    }
  });

  // Fork overlay timer: tick elapsed seconds while active
  $effect(() => {
    if (forkOverlay?.active && !forkOverlay.error) {
      const interval = setInterval(() => {
        forkElapsed = Math.floor((Date.now() - forkOverlay!.startedAt) / 1000);
      }, 1000);
      return () => clearInterval(interval);
    } else {
      forkElapsed = 0;
    }
  });

  // Fork overlay phase watcher: show error on failure during step 1 (fork_oneshot).
  // Overlay is dismissed explicitly by handleResume after step 1 succeeds.
  // Guard `!forkOverlay.error`: only set error once to prevent infinite $effect loop —
  // writing `forkOverlay = { ...spread }` creates a new object ref that re-triggers the effect.
  $effect(() => {
    if (!forkOverlay?.active) return;
    const phase = store.phase;
    if ((phase === "failed" || phase === "stopped") && !forkOverlay.error) {
      forkOverlay = { ...forkOverlay, error: store.error || t("chat_forkFailedFallback") };
    }
  });

  // Task notification: auto-show and dismiss after 5s
  $effect(() => {
    const notifications = store.taskNotifications;
    if (notifications.size === 0) return;
    const latest = Array.from(notifications.values()).pop();
    if (!latest) return;
    latestNotification = { task_id: latest.task_id, status: latest.status };
    notificationVisible = true;
    const timer = setTimeout(() => {
      notificationVisible = false;
    }, 5000);
    return () => clearTimeout(timer);
  });

  function formatElapsed(seconds: number): string {
    if (seconds < 60) return `${seconds}s`;
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  // ── URL-derived (primitive values only — avoids $effect re-trigger on unrelated URL changes) ──
  let runId = $derived($page.url.searchParams.get("run") ?? "");
  let codeAsideRunId = runId;
  const codeAsideOpenByRun = new Map<string, boolean>();

  function setCodeAsideOpen(open: boolean) {
    codeAsideOpen = open;
    codeAsideOpenByRun.set(runId, open);
  }

  // Keep the Code aside visibility with its conversation as the chat page is
  // reused while switching between runs.
  $effect(() => {
    const activeRunId = runId;
    if (activeRunId === codeAsideRunId) return;
    codeAsideOpenByRun.set(codeAsideRunId, codeAsideOpen);
    codeAsideRunId = activeRunId;
    codeAsideOpen = codeAsideOpenByRun.get(activeRunId) ?? false;
    turnReviewOpen = false;
    turnReviewDiff = "";
  });

  let hasResumeParam = $derived($page.url.searchParams.has("resume"));
  let folderParam = $derived($page.url.searchParams.get("folder"));
  let hostParam = $derived($page.url.searchParams.get("host"));
  let agentParam = $derived($page.url.searchParams.get("agent"));
  let tabParam = $derived($page.url.searchParams.get("tab"));

  // Consume ?tab= param: expand right sidebar and switch tab
  $effect(() => {
    const t = tabParam;
    if (!t) return;
    untrack(() => {
      const clean = new URL($page.url);
      clean.searchParams.delete("tab");
      replaceState(clean, {});
      if (t === "files" || t === "tools" || t === "info" || t === "tasks") {
        sidebarCollapsed = false;
        sidebarRequestedTab = t as "tools" | "files" | "info" | "tasks";
      }
    });
  });

  // Consume ?agent= param: switch agent for new sessions, then clean URL
  $effect(() => {
    const a = agentParam;
    if (!a) return;
    untrack(() => {
      const clean = new URL($page.url);
      clean.searchParams.delete("agent");
      replaceState(clean, {});
      // ?agent= only applies to new sessions — ignore when loading an existing run
      if (!runId && isKnownAgent(a)) {
        handleAgentChange(a, true); // force=true to run full side effects
      }
    });
  });

  let lastHarnessForAgent = $state<"code" | "work" | null>(null);
  let wasInExistingRun = $state(false);

  // Keep store.agent synchronized with default runtime for fresh sessions
  $effect(() => {
    const isFresh = !runId && !store.run;
    if (isFresh && settings) {
      const defaultRuntime =
        currentHarness === "work"
          ? settings.work_default_runtime || "pi"
          : settings.code_default_runtime || settings.default_agent || "pi";

      if (lastHarnessForAgent !== currentHarness || wasInExistingRun || !store.agent) {
        lastHarnessForAgent = currentHarness;
        wasInExistingRun = false;
        if (store.agent !== defaultRuntime) {
          store.agent = defaultRuntime;
          void handleAgentChange(defaultRuntime, true);
        }
      }
    } else if (runId || store.run) {
      wasInExistingRun = true;
    }
  });

  // Consume ?folder= params for local Code projects, then clean the URL.
  // Code intentionally does not accept a remote host target.
  $effect(() => {
    const folder = folderParam;
    const host = hostParam;
    if (!folder && !host) return;
    untrack(() => {
      dbg("chat", "local Code URL params", { folder, ignoredHost: host });
      if (host !== null) {
        dbgWarn("chat", "ignoring remote host URL param in local-only Code flow", { host });
        if (!runId && !store.run) {
          store.remoteHostName = null;
          setLastTarget(null);
        }
      }
      if (folder) {
        setSavedProjectCwd(folder, currentRealm);
        folderCwdOverride = folder;
        projectModelPreferenceAppliedToken = ""; // 新项目 cwd → 允许 hydrate 重新应用
        store.loadRun("", xtermRef);
      }
      const clean = new URL($page.url);
      clean.searchParams.delete("folder");
      clean.searchParams.delete("host");
      replaceState(clean, {});
      requestAnimationFrame(() => promptRef?.focus());
    });
  });

  // Example prompts for empty state
  const examplePrompts = [
    () => t("chat_examplePrompt1"),
    () => t("chat_examplePrompt2"),
    () => t("chat_examplePrompt3"),
  ];

  // ── Lifecycle ──

  function getProjectCwdForEditor(): string {
    if (codeStandaloneTask || store.run?.code_standalone_task) return "";
    return (
      store.effectiveCwd ||
      folderCwdOverride ||
      (typeof window !== "undefined" ? getSavedProjectCwd(currentRealm) || "" : "")
    );
  }

  async function refreshVscodeAvailability() {
    if (!getTransport().isDesktop()) {
      vscodeAvailable = false;
      return;
    }
    try {
      vscodeAvailable = await api.checkVscodeAvailable();
    } catch (e) {
      vscodeAvailable = false;
      dbgWarn("chat", "VS Code detection failed", e);
    }
  }

  async function openProjectInVscode() {
    const cwd = getProjectCwdForEditor();
    if (!cwd) {
      showChatToast(t("editor_noProject"));
      return;
    }
    if (store.remoteHostName) {
      showChatToast(t("editor_remoteUnsupported"));
      return;
    }

    try {
      // Re-check on click so installing VS Code while AgentCabin is open is
      // reflected immediately in the menu.
      vscodeAvailable = await api.checkVscodeAvailable();
      if (!vscodeAvailable) {
        showChatToast(t("editor_vscodeNotInstalled"));
        return;
      }
      await api.openProjectInVscode(cwd);
      showChatToast(t("editor_vscodeOpened"));
    } catch (e) {
      dbgWarn("chat", "open project in VS Code failed", e);
      showChatToast(t("editor_vscodeOpenFailed"));
    }
  }

  onMount(() => {
    void refreshVscodeAvailability();
  });

  // Clean up PTY session when the component is destroyed
  onMount(() => {
    return () => killAllPtySessions();
  });

  // Load settings
  onMount(async () => {
    const hydrationEffortVersion = effortSelectionVersion;
    try {
      settings = await api.getUserSettings();
      store.authMode = settings.auth_mode ?? "cli";
      // Fresh session: honor the user's default runtime per harness. Existing
      // runs are unaffected — loadRun sets store.agent from the run's own agent.
      if (!runId && !store.run) {
        const defaultRuntime =
          currentHarness === "work"
            ? settings.work_default_runtime || "pi"
            : settings.code_default_runtime || settings.default_agent || "pi";
        store.agent = defaultRuntime;
        void handleAgentChange(defaultRuntime, true);
      }
      // Initialize per-session platform from global active
      // Only use active_platform_id in App API Key mode; CLI Auth manages its own connection
      if (!store.platformId) {
        store.platformId =
          settings.auth_mode === "api" ? (settings.active_platform_id ?? "anthropic") : "anthropic";
      }
      // Initialize model: for managed providers (all agents), initialize binding model
      // for a fresh session before async CLI catalog resolution.
      const initialBinding =
        settings.agent_provider_bindings?.[store.agent as "claude" | "codex" | "pi" | "grok"];
      if (
        !store.model &&
        !runId &&
        store.phase !== "loading" &&
        initialBinding?.mode === "custom"
      ) {
        const provider = initialBinding.provider_id
          ? settings.global_providers?.find((item) => item.id === initialBinding.provider_id)
          : undefined;
        const managedModel = resolveManagedProviderDefaultModel(
          initialBinding,
          provider?.models?.map((item) => item.id) ?? [],
        );
        if (managedModel) {
          const pid = initialBinding.provider_id;
          store.model =
            pid && !managedModel.includes("/") ? `${pid}/${managedModel}` : managedModel;
        }
      } else if (!store.model && !runId && store.phase !== "loading" && store.useStreamSession) {
        const initCred = findCredential(
          settings.platform_credentials ?? [],
          store.platformId ?? "",
        );
        const initPreset = PLATFORM_PRESETS.find((p) => p.id === store.platformId);
        const initModels = initCred?.models?.length ? initCred.models : initPreset?.models;
        if (store.platformId !== "anthropic" && initModels?.[0]) {
          store.model = initModels[0];
        } else if (store.platformId === "anthropic" && settings.default_model) {
          // default_model is global — only valid for Anthropic native platform.
          // Third-party platforms without a models list leave model unset.
          store.model = settings.default_model;
        }
      }
      // Load auth overview for AuthSourceBadge
      api
        .getAuthOverview()
        .then((ov) => (authOverview = ov))
        .catch(() => {});
      // Detect local proxy statuses for AuthSourceBadge
      checkAllLocalProxies();
    } catch (e) {
      dbgWarn("chat", "failed to load settings:", e);
    }
    // When a valid ?agent= param exists, handleAgentChange(force) will handle
    // agentSettings + effort loading. Skip here to avoid concurrent overwrites.
    const hasValidAgentParam = agentParam && isKnownAgent(agentParam);
    if (!hasValidAgentParam) {
      try {
        agentSettings = await api.getAgentSettings(store.agent);
        if (
          store.agent === "codex" ||
          store.agent === "pi" ||
          store.agent === "dsh" ||
          store.agent === "grok"
        ) {
          // Codex/Pi/DSH: agent settings are the source of truth for the
          // agent-scoped startup preference. Do NOT read Claude's CLI config.
          // Grok leaves it empty so its model/CLI default remains authoritative
          // until the user selects one.
          // A materialized conversation owns its effort. Agent settings are
          // only the fallback for a fresh conversation; otherwise switching
          // between windows must not replace this run's next-request choice.
          const conversationEffort = store.run?.effort?.trim() || loadConversationEffort(runId);
          if (effortSelectionVersion === hydrationEffortVersion) {
            currentEffort =
              conversationEffort ||
              (store.agent === "grok" ? agentSettings?.effort || "" : "medium");
          }
          if (store.agent === "pi" && !store.model) {
            store.model =
              settings?.pi_provider?.model?.trim() || agentSettings?.model?.trim() || "";
          }
        } else {
          // Claude: read effort from CLI config (~/.claude/settings.json) — the authoritative
          // source. NOT from agentSettings.effort (that would cause --effort flag at spawn,
          // which locks effort in memory and prevents live switching via settings.json).
          try {
            const cliCfg = await api.getCliConfig();
            const cliEffort = cliCfg.effortLevel;
            if (effortSelectionVersion === hydrationEffortVersion) {
              currentEffort = typeof cliEffort === "string" && cliEffort ? cliEffort : "";
            }
          } catch {
            if (effortSelectionVersion === hydrationEffortVersion) currentEffort = "";
          }
          // One-time migration: clear stale agentSettings.effort to prevent --effort at spawn
          if (agentSettings?.effort) {
            api.updateAgentSettings(store.agent, { effort: "" }).catch(() => {});
          }
        }
      } catch (e) {
        dbgWarn("chat", "failed to load agent settings:", e);
      }
    }
    // Initialize permission mode from saved settings (before session_init arrives).
    // Uses syncPermissionModeFromSettings helper — does NOT set permissionModeSetByUser.
    // That flag is only set by handlePermissionModeChange() (user manual action).
    syncPermissionModeFromSettings(agentSettings, settings);
    // Find most recent run with session_id for "Continue last session"
    try {
      const runs = await api.listRuns();
      if (currentHarness === "code") {
        knownCodeProjectCwds = codeProjectCwdsFromRuns(runs, currentRealm);
      }
      lastContinuableRun =
        runs.find(
          (r) =>
            r.session_id &&
            (r.status === "completed" || r.status === "stopped" || r.status === "failed"),
        ) ?? null;
    } catch (e) {
      dbgWarn("chat", "failed to load runs for continue:", e);
    }
    let selfHealDone = false;
    let selfHealInFlight = false;
    // Codex model catalog (live from app-server). Fire-and-forget; 5min TTL cache.
    void loadCodexModels();
    void loadGrokModels();
    loadCliInfo().then(() => {
      // Self-heal: detect and fix contaminated default_model
      if (settings?.default_model && !selfHealDone && !selfHealInFlight) {
        const dm = settings.default_model;
        const contaminated = isContaminatedDefaultModel(dm);
        if (contaminated === true) {
          const healModel = getCliCurrentModel();
          if (healModel) {
            selfHealInFlight = true;
            dbg("chat", "self-heal: default_model contaminated, persisting fix", {
              old: dm,
              new: healModel,
            });
            api
              .updateUserSettings({ default_model: healModel })
              .then(() => {
                settings!.default_model = healModel;
                lastKnownGoodAnthropicModel = healModel;
                selfHealDone = true;
                dbg("chat", "self-heal: persist succeeded");
              })
              .catch((e) => {
                dbgWarn("chat", "self-heal persist failed, will retry next loadCliInfo", e);
              })
              .finally(() => {
                selfHealInFlight = false;
              });
          } else {
            dbg("chat", "self-heal: contaminated but CLI model unavailable, deferring", { dm });
          }
        } else if (contaminated === false) {
          selfHealDone = true;
        }
      }

      const cliModel = getCliCurrentModel();
      const isThirdParty = store.platformId && store.platformId !== "anthropic";
      // Update lastKnownGoodAnthropicModel when CLI model is available
      if (cliModel && !isThirdParty) {
        lastKnownGoodAnthropicModel = cliModel;
      }
      // Only for genuinely new chats: no run loaded/loading, no URL run param
      if (
        cliModel &&
        !store.run &&
        !runId &&
        store.phase !== "loading" &&
        !isThirdParty &&
        store.useStreamSession
      ) {
        dbg("chat", "set model from CLI after loadCliInfo", { cliModel, prev: store.model });
        store.model = cliModel;
      }
    });
    loadCliVersionInfo();
    checkProjectInit();
    // Preload project data from filesystem (no session needed)
    if (!runId) {
      const cwd = getSavedProjectCwd(currentRealm) || "";
      reloadProjectData(cwd);
    }
  });

  // Listen for project folder changes to re-check project init + reload project data
  onMount(() => {
    const handler = (event: Event) => {
      const detail = (event as CustomEvent<{ cwd?: unknown }>).detail;
      const cwd =
        typeof detail?.cwd === "string"
          ? normalizeCwd(detail.cwd)
          : normalizeCwd(getSavedProjectCwd(currentRealm));
      if (!runId && !store.run) folderCwdOverride = cwd;
      checkProjectInit();
      if (!runId && !store.run) {
        reloadProjectData(cwd);
      }
    };
    window.addEventListener("agentcabin:project-changed", handler);
    return () => window.removeEventListener("agentcabin:project-changed", handler);
  });

  // Warm up file IPC chain: validate_file_path's first invocation walks several
  // canonicalize() calls (data dir, claude dir, agents' working dirs, project cwd).
  // Firing one stat at chat-page mount primes the OS FS cache so the user's first
  // file click doesn't pay the cold-cache cost.
  onMount(() => {
    const cwd = getSavedProjectCwd(currentRealm) || "";
    if (!cwd) return;
    const t0 = performance.now();
    api
      .statTextFile(cwd, cwd)
      .then(() => dbg("file-ipc", "warmup done", { ms: +(performance.now() - t0).toFixed(0) }))
      .catch((e) =>
        dbg("file-ipc", "warmup err (still warmed)", {
          ms: +(performance.now() - t0).toFixed(0),
          err: String(e),
        }),
      );
  });

  // Sync run name when sidebar/history renames the current run
  onMount(() => {
    function onRunsChanged() {
      removedCwdsVersion++;
      if (currentHarness === "code") void refreshKnownCodeProjects();
      if (!store.run) return;
      const id = store.run.id;
      api
        .getRun(id)
        .then((fresh) => {
          if (fresh && store.run?.id === id && fresh.name !== store.run.name) {
            dbg("chat", "runs-changed: syncing name", { id, name: fresh.name });
            store.run = { ...store.run, name: fresh.name ?? undefined };
            if (fresh.name) autoNameDone = true;
          }
        })
        .catch((e) => {
          dbgWarn("chat", "runs-changed: failed to sync name", e);
        });
    }
    window.addEventListener("agentcabin:runs-changed", onRunsChanged);
    return () => window.removeEventListener("agentcabin:runs-changed", onRunsChanged);
  });

  // Sync removed project folders from sidebar remove action
  onMount(() => {
    function onProjectRemoved(event: Event) {
      const detail = (event as CustomEvent<{ cwd?: unknown }>).detail;
      const removed = typeof detail?.cwd === "string" ? normalizeCwd(detail.cwd) : "";
      if (!removed) return;
      if (normalizeCwd(folderCwdOverride) === removed) {
        folderCwdOverride = "";
      }
      knownCodeProjectCwds = knownCodeProjectCwds.filter((cwd) => normalizeCwd(cwd) !== removed);
      removedCwdsVersion++;
      if (currentHarness === "code") void refreshKnownCodeProjects();
    }
    window.addEventListener("agentcabin:project-removed", onProjectRemoved);
    return () => window.removeEventListener("agentcabin:project-removed", onProjectRemoved);
  });

  // Settings may be saved from another in-app surface while this chat component stays mounted.
  // Refresh the local source of truth and managed catalogs so a removed Provider model cannot
  // remain selectable in a fresh Runtime draft.
  onMount(() => {
    function onUserSettingsChanged(event: Event) {
      const next = (event as CustomEvent<UserSettings>).detail;
      if (!next) return;
      settings = next;
      if (store.run || store.sessionAlive) return;
      if (effectiveAgent === "pi") void loadPiModels(true);
      if (effectiveAgent === "grok") void loadGrokModels(true);
    }

    window.addEventListener("agentcabin:user-settings-changed", onUserSettingsChanged);
    return () =>
      window.removeEventListener("agentcabin:user-settings-changed", onUserSettingsChanged);
  });

  // Start middleware + register handlers
  onMount(() => {
    let destroyed = false;
    (async () => {
      try {
        await middleware.start();
      } catch (e) {
        console.error("[chat] middleware.start() failed:", e);
        store.error = t("chat_eventSystemFailed");
      }
      if (!destroyed) middlewareReady = true;
    })();

    // Pipe handler: chat-delta / chat-done (Codex pipe mode)
    middleware.setPipeHandler({
      onDelta(delta) {
        store.handleChatDelta(delta.text, xtermRef);
      },
      onDone(done) {
        store.handleChatDone(done);
      },
    });

    // Run event handler: stderr for Codex pipe mode
    middleware.setRunEventHandler({
      onRunEvent(event) {
        if (
          store.run?.execution_path === "pipe_exec" &&
          event.run_id === store.run.id &&
          xtermRef
        ) {
          if (event.type === "stderr") {
            xtermRef.writeText(`\x1b[31m${event.text}\x1b[0m\r\n`);
          }
        }
      },
    });

    return () => {
      destroyed = true;
      // Kill fork run process on unmount (but not the source run)
      if (forkOverlay?.active && store.run && store.run.id !== forkOverlay.sourceRunId) {
        api.stopSession(store.run.id).catch(() => {});
      }
      store.unmountGuards();
      middleware.destroy();
    };
  });

  // Watch runId changes → load run + subscribe middleware
  // Gated on middlewareReady to ensure listeners are registered before subscribing
  $effect(() => {
    if (!middlewareReady) return;
    const id = runId;
    const hasResume = hasResumeParam;
    untrack(() => {
      middleware.subscribeCurrent(id, store);

      // Strongest guard: resume operation in progress — don't interfere.
      // Check both store guard (set inside resumeSession) and local flag
      // (set at handleResume entry, before store guard is acquired).
      if (store.resumeInFlight || resuming) {
        dbg("effect", "skip loadRun — resume in progress");
        return;
      }
      // Resume $effect will handle this case
      if (hasResume) return;

      if (!id) {
        projectModelPreferenceAppliedToken = ""; // 路由切到空 runId（新会话）→ 允许 hydrate
        store.loadRun("", xtermRef);
        cancelProgressive(); // empty run — no progressive needed
        return;
      }

      // If store already holds an active session for this run, skip redundant loadRun
      if (store.run?.id === id && store.sessionAlive) {
        if (store.run.app_mode === "work") {
          const params = new URLSearchParams();
          if (store.run.workspace_id) params.set("workspace", store.run.workspace_id);
          params.set("run", store.run.id);
          void goto(`/chat/work?${params.toString()}`, { replaceState: true });
          return;
        }
        dbg("effect", "skip loadRun — session already alive for", id);
        const scrollTo = $page.url.searchParams.get("scrollTo");
        if (scrollTo) {
          const clean = new URL($page.url);
          clean.searchParams.delete("scrollTo");
          replaceState(clean, {});
          tick().then(() => scrollToMessage(scrollTo));
        }
        return;
      }

      loadRunProgressive(id, xtermRef);
    });
  });

  // Handle scrollTo for already-loaded runs (e.g., clicking a second search result
  // in the same run). The runId effect above won't re-fire when only scrollTo changes.
  $effect(() => {
    if (!middlewareReady) return;
    const scrollTo = $page.url.searchParams.get("scrollTo");
    if (!scrollTo) return;
    untrack(() => {
      // loadRunProgressive handles scrollTo during run loading — don't double-scroll
      if (_scrollToInFlight) return;
      if (store.phase === "loading") return;
      if (store.run?.id !== runId) return;

      dbg("effect", "same-run scrollTo", { scrollTo, runId });
      _scrollToInFlight = true;
      const clean = new URL($page.url);
      clean.searchParams.delete("scrollTo");
      replaceState(clean, {});
      tick().then(() => {
        scrollToMessage(scrollTo);
        _scrollToInFlight = false;
      });
    });
  });

  // Consume ?resume= URL param for session resume via sidebar button
  $effect(() => {
    const url = $page.url;
    const paramRunId = url.searchParams.get("run");
    const resumeMode = url.searchParams.get("resume") as SessionMode | null;

    if (paramRunId && resumeMode) {
      // Clean URL immediately to prevent re-trigger on refresh
      const clean = new URL(url);
      clean.searchParams.delete("resume");
      replaceState(clean, {});

      untrack(() => {
        handleResume(resumeMode, paramRunId);
      });
    }
  });

  // Listen for Pi extension notice & editor action events
  $effect(() => {
    if (store.latestNotice) {
      showChatToast(store.latestNotice.message);
    }
  });

  $effect(() => {
    if (store.pendingEditorText !== null && promptRef) {
      promptRef.setValue(store.pendingEditorText);
      store.pendingEditorText = null;
    }
  });

  // Auto-focus prompt input on mount + register chat keybindings
  onMount(() => {
    requestAnimationFrame(() => promptRef?.focus());
    function onNewChatEvent(e: Event) {
      const customEvt = e as CustomEvent<{ cwd?: string; codeStandaloneTask?: boolean }>;
      const cwd = customEvt.detail?.cwd;
      codeStandaloneTask = customEvt.detail?.codeStandaloneTask === true || !cwd;
      if (cwd) {
        folderCwdOverride = cwd;
        setSavedProjectCwd(cwd, currentRealm);
      } else if (codeStandaloneTask) {
        folderCwdOverride = "";
        store.remoteHostName = null;
        setLastTarget(null);
      }
      // 新建对话时必须清空 hydrate token：否则连续开多个相同 key 的 draft 会话时，
      // token（已不含 runModel）会完全相同，effect 认为已 hydrate 过而跳过，导致
      // 项目默认模型不再应用到新会话。
      projectModelPreferenceAppliedToken = "";
      lastHarnessForAgent = null;
      store.loadRun("", xtermRef);
      promptRef?.clearAll();
      requestAnimationFrame(() => promptRef?.focus());
    }
    window.addEventListener("agentcabin:new-chat", onNewChatEvent);

    // Register chat-context keybinding callbacks
    keybindingStore.registerCallback("chat:interrupt", () => {
      if (shortcutHelpOpen) {
        shortcutHelpOpen = false;
        return;
      }
      if (store.isRunning) {
        store.interrupt();
      }
    });
    keybindingStore.registerCallback("chat:sendGlobal", () => {
      if (!store.isRunning) {
        promptRef?.triggerSend();
      }
    });
    keybindingStore.registerCallback("app:shortcutHelp", () => {
      shortcutHelpOpen = !shortcutHelpOpen;
    });
    keybindingStore.registerCallback("app:modelPicker", () => {
      statusBarRef?.openModelDropdown();
    });
    keybindingStore.registerCallback("chat:cyclePermission", () => {
      // Guard: if focus is on a focusable interactive control, don't cycle (preserve Shift+Tab navigation)
      const active = document.activeElement;
      if (active && active !== document.body) {
        const el = active as HTMLElement;
        const isFocusable =
          el.tagName === "BUTTON" ||
          el.tagName === "SELECT" ||
          el.tagName === "A" ||
          (el.hasAttribute("tabindex") && el.getAttribute("tabindex") !== "-1") ||
          el.closest("[role='menu']") ||
          el.closest("[role='listbox']") ||
          el.closest("[role='dialog']") ||
          (el.hasAttribute("role") &&
            ["button", "link", "menuitem", "option", "tab"].includes(
              el.getAttribute("role") ?? "",
            ));
        if (isFocusable) return;
      }
      const modes = ["default", "acceptEdits", "bypassPermissions"];
      const idx = modes.indexOf(store.permissionMode);
      const next = modes[(idx + 1) % modes.length];
      handlePermissionModeChange(next);
    });
    keybindingStore.registerCallback("chat:stashPrompt", () => {
      if (stashedInput) {
        promptRef?.restoreSnapshot(stashedInput);
        stashedInput = null;
        showChatToast(t("toast_stashRestored"));
      } else {
        const snapshot = promptRef?.getInputSnapshot();
        if (
          snapshot &&
          (snapshot.text.trim() ||
            snapshot.attachments.length ||
            snapshot.pastedBlocks.length ||
            (snapshot.pathRefs?.length ?? 0) > 0)
        ) {
          stashedInput = snapshot;
          promptRef?.clearAll();
          showChatToast(t("toast_stashSaved"));
        }
      }
    });
    keybindingStore.registerCallback("app:toggleFastMode", () => {
      toggleCliConfigBool("fastMode");
    });
    keybindingStore.registerCallback("chat:toggleVerbose", () => {
      toggleCliConfigBool("verbose");
    });
    keybindingStore.registerCallback("chat:toggleTasks", () => {
      if (store.hasBackgroundTasks) {
        if (sidebarCollapsed) sidebarCollapsed = false;
        sidebarRequestedTab = "tasks";
      }
    });
    keybindingStore.registerCallback("chat:undoLastTurn", () => {
      handleRewind();
    });
    keybindingStore.registerCallback("app:toggleBottomPanel", toggleBottomPanel);
    keybindingStore.registerCallback("app:exportChatHtml", () => void handleExportHtml());
    const onExportHtmlEvent = () => {
      window.dispatchEvent(new CustomEvent("agentcabin:export-html-ack"));
      void handleExportHtml();
    };
    window.addEventListener("agentcabin:export-html", onExportHtmlEvent);

    // Screenshot event listener (global hotkey → attachment injection)
    const chatTransport = getTransport();
    const screenshotUnlisten = chatTransport.listen<ScreenshotPayload>(
      "screenshot-taken",
      (payload) => {
        dbg("chat", "screenshot-taken", { filename: payload.filename });
        const { contentBase64, mediaType, filename } = payload;
        const bytes = Uint8Array.from(atob(contentBase64), (c) => c.charCodeAt(0));
        const file = new File([bytes], filename, { type: mediaType });
        promptRef?.addFiles([file]);
      },
    );

    // Tauri native drag-drop listeners (dragDropEnabled: true in tauri.conf.json)
    const dragEnterUnlisten = chatTransport.listen<{ paths: string[] }>(
      "tauri://drag-enter",
      () => {
        pageDragActive = true;
      },
    );
    const dragLeaveUnlisten = chatTransport.listen("tauri://drag-leave", () => {
      pageDragActive = false;
    });
    const dragDropUnlisten = chatTransport.listen<{ paths: string[] }>(
      "tauri://drag-drop",
      handleTauriDrop,
    );

    // Preview window event listeners
    const previewSelectionUnlisten = chatTransport.listen<{
      instanceId: string;
      data: unknown;
    }>("preview-element-selected", (payload) => {
      if (payload.instanceId !== previewInstanceId) {
        dbgWarn("preview", "ignoring selection from stale instance", payload.instanceId);
        return;
      }
      if (!isElementSelection(payload.data)) {
        dbgWarn("preview", "invalid element payload", payload.data);
        return;
      }
      dbg("preview", "elementSelected", {
        domPath: payload.data.domPath,
        tagName: payload.data.tagName,
      });
      // Insert directly to chat and bring main window to front
      promptRef?.appendText(formatElementContext(payload.data));
      void platform.window.focus();
    });

    const previewClosedUnlisten = chatTransport.listen<{ instanceId: string }>(
      "preview-window-closed",
      (payload) => {
        if (payload.instanceId !== previewInstanceId) return;
        dbg("preview", "windowClosed", { instanceId: payload.instanceId });
        resetPreviewState();
      },
    );

    return () => {
      window.removeEventListener("agentcabin:new-chat", onNewChatEvent);
      keybindingStore.unregisterCallback("chat:interrupt");
      keybindingStore.unregisterCallback("chat:sendGlobal");
      keybindingStore.unregisterCallback("app:shortcutHelp");
      keybindingStore.unregisterCallback("app:modelPicker");
      keybindingStore.unregisterCallback("chat:cyclePermission");
      keybindingStore.unregisterCallback("chat:stashPrompt");
      keybindingStore.unregisterCallback("app:toggleFastMode");
      keybindingStore.unregisterCallback("chat:toggleVerbose");
      keybindingStore.unregisterCallback("chat:toggleTasks");
      keybindingStore.unregisterCallback("chat:undoLastTurn");
      keybindingStore.unregisterCallback("app:toggleBottomPanel");
      keybindingStore.unregisterCallback("app:exportChatHtml");
      window.removeEventListener("agentcabin:export-html", onExportHtmlEvent);
      screenshotUnlisten.then((fn) => fn());
      dragEnterUnlisten.then((fn) => fn());
      dragLeaveUnlisten.then((fn) => fn());
      dragDropUnlisten.then((fn) => fn());
      previewSelectionUnlisten.then((fn) => fn());
      previewClosedUnlisten.then((fn) => fn());
      // Clean up verbose retry timer
      if (verboseRetryTimer) clearTimeout(verboseRetryTimer);
      // Clean up progressive rendering timer
      cancelProgressive();
    };
  });

  // ── BTW event listeners ──
  // NOTE: Don't filter by btw_id — events may arrive before the IPC call returns
  // the btw_id (race condition). Only one BTW is active at a time, so checking
  // btwState.active is sufficient.
  onMount(() => {
    const transport = getTransport();
    const deltaUnlisten = transport.listen<import("$lib/types").BtwDelta>("btw-delta", (ev) => {
      if (btwState.active) {
        dbg("chat", "btw-delta", { len: ev.text.length });
        btwState.answer += ev.text;
      }
    });
    const completeUnlisten = transport.listen<import("$lib/types").BtwComplete>(
      "btw-complete",
      (ev) => {
        if (btwState.active) {
          dbg("chat", "btw-complete", { btwId: ev.btw_id });
          btwState.loading = false;
        }
      },
    );
    const errorUnlisten = transport.listen<import("$lib/types").BtwError>("btw-error", (ev) => {
      if (btwState.active) {
        dbgWarn("chat", "btw-error", { error: ev.error });
        btwState.error = ev.error;
        btwState.loading = false;
      }
    });
    return () => {
      deltaUnlisten.then((f) => f());
      completeUnlisten.then((f) => f());
      errorUnlisten.then((f) => f());
    };
  });

  // Auto-scroll chat (only when user is near bottom)
  let prevTl = 0;
  let prevSt = 0;

  $effect(() => {
    if (store.useStreamSession && chatAreaRef) {
      const tl = store.timeline.length;
      const st = store.streamingText.length;
      const _rid = store.run?.id;
      const changed = tl !== prevTl || st !== prevSt;
      prevTl = tl;
      prevSt = st;
      if (isChatAutoScroll) {
        requestAnimationFrame(() => {
          if (chatAreaRef) chatAreaRef.scrollTop = chatAreaRef.scrollHeight;
        });
      } else if (changed) {
        showChatScrollHint = true;
      }
    }
  });

  // Reset scroll state on run change
  $effect(() => {
    void store.run?.id;
    // _scrollToInFlight is non-reactive (plain let): reading it doesn't create a dependency.
    // When a search scroll-to navigation is in progress, suppress auto-scroll so
    // scrollToMessage isn't overridden by the auto-scroll $effect.
    isChatAutoScroll = !_scrollToInFlight;
    showChatScrollHint = false;
    prevTl = 0;
    prevSt = 0;
  });

  // Restore model when store.model is empty (e.g. after reset/loadRun):
  // For third-party platforms, use the platform's default model.
  // For Anthropic, prefer CC's current active model, fall back to our saved default_model
  // (only if confirmed clean via three-state contamination check).
  $effect(() => {
    if (!store.model && store.useStreamSession) {
      // Don't overwrite model during loadRun async gap — loadRun will set it
      if (store.phase === "loading") return;

      // Existing conversation's own model is always top priority
      if (store.run?.model?.trim()) {
        store.model = store.run.model.trim();
        return;
      }
      if (activeProviderBinding?.mode === "custom" && managedProviderDefaultModel) {
        dbg("chat", "restore managed provider default model", {
          agent: effectiveAgent,
          model: managedProviderDefaultModel,
        });
        store.model = managedProviderDefaultModel;
        return;
      }

      // Pi models are scoped to Pi settings/runtime state. Never consult the Claude CLI model
      // or global default_model when restoring a Pi session.
      if (effectiveAgent === "pi") {
        const piModel =
          settings?.pi_provider?.model?.trim() || agentSettings?.model?.trim() || undefined;
        if (piModel) {
          dbg("chat", "restore Pi model", { model: piModel });
          const pid =
            activeProviderBinding?.mode === "custom"
              ? activeProviderBinding.provider_id
              : undefined;
          store.model = pid && !piModel.includes("/") ? `${pid}/${piModel}` : piModel;
        }
        return;
      }

      // Grok Build owns its native provider/model configuration. AgentCabin's per-agent model
      // is an optional session override; blank means Grok resolves its own default.
      if (effectiveAgent === "grok") {
        const grokModel = agentSettings?.model?.trim() || undefined;
        if (grokModel) {
          dbg("chat", "restore Grok model", { model: grokModel });
          store.model = grokModel;
        }
        return;
      }

      const isThirdParty = store.platformId && store.platformId !== "anthropic";
      if (isThirdParty) {
        const restoreCred = findCredential(
          settings?.platform_credentials ?? [],
          store.platformId ?? "",
        );
        const restorePreset = PLATFORM_PRESETS.find((p) => p.id === store.platformId);
        const restoreModels = restoreCred?.models?.length
          ? restoreCred.models
          : restorePreset?.models;
        if (restoreModels?.[0]) {
          dbg("chat", "restore model from credential/preset", {
            platform: store.platformId,
            model: restoreModels[0],
          });
          store.model = restoreModels[0];
          return;
        }
      }
      // Only fall back to default_model for Anthropic platform — otherwise
      // default_model may belong to a different platform (cross-pollution).
      const cliModel = getCliCurrentModel();
      const isAnthropicPlatform = !store.platformId || store.platformId === "anthropic";
      const rawFallback = isAnthropicPlatform ? settings?.default_model : undefined;
      const contaminated = rawFallback ? isContaminatedDefaultModel(rawFallback) : null;
      // Only use default_model when confirmed clean (false). true/null → skip.
      const fallback = contaminated === false ? rawFallback : undefined;
      // Last resort: cached last-known-good Anthropic model (only for Anthropic platform)
      const model =
        cliModel || fallback || (isAnthropicPlatform ? lastKnownGoodAnthropicModel : undefined);
      if (model) {
        // Update cache when we have a trusted source
        if (isAnthropicPlatform && (cliModel || contaminated === false)) {
          lastKnownGoodAnthropicModel = model;
        }
        dbg("chat", "restore model", {
          cliModel,
          rawFallback,
          contaminated,
          lastKnownGood: lastKnownGoodAnthropicModel,
          using: model,
        });
        store.model = model;
      }
    }
  });

  // ── Terminal helpers ──

  function handleTermReady(_cols: number, _rows: number) {
    // Terminal ready — Codex pipe mode is output-only, no setup needed
  }

  function handleTermResize(_cols: number, _rows: number) {
    // Codex pipe mode doesn't need resize — terminal is output-only
  }

  // ── Chat scroll ──

  /** Threshold (px) for "near bottom" detection. Shared concept with TerminalPane. */
  const SCROLL_BOTTOM_THRESHOLD = 40;

  function handleChatScroll() {
    if (!chatAreaRef) return;
    const dist = chatAreaRef.scrollHeight - chatAreaRef.scrollTop - chatAreaRef.clientHeight;
    isChatAutoScroll = dist < SCROLL_BOTTOM_THRESHOLD;
    if (isChatAutoScroll) showChatScrollHint = false;
    // Re-arm progressive load-more after a user-initiated scroll. The IntersectionObserver
    // fires once per arm; this prevents short timelines from runaway-expanding while the
    // sentinel remains in view after a prepend. Programmatic scrollTop adjustments
    // (loadMoreEarlier's anchor compensation) raise `_suppressLoadMoreRearm` so the
    // anchor-correction scroll doesn't immediately re-arm the observer.
    if (!loadMoreArmed && !_suppressLoadMoreRearm) loadMoreArmed = true;
  }

  function scrollChatToBottom() {
    if (chatAreaRef) {
      chatAreaRef.scrollTop = chatAreaRef.scrollHeight;
      showChatScrollHint = false;
      isChatAutoScroll = true;
    }
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

  // ── Permission pending auto-scroll (only for inline AskUserQuestion/ExitPlanMode) ──
  let prevPermissionRunId = "";
  let prevHadPermission = false;

  $effect(() => {
    const runId = store.run?.id ?? "";
    const hasInline = store.hasInlinePermission;

    if (runId !== prevPermissionRunId) {
      prevPermissionRunId = runId;
      prevHadPermission = false;
    }

    if (hasInline && !prevHadPermission) {
      if (!chatAreaRef) return;
      requestAnimationFrame(() => {
        scrollChatToBottom();
      });
      dbg("chat", "inline permission pending -> autoscroll", { runId });
    }

    prevHadPermission = hasInline;
  });

  // ── Permission panel visibility log ──
  let _prevPanelCount = 0;
  $effect(() => {
    const count = pendingToolPermissions.length;
    if (count !== _prevPanelCount) {
      if (count > 0)
        dbg("chat", "permissionPanel visible", {
          count,
          ids: pendingToolPermissions.map((p) => p.requestId),
          tools: pendingToolPermissions.map((p) => p.tool.tool_name),
        });
      else if (_prevPanelCount > 0) dbg("chat", "permissionPanel hidden");
      _prevPanelCount = count;
    }
  });

  // ── Send message ──

  function selectCodeTaskScope(cwd: string | null) {
    if (codeProjectReadOnly) return;
    codeStandaloneTask = cwd === null;
    if (codeStandaloneTask) {
      folderCwdOverride = "";
      if (store.remoteHostName) {
        store.remoteHostName = null;
        setLastTarget(null);
      }
      projectModelPreferenceAppliedToken = "";
      return;
    }
    if (!cwd) return;
    const normalized = normalizeCwd(cwd);
    if (!normalized) return;

    // Choosing a local project explicitly also switches away from a remote target.
    if (store.remoteHostName) {
      store.remoteHostName = null;
      setLastTarget(null);
    }
    folderCwdOverride = normalized;
    setSavedProjectCwd(normalized, currentRealm);
    knownCodeProjectCwds = [
      normalized,
      ...knownCodeProjectCwds.filter((projectCwd) => projectCwd !== normalized),
    ];
    projectModelPreferenceAppliedToken = "";
    window.dispatchEvent(
      new CustomEvent("agentcabin:cwd-changed", { detail: { cwd: normalized } }),
    );
  }

  /** Resolve the local project target used by every new Code session path. */
  async function resolveNewSessionCwd(): Promise<string | null> {
    if (codeStandaloneTask) return "";
    // Code intentionally mirrors Work's local-only project selection. Clear a
    // stale target left by an older Code session instead of silently launching
    // a new session on a remote host.
    if (!store.run && store.remoteHostName) {
      dbgWarn("chat", "clearing stale remote target from local-only Code flow", {
        host: store.remoteHostName,
      });
      store.remoteHostName = null;
      setLastTarget(null);
    }

    let cwd = "";
    if (typeof window !== "undefined") {
      cwd =
        getSavedProjectCwd(currentRealm) || localStorage.getItem("agentcabin:settings-cwd") || "";
    }

    if (!cwd || cwd === "/") {
      const transport = getTransport();
      if (transport.isDesktop()) {
        const selected = await platform.dialog.open({
          directory: true,
          title: t("layout_selectProjectFolder"),
        });
        if (!selected) return null;
        cwd = selected as string;
        setSavedProjectCwd(cwd, currentRealm);
        window.dispatchEvent(new Event("agentcabin:cwd-changed"));
      } else {
        const result = await openFolderPicker({
          initialHost: null,
          hideTargetSelector: true,
        });
        if (!result || !result.path || result.hostName) return null;
        cwd = result.path;
        setSavedProjectCwd(cwd, currentRealm);
        window.dispatchEvent(new Event("agentcabin:cwd-changed"));
      }
    }

    return cwd || null;
  }

  /** Create a run, bind it to the URL, and refresh the run list. */
  async function startNewSession(
    prompt: string,
    attachments: Attachment[],
    options: {
      piShell?: boolean;
      slashCmd?: string | null;
      skills?: { name: string; path: string }[];
    } = {},
  ): Promise<string | null> {
    const cwd = await resolveNewSessionCwd();
    if (cwd === null || (!cwd && !codeStandaloneTask)) return null;

    if (options.slashCmd) {
      processingSlashCmd = options.slashCmd;
      slashCmdSeenRunning = false;
    }
    if (projectPreferenceCwd) {
      void persistProjectModelPreference({
        permission_mode:
          effectiveAgent === "pi"
            ? store.piPermissionState.mode
            : store.permissionMode || undefined,
      }).catch(() => {});
    }
    const createdRunId =
      resolveNewSessionStartMode(options.piShell && !codeStandaloneTask) === "shell"
        ? await store.startPiSessionShell(cwd, currentEffort || undefined)
        : await store.startSession(
            prompt,
            cwd,
            attachments,
            undefined,
            options.skills,
            currentEffort || undefined,
            codeStandaloneTask,
          );
    const routeBase = isPiCodeRoute || effectiveAgent === "pi" ? "/chat/pi" : "/chat";
    await goto(`${routeBase}?run=${createdRunId}`, { replaceState: true });
    window.dispatchEvent(new Event("agentcabin:runs-changed"));
    loadCliVersionInfo();
    return createdRunId;
  }

  async function dispatchPiFeatureCommand(command: string, args = ""): Promise<boolean> {
    if (effectiveAgent !== "pi" || !store.run) return false;
    if (!(await store.waitForPiCommandsReady())) {
      store.error = "Pi commands are unavailable (CommandUnavailable).";
      return false;
    }
    const registered = store.sessionCommands.some(
      (item) =>
        item.name.toLowerCase() === command.toLowerCase() ||
        (item.aliases ?? []).some((alias) => alias.toLowerCase() === command.toLowerCase()),
    );
    if (!registered) {
      store.error = `Command '/${command}' is not available in the current Pi session (CommandUnavailable).`;
      return false;
    }
    try {
      const response = await api.invokePiSlashCommand(store.run.id, command, args);
      if (
        response &&
        typeof response === "object" &&
        "success" in response &&
        (response as { success?: unknown }).success === false
      ) {
        throw new Error(String((response as { error?: unknown }).error ?? `Pi /${command} failed`));
      }
      return true;
    } catch (error) {
      store.error = (error as Error)?.message ?? String(error);
      return false;
    }
  }

  async function waitForPiGoalPhase(
    phase: "paused" | "active" | "inactive",
    timeoutMs = 8000,
  ): Promise<boolean> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      if (store.piGoalState.phase === phase) return true;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    return store.piGoalState.phase === phase;
  }

  /** Single coordinator for every Plan/Goal command, including manual slash input. */
  async function invokePiFeatureCommand(command: string, args = ""): Promise<boolean> {
    const normalizedCommand = command.replace(/^\//, "").toLowerCase();
    const normalizedArgs = args.trim();
    if (piFeaturePending) {
      store.error = "A Pi Plan/Goal operation is already in progress.";
      return false;
    }

    const goalKind = classifyPiGoalCommand(normalizedArgs);
    const goalResumes = normalizedCommand === "goal" && goalKind === "resume";
    if (
      normalizedCommand === "goal" &&
      shouldBlockPiGoalCommand(store.piPlanState.phase, normalizedArgs)
    ) {
      store.error = "请先退出 Plan，再启动 Goal。";
      return false;
    }

    if (
      normalizedCommand === "plan" &&
      planEntryNeedsGoalPause(store.piPlanState.phase, store.piGoalState.phase, normalizedArgs)
    ) {
      piFeaturePending = "goal_pausing";
      const paused = await dispatchPiFeatureCommand("goal", "pause");
      if (!paused || !(await waitForPiGoalPhase("paused"))) {
        piFeaturePending = null;
        store.error = "Goal 尚未确认暂停，未进入 Plan。";
        return false;
      }
      piFeaturePending = "plan_entering";
    } else if (normalizedCommand === "plan") {
      piFeaturePending = normalizedArgs === "exit" ? "plan_exiting" : "plan_entering";
    } else if (normalizedCommand === "goal") {
      piFeaturePending = goalResumes
        ? "goal_resuming"
        : goalKind === "pause"
          ? "goal_pausing"
          : goalKind === "clear"
            ? "goal_clearing"
            : "goal_starting";
    } else {
      return dispatchPiFeatureCommand(command, args);
    }

    try {
      return await dispatchPiFeatureCommand(normalizedCommand, normalizedArgs);
    } finally {
      piFeaturePending = null;
    }
  }

  function handleQueueSend(text: string, attachments: Attachment[]) {
    const queuedText =
      effectiveAgent === "pi" ? normalizePiSlashText(text, store.sessionCommands) : text;
    void store.sendQueuedMessage(queuedText, attachments, "followUp").catch((error) => {
      store.error = (error as Error)?.message ?? String(error);
    });
  }

  function editQueuedMessage(id: string) {
    const queued = store.takeQueuedMessage(id);
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
    void store.steerQueuedMessage(id).catch((error) => {
      store.error = (error as Error)?.message ?? String(error);
    });
  }

  async function handlePiPlan(action: "enter" | "exit") {
    if (!store.run) {
      piFeatureStartupPending = true;
      try {
        const runId = await startNewSession("", [], { piShell: true });
        if (!runId) return;
      } catch (error) {
        store.error = (error as Error)?.message ?? String(error);
        return;
      } finally {
        piFeatureStartupPending = false;
      }
    }
    const applied = await invokePiFeatureCommand("plan", action === "exit" ? "exit" : "");
    if (applied && action === "exit") {
      store.piPlanState = { ...store.piPlanState, phase: "inactive" };
    }
  }

  async function handlePiGoal(): Promise<void> {
    if (effectiveAgent !== "pi") return;
    if (!store.run) {
      piFeatureStartupPending = true;
      try {
        const runId = await startNewSession("", [], { piShell: true });
        if (!runId) return;
      } catch (error) {
        store.error = (error as Error)?.message ?? String(error);
        return;
      } finally {
        piFeatureStartupPending = false;
      }
    }

    if (!(await store.waitForPiCommandsReady())) {
      store.error = "Pi commands are unavailable (CommandUnavailable).";
      return;
    }
    if (store.piCapabilities?.goalAvailable === false) {
      store.error = "Pi Goal extension is unavailable. Enable @narumitw/pi-goal first.";
      return;
    }
    goalPanelOpen = true;
  }

  async function refreshWorktrees() {
    const cwd = store.effectiveCwd;
    if (!cwd) return;
    worktreeBusy = true;
    try {
      worktreeProject = await api.getGitProject(cwd);
      worktrees = await api.listGitWorktrees(cwd);
    } catch {
      worktreeProject = null;
      worktrees = [];
    } finally {
      worktreeBusy = false;
    }
  }

  function openWorktreePanel() {
    if (!worktreeProject) return;
    worktreeOpen = true;
    void refreshWorktrees();
  }

  $effect(() => {
    const cwd = store.effectiveCwd;
    if (!cwd || store.remoteHostName) {
      worktreeCheckCwd = "";
      worktreeProject = null;
      worktrees = [];
      return;
    }
    if (cwd === worktreeCheckCwd) return;
    worktreeCheckCwd = cwd;
    void api
      .getGitProject(cwd)
      .then((project) => {
        if (store.effectiveCwd === cwd) worktreeProject = project;
      })
      .catch(() => {
        if (store.effectiveCwd === cwd) {
          worktreeProject = null;
          worktrees = [];
        }
      });
  });

  async function createWorktree(branch: string, path: string) {
    const cwd = store.effectiveCwd;
    if (!cwd) return;
    worktreeBusy = true;
    try {
      await api.createGitWorktree(cwd, branch, path || undefined);
      worktrees = await api.listGitWorktrees(cwd);
    } catch (error) {
      store.error = (error as Error)?.message ?? String(error);
    } finally {
      worktreeBusy = false;
    }
  }

  async function removeWorktree(path: string, force: boolean) {
    const cwd = store.effectiveCwd;
    if (!cwd) return;
    worktreeBusy = true;
    try {
      await api.removeGitWorktree(cwd, path, force);
      worktrees = await api.listGitWorktrees(cwd);
    } catch (error) {
      store.error = (error as Error)?.message ?? String(error);
    } finally {
      worktreeBusy = false;
    }
  }

  function canOpenContinuation() {
    return (
      !!store.run &&
      !store.isRunning &&
      continuationAgentIds.includes(store.run.agent as ContinuationAgent)
    );
  }

  function openContinuationMenu(entryId: string) {
    if (!canOpenContinuation()) return;
    continuationMenu = { entryId, step: "location" };
  }

  function openEndContinuationMenu() {
    if (!canOpenContinuation()) return;
    continuationMenu = { entryId: null, step: "location" };
  }

  function getContinuationAgents(): ContinuationAgent[] {
    const enabledAgents = settings?.enabled_agents ?? ["pi"];
    return continuationAgentIds.filter((targetAgent) => enabledAgents.includes(targetAgent));
  }

  async function selectContinuationLocation(useWorktree: boolean) {
    const menu = continuationMenu;
    if (!menu || menu.step !== "location" || continuationBusy || !store.run) return;
    if (useWorktree) {
      const ok = await platform.dialog.confirm(t("chat_continueInWorktreeWarning"), {
        title: "Worktree",
        kind: "warning",
      });
      if (!ok) return;
    }
    continuationMenu = { ...menu, step: "agent", useWorktree };
  }

  async function startContinuationWithAgent(targetAgent: ContinuationAgent) {
    const menu = continuationMenu;
    if (!menu || menu.step !== "agent" || continuationBusy || !store.run) return;
    if (!getContinuationAgents().includes(targetAgent)) return;

    continuationBusy = true;
    const sourceRun = store.run;
    const sourceCwd = store.effectiveCwd;
    let createdWorktree: GitWorktreeInfo | null = null;
    let newRunId: string | null = null;
    try {
      if (!sourceCwd) throw new Error("当前会话没有可用的工作目录");
      let targetCwd = sourceCwd;
      const targetCapabilities = getAgentCapabilities(targetAgent);
      if (store.remoteHostName && !targetCapabilities.runtime.remote) {
        throw new Error("目标 Agent 不支持远程主机");
      }

      if (menu.useWorktree) {
        const prefix = settings?.worktree_branch_prefix ?? "agentcabin/";
        const branch = `${prefix}continue-${Date.now().toString(36)}`;
        createdWorktree = await api.createGitWorktree(sourceCwd, branch);
        targetCwd = createdWorktree.path;
      }

      const resultId = await startContinuationSession(
        {
          sourceRun,
          anchorId: menu.entryId ?? undefined,
          targetAgent,
          targetCwd,
          model: targetAgent === sourceRun.agent ? store.model || undefined : undefined,
          remoteHostName: targetCapabilities.runtime.remote
            ? store.remoteHostName || undefined
            : undefined,
          platformId: store.platformId || undefined,
        },
        {
          getContinuationContext: api.getContinuationContext,
          startRun: api.startRun,
          copyRunHistory: api.copyRunHistory,
          onRunCreated: (run) => (newRunId = run.id),
          resumeSession: (runId, mode) => store.resumeSession(runId, mode),
        },
      );
      newRunId = resultId;

      continuationMenu = null;
      window.dispatchEvent(new Event("agentcabin:runs-changed"));
      const routeTarget =
        targetAgent === "pi"
          ? "pi:code"
          : targetAgent === "codex"
            ? "native:codex"
            : targetAgent === "grok"
              ? "native:grok"
              : "native:claude";
      await goto(getTargetRoute(routeTarget, { runId: resultId }));
    } catch (error) {
      if (newRunId) {
        try {
          await api.stopRun(newRunId);
        } catch {
          /* best effort cleanup */
        }
      }
      if (createdWorktree && sourceCwd) {
        try {
          await api.removeGitWorktree(sourceCwd, createdWorktree.path, true);
        } catch {
          /* keep the worktree if cleanup is unsafe */
        }
      }
      store.error = (error as Error)?.message ?? String(error);
    } finally {
      continuationBusy = false;
    }
  }

  async function handlePiPermissionMode(mode: string) {
    if (piPermissionBusy || effectiveAgent !== "pi") return;
    const previousState = store.piPermissionState;
    const previousPermissionModeFlag = store.permissionModeSetByUser;
    const normalized = normalizePiPermissionMode(mode);
    piPermissionBusy = true;
    try {
      // Protect a deliberate composer choice from an overlapping settings
      // refresh while the extension config is being updated.
      store.permissionModeSetByUser = true;
      store.piPermissionState = {
        ...previousState,
        mode: normalized,
        restartRequired: false,
      };

      // The extension refreshes its config before every agent turn. Updating
      // the file through the actor is enough; `/reload` is not a Pi RPC command.
      await applyPiPermissionMode(normalized, {
        runId: store.run?.id,
        sessionAlive: store.sessionAlive,
        liveControlAvailable: effectiveCapabilities.protocol.permissionModeControl,
        persistMode: (nextMode) => persistProjectModelPreference({ permission_mode: nextMode }),
        setLiveMode: (runId, nextMode) => api.setPermissionMode(runId, nextMode),
      });
    } catch (error) {
      store.piPermissionState = previousState;
      store.permissionModeSetByUser = previousPermissionModeFlag;
      store.error = (error as Error)?.message ?? String(error);
    } finally {
      piPermissionBusy = false;
    }
  }

  async function handlePiFork(entryId: string) {
    if (
      !store.run ||
      piTreeActionBusy ||
      !store.capabilities.runtime.fork ||
      store.piCapabilities?.forkAvailable === false
    )
      return;
    piTreeActionBusy = true;
    try {
      const newRunId = await api.forkPiSessionAt(store.run.id, entryId);
      window.dispatchEvent(new Event("agentcabin:runs-changed"));
      piTreeOpen = false;
      await goto("/chat/pi?run=" + newRunId);
    } catch (error) {
      store.error = (error as Error)?.message ?? String(error);
    } finally {
      piTreeActionBusy = false;
    }
  }

  async function handlePiClone() {
    if (!store.run || piTreeActionBusy) return;
    piTreeActionBusy = true;
    try {
      const newRunId = await api.clonePiSession(store.run.id);
      window.dispatchEvent(new Event("agentcabin:runs-changed"));
      piTreeOpen = false;
      await goto("/chat/pi?run=" + newRunId);
    } catch (error) {
      store.error = (error as Error)?.message ?? String(error);
    } finally {
      piTreeActionBusy = false;
    }
  }

  async function sendMessage(
    text: string,
    attachments: Attachment[],
    // Codex skill refs picked in the composer (live-Codex only). Forwarded to store.sendMessage,
    // which threads them to the structured {type:"skill"} send. Empty for Claude / no-skill sends.
    skills?: { name: string; path: string }[],
  ) {
    if (!text.trim()) return;

    // Intercept virtual slash actions (e.g. /clear, /compact, /help, /permissions, /doctor)
    const vAct = parseVirtualAction(text, effectiveAgent);
    if (vAct) {
      const vDef = resolveVirtualCommand(vAct.name, effectiveAgent);
      if (vDef && typeof vDef["_action"] === "string") {
        dbg("chat", "intercepted virtual slash command in sendMessage", {
          command: vAct.name,
          action: vDef["_action"],
        });
        await handleVirtualCommand(vDef["_action"] as string, vAct.args);
        return;
      }
    }

    // Authoritative validation and dispatch for Pi agent slash commands
    if (effectiveAgent === "pi" && text.trim().startsWith("/")) {
      const trimmed = text.trim();
      const parsedSlash = parseSlashCommand(trimmed)!;
      const command = parsedSlash.command;
      const argsStr = parsedSlash.args;

      if (!command) {
        store.error = "Invalid Pi slash command.";
        return;
      }

      const cmdName = command.toLowerCase();
      const isVirtual = resolveVirtualCommand(cmdName, "pi") !== undefined;
      if (!isVirtual) {
        // A new Pi slash command needs only the actor/discovery shell. It must not
        // create an optimistic empty user message or an empty model turn.
        if (!store.run || !store.sessionAlive) {
          promptDraftsByRun.delete(store.run?.id ?? "");
          store.error = "";
          const newRunId = await startNewSession("", [], {
            piShell: true,
            slashCmd: `/${command}`,
          });
          if (!newRunId) return;
        }

        const piShellTitle = derivePiShellTitle({
          agent: effectiveAgent,
          prompt: store.run?.prompt,
          name: store.run?.name,
          message: text,
        });
        if (piShellTitle) {
          const renamed = await handleRename(piShellTitle);
          if (renamed) autoNameDone = true;
        }

        const commandsReady = await store.waitForPiCommandsReady();
        if (!commandsReady) {
          dbgWarn("chat", "Pi slash command discovery failed or timed out", cmdName);
          store.error = "Pi commands are unavailable (CommandUnavailable).";
          return;
        }

        // Pi's skill scanner exposes `grilling`, but Pi's command registry exposes
        // the same skill as `skill:grilling`. Accept the old bare spelling so
        // manually typed commands keep working after the picker fix.
        const resolvedCommand = resolvePiCommandName(command, store.sessionCommands);
        const resolvedCmdName = resolvedCommand.toLowerCase();
        const isRegistered = store.sessionCommands.some(
          (c) =>
            c.name.toLowerCase() === resolvedCmdName ||
            (c.aliases ?? []).some((a) => a.toLowerCase() === resolvedCmdName),
        );

        if (!isRegistered) {
          dbgWarn("chat", "unregistered Pi slash command blocked:", cmdName);
          store.error = `Command '/${command}' is not available in the current Pi session (CommandUnavailable).`;
          return;
        }

        if (store.run?.id) {
          try {
            if (resolvedCmdName === "plan" || resolvedCmdName === "goal") {
              await invokePiFeatureCommand(resolvedCommand, argsStr);
            } else {
              await api.invokePiSlashCommand(store.run.id, resolvedCommand, argsStr);
            }
          } catch (err) {
            dbgWarn("chat", "invokePiSlashCommand failed", err);
            store.error = (err as Error)?.message ?? String(err);
          }
        }
        return;
      }
    }

    // Capture a Git tree before dispatch so the final change card remains accurate even when an
    // agent edits through Bash, a formatter, an MCP tool, or a provider-specific tool name.
    if (!store.isRunning) {
      await beginTurnFileTracking();
    }

    // The draft is being sent — drop it so it can't resurface. Esp. the "" new-chat bucket: its
    // PromptInput remounts under the new run id before the clear effect flushes, so rely on this.
    promptDraftsByRun.delete(store.run?.id ?? "");

    store.error = "";
    // Follow to new reply when sending a message
    isChatAutoScroll = true;
    showChatScrollHint = false;

    // Detect slash command (same check as store timeout skip)
    const isSlash = store.isKnownSlashCommand(text);
    const slashCmd = isSlash ? (text.match(/^\/\S+/)?.[0] ?? null) : null;
    const piShellFirstMessageName = derivePiShellTitle({
      agent: effectiveAgent,
      prompt: store.run?.prompt,
      name: store.run?.name,
      message: text,
    });

    // Plan mode starts a Pi shell before the first user turn. Name that shell immediately from
    // the first real message so the sidebar does not stay at "Untitled" when the turn is slow or
    // ends with an error before the normal idle auto-name effect runs.
    if (piShellFirstMessageName) {
      const renamed = await handleRename(piShellFirstMessageName);
      if (renamed) autoNameDone = true;
    }

    try {
      if (!store.run) {
        // First message: create run.
        //
        await startNewSession(text, attachments, { slashCmd, skills });
      } else if (store.useStreamSession && !store.sessionAlive && store.run.session_id) {
        // Stopped stream session: atomic resume + send (message written to CLI stdin at spawn)
        dbg("chat", "auto-resume on send", {
          runId: store.run.id,
          sessionId: store.run.session_id,
        });
        // Make the silent recovery visible so users don't think they must click
        // "Resume Session" first. (#115)
        promptRef?.showToast(t("chat_restoringSession"), "info");
        if (slashCmd) {
          processingSlashCmd = slashCmd;
          slashCmdSeenRunning = false;
        }
        await handleResume("resume", undefined, text, attachments);
      } else {
        // Subsequent message
        if (slashCmd) {
          processingSlashCmd = slashCmd;
          slashCmdSeenRunning = false;
        }
        // A stopped Codex app-server session re-spawns inside sendMessage; refresh the
        // sidebar afterward so its status badge leaves the stale "stopped" state.
        const wasStoppedCodex =
          store.useStreamSession && !store.sessionAlive && store.run?.agent === "codex";
        await store.sendMessage(text, attachments, skills);
        if (wasStoppedCodex) window.dispatchEvent(new Event("agentcabin:runs-changed"));
        requestAnimationFrame(() => promptRef?.focus());
      }
    } catch (e) {
      store.error = String(e);
      processingSlashCmd = null;
    }
  }

  function fillPrompt(text: string) {
    promptRef?.setValue(text);
  }

  // ── Project init detection ──
  // For Claude session: check CLAUDE.md; for Codex: check AGENTS.md.
  /** Reload globally enabled capabilities and Pi commands with race guard. */
  function reloadProjectData(cwd: string) {
    const gen = ++preloadGen;
    preloadedCapabilitySkills = [];
    preloadedPiCommands = [];
    // The chat picker and both runtimes use the same global capability source.
    // The endpoint remains independent of cwd and never scans native provider
    // skill roots.
    api
      .listPiSkills(cwd || undefined)
      .then((skills) => {
        if (gen !== preloadGen) return;
        const enabledSkills = getEnabledCapabilitySkills(skills);
        preloadedCapabilitySkills = enabledSkills;
        dbg("chat", "preloaded capability-center skills", {
          count: enabledSkills.length,
        });
      })
      .catch((e) => dbgWarn("chat", "failed to preload capability-center skills", e));

    // Pi's command catalog is still needed for its native slash commands, but it
    // is separate from the shared profile skill source above.
    if (effectiveAgent === "pi") {
      api
        .listPiCommands(cwd || undefined)
        .then((cmds) => {
          if (gen !== preloadGen) return;
          preloadedPiCommands = cmds;
          dbg("chat", "preloaded Pi commands", { extensions: cmds.length });
        })
        .catch((e) => dbgWarn("chat", "failed to preload Pi commands", e));
    }
  }

  async function checkProjectInit() {
    const cwd = getSavedProjectCwd(currentRealm) || "";
    if (!cwd || cwd === "/") {
      projectInitStatus = null;
      dbg("chat", "checkProjectInit: skip (no cwd)");
      return;
    }
    const seq = ++initCheckSeq;
    try {
      const status = await api.checkProjectInit(cwd);
      dbg("chat", "checkProjectInit result", {
        cwd,
        status,
        seq,
        currentSeq: initCheckSeq,
        hasRun: !!store.run,
        isApiMode: store.isApiMode,
      });
      if (seq !== initCheckSeq) return;
      const dismissKey = `agentcabin:init-dismissed:${status.cwd}`;
      const dismissed = localStorage.getItem(dismissKey);
      if (dismissed) {
        projectInitStatus = null;
        dbg("chat", "checkProjectInit: dismissed", dismissKey);
        return;
      }
      projectInitStatus = status;
    } catch (e) {
      dbgWarn("chat", "checkProjectInit failed", e);
      if (seq === initCheckSeq) projectInitStatus = null;
    }
  }

  // ── Agent switching ──

  /**
   * Sync permission mode from agent/user settings.
   * Only syncs when the user hasn't manually changed the mode this session.
   * Does NOT set permissionModeSetByUser — that flag is only set by
   * handlePermissionModeChange() (user manual action).
   */
  function syncPermissionModeFromSettings(as: AgentSettings | null, us: UserSettings | null) {
    if (store.permissionModeSetByUser) return;
    const projectPref = projectModelPreference;
    if (projectPref?.permission_mode) {
      if (store.agent === "pi") {
        store.piPermissionState = {
          ...store.piPermissionState,
          mode: normalizePiPermissionMode(projectPref.permission_mode),
        };
        store.permissionMode = "default";
        return;
      }
      store.permissionMode = projectPref.permission_mode;
      store.permissionModeSetByUser = true;
      return;
    }
    // Without a project preference, fall back to the Pi profile default.
    // Project hydration above remains authoritative for subsequent chats.
    if (store.agent === "pi") {
      store.piPermissionState = {
        ...store.piPermissionState,
        mode: normalizePiPermissionMode(as?.permission_mode ?? "accept_edits"),
      };
      store.permissionMode = "default";
      return;
    }
    // Priority: agent.permission_mode > agent.plan_mode > user.permission_mode
    if (as?.permission_mode) {
      const cliName = APP_TO_CLI_MODE[as.permission_mode] ?? as.permission_mode;
      store.permissionMode = cliName;
    } else if (as?.plan_mode) {
      store.permissionMode = "plan";
    } else if (us?.permission_mode) {
      const cliName = APP_TO_CLI_MODE[us.permission_mode] ?? us.permission_mode;
      store.permissionMode = cliName;
    } else {
      store.permissionMode = "acceptEdits";
    }
  }

  async function handleAgentChange(newAgent: string, force = false) {
    if (!force && newAgent === store.agent) return;
    if (!force && (store.run || store.timeline.length > 0 || store.isActivelyRunning)) {
      chatToast = t("runtime_provider_switch_warning");
      setTimeout(() => {
        if (chatToast === t("runtime_provider_switch_warning")) chatToast = "";
      }, 4000);
      return;
    }
    const seq = ++agentChangeSeq;
    const agentEffortVersion = effortSelectionVersion;
    store.agent = newAgent;

    // Persist per-harness runtime choice
    if (currentHarness === "code") {
      void api.updateUserSettings({ code_default_runtime: newAgent }).catch(() => {});
    } else {
      void api.updateUserSettings({ work_default_runtime: newAgent }).catch(() => {});
    }

    // Pi's native cold-start command scan is independent of a live RPC session.
    // Re-run it when switching agents so project extensions are ready before the
    // first Pi prompt instead of waiting for session_init.
    if (!store.run && newAgent === "pi") {
      const cwd =
        folderCwdOverride ||
        (typeof window !== "undefined" ? getSavedProjectCwd(currentRealm) : "") ||
        "";
      reloadProjectData(cwd);
    }

    // Model: clear and let the restore effect rehydrate the selected agent's model.
    store.model = "";

    // Async: agentSettings
    try {
      const as = await api.getAgentSettings(newAgent);
      if (seq !== agentChangeSeq) return; // stale
      agentSettings = as;
    } catch {
      if (seq !== agentChangeSeq) return;
      agentSettings = null;
    }

    // Async: effort
    if (newAgent === "codex" || newAgent === "pi" || newAgent === "dsh" || newAgent === "grok") {
      // Codex/Pi/DSH/Grok effort lives in agent settings, not Claude's CLI config.
      if (effortSelectionVersion === agentEffortVersion) {
        currentEffort =
          newAgent === "grok" ? agentSettings?.effort || "" : agentSettings?.effort || "medium";
      }
    } else if (
      getAgentCapabilities(newAgent).ui.effortSelector &&
      getAgentCapabilities(newAgent).protocol.effortControl
    ) {
      try {
        const cfg = await api.getCliConfig();
        if (seq !== agentChangeSeq) return;
        if (effortSelectionVersion === agentEffortVersion) {
          currentEffort = typeof cfg.effortLevel === "string" ? cfg.effortLevel : "";
        }
      } catch {
        if (seq !== agentChangeSeq) return;
        if (effortSelectionVersion === agentEffortVersion) currentEffort = "";
      }
    } else {
      if (effortSelectionVersion === agentEffortVersion) currentEffort = "";
    }

    if (seq !== agentChangeSeq) return;
    // One-time migration: clear stale Claude-only agentSettings.effort. Pi and Codex use this
    // field as their source of truth and must never be migrated into Claude's CLI config.
    if (newAgent === "claude" && agentSettings?.effort) {
      api.updateAgentSettings(newAgent, { effort: "" }).catch(() => {});
    }

    // Re-sync permissionMode — reset flag (new agent needs fresh sync), then sync
    store.permissionModeSetByUser = false;
    syncPermissionModeFromSettings(agentSettings, settings);

    // Auth detection banner (Codex only)
    if (newAgent === "codex") {
      codexWarning = null;
      api
        .checkCodexAuth()
        .then((status) => {
          if (seq !== agentChangeSeq) return;
          if (!status.installed) {
            codexWarning = t("codex_notInstalled");
          } else if (!status.logged_in) {
            codexWarning = t("codex_notLoggedIn");
          } else {
            codexWarning = null;
          }
        })
        .catch(() => {});
    } else {
      codexWarning = null;
    }

    dbg("chat", "agent changed", { agent: newAgent, seq });
  }

  // ── Permission mode name translation ──
  // Store/dropdown use CLI names; UserSettings uses app names; adapter.rs maps app→CLI.
  const APP_TO_CLI_MODE: Record<string, string> = {
    ask: "default",
    "ask-all": "default",
    ask_all: "default",
    auto_read: "acceptEdits",
    auto_all: "bypassPermissions",
    "auto-grant": "bypassPermissions",
    auto_grant: "bypassPermissions",
    plan: "plan",
    auto: "auto",
    dont_ask: "dontAsk",
  };

  function getPermModeLabel(mode: string): string {
    const map: Record<string, () => string> = {
      default: () => t("prompt_permAskShort"),
      acceptEdits: () => t("prompt_permAutoReadShort"),
      bypassPermissions: () => t("prompt_permAutoAllShort"),
      plan: () => t("prompt_permPlanShort"),
      auto: () => t("prompt_permAutoShort"),
      dontAsk: () => t("prompt_permDontAskShort"),
    };
    return map[mode]?.() ?? mode;
  }

  let permissionModeChangeSeq = 0;

  async function handlePermissionModeChange(
    newMode: string,
    opts?: { toast?: boolean },
  ): Promise<boolean> {
    const seq = ++permissionModeChangeSeq;
    const oldMode = store.permissionMode;
    const oldFlag = store.permissionModeSetByUser;
    const oldPersistFailed = store.permissionModePersistFailed;
    const hadActiveSession = store.sessionAlive; // capture at entry, before awaits
    const activeRunId = store.run?.id;
    const activeSessionId = store.run?.session_id;
    const canLiveControlPermission =
      effectiveCapabilities.protocol.permissionModeControl && hadActiveSession && !!store.run;
    dbg("chat", "permission mode change", {
      from: oldMode,
      to: newMode,
      seq,
      hadActiveSession,
      canLiveControlPermission,
    });

    // Optimistic UI + protect from session_init during awaits
    store.permissionMode = newMode;
    store.permissionModeSetByUser = true;
    store.permissionModePersistFailed = false;

    try {
      await persistProjectModelPreference({ permission_mode: newMode });
    } catch (error) {
      if (seq !== permissionModeChangeSeq) return false;
      store.permissionMode = oldMode;
      store.permissionModeSetByUser = oldFlag;
      store.permissionModePersistFailed = oldPersistFailed;
      store.error = t("chat_permModeFailed", { mode: newMode, error: String(error) });
      return false;
    }

    if (effectiveAgent === "codex") {
      if (seq !== permissionModeChangeSeq) return false;
      // Live apply to the running app-server session. Future sessions consume
      // the project preference through the composer startup override.
      let liveApplied = false;
      if (canLiveControlPermission && store.run) {
        try {
          await api.setPermissionMode(store.run.id, newMode);
          liveApplied = true;
          dbg("chat", "codex permission mode hot-switched via control protocol", { newMode });
        } catch (e) {
          dbgWarn("chat", "codex permission mode hot-switch failed (persisted for next spawn):", e);
        }
      }
      if (seq !== permissionModeChangeSeq) return false;
      if (opts?.toast !== false) {
        showChatToast(
          liveApplied
            ? t("toast_permissionMode", { mode: getPermModeLabel(newMode) })
            : t("toast_permissionModeNextTurn", { mode: getPermModeLabel(newMode) }),
        );
      }
      return true;
    }

    if (effectiveAgent === "pi") {
      if (canLiveControlPermission && store.run) {
        try {
          await api.setPermissionMode(store.run.id, newMode);
        } catch (e) {
          dbgWarn("chat", "pi permission mode bridge unsupported:", e);
          if (seq !== permissionModeChangeSeq) return false;
          store.permissionMode = oldMode;
          store.permissionModeSetByUser = oldFlag;
          store.permissionModePersistFailed = oldPersistFailed;
          store.error = `Pi permission mode bridge is not supported yet (Unsupported).`;
          return false;
        }
      }
      return true;
    }

    // Grok's ACP actor accepts permission policy at process startup but does not
    // expose a live mutation primitive. Restart the
    // current actor with the same ACP session id so the new policy is effective
    // before the next tool call.
    if (effectiveAgent === "grok") {
      if (seq !== permissionModeChangeSeq) return false;
      let grokRestartFailed = false;
      if (seq !== permissionModeChangeSeq) return false;

      if (hadActiveSession) {
        if (!activeRunId || !activeSessionId) {
          grokRestartFailed = true;
          store.error =
            "Grok permission mode could not be applied: the active session has no session_id.";
        } else {
          try {
            // Grok has no ACP mode mutation endpoint. connectSession() stops the
            // old actor and resumes this session with --permission-mode=<newMode>.
            await store.connectSession(activeRunId, activeSessionId, newMode);
            dbg("chat", "grok permission mode applied by restarting ACP actor", {
              runId: activeRunId,
              sessionId: activeSessionId,
              newMode,
            });
          } catch (error) {
            grokRestartFailed = true;
            store.error = `Grok permission mode restart failed: ${String(error)}`;
            dbgWarn("chat", "grok permission mode restart failed:", error);
          }
        }
      }

      if (seq !== permissionModeChangeSeq) return false;
      if (opts?.toast !== false && !grokRestartFailed) {
        showChatToast(t("toast_permissionMode", { mode: getPermModeLabel(newMode) }));
      }
      return !grokRestartFailed;
    }

    // Claude path: hot-switch via control protocol if active session
    if (canLiveControlPermission && store.run) {
      try {
        await api.setPermissionMode(store.run.id, newMode);
        dbg("chat", "permission mode changed via control protocol", { newMode });
      } catch (e) {
        if (seq !== permissionModeChangeSeq) return false;
        // Restore mode, flag, AND persistFailed
        store.permissionMode = oldMode;
        store.permissionModeSetByUser = oldFlag;
        store.permissionModePersistFailed = oldPersistFailed;
        dbgWarn("chat", "permission mode change failed:", e);
        store.error = t("chat_permModeFailed", { mode: newMode, error: String(e) });
        if (opts?.toast !== false) {
          showChatToast(t("toast_permissionFailed"));
        }
        return false;
      }
    }

    if (seq !== permissionModeChangeSeq) return false;

    if (opts?.toast !== false) {
      showChatToast(t("toast_permissionMode", { mode: getPermModeLabel(newMode) }));
    }

    return seq === permissionModeChangeSeq;
  }

  async function handlePlanModeChange(active: boolean): Promise<void> {
    if (!store.usesSessionPlanMode) {
      await handlePermissionModeChange(active ? "plan" : "default");
      return;
    }
    if (!store.run || !store.sessionAlive) {
      // No live protocol session to update; keep the composer state truthful
      // until the next session starts.
      store.sessionMode = active ? "plan" : "default";
      return;
    }
    try {
      await api.setSessionMode(store.run.id, active ? "plan" : "default");
      // Reflect the successful native mode change immediately; the protocol
      // event will reconcile this value when it arrives.
      store.sessionMode = active ? "plan" : "default";
    } catch (e) {
      dbgWarn("chat", "agent session plan mode change failed:", e);
      store.error = `Plan mode change failed: ${String(e)}`;
    }
  }

  async function handleClearPlanChip(): Promise<void> {
    if (getComposerClearAction(effectiveAgent, "plan") === "pi-plan-exit") {
      await handlePiPlan("exit");
      return;
    }
    await handlePlanModeChange(false);
  }

  async function handleClearGoalChip(): Promise<void> {
    try {
      const clearAction = getComposerClearAction(effectiveAgent, "goal");
      if (clearAction === "pi-goal-clear") {
        const applied = await invokePiFeatureCommand("goal", "clear");
        if (applied) {
          store.piGoalState = { ...store.piGoalState, phase: "inactive", objective: undefined };
        }
      } else if (clearAction === "codex-goal-clear" && store.run?.id) {
        await api.clearGoal(store.run.id);
        store.setGoal(null);
      } else {
        // Claude/Grok goals are prompt-backed compatibility state. Clearing the
        // local state keeps future prompts clean without spending another turn.
        store.setGoal(null);
      }
    } catch (error) {
      dbgWarn("goal", "clear goal chip failed:", error);
      store.error = `Goal clear failed: ${String(error)}`;
    }
  }

  async function handleSessionModeChange(mode: string): Promise<void> {
    if (!store.run || !store.sessionAlive) return;
    if (!effectiveCapabilities.protocol.sessionModeControl) return;
    if (!store.sessionModes.some((candidate) => candidate.id === mode)) return;
    try {
      await api.setSessionMode(store.run.id, mode);
    } catch (e) {
      dbgWarn("chat", "agent session mode change failed:", e);
      store.error = `Session mode change failed: ${String(e)}`;
    }
  }

  // ── HTML Export ──

  async function handleExportHtml() {
    if (!store.run) {
      dbgWarn("chat", "handleExportHtml: no run");
      showChatToast(t("export_noConversation"));
      return;
    }
    dbg("chat", "handleExportHtml: start");

    let html: string;
    let title: string;
    const prevLimit = renderLimit;
    try {
      // Force full render (unlimited)
      renderLimit = Infinity;
      await tick();
      await new Promise((r) => requestAnimationFrame(() => r(undefined)));

      // Re-query after Svelte re-render (DOM may have been replaced)
      const rootEl = document.querySelector<HTMLElement>("[data-conversation-root]");
      if (!rootEl) {
        dbgWarn("chat", "handleExportHtml: data-conversation-root not found");
        showChatToast(t("export_noConversation"));
        return;
      }

      const { exportConversationToHtml, buildExportFilename: buildFn } =
        await import("$lib/utils/html-export");

      title = store.run.name ?? store.run.prompt?.slice(0, 80) ?? "Untitled";
      html = await exportConversationToHtml(rootEl, {
        title,
        sessionInfo: {
          model: store.model,
          cwd: store.effectiveCwd,
          startedAt: store.run.started_at,
          turnCount: store.numTurns || store.timeline.filter((e) => e.kind === "user").length,
        },
      });

      // Restore the progressive render limit immediately (HTML is already captured).
      renderLimit = prevLimit;

      const path = await platform.dialog.save({
        defaultPath: buildFn(title),
        filters: [{ name: "HTML", extensions: ["html"] }],
      });
      if (!path) {
        dbg("chat", "handleExportHtml: user cancelled");
        return;
      }

      await api.writeHtmlExport(path, html);
      dbg("chat", "handleExportHtml: done", { path });
      showChatToast(t("export_htmlSuccess"));
    } catch (e) {
      dbgWarn("chat", "handleExportHtml failed", e);
      showChatToast(t("export_htmlFailed"));
    } finally {
      // Ensure restore even on early return paths
      renderLimit = prevLimit;
    }
  }

  // The per-agent provider binding (agent_provider_bindings) is no longer
  // configured from the settings screen — the chat model picker is the single
  // source of truth. Keep the persisted binding in sync so runtimes that
  // resolve provider credentials at spawn time (Grok/DSH managed config, Pi
  // bare-model fallback, Codex app mode) follow the user's chat selection.
  async function syncProviderBindingFromSelection(newModel: string) {
    if (!settings) return;
    const agent = effectiveAgent;
    if (!["claude", "codex", "pi", "grok", "dsh"].includes(agent)) return;

    const pid = newModel.indexOf("/") > 0 ? newModel.slice(0, newModel.indexOf("/")) : "";
    const provider = pid
      ? (settings.global_providers ?? []).find(
          (item) => item.id === pid && isProviderCompatible(agent as ProviderAgent, item.protocol),
        )
      : undefined;

    const current: AgentProviderBindings = settings.agent_provider_bindings ?? {
      claude: { mode: "custom" },
      codex: { mode: "custom" },
      pi: { mode: "custom" },
      grok: { mode: "custom" },
      dsh: { mode: "custom" },
    };
    const existing = current[agent as ProviderAgent];

    let nextBinding: AgentProviderBinding;
    if (provider) {
      // Explicit `provider-id/model-id` selection → bind that provider.
      nextBinding = {
        mode: "custom",
        provider_id: provider.id,
        model: newModel.slice(provider.id.length + 1),
      };
    } else {
      const bareId = newModel.includes("/") ? newModel.slice(newModel.indexOf("/") + 1) : newModel;
      const boundProviderModels =
        existing?.mode === "custom" && existing.provider_id
          ? ((settings.global_providers ?? []).find((item) => item.id === existing.provider_id)
              ?.models ?? [])
          : [];
      const belongsToBoundProvider = boundProviderModels.some((item) => item.id === bareId);
      if (belongsToBoundProvider && existing?.provider_id) {
        // Bare model that still belongs to the currently bound provider — keep it.
        nextBinding = { mode: "custom", provider_id: existing.provider_id, model: bareId };
      } else if (existing?.mode !== "custom") {
        return; // Already native, nothing to change.
      } else {
        // Native/subscription model picked while a third-party provider was
        // bound → fall back to CLI auth so the next spawn uses native credentials.
        nextBinding = { mode: "cli" };
      }
    }

    if (
      existing?.mode === nextBinding.mode &&
      existing?.provider_id === nextBinding.provider_id &&
      existing?.model === nextBinding.model
    ) {
      return;
    }

    try {
      settings = await api.updateUserSettings({
        agent_provider_bindings: {
          ...current,
          [agent]: nextBinding,
        } as AgentProviderBindings,
      });
      dbg("chat", "provider binding synced from model selection", {
        agent,
        binding: nextBinding,
      });
    } catch (e) {
      dbgWarn("chat", "failed to sync provider binding from model selection", e);
    }
  }

  async function handleModelChange(newModel: string) {
    dbg("chat", "model change", { agent: effectiveAgent, from: store.model, to: newModel });
    store.model = newModel;
    await syncProviderBindingFromSelection(newModel);

    if (store.run) {
      store.run = { ...store.run, model: newModel };
      try {
        await api.updateRunModel(store.run.id, newModel);
      } catch (e) {
        dbgWarn("chat", "failed to persist run model", e);
      }
    }

    if (!store.run || !store.sessionAlive || !effectiveCapabilities.protocol.sessionSetModel) {
      return;
    }

    // Claude-compatible third-party providers resolve the model from spawn-time environment
    // variables rather than the native session control channel.
    const modelControlBlockedByPlatform =
      effectiveAgent === "claude" && !!store.platformId && store.platformId !== "anthropic";
    if (modelControlBlockedByPlatform) return;

    try {
      await api.setSessionModel(store.run.id, newModel);
      dbg("chat", "model hot-switched via protocol capability", {
        agent: effectiveAgent,
        newModel,
      });
    } catch (e) {
      dbgWarn("chat", "model hot-switch failed (persisted for next session):", e);
    }
  }

  async function handleEffortChange(newEffort: string) {
    const selectionVersion = ++effortSelectionVersion;
    dbg("chat", "effort change", { agent: effectiveAgent, from: currentEffort, to: newEffort });
    if (!effectiveCapabilities.protocol.effortControl) {
      dbgWarn("chat", "effort change ignored: protocol capability unavailable", {
        agent: effectiveAgent,
      });
      return;
    }

    if (store.sessionAlive && store.run) {
      try {
        await api.setEffort(store.run.id, newEffort);
        if (selectionVersion !== effortSelectionVersion) return;
        dbg("chat", "effort hot-switched via protocol capability", {
          agent: effectiveAgent,
          newEffort,
        });
      } catch (e) {
        if (selectionVersion !== effortSelectionVersion) return;
        const message = e instanceof Error ? e.message : String(e);
        dbgWarn("chat", "effort hot-switch failed:", e);
        showChatToast(`思考档位切换失败：${message}`);
        return;
      }
    }

    currentEffort = newEffort;
    if (store.run) {
      store.run = { ...store.run, effort: newEffort };
    }
    if (store.run?.id) {
      saveConversationEffort(store.run.id, newEffort);
      void api.updateRunEffort(store.run.id, newEffort).catch((e) => {
        dbgWarn("chat", "failed to persist conversation effort:", e);
      });
    }
    void persistProjectModelPreference({ effort: newEffort }).catch(() => {});
  }

  async function handleAuthModeChange(mode: string) {
    dbg("chat", "auth mode change", { from: store.authMode, to: mode });
    store.authMode = mode;
    try {
      await api.updateUserSettings({ auth_mode: mode } as Partial<UserSettings>);
      // Refresh auth overview after mode change
      authOverview = await api.getAuthOverview();
    } catch (e) {
      dbgWarn("chat", "failed to persist auth mode change", e);
    }
  }

  async function checkAllLocalProxies() {
    const localPresets = PLATFORM_PRESETS.filter((p) => p.category === "local");
    const results = await Promise.allSettled(
      localPresets.map((p) => {
        const cred = findCredential(settings?.platform_credentials ?? [], p.id);
        const url = cred?.base_url || p.base_url;
        return api.detectLocalProxy(p.id, url);
      }),
    );
    const statuses: Record<string, { running: boolean; needsAuth: boolean }> = {};
    results.forEach((r, i) => {
      if (r.status === "fulfilled") {
        statuses[localPresets[i].id] = { running: r.value.running, needsAuth: r.value.needsAuth };
      } else {
        statuses[localPresets[i].id] = { running: false, needsAuth: false };
      }
    });
    localProxyStatuses = statuses;
    dbg("chat", "checkAllLocalProxies", statuses);
  }

  async function handlePlatformChange(platformId: string) {
    dbg("chat", "platform change", { from: store.platformId, to: platformId });
    store.platformId = platformId;

    // Auto-switch model to provider's default when switching to a third-party platform.
    // Only for stream-session agents — Codex manages its own model via CLI.
    // Priority: credential.models (user-configured) > preset.models (static defaults)
    if (store.useStreamSession) {
      const cred = findCredential(settings?.platform_credentials ?? [], platformId);
      const preset = PLATFORM_PRESETS.find((p) => p.id === platformId);
      const models = cred?.models?.length ? cred.models : preset?.models;
      if (models?.length) {
        const defaultModel = models[0];
        dbg("chat", "auto-switch model for platform", { platformId, model: defaultModel });
        store.model = defaultModel;
      } else if (platformId === "anthropic") {
        // Switching back to Anthropic: always overwrite — don't keep third-party model;
        // don't fallback to settings.default_model which might be contaminated.
        const cliModel = getCliCurrentModel();
        store.model = cliModel || "";
        dbg("chat", "restore model on switch to anthropic", { cliModel, using: store.model });
      } else {
        // Custom/unknown platform without preset models: clear model
        // (let CLI use whatever default it has, or the user can set manually)
        store.model = "";
      }
    }

    // Only persist default_model when switching to Anthropic with a validated CLI model.
    // Don't persist empty or potentially-stale model values.
    const persistUpdate: Partial<UserSettings> = { active_platform_id: platformId };
    if (platformId === "anthropic") {
      const validated = getCliCurrentModel();
      if (validated) persistUpdate.default_model = validated;
    }
    try {
      await api.updateUserSettings(persistUpdate);
    } catch (e) {
      dbgWarn("chat", "failed to persist platform change", e);
    }
    // Refresh local proxy statuses after platform switch
    checkAllLocalProxies();
  }

  function appendCommandOutput(text: string) {
    const cmdId = uuid();
    store.timeline = [
      ...store.timeline,
      {
        kind: "command_output",
        id: cmdId,
        anchorId: cmdId,
        content: text,
        ts: new Date().toISOString(),
      },
    ];
  }

  async function handleRename(name: string): Promise<boolean> {
    if (!store.run) return false;
    try {
      await api.renameRun(store.run.id, name);
      store.run = { ...store.run, name };
      dispatchRunMutation({
        kind: "update",
        runId: store.run.id,
        patch: { name },
      });
      dbg("chat", "renamed run", { id: store.run.id, name });
      return true;
    } catch (e) {
      dbgWarn("chat", "rename failed", e);
      return false;
    }
  }

  // Auto-name: on first idle, generate title from prompt (one-shot per run)
  $effect(() => {
    const result = shouldAutoName({
      phase: store.phase,
      runId: store.run?.id,
      runName: store.run?.name,
      prompt: store.run?.prompt,
      autoNameDone,
    });
    if (result.fire && result.autoName) {
      autoNameDone = true;
      handleRename(result.autoName);
    }
  });

  async function handleFastModeSwitch(mode: "on" | "off") {
    const enabling = mode === "on";
    const current = store.fastModeState === "on";
    if (enabling === current) {
      appendCommandOutput(t(enabling ? "fast_alreadyOn" : "fast_alreadyOff"));
      return;
    }
    try {
      await api.updateCliConfig({ fastMode: enabling });
      store.fastModeState = enabling ? "on" : "";
      dbg("chat", "fastMode set", { mode });
      showChatToast(t(enabling ? "toast_fastModeOn" : "toast_fastModeOff"));
      appendCommandOutput(t(enabling ? "fast_enabled" : "fast_disabled"));
    } catch (e) {
      dbgWarn("chat", "fastMode set failed:", e);
    }
  }

  async function handleBtwSend(question: string) {
    if (!store.run?.id) return;
    dbg("chat", "btwSend", { runId: store.run.id, question: question.slice(0, 50) });
    btwState = { active: true, btwId: null, question, answer: "", error: null, loading: true };
    try {
      const btwId = await api.sideQuestion(store.run.id, question);
      btwState.btwId = btwId;
    } catch (e) {
      btwState.error = String(e);
      btwState.loading = false;
    }
  }

  // ── Preview helpers ──

  function isLocalhostUrl(url: string): boolean {
    try {
      const u = new URL(url);
      return (
        ["localhost", "127.0.0.1", "0.0.0.0", "[::1]"].includes(u.hostname) &&
        ["http:", "https:"].includes(u.protocol)
      );
    } catch {
      return false;
    }
  }

  async function openPreview(url: string): Promise<"ok" | "invalid_url" | "open_failed"> {
    dbg("preview", "openPreview", { url });
    if (!isLocalhostUrl(url)) return "invalid_url";

    const instanceId = uuid();
    resetPreviewState();
    previewInstanceId = instanceId;

    try {
      await api.openPreviewWindow(url, instanceId);
      detectedPreviewUrl = url;
      localStorage.setItem("agentcabin:preview-url", url);
      return "ok";
    } catch (e) {
      dbgWarn("preview", "openPreview failed", e);
      resetPreviewState();
      const msg = String(e);
      if (msg.startsWith("preview_invalid_url:")) return "invalid_url";
      return "open_failed";
    }
  }

  async function closePreview() {
    dbg("preview", "closePreview");
    resetPreviewState();
    await api.closePreviewWindow();
  }

  function resetPreviewState() {
    previewInstanceId = "";
  }

  /** Open preview + show result as command output. Returns true on success. */
  async function openPreviewAndNotify(url: string): Promise<boolean> {
    const result = await openPreview(url);
    if (result === "ok") {
      appendCommandOutput(t("preview_opened"));
      return true;
    }
    appendCommandOutput(t(result === "invalid_url" ? "preview_invalidUrl" : "preview_openFailed"));
    return false;
  }

  function formatElementContext(sel: ElementSelection): string {
    const lines = [
      `[Page Element]`,
      `URL: ${sel.url}`,
      `Path: ${sel.domPath}`,
      `Tag: ${sel.tagName}`,
    ];
    if (sel.textContent) lines.push(`Text: "${sel.textContent.slice(0, 200)}"`);
    const attrs = Object.entries(sel.attributes)
      .filter(([, v]) => v)
      .map(([k, v]) => `${k}="${v}"`)
      .join(", ");
    if (attrs) lines.push(`Attributes: ${attrs}`);
    const styles = Object.entries(sel.styleSummary)
      .map(([k, v]) => `${k}=${v}`)
      .join(", ");
    if (styles) lines.push(`Styles: ${styles}`);
    lines.push(`HTML: ${sel.outerHtmlSnippet.slice(0, 500)}`);
    return lines.join("\n");
  }

  async function handleRalphCancel() {
    if (!store.run?.id) return;
    try {
      const result = await api.cancelRalphLoop(store.run.id);
      if (result.immediate) {
        appendCommandOutput(`Loop cancelled (iteration ${result.iteration})`);
      } else {
        appendCommandOutput(
          `Loop will stop after current iteration (iteration ${result.iteration})`,
        );
      }
    } catch (err) {
      appendCommandOutput(
        `Failed to cancel loop: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }

  async function handleVirtualCommand(action: string, args: string) {
    dbg("chat", "virtualCommand", { action, args });
    // Guard: block excluded virtual commands for the current agent.
    // Emits user-visible feedback so gated commands don't fail silently when
    // typed directly (the menu already hides them, but direct invocation,
    // history replay, and tests can still reach here).
    const vDef =
      VIRTUAL_COMMANDS.find(
        (v) =>
          v["_action"] === action &&
          !(Array.isArray(v["_excludeAgents"]) && v["_excludeAgents"].includes(effectiveAgent)),
      ) ?? VIRTUAL_COMMANDS.find((v) => v["_action"] === action);
    if (vDef) {
      const excluded = vDef["_excludeAgents"];
      if (Array.isArray(excluded) && excluded.includes(effectiveAgent)) {
        dbg("chat", "virtualCommand blocked for agent", { action, agent: effectiveAgent });
        appendCommandOutput(
          t("slash_notSupportedForAgent", {
            command: vDef.name,
            agent: effectiveAgent === "codex" ? "Codex" : "Claude",
          }),
        );
        return;
      }
    }
    if (action === "copy-last") {
      const lastAssistant = [...store.timeline].reverse().find((e) => e.kind === "assistant");
      if (lastAssistant && lastAssistant.kind === "assistant" && lastAssistant.content) {
        try {
          await navigator.clipboard.writeText(lastAssistant.content);
          const chars = lastAssistant.content.length;
          const lines = lastAssistant.content.split("\n").length;
          appendCommandOutput(
            t("chat_copiedClipboard", { chars: String(chars), lines: String(lines) }),
          );
          dbg("chat", "copied last response", { chars, lines });
        } catch (e) {
          dbgWarn("chat", "copy failed", e);
          appendCommandOutput(t("chat_copyFailed"));
        }
      } else {
        appendCommandOutput(t("chat_noResponseToCopy"));
      }
    } else if (action === "rename-session") {
      if (!store.run) {
        appendCommandOutput(t("chat_noSessionToRename"));
        return;
      }
      if (args) {
        // With args: rename locally
        await handleRename(args);
        appendCommandOutput(t("chat_sessionRenamed", { name: args }));
      } else if (store.sessionAlive) {
        // No args + session alive: send /rename to CLI (AI-generated name)
        await sendMessage("/rename", []);
      } else {
        appendCommandOutput("Usage: /rename <name>");
      }
    } else if (action === "toggle-plan") {
      const entering = store.permissionMode !== "plan";
      const newMode = entering ? "plan" : "default";
      const ok = await handlePermissionModeChange(newMode, { toast: false });
      if (ok) {
        appendCommandOutput(entering ? "Plan mode enabled" : "Plan mode disabled");
        // If instructions provided, send them as a message
        if (args && entering) {
          await sendMessage(args, []);
        }
      }
      // On failure, handlePermissionModeChange already sets store.error
    } else if (action === "show-help") {
      const allCmds = filterNativeSlashCommands(
        nativeSlashCommands,
        new Set(piAvailableSkillNames),
      );
      appendCommandOutput(buildHelpText(allCmds));
    } else if (action === "run-doctor") {
      try {
        dbg("doctor", "run-doctor triggered", { cwd: store.effectiveCwd });
        const cwd = store.effectiveCwd || getSavedProjectCwd(currentRealm) || "";
        const mcpSvrs = store.sessionAlive ? store.mcpServers : undefined;
        const report = await buildDoctorReport(cwd, mcpSvrs);
        appendCommandOutput(report);
      } catch (err) {
        dbgWarn("doctor", "run_diagnostics failed", err);
        appendCommandOutput(
          `❌ ${t("doctor_failed")}: ${err instanceof Error ? err.message : String(err)}`,
        );
      }
    } else if (action === "show-status") {
      if (!hasSidebarData) {
        appendCommandOutput(t("statusPanel_noSession"));
        return;
      }
      if (sidebarCollapsed) sidebarCollapsed = false;
      sidebarRequestedTab = "info";
    } else if (action === "list-todos") {
      // Escape markdown special chars in todo content
      const esc = (s: string) => s.replace(/([\\*_~`[\]#>|])/g, "\\$1");

      // Same source as the TodoPanel (Tasks system or legacy TodoWrite). Empty covers
      // both "no tasks yet" and an empty list — CLI /todos produces no timeline event,
      // so there's nothing to send.
      const tasks = store.panelTasks;
      if (tasks.length === 0) {
        appendCommandOutput(t("todos_empty"));
      } else {
        const lines = tasks.map((task) => {
          const text = esc(task.text);
          if (task.status === "completed") return `- [x] ~~${text}~~`;
          if (task.status === "in_progress") return `- [ ] **⏳ ${text}**`;
          return `- [ ] ${text}`;
        });
        appendCommandOutput(lines.join("\n"));
      }
    } else if (action === "show-diff") {
      const cwd = store.effectiveCwd || getSavedProjectCwd(currentRealm) || "";
      if (!cwd) {
        appendCommandOutput(t("diff_noCwd"));
        return;
      }
      try {
        dbg("chat", "show-diff", { cwd });
        const [unstaged, staged] = await Promise.all([
          api.getGitDiff(cwd, false),
          api.getGitDiff(cwd, true),
        ]);
        if (!unstaged.trim() && !staged.trim()) {
          appendCommandOutput(t("diff_noChanges"));
          return;
        }
        // Add source-file line numbers parsed from @@ hunk headers
        function addLineNumbers(raw: string): string {
          const lines = raw.split("\n");
          const out: string[] = [];
          let oldLn = 0,
            newLn = 0;
          for (const line of lines) {
            if (line.startsWith("@@")) {
              const m = line.match(/@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
              if (m) {
                oldLn = parseInt(m[1]);
                newLn = parseInt(m[2]);
              }
              out.push(line);
            } else if (
              line.startsWith("diff ") ||
              line.startsWith("index ") ||
              line.startsWith("--- ") ||
              line.startsWith("+++ ")
            ) {
              out.push(line);
            } else if (line.startsWith("+")) {
              out.push(`+${String(newLn).padStart(4)} │ ${line.slice(1)}`);
              newLn++;
            } else if (line.startsWith("-")) {
              out.push(`-${String(oldLn).padStart(4)} │ ${line.slice(1)}`);
              oldLn++;
            } else if (line.length > 0 && line[0] === " ") {
              out.push(` ${String(newLn).padStart(4)} │ ${line.slice(1)}`);
              oldLn++;
              newLn++;
            } else {
              out.push(line);
            }
          }
          return out.join("\n");
        }
        const parts: string[] = [];
        if (unstaged.trim()) {
          parts.push(
            `### ${t("diff_unstaged")}\n\n\`\`\`diff\n${addLineNumbers(unstaged.trimEnd())}\n\`\`\``,
          );
        }
        if (staged.trim()) {
          parts.push(
            `### ${t("diff_staged")}\n\n\`\`\`diff\n${addLineNumbers(staged.trimEnd())}\n\`\`\``,
          );
        }
        appendCommandOutput(parts.join("\n\n"));
      } catch (err) {
        dbgWarn("chat", "show-diff failed", err);
        appendCommandOutput(
          `${t("diff_failed")}: ${err instanceof Error ? err.message : String(err)}`,
        );
      }
    } else if (action === "list-tasks") {
      const tasks = [...store.taskNotifications.values()];

      if (!args) {
        // /tasks (no args) — list all tasks as a table
        dbg("chat", "list-tasks", { count: store.taskNotifications.size });
        if (tasks.length === 0) {
          appendCommandOutput(t("slashTasks_empty"));
          return;
        }
        // Sort: active first, then by most recent
        const sorted = tasks.sort((a, b) => {
          const aActive =
            a.status !== "completed" && a.status !== "failed" && a.status !== "error" ? 1 : 0;
          const bActive =
            b.status !== "completed" && b.status !== "failed" && b.status !== "error" ? 1 : 0;
          if (aActive !== bActive) return bActive - aActive;
          return b.startedAt - a.startedAt;
        });
        const now = Date.now();
        const elapsed = (ms: number) => {
          const sec = Math.floor((now - ms) / 1000);
          if (sec < 60) return `${sec}s`;
          const min = Math.floor(sec / 60);
          if (min < 60) return `${min}m`;
          return `${Math.floor(min / 60)}h${min % 60}m`;
        };
        const lines: string[] = [
          "| ID | Type | Status | Description | Elapsed |",
          "|-----|------|--------|-------------|---------|",
        ];
        for (const task of sorted) {
          const shortId = task.task_id.length > 12 ? task.task_id.slice(0, 12) + "…" : task.task_id;
          const taskType = task.task_type || "—";
          const desc =
            (task.summary || task.message || "").length > 50
              ? (task.summary || task.message || "").slice(0, 50) + "…"
              : task.summary || task.message || "—";
          lines.push(
            `| \`${shortId}\` | ${taskType} | ${task.status} | ${desc} | ${elapsed(task.startedAt)} |`,
          );
        }
        lines.push("");
        lines.push(t("slashTasks_hint"));
        appendCommandOutput(lines.join("\n"));
      } else {
        // /tasks <id> — show detail for a specific task
        dbg("chat", "list-tasks:detail", { id: args });

        // Exact match first, then prefix match
        let matches = tasks.filter((t) => t.task_id === args);
        if (matches.length === 0) {
          matches = tasks.filter((t) => t.task_id.startsWith(args));
        }

        if (matches.length === 0) {
          dbg("chat", "list-tasks:detail", { id: args, found: false });
          appendCommandOutput(t("slashTasks_notFound", { id: args }));
        } else if (matches.length === 1) {
          const task = matches[0];
          const hasOutput = !!task.output_file;
          dbg("chat", "list-tasks:detail", { id: args, found: true, hasOutput });
          const now = Date.now();
          const sec = Math.floor((now - task.startedAt) / 1000);
          const meta = [
            `| Field | Value |`,
            `|-------|-------|`,
            `| ID | \`${task.task_id}\` |`,
            `| Status | ${task.status} |`,
            `| Type | ${task.task_type || "—"} |`,
            `| Description | ${task.message || "—"} |`,
            task.summary ? `| Summary | ${task.summary} |` : null,
            `| Elapsed | ${sec}s |`,
            task.output_file ? `| Output file | \`${task.output_file}\` |` : null,
          ]
            .filter(Boolean)
            .join("\n");

          if (task.output_file) {
            try {
              const raw = await api.readTaskOutput(task.output_file);
              dbg("chat", "readTaskOutput", { path: task.output_file, ok: true });
              // Frontend truncation: last 200 lines
              const allLines = raw.split("\n");
              const trimmed =
                allLines.length > 200
                  ? `... (${allLines.length - 200} lines truncated)\n${allLines.slice(-200).join("\n")}`
                  : raw;
              appendCommandOutput(`${meta}\n\n**Output:**\n\`\`\`\n${trimmed}\n\`\`\``);
            } catch (err) {
              dbgWarn("chat", "readTaskOutput failed", err);
              appendCommandOutput(
                `${meta}\n\n${t("slashTasks_outputError", { error: err instanceof Error ? err.message : String(err) })}`,
              );
            }
          } else {
            appendCommandOutput(meta);
          }
        } else {
          // Multiple matches — ambiguous
          const list = matches.map((m) => `- \`${m.task_id}\` (${m.status})`).join("\n");
          appendCommandOutput(`${t("slashTasks_ambiguous", { id: args })}\n${list}`);
        }
      }
    } else if (action === "toggle-fast") {
      const arg = args.toLowerCase();
      if (arg === "on" || arg === "off") {
        await handleFastModeSwitch(arg);
      } else if (arg === "") {
        const enabling = store.fastModeState !== "on";
        await handleFastModeSwitch(enabling ? "on" : "off");
      } else {
        appendCommandOutput(t("fast_usage"));
      }
    } else if (action === "add-dir") {
      try {
        await executeAddDir(
          {
            agent: effectiveAgent,
            capabilities: effectiveCapabilities,
            sessionAlive: store.sessionAlive,
            args,
          },
          {
            openDirectoryDialog: async (title) => {
              const result = await platform.dialog.open({ directory: true, title });
              return typeof result === "string" ? result : null;
            },
            sendMessage: (text) => sendMessage(text, []),
            getAgentSettings: api.getAgentSettings,
            updateAgentSettings: api.updateAgentSettings,
            appendOutput: appendCommandOutput,
            t: t as (key: string, params?: Record<string, string>) => string,
          },
        );
      } catch (err) {
        dbgWarn("chat", "add-dir failed", err);
        appendCommandOutput(
          t("chat_addDirFailed", {
            error: err instanceof Error ? err.message : String(err),
          }),
        );
      }
    } else if (action === "clear-context") {
      if (store.isRunning) {
        appendCommandOutput(t("chat_clearSessionBusy"));
        return;
      }
      if (store.useStreamSession && store.sessionAlive) {
        void store.stop().catch((e) => dbgWarn("chat", "clear-context stop failed", e));
      }
      projectModelPreferenceAppliedToken = ""; // clear-context 等同于新会话
      store.loadRun("", xtermRef);
      goto("/chat", { replaceState: true });
      window.dispatchEvent(new Event("agentcabin:runs-changed"));
      window.dispatchEvent(new CustomEvent("agentcabin:new-chat", { detail: {} }));
    } else if (action === "show-context") {
      const tokens = store.totalTokens || 0;
      const limit = 200000;
      const pct = Math.round((tokens / limit) * 100);
      const text = [
        "**Context Usage Overview**",
        `- Model: ${store.model || effectiveAgent}`,
        `- Tokens: ${tokens.toLocaleString()} / ${(limit / 1000).toFixed(0)}k (${pct}%)`,
        `- Messages in conversation: ${store.timeline.length}`,
      ].join("\n");
      appendCommandOutput(text);
    } else if (action === "rewind") {
      if (!store.run) {
        appendCommandOutput(t("rewind_noSession"));
      } else if (!store.sessionAlive) {
        appendCommandOutput(t("rewind_sessionEnded"));
      } else if (store.isRunning) {
        appendCommandOutput(t("rewind_sessionBusy"));
      } else {
        handleRewind();
      }
    } else if (action === "open-permissions") {
      window.dispatchEvent(new CustomEvent("agentcabin:open-permissions"));
    } else if (action === "open-stickers") {
      const url = "https://www.stickermule.com/claudecode";
      dbg("chat", "open-stickers", { url });
      try {
        await platform.shell.openExternal(url);
      } catch (err) {
        dbgWarn("chat", "open-stickers: shell open failed, fallback", err);
        window.open(url, "_blank");
      }
      appendCommandOutput("Opening sticker page in browser…");
    } else if (action === "start-ralph-loop") {
      // Defense-in-depth: even if menu/parse layers miss it, Ralph requires
      // a long-lived stream-session — Codex pipe-exec is incompatible.
      if (effectiveAgent === "codex") {
        appendCommandOutput("Ralph loop is not supported for Codex sessions yet.");
        return;
      }
      const parsed = parseRalphArgs(args);
      if (!parsed.prompt) {
        appendCommandOutput(
          "Usage: /ralph <prompt> [--max-iterations N] [--completion-promise TEXT]",
        );
        return;
      }
      try {
        // If no active session, send the prompt as a normal message first to bootstrap
        if (!store.run?.id || !store.sessionAlive) {
          await sendMessage(parsed.prompt, []);
          // Wait briefly for session to be alive (actor created)
          let retries = 0;
          while (!store.sessionAlive && retries < 20) {
            await new Promise((r) => setTimeout(r, 100));
            retries++;
          }
          if (!store.run?.id || !store.sessionAlive) {
            appendCommandOutput("Failed to start session for ralph loop.");
            return;
          }
        }
        await api.startRalphLoop(
          store.run.id,
          parsed.prompt,
          parsed.maxIterations,
          parsed.completionPromise,
        );
        appendCommandOutput(
          `Ralph loop started (max: ${parsed.maxIterations || "unlimited"}, promise: ${parsed.completionPromise ?? "none"})`,
        );
      } catch (err) {
        appendCommandOutput(
          `Failed to start loop: ${err instanceof Error ? err.message : String(err)}`,
        );
      }
    } else if (action === "cancel-ralph-loop") {
      if (effectiveAgent === "codex") {
        appendCommandOutput("Ralph loop is not supported for Codex sessions yet.");
        return;
      }
      await handleRalphCancel();
    } else if (action === "toggle-preview") {
      if (args) {
        await openPreviewAndNotify(args);
      } else if (previewOpen) {
        await closePreview();
        appendCommandOutput(t("preview_closed"));
      } else {
        const lastUrl = localStorage.getItem("agentcabin:preview-url");
        if (lastUrl) {
          await openPreviewAndNotify(lastUrl);
        } else {
          appendCommandOutput(t("preview_usage"));
        }
      }
    } else if (action === "init-project") {
      // Codex-only: mirror Codex TUI's /init by injecting the upstream init
      // prompt as a user message. The model runs git ls-files and writes
      // AGENTS.md via its file tools. Claude CLI handles /init itself.
      //
      // Order: agent → cwd resolve → no-cwd → remote → isRunning → pathExists
      // → dbg(requested) → localStorage handoff → sendMessage. Each guard
      // emits a dbgWarn so blocked requests are traceable; the "requested"
      // dbg only fires once all guards pass.
      if (effectiveAgent !== "codex") return;
      const projectCwd =
        store.effectiveCwd ||
        store.sessionCwd ||
        store.run?.cwd ||
        folderCwdOverride ||
        projectInitStatus?.cwd ||
        (typeof localStorage !== "undefined" ? (getSavedProjectCwd(currentRealm) ?? "") : "");
      if (!projectCwd) {
        dbgWarn("chat", "init-project blocked: no cwd");
        appendCommandOutput(t("init_noCwd"));
        return;
      }
      // Remote target: sendMessage resolves cwd via getStoredRemoteCwd(host)
      // (chat/+page.svelte:1968), not localStorage. The handoff below cannot
      // bridge those paths. Guard remote until wave 4b unifies cwd resolution.
      if (store.remoteHostName && !effectiveCapabilities.runtime.remote) {
        dbgWarn("chat", "init-project blocked: remote runtime capability unavailable", {
          host: store.remoteHostName,
        });
        appendCommandOutput(t("init_remoteNotSupported"));
        return;
      }
      if (store.isRunning) {
        dbgWarn("chat", "init-project blocked: turn running");
        appendCommandOutput(t("init_busy"));
        return;
      }
      try {
        const exists = await api.agentsMdExists(projectCwd);
        if (exists) {
          dbgWarn("chat", "init-project blocked: AGENTS.md exists");
          appendCommandOutput(t("init_alreadyExists"));
          return;
        }
      } catch (err) {
        dbgWarn("chat", "init: agentsMdExists failed", err);
        appendCommandOutput(
          `Failed to check AGENTS.md: ${err instanceof Error ? err.message : String(err)}`,
        );
        return;
      }
      dbg("chat", "init-project requested", { cwd: projectCwd });
      // Persist cwd so sendMessage's new-run path (+page.svelte:1976 reads
      // localStorage only) starts the init run in the same directory the
      // guard validated. Without this, a guard pass via projectInitStatus?.cwd
      // or folderCwdOverride could disagree with sendMessage's resolution.
      if (typeof localStorage !== "undefined") {
        setSavedProjectCwd(projectCwd, currentRealm);
        window.dispatchEvent(new Event("agentcabin:cwd-changed"));
      }
      await sendMessage(CODEX_INIT_PROMPT, []);
    } else if (action === "codex-agent-info") {
      // Codex TUI's /agent opens a sub-agent picker (OpenAgentPicker). We
      // don't have that UI yet; sub-agents nest inline as Agent tool calls.
      // This handler just explains.
      if (effectiveAgent !== "codex") return;
      dbg("chat", "codex-agent-info");
      appendCommandOutput(t("codexAgent_notSupported"));
    } else if (action === "codex-review") {
      // Codex /review v1: inject the "review uncommitted changes" prompt and
      // let the model gather git diff + analyze. Same guard order as
      // init-project. Picker for the other 3 review presets is wave 4b.
      if (effectiveAgent !== "codex") return;
      const projectCwd =
        store.effectiveCwd ||
        store.sessionCwd ||
        store.run?.cwd ||
        folderCwdOverride ||
        projectInitStatus?.cwd ||
        (typeof localStorage !== "undefined" ? (getSavedProjectCwd(currentRealm) ?? "") : "");
      if (!projectCwd) {
        dbgWarn("chat", "codex-review blocked: no cwd");
        appendCommandOutput(t("codexReview_noCwd"));
        return;
      }
      if (store.remoteHostName && !effectiveCapabilities.runtime.remote) {
        dbgWarn("chat", "codex-review blocked: remote runtime capability unavailable", {
          host: store.remoteHostName,
        });
        appendCommandOutput(t("codexReview_remoteNotSupported"));
        return;
      }
      if (store.isRunning) {
        dbgWarn("chat", "codex-review blocked: turn running");
        appendCommandOutput(t("codexReview_busy"));
        return;
      }
      dbg("chat", "codex-review requested", { cwd: projectCwd });
      if (typeof localStorage !== "undefined") {
        setSavedProjectCwd(projectCwd, currentRealm);
        window.dispatchEvent(new Event("agentcabin:cwd-changed"));
      }
      // Open the preset picker (uncommitted / base / commit / custom).
      codexReviewPickerOpen = true;
    } else if (action === "codex-login") {
      if (effectiveAgent !== "codex") return; // defensive (generic gate already blocks)
      if (store.isRunning) {
        appendCommandOutput(t("codexLogin_busy"));
        return;
      }
      appendCommandOutput(t("codexLogin_opening"));
      try {
        await api.runCodexLogin();
        appendCommandOutput(t("codexLogin_success"));
        // Settings page may be open — let it refresh its auth status.
        window.dispatchEvent(new Event("agentcabin:codex-auth-changed"));
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        dbgWarn("chat", "codex login failed", err);
        appendCommandOutput(t("codexLogin_failed", { error: msg }));
      }
    } else if (action === "codex-logout") {
      if (effectiveAgent !== "codex") return;
      if (store.isRunning) {
        appendCommandOutput(t("codexLogin_busy"));
        return;
      }
      try {
        await api.runCodexLogout();
        appendCommandOutput(t("codexLogout_success"));
        window.dispatchEvent(new Event("agentcabin:codex-auth-changed"));
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        dbgWarn("chat", "codex logout failed", err);
        appendCommandOutput(t("codexLogout_failed", { error: msg }));
      }
    } else if (action === "codex-compact") {
      if (effectiveAgent !== "codex") return;
      if (!store.run || !store.sessionAlive) {
        appendCommandOutput(t("codexCompact_noSession"));
        return;
      }
      if (store.isRunning) {
        appendCommandOutput(t("codexCompact_busy"));
        return;
      }
      try {
        await api.compactSession(store.run.id);
        appendCommandOutput(t("codexCompact_done"));
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        dbgWarn("chat", "codex compact failed", err);
        appendCommandOutput(t("codexCompact_failed", { error: msg }));
      }
    } else if (action === "pi-compact") {
      if (effectiveAgent !== "pi") return;
      if (!store.run || !store.sessionAlive) {
        appendCommandOutput(t("chat_clearNotSupported"));
        return;
      }
      if (store.isRunning) {
        appendCommandOutput(t("chat_clearSessionBusy"));
        return;
      }
      try {
        await api.compactSession(store.run.id);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        dbgWarn("chat", "pi compact failed", err);
        appendCommandOutput(msg);
      }
    } else if (action === "codex-rewind") {
      if (effectiveAgent !== "codex") return;
      if (!store.run || !store.sessionAlive) {
        appendCommandOutput(t("codexRewind_noSession"));
        return;
      }
      if (store.isRunning) {
        appendCommandOutput(t("codexRewind_busy"));
        return;
      }
      if (store.userTurnCount === 0) {
        appendCommandOutput(t("codexRewind_noTurns"));
        return;
      }
      openCodexRewind();
    } else if (action === "codex-goal" || action === "goal-open") {
      if (goalPanelAvailable) goalPanelOpen = true;
    }
  }

  async function handleResume(
    mode: SessionMode,
    overrideRunId?: string,
    initialMessage?: string,
    initialAttachments?: Attachment[],
  ) {
    const targetRunId = overrideRunId ?? store.run?.id;
    if (!targetRunId || resuming) return;
    if (mode === "fork" && !store.capabilities.runtime.fork) {
      dbgWarn("chat", "fork blocked by agent runtime capability", { agent: effectiveAgent });
      return;
    }
    resuming = true;

    // Per-session platform: resume automatically uses run's saved platform_id
    // via backend resolve_auth_env_for_platform() — no mismatch dialog needed.

    // Fork: activate overlay immediately for progress feedback
    if (mode === "fork") {
      forkOverlay = { active: true, sourceRunId: targetRunId, startedAt: Date.now(), error: null };
    }

    try {
      // Fork: don't subscribe to source — backend emits RunState(stopped)
      // for the source which would interfere with the fork state machine.
      if (mode !== "fork") {
        middleware.subscribeCurrent(targetRunId, store);
      }
      const resultId = await store.resumeSession(
        targetRunId,
        mode,
        initialMessage,
        initialAttachments,
      );
      if (resultId) {
        middleware.subscribeCurrent(resultId, store);
        if (mode === "fork") {
          // Check if user cancelled during fork_oneshot
          if (!forkOverlay) {
            dbg("chat", "fork: cancelled during fork_oneshot, skipping step 2");
          } else {
            // Step 1 complete — dismiss overlay, use normal session startup UI for step 2
            forkOverlay = null;
            goto(`/chat?run=${resultId}`, { replaceState: true });
            // Step 2: establish stream-json connection (shows "Starting session..." spinner)
            try {
              await store.connectSession(resultId);
            } catch (e) {
              store.error = String(e);
            }
          }
        } else {
          goto(`/chat?run=${resultId}`, { replaceState: true });
        }
      } else if (mode === "fork") {
        // Fork failed — don't clear overlay or navigate away.
        // The phase watcher $effect will show the error in the overlay.
        // User can Retry or Cancel from there.
        dbg("chat", "fork failed, keeping overlay for retry/cancel");
      } else {
        // Non-fork resume failed — stay on the target run's view instead of
        // navigating to blank new-session page (the run's history is still useful).
        lastContinuableRun = null;
        goto(`/chat?run=${targetRunId}`, { replaceState: true });
      }
      window.dispatchEvent(new Event("agentcabin:runs-changed"));
    } catch (e) {
      // Fork sync failure → show error in overlay instead of error bar
      if (mode === "fork" && forkOverlay) {
        forkOverlay = { ...forkOverlay, error: String(e) };
      }
    } finally {
      resuming = false;
    }
  }

  /** Stop the fork run's process (if it exists and isn't the source run). */
  async function stopForkProcess(sourceRunId: string) {
    if (store.run && store.run.id !== sourceRunId) {
      try {
        await api.stopSession(store.run.id);
      } catch {
        /* best-effort */
      }
    }
  }

  async function handleForkCancel() {
    if (!forkOverlay) return;
    const sourceRunId = forkOverlay.sourceRunId;
    await stopForkProcess(sourceRunId);
    forkOverlay = null;
    store.error = "";
    goto(`/chat?run=${sourceRunId}`, { replaceState: true });
    // Explicit reload — URL may not change if we're returning to the same run
    await loadRunProgressive(sourceRunId);
    window.dispatchEvent(new Event("agentcabin:runs-changed"));
  }

  async function handleForkRetry() {
    if (!forkOverlay || resuming) return;
    const sourceRunId = forkOverlay.sourceRunId;
    await stopForkProcess(sourceRunId);
    forkOverlay = { active: true, sourceRunId, startedAt: Date.now(), error: null };
    store.error = "";
    await handleResume("fork", sourceRunId);
  }

  // ── Chat-level toast (same pattern as PromptInput's showFileToast) ──
  let chatToast = $state<string | null>(null);
  let chatToastTimeout: ReturnType<typeof setTimeout> | null = null;
  function showChatToast(msg: string) {
    chatToast = msg;
    if (chatToastTimeout) clearTimeout(chatToastTimeout);
    chatToastTimeout = setTimeout(() => {
      chatToast = null;
    }, 2500);
  }

  async function toggleCliConfigBool(key: string) {
    try {
      const config = await api.getCliConfig();
      const current = config[key] === true;
      await api.updateCliConfig({ [key]: !current });
      dbg("chat", `toggled ${key}`, { from: current, to: !current });
      // Immediately mirror UI state
      if (key === "fastMode") {
        store.fastModeState = !current ? "on" : "";
        dbg("chat", "fastMode UI mirrored", { state: store.fastModeState });
      } else if (key === "verbose") {
        verboseEnabled = !current;
        dbg("chat", "verbose UI mirrored", { verbose: verboseEnabled });
      }
      const label =
        key === "fastMode"
          ? !current
            ? "toast_fastModeOn"
            : "toast_fastModeOff"
          : !current
            ? "toast_verboseOn"
            : "toast_verboseOff";
      showChatToast(t(label as Parameters<typeof t>[0]));
    } catch (e) {
      dbgWarn("chat", `toggle ${key} failed:`, e);
    }
  }

  // Chat keybinding callbacks — registered/unregistered via keybindingStore in onMount below

  // ── Page-level drag-drop (Tauri native events) ──
  let pageDragActive = $state(false);
  let dragProcessingCount = $state(0);
  let dragProcessing = $derived(dragProcessingCount > 0);

  // Safety net: the native `tauri://drag-leave` / `drag-drop` events can be dropped on some
  // platforms/webviews, leaving pageDragActive stuck true → the z-50 hover overlay swallows
  // every click and the whole UI looks frozen. A pointerdown cannot reach the webview while an
  // OS file-drag is in progress, so receiving one means the drag is over — force-clear the
  // stale hover. Escape does the same. Does NOT touch dragProcessing (real in-flight work).
  $effect(() => {
    function clearStaleHover() {
      if (pageDragActive) {
        pageDragActive = false;
        dbg("chat", "drag-hover safety-net cleared stale pageDragActive");
      }
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") clearStaleHover();
      if (
        (e.metaKey || e.ctrlKey) &&
        e.key.toLowerCase() === "f" &&
        !e.altKey &&
        store.useChatTimeline &&
        !document.activeElement?.closest("[role='dialog']")
      ) {
        e.preventDefault();
        focusChatSearch();
      }
    }
    window.addEventListener("pointerdown", clearStaleHover);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", clearStaleHover);
      window.removeEventListener("keydown", onKey);
    };
  });

  /** Concurrency-limited parallel map returning PromiseSettledResult for each item. */
  async function handleTauriDrop(payload: { paths: string[] }) {
    pageDragActive = false;
    const paths = payload.paths;
    const input = promptRef; // cache ref — promptRef may become undefined after awaits
    if (!paths?.length || !input) return;

    dragProcessingCount++;
    dbg("chat", "tauri-drop", { count: paths.length });

    try {
      // Phase 1: parallel classify (concurrency=5 to avoid IPC flood on large batches)
      const classified = await mapSettled(
        paths,
        async (p) => {
          const name = p.split(/[/\\]/).pop() || "file";
          const isDir = await api.checkIsDirectory(p);
          return { p, name, isDir };
        },
        5,
      );

      const dirRefs: Array<{ path: string; name: string; isDir: true }> = [];
      const fileEntries: Array<{ p: string; name: string }> = [];

      for (let i = 0; i < classified.length; i++) {
        const result = classified[i];
        const p = paths[i];
        const name = p.split(/[/\\]/).pop() || "file";
        if (result.status === "fulfilled") {
          if (result.value.isDir) {
            dirRefs.push({ path: p, name, isDir: true });
            dbg("chat", "tauri-drop: dir", { name });
          } else {
            fileEntries.push({ p, name });
          }
        } else {
          // checkIsDirectory IPC failed — conservatively treat as file
          fileEntries.push({ p, name });
          dbgWarn("chat", "tauri-drop: classify failed, treating as file", {
            name,
            error: result.reason,
          });
        }
      }

      // Phase 2: parallel file read (concurrency=2 to limit memory).
      // Pass project cwd to read_file_base64 so the backend can validate the
      // path is inside an allowed root. Files outside the project will reject
      // — the existing "rejected → path ref" fallback below handles that
      // gracefully so the user still gets the attachment as a path reference.
      const dropCwd = store.sessionCwd || store.run?.cwd || "";
      const fileResults = await mapSettled(
        fileEntries,
        async ({ p, name }) => {
          const [base64, mime] = await api.readFileBase64(p, dropCwd);
          const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
          return { file: new File([bytes], name, { type: mime }), name, mime, size: bytes.length };
        },
        2,
      );

      const filesToProcess: File[] = [];
      const fileRefs: Array<{ path: string; name: string; isDir: false }> = [];

      for (let i = 0; i < fileResults.length; i++) {
        const result = fileResults[i];
        const { p, name } = fileEntries[i];
        if (result.status === "fulfilled") {
          filesToProcess.push(result.value.file);
          dbg("chat", "tauri-drop: file", {
            name: result.value.name,
            mime: result.value.mime,
            size: result.value.size,
          });
        } else {
          fileRefs.push({ path: p, name, isDir: false });
          dbgWarn("chat", "tauri-drop: fallback to path ref", { name, error: result.reason });
        }
      }

      // Guard: if page navigated away during processing, promptRef is stale
      if (promptRef !== input) {
        dbgWarn("chat", "tauri-drop: promptRef stale after processing, discarding");
        return;
      }

      // Add path refs (dirs + failed files)
      const allPathRefs = [...dirRefs, ...fileRefs];
      if (allPathRefs.length > 0) {
        input.addPathRefs(allPathRefs);
      }

      // Normal files → existing addFiles pipeline (await so spinner covers processFiles)
      if (filesToProcess.length > 0) {
        await input.addFiles(filesToProcess);
      }

      // Single summary toast
      if (allPathRefs.length > 0) {
        const parts: string[] = [];
        if (dirRefs.length > 0) {
          parts.push(t("drag_foldersInserted", { count: String(dirRefs.length) }));
        }
        if (fileRefs.length > 0) {
          parts.push(t("drag_filesAsPathRef", { count: String(fileRefs.length) }));
        }
        input.showToast(parts.join(t("common_listSeparator")));
      }
    } finally {
      dragProcessingCount--;
    }
  }

  function toggleSidebar() {
    sidebarCollapsed = !sidebarCollapsed;
  }

  async function scrollToTool(toolUseId: string) {
    // Locate target in the data layer (DOM may not be mounted yet under progressive render).
    const ft = filteredTimeline;
    const ftIdx = ft.findIndex((e) => e.kind === "tool" && e.tool.tool_use_id === toolUseId);
    if (ftIdx < 0) return;
    expandRenderLimitTo(ftIdx);
    await tick();
    // Re-map to visibleTimeline-local index for burst expansion.
    const visibleIdx = visibleTimeline.findIndex(
      (e) => e.kind === "tool" && e.tool.tool_use_id === toolUseId,
    );
    if (visibleIdx >= 0) await ensureBurstExpandedFor(visibleIdx);
    const el = document.getElementById("tool-" + toolUseId);
    if (el) {
      // Temporarily disable content-visibility so the browser knows real heights and
      // scrollIntoView lands at the correct offset (mirrors scrollToMessage).
      const container = chatAreaRef;
      const cvEls = container
        ? Array.from(container.querySelectorAll<HTMLElement>(".cv-auto"))
        : [];
      for (const c of cvEls) c.style.contentVisibility = "visible";
      el.getBoundingClientRect();
      el.scrollIntoView({ behavior: "smooth", block: "center" });
      requestAnimationFrame(() => {
        for (const c of cvEls) c.style.contentVisibility = "";
      });
    }
  }

  async function scrollToMessage(ts: string) {
    dbg("chat", "scrollToMessage", { ts });
    // Resolve target from data — `ts` may be ts, anchorId, cliUuid, or id.
    const match = store.timeline.find(
      (e) =>
        e.ts === ts || e.anchorId === ts || (e.kind === "user" && e.cliUuid === ts) || e.id === ts,
    );
    if (!match) return;
    const ft = filteredTimeline;
    const ftIdx = ft.findIndex((e) => e.id === match.id);
    if (ftIdx < 0) return;
    expandRenderLimitTo(ftIdx);
    await tick();
    const visibleIdx = visibleTimeline.findIndex((e) => e.id === match.id);
    if (visibleIdx >= 0) await ensureBurstExpandedFor(visibleIdx);
    // DOM id uses anchorId (see `id="msg-{entry.anchorId}"` in the each block).
    const el = document.getElementById("msg-" + match.anchorId);
    if (el) {
      // Temporarily disable content-visibility on ALL entries so the browser
      // knows real heights and scrollIntoView lands at the correct offset.
      const container = chatAreaRef;
      const cvEls = container
        ? Array.from(container.querySelectorAll<HTMLElement>(".cv-auto"))
        : [];
      for (const c of cvEls) c.style.contentVisibility = "visible";

      el.getBoundingClientRect(); // force reflow
      el.scrollIntoView({ behavior: "instant", block: "center" });

      // Restore content-visibility after scroll settles
      requestAnimationFrame(() => {
        for (const c of cvEls) c.style.contentVisibility = "";
      });
    } else {
      dbg("chat", "scrollToMessage: element not found", { anchor: ts });
    }
  }

  async function handleToolAnswer(toolUseId: string, answer: string) {
    await store.answerToolQuestion(toolUseId, answer);
  }

  function handleRewind() {
    if (!store.run || !store.sessionAlive || store.isRunning) return;
    rewindModalOpen = true;
  }

  function handleRewindToMessage(entry: { cliUuid: string; content: string; ts: string }) {
    if (!store.run || !store.sessionAlive || store.isRunning) return;
    rewindDirectTarget = {
      cliUuid: entry.cliUuid,
      content: entry.content,
      ts: entry.ts,
      timelineIndex: store.timeline.findIndex(
        (e) => e.kind === "user" && e.cliUuid === entry.cliUuid,
      ),
    };
    rewindModalOpen = true;
  }

  async function handleEditAndResend(
    turnIndex: number,
    newContent: string,
    attachments?: Attachment[],
  ) {
    if (!store.run || !store.sessionAlive || store.isRunning) return;
    if (!newContent.trim()) return;

    const totalUserTurns = store.timeline.filter((e) => e.kind === "user").length;
    const numTurns = Math.max(1, totalUserTurns - turnIndex);

    try {
      if (effectiveAgent === "codex") {
        await api.rollbackTurns(store.run.id, numTurns);
      }
      store.truncateToTurn(turnIndex);
      await sendMessage(newContent, attachments || []);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      dbgWarn("chat", "edit and resend failed", err);
      appendCommandOutput(`Edit and resend failed: ${msg}`);
    }
  }

  async function handleToolApprove(toolName: string) {
    if (!store.run) return;
    approving = true;
    dbg("chat", "approving tool", { runId: store.run.id, toolName });
    try {
      await api.approveSessionTool(store.run.id, toolName);
    } catch (e) {
      dbgWarn("chat", "approve failed:", e);
      store.error = String(e);
    } finally {
      // approving resets when new RunState events arrive (spawning/running)
      setTimeout(() => {
        approving = false;
      }, 3000);
    }
  }

  async function handlePermissionRespond(
    requestId: string,
    behavior: "allow" | "deny",
    updatedPermissions?: import("$lib/types").PermissionSuggestion[],
    updatedInput?: Record<string, unknown>,
    denyMessage?: string,
    interrupt?: boolean,
  ) {
    if (!store.run || !store.sessionAlive) return;
    const runId = store.run.id; // snapshot — store.run may change after await
    dbg("chat", "inline permission respond", {
      runId,
      requestId,
      behavior,
      updatedPermissions,
      updatedInput,
      denyMessage,
      interrupt,
    });
    try {
      // Set pending mode override BEFORE responding (so reducer picks it up)
      if (behavior === "allow" && updatedPermissions) {
        const modePerm = updatedPermissions.find((p) => p.type === "setMode");
        if (modePerm && modePerm.mode) {
          store.pendingPermissionModeOverride = modePerm.mode;
          dbg("chat", "set pendingPermissionModeOverride", { mode: modePerm.mode });
        }
      }

      await api.respondPermission(
        runId,
        requestId,
        behavior,
        updatedPermissions,
        updatedInput,
        denyMessage,
        interrupt,
      );
      // Optimistic resolve + clear attention flag
      resolvePermissionOptimistic(store, runId, requestId, behavior);
    } catch (e) {
      dbgWarn("chat", "permission respond failed:", e);
      // If the CLI rejected the response (e.g. session already idle after interrupt),
      // still resolve the card locally so buttons are removed.
      if (behavior === "deny") {
        resolvePermissionOptimistic(store, runId, requestId, "deny");
      }
      // allow failure: don't change status — submitting timeout auto-resets (§5)
      store.error = String(e);
      throw e; // Let component-side wrapper catch and unlock buttons
    }
  }

  async function handleElicitationRespond(
    requestId: string,
    action: "accept" | "decline" | "cancel",
    content?: Record<string, unknown>,
  ) {
    if (!store.run || !store.sessionAlive) return;
    const runId = store.run.id;
    dbg("chat", "elicitation respond", { runId, requestId, action });
    try {
      await api.respondElicitation(runId, requestId, action, content);
      // Cleanup after successful response — not optimistic, avoids card loss on failure
      const { resolveElicitationOptimistic } = await import("$lib/utils/resolve-elicitation");
      resolveElicitationOptimistic(store, runId, requestId);
    } catch (e) {
      dbgWarn("chat", "elicitation respond failed:", e);
      store.error = String(e);
    }
  }

  // O(1) lookup: timeline entry id → index
  let timelineIdIndex = $derived.by(() => {
    const map = new Map<string, number>();
    for (let i = 0; i < store.timeline.length; i++) {
      map.set(store.timeline[i].id, i);
    }
    return map;
  });

  // ID of the last context-cleared separator (for dimming messages above it)
  let lastClearSepId = $derived.by(() => {
    for (let i = store.timeline.length - 1; i >= 0; i--) {
      const e = store.timeline[i];
      if (e.kind === "separator" && e.content === CONTEXT_CLEARED_MARKER) return e.id;
    }
    return null;
  });

  function getPlanContentForExitPlan(
    entryId: string,
  ): { content: string; fileName: string } | null {
    const idx = timelineIdIndex.get(entryId);
    if (idx == null) {
      dbgWarn("chat", "ExitPlanMode entry not found in timeline index", { id: entryId });
      return null;
    }
    const result = extractPlanContent(store.timeline, idx);
    if (result) return result;
    // Fallback: use tool_use_result.plan (--permission-mode=plan auto-approves
    // ExitPlanMode without Write, plan content is in the result directly)
    const entry = store.timeline[idx];
    if (entry?.kind === "tool" && entry.tool.status === "success") {
      const toolResult = entry.tool.tool_use_result as
        | { plan?: string; filePath?: string }
        | undefined;
      if (toolResult?.plan && typeof toolResult.plan === "string") {
        const fp = String(toolResult.filePath ?? "");
        const name = isPlanFilePath(fp) ? (planFileName(fp) ?? "plan") : "plan";
        return { content: toolResult.plan, fileName: name };
      }
    }
    return null;
  }

  /** Get the latest plan content for an approved ExitPlanMode card.
   *  Applies subsequent Edits to the approved plan content. */
  async function handleExitPlanClearContext() {
    if (!store.run) return;
    const runId = store.run.id;
    const cwd = getSavedProjectCwd(currentRealm) || "";
    dbg("chat", "ExitPlanMode: clear context + auto-accept");

    // Find the ExitPlanMode tool's permission request ID from timeline
    const exitPlanEntry = store.timeline.find(
      (e) =>
        e.kind === "tool" &&
        e.tool.tool_name === "ExitPlanMode" &&
        e.tool.status === "permission_prompt" &&
        e.tool.permission_request_id,
    );
    if (!exitPlanEntry || exitPlanEntry.kind !== "tool") return;
    const requestId = exitPlanEntry.tool.permission_request_id!;

    try {
      // 1. Set flags BEFORE responding
      store.pendingPermissionModeOverride = "acceptEdits";
      store.pendingClearContextPlan = "__pending__"; // marker: waiting for tool_end

      // 2. Allow ExitPlanMode (with setMode) — satisfies the control_response requirement
      await api.respondPermission(
        runId,
        requestId,
        "allow",
        [{ type: "setMode", mode: "acceptEdits", destination: "session" }],
        exitPlanEntry.tool.input,
      );
      resolvePermissionOptimistic(store, runId, requestId, "allow");

      // 3. Wait for tool_end to deliver plan content (via pendingClearContextPlan)
      //    Poll briefly — tool_end should arrive within a few hundred ms
      let planContent: string | null = null;
      for (let i = 0; i < 20; i++) {
        await new Promise((r) => setTimeout(r, 200));
        if (store.pendingClearContextPlan && store.pendingClearContextPlan !== "__pending__") {
          planContent = store.pendingClearContextPlan;
          break;
        }
      }
      store.pendingClearContextPlan = null;

      if (!planContent) {
        dbgWarn("chat", "ExitPlanMode: timed out waiting for plan content");
        // Fallback: continue in current session (ExitPlanMode already allowed)
        return;
      }

      // 4. Interrupt + stop current session
      await api.interruptSession(runId).catch(() => {});
      await api.stopSession(runId);
      dbg("chat", "ExitPlanMode: session stopped");

      // 5. Navigate to fresh chat URL, then start a new session inline.
      //    Using sessionStorage + onMount doesn't work: /chat?run=X → /chat
      //    is the same route component and onMount won't re-fire.
      //    permissionModeOverride threads through api.startSession → backend
      //    adapter_settings, so the CLI spawns with --permission-mode acceptEdits
      //    and the first "Implement..." turn runs in auto-accept (not plan).
      const planPrompt = `Implement the following plan:\n\n${planContent}`;
      await goto("/chat", { replaceState: true });
      await tick(); // let runId effect run loadRun("") → store.reset()
      const newRunId = await store.startSession(
        planPrompt,
        cwd,
        [],
        "acceptEdits",
        undefined,
        currentEffort || undefined,
      );
      await goto(`/chat?run=${newRunId}`, { replaceState: true });
      dbg("chat", "ExitPlanMode: new session started", { newRunId });
    } catch (e) {
      dbgWarn("chat", "ExitPlanMode clear context failed:", e);
      store.pendingClearContextPlan = null;
      store.error = String(e);
      throw e; // Let component-side wrapper catch and unlock buttons
    }
  }

  async function handleHookCallbackRespond(requestId: string, decision: "allow" | "deny") {
    if (!store.run) return;
    dbg("chat", "hook callback respond", { runId: store.run.id, requestId, decision });
    try {
      await api.respondHookCallback(store.run.id, requestId, decision);
      // Update hook event status in store
      store.hookEvents = store.hookEvents.map((h) =>
        h.request_id === requestId
          ? { ...h, status: decision === "allow" ? ("allowed" as const) : ("denied" as const) }
          : h,
      );
    } catch (e) {
      dbgWarn("chat", "hook callback respond failed:", e);
      store.error = String(e);
    }
  }
</script>

<div class="flex h-full overflow-hidden bg-background relative chat-layout-surface chat-three-pane">
  <!-- Page-level drag overlay (drag-hover or processing spinner) -->
  {#if pageDragActive || dragProcessing}
    <div
      class="absolute inset-0 z-50 flex items-center justify-center bg-background/60 backdrop-blur-[2px]"
    >
      <div
        class="flex flex-col items-center gap-2 rounded-xl border-2 border-dashed border-primary/50 bg-primary/5 px-12 py-8"
      >
        {#if dragProcessing}
          <svg
            class="h-8 w-8 text-primary/60 animate-spin"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M21 12a9 9 0 1 1-6.219-8.56" />
          </svg>
          <span class="text-sm font-medium text-primary/70">{t("drag_processing")}</span>
        {:else}
          <svg
            class="h-8 w-8 text-primary/60"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
            <polyline points="17 8 12 3 7 8" />
            <line x1="12" x2="12" y1="3" y2="15" />
          </svg>
          <span class="text-sm font-medium text-primary/70">{t("prompt_dropFiles")}</span>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Main content area -->
  <div class="flex flex-1 flex-col min-w-0 relative chat-main-content chat-canvas">
    {#if isTaskView}
      <div class="flex min-h-0 flex-1 flex-col overflow-y-auto bg-background">
        <CodeTasksCenter />
      </div>
    {:else if isArchivedView}
      <div class="flex-1 overflow-y-auto bg-background">
        <ArchivedChatsView realm={currentHarness === "work" ? "work" : "code"} />
      </div>
    {:else}
      <!-- Status bar -->
      <SessionStatusBar
        bind:this={statusBarRef}
        running={store.sessionAlive}
        run={store.run}
        agent={effectiveAgent}
        model={effectiveAgent === "codex"
          ? codexDisplayModel(store.run?.model) ||
            codexDisplayModel(store.model) ||
            managedProviderDefaultModel ||
            (activeProviderBinding?.mode === "custom" ? "" : getCodexDefaultModel()) ||
            ""
          : store.model}
        parentRunId={store.run?.parent_run_id}
        onModelChange={handleModelChange}
        modelOptions={effectiveModels}
        effort={effortAvailable ? currentEffort : undefined}
        onEffortChange={effortAvailable ? handleEffortChange : undefined}
        onNavigateParent={store.run?.parent_run_id
          ? () => goto(getRunRoute(store.run!, { runId: store.run!.parent_run_id }))
          : undefined}
        cwd={getProjectCwdForEditor()}
        {vscodeAvailable}
        onOpenVscode={getTransport().isDesktop() &&
        getProjectCwdForEditor().length > 0 &&
        !store.remoteHostName
          ? openProjectInVscode
          : undefined}
        onOpenWorktrees={worktreeProject && !store.remoteHostName ? openWorktreePanel : undefined}
        onToggleSidebar={toggleLayoutSidebar}
        sidebarOpen={isLayoutSidebarOpen ? isLayoutSidebarOpen() : true}
        mcpServers={store.mcpServers}
        onMcpToggle={() => (mcpPanelOpen = !mcpPanelOpen)}
        {platformModels}
        persistedFiles={store.persistedFiles}
        onRewind={effectiveCapabilities.execution.snapshots &&
        store.sessionAlive &&
        !store.isRunning
          ? handleRewind
          : undefined}
        activeTaskCount={store.activeBackgroundTasks.length}
        mode={store.run ? (store.useStreamSession ? "Stream" : "CLI") : ""}
        toolsCount={runEffectiveCapabilities
          ? (runEffectiveCapabilities.enabledSkills?.length ?? 0) +
            (runEffectiveCapabilities.mcpServers?.length ?? 0) +
            (runEffectiveCapabilities.connectors?.length ?? 0)
          : 0}
        onToolsClick={() => {
          capabilityInspectorOpen = true;
        }}
        onRename={store.run ? handleRename : undefined}
        {bottomPanelOpen}
        rightSidebarOpen={codeAsideOpen}
        onToggleRightSidebar={() => {
          setCodeAsideOpen(!codeAsideOpen);
          if (turnReviewOpen) turnReviewOpen = false;
        }}
        onToggleBottomPanel={toggleBottomPanel}
        onSearch={toggleChatSearch}
        searchOpen={chatSearchOpen}
        onPreviewToggle={localPreviewReady
          ? () => {
              if (previewOpen && !previewUrlBarOpen) {
                closePreview();
              } else {
                previewUrlBarOpen = !previewUrlBarOpen;
                if (previewUrlBarOpen) {
                  requestAnimationFrame(() => {
                    const el = document.querySelector<HTMLInputElement>("#__preview-url-input");
                    el?.focus();
                    el?.select();
                  });
                }
              }
            }
          : undefined}
        onStatusClick={() => {
          if (!hasSidebarData) return;
          if (sidebarCollapsed) sidebarCollapsed = false;
          sidebarRequestedTab = "info";
        }}
      />

      <!-- MCP panel (floating below status bar) -->
      {#if mcpPanelOpen && store.mcpServers.length > 0}
        <div class="absolute top-11 right-3 z-30">
          <McpStatusPanel
            runId={store.run?.id ?? ""}
            mcpServers={store.mcpServers}
            sessionAlive={store.sessionAlive}
            agent={effectiveAgent}
            onClose={() => (mcpPanelOpen = false)}
            onServersUpdate={(servers) => {
              store.updateMcpServers(servers);
            }}
          />
        </div>
      {/if}

      <!-- Preview URL input bar -->
      {#if previewUrlBarOpen}
        <div class="flex items-center gap-2 px-3 py-1.5 border-b border-border bg-muted/30 text-xs">
          <svg
            class="w-3.5 h-3.5 shrink-0 text-muted-foreground"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <circle cx="12" cy="12" r="10" /><path
              d="M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"
            />
          </svg>
          <input
            id="__preview-url-input"
            type="text"
            bind:value={previewUrlInput}
            placeholder="http://localhost:3000"
            class="flex-1 bg-transparent border-none outline-none text-xs text-foreground placeholder:text-muted-foreground/50 font-mono"
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                const url = previewUrlInput.trim();
                if (url) {
                  openPreviewAndNotify(url).then((ok) => {
                    if (ok) previewUrlBarOpen = false;
                  });
                }
              } else if (e.key === "Escape") {
                previewUrlBarOpen = false;
              }
            }}
          />
          {#if previewOpen}
            <button
              onclick={() => {
                closePreview();
                previewUrlBarOpen = false;
              }}
              class="px-2 py-0.5 rounded text-xs bg-muted hover:bg-accent text-foreground transition-colors"
            >
              {t("preview_close")}
            </button>
          {/if}
          <button
            onclick={() => {
              previewUrlBarOpen = false;
            }}
            class="text-muted-foreground hover:text-foreground transition-colors"
          >
            <svg
              class="w-3.5 h-3.5"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"><path d="M18 6 6 18M6 6l12 12" /></svg
            >
          </button>
        </div>
      {/if}

      <!-- Main area -->
      <div class="flex-1 overflow-hidden relative chat-content">
        {#if store.useChatTimeline}
          <!-- API / Codex bus-events mode: chat messages -->
          {#if conversationRailEntries.length > 1}
            <ConversationTurnRail
              container={chatAreaRef ?? null}
              entries={conversationRailEntries}
              onSelect={(entry: import("$lib/components/ConversationTurnRail.svelte").RailEntry) =>
                void scrollToMessage(entry.anchorId)}
            />
          {/if}
          <div
            class="h-full overflow-y-auto"
            style="overflow-anchor:auto"
            bind:this={chatAreaRef}
            onscroll={handleChatScroll}
          >
            {#if welcomeVisible}
              <div
                class="flex min-h-full flex-col items-center px-4 py-4 sm:px-6 sm:py-6 chat-welcome"
              >
                <div
                  class="flex w-full max-w-[760px] flex-col items-center chat-welcome-inner my-auto"
                >
                  <!-- Logo -->
                  <img
                    src="/logo.png?v=2"
                    alt="AC"
                    class="mb-3.5 h-10 w-10 opacity-95 animate-fade-in drop-shadow-[0_0_12px_rgba(var(--primary-rgb,99,102,241),0.35)]"
                  />
                  {#if codexWarning}
                    <p class="text-amber-500 text-sm mb-3 px-2 text-center">{codexWarning}</p>
                  {/if}

                  <!-- Title -->
                  {#if welcomeProjectName}
                    <h2
                      class="text-center text-xl sm:text-2xl font-semibold text-foreground mb-1 animate-fade-in tracking-tight"
                    >
                      你想让我们在 <span class="font-semibold text-foreground"
                        >{welcomeProjectName}</span
                      > 中构建什么？
                    </h2>
                    <p class="text-center text-xs text-muted-foreground/60 mb-3.5 animate-fade-in">
                      {t("chat_welcomeSubtitle")}
                    </p>
                  {:else}
                    <h2
                      class="text-center text-xl sm:text-2xl font-semibold text-foreground mb-1 animate-fade-in tracking-tight"
                    >
                      {t("chat_welcomeNoProject")}
                    </h2>
                    <p class="text-center text-xs text-muted-foreground/60 mb-3.5 animate-fade-in">
                      {t("chat_welcomeNoProjectHint")}
                    </p>
                  {/if}

                  <!-- Quick task entries (4-column cards matching Codex UI with non-auto-sending template selector) -->
                  {#if welcomeProjectName}
                    {@const quickTasks = [
                      {
                        key: "explore",
                        label: t("chat_quickTaskExplore"),
                        desc: "架构与性能分析",
                        icon: "compass",
                        subItems: [
                          {
                            label: "解释某个模块或架构",
                            prompt: "请帮我解释当前项目的整体架构与核心模块划分。",
                          },
                          {
                            label: "分析性能瓶颈",
                            prompt: "请分析当前项目可能存在的性能瓶颈，并提出优化建议。",
                          },
                          { label: "查找重构机会", prompt: "请检查项目代码，找出可优化的重构点。" },
                          {
                            label: "梳理数据流与依赖",
                            prompt: "请梳理当前应用的核心数据流与模块依赖关系。",
                          },
                        ],
                      },
                      {
                        key: "build",
                        label: t("chat_quickTaskBuild"),
                        desc: "新功能与 UI 原型",
                        icon: "hammer",
                        subItems: [
                          { label: "开发一项功能", prompt: "开发一项功能：" },
                          { label: "实现 UI 更改", prompt: "实现 UI 更改：" },
                          { label: "构建原型", prompt: "构建原型：" },
                          {
                            label: "构建内部工具",
                            subKey: "internal-tools",
                            templates: [
                              {
                                label: "构建一个内部工具",
                                type: "default",
                                prompt: "构建一个内部工具",
                              },
                              {
                                label: "基于 Linear 工单构建内部工具",
                                type: "linear",
                                prompt: "基于 Linear 工单需求，为项目构建一个内部自动化工具",
                              },
                              {
                                label: "基于 Slack 中的请求构建内部工具",
                                type: "slack",
                                prompt: "基于 Slack 中的用户请求，构建一个内部工具",
                              },
                              {
                                label: "基于 Google Drive 文档构建内部工具",
                                type: "drive",
                                prompt:
                                  "请根据 Google Drive 文档内容，为项目构建一个内部自动化工具",
                              },
                            ],
                          },
                        ],
                      },
                      {
                        key: "review",
                        label: t("chat_quickTaskReview"),
                        desc: "Git 审查与规范",
                        icon: "search",
                        subItems: [
                          {
                            label: "审查 Uncommitted Git 修改",
                            prompt: "请审查当前 Git 工作区修改的代码，提出改进与潜在问题诊断。",
                          },
                          {
                            label: "检查代码安全性与潜在漏洞",
                            prompt: "请扫描项目代码中的安全风险与异常捕获遗漏。",
                          },
                          {
                            label: "检查 TypeScript/Svelte 规范",
                            prompt: "请检查项目中的 TypeScript 类型定义与 Svelte 组件规范。",
                          },
                        ],
                      },
                      {
                        key: "fix",
                        label: t("chat_quickTaskFix"),
                        desc: "排查并解决错误",
                        icon: "wrench",
                        subItems: [
                          { label: "定位并修复 Bug", prompt: "请帮我定位并修复项目中出现的 Bug：" },
                          {
                            label: "修复单元测试失败",
                            prompt: "请运行单元测试，排查失败原因并修复测试用例。",
                          },
                          {
                            label: "排查构建与编译报错",
                            prompt: "请分析当前项目的编译/构建报错，并给出修复方案。",
                          },
                        ],
                      },
                    ]}
                    <div class="relative w-full px-4 mb-3 animate-fade-in chat-welcome-cards">
                      <!-- 2x2 Linear/Cursor style horizontal compact cards -->
                      <div
                        class="chat-welcome-main-grid grid w-full grid-cols-1 sm:grid-cols-2 gap-2.5"
                      >
                        {#each quickTasks as task (task.key)}
                          <button
                            type="button"
                            aria-expanded={activeWelcomeCard === task.key}
                            class="flex items-center gap-3 rounded-xl border p-3 text-left transition-colors group {activeWelcomeCard ===
                            task.key
                              ? 'border-primary/40 bg-primary/5 shadow-sm ring-1 ring-primary/20'
                              : 'border-border/40 bg-card/60 hover:border-border/70 hover:bg-card'}"
                            onclick={() => {
                              if (activeWelcomeCard === task.key) {
                                activeWelcomeCard = null;
                                activeWelcomeSub = null;
                              } else {
                                activeWelcomeCard = task.key;
                                activeWelcomeSub = null;
                              }
                            }}
                          >
                            <div
                              class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-muted/60 text-muted-foreground group-hover:text-foreground transition-colors"
                            >
                              {#if task.icon === "compass"}
                                <svg
                                  class="h-4 w-4"
                                  viewBox="0 0 24 24"
                                  fill="none"
                                  stroke="currentColor"
                                  stroke-width="1.8"
                                  stroke-linecap="round"
                                  stroke-linejoin="round"
                                  ><circle cx="12" cy="12" r="10" /><polygon
                                    points="16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76"
                                  /></svg
                                >
                              {:else if task.icon === "hammer"}
                                <svg
                                  class="h-4 w-4"
                                  viewBox="0 0 24 24"
                                  fill="none"
                                  stroke="currentColor"
                                  stroke-width="1.8"
                                  stroke-linecap="round"
                                  stroke-linejoin="round"
                                  ><path d="m15 12-8.5 8.5a2.12 2.12 0 1 1-3-3L12 9" /><path
                                    d="M17.64 15 22 10.64"
                                  /></svg
                                >
                              {:else if task.icon === "search"}
                                <svg
                                  class="h-4 w-4"
                                  viewBox="0 0 24 24"
                                  fill="none"
                                  stroke="currentColor"
                                  stroke-width="1.8"
                                  stroke-linecap="round"
                                  stroke-linejoin="round"
                                  ><path
                                    d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"
                                  /><path d="M3 3v5h5" /><path
                                    d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16"
                                  /><path d="M16 16h5v5" /></svg
                                >
                              {:else if task.icon === "wrench"}
                                <svg
                                  class="h-4 w-4"
                                  viewBox="0 0 24 24"
                                  fill="none"
                                  stroke="currentColor"
                                  stroke-width="1.8"
                                  stroke-linecap="round"
                                  stroke-linejoin="round"
                                  ><path d="m8 2 1.88 1.88" /><path d="M14.12 3.88 16 2" /><path
                                    d="M9 7.13v-1a3.003 3.003 0 0 1 6 0v1"
                                  /><path
                                    d="M12 20c-3.3 0-6-2.7-6-6v-3a4 4 0 0 1 4-4h4a4 4 0 0 1 4 4v3c0 3.3-2.7 6-6 6z"
                                  /><path d="M12 20v2" /></svg
                                >
                              {/if}
                            </div>
                            <div class="min-w-0 flex-1">
                              <div
                                class="text-sm font-semibold text-foreground tracking-tight leading-snug"
                              >
                                {task.label}
                              </div>
                              <div class="text-xs text-muted-foreground/65 truncate mt-0.5">
                                {task.desc}
                              </div>
                            </div>
                          </button>
                        {/each}
                      </div>

                      <!-- Sub-menu popup / drawer -->
                      {#if activeWelcomeCard}
                        {@const currentTask = quickTasks.find((t) => t.key === activeWelcomeCard)}
                        {#if currentTask}
                          <div
                            class="mt-2 w-full max-h-44 overflow-y-auto rounded-xl border border-border/70 bg-popover/95 p-1.5 shadow-lg backdrop-blur-md animate-in fade-in slide-in-from-top-1 duration-150 text-popover-foreground"
                          >
                            {#if activeWelcomeSub === "internal-tools"}
                              <!-- Tertiary templates list -->
                              <div
                                class="flex items-center justify-between px-3 py-1.5 border-b border-border/40 mb-1"
                              >
                                <button
                                  class="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                                  onclick={() => (activeWelcomeSub = null)}
                                >
                                  <svg
                                    class="h-3.5 w-3.5"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"><path d="m15 18-6-6 6-6" /></svg
                                  >
                                  <span>返回上级菜单</span>
                                </button>
                                <span class="text-[11px] font-medium text-muted-foreground/70"
                                  >选择工具模板</span
                                >
                              </div>
                              <div class="space-y-0.5">
                                {#each currentTask.subItems.find((s) => s.subKey === "internal-tools")?.templates ?? [] as tmpl}
                                  <button
                                    class="flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left text-xs font-medium text-foreground/90 hover:bg-accent transition-all group"
                                    onclick={() => applyTemplateToPrompt(tmpl.prompt)}
                                  >
                                    <div class="flex h-5 w-5 items-center justify-center shrink-0">
                                      {#if tmpl.type === "linear"}
                                        <svg
                                          class="h-3.5 w-3.5 text-muted-foreground group-hover:text-foreground"
                                          viewBox="0 0 24 24"
                                          fill="currentColor"
                                          ><circle cx="12" cy="12" r="10" opacity="0.3" /><path
                                            d="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm0 14a4 4 0 1 1 4-4 4 4 0 0 1-4 4z"
                                          /></svg
                                        >
                                      {:else if tmpl.type === "slack"}
                                        <svg
                                          class="h-3.5 w-3.5 text-muted-foreground group-hover:text-foreground"
                                          viewBox="0 0 24 24"
                                          fill="currentColor"
                                          ><path
                                            d="M6 15a2 2 0 1 1 0 4 2 2 0 0 1 0-4zm0-6a2 2 0 1 1 0 4h6a2 2 0 1 1 0-4H6zm12 0a2 2 0 1 1 0 4 2 2 0 0 1 0-4zm-6 6a2 2 0 1 1 0 4 2 2 0 0 1 0-4z"
                                          /></svg
                                        >
                                      {:else if tmpl.type === "drive"}
                                        <svg
                                          class="h-3.5 w-3.5 text-muted-foreground group-hover:text-foreground"
                                          viewBox="0 0 24 24"
                                          fill="none"
                                          stroke="currentColor"
                                          stroke-width="2"><path d="M12 2 2 19h20L12 2z" /></svg
                                        >
                                      {:else}
                                        <svg
                                          class="h-3.5 w-3.5 text-muted-foreground group-hover:text-foreground"
                                          viewBox="0 0 24 24"
                                          fill="none"
                                          stroke="currentColor"
                                          stroke-width="2"
                                          ><rect
                                            x="2"
                                            y="3"
                                            width="20"
                                            height="14"
                                            rx="2"
                                            ry="2"
                                          /><line x1="8" y1="21" x2="16" y2="21" /><line
                                            x1="12"
                                            y1="17"
                                            x2="12"
                                            y2="21"
                                          /></svg
                                        >
                                      {/if}
                                    </div>
                                    <span class="flex-1 truncate">{tmpl.label}</span>
                                    <span
                                      class="text-[10px] text-muted-foreground/50 opacity-0 group-hover:opacity-100 transition-opacity"
                                      >填入 &rarr;</span
                                    >
                                  </button>
                                {/each}
                              </div>
                            {:else}
                              <!-- Sub-items list -->
                              <div class="space-y-0.5">
                                {#each currentTask.subItems as item}
                                  <button
                                    class="flex w-full items-center justify-between gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs font-medium text-foreground/90 hover:bg-accent transition-all group"
                                    onclick={() => {
                                      if (item.subKey) {
                                        activeWelcomeSub = item.subKey;
                                      } else if (item.prompt) {
                                        applyTemplateToPrompt(item.prompt);
                                      }
                                    }}
                                  >
                                    <div class="flex items-center gap-2 min-w-0">
                                      <div
                                        class="flex h-5 w-5 items-center justify-center shrink-0 text-muted-foreground group-hover:text-foreground"
                                      >
                                        {#if currentTask.key === "explore"}
                                          <svg
                                            class="h-3.5 w-3.5"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2"
                                            ><circle cx="12" cy="12" r="10" /><polygon
                                              points="16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76"
                                            /></svg
                                          >
                                        {:else if currentTask.key === "build"}
                                          <svg
                                            class="h-3.5 w-3.5"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2"
                                            stroke-linecap="round"
                                            stroke-linejoin="round"
                                            ><path
                                              d="m15 12-8.5 8.5a2.12 2.12 0 1 1-3-3L12 9"
                                            /><path d="M17.64 15 22 10.64" /></svg
                                          >
                                        {:else if currentTask.key === "review"}
                                          <svg
                                            class="h-3.5 w-3.5"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2"
                                            stroke-linecap="round"
                                            stroke-linejoin="round"
                                            ><circle cx="11" cy="11" r="8" /><path
                                              d="m21 21-4.3-4.3"
                                            /></svg
                                          >
                                        {:else}
                                          <svg
                                            class="h-3.5 w-3.5"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2"
                                            stroke-linecap="round"
                                            stroke-linejoin="round"
                                            ><path
                                              d="m15 12-8.5 8.5a2.12 2.12 0 1 1-3-3L12 9"
                                            /><path d="M17.64 15 22 10.64" /></svg
                                          >
                                        {/if}
                                      </div>
                                      <span class="truncate">{item.label}</span>
                                    </div>
                                    {#if item.subKey}
                                      <svg
                                        class="h-3.5 w-3.5 shrink-0 text-muted-foreground/60"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"><path d="m9 18 6-6-6-6" /></svg
                                      >
                                    {:else}
                                      <span
                                        class="text-[10px] text-muted-foreground/50 opacity-0 group-hover:opacity-100 transition-opacity"
                                        >填入 &rarr;</span
                                      >
                                    {/if}
                                  </button>
                                {/each}
                              </div>
                            {/if}
                          </div>
                        {/if}
                      {/if}
                    </div>
                  {/if}

                  <!-- Embedded PromptInput in welcome area (centered like Work mode) -->
                  <div class="relative mt-2 w-full max-w-4xl px-0 chat-welcome-composer">
                    {#if chatToast}
                      <div
                        class="pointer-events-none absolute bottom-full left-1/2 z-50 mb-1.5 flex w-max max-w-[min(720px,calc(100vw-2rem))] -translate-x-1/2 items-center justify-center rounded-lg border bg-background/95 px-4 py-2 text-center text-sm shadow-lg backdrop-blur-sm animate-in fade-in slide-in-from-bottom-2 duration-200"
                        role="status"
                        aria-live="polite"
                      >
                        {chatToast}
                      </div>
                    {/if}
                    {#key store.run?.id ?? ""}
                      <PromptInput
                        bind:this={promptRef}
                        harness={currentHarness}
                        projectPicker={currentHarness === "code" ? codeProjectPickerConfig : null}
                        agent={effectiveAgent}
                        sessionInfo={currentSessionInfo}
                        enabledAgents={computedEnabledAgents}
                        capabilities={effectiveCapabilities}
                        planModeActive={store.planModeActive}
                        running={store.isActivelyRunning}
                        disabled={inputBlockedByPermission}
                        pendingPermission={store.hasInlinePermission}
                        hasRun={!!store.run || store.timeline.length > 0}
                        sessionAlive={store.sessionAlive}
                        canResume={!store.sessionAlive &&
                          canResumeNow(
                            store.run,
                            store.phase,
                            agentSettings?.no_session_persistence ?? false,
                          )}
                        useStreamSession={store.useStreamSession}
                        isRemote={store.isRemote}
                        cliCommands={nativeSlashCommands}
                        models={effectiveModels}
                        currentModel={store.model}
                        currentEffort={effortAvailable ? currentEffort : undefined}
                        permissionMode={effectiveCapabilities.ui.permissionModeSwitch
                          ? store.permissionMode
                          : "default"}
                        sessionModes={store.sessionModes}
                        sessionMode={store.sessionMode}
                        onSessionModeChange={effectiveCapabilities.protocol.sessionModeControl
                          ? handleSessionModeChange
                          : undefined}
                        onOpenGoal={goalPanelAvailable ? () => (goalPanelOpen = true) : undefined}
                        goalAvailable={goalActionAvailable}
                        planAvailable={planActionAvailable}
                        {piFeatureDiscoveryPending}
                        piFeatureColdStartAvailable={effectiveAgent === "pi" &&
                          !store.run &&
                          !store.sessionAlive}
                        piFeatureStartupPending={effectiveAgent === "pi" && piFeatureStartupPending}
                        goalActive={goalActiveForComposer}
                        onClearGoal={handleClearGoalChip}
                        onClearPlan={handleClearPlanChip}
                        piPlan={effectiveAgent === "pi" &&
                        (store.piCapabilities === null || store.piCapabilities.planAvailable)
                          ? store.piPlanState
                          : null}
                        piGoal={piGoalForComposer}
                        piPermission={effectiveAgent === "pi" &&
                        (store.piCapabilities === null || store.piCapabilities.permissionAvailable)
                          ? store.piPermissionState
                          : null}
                        {piPermissionBusy}
                        piFeaturePending={effectiveAgent === "pi" ? piFeaturePending : null}
                        onPiPlan={effectiveAgent === "pi" ? handlePiPlan : undefined}
                        onPiGoal={effectiveAgent === "pi" ? handlePiGoal : undefined}
                        onPiPermission={effectiveAgent === "pi"
                          ? handlePiPermissionMode
                          : undefined}
                        cwd={store.effectiveCwd ||
                          folderCwdOverride ||
                          getSavedProjectCwd(currentRealm) ||
                          ""}
                        authMode={store.authMode}
                        platformId={store.platformId ?? "anthropic"}
                        platformCredentials={settings?.platform_credentials ?? []}
                        onSend={sendMessage}
                        queuedMessages={store.queuedMessages}
                        onQueueSend={handleQueueSend}
                        onQueueSteer={handleQueueSteer}
                        onQueueEdit={editQueuedMessage}
                        onQueueDelete={(id) => store.deleteQueuedMessage(id)}
                        onBtwSend={handleBtwSend}
                        onAgentChange={handleAgentChange}
                        onContinueConversation={store.run && !forkOverlay && !continuationBusy
                          ? openEndContinuationMenu
                          : undefined}
                        onInterrupt={() => store.interrupt()}
                        onModelSwitch={handleModelChange}
                        onEffortChange={effortAvailable ? handleEffortChange : undefined}
                        onPlanModeChange={effectiveAgent !== "pi" &&
                        effectiveCapabilities.ui.planModeToggle &&
                        effectiveCapabilities.protocol.planMode
                          ? handlePlanModeChange
                          : undefined}
                        onPermissionModeChange={effectiveAgent !== "pi" &&
                        effectiveCapabilities.ui.permissionModeSwitch
                          ? handlePermissionModeChange
                          : undefined}
                        onVirtualCommand={handleVirtualCommand}
                        fastModeState={store.fastModeState}
                        onFastModeSwitch={handleFastModeSwitch}
                        onPlatformChange={handlePlatformChange}
                        {authOverview}
                        authSourceLabel={store.authSourceLabel}
                        authSourceCategory={store.authSourceCategory}
                        apiKeySource={store.apiKeySource}
                        onAuthModeChange={handleAuthModeChange}
                        {localProxyStatuses}
                        showAuthBadge={false}
                        onShortcutHelp={() => (shortcutHelpOpen = !shortcutHelpOpen)}
                        availableSkills={piAvailableSkillNames}
                        {skillItems}
                        codexSkillItems={codexRuntimeSkills}
                        hasStash={!!stashedInput}
                        {userHistory}
                        runId={store.run?.id ?? ""}
                        initialDraft={promptDraftsByRun.get(store.run?.id ?? "")}
                        onDraftChange={savePromptDraft}
                        onRestoreStash={() => {
                          if (stashedInput) {
                            promptRef?.restoreSnapshot(stashedInput);
                            stashedInput = null;
                            showChatToast(t("toast_stashRestored"));
                          }
                        }}
                      />
                    {/key}
                  </div>
                </div>
              </div>
            {:else if store.phase === "loading" && store.timeline.length === 0}
              <!-- Loading state — avoids welcome page flash during loadRun -->
              <div class="flex h-full items-center justify-center">
                <div
                  class="h-5 w-5 rounded-full border-2 border-muted-foreground/30 border-t-primary animate-spin"
                ></div>
              </div>
            {:else}
              <!-- Timeline: chat messages + inline tool cards -->
              <div data-conversation-root>
                {#if chatSearchOpen}
                  <div
                    class="sticky top-0 z-20 flex items-center justify-between gap-2 border-b border-border/30 bg-background/95 px-4 py-1.5 backdrop-blur-sm sm:px-6"
                    data-export-exclude
                  >
                    <div class="flex-1 flex justify-end min-w-0">
                      <ChatSearchToolbar
                        bind:this={chatSearchToolbarRef}
                        container={chatAreaRef ?? null}
                        bind:open={chatSearchOpen}
                      />
                    </div>
                    {#if !isChatAutoScroll}
                      <button
                        type="button"
                        class="shrink-0 rounded-md px-2 py-1 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
                        onclick={scrollChatToBottom}
                      >
                        回到最新
                      </button>
                    {/if}
                  </div>
                {/if}
                {#if store.run?.parent_run_id}
                  <div class="chat-content-width py-2" data-export-exclude>
                    <div
                      class="flex items-center gap-2 rounded-md border border-blue-500/20 bg-blue-500/5 px-3 py-2 text-xs text-blue-400"
                    >
                      <svg
                        class="h-3.5 w-3.5 shrink-0"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      >
                        <circle cx="12" cy="18" r="3" /><circle cx="6" cy="6" r="3" /><circle
                          cx="18"
                          cy="6"
                          r="3"
                        />
                        <path d="M18 9v2c0 .6-.4 1-1 1H7c-.6 0-1-.4-1-1V9" /><path d="M12 12v3" />
                      </svg>
                      <span class="text-foreground/60">{t("chat_forkedBanner")}</span>
                      <button
                        class="ml-auto shrink-0 text-blue-400 hover:text-blue-300 underline underline-offset-2"
                        onclick={() =>
                          goto(getRunRoute(store.run!, { runId: store.run!.parent_run_id }))}
                        >{t("chat_viewParent")}</button
                      >
                    </div>
                  </div>
                {/if}
                {#if notificationVisible && latestNotification}
                  <div class="chat-content-width py-1" data-export-exclude>
                    <div
                      class="flex items-center gap-2 text-xs text-muted-foreground bg-teal-500/5 border border-teal-500/20 rounded px-3 py-1.5 animate-fade-in"
                    >
                      <span class="h-1.5 w-1.5 rounded-full bg-teal-500 animate-pulse"></span>
                      Task #{latestNotification.task_id}: {latestNotification.status}
                    </div>
                  </div>
                {/if}
                {#if filteredTimeline.length - renderLimit > 0}
                  <div bind:this={topSentinel} aria-hidden="true" class="h-px w-full"></div>
                {/if}
                {#each presentationTurns as turn, turnIdx (turn.id)}
                  {@const isLatestTurn = turnIdx === presentationTurns.length - 1}
                  <!-- Token usage before turn -->
                  {#if turn.userTimelineIndex !== undefined && usageAnnotations.has(turn.userTimelineIndex)}
                    {@const tu = usageAnnotations.get(turn.userTimelineIndex)}
                    {#if tu}
                      <div class="w-full py-1.5">
                        <div class="chat-content-width">
                          <div class="flex items-center gap-3">
                            <div class="h-px flex-1 bg-border/40"></div>
                            <span class="text-[10px] tabular-nums text-muted-foreground">
                              {formatTokenCount(tu.inputTokens)}
                              {t("chat_usageIn")} · {formatTokenCount(tu.outputTokens)}
                              {t("chat_usageOut")}
                              {#if tu.cacheReadTokens > 0 || tu.cacheWriteTokens > 0}
                                · {t("chat_usageCache", {
                                  read: formatTokenCount(tu.cacheReadTokens),
                                  write: formatTokenCount(tu.cacheWriteTokens),
                                })}
                              {/if}
                            </span>
                            <div class="h-px flex-1 bg-border/40"></div>
                          </div>
                        </div>
                      </div>
                    {/if}
                  {/if}

                  <!-- User Message -->
                  {#if turn.userMessage}
                    <div
                      id="msg-{turn.userMessage.anchorId}"
                      data-entry-id={turn.userMessage.id}
                      class:cv-auto={true}
                      class="group/msg"
                      class:opacity-40={lastClearSepId !== null &&
                        (timelineIdIndex.get(turn.userMessage.id) ?? 0) <
                          (timelineIdIndex.get(lastClearSepId) ?? 0)}
                    >
                      <ConversationMessage
                        message={{
                          id: turn.userMessage.id,
                          role: "user",
                          content: turn.userMessage.content,
                          timestamp: turn.userMessage.timestamp,
                        }}
                        attachments={turn.userMessage.attachments}
                        onRewind={effectiveCapabilities.execution.snapshots &&
                        turn.userMessage.cliUuid &&
                        store.sessionAlive &&
                        !store.isRunning
                          ? () =>
                              handleRewindToMessage({
                                cliUuid: turn.userMessage!.cliUuid!,
                                content: turn.userMessage!.content,
                                ts: turn.userMessage!.timestamp,
                              })
                          : undefined}
                        onEdit={store.sessionAlive && !store.isRunning
                          ? (newContent) =>
                              handleEditAndResend(
                                turn.turnIndex,
                                newContent,
                                turn.userMessage?.attachments,
                              )
                          : undefined}
                        isRunning={store.isRunning}
                        agent={effectiveAgent}
                      />
                    </div>
                  {/if}

                  <!-- Separators (e.g. context cleared) -->
                  {#if turn.separators}
                    {#each turn.separators as sep (sep.id)}
                      <div class="w-full py-3">
                        <div class="chat-content-width">
                          <div class="flex items-center gap-3">
                            <div class="h-px flex-1 bg-amber-500/20"></div>
                            <span class="text-xs text-amber-500/70 font-medium whitespace-nowrap">
                              {sep.content === CONTEXT_CLEARED_MARKER
                                ? t("chat_contextCleared")
                                : sep.content}
                            </span>
                            <div class="h-px flex-1 bg-amber-500/20"></div>
                          </div>
                        </div>
                      </div>
                    {/each}
                  {/if}

                  <!-- Hooks -->
                  {#if turn.hooks}
                    {#each turn.hooks as hook (hook.id)}
                      <HookExecutionCard
                        eventName={hook.eventName}
                        status={hook.status}
                        statusMessage={hook.statusMessage}
                        durationMs={hook.durationMs}
                      />
                    {/each}
                  {/if}

                  <!-- DSH Context Injection (System Prompt & Project Rules) -->
                  {#if turnIdx === 0 && store.effectiveCwd}
                    {@const cwdFolder =
                      store.effectiveCwd.replace(/\\/g, "/").split("/").filter(Boolean).pop() ||
                      "AgentCabin"}
                    <div class="chat-content-width pb-1.5">
                      <ContextInjectionRow
                        type="injection"
                        title="上下文"
                        source={cwdFolder}
                        summary={`工作目录: ${store.effectiveCwd}`}
                        content={`CWD: ${store.effectiveCwd}`}
                      />
                    </div>
                  {/if}

                  <!-- Assistant Turn Header -->
                  {#if turn.processBlocks.length > 0 || turn.finalMessage || (turn.isRunning && isLatestTurn)}
                    <AssistantTurnHeader
                      timestamp={turn.finalMessage?.timestamp || turn.userMessage?.timestamp}
                      agent={effectiveAgent}
                      displayName={CONVERSATION_ASSISTANT_NAME}
                    />
                  {/if}

                  <!-- Process Stream (Interleaved: Reasoning -> Activity Group -> Interactions) -->
                  {#if turn.processBlocks.length > 0 || turn.interactionBlocks.length > 0 || (turn.isRunning && isLatestTurn)}
                    <div class="chat-content-width">
                      <ChatProcessStream
                        {turn}
                        runId={store.run?.id ?? ""}
                        {fetchToolResult}
                        onAnswer={(itemId: string, answer: string) => {
                          void handleToolAnswer(itemId, answer);
                        }}
                        onApprove={handleToolApprove}
                        onPermissionRespond={handlePermissionRespond}
                        onExitPlanClearContext={handleExitPlanClearContext}
                        taskNotifications={store.taskNotifications}
                        showPermissionInPanel={showPermissionPanel}
                        {agentDisplayName}
                        onPreviewFile={openPreviewForPath}
                        onToggleCollapse={() => {
                          expandedCodeTurns[turn.id] = turn.isCollapsed;
                        }}
                        onOpenDetails={openToolDetails}
                        error={isLatestTurn && store.run?.status === "failed"
                          ? { message: "运行中断或失败" }
                          : undefined}
                        onContinue={() => void sendMessage("继续", [])}
                      />
                    </div>
                  {/if}

                  <!-- Command Outputs (e.g. ContextUsageGrid, CostSummaryView, ReleaseNotesCard) -->
                  {#if turn.commandOutputs}
                    {#each turn.commandOutputs as cmdOut (cmdOut.id)}
                      <div class="w-full py-2">
                        <div class="chat-content-width">
                          <div
                            class="command-output rounded-[var(--chat-radius-md,10px)] border border-[var(--chat-border,rgba(17,24,39,0.07))] bg-[var(--chat-surface-code)] px-4 py-3 text-[var(--chat-code-size,13px)] overflow-x-auto"
                          >
                            {#if cmdOut.content.includes("## Context Usage")}
                              <ContextUsageGrid text={cmdOut.content} />
                            {:else if cmdOut.content.includes("Total cost:") && cmdOut.content.includes("Total duration")}
                              <CostSummaryView text={cmdOut.content} />
                            {:else if cmdOut.content
                              .trimStart()
                              .startsWith("Version ") && cmdOut.content.includes("•")}
                              <ReleaseNotesCard text={cmdOut.content} />
                            {:else if hasAnsiCodes(cmdOut.content)}
                              <pre
                                class="whitespace-pre font-mono text-xs leading-relaxed text-foreground dark:text-[#c0caf5] m-0">{@html ansiToHtml(
                                  cmdOut.content,
                                )}</pre>
                            {:else}
                              <MarkdownContent text={cmdOut.content} />
                            {/if}
                          </div>
                        </div>
                      </div>
                    {/each}
                  {/if}

                  <!-- Final Assistant Response (Pure Markdown, High visual weight, No bubble border) -->
                  {#if turn.finalMessage}
                    {@const currentTurnUsage =
                      usageByTurn.get(turn.turnIndex + 1) ??
                      usageByTurn.get(turn.turnIndex) ??
                      // DSH ACP usage notifications may arrive without the backend turn index.
                      // Keep historical messages exact, but let the latest answer show the usage
                      // that is already present in the conversation-level stats line.
                      (isLatestTurn ? store.turnUsages.at(-1) : undefined)}
                    <div
                      id="msg-{turn.finalMessage.anchorId}"
                      data-entry-id={turn.finalMessage.id}
                      class:cv-auto={true}
                      class="group/msg"
                      class:opacity-40={lastClearSepId !== null &&
                        (timelineIdIndex.get(turn.finalMessage.id) ?? 0) <
                          (timelineIdIndex.get(lastClearSepId) ?? 0)}
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
                          onExport={store.run ? () => void handleExportHtml() : undefined}
                          onContinueFromMessage={canOpenContinuation()
                            ? () => openContinuationMenu(turn.finalMessage!.id)
                            : undefined}
                        />
                      </div>
                    </div>
                  {/if}

                  <!-- Turn summary diff review (TurnSummaryBanner) -->
                  {#if turn.turnSummary}
                    {@const summary = parseUnifiedDiffStats(turn.turnSummary.diff)}
                    {#if summary.files.length > 0 && !dismissedTurnSummaryIds.has(turn.turnSummary.id)}
                      <div class="w-full py-3">
                        <TurnSummaryBanner
                          {summary}
                          onUndo={undoneTurnSummaryIds.has(turn.turnSummary.id)
                            ? undefined
                            : () =>
                                void undoTurnChanges({
                                  id: turn.turnSummary!.id,
                                  anchorId: turn.turnSummary!.anchorId,
                                  diff: turn.turnSummary!.diff,
                                  cwd: turn.turnSummary!.cwd,
                                  ts: turn.turnSummary!.ts,
                                  kind: "turn_summary",
                                })}
                          onReview={() =>
                            reviewTurnChanges({
                              id: turn.turnSummary!.id,
                              anchorId: turn.turnSummary!.anchorId,
                              diff: turn.turnSummary!.diff,
                              cwd: turn.turnSummary!.cwd,
                              ts: turn.turnSummary!.ts,
                              kind: "turn_summary",
                            })}
                          undoBusy={undoingTurnSummaryIds.has(turn.turnSummary.id)}
                          onDismiss={() => dismissTurnSummary(turn.turnSummary!.id)}
                        />
                      </div>
                    {/if}
                  {/if}
                {/each}
                {#each rewindMarkers as marker, mi (marker.id)}
                  <div
                    class="w-full py-3"
                    id={mi === rewindMarkers.length - 1 ? "rewind-marker-latest" : undefined}
                  >
                    <div class="chat-content-width">
                      <div class="flex items-center gap-3">
                        <div class="h-px flex-1 bg-blue-500/20"></div>
                        <div class="flex items-center gap-2 text-xs text-blue-500/80 font-medium">
                          <svg
                            class="h-3.5 w-3.5"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                          >
                            <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
                            <path d="M3 3v5h5" />
                          </svg>
                          <span
                            >{t("rewind_separatorLabel", {
                              count: String(marker.filesReverted.length),
                            })}</span
                          >
                        </div>
                        <div class="h-px flex-1 bg-blue-500/20"></div>
                      </div>
                      <div class="mt-1 ml-8 text-[11px] text-muted-foreground/60 truncate">
                        &ldquo;{marker.targetContent}&rdquo;
                      </div>
                      {#if marker.filesReverted.length > 0}
                        <details class="mt-1 ml-8">
                          <summary
                            class="cursor-pointer text-[10px] text-blue-500/50 hover:text-blue-500/80"
                          >
                            {t("rewind_separatorFiles", {
                              count: String(marker.filesReverted.length),
                            })}
                          </summary>
                          <div class="mt-1 rounded bg-muted/30 px-2 py-1">
                            {#each marker.filesReverted as file}
                              <div class="truncate font-mono text-[10px] text-muted-foreground">
                                {file}
                              </div>
                            {/each}
                          </div>
                        </details>
                      {/if}
                    </div>
                  </div>
                {/each}

                <!-- Pending hook callbacks (runtime UI — excluded from export) -->
                {#each store.hookEvents.filter((h) => h.status === "hook_pending") as hookEvent (hookEvent.request_id)}
                  <div class="chat-content-width" data-export-exclude>
                    <HookReviewCard {hookEvent} onRespond={handleHookCallbackRespond} />
                  </div>
                {/each}

                <!-- Slash command processing indicator -->
                {#if processingSlashCmd && !store.streamingText && !store.thinkingText}
                  <div class="w-full animate-fade-in" data-export-exclude>
                    <div class="chat-content-width py-2">
                      <div class="flex items-center gap-2 text-sm text-muted-foreground">
                        <div
                          class="h-3.5 w-3.5 rounded-full border-2 border-border border-t-muted-foreground animate-spin"
                        ></div>
                        <span>{t("chat_processingCommand", { command: processingSlashCmd })}</span>
                      </div>
                    </div>
                  </div>
                {/if}
              </div>
            {/if}
            <ToBottomButton visible={!isChatAutoScroll} onClick={scrollChatToBottom} />
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
          {#if showChatScrollHint}
            <button
              class="absolute bottom-3 left-1/2 -translate-x-1/2 z-10 flex items-center gap-1.5 rounded-full bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground shadow-lg transition-all duration-200 hover:bg-primary/90 animate-fade-in"
              onclick={scrollChatToBottom}
            >
              {t("chat_newMessages")}
              <svg
                class="h-3 w-3"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path d="m6 9 6 6 6-6" />
              </svg>
            </button>
          {/if}
        {:else if store.run && store.run.status !== "pending"}
          <!-- CLI mode: terminal -->
          <XTerminal
            bind:this={xtermRef}
            onResize={handleTermResize}
            onReady={handleTermReady}
            class="h-full"
          />
        {:else}
          <!-- CLI mode: welcome state -->
          <div class="flex h-full items-center justify-center">
            <div class="text-center max-w-md animate-slide-up">
              <img src="/logo.png?v=2" alt="OC" class="mx-auto mb-4 h-12 w-12 rounded-2xl" />
              <h2 class="text-lg font-semibold text-foreground mb-2">{t("layout_appName")}</h2>
              {#if codexWarning}
                <p class="text-amber-500 text-sm mb-3 px-2">{codexWarning}</p>
              {/if}
              <p class="text-sm text-muted-foreground mb-4">
                {store.run ? t("chat_typeToStartSession") : t("chat_startSessionHint")}
              </p>
              {#if !store.run}
                <div class="flex flex-col gap-2">
                  {#each examplePrompts as prompt}
                    <button
                      class="rounded-lg border border-white/10 bg-white/5 px-4 py-3 text-left text-sm text-neutral-300 hover:bg-white/10 hover:border-white/20 transition-all duration-150 group"
                      onclick={() => fillPrompt(prompt())}
                    >
                      <span
                        class="text-muted-foreground/50 mr-2 group-hover:text-muted-foreground transition-colors"
                        >&rarr;</span
                      >
                      {prompt()}
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        {/if}

        <!-- Fork overlay -->
        {#if forkOverlay}
          <div
            class="absolute inset-0 z-20 flex items-center justify-center bg-background/80 backdrop-blur-sm animate-fade-in"
          >
            {#if forkOverlay.error}
              <!-- Error state -->
              <div class="flex flex-col items-center gap-4 max-w-sm text-center animate-slide-up">
                <div
                  class="flex h-12 w-12 items-center justify-center rounded-full bg-destructive/10"
                >
                  <svg
                    class="h-6 w-6 text-destructive"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <circle cx="12" cy="12" r="10" /><path d="m15 9-6 6" /><path d="m9 9 6 6" />
                  </svg>
                </div>
                <div>
                  <h3 class="text-sm font-semibold text-foreground mb-1">{t("chat_forkFailed")}</h3>
                  <p class="text-xs text-muted-foreground">{forkOverlay.error}</p>
                </div>
                <div class="flex items-center gap-2">
                  <button
                    class="rounded-lg border border-border bg-muted px-4 py-2 text-sm text-foreground hover:bg-accent transition-colors"
                    onclick={handleForkCancel}>{t("common_cancel")}</button
                  >
                  <button
                    class="rounded-lg bg-primary px-4 py-2 text-sm text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
                    disabled={resuming}
                    onclick={handleForkRetry}>{t("common_retry")}</button
                  >
                </div>
              </div>
            {:else}
              <!-- In-progress state -->
              <div class="flex flex-col items-center gap-4 max-w-sm text-center animate-slide-up">
                <div class="flex h-12 w-12 items-center justify-center rounded-full bg-blue-500/10">
                  <svg
                    class="h-6 w-6 text-blue-400 animate-spin"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <path d="M21 12a9 9 0 1 1-6.219-8.56" />
                  </svg>
                </div>
                <div>
                  <h3 class="text-sm font-semibold text-foreground mb-1">
                    {t("chat_forkingSession")}
                  </h3>
                  <p class="text-xs text-muted-foreground">
                    {t("chat_forkingDesc")}
                  </p>
                </div>
                {#if forkElapsed > 0}
                  <span class="text-xs tabular-nums text-muted-foreground"
                    >{formatElapsed(forkElapsed)}</span
                  >
                {/if}
                <button
                  class="rounded-lg border border-border bg-muted px-4 py-2 text-sm text-foreground hover:bg-accent transition-colors"
                  onclick={handleForkCancel}>{t("common_cancel")}</button
                >
              </div>
            {/if}
          </div>
        {/if}

        <!-- Classified error card -->
        {#if store.error && !forkOverlay}
          {@const classified = classifyError(store.run?.result_subtype, store.error)}
          {@const catIcon =
            classified.category === "context_limit"
              ? "⚠"
              : classified.category === "auth_issue"
                ? "🔑"
                : classified.category === "budget_limit"
                  ? "💰"
                  : classified.category === "server_issue"
                    ? "☁"
                    : classified.category === "session_timeout"
                      ? "⏱"
                      : classified.category === "tool_issue"
                        ? "🔧"
                        : "❌"}
          <div class="absolute bottom-14 left-3 right-3 z-10">
            <div
              class="rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm backdrop-blur-sm animate-fade-in"
            >
              <div class="flex items-start gap-2">
                <span class="shrink-0 text-base leading-none mt-0.5">{catIcon}</span>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2 mb-1">
                    <span
                      class="text-[10px] font-medium uppercase tracking-wider text-destructive/70"
                      >{t(`error_category_${classified.category}`)}</span
                    >
                  </div>
                  <p class="text-destructive text-xs leading-relaxed break-words">{store.error}</p>
                  <p class="text-destructive/60 text-[10px] mt-1">
                    {t(`error_guidance_${classified.category}`)}
                  </p>
                </div>
                <button
                  class="shrink-0 text-destructive/50 hover:text-destructive text-xs"
                  onclick={() => (store.error = "")}>{t("common_dismiss")}</button
                >
              </div>
              <div class="flex items-center gap-2 mt-2 pl-6">
                {#if classified.canRetry && store.phase === "failed" && store.run?.session_id}
                  <button
                    class="rounded px-2.5 py-1 text-xs bg-destructive/20 hover:bg-destructive/30 text-destructive transition-colors"
                    onclick={() => handleResume("continue")}>{t("common_retry")}</button
                  >
                {/if}
                {#if classified.settingsLink}
                  <button
                    class="rounded px-2.5 py-1 text-xs bg-destructive/20 hover:bg-destructive/30 text-destructive transition-colors"
                    onclick={() => goto(classified.settingsLink)}>{t("chat_checkSettings")}</button
                  >
                {/if}
              </div>
            </div>
          </div>
        {/if}
      </div>

      <!-- Resume warning (if applicable) -->
      {#if canResumeNow(store.run, store.phase, agentSettings?.no_session_persistence ?? false) && getResumeWarning(store.run)}
        <div
          class="mx-3 mt-2 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-2 text-xs text-amber-400"
        >
          {getResumeWarning(store.run)}
        </div>
      {/if}

      {#if effectiveAgent === "pi"}
        <PiExtensionPanel hostState={store.piExtensionHostState} />
      {/if}

      <!-- Floating permission panel (above input bar) -->
      {#if showPermissionPanel}
        <PermissionPanel
          pendingTools={pendingToolPermissions}
          onPermissionRespond={handlePermissionRespond}
          {agentDisplayName}
        />
      {/if}

      <!-- MCP Elicitation dialog (above input bar) -->
      {#if store.hasElicitation && store.sessionAlive}
        <ElicitationDialog
          elicitations={store.pendingElicitations}
          onRespond={handleElicitationRespond}
        />
      {/if}

      <!-- BTW side question drawer -->
      {#if btwState.active}
        <div
          class="border-t border-blue-500/30 bg-blue-500/5"
          style="max-height: 40vh; overflow-y: auto;"
        >
          <div class="flex items-center justify-between px-4 py-2 border-b border-border/50">
            <span class="text-xs font-medium text-blue-400">BTW</span>
            <button
              onclick={() => (btwState = { ...btwState, active: false })}
              title="Close side question"
              class="text-muted-foreground hover:text-foreground transition-colors"
            >
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path d="M18 6 6 18" /><path d="m6 6 12 12" />
              </svg>
            </button>
          </div>
          <div class="px-4 py-3 space-y-2">
            <p class="text-xs text-muted-foreground">Q: {btwState.question}</p>
            <div class="text-sm">
              {#if btwState.error}
                <p class="text-destructive">{btwState.error}</p>
              {:else if btwState.answer}
                <MarkdownContent text={btwState.answer} streaming={btwState.loading} />
              {/if}
              {#if btwState.loading}
                <span class="inline-block w-2 h-4 bg-blue-400 animate-pulse rounded-sm"></span>
              {/if}
            </div>
          </div>
        </div>
      {/if}

      <!-- Input bar -->
      <!-- Ralph Loop status bar -->
      {#if store.ralphLoop?.active}
        <div class="mx-auto w-full max-w-4xl px-4 pb-2">
          <div
            class="flex items-center justify-between rounded-lg border border-blue-500/30 bg-blue-500/10 px-4 py-2 text-sm"
          >
            <div class="flex items-center gap-2 text-blue-400">
              <span class="animate-pulse">🔄</span>
              <span class="font-medium">Ralph Loop</span>
              <span class="text-blue-400/70">
                iteration {store.ralphLoop.iteration}/{store.ralphLoop.maxIterations || "∞"}
              </span>
              {#if store.ralphLoop.completionPromise}
                <span class="text-blue-400/50">
                  · promise: "{store.ralphLoop.completionPromise}"
                </span>
              {/if}
            </div>
            <button
              class="rounded px-2 py-0.5 text-xs text-red-400 hover:bg-red-500/20 transition-colors"
              onclick={handleRalphCancel}
            >
              Cancel
            </button>
          </div>
        </div>
      {/if}

      <ScheduledTasksChip
        tasks={store.scheduledTasks}
        busy={store.isRunning}
        onCancel={(id) => store.sendMessage(`Cancel scheduled task ${id}`, [])}
        onList={() => store.sendMessage("Show me my scheduled tasks", [])}
      />

      <TodoPanel
        tasks={store.todoPanelVisible ? store.panelTasks : []}
        piTodoState={store.todoPanelVisible && effectiveAgent === "pi" ? store.piTodoState : null}
        changeSummary={turnChanges}
        onViewDiff={effectiveTurnDiff.trim()
          ? () => {
              dbg("chat", "turn diff open", { len: effectiveTurnDiff.length });
              turnDiffOpen = true;
            }
          : undefined}
      />

      {#if (store.sessionAlive || !store.run || store.phase === "empty" || store.phase === "ready" || TERMINAL_PHASES.includes(store.phase)) && !welcomeVisible}
        <div class="relative shrink-0 mx-auto w-full max-w-4xl px-4 pb-3 pt-1">
          {#if chatToast}
            <div
              class="pointer-events-none absolute bottom-full left-1/2 z-50 mb-1.5 flex w-max max-w-[min(720px,calc(100vw-2rem))] -translate-x-1/2 items-center justify-center rounded-lg border bg-background/95 px-4 py-2 text-center text-sm shadow-lg backdrop-blur-sm animate-in fade-in slide-in-from-bottom-2 duration-200"
              role="status"
              aria-live="polite"
            >
              {chatToast}
            </div>
          {/if}

          {#if isLegacyDshRun}
            <div
              class="mb-3 rounded-2xl border border-amber-500/30 bg-amber-500/10 p-3 text-xs text-amber-700 dark:text-amber-300 flex items-center gap-2 shadow-sm"
              role="alert"
            >
              <svg
                class="h-4 w-4 shrink-0 text-amber-500"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <circle cx="12" cy="12" r="10" />
                <line x1="12" y1="8" x2="12" y2="12" />
                <line x1="12" y1="16" x2="12.01" y2="16" />
              </svg>
              <span
                >此历史会话使用已移除的 DeepSeek Harness Runtime，仅支持查看，无法继续运行。</span
              >
            </div>
          {/if}

          <!-- Key on run id so each chat gets a clean PromptInput, seeded from its own saved draft -->
          {#key store.run?.id ?? ""}
            <!-- Pi uses extension-owned Plan and permission controls; do not expose the generic
               permission-mode controls while Pi capabilities are loading. -->
            <PromptInput
              bind:this={promptRef}
              harness={currentHarness}
              projectPicker={currentHarness === "code" ? codeProjectPickerConfig : null}
              agent={effectiveAgent}
              sessionInfo={currentSessionInfo}
              enabledAgents={computedEnabledAgents}
              capabilities={effectiveCapabilities}
              planModeActive={store.planModeActive}
              running={store.isActivelyRunning}
              disabled={isLegacyDshRun || inputBlockedByPermission}
              pendingPermission={store.hasInlinePermission}
              hasRun={!!store.run || store.timeline.length > 0}
              sessionAlive={store.sessionAlive}
              canResume={!isLegacyDshRun &&
                !store.sessionAlive &&
                canResumeNow(
                  store.run,
                  store.phase,
                  agentSettings?.no_session_persistence ?? false,
                )}
              useStreamSession={store.useStreamSession}
              isRemote={store.isRemote}
              cliCommands={nativeSlashCommands}
              models={effectiveModels}
              currentModel={store.model}
              currentEffort={effortAvailable ? currentEffort : undefined}
              permissionMode={effectiveCapabilities.ui.permissionModeSwitch
                ? store.permissionMode
                : "default"}
              sessionModes={store.sessionModes}
              sessionMode={store.sessionMode}
              onSessionModeChange={effectiveCapabilities.protocol.sessionModeControl
                ? handleSessionModeChange
                : undefined}
              onOpenGoal={goalPanelAvailable ? () => (goalPanelOpen = true) : undefined}
              goalAvailable={goalActionAvailable}
              planAvailable={planActionAvailable}
              {piFeatureDiscoveryPending}
              piFeatureColdStartAvailable={effectiveAgent === "pi" &&
                !store.run &&
                !store.sessionAlive}
              piFeatureStartupPending={effectiveAgent === "pi" && piFeatureStartupPending}
              goalActive={goalActiveForComposer}
              onClearGoal={handleClearGoalChip}
              onClearPlan={handleClearPlanChip}
              piPlan={effectiveAgent === "pi" &&
              (store.piCapabilities === null || store.piCapabilities.planAvailable)
                ? store.piPlanState
                : null}
              piGoal={piGoalForComposer}
              piPermission={effectiveAgent === "pi" &&
              (store.piCapabilities === null || store.piCapabilities.permissionAvailable)
                ? store.piPermissionState
                : null}
              {piPermissionBusy}
              piFeaturePending={effectiveAgent === "pi" ? piFeaturePending : null}
              onPiPlan={effectiveAgent === "pi" ? handlePiPlan : undefined}
              onPiGoal={effectiveAgent === "pi" ? handlePiGoal : undefined}
              onPiPermission={effectiveAgent === "pi" ? handlePiPermissionMode : undefined}
              cwd={store.effectiveCwd ||
                folderCwdOverride ||
                getSavedProjectCwd(currentRealm) ||
                ""}
              authMode={store.authMode}
              platformId={store.platformId ?? "anthropic"}
              platformCredentials={settings?.platform_credentials ?? []}
              onSend={sendMessage}
              queuedMessages={store.queuedMessages}
              onQueueSend={handleQueueSend}
              onQueueSteer={handleQueueSteer}
              onQueueEdit={editQueuedMessage}
              onQueueDelete={(id) => store.deleteQueuedMessage(id)}
              onBtwSend={handleBtwSend}
              onAgentChange={handleAgentChange}
              onContinueConversation={store.run && !forkOverlay && !continuationBusy
                ? openEndContinuationMenu
                : undefined}
              onInterrupt={() => store.interrupt()}
              onModelSwitch={handleModelChange}
              onEffortChange={effortAvailable ? handleEffortChange : undefined}
              onPlanModeChange={effectiveAgent !== "pi" &&
              effectiveCapabilities.ui.planModeToggle &&
              effectiveCapabilities.protocol.planMode
                ? handlePlanModeChange
                : undefined}
              onPermissionModeChange={effectiveAgent !== "pi" &&
              effectiveCapabilities.ui.permissionModeSwitch
                ? handlePermissionModeChange
                : undefined}
              onVirtualCommand={handleVirtualCommand}
              fastModeState={store.fastModeState}
              onFastModeSwitch={handleFastModeSwitch}
              onPlatformChange={handlePlatformChange}
              {authOverview}
              authSourceLabel={store.authSourceLabel}
              authSourceCategory={store.authSourceCategory}
              apiKeySource={store.apiKeySource}
              onAuthModeChange={handleAuthModeChange}
              {localProxyStatuses}
              showAuthBadge={!welcomeVisible && store.useStreamSession}
              onShortcutHelp={() => (shortcutHelpOpen = !shortcutHelpOpen)}
              availableSkills={piAvailableSkillNames}
              {skillItems}
              codexSkillItems={codexRuntimeSkills}
              hasStash={!!stashedInput}
              {userHistory}
              runId={store.run?.id ?? ""}
              initialDraft={promptDraftsByRun.get(store.run?.id ?? "")}
              onDraftChange={savePromptDraft}
              onRestoreStash={() => {
                if (stashedInput) {
                  promptRef?.restoreSnapshot(stashedInput);
                  stashedInput = null;
                  showChatToast(t("toast_stashRestored"));
                  dbg("chat", "stash restored via badge click");
                }
              }}
            />
          {/key}

          <!-- DeepSeek Harness (DSH) StatsLine: Multi-dimensional session metrics -->
          <ChatSessionStatsLine turnUsages={store.turnUsages} timeline={filteredTimeline} />
        </div>
      {/if}

      <!-- Bottom Terminal Panel (Codex Cmd+J 1:1 matching drawer) -->
      {#if bottomPanelOpen}
        <div
          class="h-px w-full cursor-row-resize bg-border/60 hover:bg-primary/40 transition-colors {isResizingPanel
            ? 'bg-primary/40'
            : ''}"
          onmousedown={startPanelResize}
          title="拖动调节高度"
        ></div>
        <div
          class="w-full bg-[#09090b] flex flex-col shrink-0 overflow-hidden z-30"
          style="height: {bottomPanelHeight}px;"
        >
          <!-- Header: tab list + action buttons -->
          <div
            class="flex h-9 items-center justify-between border-b border-border/40 bg-background/80 px-3 text-xs font-mono select-none"
          >
            <div class="flex items-center gap-1 overflow-x-auto">
              {#each ptyTabs as tab (tab.id)}
                <div
                  class="group flex items-center gap-1.5 rounded-lg px-2 py-1 text-[11px] font-medium border transition-colors cursor-pointer {tab.id ===
                  activeTabId
                    ? 'bg-accent/80 text-foreground border-border/40'
                    : 'bg-transparent text-muted-foreground border-transparent hover:bg-accent/40'}"
                  onclick={() => (activeTabId = tab.id)}
                >
                  <svg
                    class="h-3 w-3 shrink-0"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    ><polyline points="4 17 10 11 4 5" /><line
                      x1="12"
                      y1="19"
                      x2="20"
                      y2="19"
                    /></svg
                  >
                  <span class="truncate max-w-[140px]">{tab.label}</span>
                  <button
                    class="ml-0.5 rounded p-0.5 text-muted-foreground/60 hover:text-red-400 hover:bg-red-500/10 transition-colors"
                    title="关闭终端"
                    onclick={(e) => {
                      e.stopPropagation();
                      closeTerminalTab(tab.id);
                    }}
                  >
                    <svg
                      class="h-3 w-3"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2.5"
                      ><line x1="18" y1="6" x2="6" y2="18" /><line
                        x1="6"
                        y1="6"
                        x2="18"
                        y2="18"
                      /></svg
                    >
                  </button>
                </div>
              {/each}

              <button
                class="ml-1 shrink-0 rounded-md p-1 text-muted-foreground hover:text-foreground hover:bg-accent/60 transition-colors"
                title="新建终端"
                onclick={createNewTerminal}
              >
                <svg
                  class="h-3.5 w-3.5"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  ><line x1="12" y1="5" x2="12" y2="19" /><line
                    x1="5"
                    y1="12"
                    x2="19"
                    y2="12"
                  /></svg
                >
              </button>
            </div>

            <button
              class="shrink-0 rounded-md p-1 text-muted-foreground hover:text-foreground hover:bg-accent/60 transition-colors"
              onclick={closeBottomPanel}
              title="关闭面板 (⌘J)"
            >
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                ><line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" /></svg
              >
            </button>
          </div>

          <!-- Terminal containers: one per tab, hidden when inactive -->
          {#each ptyTabs as tab (tab.id)}
            <div class="flex-1 overflow-hidden p-2" class:hidden={tab.id !== activeTabId}>
              <XTerminal
                bind:this={termRefs[tab.id]}
                onReady={(cols: number, rows: number) => handleTabReady(tab.id, cols, rows)}
                onResize={(cols: number, rows: number) => handleTabResize(tab.id, cols, rows)}
                onData={(data: string) => handleTabData(tab.id, data)}
                class="h-full w-full"
              />
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  </div>

  {#if turnReviewOpen && turnReviewDiff.trim()}
    <TurnReviewPanel diffText={turnReviewDiff} onClose={() => (turnReviewOpen = false)} />
  {/if}

  <!-- Code Mode Multi-Tab Workspace Aside -->
  <CodeAsideWorkspace
    open={codeAsideOpen}
    onClose={() => setCodeAsideOpen(false)}
    cwd={store.effectiveCwd || getProjectCwdForEditor()}
    runId={store.run?.id ?? ""}
    turnDiff={turnReviewDiff || effectiveTurnDiff}
    requestedTab={codeAsideRequestedTab}
    requestedFilePath={codeAsideRequestedPath}
  />

  <!-- Tool Activity sidebar -->
  <ToolActivity
    timeline={store.timeline}
    tools={store.tools}
    turnUsages={store.turnUsages}
    persistedFiles={store.persistedFiles}
    sessionInfo={currentSessionInfo}
    collapsed={sidebarCollapsed || turnReviewOpen || codeAsideOpen || isArchivedView}
    onToggle={toggleSidebar}
    onScrollToTool={scrollToTool}
    onScrollToTurn={(anchorId) => scrollToMessage(anchorId)}
    bind:requestedTab={sidebarRequestedTab}
    backgroundTasks={store.taskNotifications}
    activeBackgroundTasks={store.activeBackgroundTasks}
    cwd={store.effectiveCwd}
    runId={store.run?.id ?? ""}
    isRemote={store.isRemote}
    bind:requestedPreviewPath
  />

  <RewindModal
    bind:open={rewindModalOpen}
    runId={store.run?.id ?? ""}
    candidates={rewindCandidates}
    initialCandidate={rewindDirectTarget}
    onSuccess={(info) => {
      // Run-id debounce: discard if run changed while modal was open
      if (info.runId !== store.run?.id) return;
      rewindMarkers = [
        ...rewindMarkers,
        {
          id: uuid(),
          ts: new Date().toISOString(),
          targetContent: truncate(info.targetContent, 80),
          filesReverted: info.filesReverted,
        },
      ];
      if (info.degraded) {
        showChatToast(t("rewind_degradedToFull"));
      } else {
        showChatToast(t("toast_rewindSuccess"));
      }
      tick().then(() => {
        document
          .getElementById("rewind-marker-latest")
          ?.scrollIntoView({ behavior: "smooth", block: "center" });
      });
    }}
  />

  <CodexReviewModal bind:open={codexReviewPickerOpen} onSubmit={runCodexReview} />

  <!-- Codex turn diff: supplied-diff mode (no git fetch, no staged/unstaged tabs). -->
  <DiffModal bind:open={turnDiffOpen} title="Turn diff" diffText={effectiveTurnDiff} />

  <RewindCodexModal
    bind:open={codexRewindOpen}
    turns={codexRewindTurns}
    busy={codexRewindBusy}
    onConfirm={runCodexRewind}
  />

  {#if goalPanelAvailable}
    <GoalPanel
      bind:open={goalPanelOpen}
      runId={store.run?.id}
      goal={effectiveAgent === "pi" ? piGoalForPanel : store.goal}
      agent={effectiveAgent}
      onGoalChange={(g) => store.setGoal(g)}
      onSendPrompt={(p) => sendMessage(p, [])}
    />
  {/if}

  <PiSessionTreePanel
    bind:open={piTreeOpen}
    tree={store.piSessionTree}
    busy={piTreeActionBusy}
    onRefresh={() => store.loadPiSessionTree()}
    onFork={handlePiFork}
    onClone={handlePiClone}
  />

  <GitWorktreePanel
    bind:open={worktreeOpen}
    project={worktreeProject}
    {worktrees}
    busy={worktreeBusy}
    onRefresh={refreshWorktrees}
    onCreate={createWorktree}
    onRemove={removeWorktree}
  />

  {#if continuationMenu}
    <div
      class="fixed inset-0 z-[90] flex items-center justify-center bg-black/30 p-4 backdrop-blur-[2px] animate-fade-in"
      role="presentation"
      onclick={() => {
        if (!continuationBusy) continuationMenu = null;
      }}
    >
      <div
        class="w-full max-w-md overflow-hidden rounded-xl border border-border bg-background shadow-2xl animate-slide-up"
        role="dialog"
        aria-modal="true"
        aria-labelledby="continue-from-message-title"
        tabindex="-1"
        onclick={(event) => event.stopPropagation()}
        onkeydown={(event) => event.stopPropagation()}
      >
        <div class="flex items-center justify-between border-b border-border/70 px-5 py-4">
          <div class="flex min-w-0 items-center gap-2">
            {#if continuationMenu.step === "agent" && !continuationBusy}
              <button
                type="button"
                class="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                onclick={() => {
                  if (continuationMenu) {
                    continuationMenu = { entryId: continuationMenu.entryId, step: "location" };
                  }
                }}
                aria-label={t("chat_continueBack")}
              >
                <svg
                  class="h-4 w-4"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  aria-hidden="true"
                >
                  <path d="m15 18-6-6 6-6" />
                </svg>
              </button>
            {/if}
            <div class="min-w-0">
              <h2 id="continue-from-message-title" class="text-base font-semibold text-foreground">
                {continuationBusy
                  ? t("chat_continuationCreatingTitle")
                  : continuationMenu.step === "agent"
                    ? t("chat_continueChooseAgent")
                    : t("chat_continueInNewChat")}
              </h2>
              {#if continuationBusy}
                <p class="mt-0.5 text-xs text-muted-foreground">
                  {t("chat_continuationCreatingDescription")}
                </p>
              {:else if continuationMenu.step === "agent"}
                <p class="mt-0.5 text-xs text-muted-foreground">
                  {t("chat_continueChooseAgentDescription")}
                </p>
              {/if}
            </div>
          </div>
          <button
            class="rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors disabled:opacity-40"
            disabled={continuationBusy}
            onclick={() => (continuationMenu = null)}
            aria-label={t("common_close")}
          >
            <svg
              class="h-4 w-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M18 6 6 18" /><path d="m6 6 12 12" />
            </svg>
          </button>
        </div>
        {#if continuationBusy}
          <div class="flex flex-col items-center gap-3 px-5 py-10 text-center" aria-busy="true">
            <span
              class="flex h-10 w-10 items-center justify-center rounded-full bg-primary/10 text-primary"
            >
              <svg class="h-5 w-5 animate-spin" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <circle
                  class="opacity-25"
                  cx="12"
                  cy="12"
                  r="9"
                  stroke="currentColor"
                  stroke-width="3"
                />
                <path
                  class="opacity-90"
                  d="M21 12a9 9 0 0 0-9-9"
                  stroke="currentColor"
                  stroke-width="3"
                  stroke-linecap="round"
                />
              </svg>
            </span>
            <p class="text-sm text-muted-foreground">{t("chat_continuationCreatingDescription")}</p>
          </div>
        {:else if continuationMenu.step === "location"}
          <div class="space-y-2 p-3">
            <button
              class="flex min-h-16 w-full items-center gap-3 rounded-lg px-3 py-3 text-left transition-colors hover:bg-muted/70 disabled:cursor-wait disabled:opacity-50"
              disabled={continuationBusy}
              onclick={() => void selectContinuationLocation(false)}
            >
              <span
                class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary"
              >
                <svg
                  class="h-4 w-4"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <path d="M4 5h16" /><path d="M4 12h16" /><path d="M4 19h16" />
                </svg>
              </span>
              <span class="min-w-0">
                <span class="block text-sm font-medium text-foreground">
                  {t("chat_continueInWorkspace")}
                </span>
                <span class="mt-0.5 block text-xs text-muted-foreground">
                  {continuationMenu.entryId === null
                    ? t("chat_continueCurrentWorkspaceDescription")
                    : t("chat_continueInWorkspaceDescription")}
                </span>
              </span>
            </button>

            <button
              class="flex min-h-16 w-full items-center gap-3 rounded-lg px-3 py-3 text-left transition-colors hover:bg-muted/70 disabled:cursor-not-allowed disabled:opacity-45"
              disabled={continuationBusy || store.isRemote}
              onclick={() => void selectContinuationLocation(true)}
            >
              <span
                class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary"
              >
                <svg
                  class="h-4 w-4"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <path d="M6 3v12" /><path d="M6 9h8a4 4 0 0 1 4 4v8" /><circle
                    cx="6"
                    cy="3"
                    r="2"
                  /><circle cx="6" cy="15" r="2" /><circle cx="18" cy="21" r="2" />
                </svg>
              </span>
              <span class="min-w-0">
                <span class="block text-sm font-medium text-foreground">
                  {t("chat_continueInNewWorktree")}
                </span>
                <span class="mt-0.5 block text-xs text-muted-foreground">
                  {store.isRemote
                    ? t("chat_continueInNewWorktreeRemoteUnsupported")
                    : continuationMenu.entryId === null
                      ? t("chat_continueCurrentWorktreeDescription")
                      : t("chat_continueInNewWorktreeDescription")}
                </span>
              </span>
            </button>
          </div>
        {:else}
          <div class="space-y-2 p-3">
            {#each getContinuationAgents() as targetAgent}
              <button
                class="flex min-h-16 w-full items-center gap-3 rounded-lg px-3 py-3 text-left transition-colors hover:bg-muted/70 disabled:cursor-wait disabled:opacity-50"
                disabled={continuationBusy}
                onclick={() => void startContinuationWithAgent(targetAgent)}
              >
                <span
                  class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-sm font-semibold text-primary"
                >
                  {targetAgent === "pi"
                    ? "P"
                    : targetAgent === "dsh"
                      ? "D"
                      : targetAgent === "claude"
                        ? "C"
                        : targetAgent === "codex"
                          ? "X"
                          : "G"}
                </span>
                <span class="min-w-0">
                  <span class="block text-sm font-medium text-foreground">
                    {t("chat_newAgentWithHistory", {
                      agent: getAgentDisplayName(targetAgent),
                    })}
                  </span>
                  <span class="mt-0.5 block text-xs text-muted-foreground">
                    {t("chat_carryHistory")}
                  </span>
                </span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <ShortcutHelpPanel bind:open={shortcutHelpOpen} />

  <FolderPicker
    bind:open={folderPickerOpen}
    initialHost={folderPickerInitialHost}
    initialPath={folderPickerInitialPath}
    hideTargetSelector={folderPickerHideTarget}
    onConfirm={(result) => {
      const fn = folderPickerResolve;
      folderPickerResolve = null;
      fn?.(result);
    }}
    onCancel={() => {
      const fn = folderPickerResolve;
      folderPickerResolve = null;
      fn?.(null);
    }}
  />

  <CapabilityRunInspector
    runId={store.run?.id || ""}
    capabilities={runEffectiveCapabilities}
    loading={loadingRunCapabilities}
    open={capabilityInspectorOpen}
    onClose={() => (capabilityInspectorOpen = false)}
  />
</div>
