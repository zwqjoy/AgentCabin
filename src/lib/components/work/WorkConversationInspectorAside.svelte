<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import type { SessionInfoData } from "$lib/types";
  import type {
    InboxItem,
    WorkArtifactStorageMode,
    WorkArtifactSummary,
    WorkProgressSnapshot,
    WorkRunProgressView,
    WorkRunReceipt,
    WorkRunRecovery,
  } from "$lib/types/work";
  import WorkConversationInspector from "./WorkConversationInspector.svelte";
  import BrowserInspector from "$lib/components/browser/BrowserInspector.svelte";
  import { isBrowserToolName } from "$lib/utils/work-browser";

  type WorkAsideTab = "tasks" | "browser";

  interface Props {
    open: boolean;
    onClose: () => void;
    onRequestOpenBrowser?: () => void;
    activeTab: WorkAsideTab;
    browserActivitySeen?: boolean;
    sessionInfo: SessionInfoData | null;
    progress: WorkProgressSnapshot | null;
    progressView?: WorkRunProgressView | null;
    recovery?: WorkRunRecovery | null;
    readOnly?: boolean;
    artifacts: WorkArtifactSummary[];
    pendingInteractions?: InboxItem[];
    workspaceRoot?: string;
    primaryWorkRoot?: string;
    artifactStorageMode?: WorkArtifactStorageMode;
    onExportArtifact: (id: string) => Promise<void>;
    onOpenArtifact: (id: string) => Promise<void>;
    onDeleteArtifact: (id: string) => Promise<void>;
    onCopyArtifactToPrimary?: (id: string) => Promise<string>;
    onValidateArtifact?: (id: string) => Promise<void>;
    onDeliverArtifact?: (id: string) => Promise<void>;
    onOpenArtifactDirectory?: () => Promise<void>;
    onSaveOfficeArtifact?: (id: string, contentBase64: string) => Promise<void>;
    getReceipt?: () => Promise<WorkRunReceipt | null>;
    onArtifactsChanged?: () => void;
  }

  let {
    open,
    onClose,
    onRequestOpenBrowser,
    activeTab = $bindable<WorkAsideTab>("tasks"),
    browserActivitySeen = false,
    sessionInfo,
    progress,
    progressView = null,
    recovery = null,
    readOnly = false,
    artifacts,
    pendingInteractions = [],
    workspaceRoot = "",
    primaryWorkRoot = "",
    artifactStorageMode = "managed",
    onExportArtifact,
    onOpenArtifact,
    onDeleteArtifact,
    onCopyArtifactToPrimary,
    onValidateArtifact,
    onDeliverArtifact,
    onOpenArtifactDirectory,
    onSaveOfficeArtifact,
    getReceipt,
    onArtifactsChanged,
  }: Props = $props();

  const browserRunId = $derived(sessionInfo?.runId ?? progressView?.workRunId ?? "");
  let autoOpenedBrowserRunId = $state("");

  let hasBrowserActivity = $derived.by(() => {
    const hint = [
      progressView?.currentActivity?.kind,
      progressView?.currentActivity?.toolName,
      progress?.activeToolName,
    ]
      .filter(Boolean)
      .join(" ");
    return browserActivitySeen || isBrowserToolName(hint);
  });

  // Open the outer inspector on the first browser action for each run. This
  // component stays mounted while the aside is closed, so it can request the
  // parent to reveal the Browser surface before its contents mount.
  $effect(() => {
    const runId = browserRunId;
    if (!hasBrowserActivity || !runId || autoOpenedBrowserRunId === runId) return;
    autoOpenedBrowserRunId = runId;
    activeTab = "browser";
    if (!open) onRequestOpenBrowser?.();
  });

  const isTaskCompleted = $derived.by(() => {
    if (readOnly) return true;
    const phase = progress?.phase ?? progressView?.phase;
    if (
      phase === "completed" ||
      phase === "failed" ||
      phase === "cancelled" ||
      phase === "stopped"
    ) {
      return true;
    }
    const runStatus = progress?.runStatus ?? progressView?.runStatus;
    if (runStatus === "completed" || runStatus === "failed" || runStatus === "cancelled") {
      return true;
    }
    return false;
  });

  // ── Width & Resize handling ──
  const WIDTH_MIN = 340;
  const WIDTH_MAX = 1200;
  const WIDTH_DEFAULT = 440;
  const WIDTH_STORAGE_KEY = "agentcabin:work-inspector-width";

  function clampWidth(v: number): number {
    const max =
      typeof window !== "undefined"
        ? Math.min(WIDTH_MAX, Math.max(WIDTH_MIN, window.innerWidth - 360))
        : WIDTH_MAX;
    return Math.max(WIDTH_MIN, Math.min(max, v));
  }

  let savedWidth = $state(WIDTH_DEFAULT);
  let isMaximized = $state(false);

  onMount(() => {
    if (typeof window === "undefined") return;
    const stored = window.localStorage.getItem(WIDTH_STORAGE_KEY);
    if (stored) {
      const n = parseInt(stored, 10);
      if (Number.isFinite(n)) savedWidth = clampWidth(n);
    }
  });

  let effectiveWidth = $derived(clampWidth(savedWidth));

  let asideEl: HTMLElement | null = $state(null);
  let ghostEl: HTMLElement | null = $state(null);
  let resizing = $state(false);
  let ghostX = $state(0);
  let resizeCleanup: (() => void) | null = null;

  function startResize(e: PointerEvent) {
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    const startX = e.clientX;
    const startWidth = effectiveWidth;
    let pendingWidth = startWidth;
    const asideRight =
      asideEl?.getBoundingClientRect().right ??
      (typeof window !== "undefined" ? window.innerWidth : startX + startWidth);

    resizing = true;
    ghostX = asideRight - startWidth;
    handle.setPointerCapture?.(e.pointerId);

    const prevUserSelect = document.body.style.userSelect;
    const prevCursor = document.body.style.cursor;
    document.body.style.userSelect = "none";
    document.body.style.cursor = "col-resize";

    function onMove(ev: PointerEvent) {
      const delta = startX - ev.clientX; // Dragging left increases width
      pendingWidth = clampWidth(startWidth + delta);
      const x = asideRight - pendingWidth;
      if (ghostEl) {
        ghostEl.style.left = `${x - 1}px`;
      }
    }

    function cleanup() {
      handle.removeEventListener("pointermove", onMove);
      handle.removeEventListener("pointerup", onUp);
      handle.removeEventListener("pointercancel", onUp);
      try {
        handle.releasePointerCapture?.(e.pointerId);
      } catch {
        // ignore
      }
      document.body.style.userSelect = prevUserSelect;
      document.body.style.cursor = prevCursor;

      savedWidth = pendingWidth;
      if (typeof window !== "undefined") {
        window.localStorage.setItem(WIDTH_STORAGE_KEY, String(savedWidth));
      }
      resizing = false;
      ghostEl = null;
      resizeCleanup = null;
    }

    function onUp() {
      cleanup();
    }

    handle.addEventListener("pointermove", onMove);
    handle.addEventListener("pointerup", onUp);
    handle.addEventListener("pointercancel", onUp);
    resizeCleanup = cleanup;
  }

  function resetWidth() {
    savedWidth = WIDTH_DEFAULT;
    if (typeof window !== "undefined") {
      window.localStorage.setItem(WIDTH_STORAGE_KEY, String(WIDTH_DEFAULT));
    }
  }

  onDestroy(() => {
    resizeCleanup?.();
  });
</script>

{#if resizing && open && !isMaximized}
  <!-- Ghost line during drag: zero-cost preview, no layout reflow elsewhere -->
  <div
    bind:this={ghostEl}
    class="fixed top-0 bottom-0 z-[9999] pointer-events-none bg-primary"
    style="left: {ghostX - 1}px; width: 3px; box-shadow: 0 0 8px hsl(var(--primary) / 0.6);"
  ></div>
{/if}

<aside
  bind:this={asideEl}
  class="pointer-events-auto flex min-h-0 shrink-0 flex-col overflow-hidden bg-background {!open
    ? 'invisible pointer-events-none'
    : isMaximized
      ? 'absolute inset-0 z-30 w-full'
      : 'relative border-l border-border/60'}"
  style={isMaximized && open ? "width: 100%;" : `width: ${open ? effectiveWidth : 0}px;`}
>
  {#if open && !isMaximized}
    <!-- Resize handle on the left edge -->
    <div
      role="separator"
      aria-orientation="vertical"
      tabindex="-1"
      class="group absolute left-0 top-0 bottom-0 w-2 -translate-x-1/2 cursor-col-resize z-20 hover:bg-primary/20 active:bg-primary/30 transition-colors {resizing
        ? 'bg-primary/30'
        : ''}"
      onpointerdown={startResize}
      ondblclick={resetWidth}
      title="拖动调整左右宽度，双击恢复默认"
    >
      <div
        class="h-full w-0.5 mx-auto transition-colors group-hover:bg-primary/50 group-active:bg-primary {resizing
          ? 'bg-primary'
          : 'bg-transparent'}"
      ></div>
    </div>
  {/if}

  <div
    class="shrink-0 border-b border-border/60 px-3 py-2 flex items-center justify-between bg-muted/20"
  >
    <!-- Tabs matching Code mode / modern IDE style -->
    <div class="flex items-center gap-1 bg-muted/40 p-0.5 rounded-lg border border-border/60">
      <button
        type="button"
        class="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-medium transition-colors {activeTab ===
        'tasks'
          ? 'bg-background text-foreground shadow-xs'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => (activeTab = "tasks")}
      >
        <span class="text-xs">📋</span>
        <span>任务与成果</span>
      </button>
      <button
        type="button"
        class="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-medium transition-colors relative {activeTab ===
        'browser'
          ? 'bg-background text-foreground shadow-xs'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => (activeTab = "browser")}
        title="受控浏览器与交互画布"
      >
        <span class="text-xs">🌐</span>
        <span>浏览器</span>
        {#if hasBrowserActivity}
          <span class="h-1.5 w-1.5 rounded-full bg-blue-500 animate-pulse"></span>
        {/if}
      </button>
    </div>

    <div class="flex items-center gap-1">
      <button
        type="button"
        class="rounded p-1 text-muted-foreground hover:bg-card hover:text-foreground transition-colors"
        onclick={() => (isMaximized = !isMaximized)}
        title={isMaximized ? "还原面板" : "最大化面板"}
        aria-label={isMaximized ? "还原面板" : "最大化面板"}
      >
        {#if isMaximized}
          <svg
            viewBox="0 0 24 24"
            class="h-3.5 w-3.5"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <rect x="4" y="8" width="12" height="12" rx="1.5" />
            <path d="M8 4h10a2 2 0 0 1 2 2v10" />
          </svg>
        {:else}
          <svg
            viewBox="0 0 24 24"
            class="h-3.5 w-3.5"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <rect x="3" y="3" width="18" height="18" rx="2" />
          </svg>
        {/if}
      </button>
      <button
        type="button"
        class="rounded p-1 text-muted-foreground hover:bg-card hover:text-foreground transition-colors"
        onclick={onClose}
        aria-label="关闭侧边栏"
        title="关闭"
      >
        <svg
          viewBox="0 0 24 24"
          class="h-3.5 w-3.5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="M18 6L6 18M6 6l12 12" />
        </svg>
      </button>
    </div>
  </div>

  <div class="relative flex-1 min-h-0 overflow-hidden">
    {#if activeTab === "tasks"}
      <div class="h-full w-full overflow-hidden">
        <WorkConversationInspector
          {sessionInfo}
          {progress}
          {progressView}
          {recovery}
          {readOnly}
          {artifacts}
          {pendingInteractions}
          {workspaceRoot}
          {primaryWorkRoot}
          {artifactStorageMode}
          {onExportArtifact}
          {onOpenArtifact}
          {onDeleteArtifact}
          {onCopyArtifactToPrimary}
          {onValidateArtifact}
          {onDeliverArtifact}
          {onOpenArtifactDirectory}
          {onSaveOfficeArtifact}
          {getReceipt}
        />
      </div>
    {/if}
    <div
      class="absolute inset-0 overflow-hidden {open && activeTab === 'browser'
        ? 'visible pointer-events-auto'
        : 'invisible pointer-events-none'}"
    >
      {#if browserRunId}
        <BrowserInspector
          runId={browserRunId}
          mode="work"
          {readOnly}
          {isTaskCompleted}
          surfaceVisible={open && activeTab === "browser"}
        />
      {/if}
    </div>
  </div>
</aside>
