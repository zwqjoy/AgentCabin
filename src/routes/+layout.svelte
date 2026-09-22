<script lang="ts">
  import "../app.css";
  import { escapeHtml } from "$lib/utils/ansi";
  import {
    listRuns,
    getUserSettings,
    updateUserSettings,
    listDirectory,
    getGitSummary,
    listPromptFavorites,
    searchPrompts,
    listMemoryFiles,
    deleteRuns,
    getRun,
    stopRun,
    stopSession,
    setRunFlags,
  } from "$lib/api";
  import ProjectFolderItem from "$lib/components/ProjectFolderItem.svelte";
  import SidebarNavItem from "$lib/components/SidebarNavItem.svelte";
  import SidebarSectionLabel from "$lib/components/SidebarSectionLabel.svelte";
  import ModeSwitcher from "$lib/components/ModeSwitcher.svelte";
  import WorkSidebar from "$lib/components/work/WorkSidebar.svelte";
  import CommandPalette from "$lib/components/CommandPalette.svelte";
  import SetupWizard from "$lib/components/SetupWizard.svelte";
  import AboutModal from "$lib/components/AboutModal.svelte";
  import PermissionsModal from "$lib/components/PermissionsModal.svelte";
  import Modal from "$lib/components/Modal.svelte";
  import CliSessionBrowser from "$lib/components/CliSessionBrowser.svelte";
  import UpdateBanner from "$lib/components/UpdateBanner.svelte";
  import FolderPicker from "$lib/components/FolderPicker.svelte";
  import type {
    TaskRun,
    UserSettings,
    DirEntry,
    GitSummary,
    PromptFavorite,
    PromptSearchResult,
    MemoryFileCandidate,
  } from "$lib/types";
  import { cwdDisplayLabel, truncate, snippetAround, relativeTime } from "$lib/utils/format";
  import {
    buildProjectFolders,
    autoExpandForRun,
    expandForProjectChange,
    normalizeCwd,
    type ConversationGroup,
  } from "$lib/utils/sidebar-groups";
  import { loadRemovedCwds } from "$lib/utils/removed-cwds";
  import {
    getLastTarget,
    setLastTarget,
    getStoredRemoteCwd,
    setStoredRemoteCwd,
  } from "$lib/utils/remote-cwd";
  import {
    getSavedProjectCwd,
    setSavedProjectCwd,
    getSavedPinnedCwds,
    setSavedPinnedCwds,
    getSavedExpandedProjects,
    setSavedExpandedProjects,
  } from "$lib/utils/project-cwd";
  import { page } from "$app/stores";
  import { goto, afterNavigate } from "$app/navigation";
  import { onMount, setContext, untrack } from "svelte";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { fpsCounter } from "$lib/utils/perf";
  import { IS_MAC } from "$lib/utils/platform";
  import { PLATFORM_PRESETS } from "$lib/utils/platform-presets";
  import { platform } from "$lib/platform";
  import { loadAgentSettingsCache } from "$lib/stores/agent-settings-cache.svelte";
  import type { PlatformCredential } from "$lib/types";
  import { TeamStore } from "$lib/stores/team-store.svelte";
  import { getEventMiddleware } from "$lib/stores/event-middleware";
  import { KeybindingStore } from "$lib/stores/keybindings.svelte";
  import { getTransport } from "$lib/transport";
  import { deleteSnapshot } from "$lib/utils/snapshot-cache";
  import {
    clampSidebarWidth,
    persistSidebarWidth,
    readSidebarWidth,
    SIDEBAR_WIDTH_MIN,
    SIDEBAR_WIDTH_CHANGED_EVENT,
  } from "$lib/utils/sidebar-width";
  import {
    applyRunMutation,
    dispatchRunMutation,
    RUNS_CHANGED_EVENT,
    type RunMutation,
  } from "$lib/utils/run-mutations";
  import {
    t,
    LOCALE_REGISTRY,
    getEntry,
    initLocale,
    switchLocale,
    currentLocale,
  } from "$lib/i18n/index.svelte";
  import {
    getAppearance,
    setAppearance,
    applyAppearanceCss,
    effectiveReduceMotion as resolveReduceMotion,
    type ColorScheme,
    type ThemeMode,
  } from "$lib/stores/appearance.svelte";
  import {
    getAppMode,
    getAppRealm,
    getAgentTargetFromUrl,
    getModeHref,
    getRealmHref,
    getSavedAppMode,
    setSavedAppMode,
    getSavedRealm,
    setSavedRealm,
    getSavedPiSubMode,
    setSavedPiSubMode,
    type AppRealm,
  } from "$lib/stores/app-mode.svelte";
  import {
    getRunRoute,
    isWorkRun,
    isNativeTarget,
    isPiTarget,
    getTargetRoute,
    getConversationDeleteFallbackRoute,
  } from "$lib/utils/agent-target";
  import type { AppMode } from "$lib/types/work";

  // Wire reactive locale before any t() usage
  initLocale();

  // Apply saved appearance CSS variables immediately (before first paint)
  if (typeof document !== "undefined") {
    applyAppearanceCss(getAppearance());
  }

  let localePopupOpen = $state(false);

  function handleLocaleSelect(code: string) {
    switchLocale(code);
    localePopupOpen = false;
  }

  let commandPaletteOpen = $state(false);
  let showSetupWizard = $state(false);
  let setupWizardCanGoBack = $state(false);
  let showAbout = $state(false);
  let showCliBrowser = $state(false);
  // cwd the CLI session browser is scoped to: "/" = show all.
  let cliBrowserCwd = $state("/");
  let permissionsModalOpen = $state(false);

  // Team store (shared via context with /teams page)
  const teamStore = new TeamStore();
  setContext("teamStore", teamStore);
  getEventMiddleware().setCollaborationHandler((event) => teamStore.handleBusEvent(event));

  // Keybinding store (shared via context with all pages)
  const keybindingStore = new KeybindingStore();
  setContext("keybindings", keybindingStore);

  let { children } = $props();

  let runs = $state<TaskRun[]>([]);
  let sidebarFavorites = $state<PromptFavorite[]>([]);
  let favoriteRunIds = $derived(new Set(sidebarFavorites.map((f) => f.runId)));
  let settings = $state<UserSettings | null>(null);
  let sidebarOpen = $state(true);
  let projectCwd = $state("");

  function getInitialTheme(): ThemeMode {
    if (typeof window === "undefined") return "light";
    const saved = localStorage.getItem("agentcabin:theme");
    if (saved === "light" || saved === "dark" || saved === "system") return saved;
    return "light";
  }

  function getInitialScheme(): ColorScheme {
    if (typeof window === "undefined") return "neutral";
    const saved = localStorage.getItem("agentcabin:colorScheme");
    const allowed: ColorScheme[] = [
      "neutral",
      "zinc",
      "slate",
      "stone",
      "gray",
      "warm",
      "emerald",
      "violet",
      "ocean",
      "rose",
    ];
    return allowed.includes(saved as ColorScheme) ? (saved as ColorScheme) : "neutral";
  }

  let themeMode = $state<ThemeMode>(getInitialTheme());
  let colorScheme = $state<ColorScheme>(getInitialScheme());
  // Three-way reduce motion: stored as "system"|"on"|"off", resolved to boolean for CSS class
  function getInitialReduceMotionTristate(): boolean {
    const raw =
      typeof window !== "undefined" ? localStorage.getItem("agentcabin:reduceMotion") : null;
    if (raw === "on" || raw === "true") return true;
    if (raw === "off" || raw === "false") return false;
    // "system" or null: honour OS preference
    return (
      typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches
    );
  }
  let reduceMotion = $state(getInitialReduceMotionTristate());
  let systemDark = $state(
    typeof window !== "undefined"
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
      : true,
  );
  let effectiveDark = $derived(themeMode === "system" ? systemDark : themeMode === "dark");
  let pinnedCwds = $state<string[]>([]);
  let removedCwds = $state<string[]>([]);

  let panelTab = $state<"chats" | "memory">("chats");
  let runSearchQuery = $state("");
  let chatSearchOpen = $state(false);
  let chatSearchInput: HTMLInputElement | undefined = $state();

  // ── Folder tree state ──
  let expandedProjects = $state<Set<string>>(new Set());
  let runsLoadSucceededOnce = $state(false);

  // ── Deep search (backend full-text) ──
  let searchResults = $state<PromptSearchResult[]>([]);
  let searching = $state(false);
  let searchRequestId = $state(0);
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  // ── Sidebar resize (ghost-line strategy, same as right panel) ──
  // chat-main reflow during drag is still expensive even with cv-auto: visible tool cards
  // contain markdown / hljs code blocks that re-measure on every width change. Ghost line
  // gives zero-reflow drag preview and commits once on release.
  let sidebarWidth = $state(readSidebarWidth());
  let sidebarResizing = $state(false);
  let sidebarGhostX = $state(0);
  let resizeCleanup: (() => void) | null = null;

  /** Ghost line DOM element — bound after sidebarResizing toggles true.
   *  Used only for imperative DOM writes during drag, but declared as $state to satisfy
   *  Svelte 5's bind:this reactivity contract (silences non_reactive_update warning). */
  let sidebarGhostEl: HTMLElement | null = $state(null);

  function startResize(e: PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = sidebarWidth;
    let pendingWidth = startWidth;
    sidebarResizing = true;
    sidebarGhostX = e.clientX; // initial position via Svelte (single render)
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture?.(e.pointerId);
    dbg("layout", "sidebar resize start", { startWidth });
    document.body.style.userSelect = "none";
    document.body.style.cursor = "col-resize";
    const stopFps = fpsCounter("sidebar-drag");

    function onMove(ev: PointerEvent) {
      pendingWidth = clampSidebarWidth(startWidth + (ev.clientX - startX));
      const x = startX + (pendingWidth - startWidth);
      // BYPASS Svelte: write DOM directly. Svelte's reactive batching + WKWebView's pointer
      // capture don't cooperate during drag — updates pile up until pointerup. Direct DOM
      // write is synchronous and the browser repaints on the next frame regardless.
      if (sidebarGhostEl) {
        sidebarGhostEl.style.left = x - 1 + "px";
      }
    }
    function cleanup() {
      handle.removeEventListener("pointermove", onMove);
      handle.removeEventListener("pointerup", onUp);
      handle.removeEventListener("pointercancel", onUp);
      document.body.style.userSelect = "";
      document.body.style.cursor = "";
      sidebarWidth = persistSidebarWidth(pendingWidth);
      sidebarResizing = false;
      sidebarGhostEl = null;
      resizeCleanup = null;
      dbg("layout", "sidebar resize end", { width: sidebarWidth });
      stopFps();
    }
    function onUp() {
      cleanup();
    }

    handle.addEventListener("pointermove", onMove);
    handle.addEventListener("pointerup", onUp);
    handle.addEventListener("pointercancel", onUp);
    resizeCleanup = cleanup;
  }

  // ── File tree state (shown in sidebar when on /explorer) ──
  interface TreeNode {
    name: string;
    fullPath: string;
    is_dir: boolean;
    size: number;
    expanded: boolean;
    loaded: boolean;
    children: TreeNode[];
    depth: number;
  }

  let fileTree = $state<TreeNode[]>([]);
  let treeLoading = $state(false);
  let explorerSelectedFile = $state("");
  let explorerTab = $state<"files" | "git">("files");
  let explorerProjectOpen = $state(false);

  // ── Git state (shown in sidebar Git tab when on /explorer) ──
  let gitSummary = $state<GitSummary | null>(null);
  let gitLoading = $state(false);

  const GIT_STATUS_COLORS: Record<string, string> = {
    M: "text-blue-400",
    A: "text-green-400",
    D: "text-red-400",
    R: "text-purple-400",
    "?": "text-muted-foreground",
  };

  function entriesToNodes(entries: DirEntry[], parentPath: string, depth: number): TreeNode[] {
    return entries.map((e) => ({
      name: e.name,
      fullPath: `${parentPath}/${e.name}`,
      is_dir: e.is_dir,
      size: e.size,
      expanded: false,
      loaded: false,
      children: [],
      depth,
    }));
  }

  let _treeSeq = 0;
  async function loadRootTree() {
    if (!projectCwd) {
      fileTree = [];
      return;
    }
    const seq = ++_treeSeq;
    treeLoading = true;
    try {
      const listing = await listDirectory(projectCwd, true);
      if (seq !== _treeSeq) return; // stale response, discard
      fileTree = entriesToNodes(listing.entries, projectCwd, 0);
      dbg("layout", "file tree loaded", { count: fileTree.length });
    } catch (e) {
      if (seq !== _treeSeq) return;
      dbgWarn("layout", "file tree load error", e);
      fileTree = [];
    } finally {
      if (seq === _treeSeq) treeLoading = false;
    }
  }

  async function toggleFolder(node: TreeNode) {
    if (!node.loaded) {
      try {
        const listing = await listDirectory(node.fullPath, true);
        node.children = entriesToNodes(listing.entries, node.fullPath, node.depth + 1);
        node.loaded = true;
        dbg("layout", "folder loaded", { path: node.fullPath, count: node.children.length });
      } catch (e) {
        dbgWarn("layout", "folder load error", e);
        node.children = [];
        node.loaded = true;
      }
    }
    node.expanded = !node.expanded;
  }

  function selectFile(node: TreeNode) {
    explorerSelectedFile = node.fullPath;
    // Notify explorer page via custom event
    window.dispatchEvent(
      new CustomEvent("agentcabin:explorer-file", { detail: { path: node.fullPath } }),
    );
  }

  let _gitSeq = 0;
  let _gitLoadedCwd = "";
  async function loadGitSummary() {
    if (!projectCwd) {
      gitSummary = null;
      _gitLoadedCwd = "";
      return;
    }
    const requestedCwd = projectCwd;
    const seq = ++_gitSeq;
    gitLoading = true;
    try {
      const result = await getGitSummary(requestedCwd);
      if (seq !== _gitSeq) return; // stale response, discard
      gitSummary = result;
      _gitLoadedCwd = requestedCwd;
      dbg("layout", "git summary loaded", {
        branch: result.branch,
        files: result.total_files,
      });
    } catch (e) {
      if (seq !== _gitSeq) return;
      dbgWarn("layout", "git summary load error", e);
      gitSummary = null;
      _gitLoadedCwd = "";
    } finally {
      if (seq === _gitSeq) gitLoading = false;
    }
  }

  function selectDiffFile(filePath: string) {
    // Notify explorer page to show diff
    window.dispatchEvent(
      new CustomEvent("agentcabin:explorer-diff", { detail: { path: filePath } }),
    );
  }

  // Load tree when switching to explorer page or changing project
  // Git summary is lazy-loaded when user clicks the Git tab (see below)
  let _prevExplorerCwd: string | undefined;
  $effect(() => {
    const _path = currentPath;
    const _cwd = projectCwd;
    if (_path?.startsWith("/explorer")) {
      if (_cwd) {
        loadRootTree();
        // Invalidate git cache when cwd changes so Git tab reloads on next switch
        if (_prevExplorerCwd !== undefined && _prevExplorerCwd !== _cwd) {
          ++_gitSeq; // cancel in-flight request so it can't backfill _gitLoadedCwd
          gitLoading = false;
          _gitLoadedCwd = "";
        }
        _prevExplorerCwd = _cwd;
      } else {
        // Increment seq to invalidate any in-flight requests
        ++_treeSeq;
        ++_gitSeq;
        // Clear state
        fileTree = [];
        gitSummary = null;
        gitLoading = false;
        treeLoading = false;
        _gitLoadedCwd = "";
        _prevExplorerCwd = _cwd;
      }
    }
  });

  // Lazy-load git summary when user switches to Git tab (only on Explorer page)
  $effect(() => {
    if (
      currentPath?.startsWith("/explorer") &&
      explorerTab === "git" &&
      projectCwd &&
      _gitLoadedCwd !== projectCwd
    ) {
      loadGitSummary();
    }
  });

  // Load memory candidates when switching to memory page or changing project
  // Navigation items (declared before pageName derivation)
  const navItems = [
    { path: "/chat", label: () => t("nav_chat"), icon: "message" },
    { path: "/usage", label: () => t("nav_usage"), icon: "chart" },
    { path: "/settings", label: () => t("nav_settings"), icon: "settings" },
  ];

  // Load initial data
  let runsLoadVersion = 0;

  function sidebarStatusFromRunState(state: unknown): TaskRun["status"] | null {
    switch (state) {
      case "spawning":
      case "running":
        return "running";
      case "idle":
      case "completed":
      case "failed":
      case "stopped":
        return state;
      default:
        return null;
    }
  }

  async function loadRuns() {
    const requestVersion = ++runsLoadVersion;
    try {
      const nextRuns = await listRuns();
      if (requestVersion !== runsLoadVersion) return;
      runs = nextRuns;
      runsLoadSucceededOnce = true;
    } catch {
      // Silently fail
    }
  }

  async function loadSidebarFavorites() {
    try {
      sidebarFavorites = await listPromptFavorites();
    } catch {
      // Silently fail
    }
  }

  // ── Deep search ──

  function onDeepQueryInput() {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => doDeepSearch(), 300);
  }

  async function doDeepSearch() {
    const q = runSearchQuery.trim();
    if (!q) {
      searchResults = [];
      searching = false;
      return;
    }
    searching = true;
    const reqId = ++searchRequestId;
    try {
      const results = await searchPrompts(q);
      if (reqId !== searchRequestId) return;
      searchResults = results;
      dbg("layout", "search results", { count: results.length });
    } catch (e) {
      if (reqId !== searchRequestId) return;
      dbg("layout", "search error", e);
      searchResults = [];
    } finally {
      if (reqId === searchRequestId) searching = false;
    }
  }

  function highlightMatch(text: string, query: string): string {
    if (!query.trim()) return escapeHtml(text);
    const escaped = escapeHtml(text);
    const q = escapeHtml(query.trim());
    const re = new RegExp(`(${q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "gi");
    return escaped.replace(re, "<mark>$1</mark>");
  }

  async function loadSettings() {
    try {
      settings = await getUserSettings();
      const normalizedWd = normalizeCwd(settings.working_directory);
      if (normalizedWd) {
        localStorage.setItem("agentcabin:settings-cwd", normalizedWd);
        if (!projectCwd) projectCwd = normalizedWd;
      } else {
        localStorage.removeItem("agentcabin:settings-cwd");
      }
      // Show setup wizard if onboarding not completed
      if (!settings.onboarding_completed) {
        setupWizardCanGoBack = false;
        showSetupWizard = true;
      }
      // One-time migration: if platform_credentials is empty but api_key exists,
      // create an initial credential from current settings
      await migrateCredentialsIfNeeded(settings);
      applyZoom(settings.ui_zoom);
    } catch {
      // Silently fail
    }
  }

  function applyZoom(zoom?: number) {
    const factor = Math.min(1.5, Math.max(0.75, zoom ?? 1.0));
    platform.window
      .setZoom(factor)
      .then(() => dbg("layout", "applyZoom success", { factor }))
      .catch((e) => dbgWarn("layout", "setZoom failed", e));
  }

  /** Migrate existing api_key into platform_credentials (one-time). */
  async function migrateCredentialsIfNeeded(s: UserSettings) {
    if (s.platform_credentials && s.platform_credentials.length > 0) return;
    if (!s.anthropic_api_key) return;

    // Detect platform from base_url
    let platformId = "anthropic";
    if (s.anthropic_base_url) {
      const match = PLATFORM_PRESETS.find((p) => p.base_url && s.anthropic_base_url === p.base_url);
      platformId = match?.id ?? "custom-migrated";
    }

    const cred: PlatformCredential = {
      platform_id: platformId,
      api_key: s.anthropic_api_key,
      base_url: s.anthropic_base_url || undefined,
      auth_env_var: s.auth_env_var || undefined,
      ...(platformId === "custom-migrated" ? { name: "Migrated" } : {}),
    };

    try {
      await updateUserSettings({
        platform_credentials: [cred],
        active_platform_id: platformId,
      } as Partial<UserSettings>);
      dbg("layout", "migrated credentials", { platformId });
    } catch (e) {
      dbgWarn("layout", "credential migration failed:", e);
    }
  }

  function handleSetupComplete() {
    showSetupWizard = false;
    setupWizardCanGoBack = false;
    loadSettings();
  }

  function handleSetupBack() {
    showSetupWizard = false;
    setupWizardCanGoBack = false;
  }

  // Use onMount for initialization (not $effect - avoids accidental reactive tracking)
  onMount(() => {
    // Remove splash screen
    const splash = document.getElementById("app-splash");
    if (splash) {
      splash.style.opacity = "0";
      setTimeout(() => splash.remove(), 300);
    }

    // The pet window only needs the shared shell and event bridge. Avoid
    // booting the main-window stores and data loaders in this lightweight view.
    if (isPetPage) return;

    void loadRuns();
    loadSettings();
    void loadSidebarFavorites();
    void loadAgentSettingsCache();

    // Load saved CWD and pinned folders from scoped realm storage
    const initialRealm = getAppRealm(new URL(window.location.href));
    const saved = getSavedProjectCwd(initialRealm);
    if (saved) projectCwd = normalizeCwd(saved) || "";

    expandedProjects = getSavedExpandedProjects(initialRealm);
    pinnedCwds = getSavedPinnedCwds(initialRealm);
    removedCwds = loadRemovedCwds();

    let destroyed = false;

    const transport = getTransport();

    // Keybinding store: load overrides + CLI bindings, register app-level callbacks
    keybindingStore.loadOverrides();
    keybindingStore.loadCliBindings();
    keybindingStore.registerCallback("app:toggleSidebar", toggleSidebar);
    keybindingStore.registerCallback("app:commandPalette", () => {
      commandPaletteOpen = !commandPaletteOpen;
    });
    keybindingStore.registerCallback("app:newChat", newChat);

    // Immediate refresh when chat page signals a status change
    function onRunsChanged(event: Event) {
      const mutation = (event as CustomEvent<RunMutation>).detail;
      if (
        mutation?.kind === "create" ||
        mutation?.kind === "update" ||
        mutation?.kind === "delete"
      ) {
        // Ignore an older full-list response that was already in flight when the
        // local mutation completed.
        ++runsLoadVersion;
        runs = applyRunMutation(runs, mutation);
      } else {
        loadRuns();
      }
      // Invalidate git cache unconditionally; if currently viewing Git tab on Explorer,
      // reload immediately — otherwise lazy $effect picks it up on next visit.
      ++_gitSeq; // cancel in-flight request so it can't backfill _gitLoadedCwd
      gitLoading = false;
      _gitLoadedCwd = "";
      if (currentPath?.startsWith("/explorer") && explorerTab === "git") {
        loadGitSummary();
      }
    }
    window.addEventListener(RUNS_CHANGED_EVENT, onRunsChanged);

    // Chat and Settings use one persisted content-sidebar width. Settings can
    // update it while the chat panel is unmounted, so keep this layout state in sync.
    function onSidebarWidthChanged(e: Event) {
      const width = (e as CustomEvent<{ width?: unknown }>).detail?.width;
      if (typeof width === "number") sidebarWidth = clampSidebarWidth(width);
    }
    window.addEventListener(SIDEBAR_WIDTH_CHANGED_EVENT, onSidebarWidthChanged);

    // Refresh sidebar favorites when /runs page changes them
    function onFavoritesChanged() {
      loadSidebarFavorites();
    }
    window.addEventListener("agentcabin:favorites-changed", onFavoritesChanged);

    // Listen for Settings page requesting wizard re-open
    function onShowWizard() {
      setupWizardCanGoBack = true;
      showSetupWizard = true;
    }
    window.addEventListener("agentcabin:show-wizard", onShowWizard);

    // Settings owns the appearance controls; keep the app shell in sync without
    // requiring a full navigation or reload.
    function onAppearanceChanged(e: Event) {
      const detail = (
        e as CustomEvent<{
          themeMode?: ThemeMode;
          colorScheme?: ColorScheme;
          // Extended: reduceMotion is now "system"|"on"|"off"
          reduceMotion?: string | boolean;
          pointerCursor?: boolean;
          translucentSidebar?: boolean;
          fontSmoothing?: boolean;
          diffMarkerStyle?: string;
          uiFontSize?: number;
          codeFontSize?: number;
          contrast?: number;
          accentColor?: string;
          backgroundColor?: string;
          foregroundColor?: string;
          uiFontFamily?: string;
          codeFontFamily?: string;
        }>
      ).detail;
      if (
        detail?.themeMode === "light" ||
        detail?.themeMode === "dark" ||
        detail?.themeMode === "system"
      ) {
        themeMode = detail.themeMode;
      }
      if (detail?.colorScheme) colorScheme = detail.colorScheme;
      // Handle three-way reduce motion
      if (detail?.reduceMotion !== undefined) {
        const rm = detail.reduceMotion;
        if (rm === "on" || rm === true) {
          reduceMotion = true;
        } else if (rm === "off" || rm === false) {
          reduceMotion = false;
        } else {
          // "system"
          reduceMotion =
            typeof window !== "undefined" &&
            window.matchMedia("(prefers-reduced-motion: reduce)").matches;
        }
      }
      // Re-apply all CSS variables from the store for extended fields
      applyAppearanceCss(getAppearance());
    }

    // A transparent fixed backdrop is used by the shell's locale/theme
    // popovers. If a Work stop races a route/state transition, make sure that
    // transient click-catcher cannot outlive the action that opened it.
    function onCloseTransientOverlays() {
      localePopupOpen = false;
      themePopoverOpen = false;
      chatSearchOpen = false;
      runSearchQuery = "";
      searchResults = [];
      searching = false;
    }
    window.addEventListener("agentcabin:close-transient-overlays", onCloseTransientOverlays);
    window.addEventListener("agentcabin:appearance-changed", onAppearanceChanged);

    // Sync projectCwd when chat page picks a folder via dialog
    function handleCwdChanged() {
      const newCwd = normalizeCwd(getSavedProjectCwd(appRealm)) || "";
      if (newCwd !== projectCwd) {
        projectCwd = newCwd;
      }
    }
    window.addEventListener("agentcabin:cwd-changed", handleCwdChanged);

    // Open permissions modal from any entry point (Command Palette, PromptInput button)
    function onOpenPermissions() {
      permissionsModalOpen = true;
    }
    window.addEventListener("agentcabin:open-permissions", onOpenPermissions);

    // ── External link interceptor ──
    // Prevent webview from navigating away to external URLs.
    // Opens them in the system browser instead.
    function handleExternalLink(e: MouseEvent) {
      // Only intercept plain left-click (no modifier keys)
      if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;

      const anchor = (e.target as HTMLElement)?.closest?.("a");
      if (!anchor) return;

      const href = anchor.getAttribute("href");
      if (!href) return;

      // Parse URL — handles protocol-relative (//example.com), case-insensitive schemes
      let url: URL;
      try {
        url = new URL(href, window.location.origin);
      } catch {
        return;
      }

      // Only intercept http/https
      if (url.protocol !== "http:" && url.protocol !== "https:") return;
      // Skip internal SvelteKit routes (same origin)
      if (url.origin === window.location.origin) return;

      // Prevent webview navigation, don't stopPropagation (let other listeners see it)
      e.preventDefault();

      dbg("layout", "external-link: opening in system browser", { href });
      platform.shell.openExternal(href).catch((err) => {
        dbgWarn("layout", "external-link: shell open failed, fallback to window.open", err);
        window.open(href, "_blank");
      });
    }
    document.addEventListener("click", handleExternalLink, true);
    dbg("layout", "external-link interceptor mounted");

    // Explorer → layout: sync sidebar highlight when explorer restores cached file
    function onExplorerFileSelected(e: Event) {
      explorerSelectedFile = (e as CustomEvent).detail?.path ?? "";
    }
    window.addEventListener("agentcabin:explorer-file-selected", onExplorerFileSelected);

    // Listen for run status changes (idle↔running) from backend
    let unlistenStatus: (() => void) | undefined;
    transport
      .listen("agentcabin:status-changed", (payload: unknown) => {
        dbg("layout", "status-changed", payload);
        loadRuns();
      })
      .then((fn) => {
        if (destroyed) {
          fn();
          return;
        }
        unlistenStatus = fn;
      });

    // Keep the sidebar in sync with the same authoritative lifecycle event used
    // by the open chat. Some agent transports persist the new status and emit
    // run_state without also sending the legacy status-changed refresh signal.
    let unlistenBusEvent: (() => void) | undefined;
    transport
      .listen<{
        type?: unknown;
        run_id?: unknown;
        state?: unknown;
      }>("bus-event", (payload) => {
        if (payload?.type !== "run_state" || typeof payload.run_id !== "string") return;
        const status = sidebarStatusFromRunState(payload.state);
        if (!status) return;
        runs = runs.map((run) => (run.id === payload.run_id ? { ...run, status } : run));
      })
      .then((fn) => {
        if (destroyed) {
          fn();
          return;
        }
        unlistenBusEvent = fn;
      });

    return () => {
      resizeCleanup?.(); // Clean up resize drag if component unmounts mid-drag
      unlistenStatus?.();
      unlistenBusEvent?.();
      if (debounceTimer) clearTimeout(debounceTimer);
      destroyed = true;
      keybindingStore.unregisterCallback("app:toggleSidebar");
      keybindingStore.unregisterCallback("app:commandPalette");
      keybindingStore.unregisterCallback("app:newChat");
      window.removeEventListener(RUNS_CHANGED_EVENT, onRunsChanged);
      window.removeEventListener(SIDEBAR_WIDTH_CHANGED_EVENT, onSidebarWidthChanged);
      window.removeEventListener("agentcabin:favorites-changed", onFavoritesChanged);
      window.removeEventListener("agentcabin:show-wizard", onShowWizard);
      window.removeEventListener("agentcabin:appearance-changed", onAppearanceChanged);
      window.removeEventListener("agentcabin:close-transient-overlays", onCloseTransientOverlays);
      window.removeEventListener("agentcabin:cwd-changed", handleCwdChanged);
      window.removeEventListener("agentcabin:open-permissions", onOpenPermissions);
      document.removeEventListener("click", handleExternalLink, true);
      window.removeEventListener("agentcabin:explorer-file-selected", onExplorerFileSelected);
    };
  });

  // Save CWD to scoped realm storage when changed (clear key for "All Projects")
  // Also pin manually-selected folders so they persist in the project list
  $effect(() => {
    if (typeof window !== "undefined") {
      setSavedProjectCwd(projectCwd, appRealm);
      if (projectCwd) {
        // Pin this cwd so it stays in the dropdown after switching away
        if (projectCwd !== "/" && !pinnedCwds.includes(projectCwd)) {
          pinnedCwds = [...pinnedCwds, projectCwd];
          setSavedPinnedCwds(pinnedCwds, appRealm);
        }
      }
      // Notify child pages (e.g. Memory) that project cwd changed
      window.dispatchEvent(
        new CustomEvent("agentcabin:project-changed", { detail: { cwd: projectCwd } }),
      );
    }
  });

  // Apply the accessibility preference globally so it also covers third-party
  // components and pages outside the settings route.
  $effect(() => {
    document.documentElement.classList.toggle("reduce-motion", reduceMotion);
  });

  function handleNativeWindowDrag(event: MouseEvent) {
    if (event.button !== 0) return;

    const target = event.target instanceof Element ? event.target : null;
    if (!target) return;

    const dragRegion = target.closest("[data-window-drag-region], [data-tauri-drag-region]");
    if (!dragRegion) return;

    // Do not drag if clicking an interactive control inside the drag region
    if (
      target.closest(
        "button, a, input, textarea, select, [contenteditable='true'], [role='button'], .no-drag",
      )
    ) {
      return;
    }

    try {
      if (typeof window !== "undefined") {
        void getTransport().startDragging();
      }
    } catch {
      /* ignore non-tauri */
    }
  }

  onMount(() => {
    if (isPetPage) return;
    window.addEventListener("mousedown", handleNativeWindowDrag, true);
    return () => window.removeEventListener("mousedown", handleNativeWindowDrag, true);
  });

  afterNavigate(({ to }) => {
    dbg("layout", "navigated to:", to?.url.pathname);
    // Keep the left sidebar tab aligned with the destination page.
    if (to?.url.pathname === "/memory") panelTab = "memory";
    else if (
      to?.url.pathname === "/chat" ||
      to?.url.pathname === "/chat/pi" ||
      to?.url.pathname === "/history"
    ) {
      panelTab = "chats";
    }
    // Sync plugin section from URL when navigating to the plugin manager.
    if (to?.url.pathname.startsWith("/chat/plugins")) {
      const section = to.url.searchParams.get("section");
      const normalizedSection = section === "plugins" ? "claude-plugins" : section;
      if (normalizedSection && pluginSections.some((s) => s.id === normalizedSection)) {
        pluginActiveSection = normalizedSection;
      }
    }
    if (!isWorkPage && !isPetPage && (!runsLoadSucceededOnce || runs.length === 0)) {
      void loadRuns();
    }
  });

  // Catch unhandled errors that could break the router
  onMount(() => {
    if (isPetPage) return;

    function onError(e: ErrorEvent) {
      dbgWarn("layout", "global error", e.message, e.filename, e.lineno);
    }
    function onRejection(e: PromiseRejectionEvent) {
      dbgWarn("layout", "unhandled rejection", e.reason);
      // Don't call e.preventDefault() — let rejections surface in devtools
    }
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);

    return () => {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    };
  });

  // Get selected run from URL
  let selectedRunId = $derived.by(() => {
    const url = $page.url;
    return url.searchParams.get("run") ?? "";
  });

  // ── Delete conversation confirm flow ──
  let deleteConfirmOpen = $state(false);
  let deleteTarget: ConversationGroup | null = $state(null);

  function requestDeleteConversation(conv: ConversationGroup) {
    deleteTarget = conv;
    deleteConfirmOpen = true;
  }

  async function endConversation(conv: ConversationGroup) {
    const activeRuns = conv.runs.filter(
      (run) => !["completed", "failed", "stopped"].includes(run.status),
    );
    if (activeRuns.length === 0) return;

    try {
      for (const run of activeRuns) {
        // stop_session emits a durable stopped event when an actor exists and also
        // handles idle sessions. If the actor already disappeared, stop_run is the
        // hard fallback that still repairs meta.json.
        let needsFallback = false;
        try {
          await stopSession(run.id);
          const fresh = await getRun(run.id);
          needsFallback = !["completed", "failed", "stopped"].includes(fresh.status);
        } catch (e) {
          needsFallback = true;
          dbgWarn("layout", "stop_session fallback required", { runId: run.id, error: e });
        }
        if (needsFallback) {
          await stopRun(run.id);
        }
      }
      dbg("layout", "endConversation success", { ids: activeRuns.map((run) => run.id) });
      await loadRuns();
      window.dispatchEvent(new Event(RUNS_CHANGED_EVENT));
    } catch (e) {
      dbgWarn("layout", "endConversation failed", e);
    }
  }

  async function confirmDeleteConversation() {
    const conv = deleteTarget;
    deleteConfirmOpen = false;
    deleteTarget = null;
    if (!conv) return;
    try {
      const ids = conv.runs.map((r) => r.id);
      await deleteRuns(ids);
      await Promise.all(ids.map((id) => deleteSnapshot(id)));
      dbg("layout", "deleteConversation success", { ids });
      dispatchRunMutation({ kind: "delete", runIds: ids });
      if (conv.runs.some((r) => r.id === selectedRunId)) {
        goto(getConversationDeleteFallbackRoute(currentTarget));
      }
    } catch (e) {
      dbgWarn("layout", "deleteConversation failed", e);
    }
  }

  function cancelDeleteConversation() {
    deleteConfirmOpen = false;
    deleteTarget = null;
  }

  // ── Remove project folder confirm flow ──
  let removeProjectConfirmOpen = $state(false);
  let removeProjectTarget = $state("");

  function persistRemovedCwds() {
    localStorage.setItem("agentcabin:removed-cwds", JSON.stringify(removedCwds));
  }

  function requestRemoveProject(cwd: string) {
    removeProjectTarget = normalizeCwd(cwd);
    removeProjectConfirmOpen = true;
  }

  function confirmRemoveProject() {
    const normalized = removeProjectTarget;
    removeProjectConfirmOpen = false;
    removeProjectTarget = "";
    if (!normalized) return;

    // Add to removedCwds
    if (!removedCwds.includes(normalized)) {
      removedCwds = [...removedCwds, normalized];
      persistRemovedCwds();
    }

    // Remove from pinnedCwds (compare normalized) across both realms
    const cleanNativePinned = getSavedPinnedCwds("native").filter(
      (c) => normalizeCwd(c) !== normalized,
    );
    setSavedPinnedCwds(cleanNativePinned, "native");
    const cleanPiPinned = getSavedPinnedCwds("pi").filter((c) => normalizeCwd(c) !== normalized);
    setSavedPinnedCwds(cleanPiPinned, "pi");
    pinnedCwds = appRealm === "pi" ? cleanPiPinned : cleanNativePinned;

    // Clear saved project CWD if it matches the removed folder
    if (normalizeCwd(getSavedProjectCwd("native")) === normalized) {
      setSavedProjectCwd("", "native");
    }
    if (normalizeCwd(getSavedProjectCwd("pi")) === normalized) {
      setSavedProjectCwd("", "pi");
    }

    // If currently viewing this project, switch to All Projects
    if (normalizeCwd(projectCwd) === normalized) {
      projectCwd = "";
    }

    // Dispatch events to inform child components and pages
    window.dispatchEvent(
      new CustomEvent("agentcabin:project-removed", { detail: { cwd: normalized } }),
    );
    window.dispatchEvent(new Event(RUNS_CHANGED_EVENT));

    dbg("layout", "removeProject", { cwd: normalized });
  }

  function cancelRemoveProject() {
    removeProjectConfirmOpen = false;
    removeProjectTarget = "";
  }

  // Current page detection
  let currentPath = $derived($page.url.pathname);
  let isPetPage = $derived(currentPath === "/pet");
  let currentTarget = $derived(getAgentTargetFromUrl($page.url));
  let appMode = $derived(getAppMode($page.url));
  let isPiCodePage = $derived(currentTarget === "pi:code");
  let isWorkPage = $derived(appMode === "work");
  // Work keeps the Pi sidebar-storage partition for compatibility; this does
  // not identify its runtime provider.
  let isPiPage = $derived(isPiTarget(currentTarget) || isWorkPage);
  let appRealm = $derived<AppRealm>(isPiPage ? "pi" : "native");

  // Re-read runs, favorites, and settings cache when switching from Work to Code
  let _wasWorkPage: boolean | undefined;
  $effect(() => {
    const isWork = isWorkPage;
    if (_wasWorkPage === true && !isWork) {
      untrack(() => {
        void loadRuns();
        void loadSidebarFavorites();
        void loadAgentSettingsCache();
      });
    }
    _wasWorkPage = isWork;
  });

  // Ensure runs are hydrated on non-work pages if not loaded yet
  $effect(() => {
    if (!isWorkPage && !isPetPage && !runsLoadSucceededOnce) {
      untrack(() => {
        void loadRuns();
      });
    }
  });

  // Poll for runs every 60s while viewing non-work pages (fallback only — primary updates via agentcabin:runs-changed event)
  $effect(() => {
    if (isWorkPage || isPetPage) return;
    const pollInterval = setInterval(() => {
      void loadRuns();
    }, 60000);
    return () => clearInterval(pollInterval);
  });

  let isChatPage = $derived(currentPath === "/chat" || currentPath.startsWith("/chat/"));
  let isExplorerPage = $derived(currentPath.startsWith("/explorer"));
  let isMemoryPage = $derived(currentPath.startsWith("/memory"));
  let isSettingsPage = $derived(currentPath.startsWith("/settings"));
  let isPluginsPage = $derived(currentPath.startsWith("/chat/plugins"));

  // Sync project workspace state when realm switches (Native <-> Pi)
  let _prevLayoutRealm: AppRealm | undefined;
  $effect(() => {
    if (!isChatPage) return;
    const realm = appRealm;
    if (_prevLayoutRealm !== undefined && _prevLayoutRealm !== realm) {
      // Switching realms: persist state for previous realm
      setSavedProjectCwd(projectCwd, _prevLayoutRealm);
      setSavedPinnedCwds(pinnedCwds, _prevLayoutRealm);
      setSavedExpandedProjects(expandedProjects, _prevLayoutRealm);

      // Restore state for new realm
      projectCwd = normalizeCwd(getSavedProjectCwd(realm)) || "";
      pinnedCwds = getSavedPinnedCwds(realm);
      expandedProjects = getSavedExpandedProjects(realm);
    }
    _prevLayoutRealm = realm;
  });

  let piSubMode = $derived<"code" | "work">(isWorkPage ? "work" : "code");

  // Scoped runs for sidebar depending on current realm / mode
  let scopedRuns = $derived.by(() => {
    if (isWorkPage) {
      return runs.filter((run) => isWorkRun(run));
    }
    // Code harness mode (includes all code runs: pi:code, native:claude, native:codex, native:grok)
    return runs.filter((run) => !isWorkRun(run));
  });

  // Build project folder tree for chats tab. Archived conversations are managed
  // from dedicated archived views and stay out of the main conversation list.
  let projectFolders = $derived.by(() =>
    buildProjectFolders(scopedRuns, favoriteRunIds, pinnedCwds, removedCwds, false),
  );

  let archivedRunsCount = $derived(scopedRuns.filter((run) => run.archived).length);

  // Selectable folders: real project folders (exclude Uncategorized)
  const selectableFolders = $derived(projectFolders.filter((f) => !f.isUncategorized));

  // Removed cwd set for O(1) lookup in search filtering
  let removedCwdSet = $derived(new Set(removedCwds.map(normalizeCwd)));
  let scopedRunIdSet = $derived(new Set(scopedRuns.map((r) => r.id)));

  // Filter search results to exclude removed project cwds and respect current realm
  let visibleSearchResults = $derived.by(() => {
    let list = searchResults.filter((result) => scopedRunIdSet.has(result.runId));
    if (removedCwdSet.size === 0) return list;
    // Build runId→cwd mapping from runs
    const runCwdMap = new Map<string, string>();
    for (const run of runs) {
      runCwdMap.set(run.id, normalizeCwd(run.cwd));
    }
    return list.filter((result) => {
      const cwd = runCwdMap.get(result.runId);
      // Unknown runId (not in runs yet) → show by default (avoid async timing issues)
      if (cwd === undefined) return true;
      // "" = Uncategorized → always show
      if (!cwd) return true;
      return !removedCwdSet.has(cwd);
    });
  });

  // Debug log when folder tree rebuilds
  $effect(() => {
    dbg("layout", "folders rebuilt", {
      count: projectFolders.length,
      total: projectFolders.reduce((s, f) => s + f.conversationCount, 0),
    });
  });

  // Defensive fallback: reset projectCwd if it's no longer in selectable folders
  $effect(() => {
    if (!projectCwd) return; // "" is always valid (All Projects)
    const validCwds = new Set(selectableFolders.map((f) => f.cwd));
    if (!validCwds.has(projectCwd)) {
      dbg("layout", "projectCwd not in selectable folders, resetting", { projectCwd });
      projectCwd = "";
    }
  });

  function handleRealmSelect(realm: AppRealm, targetPiSubMode?: "code" | "work") {
    setSavedRealm(realm);
    if (realm === "pi") {
      const subMode = targetPiSubMode ?? getSavedPiSubMode();
      setSavedPiSubMode(subMode);
      setSavedAppMode(subMode === "work" ? "work" : "code");
      if (
        subMode === "work" &&
        (settings?.work_mode_enabled === false || !getTransport().isDesktop())
      ) {
        void goto("/chat/pi");
        return;
      }
      void goto(getRealmHref("pi", subMode));
      return;
    }
    setSavedAppMode("code");
    setSavedPiSubMode("code");
    void goto("/chat");
  }

  function handleModeSelect(mode: AppMode) {
    if (mode === appMode) return;
    setSavedAppMode(mode);
    if (mode === "work") {
      setSavedRealm("pi");
      setSavedPiSubMode("work");
    } else {
      setSavedPiSubMode("code");
    }
    if (mode === "work" && (settings?.work_mode_enabled === false || !getTransport().isDesktop())) {
      void goto("/chat");
      return;
    }
    void goto(getModeHref(mode));
  }

  $effect(() => {
    if (isChatPage) {
      setSavedAppMode(appMode);
      setSavedRealm(appRealm);
      setSavedPiSubMode(piSubMode);
    }
  });

  $effect(() => {
    if (isWorkPage && settings?.work_mode_enabled === false) {
      void goto("/chat", { replaceState: true });
    }
  });

  $effect(() => {
    if (currentPath.startsWith("/explorer")) {
      goto("/chat?tab=files", { replaceState: true, noScroll: true });
    }
  });

  // Plugin section state is shared by the chat plugin route and the embedded Settings view.
  const pluginSections = [
    { id: "skills", label: () => t("sidebar_skills"), icon: "sparkles" },
    { id: "mcp", label: () => t("sidebar_mcpServers"), icon: "server" },
    { id: "claude-plugins", label: () => t("sidebar_claudePlugins"), icon: "package" },
    { id: "codex-plugins", label: () => t("sidebar_codexPlugins"), icon: "package" },
    { id: "pi-extensions", label: () => t("sidebar_piExtensions"), icon: "pi-ext" },
    { id: "prompts", label: () => t("sidebar_promptTemplates"), icon: "file-text" },
  ];

  let pluginActiveSection = $state<string>("skills");
  setContext("pluginSection", {
    get active() {
      return pluginActiveSection;
    },
    set active(v: string) {
      pluginActiveSection = v;
    },
  });

  // Breadcrumb for non-chat pages
  let pageName = $derived.by(() => {
    const nav = navItems.find((n) => currentPath.startsWith(n.path));
    if (nav) return nav.label();
    if (currentPath.startsWith("/release-notes")) return t("release_cliChangelog");
    return t("layout_appName");
  });

  function newChat() {
    if (isWorkPage) {
      if (typeof window !== "undefined") {
        window.dispatchEvent(
          new CustomEvent("agentcabin:work-new-chat", {
            detail: { workspaceId: "" },
          }),
        );
      }
      void goto("/chat/work?newSession=1");
      return;
    }
    if (typeof window !== "undefined") {
      window.dispatchEvent(new CustomEvent("agentcabin:new-chat", { detail: {} }));
    }
    void goto(getTargetRoute(currentTarget));
  }

  function closeChatSearch() {
    chatSearchOpen = false;
    runSearchQuery = "";
    searchResults = [];
    searching = false;
  }

  function toggleChatSearch() {
    if (chatSearchOpen) {
      closeChatSearch();
      return;
    }
    chatSearchOpen = true;
    requestAnimationFrame(() => chatSearchInput?.focus());
  }

  function newChatInFolder(cwd: string) {
    projectCwd = cwd;
    if (typeof window !== "undefined") {
      window.dispatchEvent(new CustomEvent("agentcabin:new-chat", { detail: { cwd } }));
    }
    const base = isPiCodePage ? "/chat/pi" : "/chat";
    goto(`${base}?folder=${encodeURIComponent(cwd)}`);
  }

  // Open the CLI session browser, optionally scoped to a folder's cwd.
  // "/" keeps the original show-all behavior used by the toolbar button.
  function openCliBrowser(cwd: string = "/") {
    cliBrowserCwd = cwd || "/";
    showCliBrowser = true;
  }

  function toggleProject(folderKey: string) {
    const next = new Set(expandedProjects);
    if (next.has(folderKey)) next.delete(folderKey);
    else next.add(folderKey);
    expandedProjects = next;
    setSavedExpandedProjects(expandedProjects, appRealm);
  }

  // ── Folder picker (sidebar "+ Open folder") ──
  let folderPickerOpen = $state(false);
  let folderPickerInitialHost = $state<string | null>(null);
  let folderPickerInitialPath = $state("");

  async function pickFolder() {
    // Pre-fill from last-target so remote-using users don't lose their target.
    // Validate against current settings — a host removed/renamed since the
    // value was persisted should not silently leak through to the picker.
    const lastTarget = getLastTarget();
    const validatedTarget =
      lastTarget && (settings?.remote_hosts ?? []).some((h) => h.name === lastTarget)
        ? lastTarget
        : null;
    if (lastTarget && !validatedTarget) {
      dbgWarn("layout", "lastTarget references unknown remote — falling back to local", {
        lastTarget,
      });
    }

    // On desktop for local folder picking, directly open the native OS folder dialog
    if (getTransport().isDesktop() && !validatedTarget) {
      try {
        const selected = await platform.dialog.open({
          directory: true,
          multiple: false,
          title: t("layout_selectProjectFolder"),
          defaultPath: projectCwd || settings?.working_directory || undefined,
        });
        if (typeof selected === "string" && selected.trim()) {
          onFolderPicked({ hostName: null, path: selected.trim() });
        }
        return;
      } catch (e) {
        dbgWarn("layout", "native folder picker failed, falling back to modal", e);
      }
    }

    folderPickerInitialHost = validatedTarget;
    folderPickerInitialPath = validatedTarget
      ? getStoredRemoteCwd(validatedTarget)
      : projectCwd || settings?.working_directory || "";
    folderPickerOpen = true;
  }

  function onFolderPicked(result: { hostName: string | null; path: string }) {
    const { hostName, path } = result;
    if (!path) return;
    if (hostName) {
      // Remote: persist and navigate to chat with host+folder
      setStoredRemoteCwd(hostName, path);
      setLastTarget(hostName);
      // Clear local projectCwd so the local file tree doesn't try to list a remote path
      projectCwd = "";
      dbg("layout", "pickFolder (remote)", { hostName, path });
      goto(`/chat?host=${encodeURIComponent(hostName)}&folder=${encodeURIComponent(path)}`);
    } else {
      // Local target
      const normalized = normalizeCwd(path) || "";
      let wasRemoved = false;
      if (normalized && removedCwds.includes(normalized)) {
        removedCwds = removedCwds.filter((c) => c !== normalized);
        persistRemovedCwds();
        wasRemoved = true;
        dbg("layout", "pickFolder: un-removed cwd", { cwd: normalized });
      }
      projectCwd = normalized;
      setLastTarget(null);
      if (wasRemoved) {
        window.dispatchEvent(new Event(RUNS_CHANGED_EVENT));
      }
    }
  }

  function toggleSidebar() {
    sidebarOpen = !sidebarOpen;
  }

  setContext("toggleSidebar", toggleSidebar);
  setContext("isSidebarOpen", () => sidebarOpen);

  function cycleTheme() {
    const order: ThemeMode[] = ["dark", "light", "system"];
    const idx = order.indexOf(themeMode);
    themeMode = order[(idx + 1) % order.length];
    dbg("layout", "theme cycled", { themeMode, effectiveDark });
  }

  let themePopoverOpen = $state(false);

  const ALL_SCHEME_CLASSES = [
    "scheme-neutral",
    "scheme-zinc",
    "scheme-slate",
    "scheme-stone",
    "scheme-gray",
    "scheme-warm",
    "scheme-emerald",
    "scheme-violet",
    "scheme-ocean",
    "scheme-rose",
  ];

  function selectColorScheme(scheme: string) {
    colorScheme = scheme as ColorScheme;
    setAppearance({ colorScheme: scheme as ColorScheme });
  }

  function cycleScheme() {
    const schemes = ["neutral", "warm", "emerald", "violet", "ocean", "rose"];
    const idx = schemes.indexOf(colorScheme);
    const next = schemes[(idx + 1) % schemes.length];
    selectColorScheme(next);
  }

  // Persist theme + apply class
  $effect(() => {
    localStorage.setItem("agentcabin:theme", themeMode);
    document.documentElement.classList.toggle("dark", effectiveDark);
    if (getAppearance().themeMode !== themeMode) {
      setAppearance({ themeMode });
    }
  });

  // Persist color scheme + apply class
  $effect(() => {
    localStorage.setItem("agentcabin:colorScheme", colorScheme);
    document.documentElement.classList.remove(...ALL_SCHEME_CLASSES);
    if (colorScheme && colorScheme !== "neutral") {
      document.documentElement.classList.add(`scheme-${colorScheme}`);
    }
    if (getAppearance().colorScheme !== colorScheme) {
      setAppearance({ colorScheme });
    }
  });

  // Auto-expand folder containing selected run (chats tab only)
  // Track runId + runs.length as change signals. runs.length is the most
  // reliable: it changes on any new run (including resume into existing
  // session where conversationCount stays the same).
  // Don't track expandedProjects itself (otherwise collapsing re-expands).
  let _prevAutoExpandRunId = "";
  let _prevAutoExpandRunsLen = 0;
  $effect(() => {
    if (!isChatPage || panelTab !== "chats") return;
    const runId = selectedRunId;
    const runsLen = runs.length;
    const runChanged = runId !== _prevAutoExpandRunId;
    const runsChanged = runsLen !== _prevAutoExpandRunsLen;
    if (!runChanged && !runsChanged) return; // early-return avoids tracking expandedProjects
    _prevAutoExpandRunId = runId;
    _prevAutoExpandRunsLen = runsLen;
    if (!runId) return;
    const next = autoExpandForRun(runId, projectFolders, expandedProjects);
    if (next) {
      dbg("layout", "auto-expand for run", { selectedRunId: runId });
      expandedProjects = next;
    }
  });

  // Auto-expand folder matching projectCwd (cross-tab sync)
  let _prevAutoExpandCwd = "";
  $effect(() => {
    const cwd = projectCwd;
    if (cwd === _prevAutoExpandCwd) return;
    _prevAutoExpandCwd = cwd;
    if (!cwd) return;
    const folderKey = `cwd:${cwd}`;
    const next = expandForProjectChange(folderKey, expandedProjects);
    if (next) {
      dbg("layout", "auto-expand for cwd change", { cwd });
      expandedProjects = next;
    }
  });

  // Persist expandedProjects + prune stale keys (only after first successful load)
  $effect(() => {
    if (!runsLoadSucceededOnce) return;
    const validKeys = new Set(projectFolders.map((f) => f.folderKey));
    const pruned = [...expandedProjects].filter((k) => validKeys.has(k));
    setSavedExpandedProjects(expandedProjects, appRealm);
  });

  // Note: <html lang> is set by initLocale() and switchLocale() directly.

  // Listen for system preference changes
  onMount(() => {
    if (isPetPage) return;

    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    function onSystemChange(e: MediaQueryListEvent) {
      systemDark = e.matches;
      applyAppearanceCss(getAppearance());
    }
    mq.addEventListener("change", onSystemChange);
    // Apply initial theme
    document.documentElement.classList.toggle("dark", effectiveDark);
    applyAppearanceCss(getAppearance());
    return () => mq.removeEventListener("change", onSystemChange);
  });

  function handleKeydown(e: KeyboardEvent) {
    if (isPetPage) return;
    keybindingStore.dispatch(e);
  }
</script>

{#snippet treeNodes(nodes: TreeNode[])}
  {#each nodes as node}
    <button
      class="flex w-full items-center gap-1 py-0.5 text-[13px] transition-colors
        text-sidebar-foreground hover:bg-sidebar-accent/50
        {explorerSelectedFile === node.fullPath ? 'bg-sidebar-accent/70' : ''}"
      style="padding-left: {8 + node.depth * 12}px"
      onclick={() => (node.is_dir ? toggleFolder(node) : selectFile(node))}
    >
      {#if node.is_dir}
        <svg
          class="h-3 w-3 shrink-0 transition-transform duration-150 {node.expanded
            ? 'rotate-90'
            : ''}"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"><path d="m9 18 6-6-6-6" /></svg
        >
        <svg
          class="h-3.5 w-3.5 shrink-0 text-blue-400/70"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          ><path
            d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
          /></svg
        >
      {:else}
        <span class="w-3 shrink-0"></span>
        <svg
          class="h-3.5 w-3.5 shrink-0 opacity-40"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          ><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" /><path
            d="M14 2v4a2 2 0 0 0 2 2h4"
          /></svg
        >
      {/if}
      <span class="min-w-0 truncate">{node.name}</span>
    </button>
    {#if node.is_dir && node.expanded}
      {@render treeNodes(node.children)}
    {/if}
  {/each}
{/snippet}

<svelte:window onkeydown={handleKeydown} />

<div class="flex h-screen overflow-hidden app-shell" class:hidden={isPetPage}>
  <!-- Sidebar: Project / conversation panel. App navigation lives in Settings. -->
  {#if sidebarOpen}
    <aside
      class="relative z-20 flex shrink-0 bg-sidebar text-sidebar-foreground transition-all duration-200 app-shell-sidebar"
    >
      <!-- A. Icon Rail -->
      <div class="hidden" aria-hidden="true">
        <!-- Rail logo -->
        <div class="flex h-12 w-full items-center justify-center">
          <img src="/logo.png?v=2" alt="AC" class="h-6 w-6 rounded-md opacity-80" />
        </div>

        <!-- Rail nav icons -->
        <nav class="flex flex-1 flex-col items-center gap-1 py-2">
          {#each navItems as item}
            {@const isActive =
              item.path === "/explorer"
                ? currentPath.startsWith("/explorer") ||
                  $page.url.searchParams.get("tab") === "files"
                : currentPath.startsWith(item.path)}
            <a
              href={item.path === "/explorer"
                ? "/chat?tab=files"
                : item.path === "/chat"
                  ? isWorkPage
                    ? "/chat/work"
                    : isPiCodePage
                      ? "/chat/pi"
                      : "/chat"
                  : item.path}
              class="relative flex h-9 w-9 items-center justify-center rounded-md transition-colors duration-150 no-underline
                {isActive
                ? 'bg-sidebar-accent text-sidebar-accent-foreground'
                : 'hover:bg-sidebar-accent/50 text-sidebar-foreground'}"
              title={item.label()}
              onclick={(e) => {
                if (item.path === "/explorer") {
                  e.preventDefault();
                  goto("/chat?tab=files", { replaceState: true, noScroll: true });
                }
              }}
            >
              <!-- Active indicator: subtle dot instead of orange bar -->
              {#if isActive}
                <span
                  class="absolute left-1 top-1/2 -translate-y-1/2 h-1 w-1 rounded-full bg-foreground/30"
                ></span>
              {/if}
              {#if item.icon === "message"}
                <svg
                  class="h-[18px] w-[18px]"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"><path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" /></svg
                >
              {:else if item.icon === "folder"}
                <svg
                  class="h-[18px] w-[18px]"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.5"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  ><path
                    d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
                  /></svg
                >
              {:else if item.icon === "zap"}
                <svg
                  class="h-[18px] w-[18px]"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  ><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" /></svg
                >
              {:else if item.icon === "book"}
                <svg
                  class="h-[18px] w-[18px]"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  ><path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20" /></svg
                >
              {:else if item.icon === "chart"}
                <svg
                  class="h-[18px] w-[18px]"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"><path d="M3 3v18h18" /><path d="m19 9-5 5-4-4-3 3" /></svg
                >
              {:else if item.icon === "clock"}
                <svg
                  class="h-[18px] w-[18px]"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  ><circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" /></svg
                >
              {:else if item.icon === "settings"}
                <svg
                  class="h-[18px] w-[18px]"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  ><path
                    d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"
                  /><circle cx="12" cy="12" r="3" /></svg
                >
              {/if}
              <span class="sr-only">{item.label()}</span>
            </a>
          {/each}
        </nav>

        <!-- Rail locale + dark mode + theme customizer toggle -->
        <div class="py-2 space-y-1">
          <div class="relative mx-auto">
            <button
              class="flex h-9 w-9 items-center justify-center rounded-md text-sidebar-foreground hover:bg-sidebar-accent/50 transition-colors duration-150"
              onclick={() => (localePopupOpen = !localePopupOpen)}
              title={currentLocale()}
            >
              <span class="text-xs font-medium"
                >{getEntry(currentLocale())?.shortLabel ?? currentLocale()}</span
              >
            </button>
            {#if localePopupOpen}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="fixed inset-0 z-40"
                onclick={() => (localePopupOpen = false)}
                onkeydown={(e) => e.key === "Escape" && (localePopupOpen = false)}
              ></div>
              <div
                class="absolute bottom-0 left-full ml-1 z-50 min-w-[140px] rounded-md border border-sidebar-border bg-popover py-1 shadow-lg"
              >
                {#each LOCALE_REGISTRY as entry}
                  <button
                    class="flex w-full items-center gap-2 px-3 py-1.5 text-xs transition-colors
                      {currentLocale() === entry.code
                      ? 'bg-accent text-accent-foreground'
                      : 'text-popover-foreground hover:bg-accent/50'}"
                    onclick={() => handleLocaleSelect(entry.code)}
                  >
                    <span class="w-5 text-center font-medium">{entry.shortLabel}</span>
                    <span>{entry.nativeName}</span>
                    {#if (entry.status as string) === "beta"}
                      <span
                        class="ml-auto text-[10px] text-muted-foreground/60 border border-muted-foreground/20 rounded px-1"
                        >Beta</span
                      >
                    {/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
          <button
            class="flex h-9 w-9 items-center justify-center rounded-md text-sidebar-foreground hover:bg-sidebar-accent/50 transition-colors duration-150"
            onclick={cycleTheme}
            title={themeMode === "dark"
              ? t("layout_themeTitle_dark")
              : themeMode === "light"
                ? t("layout_themeTitle_light")
                : t("layout_themeTitle_system")}
          >
            {#if themeMode === "dark"}
              <!-- Moon icon (dark mode active) -->
              <svg
                class="h-[18px] w-[18px]"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" /></svg
              >
            {:else if themeMode === "light"}
              <!-- Sun icon (light mode active) -->
              <svg
                class="h-[18px] w-[18px]"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                ><circle cx="12" cy="12" r="4" /><path
                  d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"
                /></svg
              >
            {:else}
              <!-- Monitor icon (system mode active) -->
              <svg
                class="h-[18px] w-[18px]"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><rect width="20" height="14" x="2" y="3" rx="2" /><line
                  x1="8"
                  x2="16"
                  y1="21"
                  y2="21"
                /><line x1="12" x2="12" y1="17" y2="21" /></svg
              >
            {/if}
          </button>

          <!-- Theme Customizer Popover (Palette icon click) -->
          <div class="relative mx-auto">
            <button
              class="flex h-9 w-9 items-center justify-center rounded-md text-sidebar-foreground hover:bg-sidebar-accent/50 transition-colors duration-150 {themePopoverOpen
                ? 'bg-sidebar-accent text-sidebar-primary'
                : ''}"
              onclick={() => (themePopoverOpen = !themePopoverOpen)}
              title="主题与外观定制"
            >
              <!-- Palette icon -->
              <svg
                class="h-[18px] w-[18px]"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><circle cx="13.5" cy="6.5" r=".5" fill="currentColor" /><circle
                  cx="17.5"
                  cy="10.5"
                  r=".5"
                  fill="currentColor"
                /><circle cx="8.5" cy="7.5" r=".5" fill="currentColor" /><circle
                  cx="6.5"
                  cy="12"
                  r=".5"
                  fill="currentColor"
                /><path
                  d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h1.996c3.051 0 5.555-2.503 5.555-5.554C21.965 6.012 17.461 2 12 2z"
                /></svg
              >
            </button>

            {#if themePopoverOpen}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="fixed inset-0 z-40"
                onclick={() => (themePopoverOpen = false)}
                onkeydown={(e) => e.key === "Escape" && (themePopoverOpen = false)}
              ></div>
              <div
                class="absolute bottom-0 left-full ml-2 z-50 w-72 rounded-2xl border border-sidebar-border/80 bg-popover/95 p-3.5 shadow-2xl backdrop-blur-md animate-in fade-in slide-in-from-left-2 duration-150 text-popover-foreground text-left"
              >
                <!-- Header -->
                <div class="flex items-center justify-between pb-2 border-b border-border/40 mb-3">
                  <div class="flex items-center gap-1.5 font-medium text-xs">
                    <svg
                      class="h-4 w-4 text-primary"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                      ><path
                        d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"
                      /></svg
                    >
                    <span>主题与外观定制</span>
                  </div>
                  <button
                    class="text-muted-foreground hover:text-foreground text-xs p-1"
                    onclick={() => (themePopoverOpen = false)}
                  >
                    ✕
                  </button>
                </div>

                <!-- Appearance Mode -->
                <div class="mb-3.5">
                  <div
                    class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60 mb-1.5"
                  >
                    外观模式
                  </div>
                  <div
                    class="grid grid-cols-3 gap-1 rounded-xl bg-accent/40 p-1 border border-border/40"
                  >
                    <button
                      class="rounded-lg py-1.5 text-xs transition-colors flex items-center justify-center gap-1 {themeMode ===
                      'dark'
                        ? 'bg-background text-foreground shadow-xs font-medium'
                        : 'text-muted-foreground hover:text-foreground'}"
                      onclick={() => (themeMode = "dark")}
                    >
                      <span>🌙</span> <span>深色</span>
                    </button>
                    <button
                      class="rounded-lg py-1.5 text-xs transition-colors flex items-center justify-center gap-1 {themeMode ===
                      'light'
                        ? 'bg-background text-foreground shadow-xs font-medium'
                        : 'text-muted-foreground hover:text-foreground'}"
                      onclick={() => (themeMode = "light")}
                    >
                      <span>☀️</span> <span>浅色</span>
                    </button>
                    <button
                      class="rounded-lg py-1.5 text-xs transition-colors flex items-center justify-center gap-1 {themeMode ===
                      'system'
                        ? 'bg-background text-foreground shadow-xs font-medium'
                        : 'text-muted-foreground hover:text-foreground'}"
                      onclick={() => (themeMode = "system")}
                    >
                      <span>💻</span> <span>自动</span>
                    </button>
                  </div>
                </div>

                <!-- Accent Color Schemes -->
                <div>
                  <div
                    class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60 mb-1.5"
                  >
                    配色主题 (Accent Presets)
                  </div>
                  <div class="grid grid-cols-2 gap-1.5">
                    {#each [{ id: "neutral", name: "Codex 极简灰", color: "bg-neutral-500" }, { id: "warm", name: "Sand 暖沙", color: "bg-amber-500" }, { id: "emerald", name: "Mint 薄荷绿", color: "bg-emerald-500" }, { id: "violet", name: "Velvet 罗兰紫", color: "bg-violet-500" }, { id: "ocean", name: "Ocean 海洋蓝", color: "bg-blue-500" }, { id: "rose", name: "Sunset 玫瑰红", color: "bg-rose-500" }] as scheme}
                      <button
                        class="flex items-center gap-2 rounded-xl px-2.5 py-2 text-xs border transition-all text-left {colorScheme ===
                        scheme.id
                          ? 'border-primary bg-primary/10 text-foreground font-medium'
                          : 'border-border/40 hover:bg-accent/40 text-muted-foreground'}"
                        onclick={() => selectColorScheme(scheme.id)}
                      >
                        <span class="h-3 w-3 rounded-full shrink-0 {scheme.color}"></span>
                        <span class="truncate">{scheme.name}</span>
                      </button>
                    {/each}
                  </div>
                </div>
              </div>
            {/if}
          </div>
        </div>
      </div>
      <!-- Content Panel (hidden when on /settings page) -->
      {#if !isSettingsPage}
        <div
          class="flex flex-none flex-col overflow-hidden border-r border-sidebar-border/50 relative app-shell-sidebar-panel chat-sidebar-panel"
          class:work-mode={isWorkPage}
          style:width="{sidebarWidth}px"
          style:min-width="{SIDEBAR_WIDTH_MIN}px"
        >
          <!-- Panel header: Brand identity & Mode selection (TianGong Work style) -->
          <div
            class="app-shell-sidebar-header mode-header-multiline shrink-0 flex flex-col justify-end px-3.5 {IS_MAC
              ? 'pt-9 pb-2.5'
              : 'pt-3 pb-2.5'}"
            data-window-drag-region
            data-tauri-drag-region
          >
            {#if isChatPage || isMemoryPage || isWorkPage || isPluginsPage || currentPath === "/"}
              <div class="flex items-center gap-2.5 w-full">
                <img
                  src="/logo.png"
                  alt="AgentCabin Logo"
                  class="h-8 w-8 rounded-xl shadow-xs object-cover shrink-0 select-none pointer-events-none"
                />
                <div class="flex flex-col min-w-0 flex-1 leading-tight justify-center">
                  <span
                    class="truncate text-[14px] font-bold tracking-tight text-sidebar-foreground whitespace-nowrap"
                  >
                    {t("layout_appName")}
                  </span>
                  <span
                    class="text-[10.5px] text-muted-foreground/75 font-normal tracking-wide truncate mt-0.5 whitespace-nowrap"
                  >
                    {t("layout_appSubtitle")}
                  </span>
                </div>
                <div class="shrink-0">
                  <ModeSwitcher
                    realm={appRealm}
                    {piSubMode}
                    onSelectRealm={handleRealmSelect}
                    workEnabled={settings?.work_mode_enabled !== false &&
                      getTransport().isDesktop()}
                  />
                </div>
              </div>
            {:else}
              <div class="flex h-9 w-full items-center">
                <span class="flex-1 min-w-0 truncate text-sm font-medium text-sidebar-foreground"
                  >{pageName}</span
                >
              </div>
            {/if}
          </div>

          {#if chatSearchOpen}
            <div class="px-2 pt-2 pb-1 shrink-0 app-shell-sidebar-search">
              <div class="relative">
                <svg
                  class="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/60"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="m20 20-4-4" /></svg
                >
                <input
                  bind:this={chatSearchInput}
                  type="text"
                  bind:value={runSearchQuery}
                  oninput={onDeepQueryInput}
                  onkeydown={(e) => e.key === "Escape" && closeChatSearch()}
                  placeholder={isWorkPage ? "搜索 Work 对话" : t("sidebar_searchChats")}
                  class="w-full rounded-md border border-sidebar-border bg-sidebar py-1 text-xs text-sidebar-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:border-ring/50 app-shell-sidebar-search-input app-shell-sidebar-search-input--with-adornments"
                />
                <button
                  class="absolute right-1 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded text-muted-foreground/60 hover:bg-sidebar-accent/60 hover:text-sidebar-foreground"
                  onclick={closeChatSearch}
                  title={t("common_close")}
                  aria-label={t("common_close")}
                >
                  <svg
                    class="h-3.5 w-3.5"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"><path d="M6 6l12 12M18 6 6 18" /></svg
                  >
                </button>
              </div>
              {#if runSearchQuery.trim()}
                {#if searching}
                  <p class="px-1 pt-0.5 text-xs text-muted-foreground">
                    {t("runs_searching")}
                  </p>
                {:else if visibleSearchResults.length > 0}
                  <p
                    class="flex items-center justify-between px-1 pt-0.5 text-xs text-muted-foreground"
                  >
                    <span
                      >{t("runs_resultsCount", {
                        count: String(visibleSearchResults.length),
                      })}</span
                    >
                  </p>
                {/if}
              {/if}
            </div>

            {#if runSearchQuery.trim()}
              <!-- Search results -->
              <div class="flex-1 overflow-y-auto px-2 py-1">
                {#if searching && visibleSearchResults.length === 0}
                  <div class="flex items-center justify-center py-10">
                    <div
                      class="h-4 w-4 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
                    ></div>
                  </div>
                {:else if !searching && visibleSearchResults.length === 0}
                  <div class="flex items-center justify-center px-3 py-10 text-center">
                    <p class="text-xs text-muted-foreground">{t("runs_noMatching")}</p>
                  </div>
                {:else}
                  {#each visibleSearchResults as result}
                    <button
                      class="w-full text-left flex flex-col gap-0.5 px-3 py-2 rounded-lg hover:bg-sidebar-accent/50 transition-colors text-sidebar-foreground"
                      onclick={() => {
                        closeChatSearch();
                        const run = runs.find((r) => r.id === result.runId);
                        const target = run
                          ? getRunRoute(run, { runId: result.runId })
                          : isWorkPage
                            ? getTargetRoute("work", { runId: result.runId })
                            : isPiCodePage
                              ? getTargetRoute("pi:code", { runId: result.runId })
                              : getTargetRoute("native:claude", { runId: result.runId });
                        const baseRoute = target;
                        const sep = baseRoute.includes("?") ? "&" : "?";
                        goto(
                          `${baseRoute}${sep}scrollTo=${encodeURIComponent(result.matchedEventId || result.matchedTs)}`,
                        );
                      }}
                    >
                      <p class="text-[12px] min-w-0 line-clamp-2 break-all">
                        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                        {@html highlightMatch(
                          snippetAround(result.matchedText, runSearchQuery, 80),
                          runSearchQuery,
                        )}
                      </p>
                      <div
                        class="flex items-center gap-1 text-[10px] text-muted-foreground min-w-0"
                      >
                        <span class="flex-1 min-w-0 truncate"
                          >{result.runName || truncate(result.runPrompt, 30)}</span
                        >
                        <span class="ml-auto shrink-0">{relativeTime(result.matchedTs)}</span>
                      </div>
                    </button>
                  {/each}
                {/if}
              </div>
            {/if}
          {:else if !isExplorerPage}
            <div class="border-b border-sidebar-border/50 p-2 shrink-0 space-y-0.5">
              <!-- New conversation for the current Native / Pi Code mode -->
              <SidebarNavItem label={t("sidebar_newChat")} onclick={newChat}>
                {#snippet icon()}
                  <svg
                    viewBox="0 0 24 24"
                    class="h-3.5 w-3.5 shrink-0 text-primary"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    aria-hidden="true"
                  >
                    <path d="M12 5v14M5 12h14" />
                  </svg>
                {/snippet}
              </SidebarNavItem>

              <!-- Archived (已归档) -->
              <SidebarNavItem
                href="{isWorkPage
                  ? '/chat/work'
                  : isPiCodePage
                    ? '/chat/pi'
                    : '/chat'}?view=archived"
                label="已归档"
                active={$page.url.searchParams.get("view") === "archived"}
                badge={archivedRunsCount}
              >
                {#snippet icon()}
                  <svg
                    viewBox="0 0 24 24"
                    class="h-3.5 w-3.5 shrink-0 text-muted-foreground"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <rect x="2" y="3" width="20" height="5" rx="1" />
                    <path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8" />
                    <path d="M10 12h4" />
                  </svg>
                {/snippet}
              </SidebarNavItem>
            </div>
          {/if}

          {#if !chatSearchOpen && isWorkPage}
            <div class="work-mode-sidebar flex min-h-0 flex-1 flex-col">
              <WorkSidebar />
            </div>
          {/if}

          {#if isExplorerPage}
            <!-- Explorer tab bar: Files / Git -->
            <div class="flex shrink-0 border-b border-sidebar-border/50 app-shell-sidebar-tabs">
              <button
                class="flex-1 py-1.5 text-xs font-medium text-center transition-colors
              {explorerTab === 'files'
                  ? 'text-sidebar-foreground border-b-2 border-primary'
                  : 'text-muted-foreground hover:text-sidebar-foreground'}"
                onclick={() => (explorerTab = "files")}>{t("sidebar_files")}</button
              >
              <button
                class="relative flex-1 py-1.5 text-xs font-medium text-center transition-colors
              {explorerTab === 'git'
                  ? 'text-sidebar-foreground border-b-2 border-primary'
                  : 'text-muted-foreground hover:text-sidebar-foreground'}"
                onclick={() => (explorerTab = "git")}
                >{t("sidebar_git")}
                {#if gitSummary && gitSummary.total_files > 0}
                  <span
                    class="ml-0.5 inline-flex h-3.5 min-w-[14px] items-center justify-center rounded-full bg-blue-500/80 px-1 text-[10px] font-bold text-white"
                    >{gitSummary.total_files}</span
                  >
                {/if}
              </button>
            </div>

            <!-- Compact project picker (below tabs) -->
            <div class="relative shrink-0 border-b border-sidebar-border/50">
              <button
                class="flex w-full items-center gap-1.5 px-2.5 py-1.5 text-xs transition-colors hover:bg-sidebar-accent/50"
                onclick={() => (explorerProjectOpen = !explorerProjectOpen)}
              >
                <svg
                  class="h-3.5 w-3.5 shrink-0 text-muted-foreground/70"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  ><path
                    d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
                  /></svg
                >
                <span class="min-w-0 truncate text-sidebar-foreground"
                  >{projectCwd
                    ? cwdDisplayLabel(projectCwd)
                    : t("sidebar_selectProjectBrowse")}</span
                >
                <svg
                  class="ml-auto h-3 w-3 shrink-0 text-muted-foreground/50 transition-transform {explorerProjectOpen
                    ? 'rotate-180'
                    : ''}"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"><path d="m6 9 6 6 6-6" /></svg
                >
              </button>
              {#if explorerProjectOpen}
                <div class="border-b border-sidebar-border/50 bg-sidebar">
                  {#each selectableFolders as folder (folder.folderKey)}
                    <button
                      class="flex w-full items-center gap-1.5 px-2.5 py-1.5 text-xs transition-colors
                      {folder.cwd === projectCwd
                        ? 'bg-sidebar-accent text-sidebar-foreground'
                        : 'text-muted-foreground hover:bg-sidebar-accent/50 hover:text-sidebar-foreground'}"
                      onclick={() => {
                        projectCwd = folder.cwd;
                        explorerProjectOpen = false;
                      }}
                    >
                      <svg
                        class="h-3 w-3 shrink-0 text-muted-foreground/70"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        ><path
                          d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
                        /></svg
                      >
                      <span class="min-w-0 truncate">{cwdDisplayLabel(folder.cwd)}</span>
                    </button>
                  {/each}
                  <button
                    class="flex w-full items-center gap-1.5 px-2.5 py-1.5 text-xs text-muted-foreground hover:text-sidebar-foreground hover:bg-sidebar-accent/50 transition-colors"
                    onclick={() => {
                      pickFolder();
                      explorerProjectOpen = false;
                    }}
                  >
                    <svg
                      class="h-3 w-3 shrink-0"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                      stroke-linecap="round"
                      stroke-linejoin="round"><path d="M12 5v14" /><path d="M5 12h14" /></svg
                    >
                    <span>{t("project_openFolder")}</span>
                  </button>
                </div>
              {/if}
            </div>

            <!-- Explorer tab content -->
            {#if explorerTab === "files"}
              <div class="flex-1 overflow-y-auto px-1 py-1">
                {#if !projectCwd}
                  {@const lastRemote = getLastTarget()}
                  <div class="flex items-center justify-center px-3 py-12">
                    <p class="text-xs text-muted-foreground text-center">
                      {lastRemote
                        ? t("layout_remoteFileTreeUnavailable")
                        : t("sidebar_selectProjectBrowse")}
                    </p>
                  </div>
                {:else if treeLoading}
                  <div class="flex items-center justify-center py-12">
                    <div
                      class="h-4 w-4 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
                    ></div>
                  </div>
                {:else if fileTree.length === 0}
                  <p class="px-2 py-8 text-xs text-muted-foreground text-center">
                    {t("sidebar_emptyDirectory")}
                  </p>
                {:else}
                  {@render treeNodes(fileTree)}
                {/if}
              </div>
            {:else}
              <!-- Git tab -->
              {#if !projectCwd}
                <div class="flex-1 flex items-center justify-center px-3">
                  <p class="text-xs text-muted-foreground text-center">
                    {t("sidebar_selectProjectGit")}
                  </p>
                </div>
              {:else if gitLoading}
                <div class="flex-1 flex items-center justify-center">
                  <div
                    class="h-4 w-4 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
                  ></div>
                </div>
              {:else if !gitSummary}
                <div class="flex-1 flex items-center justify-center px-3">
                  <p class="text-xs text-muted-foreground text-center">
                    {t("sidebar_notGitRepo")}
                  </p>
                </div>
              {:else}
                <!-- Branch info -->
                <div
                  class="flex items-center gap-1.5 px-3 py-2 border-b border-sidebar-border/50 shrink-0"
                >
                  <svg
                    class="h-3 w-3 shrink-0 text-muted-foreground"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    ><circle cx="12" cy="12" r="3" /><line x1="3" x2="9" y1="12" y2="12" /><line
                      x1="15"
                      x2="21"
                      y1="12"
                      y2="12"
                    /></svg
                  >
                  <span class="text-[12px] font-medium text-sidebar-foreground min-w-0 truncate"
                    >{gitSummary.branch || t("sidebar_detached")}</span
                  >
                  <button
                    class="ml-auto flex h-5 w-5 items-center justify-center rounded text-muted-foreground hover:text-sidebar-foreground hover:bg-sidebar-accent/50 transition-colors"
                    onclick={loadGitSummary}
                    title={t("sidebar_refresh")}
                  >
                    <svg
                      class="h-3 w-3"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      ><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path
                        d="M3 3v5h5"
                      /><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" /><path
                        d="M16 16h5v5"
                      /></svg
                    >
                  </button>
                </div>
                <!-- Summary -->
                {#if gitSummary.total_files > 0}
                  <div
                    class="flex items-center gap-2 px-3 py-1.5 text-xs text-muted-foreground border-b border-sidebar-border/50 shrink-0"
                  >
                    <span class="tabular-nums"
                      >{gitSummary.total_files !== 1
                        ? t("sidebar_changedFiles", { count: String(gitSummary.total_files) })
                        : t("sidebar_changedFile", {
                            count: String(gitSummary.total_files),
                          })}</span
                    >
                    {#if gitSummary.total_insertions > 0}
                      <span class="text-green-500 tabular-nums">+{gitSummary.total_insertions}</span
                      >
                    {/if}
                    {#if gitSummary.total_deletions > 0}
                      <span class="text-red-400 tabular-nums">-{gitSummary.total_deletions}</span>
                    {/if}
                  </div>
                  <!-- Changed files list -->
                  <div class="flex-1 overflow-y-auto">
                    {#each gitSummary.files as file}
                      <button
                        class="flex w-full items-center gap-1.5 px-3 py-1 text-[12px] hover:bg-sidebar-accent/50 transition-colors"
                        onclick={() => selectDiffFile(file.path)}
                      >
                        <span
                          class="w-3 shrink-0 text-center font-mono text-[10px] font-bold {GIT_STATUS_COLORS[
                            file.status
                          ] ?? 'text-muted-foreground'}">{file.status}</span
                        >
                        <span class="flex-1 min-w-0 truncate text-sidebar-foreground text-left"
                          >{file.path}</span
                        >
                        {#if file.insertions > 0}
                          <span class="text-[10px] text-green-500">+{file.insertions}</span>
                        {/if}
                        {#if file.deletions > 0}
                          <span class="text-[10px] text-red-400">-{file.deletions}</span>
                        {/if}
                      </button>
                    {/each}
                  </div>
                {:else}
                  <div class="flex-1 flex items-center justify-center px-3">
                    <div class="flex flex-col items-center gap-1.5 text-center">
                      <svg
                        class="h-6 w-6 text-muted-foreground/30"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.5"><path d="M20 6 9 17l-5-5" /></svg
                      >
                      <p class="text-xs text-muted-foreground">{t("sidebar_workingTreeClean")}</p>
                    </div>
                  </div>
                {/if}
              {/if}
            {/if}
          {:else}
            <!-- Tab content -->
            {#if panelTab === "chats"}
              <!-- Project folder tree -->
              <div
                class="flex-1 overflow-y-auto px-2 py-2 app-shell-sidebar-content chat-sidebar-content"
              >
                <!-- Workspace Header with Micro Actions -->
                <SidebarSectionLabel
                  label={t("layout_workspace_title")}
                  count={selectableFolders.length}
                  class="mb-0.5 shrink-0"
                >
                  {#snippet actions()}
                    <!-- 搜索会话 -->
                    <button
                      type="button"
                      class="flex h-6 w-6 items-center justify-center rounded-md text-sidebar-foreground/45 hover:bg-sidebar-accent/60 hover:text-sidebar-foreground transition-colors"
                      class:bg-sidebar-accent={chatSearchOpen}
                      onclick={toggleChatSearch}
                      title={t("sidebar_searchChats")}
                      aria-label={t("sidebar_searchChats")}
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
                        <circle cx="11" cy="11" r="7" /><path d="m20 20-4-4" />
                      </svg>
                    </button>
                    <!-- 添加工作区/打开文件夹 -->
                    <button
                      type="button"
                      class="flex h-6 w-6 items-center justify-center rounded-md text-sidebar-foreground/45 hover:bg-sidebar-accent/60 hover:text-sidebar-foreground transition-colors"
                      onclick={pickFolder}
                      title={t("project_openFolder")}
                      aria-label={t("project_openFolder")}
                    >
                      <svg
                        class="h-3.5 w-3.5"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      >
                        <path d="M12 5v14M5 12h14" />
                      </svg>
                    </button>
                  {/snippet}
                </SidebarSectionLabel>

                <div class="mt-1 space-y-0.5">
                  {#each projectFolders as folder (folder.folderKey)}
                    <ProjectFolderItem
                      {folder}
                      label={folder.isUncategorized
                        ? t("sidebar_uncategorized")
                        : cwdDisplayLabel(folder.cwd)}
                      expanded={expandedProjects.has(folder.folderKey)}
                      {selectedRunId}
                      onToggle={() => toggleProject(folder.folderKey)}
                      onSelectConversation={(runId) => {
                        // Navigate immediately for instant UI response
                        const run = runs.find((r) => r.id === runId);
                        const target = run
                          ? getRunRoute(run, { runId })
                          : isPiCodePage
                            ? getTargetRoute("pi:code", { runId })
                            : getTargetRoute("native:claude", { runId });
                        goto(target);

                        // Clear unread badge asynchronously in background (non-blocking)
                        const conv = projectFolders
                          .flatMap((f) => f.conversations)
                          .find((c) => c.runs.some((r) => r.id === runId));
                        if (conv?.unread) {
                          setRunFlags(conv.latestRun.id, { unread: false })
                            .then(() =>
                              dispatchRunMutation({
                                kind: "update",
                                runId: conv.latestRun.id,
                                patch: { unread: false },
                              }),
                            )
                            .catch((e) => dbgWarn("layout", "clear unread failed", e));
                        }
                      }}
                      onDelete={requestDeleteConversation}
                      onEnd={endConversation}
                      onRemove={folder.isUncategorized
                        ? undefined
                        : () => requestRemoveProject(folder.cwd)}
                      onNewChat={folder.isUncategorized
                        ? undefined
                        : () => newChatInFolder(folder.cwd)}
                    />
                  {/each}

                  <!-- Open folder... -->
                  <button
                    type="button"
                    class="flex w-full items-center gap-2 rounded-lg border border-dashed border-sidebar-border/60 px-2.5 py-1.5 text-xs text-sidebar-foreground/50 transition-colors hover:border-sidebar-border hover:bg-sidebar-accent/40 hover:text-sidebar-foreground mt-1"
                    onclick={pickFolder}
                  >
                    <svg
                      viewBox="0 0 24 24"
                      class="h-3.5 w-3.5 text-sidebar-foreground/40"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                      stroke-linecap="round"
                    >
                      <path d="M12 5v14M5 12h14" />
                    </svg>
                    <span>{t("project_openFolder")}</span>
                  </button>
                </div>

                {#if projectFolders.length === 0}
                  <div class="flex flex-col items-center gap-2 px-3 py-6 text-center">
                    <p class="text-xs text-muted-foreground">
                      {t("sidebar_noConversationsYet")}<br />{t("sidebar_startNewChat")}
                    </p>
                  </div>
                {/if}
              </div>
            {/if}
          {/if}

          <!-- Utility footer: navigation is managed from Settings, while these small
               controls are managed from the Appearance settings page. -->
          <div
            class="flex shrink-0 items-center justify-end gap-1 border-t border-sidebar-border/50 px-2 py-2 app-shell-sidebar-footer"
          >
            <a
              href="/settings"
              class="flex h-7 items-center gap-1.5 rounded-md px-2 text-xs text-sidebar-foreground hover:bg-sidebar-accent/50 transition-colors no-underline"
              title={t("nav_settings")}
            >
              <svg
                class="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><path
                  d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"
                /><circle cx="12" cy="12" r="3" /></svg
              >
              <span class="hidden min-[240px]:inline">{t("nav_settings")}</span>
            </a>
          </div>
          <!-- Resize handle -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="work-mode-resize absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-primary/20 active:bg-primary/30 transition-colors z-10"
            onpointerdown={startResize}
          ></div>
        </div>
      {/if}
    </aside>
  {/if}

  <!-- Ghost line during sidebar drag (zero-reflow preview) -->
  {#if sidebarResizing}
    <div
      bind:this={sidebarGhostEl}
      class="fixed top-0 bottom-0 z-[9999] pointer-events-none bg-primary"
      style="left: {sidebarGhostX -
        1}px; width: 3px; box-shadow: 0 0 8px hsl(var(--primary) / 0.6);"
    ></div>
  {/if}

  <!-- Main content -->
  <div class="flex flex-1 flex-col overflow-hidden">
    <UpdateBanner />
    <!-- Top bar (non-chat and non-settings pages only) -->
    {#if !isChatPage && !isSettingsPage && currentPath !== "/"}
      <header
        class="flex h-11 items-center gap-3 border-b px-4 {IS_MAC && !sidebarOpen
          ? 'pl-[76px]'
          : ''}"
        data-window-drag-region
        data-tauri-drag-region
      >
        <button
          class="rounded-md p-1.5 hover:bg-accent transition-all duration-150"
          onclick={toggleSidebar}
          title={t("layout_toggleSidebar")}
        >
          <svg
            class="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            ><rect width="18" height="18" x="3" y="3" rx="2" /><path d="M9 3v18" /></svg
          >
        </button>

        <div class="flex items-center gap-2 text-sm">
          <span class="text-muted-foreground">{t("layout_appName")}</span>
          <svg
            class="h-3.5 w-3.5 text-muted-foreground/50"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"><path d="m9 18 6-6-6-6" /></svg
          >
          <span class="font-medium">{pageName}</span>
        </div>
      </header>
    {/if}

    <!-- Page content -->
    <main
      class="min-h-0 flex-1 {isSettingsPage || isWorkPage ? 'overflow-hidden' : 'overflow-y-auto'}"
    >
      {#if !isPetPage}
        {@render children()}
      {/if}
    </main>
  </div>
</div>

<CommandPalette
  bind:open={commandPaletteOpen}
  cwd={projectCwd || "/"}
  onOpenFolderBrowser={pickFolder}
/>

{#if showSetupWizard}
  <SetupWizard
    onComplete={handleSetupComplete}
    onBack={setupWizardCanGoBack ? handleSetupBack : undefined}
  />
{/if}

<AboutModal bind:open={showAbout} />

<PermissionsModal bind:open={permissionsModalOpen} cwd={projectCwd} />

<FolderPicker
  bind:open={folderPickerOpen}
  initialHost={folderPickerInitialHost}
  initialPath={folderPickerInitialPath}
  onConfirm={onFolderPicked}
/>

{#if showCliBrowser}
  <CliSessionBrowser
    cwd={cliBrowserCwd}
    onclose={() => (showCliBrowser = false)}
    onimported={(runId) => {
      showCliBrowser = false;
      loadRuns();
      goto(`/chat?run=${runId}`);
    }}
  />
{/if}

<Modal bind:open={deleteConfirmOpen} title={t("sidebar_deleteConfirm")}>
  <p class="text-sm text-muted-foreground mb-4">{t("sidebar_deleteDesc")}</p>
  <div class="flex justify-end gap-2">
    <button
      class="px-3 py-1.5 text-sm rounded-md border border-border hover:bg-accent transition-colors"
      onclick={cancelDeleteConversation}
    >
      {t("sidebar_deleteCancel")}
    </button>
    <button
      class="px-3 py-1.5 text-sm rounded-md bg-destructive text-destructive-foreground hover:bg-destructive/90 transition-colors"
      onclick={confirmDeleteConversation}
    >
      {t("sidebar_deleteOk")}
    </button>
  </div>
</Modal>

<Modal bind:open={removeProjectConfirmOpen} title={t("sidebar_removeProjectConfirm")}>
  <p class="text-sm text-muted-foreground mb-4">{t("sidebar_removeProjectDesc")}</p>
  <div class="flex justify-end gap-2">
    <button
      class="px-3 py-1.5 text-sm rounded-md border border-border hover:bg-accent transition-colors"
      onclick={cancelRemoveProject}
    >
      {t("sidebar_deleteCancel")}
    </button>
    <button
      class="px-3 py-1.5 text-sm rounded-md bg-destructive text-destructive-foreground hover:bg-destructive/90 transition-colors"
      onclick={confirmRemoveProject}
    >
      {t("sidebar_deleteOk")}
    </button>
  </div>
</Modal>

{#if isPetPage}
  <div class="h-screen w-screen overflow-hidden bg-transparent">
    {@render children()}
  </div>
{/if}

<style>
  :global(.app-shell-sidebar-header.mode-header-multiline) {
    height: 52px !important;
    min-height: 52px !important;
  }

  :global(.app-shell-sidebar-header.mode-header-multiline.pt-9),
  :global(.app-shell-sidebar-header.mode-header-multiline.pt-7) {
    height: 84px !important;
    min-height: 84px !important;
  }

  :global(
    .app-shell-sidebar-panel.work-mode
      > :not(.app-shell-sidebar-header):not(.work-mode-sidebar):not(.app-shell-sidebar-footer):not(
        .work-mode-resize
      )
  ) {
    display: none;
  }
</style>
