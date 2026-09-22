<script lang="ts">
  import { onMount, onDestroy, untrack } from "svelte";
  import type { CodeAsideTab, CodeAsideTabType } from "$lib/types/code-aside";
  import { CODE_ASIDE_TAB_TITLES } from "$lib/types/code-aside";
  import CodeAsideTabBar from "./CodeAsideTabBar.svelte";
  import CodeAsideReviewView from "./views/CodeAsideReviewView.svelte";
  import CodeAsideTerminalView from "./views/CodeAsideTerminalView.svelte";
  import CodeAsideBrowserView from "./views/CodeAsideBrowserView.svelte";
  import CodeAsideFileView from "./views/CodeAsideFileView.svelte";
  import CodeAsideLauncher from "./CodeAsideLauncher.svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
    cwd?: string;
    runId?: string;
    turnDiff?: string;
    requestedTab?: CodeAsideTabType | null;
    requestedFilePath?: string | null;
  }

  let {
    open,
    onClose,
    cwd = "",
    runId = "",
    turnDiff = "",
    requestedTab = null,
    requestedFilePath = null,
  }: Props = $props();

  // ── Tab Management ──
  let tabs = $state<CodeAsideTab[]>([]);
  let activeTabId = $state<string>("");
  let showFileTree = $state(true);
  let isMaximized = $state(false);

  let activeTab = $derived(tabs.find((t) => t.id === activeTabId) ?? null);

  // Handle external tab switch requests (e.g. clicking review button in chat)
  let lastHandledRequest = "";
  $effect(() => {
    const tab = requestedTab;
    const path = requestedFilePath;
    if (!tab) return;
    const reqKey = `${tab}:${path || ""}`;
    if (reqKey !== lastHandledRequest) {
      lastHandledRequest = reqKey;
      untrack(() => {
        ensureTab(tab, path);
      });
    }
  });

  // If turnDiff becomes non-empty and user has review tab, update it
  $effect(() => {
    if (turnDiff.trim()) {
      untrack(() => {
        const reviewTab = tabs.find((t) => t.type === "review");
        if (reviewTab) {
          reviewTab.turnDiff = turnDiff;
        }
      });
    }
  });

  export function ensureTab(type: CodeAsideTabType, filePath?: string | null) {
    let existing = tabs.find((t) => {
      if (type === "file" && filePath) {
        return t.type === "file" && t.filePath === filePath;
      }
      return t.type === type;
    });

    if (existing) {
      activeTabId = existing.id;
    } else {
      const newId = `tab-${type}-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
      const title = filePath ? filePath.split("/").pop() || "文件" : CODE_ASIDE_TAB_TITLES[type];
      const newTab: CodeAsideTab = {
        id: newId,
        type,
        title,
        filePath: filePath || undefined,
        turnDiff: type === "review" ? turnDiff : undefined,
        closable: true,
      };
      tabs = [...tabs, newTab];
      activeTabId = newId;
    }
  }

  function handleSelectTab(id: string) {
    activeTabId = id;
  }

  function handleCloseTab(id: string) {
    const index = tabs.findIndex((t) => t.id === id);
    if (index < 0) return;
    const remaining = tabs.filter((t) => t.id !== id);
    tabs = remaining;
    if (remaining.length === 0) {
      activeTabId = "";
      return;
    }
    if (activeTabId === id) {
      const nextIndex = Math.min(remaining.length - 1, Math.max(0, index - 1));
      activeTabId = remaining[nextIndex]?.id ?? remaining[0].id;
    }
  }

  function handleAddTab(type: CodeAsideTabType) {
    const newId = `tab-${type}-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    const newTab: CodeAsideTab = {
      id: newId,
      type,
      title: CODE_ASIDE_TAB_TITLES[type],
      turnDiff: type === "review" ? turnDiff : undefined,
      closable: true,
    };
    tabs = [...tabs, newTab];
    activeTabId = newId;
  }

  // ── Width & Resize handling ──
  const WIDTH_MIN = 400;
  const WIDTH_MAX = 1300;
  const WIDTH_DEFAULT = 600;
  const WIDTH_STORAGE_KEY = "agentcabin:code-aside-width";

  function clampWidth(v: number): number {
    const max =
      typeof window !== "undefined"
        ? Math.min(WIDTH_MAX, Math.max(WIDTH_MIN, window.innerWidth - 300))
        : WIDTH_MAX;
    return Math.max(WIDTH_MIN, Math.min(max, v));
  }

  let savedWidth = $state(WIDTH_DEFAULT);
  let effectiveWidth = $derived(clampWidth(savedWidth));

  let asideEl = $state<HTMLElement | null>(null);
  let ghostEl = $state<HTMLElement | null>(null);
  let resizing = $state(false);
  let ghostX = $state(0);
  let resizeCleanup: (() => void) | null = null;

  onMount(() => {
    if (typeof window === "undefined") return;
    const stored = window.localStorage.getItem(WIDTH_STORAGE_KEY);
    if (stored) {
      const n = parseInt(stored, 10);
      if (Number.isFinite(n)) savedWidth = clampWidth(n);
    }

    // Global keyboard shortcuts
    function handleKeyDown(e: KeyboardEvent) {
      if (!open) return;
      // Ctrl+Shift+G -> Review
      if (e.ctrlKey && e.shiftKey && (e.key === "G" || e.key === "g")) {
        e.preventDefault();
        ensureTab("review");
      }
      // Ctrl+` -> Terminal
      if (e.ctrlKey && e.key === "`") {
        e.preventDefault();
        ensureTab("terminal");
      }
      // Cmd+T -> Browser
      if ((e.metaKey || e.ctrlKey) && (e.key === "T" || e.key === "t") && !e.shiftKey) {
        e.preventDefault();
        ensureTab("browser");
      }
      // Cmd+P -> Files
      if ((e.metaKey || e.ctrlKey) && (e.key === "P" || e.key === "p") && !e.shiftKey) {
        e.preventDefault();
        ensureTab("file");
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  });

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
      const delta = startX - ev.clientX;
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

  onDestroy(() => {
    resizeCleanup?.();
  });
</script>

{#if open}
  {#if resizing && !isMaximized}
    <div
      bind:this={ghostEl}
      class="fixed top-0 bottom-0 z-[9999] pointer-events-none bg-primary"
      style="left: {ghostX - 1}px; width: 3px; box-shadow: 0 0 8px hsl(var(--primary) / 0.6);"
    ></div>
  {/if}

  <aside
    bind:this={asideEl}
    class="pointer-events-auto flex min-h-0 shrink-0 flex-col overflow-hidden bg-background {isMaximized
      ? 'absolute inset-0 z-30 w-full'
      : 'relative border-l border-border/70'}"
    style={isMaximized ? "width: 100%;" : `width: ${effectiveWidth}px;`}
    aria-label="Code 工作区侧边栏"
  >
    {#if !isMaximized}
      <div
        role="separator"
        aria-orientation="vertical"
        tabindex="-1"
        class="group absolute left-0 top-0 bottom-0 w-2 -translate-x-1/2 cursor-col-resize z-20 hover:bg-primary/20 active:bg-primary/30 transition-colors {resizing
          ? 'bg-primary/30'
          : ''}"
        onpointerdown={startResize}
        ondblclick={() => (savedWidth = WIDTH_DEFAULT)}
        title="拖动调整宽度，双击恢复默认"
      >
        <div
          class="h-full w-0.5 mx-auto transition-colors group-hover:bg-primary/50 group-active:bg-primary {resizing
            ? 'bg-primary'
            : 'bg-transparent'}"
        ></div>
      </div>
    {/if}

    <!-- Tab Bar Header -->
    <CodeAsideTabBar
      {tabs}
      activeTabId={activeTab?.id ?? ""}
      {isMaximized}
      onSelectTab={handleSelectTab}
      onCloseTab={handleCloseTab}
      onAddTab={handleAddTab}
      onToggleMaximize={() => (isMaximized = !isMaximized)}
      onCloseAside={onClose}
    />

    <!-- Tab Views or Launcher when no tabs open -->
    <div class="flex-1 min-h-0 overflow-hidden relative">
      {#if !activeTab || tabs.length === 0}
        <CodeAsideLauncher onSelect={(type) => ensureTab(type)} />
      {:else if activeTab.type === "review"}
        <CodeAsideReviewView
          diffText={activeTab.turnDiff || turnDiff}
          bind:showFileTree
          onToggleFileTree={() => (showFileTree = !showFileTree)}
        />
      {:else if activeTab.type === "terminal"}
        <CodeAsideTerminalView {cwd} sessionId={activeTab.id} />
      {:else if activeTab.type === "browser"}
        <CodeAsideBrowserView {runId} url={activeTab.url} />
      {:else if activeTab.type === "file"}
        <CodeAsideFileView
          {cwd}
          filePath={activeTab.filePath}
          bind:showFileTree
          onToggleFileTree={() => (showFileTree = !showFileTree)}
          onSelectFile={(p) => {
            if (activeTab) {
              activeTab.filePath = p;
              activeTab.title = p.split("/").pop() || "文件";
            }
          }}
        />
      {/if}
    </div>
  </aside>
{/if}
