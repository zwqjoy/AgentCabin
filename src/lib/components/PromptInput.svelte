<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { goto } from "$app/navigation";
  import type {
    Attachment,
    AgentSessionMode,
    AuthOverview,
    CliCommand,
    CliModelInfo,
    AtMentionEntry,
    PlatformCredential,
    PiGoalState,
    PiPermissionState,
    PiPlanState,
    QueuedMessage,
    PromptTemplate,
  } from "$lib/types";
  import type { EffectiveAgentCapabilities } from "$lib/utils/agent-capabilities";
  import * as api from "$lib/api";
  import { createGitBranchPoller } from "$lib/utils/git-branch";
  import AgentSelector from "./AgentSelector.svelte";
  import AuthSourceBadge from "./AuthSourceBadge.svelte";
  import FileAttachment from "./FileAttachment.svelte";
  import SlashMenu from "./SlashMenu.svelte";
  import CompactModelPicker from "./CompactModelPicker.svelte";
  import WorkBuddyCascadingMenu, { type SelectedExpert } from "./WorkBuddyCascadingMenu.svelte";
  import { dshExpertSkillName, isExpertSkill } from "$lib/utils/expert-context";
  import AtMentionMenu from "./AtMentionMenu.svelte";
  import {
    filterSlashCommands,
    filterNativeSlashCommands,
    parseVirtualAction,
    resolveVirtualCommand,
    getCommandInteraction,
    getArgumentHint,
    shouldBackFromSubView,
    isSubViewInputValid,
    getQuickActions,
    classifyCloseReason,
    groupSlashCommands,
    extractSlashQuery,
    getPiSkillCommandName,
    isSkillCommand,
  } from "$lib/utils/slash-commands";
  import type { SlashCommandGroups } from "$lib/utils/slash-commands";
  import type { MessageKey } from "$lib/i18n/types";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { IS_MAC } from "$lib/utils/platform";
  import { t } from "$lib/i18n/index.svelte";
  import { filterPiThinkingLevelsForUi, PI_THINKING_LEVEL_OPTIONS } from "$lib/utils/pi-thinking";
  import { AGENT_ORDER, getAgentDisplayName } from "$lib/utils/agent-metadata";
  import { composerSlashEnabled } from "$lib/utils/composer-features";
  import { formatPasteSize } from "$lib/utils/format";
  let queueSteerInFlight = $state<string | null>(null);
  import {
    buildPasteToken,
    singlePasteTokenRe,
    insertAtSelection,
    parsePasteTokenSeqs,
    expandPasteTokens,
  } from "$lib/utils/paste-tokens";
  import {
    BINARY_ATTACHMENT_TYPES,
    MAX_ATTACHMENTS,
    MAX_PASTE_BLOCKS,
    PDF_MAX_BINARY_SIZE,
    PDF_MAX_PATH_SIZE,
    isTextFile,
    isPdf,
    isOfficeFile,
    isSpreadsheetExt,
    getFileExtension,
    classifyByMime,
    getFileSizeLimit,
    getSizeLimitByMime,
  } from "$lib/utils/file-types";
  import { uuid } from "$lib/utils/uuid";
  import type { ClipboardFileInfo } from "$lib/api";
  import type { PromptInputSnapshot } from "$lib/types";
  import {
    type HistoryState,
    type HistoryAction,
    createHistoryState,
    checkAndReset,
    resetHistory,
    shouldIntercept,
    getHistoryAction,
    hasMultipleVisualLines,
  } from "$lib/utils/input-history";

  let {
    agent = "pi",
    enabledAgents = ["pi"],
    capabilities,
    planModeActive = false,
    sessionModes = [],
    sessionMode = "",
    disabled = false,
    hasRun = false,
    running = false,
    sessionAlive = false,
    canResume = false,
    useStreamSession = false,
    isRemote = false,
    cliCommands = [],
    models = [],
    currentModel = "",
    projectDefaultModel = "",
    projectDefaultLabel,
    setProjectDefaultLabel,
    currentEffort = "",
    permissionMode = "",
    onOpenGoal,
    goalActive = false,
    goalAvailable = false,
    planAvailable = false,
    piFeatureDiscoveryPending = false,
    piFeatureColdStartAvailable = false,
    piFeatureStartupPending = false,
    onClearGoal,
    onClearPlan,
    piPlan = null,
    piGoal = null,
    piPermission = null,
    piPermissionBusy = false,
    piFeaturePending = null,
    onPiPlan,
    onPiGoal,
    onPiPermission,
    onPiTree,
    permissionPicker = null,
    onSend,
    onQueueSend,
    queueAvailable = true,
    queuedMessages = [],
    onQueueSteer,
    queueActionAvailable = true,
    onQueueEdit,
    onQueueDelete,
    onAgentChange,
    onContinueConversation,
    onInterrupt,
    onCancelTurn,
    interruptDisabled = false,
    cancelTurnDisabled = false,
    onModelSwitch,
    onSetProjectDefault,
    onEffortChange,
    onPermissionModeChange,
    onPlanModeChange,
    onSessionModeChange,
    onVirtualCommand,
    fastModeState = "",
    onFastModeSwitch,
    cwd = "/",
    authMode = "cli",
    platformId = "anthropic",
    platformCredentials = [],
    onPlatformChange,
    authOverview = null,
    authSourceLabel = "",
    authSourceCategory = "unknown",
    apiKeySource = "",
    onAuthModeChange,
    localProxyStatuses = {} as Record<string, { running: boolean; needsAuth: boolean }>,
    availableSkills = [],
    skillItems = [],
    codexSkillItems = [],
    skillSelectorEmptyMessage = "",
    showAuthBadge = true,
    pendingPermission = false,
    hasStash = false,
    onBtwSend,
    onRestoreStash,
    onShortcutHelp,
    userHistory = [] as string[],
    runId = "",
    initialDraft,
    onDraftChange,
    customPlaceholder = "",
    projectPicker = null,
    workspacePicker = null,
    sessionInfo = null,
    harness = "code",
    selectedExpert = $bindable(null),
    onExpertClear,
  }: {
    harness?: "code" | "work";
    sessionInfo?: import("$lib/types").SessionInfoData | null;
    customPlaceholder?: string;
    projectPicker?: {
      currentProjectCwd?: string | null;
      currentProjectName?: string;
      currentStandaloneTask?: boolean;
      projects: Array<{ cwd: string; name: string }>;
      onSelect: (cwd: string | null) => void;
      /** 只读模式：当前对话已经绑定项目目录，不允许中途切换。 */
      readOnly?: boolean;
    } | null;
    workspacePicker?: {
      currentWorkspaceId?: string | null;
      currentWorkspaceName?: string;
      workspaces: Array<{ id: string; name: string }>;
      onSelect: (workspaceId: string | null) => void;
      onCreateWorkspace?: () => void;
      onOpenFolderWorkspace?: () => void;
      /** 只读模式：仅显示当前工作空间，不允许切换（已开始的对话数据归属不可变更）。 */
      readOnly?: boolean;
    } | null;
    agent?: string;
    enabledAgents?: string[];
    capabilities: EffectiveAgentCapabilities;
    planModeActive?: boolean;
    sessionModes?: AgentSessionMode[];
    sessionMode?: string;
    disabled?: boolean;
    hasRun?: boolean;
    running?: boolean;
    sessionAlive?: boolean;
    canResume?: boolean;
    useStreamSession?: boolean;
    isRemote?: boolean;
    cliCommands?: CliCommand[];
    models?: CliModelInfo[];
    currentModel?: string;
    projectDefaultModel?: string;
    projectDefaultLabel?: string;
    setProjectDefaultLabel?: string;
    currentEffort?: string;
    permissionMode?: string;
    onOpenGoal?: () => void;
    goalActive?: boolean;
    goalAvailable?: boolean;
    planAvailable?: boolean;
    piFeatureDiscoveryPending?: boolean;
    /** Allow Pi Goal/Plan clicks to bootstrap a shell before runtime discovery. */
    piFeatureColdStartAvailable?: boolean;
    /** True while a no-message Pi shell is being created for a feature action. */
    piFeatureStartupPending?: boolean;
    onClearGoal?: () => void | Promise<void>;
    onClearPlan?: () => void | Promise<void>;
    piPlan?: PiPlanState | null;
    piGoal?: PiGoalState | null;
    piPermission?: PiPermissionState | null;
    piPermissionBusy?: boolean;
    piFeaturePending?:
      | "plan_entering"
      | "plan_exiting"
      | "goal_starting"
      | "goal_pausing"
      | "goal_resuming"
      | "goal_clearing"
      | null;
    onPiPlan?: (action: "enter" | "exit") => void | Promise<void>;
    onPiGoal?: () => void;
    onPiPermission?: (mode: PiPermissionState["mode"]) => void | Promise<void>;
    onPiTree?: () => void;
    /**
     * Generic permission-mode picker rendered in the composer toolbar (same
     * slot/style as the Pi permission button). Lets surfaces like Work mode
     * expose their own execution-mode selector without reusing piPermission.
     */
    permissionPicker?: {
      value: string;
      busy?: boolean;
      title?: string;
      options: Array<{
        value: string;
        label: string;
        description?: string;
        disabled?: boolean;
        /** Text colour classes for the option row. */
        cls?: string;
      }>;
      onSelect: (value: string) => void;
    } | null;
    onSend: (
      text: string,
      attachments: Attachment[],
      skills?: { name: string; path: string }[],
    ) => void;
    onQueueSend?: (text: string, attachments: Attachment[]) => void;
    /** Whether a running session can accept a delayed follow-up turn. */
    queueAvailable?: boolean;
    queuedMessages?: QueuedMessage[];
    onQueueSteer?: (id: string) => void | Promise<void>;
    /** Whether queued rows can invoke the agent's immediate steer action. */
    queueActionAvailable?: boolean;
    onQueueEdit?: (id: string) => void;
    onQueueDelete?: (id: string) => void;
    onAgentChange?: (agent: string) => void;
    onContinueConversation?: () => void;
    onInterrupt?: () => void;
    /** Cancel the active turn while keeping the provider session alive. */
    onCancelTurn?: () => void;
    /** Keep the active-run stop control usable while other input is busy. */
    interruptDisabled?: boolean;
    cancelTurnDisabled?: boolean;
    onModelSwitch?: (model: string) => void;
    onSetProjectDefault?: () => void | Promise<void>;
    onEffortChange?: (effort: string) => void;
    onPermissionModeChange?: (mode: string) => void;
    onPlanModeChange?: (active: boolean) => void | Promise<void>;
    onSessionModeChange?: (mode: string) => void | Promise<void>;
    onVirtualCommand?: (action: string, args: string) => void;
    fastModeState?: string;
    onFastModeSwitch?: (mode: "on" | "off") => void;
    cwd?: string;
    authMode?: string;
    platformId?: string;
    platformCredentials?: PlatformCredential[];
    onPlatformChange?: (platformId: string) => void;
    authOverview?: AuthOverview | null;
    authSourceLabel?: string;
    authSourceCategory?: string;
    apiKeySource?: string;
    onAuthModeChange?: (mode: string) => void;
    localProxyStatuses?: Record<string, { running: boolean; needsAuth: boolean }>;
    availableSkills?: string[];
    skillItems?: { name: string; description: string }[];
    skillSelectorEmptyMessage?: string;
    // Live Codex runtime skills (name + path + description). `path` lets us send a picked skill
    // as a structured {type:"skill"} UserInput. Non-empty only for a live Codex session.
    codexSkillItems?: { name: string; path: string; description: string }[];
    showAuthBadge?: boolean; // TODO: remove unused auth props after hero migration
    pendingPermission?: boolean;
    hasStash?: boolean;
    onBtwSend?: (question: string) => void;
    onRestoreStash?: () => void;
    onShortcutHelp?: () => void;
    userHistory?: string[];
    runId?: string;
    /** Draft to seed the prompt with on mount (per-run persistence — issue #156). */
    initialDraft?: PromptInputSnapshot;
    /** Fired whenever the draft (text/attachments/paste blocks/path refs) changes, so the
     *  parent can persist it keyed by run. */
    onDraftChange?: (snapshot: PromptInputSnapshot) => void;
    selectedExpert?: SelectedExpert | null;
    onExpertClear?: () => void;
  } = $props();

  const queueActionLabel = $derived(agent === "claude" ? "中断并发送" : "引导");
  const queueActionProgressLabel = $derived(agent === "claude" ? "中断中…" : "引导中…");
  const agentDisplayName = $derived(getAgentDisplayName(agent));
  const btwAvailable = $derived(!!onBtwSend && agent !== "pi" && agent !== "dsh");

  // ── BTW mode (side question) ──
  let btwMode = $state(false);

  // Auto-close BTW mode when agent stops running
  $effect(() => {
    if (!running || !btwAvailable) btwMode = false;
  });

  let effectivePlaceholder = $derived(
    btwMode
      ? "Ask a side question..."
      : pendingPermission
        ? t("prompt_pendingPermission")
        : // Inactive but resumable: tell the user they can just type
          canResume && !sessionAlive && !running
          ? t("prompt_continueSessionPlaceholder")
          : customPlaceholder.trim()
            ? customPlaceholder
            : harness === "work"
              ? hasRun
                ? t("prompt_hasRunPlaceholderWork")
                : t("prompt_newPlaceholderWork")
              : hasRun
                ? t("prompt_hasRunPlaceholderCode")
                : t("prompt_newPlaceholderCode"),
  );

  // ── Git branch (fetched from cwd) ──
  const branchPoller = createGitBranchPoller(api.getGitBranch);
  let gitBranch = $state("");

  // Fetch on cwd / isRemote change
  $effect(() => {
    void cwd;
    void isRemote;
    const effectiveCwd = isRemote ? "" : cwd;
    branchPoller.refresh(effectiveCwd).then((b) => {
      gitBranch = b;
    });
  });

  // Poll every 10s to catch branch changes made by CLI commands
  $effect(() => {
    if (isRemote) return;
    const interval = setInterval(() => {
      branchPoller.refresh(cwd).then((b) => {
        gitBranch = b;
      });
    }, 10_000);
    return () => clearInterval(interval);
  });

  // ── Permission mode selector ──
  // Plan mode is handled by the dedicated Plan button, not the dropdown.
  // auto/dontAsk are removed: Claude CLI doesn't support them, Codex maps them
  // to existing modes (bypassPermissions/default). Only show what each agent
  // actually supports.
  const PERMISSION_MODES = [
    {
      value: "default",
      label: () => t("prompt_permAskLabel"),
      shortLabel: () => t("prompt_permAskShort"),
      description: () => t("prompt_permAskDesc"),
      cls: "text-muted-foreground hover:text-foreground",
      dotCls: "bg-muted-foreground/40",
      borderCls: "",
    },
    {
      value: "acceptEdits",
      label: () => t("prompt_permAutoReadLabel"),
      shortLabel: () => t("prompt_permAutoReadShort"),
      description: () => t("prompt_permAutoReadDesc"),
      cls: "text-muted-foreground hover:text-foreground",
      dotCls: "bg-muted-foreground/40",
      borderCls: "",
    },
    {
      value: "bypassPermissions",
      label: () => t("prompt_permAutoAllLabel"),
      shortLabel: () => t("prompt_permAutoAllShort"),
      description: () => t("prompt_permAutoAllDesc"),
      cls: "text-muted-foreground hover:text-foreground",
      dotCls: "bg-muted-foreground/40",
      borderCls: "",
    },
  ];

  const PI_PERMISSION_MODES: Array<{
    value: PiPermissionState["mode"];
    label: () => string;
    shortLabel: () => string;
    description: () => string;
    cls: string;
  }> = [
    {
      value: "guarded",
      label: () => t("prompt_permAskLabel"),
      shortLabel: () => t("prompt_permAskShort"),
      description: () => t("prompt_piPermAskDesc"),
      cls: "text-muted-foreground hover:text-foreground",
    },
    {
      value: "accept_edits",
      label: () => t("prompt_permAutoReadLabel"),
      shortLabel: () => t("prompt_permAutoReadShort"),
      description: () => t("prompt_piPermEditDesc"),
      cls: "text-muted-foreground hover:text-foreground",
    },
    {
      value: "auto_approve",
      label: () => t("prompt_piPermAutoLabel"),
      shortLabel: () => t("prompt_piPermAutoShort"),
      description: () => t("prompt_piPermBypassDesc"),
      cls: "text-amber-500",
    },
  ];

  let modeDropdownOpen = $state(false);
  let permissionBtnEl: HTMLButtonElement | undefined = $state();
  let modeDropdownEl: HTMLDivElement | undefined = $state();
  let modeDropdownStyle = $state("");

  // ── Project picker (Code mode) ────────────────────────────────────────────
  let projectDropdownOpen = $state(false);
  let projectPickerBtnEl: HTMLButtonElement | undefined = $state();
  let projectDropdownEl: HTMLDivElement | undefined = $state();
  let projectDropdownStyle = $state("");

  $effect(() => {
    if (!projectPicker || projectPicker.readOnly) {
      projectDropdownOpen = false;
    }
  });

  function toggleProjectDropdown() {
    if (projectPicker?.readOnly) return;
    if (projectDropdownOpen) {
      projectDropdownOpen = false;
      return;
    }
    if (slashMenuOpen) closeSlashMenu("project-open");
    if (atMenuOpen) closeAtMenu("project-open");
    if (modeDropdownOpen) modeDropdownOpen = false;
    if (wsDropdownOpen) wsDropdownOpen = false;

    projectDropdownOpen = true;
    if (projectPickerBtnEl) {
      const rect = projectPickerBtnEl.getBoundingClientRect();
      const openUpward = rect.bottom + 240 > window.innerHeight;
      if (openUpward) {
        projectDropdownStyle = `position:fixed; bottom:${window.innerHeight - rect.top + 4}px; left:${rect.left}px; z-index:60;`;
      } else {
        projectDropdownStyle = `position:fixed; top:${rect.bottom + 4}px; left:${rect.left}px; z-index:60;`;
      }
    }
  }

  function selectProject(cwd: string | null) {
    projectDropdownOpen = false;
    projectPicker?.onSelect(cwd);
  }

  // ── Workspace picker (Work mode) ──────────────────────────────────────────
  let wsDropdownOpen = $state(false);
  let wsPickerBtnEl: HTMLButtonElement | undefined = $state();
  let wsDropdownEl: HTMLDivElement | undefined = $state();
  let wsDropdownStyle = $state("");

  function toggleWsDropdown() {
    if (wsDropdownOpen) {
      wsDropdownOpen = false;
      return;
    }
    if (slashMenuOpen) closeSlashMenu("ws-open");
    if (atMenuOpen) closeAtMenu("ws-open");
    if (modeDropdownOpen) modeDropdownOpen = false;
    if (projectDropdownOpen) projectDropdownOpen = false;

    wsDropdownOpen = true;
    if (wsPickerBtnEl) {
      const rect = wsPickerBtnEl.getBoundingClientRect();
      const openUpward = rect.bottom + 240 > window.innerHeight;
      if (openUpward) {
        wsDropdownStyle = `position:fixed; bottom:${window.innerHeight - rect.top + 4}px; left:${rect.left}px; z-index:60;`;
      } else {
        wsDropdownStyle = `position:fixed; top:${rect.bottom + 4}px; left:${rect.left}px; z-index:60;`;
      }
    }
  }

  // Agent-native plan state is separate from permission policy.
  let planActive = $derived(planModeActive);
  let featurePlanActive = $derived(agent === "pi" ? piPlan?.phase === "active" : planModeActive);
  let featureGoalActive = $derived(
    agent === "pi"
      ? ["active", "paused", "budget_limited", "complete"].includes(piGoal?.phase ?? "")
      : goalActive,
  );
  interface PermissionSelectorOption {
    value: string;
    label: string;
    shortLabel?: string;
    description: string;
    cls: string;
    disabled?: boolean;
  }

  let permissionSelectorAvailable = $derived(
    !!permissionPicker || (agent === "pi" && !!piPermission) || !!onPermissionModeChange,
  );
  let permissionSelectorOptions = $derived.by((): PermissionSelectorOption[] => {
    if (permissionPicker) {
      return permissionPicker.options.map((option) => ({
        value: option.value,
        label: option.label,
        description: option.description ?? "",
        cls: option.cls ?? "",
        disabled: option.disabled,
      }));
    }
    if (agent === "pi" && piPermission) {
      return PI_PERMISSION_MODES.map((mode) => ({
        value: mode.value,
        label: mode.label(),
        shortLabel: mode.shortLabel(),
        description: mode.description(),
        cls: mode.cls,
      }));
    }
    if (onPermissionModeChange) {
      return PERMISSION_MODES.map((mode) => ({
        value: mode.value,
        label: mode.label(),
        shortLabel: mode.shortLabel(),
        description: mode.description(),
        cls: mode.cls,
      }));
    }
    return [];
  });
  let permissionSelectorValue = $derived(
    permissionPicker?.value ??
      (agent === "pi" && piPermission ? piPermission.mode : permissionMode),
  );
  let permissionSelectorActiveOption = $derived(
    permissionSelectorOptions.find((option) => option.value === permissionSelectorValue),
  );
  let permissionSelectorBusy = $derived(
    permissionPicker
      ? !!permissionPicker.busy
      : agent === "pi" && piPermission
        ? piPermissionBusy
        : false,
  );
  let permissionSelectorDisabled = $derived(
    disabled ||
      permissionSelectorBusy ||
      (!permissionPicker && !(agent === "pi" && piPermission) && planActive),
  );
  let permissionSelectorLabel = $derived(
    permissionSelectorActiveOption?.shortLabel ??
      permissionSelectorActiveOption?.label ??
      permissionSelectorValue,
  );
  let permissionSelectorTitle = $derived.by(() => {
    if (permissionPicker?.title) return permissionPicker.title;
    if (agent === "pi" && piPermission) {
      return permissionSelectorBusy
        ? t("prompt_permissionModeSwitchingTitle")
        : t("prompt_piPermissionModeSelectTitle");
    }
    if (planActive) return t("prompt_permissionPlanActiveTitle");
    return t("prompt_permissionModeTitle", {
      mode: permissionSelectorActiveOption?.label ?? permissionSelectorValue,
    });
  });

  $effect(() => {
    if ((!permissionSelectorAvailable || permissionSelectorDisabled) && modeDropdownOpen) {
      modeDropdownOpen = false;
    }
  });
  let nativeModeSelector = $derived(
    capabilities.protocol.sessionModeControl &&
      sessionModes.length > 1 &&
      !(
        sessionModes.length === 2 &&
        sessionModes.some((mode) => mode.id === "default") &&
        sessionModes.some((mode) => mode.id === "plan")
      ) &&
      !!onSessionModeChange,
  );
  // Before the first session_init, a session actor may widen this capability
  // from live ACP negotiation. Keep initial attachments available so the
  // backend can make the authoritative accept/reject decision.
  let attachmentsAvailable = $derived(
    capabilities.runtime.attachments || (!hasRun && capabilities.execution.sessionActor),
  );

  // ── Shared permission selector (Code and Work) ───────────────────────────
  function togglePermissionDropdown() {
    if (permissionSelectorDisabled) return;
    if (modeDropdownOpen) {
      modeDropdownOpen = false;
      return;
    }
    if (slashMenuOpen) closeSlashMenu("permission-open");
    if (atMenuOpen) closeAtMenu("permission-open");
    if (wsDropdownOpen) wsDropdownOpen = false;
    if (projectDropdownOpen) projectDropdownOpen = false;

    modeDropdownOpen = true;
    if (permissionBtnEl) {
      const rect = permissionBtnEl.getBoundingClientRect();
      const openUpward = rect.bottom + 220 > window.innerHeight;
      if (openUpward) {
        modeDropdownStyle = `position:fixed; bottom:${window.innerHeight - rect.top + 4}px; left:${rect.left}px; z-index:50;`;
      } else {
        modeDropdownStyle = `position:fixed; top:${rect.bottom + 4}px; left:${rect.left}px; z-index:50;`;
      }
    }
  }

  function selectPermissionOption(value: string) {
    const option = permissionSelectorOptions.find((candidate) => candidate.value === value);
    if (!option || option.disabled || permissionSelectorDisabled) return;

    modeDropdownOpen = false;
    if (permissionPicker) {
      permissionPicker.onSelect(value);
      return;
    }
    if (agent === "pi" && piPermission) {
      void onPiPermission?.(value as PiPermissionState["mode"]);
      return;
    }
    onPermissionModeChange?.(value);
  }

  interface PastedBlock {
    id: string;
    text: string;
    lineCount: number;
    charCount: number;
    preview: string;
    ext?: string;
    /** Display sequence for the inline token label. Set only for blocks anchored by a token
     *  (clipboard paste); undefined for drag-dropped text files, which append at send instead. */
    seq?: number;
  }

  interface PathRef {
    id: string;
    name: string;
    path: string;
    isDir: boolean;
  }

  // Seed from initialDraft so a per-run draft survives the unmount/remount the chat page forces
  // on run switch (issue #156). Read untracked — this is a one-time construction seed (the parent
  // remounts per run via {#key}); onDraftChange keeps the draft current after.
  const seed = untrack(() => initialDraft);
  let inputText = $state(seed?.text ?? "");
  let pendingAttachments = $state<
    Array<{
      id: string;
      name: string;
      type: string;
      size: number;
      contentBase64?: string;
      /** Filesystem path for >20MB clipboard PDFs (path-reference mode). */
      filePath?: string;
    }>
  >(seed?.attachments ?? []);
  let pastedBlocks = $state<PastedBlock[]>((seed?.pastedBlocks as PastedBlock[]) ?? []);
  let pendingPathRefs = $state<PathRef[]>(seed?.pathRefs ?? []);
  /** Codex skills the user picked for THIS message (sent as structured {type:"skill"} refs, not
   *  text). Rendered as removable chips; cleared on send. Codex-only — Claude uses slash text. */
  let pendingSkills = $state<{ name: string; path: string }[]>([]);

  /**
   * Prompt templates are stored by AgentCabin and are shared by Claude, Codex, and Pi.
   * The textarea keeps a readable `/name` token; the full prompt is expanded at send time so
   * the template remains at the exact position where the user inserted it.
   */
  interface PromptTemplateToken {
    token: string;
    name: string;
    content: string;
  }
  let promptTemplateTokens = $state<PromptTemplateToken[]>(seed?.promptTemplateTokens ?? []);
  let promptTemplateSelection = $state({ start: 0, end: 0 });

  let fileInput: HTMLInputElement | undefined = $state();
  let textareaEl: HTMLTextAreaElement | undefined = $state();
  let cascadingMenuOpen = $state(false);

  function insertAtMentionChar() {
    if (!textareaEl) return;
    textareaEl.focus();
    const pos = textareaEl.selectionStart ?? inputText.length;
    inputText = inputText.slice(0, pos) + "@" + inputText.slice(pos);
    requestAnimationFrame(() => {
      if (!textareaEl) return;
      textareaEl.focus();
      const newPos = pos + 1;
      textareaEl.setSelectionRange(newPos, newPos);
      handleInput();
    });
  }

  let lastEscTime = 0;
  let histState: HistoryState = createHistoryState();

  $effect(() => {
    if (checkAndReset(histState, userHistory.length, runId)) {
      dbg("prompt-history", "reset", { runId, len: userHistory.length });
    }
  });

  // Prune token-anchored paste chips whose token the user deleted from the textarea.
  // Only blocks with a seq carry a token; token-less drag-dropped blocks are left untouched.
  $effect(() => {
    const present = new Set(parsePasteTokenSeqs(inputText));
    const next = pastedBlocks.filter((b) => b.seq == null || present.has(b.seq));
    if (next.length !== pastedBlocks.length) pastedBlocks = next;
  });

  // Report draft changes upward so the parent can persist them per-run (issue #156). Reading the
  // snapshot tracks all four state arrays, so this re-fires on any edit/paste/attachment change.
  $effect(() => {
    onDraftChange?.(getInputSnapshot());
  });

  /** Chunked ArrayBuffer→base64 (32KB chunks — safe for large files, avoids stack overflow). */
  function arrayBufferToBase64(buffer: ArrayBuffer): string {
    const bytes = new Uint8Array(buffer);
    const CHUNK = 0x8000;
    let binary = "";
    for (let i = 0; i < bytes.length; i += CHUNK) {
      const slice = bytes.subarray(i, Math.min(i + CHUNK, bytes.length));
      binary += String.fromCharCode.apply(null, slice as unknown as number[]);
    }
    return btoa(binary);
  }

  // ── File toast ──
  let toastMessage = $state<string | null>(null);
  let toastVariant = $state<"error" | "info">("error");
  let toastTimeout: ReturnType<typeof setTimeout> | null = null;
  function showFileToast(msg: string, variant: "error" | "info" = "error") {
    toastMessage = msg;
    toastVariant = variant;
    if (toastTimeout) clearTimeout(toastTimeout);
    toastTimeout = setTimeout(() => {
      toastMessage = null;
    }, 3500);
  }

  // ── Slash menu state ──
  let slashMenuOpen = $state(false);
  let slashSelectedIndex = $state(0);
  let slashPhase: "commands" | "sub-model" | "sub-fast" = $state("commands");
  let slashSubSelectedIndex = $state(0);
  let activeSlashCmd: CliCommand | null = $state(null);

  // Skill picker source: Codex draws from the runtime list (name + path); before a session,
  // every provider draws from the global capability list loaded by the Capability Center.
  // Skills stay in the dedicated selector, not the native slash-command catalog.
  let selectorSkills = $derived(
    (agent === "codex"
      ? codexSkillItems.map((s) => ({ name: s.name, description: s.description }))
      : skillItems
    ).filter((s) => !isExpertSkill(s)),
  );
  let slashSkillNames = $derived(
    new Set([...availableSkills, ...selectorSkills.map((skill) => skill.name)]),
  );

  let nativeCommands = $derived(filterNativeSlashCommands(cliCommands ?? [], slashSkillNames));
  let slashEnabled = $derived(
    composerSlashEnabled(
      agent,
      capabilities.protocol.slashCommands,
      capabilities.ui.slashCommandMenu,
      hasRun,
    ),
  );
  let savedInputForSlash = $state("");

  /** Remove stale template tokens when the user deletes their readable token from the textarea. */
  $effect(() => {
    if (promptTemplateTokens.length === 0) return;
    let remaining = inputText;
    const kept: PromptTemplateToken[] = [];
    for (const template of promptTemplateTokens) {
      const index = remaining.indexOf(template.token);
      if (index < 0) continue;
      kept.push(template);
      remaining = remaining.slice(0, index) + remaining.slice(index + template.token.length);
    }
    if (kept.length !== promptTemplateTokens.length) promptTemplateTokens = kept;
  });

  // ── Chinese IME support ──
  let isComposing = $state(false);

  let allCommands = $derived(slashEnabled ? nativeCommands : []);
  let quickActions = $derived(getQuickActions(allCommands, agent));
  let skillNameSet = $derived(slashSkillNames);

  let slashQuery = $derived.by(() => {
    if (!slashMenuOpen || slashPhase !== "commands") return null;
    return extractSlashQuery(inputText) ?? "";
  });

  let filteredCommands = $derived.by(() => {
    if (slashQuery === null) return [];
    return filterSlashCommands(allCommands, slashQuery);
  });

  let slashGroups = $derived.by((): SlashCommandGroups | null => {
    if (slashQuery !== "") return null; // non-empty query or menu closed → flat mode
    if (filteredCommands.length === 0) return null;
    return groupSlashCommands(filteredCommands, skillNameSet);
  });

  let effectiveCommands = $derived(slashGroups ? slashGroups.flatOrder : filteredCommands);

  let hintText = $derived.by(() => {
    if (slashPhase !== "commands" || effectiveCommands.length === 0) return "";
    const idx = Math.min(slashSelectedIndex, effectiveCommands.length - 1);
    return getArgumentHint(effectiveCommands[idx]);
  });

  $effect(() => {
    if (slashMenuOpen)
      dbg("slash", slashGroups ? "grouped" : "flat", { count: effectiveCommands.length });
  });

  // ── @-mention state ──
  let atMenuOpen = $state(false);
  let atQuery = $state("");
  let atResults = $state<AtMentionEntry[]>([]);
  let atSelectedIndex = $state(0);
  let atLoading = $state(false);
  /** Position in inputText where the `@` trigger starts. */
  let atStartPos = $state(-1);
  let atDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  function closeAtMenu(reason: string) {
    if (!atMenuOpen) return;
    dbg("at-mention", `close:${reason}`);
    atMenuOpen = false;
    atQuery = "";
    atResults = [];
    atSelectedIndex = 0;
    atStartPos = -1;
    atLoading = false;
    if (atDebounceTimer) {
      clearTimeout(atDebounceTimer);
      atDebounceTimer = null;
    }
  }

  function openAtMenu(pos: number) {
    // Audit #6: disable @ completion in remote mode (local listDirectory not applicable)
    if (isRemote) return;
    if (slashMenuOpen) closeSlashMenu("at-open");
    if (modeDropdownOpen) modeDropdownOpen = false;
    atMenuOpen = true;
    atStartPos = pos;
    atQuery = "";
    atResults = [];
    atSelectedIndex = 0;
    dbg("at-mention", "open", { pos });
  }

  function resolveAtPath(query: string): string {
    // Resolve relative query against cwd to get absolute path for listDirectory
    if (!query) return cwd;
    if (query.startsWith("/")) return query;
    const base = cwd.endsWith("/") ? cwd : cwd + "/";
    return base + query;
  }

  async function fetchAtResults(query: string) {
    atLoading = true;
    try {
      // Split into directory path + filename prefix
      const lastSlash = query.lastIndexOf("/");
      let dirQuery: string;
      let prefix: string;
      if (lastSlash >= 0) {
        dirQuery = query.slice(0, lastSlash + 1);
        prefix = query.slice(lastSlash + 1).toLowerCase();
      } else {
        dirQuery = "";
        prefix = query.toLowerCase();
      }
      const absPath = resolveAtPath(dirQuery);
      dbg("at-mention", "fetch", { absPath, prefix });
      const listing = await api.listDirectory(absPath, true);
      // Filter by prefix and limit to 10
      const filtered: AtMentionEntry[] = listing.entries
        .filter((e) => e.name.toLowerCase().startsWith(prefix))
        .slice(0, 10)
        .map((entry) => ({ ...entry, kind: "file" as const }));
      atResults = filtered;
      atSelectedIndex = 0;
    } catch (e) {
      dbg("at-mention", "fetch error", e);
      atResults = [];
    } finally {
      atLoading = false;
    }
  }

  function handleAtInput(cursorPos: number) {
    // Scan backwards from cursor for nearest @ preceded by whitespace or at position 0
    let atPos = -1;
    for (let i = cursorPos - 1; i >= 0; i--) {
      const ch = inputText[i];
      if (ch === "@") {
        // Valid if at start or preceded by whitespace
        if (i === 0 || /\s/.test(inputText[i - 1])) {
          atPos = i;
        }
        break;
      }
      if (/\s/.test(ch)) break; // whitespace before finding @ means no active @-mention
    }

    if (atPos >= 0) {
      const query = inputText.slice(atPos + 1, cursorPos);
      if (!atMenuOpen) openAtMenu(atPos);
      atQuery = query;

      // Debounce directory listing
      if (atDebounceTimer) clearTimeout(atDebounceTimer);
      atDebounceTimer = setTimeout(() => {
        fetchAtResults(query);
      }, 150);
    } else if (atMenuOpen) {
      closeAtMenu("no-at");
    }
  }

  function selectAtEntry(entry: AtMentionEntry) {
    if (atStartPos < 0 || !textareaEl) return;
    const cursorPos = textareaEl.selectionStart ?? inputText.length;
    const prefix = inputText.slice(0, atStartPos + 1); // keeps the @
    const suffix = inputText.slice(cursorPos);

    // Build the path relative to what was already typed
    const lastSlash = atQuery.lastIndexOf("/");
    const dirPrefix = lastSlash >= 0 ? atQuery.slice(0, lastSlash + 1) : "";
    const relativePath = dirPrefix + entry.name;

    if (entry.is_dir) {
      // Append / and keep menu open for deeper navigation
      inputText = prefix + relativePath + "/" + suffix;
      requestAnimationFrame(() => {
        if (textareaEl) {
          const newPos = atStartPos + 1 + relativePath.length + 1;
          textareaEl.selectionStart = textareaEl.selectionEnd = newPos;
          textareaEl.focus();
        }
        // Trigger new fetch for subdirectory contents
        handleAtInput(atStartPos + 1 + relativePath.length + 1);
      });
    } else {
      // Insert file path and close menu
      inputText = prefix + relativePath + suffix;
      closeAtMenu("select");
      requestAnimationFrame(() => {
        if (textareaEl) {
          const newPos = atStartPos + 1 + relativePath.length;
          textareaEl.selectionStart = textareaEl.selectionEnd = newPos;
          textareaEl.focus();
        }
      });
    }
    dbg("at-mention", "select", { name: entry.name, isDir: entry.is_dir });
  }

  // Force close when conditions no longer met
  $effect(() => {
    if (!slashEnabled && slashMenuOpen) {
      closeSlashMenu("disabled");
    }
  });

  /** Restore saved input text and clear the saved value to prevent stale restores. */
  function restoreSavedInput() {
    if (savedInputForSlash !== "") {
      inputText = savedInputForSlash;
      savedInputForSlash = "";
    }
  }

  function clearSavedInput() {
    savedInputForSlash = "";
  }

  function closeSlashMenu(reason: string) {
    if (!slashMenuOpen) return;
    dbg("slash", `close:${reason}`);
    slashMenuOpen = false;
    slashPhase = "commands";
    activeSlashCmd = null;
    slashSelectedIndex = 0;
    slashSubSelectedIndex = 0;

    if (classifyCloseReason(reason) === "clear") {
      clearSavedInput();
    } else {
      restoreSavedInput();
    }
  }

  function selectSlashCommand(cmd: CliCommand, trigger: "enter" | "tab") {
    const interaction = getCommandInteraction(cmd, skillNameSet);
    const selectedSkill = isSkillCommand(cmd, skillNameSet);
    dbg("slash", `select:${interaction}:${trigger}`, { name: cmd.name });

    // Both / and 、 triggers normalize to / when filling inputText below,
    // so the backend always sees the standard slash form.
    switch (interaction) {
      case "immediate":
        if (trigger === "enter") {
          inputText = `/${cmd.name}`;
          closeSlashMenu("execute");
          handleSend();
        } else {
          // Tab: fill only, don't execute
          closeSlashMenu("fill");
          inputText = `/${cmd.name} `;
          moveCursorToEnd();
        }
        break;
      case "free-text": {
        // Codex skills must be sent as structured refs. Selecting one from `/` should use the
        // same path as the bottom SkillSelector instead of inserting `/name` as plain text.
        if (agent === "codex" && selectedSkill) {
          inputText = "";
          closeSlashMenu("fill");
          handleSkillSelect(cmd.name.replace(/^skill:/i, ""));
          break;
        }
        closeSlashMenu("fill");
        // Pi's skill commands are registered with the `skill:` namespace. Keep
        // the namespace even when a legacy runtime returns the bare skill name.
        const commandName =
          agent === "pi" && isSkillCommand(cmd, skillNameSet)
            ? getPiSkillCommandName(cmd.name)
            : cmd.name;
        inputText = `/${commandName} `;
        moveCursorToEnd();
        break;
      }
      case "enum":
        activeSlashCmd = cmd;
        inputText = `/${cmd.name} `;
        if (cmd.name === "fast") {
          slashPhase = "sub-fast";
          slashSubSelectedIndex = fastModeState === "on" ? 1 : 0;
        } else {
          slashPhase = "sub-model";
          slashSubSelectedIndex = 0;
        }
        moveCursorToEnd();
        break;
    }
  }

  function goBackToCommands() {
    const cmdName = activeSlashCmd?.name;
    dbg("slash", "back-to-commands", { from: cmdName });
    activeSlashCmd = null;
    slashPhase = "commands";
    slashSubSelectedIndex = 0;
    if (cmdName) inputText = `/${cmdName}`;
    slashSelectedIndex = 0;
    moveCursorToEnd();
  }

  function handleSubModelSelect(model: CliModelInfo) {
    dbg("slash", "sub-model-select", { value: model.value });
    const restoreText = savedInputForSlash;
    closeSlashMenu("sub-select"); // clears savedInputForSlash
    inputText = restoreText; // restore user draft
    if (textareaEl) textareaEl.style.height = "auto";
    onModelSwitch?.(model.value);
  }

  function handleFastModeSelect(mode: "on" | "off") {
    dbg("slash", "fast-select", { mode });
    const restoreText = savedInputForSlash;
    closeSlashMenu("sub-select");
    inputText = restoreText;
    if (textareaEl) textareaEl.style.height = "auto";
    onFastModeSwitch?.(mode);
  }

  function moveCursorToEnd() {
    requestAnimationFrame(() => {
      if (textareaEl) {
        textareaEl.selectionStart = textareaEl.selectionEnd = inputText.length;
        textareaEl.focus();
      }
    });
  }

  /** Insert a template picked from the composer's “提示词” menu category. */
  function insertPromptTemplateFromMenu(template: PromptTemplate) {
    // The menu trigger steals focus, so snapshot the caret the textarea last held.
    promptTemplateSelection = {
      start: textareaEl?.selectionStart ?? inputText.length,
      end: textareaEl?.selectionEnd ?? inputText.length,
    };
    insertPromptTemplate(template);
  }

  function insertPromptTemplate(template: PromptTemplate) {
    const start = Math.max(0, Math.min(promptTemplateSelection.start, inputText.length));
    const end = Math.max(start, Math.min(promptTemplateSelection.end, inputText.length));
    const token = `/${template.name}`;
    const before = inputText.slice(0, start);
    const after = inputText.slice(end);
    // Keep the exact selection location; the user can add surrounding spaces like any other text.
    const insertion = token;
    inputText = before + insertion + after;
    promptTemplateTokens = [
      ...promptTemplateTokens,
      { token, name: template.name, content: template.content },
    ];
    const cursor = start + insertion.length;
    requestAnimationFrame(() => {
      if (!textareaEl) return;
      textareaEl.selectionStart = textareaEl.selectionEnd = cursor;
      textareaEl.focus();
      autoResize();
    });
    showFileToast(t("prompt_templatePickerInserted", { name: token }), "info");
    dbg("prompt-templates", "insert", { name: template.name, position: start });
  }

  function expandPromptTemplateTokens(body: string): string {
    let expanded = body;
    for (const template of promptTemplateTokens) {
      const index = expanded.indexOf(template.token);
      if (index < 0) continue;
      expanded =
        expanded.slice(0, index) + template.content + expanded.slice(index + template.token.length);
    }
    return expanded;
  }

  function removePromptTemplate(index: number) {
    const template = promptTemplateTokens[index];
    if (!template) return;
    let occurrence = 0;
    for (let i = 0; i < index; i++) {
      if (promptTemplateTokens[i].token === template.token) occurrence++;
    }
    let from = -1;
    let searchFrom = 0;
    for (let i = 0; i <= occurrence; i++) {
      from = inputText.indexOf(template.token, searchFrom);
      if (from < 0) break;
      searchFrom = from + template.token.length;
    }
    if (from >= 0)
      inputText = inputText.slice(0, from) + inputText.slice(from + template.token.length);
    promptTemplateTokens = promptTemplateTokens.filter((_, i) => i !== index);
    requestAnimationFrame(() => textareaEl?.focus());
  }

  let branchDropdownOpen = $state(false);
  let branchBtnEl: HTMLButtonElement | undefined = $state();
  let branchDropdownEl: HTMLDivElement | undefined = $state();
  let branchSearchQuery = $state("");
  let branchList = $state<import("$lib/api").GitBranchInfo[]>([]);
  let branchLoading = $state(false);
  let branchCreatingMode = $state(false);
  let newBranchInput = $state("");

  const displayBranch = $derived.by(() => {
    if (!gitBranch) return "";
    return gitBranch.includes("/") ? gitBranch.split("/").pop()! : gitBranch;
  });

  async function openBranchDropdown() {
    branchDropdownOpen = !branchDropdownOpen;
    if (branchDropdownOpen) {
      if (modeDropdownOpen) modeDropdownOpen = false;
      if (slashMenuOpen) closeSlashMenu("branch-open");
      branchSearchQuery = "";
      branchCreatingMode = false;
      newBranchInput = "";
      branchLoading = true;
      try {
        const branches = await api.listGitBranches(cwd || "/");
        branchList = branches;
      } catch (e) {
        dbgWarn("git", "listGitBranches failed", e);
      } finally {
        branchLoading = false;
      }
    }
  }

  async function handleSelectBranch(branch: import("$lib/api").GitBranchInfo) {
    branchDropdownOpen = false;
    if (branch.worktree_path) {
      showFileToast(
        t("prompt_gitBranchCheckedOut", { branch: branch.name, path: branch.worktree_path }),
      );
      return;
    }
    try {
      await api.checkoutGitBranch(cwd || "/", branch.name);
      gitBranch = branch.name;
      branchPoller.refresh(cwd || "/");
    } catch (e) {
      const error = e instanceof Error ? e.message : String(e);
      showFileToast(t("prompt_gitBranchCheckoutFailed", { error }));
      dbgWarn("git", "checkoutGitBranch failed", e);
    }
  }

  async function handleCreateBranch() {
    if (!newBranchInput.trim()) return;
    const name = newBranchInput.trim();
    branchDropdownOpen = false;
    try {
      await api.createGitBranch(cwd || "/", name);
      gitBranch = name;
      branchPoller.refresh(cwd || "/");
    } catch (e) {
      dbgWarn("git", "createGitBranch failed", e);
    }
  }

  const CLAUDE_EFFORT_OPTIONS = [
    { value: "", label: "默认" },
    { value: "low", label: "低" },
    { value: "medium", label: "中" },
    { value: "high", label: "高" },
    { value: "xhigh", label: "极高" },
    { value: "max", label: "最大" },
  ];
  const CODEX_EFFORT_OPTIONS = [
    { value: "", label: "默认" },
    { value: "low", label: "轻度" },
    { value: "medium", label: "中" },
    { value: "high", label: "高" },
    { value: "xhigh", label: "极高" },
    { value: "max", label: "最高" },
  ];
  const PI_EFFORT_OPTIONS = [...PI_THINKING_LEVEL_OPTIONS];

  const CODEX_EFFORT_LABELS: Record<string, string> = {
    none: "无",
    minimal: "极少",
    low: "轻度",
    medium: "中",
    high: "高",
    xhigh: "极高",
    max: "最高",
  };
  const EFFORT_LABELS: Record<string, string> = {
    none: "无",
    off: "关闭",
    minimal: "极少",
    low: "低",
    medium: "中",
    high: "高",
    xhigh: "极高",
    max: "最大",
  };

  let effortOptions = $derived.by(() => {
    const fallback =
      agent === "pi"
        ? PI_EFFORT_OPTIONS
        : agent === "claude"
          ? CLAUDE_EFFORT_OPTIONS
          : CODEX_EFFORT_OPTIONS;
    const modelInfo = models.find((model) => model.value === currentModel);
    const supportedLevels = modelInfo?.supportedEffortLevels;
    if (!supportedLevels?.length) return fallback;
    const piLevels =
      agent === "pi" ? filterPiThinkingLevelsForUi(supportedLevels) : supportedLevels;
    return [
      ...(agent === "pi" ? [] : [{ value: "", label: "默认" }]),
      ...piLevels.map((value) => ({
        value,
        label: (agent === "codex" ? CODEX_EFFORT_LABELS : EFFORT_LABELS)[value] ?? value,
      })),
    ];
  });

  const SESSION_REQUIRED_COMMANDS = new Set(["compact", "copy-last", "context", "clear", "rename"]);

  function isSessionRequired(cmdName: string): boolean {
    return SESSION_REQUIRED_COMMANDS.has(cmdName);
  }

  /** Handle L3 quick-action pill click. Three branches: enum, free-text, immediate. */
  function handleQuickAction(cmd: CliCommand) {
    if (!slashEnabled) return;
    dbg("slash", "quick-action", { name: cmd.name });

    if (cmd.name === "clear" || cmd.name === "clear-context") {
      if (!hasRun) {
        clearAll();
        focus();
        return;
      }
    }

    const interaction = getCommandInteraction(cmd);

    if (interaction === "enum") {
      // e.g., model/fast: close other menus → save input → open sub-view
      if (atMenuOpen) closeAtMenu("quick-action");
      if (modeDropdownOpen) modeDropdownOpen = false;
      savedInputForSlash = inputText;
      inputText = `/${cmd.name} `;
      activeSlashCmd = cmd;
      if (cmd.name === "fast") {
        slashPhase = "sub-fast";
        slashSubSelectedIndex = fastModeState === "on" ? 1 : 0;
      } else {
        slashPhase = "sub-model";
        slashSubSelectedIndex = 0;
      }
      slashMenuOpen = true;
      moveCursorToEnd();
      return;
    }

    if (interaction === "free-text") {
      // Fill "/cmd " and focus — don't send, don't clear draft
      if (atMenuOpen) closeAtMenu("quick-action");
      if (modeDropdownOpen) modeDropdownOpen = false;
      inputText = `/${cmd.name} `;
      moveCursorToEnd();
      return;
    }

    // immediate: execute directly without touching inputText/pastedBlocks/attachments.
    // Resolve agent-aware so per-agent variants pick the correct _action.
    const vDef = resolveVirtualCommand(cmd.name, agent) ?? cmd;
    if (vDef) {
      if (typeof vDef["_action"] === "string" && onVirtualCommand) {
        onVirtualCommand(vDef["_action"] as string, "");
        return;
      }
      if (typeof vDef["_navigate"] === "string") {
        goto(vDef["_navigate"] as string);
        return;
      }
    }
    // Regular CLI command: send "/cmd" directly without draft/attachments
    onSend(`/${cmd.name}`, []);
  }

  // ── IME composition event handlers ──
  function handleCompositionStart() {
    isComposing = true;
  }

  function handleCompositionEnd() {
    isComposing = false;
    // Re-check input after IME completes (e.g., the dun trigger char "、")
    handleInput();
  }

  function handleInput() {
    autoResize();

    // Exit history mode if user edits the recalled text — runs regardless of IME
    if (histState.index >= 0 && inputText !== userHistory[histState.index]) {
      dbg("prompt-history", "exit: user edited", { index: histState.index });
      resetHistory(histState);
    }

    // Skip menu detection (slash menu + @-mention menu) during IME composition
    // to avoid flicker. History exit above runs regardless — only menu logic is
    // deferred. Intentional: menus update once after composition completes
    // (e.g., pinyin → 你好), not on every intermediate pinyin keystroke.
    if (isComposing) return;

    // @-mention detection: runs BEFORE slashEnabled guard so it works pre-session
    const cursorPos = textareaEl?.selectionStart ?? inputText.length;
    handleAtInput(cursorPos);

    if (!slashEnabled) {
      dbg("slash", "disabled", { agent, useStreamSession, sessionAlive, canResume, inputText });
      return;
    }

    if (slashPhase === "sub-model" || slashPhase === "sub-fast") {
      // Close sub-view if input no longer matches /activeCmdName
      if (activeSlashCmd && !isSubViewInputValid(inputText, activeSlashCmd.name)) {
        closeSlashMenu("sub-invalid-input");
      }
      return;
    }

    // Commands phase: support both slash (/) and Chinese dun (、) triggers
    const query = extractSlashQuery(inputText);
    if (query !== null) {
      slashSelectedIndex = 0;
      if (!slashMenuOpen) {
        dbg("slash", "open", { query });
        if (modeDropdownOpen) modeDropdownOpen = false;
        slashMenuOpen = true;
        slashPhase = "commands";
      }
    } else if (slashMenuOpen) {
      closeSlashMenu("no-match");
    }
  }

  /** Apply a history action (shared by immediate and deferred paths). */
  function applyHistoryAction(action: NonNullable<HistoryAction>) {
    if (action.type === "boundary") {
      dbg("prompt-history", "boundary", { index: histState.index });
      return;
    }

    if (action.type === "enter") {
      histState.draft = getInputSnapshot();
      histState.index = action.index;
      dbg("prompt-history", "up: enter history", { index: 0, total: userHistory.length });
    } else if (action.type === "up") {
      histState.index = action.index;
      dbg("prompt-history", "up", { index: action.index });
    } else if (action.type === "down") {
      histState.index = action.index;
      dbg("prompt-history", "down", { index: action.index });
    } else if (action.type === "restore-draft") {
      histState.index = -1;
      if (histState.draft) {
        dbg("prompt-history", "restore-draft", {
          textLen: histState.draft.text.length,
          atts: histState.draft.attachments.length,
          pastes: histState.draft.pastedBlocks.length,
        });
        restoreSnapshot(histState.draft);
        histState.draft = null;
        return; // restoreSnapshot handles autoResize + focus
      }
      inputText = "";
      pendingAttachments = [];
      pastedBlocks = [];
    }

    if (action.type !== "restore-draft") {
      // Bounds guard: if index is stale (timeline changed between events), bail out
      if (histState.index >= userHistory.length) {
        dbg("prompt-history", "stale index, resetting", {
          index: histState.index,
          len: userHistory.length,
        });
        resetHistory(histState);
        return;
      }
      inputText = userHistory[histState.index];
      pendingAttachments = [];
      pastedBlocks = [];
    }

    requestAnimationFrame(() => {
      autoResize();
      if (textareaEl) {
        textareaEl.selectionStart = textareaEl.selectionEnd = textareaEl.value.length;
      }
    });
  }

  function handleKeydown(e: KeyboardEvent) {
    // Skip during IME composition (e.g., Chinese input confirming with Enter)
    if (e.isComposing || e.keyCode === 229) return;

    // ── @-mention menu ──
    if (atMenuOpen) {
      if (e.key === "Escape") {
        e.preventDefault();
        closeAtMenu("escape");
        return;
      }
      if (atResults.length > 0) {
        if (e.key === "ArrowDown") {
          e.preventDefault();
          atSelectedIndex = Math.min(atSelectedIndex + 1, atResults.length - 1);
          return;
        }
        if (e.key === "ArrowUp") {
          e.preventDefault();
          atSelectedIndex = Math.max(atSelectedIndex - 1, 0);
          return;
        }
        if (e.key === "Enter" || e.key === "Tab") {
          e.preventDefault();
          selectAtEntry(atResults[atSelectedIndex]);
          return;
        }
      }
      // Let other keys through for typing
    }

    // ── Sub-model phase ──
    if (slashMenuOpen && slashPhase === "sub-model") {
      if (e.key === "Escape") {
        e.preventDefault();
        goBackToCommands();
        return;
      }
      if (e.key === "Backspace") {
        if (
          shouldBackFromSubView(inputText, textareaEl?.selectionStart ?? 0, activeSlashCmd?.name)
        ) {
          e.preventDefault();
          goBackToCommands();
          return;
        }
        // else: normal backspace (let it through)
        return;
      }
      if (models.length > 0) {
        if (e.key === "ArrowDown") {
          e.preventDefault();
          slashSubSelectedIndex = Math.min(slashSubSelectedIndex + 1, models.length - 1);
          return;
        }
        if (e.key === "ArrowUp") {
          e.preventDefault();
          slashSubSelectedIndex = Math.max(slashSubSelectedIndex - 1, 0);
          return;
        }
        if (e.key === "Enter" || e.key === "Tab") {
          e.preventDefault();
          handleSubModelSelect(models[slashSubSelectedIndex]);
          return;
        }
      }
      // Let other keys through for typing in sub-view
      return;
    }

    // ── Sub-fast phase ──
    if (slashMenuOpen && slashPhase === "sub-fast") {
      if (e.key === "Escape") {
        e.preventDefault();
        goBackToCommands();
        return;
      }
      if (e.key === "Backspace") {
        if (
          shouldBackFromSubView(inputText, textareaEl?.selectionStart ?? 0, activeSlashCmd?.name)
        ) {
          e.preventDefault();
          goBackToCommands();
          return;
        }
        return;
      }
      const FAST_OPTIONS = 2;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        slashSubSelectedIndex = Math.min(slashSubSelectedIndex + 1, FAST_OPTIONS - 1);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        slashSubSelectedIndex = Math.max(slashSubSelectedIndex - 1, 0);
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        handleFastModeSelect(slashSubSelectedIndex === 0 ? "off" : "on");
        return;
      }
      return;
    }

    // ── Commands phase ──
    if (slashMenuOpen && slashPhase === "commands") {
      if (e.key === "Escape") {
        e.preventDefault();
        closeSlashMenu("escape");
        return;
      }
      if (effectiveCommands.length > 0) {
        if (e.key === "ArrowDown") {
          e.preventDefault();
          slashSelectedIndex = Math.min(slashSelectedIndex + 1, effectiveCommands.length - 1);
          return;
        }
        if (e.key === "ArrowUp") {
          e.preventDefault();
          slashSelectedIndex = Math.max(slashSelectedIndex - 1, 0);
          return;
        }
        if (e.key === "Enter") {
          e.preventDefault();
          selectSlashCommand(effectiveCommands[slashSelectedIndex], "enter");
          return;
        }
        if (e.key === "Tab") {
          e.preventDefault();
          selectSlashCommand(effectiveCommands[slashSelectedIndex], "tab");
          return;
        }
      }
      // No filteredCommands but menu open (empty state) — only Esc handled above
      return;
    }

    // ── Input history (Up/Down arrow) ──
    if (
      shouldIntercept(
        e.key,
        e,
        { atMenuOpen, slashMenuOpen, modeDropdownOpen },
        textareaEl?.selectionStart ?? 0,
        textareaEl?.selectionEnd ?? 0,
        userHistory.length,
      ) &&
      textareaEl
    ) {
      // Multi-line or visually wrapped text: defer to next frame to let the
      // browser move the cursor first. Only trigger history if cursor didn't
      // move (meaning we're at the visual top/bottom edge).
      if (hasMultipleVisualLines(textareaEl)) {
        const posBefore = textareaEl.selectionStart;
        const key = e.key;
        // Immediate path: cursor at absolute start (Up) or end (Down)
        // — guaranteed to be at the visual edge, no need to defer.
        const atAbsoluteEdge =
          (key === "ArrowUp" && posBefore === 0) ||
          (key === "ArrowDown" && posBefore === textareaEl.value.length);
        if (atAbsoluteEdge) {
          const action = getHistoryAction(
            key,
            histState,
            userHistory.length,
            textareaEl.value,
            posBefore,
          );
          if (action) {
            e.preventDefault();
            applyHistoryAction(action);
            return;
          }
        }
        // Let browser handle cursor movement, check on next frame
        requestAnimationFrame(() => {
          if (!textareaEl) return;
          if (textareaEl.selectionStart !== posBefore) return; // cursor moved — normal nav
          const action = getHistoryAction(
            key,
            histState,
            userHistory.length,
            textareaEl.value,
            textareaEl.selectionStart,
          );
          if (action) {
            applyHistoryAction(action);
          }
        });
        return;
      }

      // Single visual line: handle immediately
      const action = getHistoryAction(
        e.key,
        histState,
        userHistory.length,
        textareaEl.value,
        textareaEl.selectionStart,
      );
      if (action) {
        e.preventDefault();
        applyHistoryAction(action);
        return;
      }
    }

    // ── Double Esc: clear all input ──
    if (e.key === "Escape") {
      const now = Date.now();
      if (now - lastEscTime < 400 && hasContent()) {
        e.preventDefault();
        clearAll();
        lastEscTime = 0;
        return;
      }
      lastEscTime = now;
      return;
    }

    // ── ? shortcut help: when input is empty, forward to parent instead of typing "?" ──
    if (e.key === "?" && !hasContent() && onShortcutHelp) {
      e.preventDefault();
      onShortcutHelp();
      return;
    }

    // ── Normal input ──
    // Cmd/Ctrl+Enter is handled by the global chat:sendGlobal binding. Do not also submit here,
    // otherwise one shortcut press sends the same prompt twice.
    if (e.key === "Enter" && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
      e.preventDefault();
      handleSend();
    }
  }

  /** Wrap path in backtick fence that won't conflict with path content. */
  function wrapPathInBackticks(p: string): string {
    let maxRun = 0;
    let currentRun = 0;
    for (const ch of p) {
      if (ch === "`") {
        currentRun++;
        maxRun = Math.max(maxRun, currentRun);
      } else {
        currentRun = 0;
      }
    }
    const fence = "`".repeat(maxRun + 1);
    const needsPadding = p.startsWith("`") || p.endsWith("`");
    return needsPadding ? `${fence} ${p} ${fence}` : `${fence}${p}${fence}`;
  }

  function handleSend() {
    const expandedTemplateText = expandPromptTemplateTokens(inputText);
    const typed = expandedTemplateText.trim();

    // Virtual slash command check — based on raw textarea, not paste blocks
    if (typed) {
      const virtual = parseVirtualAction(typed, agent);
      if (virtual) {
        dbg("slash", `virtual:${virtual.name}`, { args: virtual.args });
        if (virtual.name === "model" && virtual.args) {
          if (onModelSwitch) {
            inputText = "";
            promptTemplateTokens = [];
            if (textareaEl) textareaEl.style.height = "auto";
            onModelSwitch(virtual.args);
          }
          return; // Always consume /model command, even without handler
        }
        // Navigation virtual commands (e.g. /config → /settings?tab=cli-config).
        // Resolve agent-aware so per-agent variants (Codex /rewind → "codex-rewind")
        // dispatch the right _action instead of the first same-named entry.
        const vDef = resolveVirtualCommand(virtual.name, agent);
        if (vDef && typeof vDef["_navigate"] === "string") {
          inputText = "";
          promptTemplateTokens = [];
          if (textareaEl) textareaEl.style.height = "auto";
          goto(vDef["_navigate"] as string);
          return;
        }
        // Side question virtual command (/btw <question>)
        if (vDef && vDef["_action"] === "side-question") {
          if (btwAvailable && virtual.args) {
            inputText = "";
            promptTemplateTokens = [];
            if (textareaEl) textareaEl.style.height = "auto";
            onBtwSend?.(virtual.args);
          }
          return;
        }
        // Action virtual commands (e.g. /copy → copy-last)
        if (vDef && typeof vDef["_action"] === "string" && onVirtualCommand) {
          inputText = "";
          promptTemplateTokens = [];
          if (textareaEl) textareaEl.style.height = "auto";
          onVirtualCommand(vDef["_action"] as string, virtual.args);
          return;
        }
      }
    }

    // Separate regular (binary) and path-reference attachments
    const regularAtts = attachmentsAvailable
      ? pendingAttachments.filter((a) => a.contentBase64)
      : [];
    const pathRefAtts = attachmentsAvailable
      ? pendingAttachments.filter((a) => a.filePath && !a.contentBase64)
      : [];

    // Expand inline paste tokens at their cursor positions (issue #156). `usedIds` are the blocks
    // consumed by a token; the rest are orphans (drag-dropped text files, or blocks whose token the
    // user deleted) and keep the legacy leading-append behavior.
    const { text: expandedBody, usedSeqs } = expandPasteTokens(inputText, pastedBlocks);
    const bodyText = expandPromptTemplateTokens(expandedBody).trim();
    const orphanBlocks = pastedBlocks.filter((b) => b.seq == null || !usedSeqs.has(b.seq));

    const parts: string[] = orphanBlocks.map((b) => b.text);
    if (pathRefAtts.length > 0) {
      const refs = pathRefAtts.map((a) => `[PDF: ${a.filePath}]`).join("\n");
      parts.push(refs);
    }
    // pendingPathRefs (directories, large files from drag-drop)
    if (attachmentsAvailable && pendingPathRefs.length > 0) {
      parts.push(pendingPathRefs.map((r) => wrapPathInBackticks(r.path)).join("\n"));
    }

    if (bodyText) parts.push(bodyText);
    let text = parts.join("\n\n");
    if (selectedExpert) {
      const expertTag = `[当前协作专家: ${selectedExpert.title}${selectedExpert.isTeam ? " (专家团队)" : ""}]`;
      if (!text.includes(expertTag)) {
        text = `${expertTag}\n${text}`;
      }
      // DSH does not have a separate expert/team protocol. Its supported
      // contract is the model-invocable Skill tool, so make the selected
      // expert deterministic instead of relying on the model to infer the
      // mapping from the display title alone.
      if (agent === "dsh") {
        const skillGesture = `/${dshExpertSkillName(selectedExpert.id)}`;
        if (!text.includes(skillGesture)) {
          text = `${skillGesture}\n${text}`;
        }
      }
    }
    // Allow a skill-only send (a picked skill with no typed text is a valid skill turn).
    if ((!text && pendingSkills.length === 0) || disabled) return;

    const skills = pendingSkills.length > 0 ? pendingSkills : undefined;
    dbg("prompt", "send", {
      len: text.length,
      pasteBlocks: pastedBlocks.length,
      attachments: regularAtts.length,
      pathRefs: pathRefAtts.length,
      dragPathRefs: pendingPathRefs.length,
      skills: pendingSkills.length,
      promptTemplates: promptTemplateTokens.length,
      agent,
    });

    const attachments: Attachment[] = regularAtts.map((a) => ({
      name: a.name,
      type: a.type,
      size: a.size,
      contentBase64: a.contentBase64!,
    }));

    // A live Pi session must not fall through to the generic send path while
    // its follow-up capability is unknown or unavailable. The caller may still
    // expose the draft editor; submitting waits for the capability projection.
    if ((agent === "pi" || agent === "claude" || agent === "dsh") && running && !queueAvailable)
      return;

    inputText = "";
    pendingAttachments = [];
    pastedBlocks = [];
    pendingPathRefs = [];
    pendingSkills = [];
    promptTemplateTokens = [];
    resetHistory(histState);

    if ((agent === "pi" || agent === "claude" || agent === "dsh") && running && onQueueSend) {
      // Running sessions use the safe follow-up queue. Immediate action is available per queued
      // row (Pi/DSH: native steer; Claude: interrupt and send).
      onQueueSend(text, attachments);
    } else {
      onSend(text, attachments, skills);
    }

    // Reset textarea height
    if (textareaEl) textareaEl.style.height = "auto";
  }

  async function handleQueueSteer(id: string) {
    if (queueSteerInFlight || !queueActionAvailable) return;
    queueSteerInFlight = id;
    try {
      await onQueueSteer?.(id);
    } finally {
      if (queueSteerInFlight === id) queueSteerInFlight = null;
    }
  }

  function handleBtwSend() {
    const question = inputText.trim();
    if (disabled || !question || !btwAvailable) return;
    dbg("prompt", "btwSend", { len: question.length });
    inputText = "";
    if (textareaEl) textareaEl.style.height = "auto";
    onBtwSend?.(question);
  }

  async function processFiles(files: FileList | File[]) {
    if (disabled || !attachmentsAvailable) return;
    let binaryRemaining = MAX_ATTACHMENTS - pendingAttachments.length;
    let textRemaining = MAX_PASTE_BLOCKS - pastedBlocks.length;
    const rejected: string[] = [];

    for (const file of Array.from(files)) {
      // MIME normalization: force application/pdf when detected by extension
      // (backend silently skips attachments with unrecognized MIME types)
      const detectedPdf = !isPdf(file.type) && getFileExtension(file.name) === "pdf";
      const effectivePdf = isPdf(file.type) || detectedPdf;

      // PDF >20MB ≤100MB: save to temp, use path-reference (CLI handles via pdftoppm)
      if (effectivePdf && file.size > PDF_MAX_BINARY_SIZE) {
        if (file.size > PDF_MAX_PATH_SIZE) {
          showFileToast(t("prompt_fileTooLarge", { limit: "100", name: file.name }));
          continue;
        }
        if (binaryRemaining <= 0) {
          showFileToast(t("prompt_maxAttachments", { count: String(MAX_ATTACHMENTS) }));
          break;
        }
        binaryRemaining--;
        try {
          const buffer = await file.arrayBuffer();
          const base64 = arrayBufferToBase64(buffer);
          const tempPath = await api.saveTempAttachment(file.name, base64);
          pendingAttachments = [
            ...pendingAttachments,
            {
              id: uuid().slice(0, 8),
              name: file.name,
              type: "application/pdf",
              size: file.size,
              filePath: tempPath,
            },
          ];
          dbg("prompt", "pdf-temp-path-ref", { name: file.name, size: file.size, path: tempPath });
        } catch (e) {
          binaryRemaining++;
          dbgWarn("prompt", "pdf-temp-save-failed", { name: file.name, error: e });
          showFileToast(t("prompt_fileTooLarge", { limit: "20", name: file.name }));
        }
        continue;
      }

      // 1) Size check — per type (images: no limit, PDF: 20MB, text: 10MB)
      const sizeLimit = getFileSizeLimit(file);
      if (file.size > sizeLimit) {
        const limitMB = sizeLimit / (1024 * 1024);
        showFileToast(t("prompt_fileTooLarge", { limit: String(limitMB), name: file.name }));
        continue;
      }

      // 2) Binary attachment: images, PDF, and Office files. Office files must
      // reach their built-in Skill as the original file, not as Markdown text.
      if (BINARY_ATTACHMENT_TYPES.includes(file.type) || detectedPdf || isOfficeFile(file)) {
        if (binaryRemaining <= 0) {
          showFileToast(t("prompt_maxAttachments", { count: String(MAX_ATTACHMENTS) }));
          break;
        }
        binaryRemaining--;
        const reader = new FileReader();
        reader.onload = () => {
          const dataUrl = reader.result as string;
          const base64 = dataUrl.split(",")[1] ?? "";
          pendingAttachments = [
            ...pendingAttachments,
            {
              id: uuid().slice(0, 8),
              name: file.name || `attachment.${file.type.split("/")[1] || "bin"}`,
              type: detectedPdf
                ? "application/pdf"
                : isOfficeFile(file)
                  ? mimeForOfficeFile(file)
                  : file.type,
              size: file.size,
              contentBase64: base64,
            },
          ];
          dbg("prompt", "add-binary-file", { name: file.name, type: file.type, size: file.size });
        };
        reader.readAsDataURL(file);
        continue;
      }
      // 3) Text file → pastedBlock
      if (isTextFile(file)) {
        if (textRemaining <= 0) {
          showFileToast(t("prompt_maxTextFiles", { count: String(MAX_PASTE_BLOCKS) }));
          break;
        }
        textRemaining--; // Pre-decrement before async read to prevent race
        const reader = new FileReader();
        reader.onload = () => {
          const text = reader.result as string;
          const lines = text.split("\n");
          const lineCount = lines.length;
          const charCount = text.length;
          const ext = getFileExtension(file.name);
          const preview = file.name || `file.${ext}`;

          pastedBlocks = [
            ...pastedBlocks,
            {
              id: uuid().slice(0, 8),
              text,
              lineCount,
              charCount,
              preview,
              ext,
            },
          ];
          dbg("prompt", "add-text-file", {
            name: file.name,
            lines: lineCount,
            chars: charCount,
          });
        };
        reader.readAsText(file);
        continue;
      }
      // 4) Unsupported
      rejected.push(getFileExtension(file.name) || file.type || "unknown");
    }
    if (rejected.length > 0) {
      showFileToast(t("prompt_unsupportedFile", { ext: rejected[0] }));
    }
  }

  function handleFileSelect(e: Event) {
    if (disabled) return;
    const input = e.target as HTMLInputElement;
    const files = input.files;
    if (!files) return;
    processFiles(files);
    input.value = "";
  }

  function removeAttachment(id: string) {
    pendingAttachments = pendingAttachments.filter((a) => a.id !== id);
  }

  function handlePaste(e: ClipboardEvent) {
    if (disabled || !attachmentsAvailable) return;
    // Step 1: Check for clipboard binary files (images, PDF) BEFORE text
    const items = e.clipboardData?.items;
    if (items) {
      const binaryItems: DataTransferItem[] = [];
      for (let i = 0; i < items.length; i++) {
        if (BINARY_ATTACHMENT_TYPES.includes(items[i].type)) {
          binaryItems.push(items[i]);
        } else if (items[i].kind === "file") {
          // Extension fallback: browser may give wrong/empty MIME for PDF
          const file = items[i].getAsFile();
          if (file && (getFileExtension(file.name) === "pdf" || isOfficeFile(file))) {
            binaryItems.push(items[i]);
          }
        }
      }
      if (binaryItems.length > 0) {
        e.preventDefault();
        const filesToProcess: File[] = [];
        for (const item of binaryItems) {
          const file = item.getAsFile();
          if (file) filesToProcess.push(file);
        }
        if (filesToProcess.length > 0) processFiles(filesToProcess);
        return;
      }
    }

    // Step 2: Text paste handling
    const text = e.clipboardData?.getData("text/plain");

    if (!text) {
      // Empty text — likely Finder file paste (macOS puts file URLs, not text)
      e.preventDefault();
      tryNativeClipboardPaste();
      return;
    }

    const lines = text.split("\n");
    const lineCount = lines.length;
    const charCount = text.length;

    if (lineCount < 5 && charCount < 500) {
      // Short text — could be Finder filename or normal short text
      // Don't preventDefault → let browser insert text normally
      const snapshot = inputText;
      const cursorPos = textareaEl?.selectionStart ?? inputText.length;
      // Async check: if native clipboard has files, roll back the inserted text
      tryNativeClipboardPaste(snapshot, cursorPos);
      return;
    }

    // Long text → intercept, compress into chip + drop a position-anchored token at the cursor
    // so it expands back at the right spot on send (issue #156), instead of always prepending.
    e.preventDefault();
    if (pastedBlocks.length >= MAX_PASTE_BLOCKS) {
      showFileToast(t("prompt_maxPasteBlocks", { count: String(MAX_PASTE_BLOCKS) }));
      return;
    }

    const firstLine = lines[0].trim();
    const preview = firstLine.length > 40 ? firstLine.slice(0, 40) + "..." : firstLine;

    const id = uuid().slice(0, 8);
    const seq = nextPasteSeq();
    const label = t("prompt_pasteToken", {
      seq: String(seq),
      size: formatPasteSize(lineCount, charCount),
    });
    const token = buildPasteToken(label);

    // Insert at cursor, replacing any active selection.
    const selStart = textareaEl?.selectionStart ?? inputText.length;
    const selEnd = textareaEl?.selectionEnd ?? selStart;
    inputText = insertAtSelection(inputText, selStart, selEnd, token);
    pastedBlocks = [...pastedBlocks, { id, text, lineCount, charCount, preview, seq }];

    requestAnimationFrame(() => {
      if (textareaEl) {
        const pos = selStart + token.length;
        textareaEl.selectionStart = textareaEl.selectionEnd = pos;
        textareaEl.focus();
      }
    });

    dbg("prompt", "paste-compressed", { lineCount, charCount, blocks: pastedBlocks.length, seq });
  }

  // Monotonic per-message token sequence — max existing seq + 1, so removing a chip and
  // pasting again never reuses a visible "#N" that's still on screen.
  function nextPasteSeq(): number {
    return pastedBlocks.reduce((max, b) => Math.max(max, b.seq ?? 0), 0) + 1;
  }

  function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
    return Promise.race([
      promise,
      new Promise<never>((_, reject) => setTimeout(() => reject(new Error("timeout")), ms)),
    ]);
  }

  async function tryNativeClipboardPaste(snapshot?: string, cursorPos?: number) {
    try {
      const files = await withTimeout(api.getClipboardFiles(), 250);
      if (files.length === 0) return; // No files — text already inserted (or empty paste)

      dbg("prompt", "native-clipboard-files", { count: files.length });

      // Roll back browser-inserted text if we have a snapshot
      if (snapshot !== undefined) {
        inputText = snapshot;
        if (textareaEl && cursorPos !== undefined) {
          requestAnimationFrame(() => {
            textareaEl!.selectionStart = textareaEl!.selectionEnd = cursorPos;
          });
        }
      }
      await processClipboardPaths(files);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      // Only show toast when user explicitly pasted files (no text in clipboard)
      if (snapshot === undefined && msg.includes("not yet supported")) {
        showFileToast(t("prompt_clipboardUnsupported"));
      }
      dbg("prompt", "native clipboard failed/timeout", e);
    }
  }

  async function processClipboardPaths(files: ClipboardFileInfo[]) {
    if (disabled || !attachmentsAvailable) return;
    let binaryRemaining = MAX_ATTACHMENTS - pendingAttachments.length;
    let textRemaining = MAX_PASTE_BLOCKS - pastedBlocks.length;
    const rejected: string[] = [];

    for (const file of files) {
      // MIME normalization: force application/pdf for extension-detected PDFs
      // (backend silently skips attachments with unrecognized MIME types)
      const clipboardPdf =
        file.mime_type !== "application/pdf" && getFileExtension(file.name).toLowerCase() === "pdf";
      const officeMime = mimeForOfficeExtension(getFileExtension(file.name));
      const effectiveMime = clipboardPdf
        ? "application/pdf"
        : isOfficeFile(new File([], file.name))
          ? officeMime
          : file.mime_type;

      // PDF path-reference: >20MB ≤100MB → store path only, CLI handles via pdftoppm
      if (isPdf(effectiveMime) && file.size > PDF_MAX_BINARY_SIZE) {
        if (file.size > PDF_MAX_PATH_SIZE) {
          showFileToast(t("prompt_fileTooLarge", { limit: "100", name: file.name }));
          continue;
        }
        if (binaryRemaining <= 0) {
          showFileToast(t("prompt_maxAttachments", { count: String(MAX_ATTACHMENTS) }));
          break;
        }
        binaryRemaining--;
        pendingAttachments = [
          ...pendingAttachments,
          {
            id: uuid().slice(0, 8),
            name: file.name,
            type: effectiveMime,
            size: file.size,
            filePath: file.path,
          },
        ];
        dbg("prompt", "clipboard-pdf-path-ref", {
          name: file.name,
          size: file.size,
          path: file.path,
        });
        continue;
      }

      const sizeLimit = getSizeLimitByMime(effectiveMime);
      if (file.size > sizeLimit) {
        const limitMB = sizeLimit / (1024 * 1024);
        showFileToast(t("prompt_fileTooLarge", { limit: String(limitMB), name: file.name }));
        continue;
      }
      const cls = classifyByMime(effectiveMime);

      if (cls === "binary") {
        if (binaryRemaining <= 0) {
          showFileToast(t("prompt_maxAttachments", { count: String(MAX_ATTACHMENTS) }));
          break;
        }
        binaryRemaining--;
        try {
          const content = await api.readClipboardFile(file.path, false);
          pendingAttachments = [
            ...pendingAttachments,
            {
              id: uuid().slice(0, 8),
              name: file.name,
              type: effectiveMime,
              size: file.size,
              contentBase64: content.content_base64,
            },
          ];
          dbg("prompt", "clipboard-binary", { name: file.name, type: effectiveMime });
        } catch (e) {
          dbg("prompt", "clipboard-read-error", { name: file.name, error: e });
        }
      } else if (cls === "text") {
        if (textRemaining <= 0) {
          showFileToast(t("prompt_maxTextFiles", { count: String(MAX_PASTE_BLOCKS) }));
          break;
        }
        textRemaining--;
        try {
          const content = await api.readClipboardFile(file.path, true);
          const text = content.content_text ?? "";
          const lineCount = text.split("\n").length;
          pastedBlocks = [
            ...pastedBlocks,
            {
              id: uuid().slice(0, 8),
              text,
              lineCount,
              charCount: text.length,
              preview: file.name,
              ext: getFileExtension(file.name),
            },
          ];
          dbg("prompt", "clipboard-text", { name: file.name, lines: lineCount });
        } catch (e) {
          dbg("prompt", "clipboard-read-error", { name: file.name, error: e });
        }
      } else {
        rejected.push(getFileExtension(file.name) || "unknown");
      }
    }
    if (rejected.length > 0) {
      showFileToast(t("prompt_unsupportedFile", { ext: rejected[0] }));
    }
  }

  function mimeForOfficeExtension(ext: string): string {
    switch (ext.toLowerCase()) {
      case "docx":
        return "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
      case "xlsx":
        return "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
      case "pptx":
        return "application/vnd.openxmlformats-officedocument.presentationml.presentation";
      default:
        return "application/octet-stream";
    }
  }

  function mimeForOfficeFile(file: File): string {
    return mimeForOfficeExtension(getFileExtension(file.name));
  }

  function removePastedBlock(id: string) {
    const blk = pastedBlocks.find((b) => b.id === id);
    pastedBlocks = pastedBlocks.filter((b) => b.id !== id);
    // Drop the anchoring token too (token-less drag-dropped blocks have no seq → skip).
    if (blk?.seq != null) inputText = inputText.replace(singlePasteTokenRe(blk.seq), "");
  }

  // ── Paste block editor (issue #156) ──
  // The main textarea is capped at ~4 lines, so long pastes can't be edited inline. Instead the
  // chip opens a full-height modal editor; on save we write back into the block and refresh the
  // inline token's "N lines" label so chip and token stay in sync.
  let editingBlockId = $state<string | null>(null);
  let editingText = $state("");
  let editorTextareaEl: HTMLTextAreaElement | undefined = $state();
  let editingBlock = $derived(pastedBlocks.find((b) => b.id === editingBlockId) ?? null);

  function openPasteEditor(id: string) {
    const blk = pastedBlocks.find((b) => b.id === id);
    if (!blk) return;
    editingBlockId = id;
    editingText = blk.text;
    dbg("prompt", "paste-edit-open", { id, lines: blk.lineCount });
    requestAnimationFrame(() => editorTextareaEl?.focus());
  }

  function cancelPasteEditor() {
    editingBlockId = null;
    editingText = "";
  }

  function savePasteEditor() {
    if (editingBlockId == null) return;
    const id = editingBlockId;
    const text = editingText;
    if (!text.trim()) {
      // Emptied out → treat as removal (drops chip + token).
      removePastedBlock(id);
      cancelPasteEditor();
      return;
    }
    const lines = text.split("\n");
    const lineCount = lines.length;
    const charCount = text.length;
    const firstLine = lines[0].trim();
    const preview = firstLine.length > 40 ? firstLine.slice(0, 40) + "..." : firstLine;
    let seq: number | undefined;
    pastedBlocks = pastedBlocks.map((b) => {
      if (b.id !== id) return b;
      seq = b.seq;
      return { ...b, text, lineCount, charCount, preview };
    });
    // Refresh the token label so its "N lines" matches the edited content (token-anchored only).
    if (seq != null) {
      const label = t("prompt_pasteToken", {
        seq: String(seq),
        size: formatPasteSize(lineCount, charCount),
      });
      inputText = inputText.replace(singlePasteTokenRe(seq), buildPasteToken(label));
    }
    dbg("prompt", "paste-edit-save", { id, lines: lineCount });
    cancelPasteEditor();
  }

  function handlePasteEditorKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      cancelPasteEditor();
    } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      e.stopPropagation();
      savePasteEditor();
    }
  }

  function handleSkillSelect(skillName: string) {
    // Codex: plain "/name" text does NOT trigger a skill — it must be sent as a structured
    // {type:"skill", name, path} UserInput. So record the picked ref (with path from the runtime
    // list) as a chip and send it via onSend's skills arg, rather than filling the textarea.
    if (agent === "codex") {
      const ref = codexSkillItems.find((s) => s.name === skillName);
      if (!ref) {
        dbgWarn("skills", "picked codex skill not in runtime list", { skillName });
        return; // never attach a skill we lack a path for — it couldn't be sent
      }
      if (pendingSkills.some((s) => s.name === ref.name)) return; // already attached
      pendingSkills = [...pendingSkills, { name: ref.name, path: ref.path }];
      dbg("skills", "codex skill picked", { name: ref.name });
      requestAnimationFrame(() => textareaEl?.focus());
      return;
    }
    // Claude uses /name, while Pi namespaces discovered skills as /skill:name.
    dbg("prompt", "skill-select fill", { skillName });
    const commandName = agent === "pi" ? getPiSkillCommandName(skillName) : skillName;
    inputText = `/${commandName} `;
    requestAnimationFrame(() => {
      autoResize();
      textareaEl?.focus();
    });
  }

  function removeSkill(name: string) {
    pendingSkills = pendingSkills.filter((s) => s.name !== name);
    dbg("skills", "codex skill removed", { name });
  }

  function autoResize() {
    if (!textareaEl) return;
    textareaEl.style.height = "auto";
    const maxHeight = 4 * 24; // ~4 lines
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, maxHeight) + "px";
  }

  // ── Drag-drop state ──
  let dragCounter = $state(0);
  let dragActive = $derived(dragCounter > 0);

  function handleDragEnter(e: DragEvent) {
    if (!attachmentsAvailable) return;
    e.preventDefault();
    dragCounter++;
  }

  function handleDragLeave(e: DragEvent) {
    if (!attachmentsAvailable) return;
    e.preventDefault();
    dragCounter--;
  }

  function handleDragOver(e: DragEvent) {
    if (!attachmentsAvailable) return;
    e.preventDefault();
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    dragCounter = 0;
    if (!attachmentsAvailable || disabled) return;
    const files = e.dataTransfer?.files;
    if (!files || files.length === 0) return;
    processFiles(files);
  }

  let canSend = $derived(
    !disabled &&
      (!!inputText.trim() ||
        pastedBlocks.length > 0 ||
        pendingAttachments.some((a) => a.filePath) ||
        pendingPathRefs.length > 0),
  );

  // ── Mode, Project & Workspace dropdown outside-click + Escape ──
  onMount(() => {
    function onDocClick(e: MouseEvent) {
      const target = e.target as Node;
      const clickedTrigger =
        permissionBtnEl?.contains(target) === true ||
        projectPickerBtnEl?.contains(target) === true ||
        wsPickerBtnEl?.contains(target) === true;
      if (
        modeDropdownOpen &&
        !clickedTrigger &&
        modeDropdownEl &&
        !modeDropdownEl.contains(target)
      ) {
        modeDropdownOpen = false;
      }
      if (wsDropdownOpen && !clickedTrigger && wsDropdownEl && !wsDropdownEl.contains(target)) {
        wsDropdownOpen = false;
      }
      if (
        projectDropdownOpen &&
        !clickedTrigger &&
        projectDropdownEl &&
        !projectDropdownEl.contains(target)
      ) {
        projectDropdownOpen = false;
      }
      if (
        branchDropdownOpen &&
        !branchBtnEl?.contains(target) &&
        !branchDropdownEl?.contains(target)
      ) {
        branchDropdownOpen = false;
      }
    }
    function onDocKeydown(e: KeyboardEvent) {
      if (modeDropdownOpen && e.key === "Escape") {
        modeDropdownOpen = false;
      }
      if (branchDropdownOpen && e.key === "Escape") {
        branchDropdownOpen = false;
      }
      if (wsDropdownOpen && e.key === "Escape") {
        wsDropdownOpen = false;
      }
      if (projectDropdownOpen && e.key === "Escape") {
        projectDropdownOpen = false;
      }
      if (branchDropdownOpen && e.key === "Escape") {
        branchDropdownOpen = false;
      }
    }
    document.addEventListener("mousedown", onDocClick, true);
    document.addEventListener("keydown", onDocKeydown);
    return () => {
      document.removeEventListener("mousedown", onDocClick, true);
      document.removeEventListener("keydown", onDocKeydown);
    };
  });

  export function focus() {
    textareaEl?.focus();
  }

  export function setValue(text: string) {
    inputText = text;
    requestAnimationFrame(() => {
      autoResize();
      textareaEl?.focus();
    });
  }

  export function appendText(text: string) {
    inputText = inputText ? inputText + "\n" + text : text;
    requestAnimationFrame(() => {
      autoResize();
      textareaEl?.focus();
    });
  }

  export function triggerSend() {
    handleSend();
  }

  export function addFiles(files: FileList | File[]) {
    return processFiles(files);
  }

  export function addPathRefs(refs: Array<{ path: string; name: string; isDir: boolean }>) {
    const newRefs = refs.map((ref) => ({
      id: uuid().slice(0, 8),
      name: ref.name,
      path: ref.path,
      isDir: ref.isDir,
    }));
    pendingPathRefs = [...pendingPathRefs, ...newRefs];
    dbg("prompt", "add-path-refs", { count: refs.length });
  }

  function removePathRef(id: string) {
    pendingPathRefs = pendingPathRefs.filter((r) => r.id !== id);
  }

  export function showToast(message: string, variant: "error" | "info" = "info") {
    showFileToast(message, variant);
  }

  export function getInputSnapshot(): PromptInputSnapshot {
    return {
      text: inputText,
      attachments: [...pendingAttachments],
      pastedBlocks: [...pastedBlocks],
      pathRefs: [...pendingPathRefs],
      promptTemplateTokens: [...promptTemplateTokens],
    };
  }

  export function restoreSnapshot(snapshot: PromptInputSnapshot): void {
    inputText = snapshot.text;
    pendingAttachments = snapshot.attachments;
    pastedBlocks = snapshot.pastedBlocks as PastedBlock[];
    pendingPathRefs = snapshot.pathRefs ?? [];
    promptTemplateTokens = snapshot.promptTemplateTokens ?? [];
    resetHistory(histState);
    requestAnimationFrame(() => {
      autoResize();
      textareaEl?.focus();
    });
  }

  export function clearAll(): void {
    inputText = "";
    pendingAttachments = [];
    pendingPathRefs = [];
    pastedBlocks = [];
    promptTemplateTokens = [];
    resetHistory(histState);
    requestAnimationFrame(() => autoResize());
  }

  function hasContent(): boolean {
    return !!(
      inputText.trim() ||
      pendingAttachments.length ||
      pastedBlocks.length ||
      pendingPathRefs.length
    );
  }
</script>

<!-- Web drag handlers — only fire when Tauri dragDropEnabled is false (non-Tauri builds).
     When dragDropEnabled: true, Tauri intercepts OS drag events and Web drag events do not fire. -->
<div
  class="px-4 pb-5 pt-2 relative mx-auto w-full max-w-4xl"
  ondragenter={handleDragEnter}
  ondragleave={handleDragLeave}
  ondragover={handleDragOver}
  ondrop={handleDrop}
>
  <!-- Drag overlay -->
  {#if dragActive}
    <div
      class="absolute inset-0 z-10 flex items-center justify-center rounded-[18px] border-2 border-dashed border-foreground/30 bg-foreground/5 backdrop-blur-[1px]"
    >
      <span class="text-sm font-medium text-foreground/60">{t("prompt_dropFiles")}</span>
    </div>
  {/if}

  <!-- File toast -->
  {#if toastMessage}
    <div
      class="absolute -top-10 left-4 right-4 z-20 flex items-center gap-2 rounded-md px-3 py-1.5 text-xs shadow-lg animate-fade-in {toastVariant ===
      'error'
        ? 'bg-destructive/90 text-destructive-foreground'
        : 'bg-muted text-foreground'}"
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
        <circle cx="12" cy="12" r="10" /><path d="M12 8v4" /><path d="M12 16h.01" />
      </svg>
      <span>{toastMessage}</span>
    </div>
  {/if}

  <!-- Composer-owned follow-up queue. It stays above the input and is intentionally compact:
       each row is independently actionable, editable, or removable. -->
  {#if (agent === "pi" || agent === "claude" || agent === "dsh") && queuedMessages.length > 0}
    <div class="mb-2 overflow-hidden rounded-xl border border-border/70 bg-muted/20 shadow-sm">
      <div
        class="flex items-center justify-between border-b border-border/50 px-3 py-1.5 text-[11px] text-muted-foreground"
      >
        <span class="font-medium text-foreground/80">{queuedMessages.length} 条排队消息</span>
        <span>任务完成后按顺序发送</span>
      </div>
      <div class="divide-y divide-border/50">
        {#each queuedMessages as queued (queued.id)}
          <div class="group flex min-h-9 items-center gap-2 px-3 py-1.5 text-xs">
            <span class="min-w-0 flex-1 truncate text-foreground/85" title={queued.text}
              >{queued.text}</span
            >
            <div
              class="flex shrink-0 items-center gap-0.5 opacity-70 transition-opacity group-hover:opacity-100"
            >
              <button
                type="button"
                class="rounded-md px-2 py-1 text-[11px] font-medium text-primary hover:bg-primary/10"
                onclick={() => handleQueueSteer(queued.id)}
                disabled={queueSteerInFlight !== null || !queueActionAvailable}
                title={!queueActionAvailable
                  ? "当前会话尚未确认支持立即引导"
                  : agent === "claude"
                    ? "中断当前任务并发送这条消息"
                    : "立即引导当前任务"}
                >{queueSteerInFlight === queued.id
                  ? queueActionProgressLabel
                  : queueActionLabel}</button
              >
              <button
                type="button"
                class="flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
                onclick={() => onQueueEdit?.(queued.id)}
                title="编辑消息"
                aria-label="编辑消息"
              >
                <svg
                  class="h-3.5 w-3.5"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.8"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <path d="M12 20h9" /><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z" />
                </svg>
              </button>
              <button
                type="button"
                class="flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                onclick={() => onQueueDelete?.(queued.id)}
                title="删除消息"
                aria-label="删除消息"
              >
                <svg
                  class="h-3.5 w-3.5"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.8"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <path d="M3 6h18" /><path d="M8 6V4h8v2" /><path d="m19 6-1 14H6L5 6" /><path
                    d="M10 11v5M14 11v5"
                  />
                </svg>
              </button>
            </div>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Attachment & paste block previews -->
  {#if pendingAttachments.length > 0 || pastedBlocks.length > 0 || pendingPathRefs.length > 0 || pendingSkills.length > 0 || promptTemplateTokens.length > 0}
    <div class="mb-2 flex flex-wrap gap-1.5">
      {#each promptTemplateTokens as template, index (index + template.token)}
        <span
          class="inline-flex items-center gap-1.5 rounded-md border border-purple-200 bg-purple-50 px-2 py-1 text-xs text-purple-700 dark:border-purple-800 dark:bg-purple-950/50 dark:text-purple-300"
        >
          <svg
            class="h-3.5 w-3.5 shrink-0"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
            <polyline points="14 2 14 8 20 8" />
            <path d="M8 13h8M8 17h5" />
          </svg>
          <span class="font-mono font-medium">/{template.name}</span>
          <button
            type="button"
            class="ml-0.5 rounded p-0.5 transition-colors hover:bg-purple-200/50 dark:hover:bg-purple-800/50"
            onclick={() => removePromptTemplate(index)}
            title={t("prompt_templatePicker")}
            aria-label={t("prompt_templatePicker")}
          >
            <svg
              class="h-3 w-3"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="M18 6 6 18M6 6l12 12" />
            </svg>
          </button>
        </span>
      {/each}
      {#each pendingSkills as skill (skill.name)}
        <!-- Picked Codex skill — sent as a structured {type:"skill"} ref, not text. -->
        <span
          class="inline-flex items-center gap-1.5 rounded-md border border-violet-200 dark:border-violet-800 bg-violet-50 dark:bg-violet-950/50 text-violet-700 dark:text-violet-300 px-2 py-1 text-xs"
        >
          <!-- Sparkles icon (matches SkillSelector) -->
          <svg
            class="h-3.5 w-3.5 shrink-0"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path
              d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z"
            />
          </svg>
          <span class="font-medium">{skill.name}</span>
          <button
            onclick={() => removeSkill(skill.name)}
            class="ml-0.5 rounded p-0.5 transition-colors hover:bg-violet-200/50 dark:hover:bg-violet-800/50"
            title={t("prompt_removeSkill")}
          >
            <svg
              class="h-3 w-3"
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
        </span>
      {/each}
      {#each pendingAttachments as att (att.id)}
        <FileAttachment
          name={att.name}
          size={att.size}
          mimeType={att.type}
          contentBase64={att.contentBase64}
          isPathRef={!!att.filePath && !att.contentBase64}
          onremove={() => removeAttachment(att.id)}
        />
      {/each}
      {#each pendingPathRefs as ref (ref.id)}
        <FileAttachment
          name={ref.name}
          size={0}
          mimeType={ref.isDir ? "inode/directory" : "application/octet-stream"}
          isPathRef={true}
          onremove={() => removePathRef(ref.id)}
        />
      {/each}
      {#each pastedBlocks as block (block.id)}
        {@const isSpreadsheet = block.ext ? isSpreadsheetExt(block.ext) : false}
        <span
          class="inline-flex items-center gap-1.5 rounded-md border border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-950/50 text-blue-700 dark:text-blue-300 px-2 py-1 text-xs"
        >
          <!-- Click the chip body to open the full editor (issue #156) -->
          <button
            type="button"
            onclick={() => openPasteEditor(block.id)}
            class="inline-flex min-w-0 items-center gap-1.5 hover:underline"
            title={t("prompt_editPaste")}
          >
            {#if isSpreadsheet}
              <!-- Table/spreadsheet icon -->
              <svg
                class="h-3.5 w-3.5 shrink-0"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
                <path d="M3 9h18" /><path d="M3 15h18" /><path d="M9 3v18" />
              </svg>
            {:else}
              <!-- Clipboard icon for text -->
              <svg
                class="h-3.5 w-3.5 shrink-0"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <rect width="8" height="4" x="8" y="2" rx="1" ry="1" /><path
                  d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"
                />
              </svg>
            {/if}
            <span class="truncate max-w-[200px]">{block.preview}</span>
            <span class="text-blue-400 dark:text-blue-500"
              >{formatPasteSize(block.lineCount, block.charCount)}</span
            >
          </button>
          <button
            onclick={() => removePastedBlock(block.id)}
            class="rounded p-0.5 transition-colors hover:bg-blue-200/50 dark:hover:bg-blue-800/50"
            title={t("prompt_removePaste")}
          >
            <svg
              class="h-3 w-3"
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
        </span>
      {/each}
    </div>
  {/if}

  <!-- Unified input container - Modern floating island workbench -->
  <div
    class="prompt-composer ui-composer rounded-[18px] border border-border/70 bg-background shadow-[0_2px_14px_rgba(0,0,0,0.04)] transition-all {btwMode
      ? 'border-primary/50'
      : ''}"
  >
    <!-- Context row (Codex style): the agent / project / workspace / branch chips sit in a
         muted strip above the input instead of being crowded into the bottom action bar. -->
    <div class="composer-context-row">
      {#if projectPicker}
        <div class="relative inline-flex items-center">
          {#if projectPicker.readOnly}
            <div
              class="inline-flex items-center gap-1.5 rounded-md px-1.5 py-1 text-xs text-muted-foreground"
              title={projectPicker.currentProjectCwd || t("prompt_projectNotSelected")}
            >
              <svg
                class="h-3.5 w-3.5 shrink-0 text-muted-foreground/70"
                viewBox="0 0 24 24"
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
              <span class="truncate max-w-[140px] font-medium"
                >{projectPicker.currentStandaloneTask
                  ? "独立任务"
                  : projectPicker.currentProjectName || t("prompt_projectNotSelected")}</span
              >
            </div>
          {:else}
            <button
              bind:this={projectPickerBtnEl}
              type="button"
              class="inline-flex items-center gap-1 rounded-md px-1.5 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
              onclick={toggleProjectDropdown}
              title={projectPicker.currentProjectCwd || t("prompt_selectProject")}
              aria-label={projectPicker.currentStandaloneTask
                ? "独立任务"
                : projectPicker.currentProjectName || t("prompt_selectProject")}
              aria-expanded={projectDropdownOpen}
              aria-haspopup="menu"
            >
              <svg
                class="h-3.5 w-3.5 shrink-0 text-muted-foreground/70"
                viewBox="0 0 24 24"
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
              <span class="truncate max-w-[140px] font-medium"
                >{projectPicker.currentStandaloneTask
                  ? "独立任务"
                  : projectPicker.currentProjectName || t("prompt_selectProject")}</span
              >
              <svg
                class="h-2.5 w-2.5 opacity-50"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"><path d="m6 9 6 6 6-6" /></svg
              >
            </button>
          {/if}
        </div>
      {/if}

      {#if workspacePicker}
        <div class="relative inline-flex items-center">
          {#if workspacePicker.readOnly}
            <div
              class="inline-flex items-center gap-1.5 rounded-md px-1.5 py-1 text-xs text-muted-foreground"
              title={workspacePicker.currentWorkspaceId
                ? "当前对话已绑定该工作空间"
                : "独立任务（无工作空间）"}
            >
              {#if workspacePicker.currentWorkspaceId}
                <svg
                  class="h-3.5 w-3.5 shrink-0 text-muted-foreground/70"
                  viewBox="0 0 24 24"
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
                <span class="truncate max-w-[140px] font-medium"
                  >{workspacePicker.currentWorkspaceName}</span
                >
              {:else}
                <span class="text-xs shrink-0 opacity-70">⚡</span>
                <span class="truncate max-w-[140px] font-medium">独立任务</span>
              {/if}
            </div>
          {:else}
            <button
              bind:this={wsPickerBtnEl}
              type="button"
              class="inline-flex items-center gap-1 rounded-md px-1.5 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
              onclick={toggleWsDropdown}
              title="选择任务工作空间"
            >
              {#if workspacePicker.currentWorkspaceId}
                <svg
                  class="h-3.5 w-3.5 shrink-0 text-muted-foreground/70"
                  viewBox="0 0 24 24"
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
                <span class="truncate max-w-[140px] font-medium"
                  >{workspacePicker.currentWorkspaceName || "选择工作空间"}</span
                >
              {:else}
                <span class="text-xs shrink-0 opacity-70">⚡</span>
                <span class="truncate max-w-[140px] font-medium">独立任务</span>
              {/if}
              <svg
                class="h-2.5 w-2.5 opacity-50"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"><path d="m6 9 6 6 6-6" /></svg
              >
            </button>
          {/if}
        </div>
      {/if}

      {#if gitBranch && !projectPicker?.currentStandaloneTask && !(workspacePicker && !workspacePicker.currentWorkspaceId)}
        <div class="relative inline-flex items-center">
          <button
            bind:this={branchBtnEl}
            type="button"
            class="inline-flex max-w-[min(28vw,12rem)] items-center gap-1 rounded-md px-1.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            onclick={openBranchDropdown}
            title={gitBranch}
            aria-label="切换 Git 分支"
            aria-expanded={branchDropdownOpen}
          >
            <svg
              class="h-3.5 w-3.5 shrink-0 opacity-70"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <line x1="6" y1="3" x2="6" y2="15" />
              <circle cx="18" cy="6" r="3" />
              <circle cx="6" cy="18" r="3" />
              <path d="M18 9a9 9 0 0 1-9 9" />
            </svg>
            <span class="truncate font-medium">{displayBranch}</span>
            <svg
              class="h-2.5 w-2.5 shrink-0 opacity-50 transition-transform {branchDropdownOpen
                ? 'rotate-180'
                : ''}"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg
            >
          </button>

          {#if branchDropdownOpen}
            <div
              bind:this={branchDropdownEl}
              data-composer-menu
              class="absolute left-0 bottom-full mb-2 z-50 w-72 rounded-2xl border border-border/80 bg-background/95 p-2.5 shadow-xl backdrop-blur-md animate-in fade-in slide-in-from-bottom-2 duration-150 text-left"
            >
              <!-- Search input -->
              <div class="relative mb-2">
                <svg
                  class="absolute left-2.5 top-2.5 h-3.5 w-3.5 text-muted-foreground/60"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"><circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" /></svg
                >
                <input
                  type="text"
                  bind:value={branchSearchQuery}
                  placeholder="搜索分支"
                  class="w-full rounded-xl bg-accent/50 pl-8 pr-3 py-1.5 text-xs text-foreground placeholder:text-muted-foreground/60 focus:outline-none focus:ring-1 focus:ring-primary/40 border border-border/40"
                />
              </div>

              <!-- Header -->
              <div
                class="px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60"
              >
                分支
              </div>

              <!-- Branch list -->
              <div class="max-h-52 overflow-y-auto space-y-0.5 my-1 scrollbar-hide">
                {#if branchLoading}
                  <div
                    class="flex items-center justify-center py-4 text-xs text-muted-foreground/60"
                  >
                    加载分支列表...
                  </div>
                {:else}
                  {@const filteredBranches = branchList.filter((b) =>
                    b.name.toLowerCase().includes(branchSearchQuery.toLowerCase()),
                  )}
                  {#each filteredBranches as b (b.name)}
                    {@const isCheckedOutElsewhere = !!b.worktree_path && b.name !== gitBranch}
                    <button
                      class="flex w-full items-center justify-between rounded-xl px-2.5 py-1.5 text-left text-xs transition-colors {isCheckedOutElsewhere
                        ? 'cursor-not-allowed text-muted-foreground/45'
                        : b.name === gitBranch
                          ? 'bg-accent font-medium text-foreground'
                          : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground'}"
                      disabled={isCheckedOutElsewhere}
                      title={isCheckedOutElsewhere
                        ? t("prompt_gitBranchCheckedOut", {
                            branch: b.name,
                            path: b.worktree_path ?? "",
                          })
                        : undefined}
                      onclick={() => handleSelectBranch(b)}
                    >
                      <div class="flex items-center gap-2 min-w-0">
                        <svg
                          class="h-3.5 w-3.5 shrink-0 opacity-70"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="2"
                          ><line x1="6" y1="3" x2="6" y2="15" /><circle
                            cx="18"
                            cy="6"
                            r="3"
                          /><circle cx="6" cy="18" r="3" /><path d="M18 9a9 9 0 0 1-9 9" /></svg
                        >
                        <div class="min-w-0">
                          <span class="truncate block">{b.name}</span>
                          {#if b.is_current && b.uncommitted_count > 0}
                            <span class="text-[10px] text-amber-500 font-normal block"
                              >未提交: {b.uncommitted_count} 个文件</span
                            >
                          {/if}
                          {#if isCheckedOutElsewhere}
                            <span class="text-[10px] font-normal block truncate"
                              >{t("prompt_gitBranchUnavailableAt", {
                                path: b.worktree_path ?? "",
                              })}</span
                            >
                          {/if}
                        </div>
                      </div>
                      {#if b.name === gitBranch}
                        <svg
                          class="h-3.5 w-3.5 text-primary shrink-0"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="2.5"><polyline points="20 6 9 17 4 12" /></svg
                        >
                      {/if}
                    </button>
                  {:else}
                    <div class="py-3 text-center text-xs text-muted-foreground/50">无匹配分支</div>
                  {/each}
                {/if}
              </div>

              <!-- Bottom action: Create & Checkout New Branch -->
              <div class="border-t border-border/40 pt-1.5 mt-1">
                {#if branchCreatingMode}
                  <div class="flex items-center gap-1.5 px-1 py-1">
                    <input
                      type="text"
                      bind:value={newBranchInput}
                      placeholder="新分支名称"
                      class="flex-1 rounded-lg bg-accent/60 px-2 py-1 text-xs text-foreground placeholder:text-muted-foreground/60 focus:outline-none border border-border/50"
                      onkeydown={(e) => {
                        if (e.key === "Enter") handleCreateBranch();
                        if (e.key === "Escape") branchCreatingMode = false;
                      }}
                    />
                    <button
                      class="rounded-lg bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground hover:opacity-90 transition-opacity"
                      onclick={handleCreateBranch}
                    >
                      创建
                    </button>
                  </div>
                {:else}
                  <button
                    class="flex w-full items-center gap-2 rounded-xl px-2.5 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-accent/50 transition-colors"
                    onclick={() => (branchCreatingMode = true)}
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
                    <span>创建并检出新分支...</span>
                  </button>
                {/if}
              </div>
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Textarea -->
    <textarea
      bind:this={textareaEl}
      bind:value={inputText}
      onkeydown={handleKeydown}
      oninput={handleInput}
      onpaste={handlePaste}
      oncompositionstart={handleCompositionStart}
      oncompositionend={handleCompositionEnd}
      placeholder={effectivePlaceholder}
      rows={2}
      {disabled}
      class="w-full resize-none border-0 bg-transparent px-4 pt-3 pb-2 text-[var(--chat-font-size,15px)] leading-[var(--chat-line-height,1.62)] text-foreground placeholder:text-[var(--chat-text-tertiary,#9298a1)] shadow-none outline-none ring-0 focus:border-0 focus:outline-none focus:ring-0 focus:shadow-none focus-visible:border-0 focus-visible:outline-none focus-visible:ring-0 focus-visible:shadow-none disabled:opacity-50"
      style="min-height: 3.5rem;"
    ></textarea>

    {#if atMenuOpen}
      <AtMentionMenu
        entries={atResults}
        selectedIndex={atSelectedIndex}
        loading={atLoading}
        query={atQuery}
        anchorEl={textareaEl}
        onSelect={selectAtEntry}
        onHover={(i) => (atSelectedIndex = i)}
        onDismiss={() => closeAtMenu("click-outside")}
      />
    {/if}

    {#if slashMenuOpen}
      <SlashMenu
        commands={filteredCommands}
        {slashGroups}
        selectedIndex={slashSelectedIndex}
        anchorEl={textareaEl}
        phase={slashPhase}
        {models}
        {currentModel}
        subSelectedIndex={slashSubSelectedIndex}
        {hintText}
        inputDisplay={inputText}
        {fastModeState}
        commandCatalogPending={nativeCommands.length === 0 && slashQuery === ""}
        onSelect={(cmd) => selectSlashCommand(cmd, "enter")}
        onHover={(i) => (slashSelectedIndex = i)}
        onSubHover={(i) => (slashSubSelectedIndex = i)}
        onSubSelect={handleSubModelSelect}
        onFastSelect={handleFastModeSelect}
        onBack={goBackToCommands}
        onDismiss={() => closeSlashMenu("click-outside")}
      />
    {/if}

    <!-- Bottom action bar -->
    <div class="flex items-center justify-between px-2 pb-2">
      <!-- Left: plan mode + permission + rules + skills -->
      <div class="flex items-center gap-1 min-w-0 flex-wrap sm:flex-nowrap">
        <WorkBuddyCascadingMenu
          bind:open={cascadingMenuOpen}
          disabled={disabled || running}
          planAvailable={agent === "pi" ? planAvailable : planAvailable || !!onPlanModeChange}
          planActive={featurePlanActive}
          goalAvailable={agent === "pi" ? goalAvailable : goalAvailable || !!onOpenGoal}
          goalActive={featureGoalActive}
          {selectedExpert}
          onSelectExpert={(exp) => {
            selectedExpert = exp;
          }}
          onSelectSkill={(skillName) => {
            handleSkillSelect(skillName);
          }}
          onSelectPromptTemplate={(template) => {
            insertPromptTemplateFromMenu(template);
          }}
          onPlanToggle={() => {
            if (agent === "pi") {
              onPiPlan?.(piPlan?.phase === "active" ? "exit" : "enter");
            } else {
              onPlanModeChange?.(!planActive);
            }
          }}
          onGoalToggle={() => {
            if (agent === "pi") onPiGoal?.();
            else onOpenGoal?.();
          }}
          onTriggerUpload={() => {
            fileInput?.click();
          }}
          onInsertAtMention={() => {
            insertAtMentionChar();
          }}
          onCaptureScreenshot={() => {
            void api.captureScreenshot();
          }}
          onDismiss={() => {
            cascadingMenuOpen = false;
          }}
        />

        {#if selectedExpert}
          {@const initial = (selectedExpert.title || "专").slice(0, 1)}
          <div
            class="flex items-center gap-1.5 rounded-lg border border-primary/30 bg-primary/10 px-2 py-0.5 text-xs text-primary font-medium animate-in fade-in duration-150"
          >
            <div
              class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full {selectedExpert.isTeam
                ? 'bg-indigo-600'
                : 'bg-rose-500'} text-[9px] font-bold text-white shadow-xs"
            >
              {selectedExpert.icon &&
              selectedExpert.icon.length === 1 &&
              !/\s/.test(selectedExpert.icon)
                ? selectedExpert.icon
                : initial}
            </div>
            <button
              type="button"
              class="cursor-pointer hover:underline truncate max-w-[130px] text-left text-foreground font-medium"
              onclick={() => (cascadingMenuOpen = true)}
              title="点击切换专家"
            >
              {selectedExpert.title}
            </button>
            <button
              type="button"
              class="flex items-center justify-center text-muted-foreground/60 hover:text-destructive transition-colors cursor-pointer p-0.5 rounded-sm"
              onclick={(e) => {
                e.stopPropagation();
                selectedExpert = null;
                onExpertClear?.();
              }}
              title="移除专家"
            >
              <svg
                class="h-3 w-3"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M18 6 6 18M6 6l12 12" />
              </svg>
            </button>
          </div>
        {/if}

        {#if featurePlanActive}
          <div
            class="flex items-center rounded-md border border-purple-500/30 bg-purple-500/10 text-purple-500"
          >
            <button
              type="button"
              class="flex items-center gap-1 rounded-l-md px-1.5 py-1 text-[11px] font-medium hover:bg-purple-500/10"
              onclick={() => (agent === "pi" ? onPiPlan?.("exit") : onPlanModeChange?.(false))}
              disabled={disabled || running || piFeaturePending !== null}
              title="打开计划设置"
            >
              <svg
                class="h-3 w-3"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                aria-hidden="true"
              >
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><polyline
                  points="14 2 14 8 20 8"
                />
              </svg>
              <span>计划</span>
            </button>
            <button
              type="button"
              class="flex h-6 w-6 items-center justify-center rounded-r-md text-purple-500/70 hover:bg-purple-500/20 hover:text-purple-500"
              onclick={() => void onClearPlan?.()}
              disabled={disabled || running || piFeaturePending !== null}
              aria-label="取消计划模式"
              title="取消计划模式"
            >
              <svg
                class="h-3 w-3"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg
              >
            </button>
          </div>
        {/if}

        {#if featureGoalActive}
          <div
            class="flex items-center rounded-md border border-emerald-500/30 bg-emerald-500/10 text-emerald-500"
          >
            <button
              type="button"
              class="flex items-center gap-1 rounded-l-md px-1.5 py-1 text-[11px] font-medium hover:bg-emerald-500/10"
              onclick={() => (agent === "pi" ? onPiGoal?.() : onOpenGoal?.())}
              disabled={disabled || running || piFeaturePending !== null}
              title="打开目标设置"
            >
              <svg
                class="h-3 w-3"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                aria-hidden="true"
              >
                <circle cx="12" cy="12" r="9" /><circle cx="12" cy="12" r="5" /><circle
                  cx="12"
                  cy="12"
                  r="1.5"
                />
              </svg>
              <span>目标</span>
            </button>
            <button
              type="button"
              class="flex h-6 w-6 items-center justify-center rounded-r-md text-emerald-500/70 hover:bg-emerald-500/20 hover:text-emerald-500"
              onclick={() => void onClearGoal?.()}
              disabled={disabled || running || piFeaturePending !== null}
              aria-label="清除目标"
              title="清除目标"
            >
              <svg
                class="h-3 w-3"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg
              >
            </button>
          </div>
        {/if}

        {#if nativeModeSelector}
          <label
            class="flex items-center gap-1 rounded-md border px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground"
          >
            <span class="sr-only">会话模式</span>
            <select
              class="max-w-[9rem] bg-transparent text-[11px] text-foreground outline-none"
              value={sessionMode}
              disabled={disabled || running}
              onchange={(event) => {
                void onSessionModeChange?.(event.currentTarget.value);
              }}
              title="选择会话模式"
            >
              {#each sessionModes as mode (mode.id)}
                <option value={mode.id}>{mode.name || mode.id}</option>
              {/each}
            </select>
          </label>
        {/if}

        {#if permissionSelectorAvailable}
          <button
            bind:this={permissionBtnEl}
            disabled={permissionSelectorDisabled}
            aria-haspopup="menu"
            aria-expanded={modeDropdownOpen}
            class="flex items-center gap-1 rounded-md border border-transparent px-1.5 py-1 text-xs font-medium text-muted-foreground hover:border-border hover:bg-accent hover:text-foreground transition-colors
              {permissionSelectorBusy
              ? 'cursor-wait opacity-60'
              : permissionSelectorDisabled
                ? 'cursor-not-allowed opacity-40'
                : ''}"
            onclick={togglePermissionDropdown}
            title={permissionSelectorTitle}
          >
            <svg
              class="h-3 w-3"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path
                d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"
              />
            </svg>
            <span>{permissionSelectorLabel}</span>
            <svg
              class="h-2.5 w-2.5 opacity-50"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="m6 9 6 6 6-6" />
            </svg>
          </button>
        {/if}
        {#if agent === "pi" && onPiTree}
          <button
            class="flex items-center gap-1 rounded-md border border-transparent px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground transition-colors hover:border-border hover:bg-accent hover:text-foreground"
            onclick={onPiTree}
            title="打开 Pi 会话树"
          >
            <svg
              class="h-3 w-3 opacity-70"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <circle cx="6" cy="5" r="2" />
              <circle cx="18" cy="12" r="2" />
              <circle cx="6" cy="19" r="2" />
              <path d="M8 5h3a4 4 0 0 1 4 4v1" />
              <path d="M8 19h3a4 4 0 0 0 4-4v-1" />
            </svg>
            <span>Tree</span>
          </button>
        {/if}
        {#if showAuthBadge && !hasRun && agent === "claude"}
          <!-- Legacy composer auth badge (Claude only). The hero's AgentAuthBadge is the
               canonical auth surface now; this is slated for removal post hero-migration and
               is skipped for Codex (it would show wrong Anthropic/version data). -->
          <AuthSourceBadge
            {authOverview}
            {authSourceLabel}
            {authSourceCategory}
            {apiKeySource}
            {hasRun}
            {authMode}
            {platformCredentials}
            {platformId}
            {onAuthModeChange}
            {onPlatformChange}
            {localProxyStatuses}
          />
        {/if}
        {#if hasStash && onRestoreStash}
          <button
            class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] font-medium bg-violet-500/15 text-violet-400 hover:bg-violet-500/25 transition-colors"
            title={t("prompt_stashRestore")}
            {disabled}
            onclick={onRestoreStash}
          >
            <svg
              class="h-3 w-3"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path d="M3 3v5h5" />
            </svg>
            {t("prompt_stashBadge")}
          </button>
        {/if}
      </div>

      <!-- Right: actions -->
      <div class="flex items-center gap-0.5">
        {#if (currentModel || models.length > 0) && (onModelSwitch || onEffortChange)}
          <CompactModelPicker
            {agent}
            {models}
            {currentModel}
            {projectDefaultModel}
            {projectDefaultLabel}
            {setProjectDefaultLabel}
            {currentEffort}
            {effortOptions}
            {fastModeState}
            {onModelSwitch}
            {onSetProjectDefault}
            {onEffortChange}
            {onFastModeSwitch}
          />
        {/if}

        <input
          bind:this={fileInput}
          type="file"
          multiple
          accept="image/png,image/jpeg,image/webp,image/gif,application/pdf,.txt,.md,.json,.ts,.tsx,.js,.jsx,.py,.rs,.svelte,.html,.css,.yaml,.yml,.toml,.xml,.sh,.sql,.go,.java,.c,.cpp,.h,.rb,.php,.swift,.csv,.log,.docx,.xlsx,.pptx"
          class="hidden"
          onchange={handleFileSelect}
        />

        {#if running && onInterrupt}
          {#if canSend}
            {#if btwMode}
              <!-- BTW send: blue theme -->
              <button
                class="flex h-8 w-8 items-center justify-center rounded-full transition-colors bg-primary text-primary-foreground hover:bg-primary/90"
                onclick={handleBtwSend}
                title="Send side question"
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
                  <path d="M5 12h14" /><path d="m12 5 7 7-7 7" />
                </svg>
              </button>
            {:else}
              <!-- Mid-turn send: allow injecting a message while agent is running -->
              <button
                class="flex h-8 w-8 items-center justify-center rounded-full transition-colors bg-foreground text-background hover:bg-foreground/90"
                onclick={handleSend}
                title={t("prompt_send")}
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
                  <path d="M5 12h14" /><path d="m12 5 7 7-7 7" />
                </svg>
              </button>
            {/if}
          {/if}
          <!-- BTW toggle button (only during running) -->
          {#if btwAvailable}
            <button
              onclick={() => (btwMode = !btwMode)}
              {disabled}
              title="Side question (btw)"
              class="flex h-7 w-7 items-center justify-center rounded-lg transition-colors {btwMode
                ? 'text-primary bg-primary/10'
                : 'text-muted-foreground/60 hover:text-foreground hover:bg-accent'}"
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
                <path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" />
              </svg>
            </button>
          {/if}
          {#if onCancelTurn}
            <button
              class="flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground/70 transition-colors hover:bg-accent hover:text-foreground"
              onclick={onCancelTurn}
              disabled={cancelTurnDisabled}
              title="取消当前回合，保留会话"
              aria-label="取消当前回合，保留会话"
            >
              <svg
                class="h-4 w-4"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
              >
                <path d="M4 12a8 8 0 1 0 2.34-5.66" />
                <path d="M4 5v5h5" />
                <path d="m9 12 3 3 4-5" />
              </svg>
            </button>
          {/if}
          <button
            class="flex h-8 w-8 items-center justify-center rounded-full bg-foreground text-background hover:bg-foreground/90 transition-colors"
            onclick={onInterrupt}
            disabled={interruptDisabled}
            title={t("prompt_stop")}
          >
            <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="currentColor">
              <rect x="6" y="6" width="12" height="12" rx="2" />
            </svg>
          </button>
        {:else}
          <button
            class="flex h-8 w-8 items-center justify-center rounded-full transition-colors {canSend
              ? 'bg-foreground text-background hover:bg-foreground/90'
              : 'text-muted-foreground/40'}"
            onclick={handleSend}
            disabled={!canSend}
            title={t("prompt_send")}
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
              <path d="M5 12h14" /><path d="m12 5 7 7-7 7" />
            </svg>
          </button>
        {/if}
      </div>
    </div>
  </div>

  {#if modeDropdownOpen}
    <div
      bind:this={modeDropdownEl}
      class="min-w-[220px] w-max rounded-lg border bg-background shadow-lg animate-fade-in"
      role="menu"
      style={modeDropdownStyle}
    >
      <div class="p-1">
        {#each permissionSelectorOptions as option (option.value)}
          {@const selected = permissionSelectorValue === option.value}
          {@const optionDisabled = !!option.disabled || permissionSelectorDisabled}
          <button
            role="menuitemradio"
            aria-checked={selected}
            disabled={optionDisabled}
            title={option.description}
            class="flex w-full items-center gap-2 rounded-md px-3 py-2 text-xs transition-colors
              {optionDisabled ? 'cursor-not-allowed opacity-40' : 'hover:bg-accent'}
              {selected ? 'bg-accent font-medium' : ''}"
            onclick={() => selectPermissionOption(option.value)}
          >
            {#if selected}
              <svg
                class="h-3 w-3 text-primary shrink-0"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"><path d="M20 6 9 17l-5-5" /></svg
              >
            {:else}
              <span class="w-3 shrink-0"></span>
            {/if}
            <span class="shrink-0 {option.cls}">{option.label}</span>
            <span class="flex-1 min-w-0 text-[10px] text-foreground/50 truncate"
              >{option.description}</span
            >
          </button>
        {/each}
      </div>
    </div>
  {/if}

  {#if projectDropdownOpen && projectPicker}
    <div
      bind:this={projectDropdownEl}
      class="min-w-[220px] max-w-[320px] rounded-xl border border-border bg-popover p-1.5 shadow-xl backdrop-blur-md animate-fade-in z-50"
      style={projectDropdownStyle}
      role="menu"
    >
      <div
        class="px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground"
      >
        选择任务作用域
      </div>

      <button
        type="button"
        role="menuitemradio"
        aria-checked={projectPicker.currentStandaloneTask === true}
        class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs text-foreground transition-colors hover:bg-accent {projectPicker.currentStandaloneTask
          ? 'bg-accent/70 font-medium'
          : ''}"
        onclick={() => selectProject(null)}
      >
        <span class="shrink-0 text-sm">⚡</span>
        <span class="flex-1 truncate">独立任务（应用隔离目录）</span>
        {#if projectPicker.currentStandaloneTask}
          <svg
            class="h-3.5 w-3.5 shrink-0 text-primary"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
          >
        {/if}
      </button>
      <div class="my-1 border-t border-border/50"></div>

      {#if projectPicker.projects.length > 0}
        <div class="max-h-48 space-y-0.5 overflow-y-auto">
          {#each projectPicker.projects as project (project.cwd)}
            <button
              type="button"
              role="menuitemradio"
              aria-checked={projectPicker.currentProjectCwd === project.cwd}
              class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs text-foreground transition-colors hover:bg-accent {projectPicker.currentProjectCwd ===
              project.cwd
                ? 'bg-accent/70 font-medium'
                : ''}"
              onclick={() => selectProject(project.cwd)}
              title={project.cwd}
            >
              <svg
                class="h-3.5 w-3.5 shrink-0 text-muted-foreground"
                viewBox="0 0 24 24"
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
              <span class="flex-1 truncate">{project.name}</span>
              {#if projectPicker.currentProjectCwd === project.cwd}
                <svg
                  class="h-3.5 w-3.5 shrink-0 text-primary"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <path d="M20 6 9 17l-5-5" />
                </svg>
              {/if}
            </button>
          {/each}
        </div>
      {:else}
        <div class="px-2.5 py-2 text-xs text-muted-foreground">
          {t("prompt_noProjects")}
        </div>
      {/if}
    </div>
  {/if}

  {#if wsDropdownOpen && workspacePicker}
    <div
      bind:this={wsDropdownEl}
      class="min-w-[220px] max-w-[320px] rounded-xl border border-border bg-popover p-1.5 shadow-xl backdrop-blur-md animate-fade-in z-50"
      style={wsDropdownStyle}
      role="menu"
    >
      <div
        class="px-2 py-1 text-[10px] font-semibold text-muted-foreground uppercase tracking-wider"
      >
        选择工作空间
      </div>
      <button
        type="button"
        class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-xs text-left text-foreground hover:bg-accent transition-colors {!workspacePicker.currentWorkspaceId
          ? 'bg-accent/70 font-medium'
          : ''}"
        onclick={() => {
          workspacePicker?.onSelect(null);
          wsDropdownOpen = false;
        }}
      >
        <span class="text-sm">⚡</span>
        <span class="truncate flex-1">独立任务 (无需工作空间)</span>
        {#if !workspacePicker.currentWorkspaceId}
          <svg
            class="h-3.5 w-3.5 text-primary shrink-0"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"><path d="M20 6 9 17l-5-5" /></svg
          >
        {/if}
      </button>

      {#if workspacePicker.workspaces.length > 0}
        <div class="my-1 border-t border-border/50"></div>
        <div class="max-h-48 overflow-y-auto space-y-0.5">
          {#each workspacePicker.workspaces as ws (ws.id)}
            <button
              type="button"
              class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-xs text-left text-foreground hover:bg-accent transition-colors {workspacePicker.currentWorkspaceId ===
              ws.id
                ? 'bg-accent/70 font-medium'
                : ''}"
              onclick={() => {
                workspacePicker?.onSelect(ws.id);
                wsDropdownOpen = false;
              }}
            >
              <svg
                class="h-3.5 w-3.5 text-muted-foreground shrink-0"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                ><path
                  d="M3 7.5A2.5 2.5 0 0 1 5.5 5h4l2 2h7A2.5 2.5 0 0 1 21 9.5v7A2.5 2.5 0 0 1 18.5 19h-13A2.5 2.5 0 0 1 3 16.5Z"
                /></svg
              >
              <span class="truncate flex-1">{ws.name}</span>
              {#if workspacePicker.currentWorkspaceId === ws.id}
                <svg
                  class="h-3.5 w-3.5 text-primary shrink-0"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"><path d="M20 6 9 17l-5-5" /></svg
                >
              {/if}
            </button>
          {/each}
        </div>
      {/if}

      {#if workspacePicker.onOpenFolderWorkspace || workspacePicker.onCreateWorkspace}
        <div class="my-1 border-t border-border/50"></div>
      {/if}
      {#if workspacePicker.onOpenFolderWorkspace}
        <button
          type="button"
          class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-xs text-left text-primary hover:bg-primary/10 transition-colors"
          onclick={() => {
            wsDropdownOpen = false;
            workspacePicker?.onOpenFolderWorkspace?.();
          }}
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
            <path
              d="M3 7.5A2.5 2.5 0 0 1 5.5 5h4l2 2h7A2.5 2.5 0 0 1 21 9.5v7A2.5 2.5 0 0 1 18.5 19h-13A2.5 2.5 0 0 1 3 16.5Z"
            />
          </svg>
          <span class="font-medium">打开本地文件夹...</span>
        </button>
      {/if}
      {#if workspacePicker.onCreateWorkspace}
        <button
          type="button"
          class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-xs text-left text-primary hover:bg-primary/10 transition-colors"
          onclick={() => {
            wsDropdownOpen = false;
            workspacePicker?.onCreateWorkspace?.();
          }}
        >
          <svg
            class="h-3.5 w-3.5 shrink-0"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"><path d="M12 5v14M5 12h14" /></svg
          >
          <span class="font-medium">新建 Workspace...</span>
        </button>
      {/if}
    </div>
  {/if}
</div>

<!-- Paste block editor modal (issue #156): full-height surface for editing long pastes, since the
     main textarea is capped at ~4 lines. -->
{#if editingBlock}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onkeydown={handlePasteEditorKeydown}
  >
    <div
      class="fixed inset-0 bg-black/60 backdrop-blur-sm"
      onclick={cancelPasteEditor}
      role="presentation"
    ></div>
    <div
      class="relative z-50 flex h-[80vh] w-[min(90vw,900px)] flex-col rounded-lg border bg-background p-4 shadow-lg"
    >
      <div class="mb-3 flex items-center justify-between gap-2">
        <h2 class="text-sm font-semibold">
          {t("prompt_editPasteTitle", { seq: String(editingBlock.seq ?? 1) })}
        </h2>
        <span class="text-xs text-muted-foreground">
          {formatPasteSize(editingText.split("\n").length, editingText.length)}
        </span>
      </div>
      <textarea
        bind:this={editorTextareaEl}
        bind:value={editingText}
        spellcheck="false"
        class="flex-1 w-full resize-none rounded-md border bg-muted/30 p-3 font-mono text-xs leading-relaxed focus:outline-none focus:ring-1 focus:ring-ring"
      ></textarea>
      <div class="mt-3 flex items-center justify-end gap-2">
        <button
          type="button"
          onclick={cancelPasteEditor}
          class="rounded-md border px-3 py-1.5 text-xs hover:bg-accent"
        >
          {t("common_cancel")}
        </button>
        <button
          type="button"
          onclick={savePasteEditor}
          class="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90"
        >
          {t("common_save")}
        </button>
      </div>
    </div>
  </div>
{/if}
