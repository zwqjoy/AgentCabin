<script lang="ts">
  import { onMount } from "svelte";
  import type { TaskRun, McpServerInfo, CliModelInfo } from "$lib/types";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { revealInFinder } from "$lib/api";
  import { getModelsForAgent } from "$lib/stores/cli-info.svelte";
  import {
    getAgentDisplayName,
    RUNTIME_PROVIDERS_CONFIG,
    type RuntimeProviderId,
  } from "$lib/utils/agent-metadata";
  import { t } from "$lib/i18n/index.svelte";
  import { stripExpertTag } from "$lib/utils/expert-context";
  import { IS_MAC } from "$lib/utils/platform";
  import { filterPiThinkingLevelsForUi } from "$lib/utils/pi-thinking";
  import VscodeIcon from "./VscodeIcon.svelte";

  let {
    run = null,
    agent = "claude",
    model = "",
    running = false,
    parentRunId,
    onModelChange,
    projectDefaultModel = "",
    onSetProjectDefault,
    projectDefaultLabel,
    setProjectDefaultLabel,
    onNavigateParent,
    onToggleSidebar,
    mcpServers,
    onMcpToggle,
    persistedFiles,
    onRewind,
    cwd = "",
    activeTaskCount = 0,
    mode = "",
    toolsCount = 0,
    onToolsClick,
    onRename,
    platformModels = [],
    effort,
    onEffortChange,
    vscodeAvailable = null,
    onOpenVscode,
    onOpenWorktrees,
    onPreviewToggle,
    onStatusClick,
    onToggleBottomPanel,
    bottomPanelOpen = false,
    onToggleRightSidebar,
    rightSidebarOpen = false,
    sidebarOpen = true,
    modelOptions,
    onSearch,
    searchOpen = false,
  }: {
    run?: TaskRun | null;
    agent?: string;
    model?: string;
    running?: boolean;
    parentRunId?: string;
    onModelChange?: (model: string) => void;
    projectDefaultModel?: string;
    onSetProjectDefault?: () => void | Promise<void>;
    projectDefaultLabel?: string;
    setProjectDefaultLabel?: string;
    onNavigateParent?: () => void;
    onToggleSidebar?: () => void;
    mcpServers?: McpServerInfo[];
    onMcpToggle?: () => void;
    persistedFiles?: unknown[];
    onRewind?: () => void;
    cwd?: string;
    activeTaskCount?: number;
    mode?: string;
    toolsCount?: number;
    onToolsClick?: () => void;
    onRename?: (name: string) => void;
    platformModels?: CliModelInfo[];
    effort?: string;
    onEffortChange?: (effort: string) => void;
    vscodeAvailable?: boolean | null;
    onOpenVscode?: () => void;
    onOpenWorktrees?: () => void;
    onPreviewToggle?: () => void;
    onStatusClick?: () => void;
    onToggleBottomPanel?: () => void;
    bottomPanelOpen?: boolean;
    onToggleRightSidebar?: () => void;
    rightSidebarOpen?: boolean;
    sidebarOpen?: boolean;
    /** Optional agent-scoped model list (for global Provider multi-model bindings). */
    modelOptions?: CliModelInfo[];
    onSearch?: () => void;
    searchOpen?: boolean;
  } = $props();

  const agentDisplayName = $derived(getAgentDisplayName(agent));
  const agentDotClass = $derived(
    RUNTIME_PROVIDERS_CONFIG[agent as RuntimeProviderId]?.dotClass ?? "bg-muted-foreground/60",
  );

  // Work may use different providers. Keep the primary status bar honest about
  // the selected runtime instead of baking the current Pi implementation into
  // the product-mode label.
  let visibleAgentDisplayName = $derived(agentDisplayName);

  $effect(() => {
    dbg("status", "state", { agent, model, running, runId: run?.id });
  });

  let moreMenuOpen = $state(false);

  let sidCopied = $state(false);
  let pathCopied = $state(false);

  async function copySessionId() {
    if (!run?.session_id) return;
    try {
      await navigator.clipboard.writeText(run.session_id);
      sidCopied = true;
      setTimeout(() => (sidCopied = false), 1500);
    } catch {
      /* ignore */
    }
  }

  async function copyPath() {
    const val = cwd || run?.cwd || "";
    if (!val) return;
    try {
      await navigator.clipboard.writeText(val);
      pathCopied = true;
      setTimeout(() => (pathCopied = false), 1500);
    } catch {
      /* ignore */
    }
  }

  async function handleReveal() {
    const val = cwd || run?.cwd || "";
    if (!val) return;
    try {
      moreMenuOpen = false;
      await revealInFinder(val);
    } catch (e) {
      dbgWarn("SessionStatusBar", "revealInFinder failed", e);
    }
  }

  // ── Title inline editing ──
  let titleEditing = $state(false);
  let titleEditValue = $state("");
  let titleInputEl: HTMLInputElement | undefined = $state();

  function startTitleEdit() {
    if (!onRename || !run) return;
    titleEditValue = run.name || run.prompt;
    titleEditing = true;
    requestAnimationFrame(() => titleInputEl?.select());
  }

  function commitTitleEdit() {
    titleEditing = false;
    const trimmed = titleEditValue.trim();
    if (trimmed && run && trimmed !== (run.name || run.prompt)) {
      onRename?.(trimmed);
    }
  }

  function cancelTitleEdit() {
    titleEditing = false;
  }

  // ── Model selector dropdown ──
  // Use platform-specific models when a third-party provider is active
  let models = $derived(
    modelOptions?.length ? modelOptions : getModelsForAgent(agent, { platformModels }),
  );
  let dropdownOpen = $state(false);
  let focusedModelIdx = $state(-1);
  let modelBtnEl: HTMLButtonElement | undefined = $state();
  let dropdownEl: HTMLDivElement | undefined = $state();
  let dropdownStyle = $state("");

  // ── More Menu refs ──
  let moreMenuBtnEl: HTMLButtonElement | undefined = $state();
  let moreMenuEl: HTMLDivElement | undefined = $state();

  // Menus are transient UI. If a run is stopped while one is open, its fixed
  // click-catcher must not survive the lifecycle transition and block the rest
  // of the application. Close both selectors whenever the session is no
  // longer alive or has reached a terminal state.
  $effect(() => {
    const terminal = ["completed", "failed", "stopped", "cancelled"].includes(run?.status ?? "");
    if (!running || terminal) {
      moreMenuOpen = false;
      dropdownOpen = false;
    }
  });

  export function openModelDropdown() {
    dropdownOpen = true;
    if (modelBtnEl) {
      const rect = modelBtnEl.getBoundingClientRect();
      dropdownStyle = `position:fixed; bottom:${window.innerHeight - rect.top + 4}px; left:${rect.left}px; z-index:50;`;
    }
    focusedModelIdx = models.findIndex((m) => m.value === model);
    if (focusedModelIdx < 0) focusedModelIdx = 0;
    requestAnimationFrame(() => dropdownEl?.focus());
  }

  function selectModel(val: string) {
    dropdownOpen = false;
    onModelChange?.(val);
  }

  function setProjectDefault() {
    dropdownOpen = false;
    void onSetProjectDefault?.();
  }

  function handleDropdownKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      focusedModelIdx = Math.min(focusedModelIdx + 1, models.length - 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      focusedModelIdx = Math.max(focusedModelIdx - 1, 0);
    } else if (e.key === "Enter" && focusedModelIdx >= 0 && focusedModelIdx < models.length) {
      e.preventDefault();
      dbg("statusbar", "model selected via keyboard", { model: models[focusedModelIdx].value });
      selectModel(models[focusedModelIdx].value);
    } else if (e.key === "Escape") {
      e.preventDefault();
      dropdownOpen = false;
    }
    // Tab: allow focus to leave dropdown; all other keys: stop propagation to prevent global shortcuts
    if (e.key !== "Tab") {
      e.stopPropagation();
    }
  }

  onMount(() => {
    function onDocClick(e: MouseEvent | PointerEvent) {
      // Close model dropdown on outside click
      if (
        dropdownOpen &&
        modelBtnEl &&
        !modelBtnEl.contains(e.target as Node) &&
        dropdownEl &&
        !dropdownEl.contains(e.target as Node)
      ) {
        dropdownOpen = false;
      }
      // Close more menu on outside click
      if (moreMenuOpen) {
        const target = e.target as Node;
        if (!moreMenuBtnEl?.contains(target) && !moreMenuEl?.contains(target)) {
          moreMenuOpen = false;
        }
      }
    }
    function onDocKeydown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        if (dropdownOpen) {
          dropdownOpen = false;
          e.preventDefault();
          e.stopPropagation();
        } else if (moreMenuOpen) {
          moreMenuOpen = false;
          e.preventDefault();
          e.stopPropagation();
        }
      }
    }
    document.addEventListener("pointerdown", onDocClick, true);
    document.addEventListener("keydown", onDocKeydown);
    return () => {
      document.removeEventListener("pointerdown", onDocClick, true);
      document.removeEventListener("keydown", onDocKeydown);
    };
  });

  let mcpAggregateStatus = $derived.by(() => {
    if (!mcpServers || mcpServers.length === 0) return "none";
    const hasFailure = mcpServers.some((s) => s.status === "failed" || s.status === "needs-auth");
    const hasPending = mcpServers.some((s) => s.status === "pending");
    const allDisabled = mcpServers.every((s) => s.status === "disabled");
    if (hasFailure) return "error";
    if (hasPending) return "pending";
    if (allDisabled) return "disabled";
    return "ok";
  });

  let mcpDotClass = $derived(
    mcpAggregateStatus === "error"
      ? "bg-destructive"
      : mcpAggregateStatus === "pending"
        ? "bg-amber-500"
        : mcpAggregateStatus === "disabled"
          ? "bg-muted-foreground/30"
          : "bg-emerald-500",
  );

  // Find model info: exact match first, then fuzzy (model ID contains alias)
  let currentModelInfo = $derived.by(() => {
    const exact = models.find((m) => m.value === model);
    if (exact) return exact;
    return models.find((m) => model.includes(m.value) && m.value !== "default");
  });
  // Effort: always collect levels from any model that supports them (for always-visible UI)
  let anyModelEffortLevels = $derived.by(() => {
    const supporting = models.find(
      (m) => m.supportsEffort === true && m.supportedEffortLevels?.length,
    );
    return supporting?.supportedEffortLevels ?? [];
  });
  let effortLevels = $derived.by(() => {
    const levels = currentModelInfo?.supportedEffortLevels ?? anyModelEffortLevels;
    return agent === "pi" ? filterPiThinkingLevelsForUi(levels) : levels;
  });
  let effortDisabled = $derived(currentModelInfo?.supportsEffort !== true);

  const EFFORT_LABEL_KEYS = {
    none: "settings_codexConfig_optNone",
    off: "settings_codexConfig_optNone",
    minimal: "settings_codexConfig_optMinimal",
    low: "settings_codexConfig_optLow",
    medium: "settings_codexConfig_optMedium",
    high: "settings_codexConfig_optHigh",
    xhigh: "settings_codexConfig_optXHigh",
    max: "settings_cliConfig_effortLevelMax",
  } as const;
  const CODEX_EFFORT_LABEL_KEYS = {
    none: "settings_codexConfig_optNone",
    minimal: "settings_codexConfig_optMinimal",
    low: "settings_codexConfig_effortLight",
    medium: "settings_codexConfig_effortMedium",
    high: "settings_codexConfig_optHigh",
    xhigh: "settings_codexConfig_optXHigh",
    max: "settings_codexConfig_effortMax",
  } as const;

  function formatEffortLevel(level: string): string {
    const labels = agent === "codex" ? CODEX_EFFORT_LABEL_KEYS : EFFORT_LABEL_KEYS;
    const key = labels[level as keyof typeof labels];
    return key ? t(key) : level;
  }
</script>

<div
  class="session-status-bar border-b border-border/50 bg-background font-mono text-xs text-foreground/70 select-none"
  data-window-drag-region
  data-tauri-drag-region
>
  <!-- Single-row Modern Header: 44px (h-11) like Codex / ZCode -->
  <div
    class="flex h-11 items-center justify-between px-3"
    style:padding-left={IS_MAC && !sidebarOpen ? "80px" : undefined}
  >
    <!-- Left: Session title & identity -->
    <div class="flex items-center gap-2 min-w-0 flex-1 overflow-hidden pr-2">
      {#if onToggleSidebar}
        <button
          class="rounded p-1 -ml-1 hover:bg-accent text-foreground/70 hover:text-foreground transition-colors shrink-0"
          onclick={onToggleSidebar}
          title={t("statusbar_toggleSidebar")}
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
      {/if}

      <!-- Folder icon + Session title (prominent, editable) -->
      <div class="group flex items-center gap-1.5 min-w-0 flex-shrink">
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
            d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
          />
        </svg>

        {#if titleEditing}
          <input
            bind:this={titleInputEl}
            bind:value={titleEditValue}
            class="w-48 sm:w-64 bg-transparent border-b border-primary outline-none text-foreground font-semibold text-xs px-0.5"
            onkeydown={(e) => {
              if (e.key === "Enter") commitTitleEdit();
              else if (e.key === "Escape") cancelTitleEdit();
            }}
            onblur={commitTitleEdit}
          />
        {:else}
          <span
            class="truncate font-semibold text-xs text-foreground/90"
            title={stripExpertTag(run?.name || run?.prompt) || t("statusbar_sessionTitle")}
          >
            {stripExpertTag(run?.name || run?.prompt) || t("statusbar_sessionTitle")}
          </span>
          {#if onRename && run}
            <button
              type="button"
              class="opacity-0 group-hover:opacity-100 transition-opacity p-0.5 text-muted-foreground hover:text-foreground rounded shrink-0"
              onclick={startTitleEdit}
              title="重命名会话"
              aria-label="重命名会话"
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
                <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
                <path d="m15 5 4 4" />
              </svg>
            </button>
          {/if}
        {/if}
      </div>

      <!-- Subtle separator -->
      <span class="text-foreground/20 shrink-0">·</span>

      <!-- Status dot + Runtime Provider & Mode (neutral secondary desktop header, no background badge) -->
      <div
        class="inline-flex items-center gap-1.5 shrink-0 text-xs text-muted-foreground/70 select-none"
      >
        {#if running}
          <span class="inline-block h-1.5 w-1.5 rounded-full {agentDotClass} animate-pulse"></span>
        {/if}
        {#if onStatusClick}
          <button
            type="button"
            class="hover:text-foreground transition-colors font-normal"
            onclick={onStatusClick}
            title={t("toolActivity_tabInfo")}
          >
            <span>{visibleAgentDisplayName}</span>
          </button>
        {:else}
          <span class="font-normal">{visibleAgentDisplayName}</span>
        {/if}

        {#if mode}
          <span class="text-muted-foreground/30">·</span>
          <span>{mode}</span>
        {/if}
      </div>

      <!-- Background task count (if any) -->
      {#if activeTaskCount && activeTaskCount > 0}
        <span class="text-foreground/20 shrink-0">·</span>
        <span
          class="flex items-center gap-1 text-blue-400 shrink-0"
          title={t("bgTask_activeTitle", { count: String(activeTaskCount) })}
        >
          <span class="inline-block h-1.5 w-1.5 rounded-full bg-blue-400 animate-pulse"></span>
          <span class="text-[10px]">{t("bgTask_active", { count: String(activeTaskCount) })}</span>
        </span>
      {/if}
    </div>

    <!-- Right: Quick actions + Panel toggles + More Actions (···) -->
    <div class="flex items-center gap-1 shrink-0">
      <!-- Search dialog trigger (⌘F) -->
      {#if onSearch}
        <button
          type="button"
          class="flex items-center justify-center rounded p-1.5 transition-colors {searchOpen
            ? 'bg-accent text-foreground'
            : 'text-foreground/70 hover:text-foreground hover:bg-accent'}"
          onclick={onSearch}
          title="查找对话 ⌘F"
          aria-label="查找对话 ⌘F"
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
            <circle cx="11" cy="11" r="8" />
            <path d="m21 21-4.3-4.3" />
          </svg>
        </button>
      {/if}

      <!-- ··· More actions Dropdown (first on the left of right actions, ZCode style) -->
      <div class="relative" data-export-exclude>
        <button
          bind:this={moreMenuBtnEl}
          type="button"
          class="flex items-center justify-center rounded p-1.5 text-foreground/70 hover:text-foreground hover:bg-accent transition-colors {moreMenuOpen
            ? 'bg-accent text-foreground'
            : ''}"
          onclick={() => (moreMenuOpen = !moreMenuOpen)}
          title="更多操作"
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
            <circle cx="12" cy="12" r="1" />
            <circle cx="19" cy="12" r="1" />
            <circle cx="5" cy="12" r="1" />
          </svg>
        </button>

        {#if moreMenuOpen}
          <div
            bind:this={moreMenuEl}
            class="absolute right-0 top-full z-50 mt-1 w-56 rounded-xl border border-border bg-popover p-1 text-xs text-popover-foreground shadow-xl animate-in fade-in zoom-in-95 duration-100 divide-y divide-border/40"
          >
            <!-- Group 1: Session Actions -->
            <div class="space-y-0.5 pb-1">
              {#if onRename}
                <button
                  class="w-full flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-foreground/80 hover:bg-accent hover:text-foreground transition-colors"
                  onclick={() => {
                    moreMenuOpen = false;
                    startTitleEdit();
                  }}
                >
                  <svg
                    class="h-3.5 w-3.5 text-muted-foreground"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    ><path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" /><path
                      d="m15 5 4 4"
                    /></svg
                  >
                  <span>重命名会话</span>
                </button>
              {/if}

              {#if parentRunId && onNavigateParent}
                <button
                  class="w-full flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-blue-400 hover:bg-accent transition-colors"
                  onclick={() => {
                    moreMenuOpen = false;
                    onNavigateParent();
                  }}
                >
                  <svg
                    class="h-3.5 w-3.5"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    ><circle cx="12" cy="18" r="3" /><circle cx="6" cy="6" r="3" /><circle
                      cx="18"
                      cy="6"
                      r="3"
                    /><path d="M18 9v2c0 .6-.4 1-1 1H7c-.6 0-1-.4-1-1V9" /><path
                      d="M12 12v3"
                    /></svg
                  >
                  <span>{t("statusbar_viewParent")}</span>
                </button>
              {/if}
            </div>

            <!-- Group 2: Files & Workspace -->
            <div class="space-y-0.5 py-1">
              {#if cwd || run?.cwd}
                <button
                  class="w-full flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-foreground/80 hover:bg-accent hover:text-foreground transition-colors"
                  onclick={handleReveal}
                >
                  <svg
                    class="h-3.5 w-3.5 text-muted-foreground"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    ><path
                      d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
                    /></svg
                  >
                  <span>在 Finder 中打开</span>
                </button>

                <button
                  class="w-full flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-foreground/80 hover:bg-accent hover:text-foreground transition-colors"
                  onclick={copyPath}
                >
                  <svg
                    class="h-3.5 w-3.5 text-muted-foreground"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    ><rect width="14" height="14" x="8" y="8" rx="2" ry="2" /><path
                      d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"
                    /></svg
                  >
                  <span>{pathCopied ? "已复制工作区路径" : "复制工作区路径"}</span>
                </button>
              {/if}

              {#if onOpenVscode}
                <button
                  class="w-full flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-foreground/80 hover:bg-accent hover:text-foreground transition-colors {vscodeAvailable ===
                  false
                    ? 'opacity-40 cursor-not-allowed'
                    : ''}"
                  disabled={vscodeAvailable === false}
                  onclick={() => {
                    moreMenuOpen = false;
                    onOpenVscode();
                  }}
                >
                  <VscodeIcon class="h-3.5 w-3.5" />
                  <span>在 VS Code 中打开</span>
                </button>
              {/if}

              {#if onPreviewToggle}
                <button
                  class="w-full flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-foreground/80 hover:bg-accent hover:text-foreground transition-colors"
                  onclick={() => {
                    moreMenuOpen = false;
                    onPreviewToggle();
                  }}
                >
                  <svg
                    class="h-3.5 w-3.5 text-purple-400"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    ><rect x="2" y="3" width="20" height="14" rx="2" /><path
                      d="M8 21h8M12 17v4"
                    /></svg
                  >
                  <span>{t("preview_buttonLabel")}</span>
                </button>
              {/if}

              {#if onOpenWorktrees}
                <button
                  class="w-full flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-foreground/80 hover:bg-accent hover:text-foreground transition-colors"
                  onclick={() => {
                    moreMenuOpen = false;
                    onOpenWorktrees();
                  }}
                >
                  <svg
                    class="h-3.5 w-3.5 text-amber-500"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"><path d="M8 3v18M16 3v18M3 8h18M3 16h18" /></svg
                  >
                  <span>{t("chat_worktree_label")}</span>
                </button>
              {/if}

              {#if run?.session_id}
                <button
                  class="w-full flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-foreground/80 hover:bg-accent hover:text-foreground transition-colors"
                  onclick={copySessionId}
                >
                  <svg
                    class="h-3.5 w-3.5 text-muted-foreground"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    ><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" /><path
                      d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"
                    /></svg
                  >
                  <span>{sidCopied ? "已复制会话 ID" : "复制会话 ID"}</span>
                </button>
              {/if}
            </div>

            <!-- Group 3: Capabilities & Tools -->
            <div class="space-y-0.5 py-1">
              {#if toolsCount && toolsCount > 0 && onToolsClick}
                <button
                  class="w-full flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-foreground/80 hover:bg-accent hover:text-foreground transition-colors"
                  onclick={() => {
                    moreMenuOpen = false;
                    onToolsClick();
                  }}
                >
                  <span class="inline-block h-2 w-2 rounded-full bg-emerald-500"></span>
                  <span>运行有效能力 ({toolsCount})</span>
                </button>
              {/if}

              {#if mcpServers && mcpServers.length > 0 && onMcpToggle}
                <button
                  class="w-full flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-foreground/80 hover:bg-accent hover:text-foreground transition-colors"
                  onclick={() => {
                    moreMenuOpen = false;
                    onMcpToggle();
                  }}
                >
                  <span class="inline-block h-2 w-2 rounded-full {mcpDotClass}"></span>
                  <span>{t("statusbar_mcpLabel", { count: String(mcpServers.length) })}</span>
                </button>
              {/if}

              <a
                href="/settings?tab=capability-center"
                class="w-full flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-foreground/80 hover:bg-accent hover:text-foreground transition-colors no-underline"
                onclick={() => (moreMenuOpen = false)}
              >
                <span class="inline-block h-2 w-2 rounded-full bg-fuchsia-500"></span>
                <span>统一能力中心</span>
              </a>
            </div>

            <!-- Group 4: Details & Settings -->
            {#if onStatusClick}
              <div class="space-y-0.5 pt-1">
                <button
                  class="w-full flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-foreground/80 hover:bg-accent hover:text-foreground transition-colors"
                  onclick={() => {
                    moreMenuOpen = false;
                    onStatusClick();
                  }}
                >
                  <svg
                    class="h-3.5 w-3.5 text-muted-foreground"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    ><circle cx="12" cy="12" r="10" /><path d="M12 16v-4" /><path
                      d="M12 8h.01"
                    /></svg
                  >
                  <span>会话运行详情与配置</span>
                </button>
              </div>
            {/if}
          </div>
        {/if}
      </div>

      <!-- VS Code quick open button (ZCode style) -->
      {#if onOpenVscode}
        <button
          type="button"
          class="flex items-center gap-1 rounded p-1.5 text-foreground/60 hover:text-foreground hover:bg-accent transition-colors {vscodeAvailable ===
          false
            ? 'opacity-40 cursor-not-allowed'
            : ''}"
          onclick={() => onOpenVscode?.()}
          title={vscodeAvailable === false ? t("editor_vscodeNotInstalled") : "在 VS Code 中打开"}
        >
          <VscodeIcon class="h-3.5 w-3.5" />
        </button>
      {/if}

      <!-- Rewind button (if available) -->
      {#if !running && onRewind && persistedFiles && persistedFiles.length > 0}
        <button
          class="flex items-center gap-1 rounded px-2 py-1 text-xs text-foreground/60 hover:text-foreground hover:bg-accent transition-colors"
          onclick={onRewind}
          title={t("statusbar_rewindTitle")}
        >
          <svg
            class="h-3 w-3"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            ><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path
              d="M3 3v5h5"
            /></svg
          >
          <span class="hidden sm:inline">{t("statusbar_rewind")}</span>
        </button>
      {/if}

      <div class="h-3.5 w-px bg-border/40 mx-0.5"></div>

      <!-- Panel Toggle Buttons (Bottom Terminal & Right Sidebar) -->
      {#if onToggleBottomPanel}
        <button
          class="flex items-center gap-1 rounded p-1.5 transition-colors
            {bottomPanelOpen
            ? 'text-primary bg-primary/10'
            : 'text-foreground/50 hover:text-foreground hover:bg-accent'}"
          onclick={onToggleBottomPanel}
          title="切换底部面板显示 ⌘J"
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
            <rect width="18" height="18" x="3" y="3" rx="2" />
            <path d="M3 15h18" />
          </svg>
        </button>
      {/if}

      {#if onToggleRightSidebar}
        <button
          class="flex items-center gap-1 rounded p-1.5 transition-colors
            {rightSidebarOpen
            ? 'text-primary bg-primary/10'
            : 'text-foreground/50 hover:text-foreground hover:bg-accent'}"
          onclick={onToggleRightSidebar}
          title="切换右侧扩展栏显示 ⌘B"
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
            <rect width="18" height="18" x="3" y="3" rx="2" />
            <path d="M15 3v18" />
          </svg>
        </button>
      {/if}
    </div>
  </div>
</div>

{#if dropdownOpen}
  <div
    bind:this={dropdownEl}
    tabindex="-1"
    role="listbox"
    class="min-w-[560px] w-max rounded-md border bg-background shadow-lg animate-fade-in outline-none"
    style={dropdownStyle}
    onkeydown={handleDropdownKeydown}
  >
    <div class="p-1">
      {#each models as m, i}
        <button
          class="flex w-full items-center gap-2 rounded-sm px-3 py-1 text-xs hover:bg-accent transition-colors {model ===
          m.value
            ? 'bg-accent font-medium'
            : ''} {i === focusedModelIdx ? 'ring-1 ring-primary/50' : ''}"
          onclick={() => selectModel(m.value)}
        >
          {#if model === m.value}
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
          {#if m.providerName}
            <span
              class="shrink-0 rounded px-1 py-0.5 text-[10px] font-medium bg-primary/10 text-primary"
              >{m.providerName}</span
            >
          {/if}
          <span class="shrink-0 text-foreground">{m.displayName}</span>
          <span class="text-[10px] text-foreground/70 truncate">{m.description}</span>
        </button>
      {/each}
    </div>
    {#if effortLevels.length > 0 && onEffortChange}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        onkeydown={(e) => {
          if (["Enter", " ", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key)) {
            e.stopPropagation();
          }
        }}
      >
        <div class="border-t mx-1 my-1"></div>
        <div class="px-3 py-2">
          <div class="text-[10px] text-muted-foreground mb-1.5">
            {t("effort_label")}{#if effortDisabled}<span class="ml-1 opacity-50"
                >— {currentModelInfo?.displayName ?? model} not supported</span
              >{/if}
          </div>
          <div class="flex gap-1">
            {#each effortLevels as level}
              <button
                class="flex-1 rounded px-2 py-1 text-xs transition-colors
                  {effortDisabled
                  ? 'bg-muted/30 text-muted-foreground/40 cursor-not-allowed'
                  : effort === level
                    ? 'bg-primary text-primary-foreground font-medium'
                    : 'bg-muted/50 text-muted-foreground hover:bg-accent'}"
                disabled={effortDisabled}
                onclick={() => onEffortChange(level)}>{formatEffortLevel(level)}</button
              >
            {/each}
          </div>
        </div>
      </div>
    {/if}
    {#if onSetProjectDefault && model}
      <div class="border-t mx-1 my-1"></div>
      <button
        type="button"
        class="flex w-full items-center gap-2 rounded-sm px-3 py-2 text-xs transition-colors
          {projectDefaultModel === model ? 'text-primary' : 'text-foreground hover:bg-accent'}"
        disabled={projectDefaultModel === model}
        onclick={setProjectDefault}
      >
        <span class="w-3 shrink-0 text-primary">{projectDefaultModel === model ? "✓" : ""}</span>
        <span
          >{projectDefaultModel === model
            ? (projectDefaultLabel ?? t("model_projectDefault"))
            : (setProjectDefaultLabel ?? t("model_setProjectDefault"))}</span
        >
      </button>
    {/if}
  </div>
{/if}
