<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getBrowserSession, getBrowserTraces, browserUserInteract } from "$lib/api/work";
  import { getTransport } from "$lib/transport";
  import { withTimeout } from "$lib/utils/async-utils";
  import EmbeddedBrowserSurface from "$lib/components/browser/EmbeddedBrowserSurface.svelte";
  import { isEmbeddedBrowserAvailable } from "$lib/platform/browser";
  import type { EmbeddedBrowserTab } from "$lib/platform/browser";
  import { t } from "$lib/i18n/index.svelte";
  import {
    formatBrowserAction,
    formatBrowserStatus,
    filterBrowserTraces,
    sanitizeTraceText,
  } from "$lib/utils/work-browser";
  import type { BrowserEvent, BrowserSession, BrowserTraceEntry } from "$lib/types/work";

  interface Props {
    runId: string;
    mode?: "code" | "work";
    readOnly?: boolean;
    isTaskCompleted?: boolean;
    surfaceVisible?: boolean;
    onClose?: () => void;
  }

  let {
    runId,
    mode = "work",
    readOnly = false,
    isTaskCompleted = false,
    surfaceVisible = true,
    onClose,
  }: Props = $props();

  let session = $state<BrowserSession | null>(null);
  let traces = $state<BrowserTraceEntry[]>([]);
  let loading = $state(true);
  let interacting = $state(false);
  let addressInput = $state("");
  let browserTabs = $state<EmbeddedBrowserTab[]>([]);
  let activeTargetId = $state("");
  let searchQuery = $state("");
  let previewImageModal = $state<string | null>(null);
  let embeddedAttachFailed = $state(false);
  let pollInterval: ReturnType<typeof setInterval> | null = null;
  let unlistenBrowserEvent: (() => void) | null = null;
  let stateRequestInFlight = false;
  let pendingStateRefresh = false;
  let stateRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  let lastLoadedUpdatedAt = "";
  const BROWSER_STATE_TIMEOUT_MS = 5_000;

  let interactError = $state("");
  let pageHidden = $state(false);
  let lastPageUrl = "";

  function isTerminalStatus(status: string | undefined): boolean {
    return status === "completed" || status === "failed";
  }

  const isTerminal = $derived(readOnly || isTaskCompleted);

  const effectiveRunId = $derived(runId?.trim() || "default-browser");

  const embedded = isEmbeddedBrowserAvailable();
  const embeddedViewId = $derived(`browser-view-${effectiveRunId}`);
  let overlayOpen = $state(false);

  const statusMeta = $derived(formatBrowserStatus(session?.status ?? "idle"));

  const displayStatusMeta = $derived.by(() => {
    if (isTerminal) {
      return {
        label: "已归档",
        colorClass:
          "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20",
        dotClass: "bg-emerald-500",
      };
    }
    return statusMeta;
  });

  const filteredTraces = $derived(filterBrowserTraces(traces, searchQuery));

  function traceStatusMeta(status: string): { label: string; className: string } {
    switch (status) {
      case "started":
        return {
          label: "进行中",
          className: "text-blue-600 bg-blue-500/10 border-blue-500/20",
        };
      case "success":
        return {
          label: "已执行",
          className: "text-emerald-600 bg-emerald-500/10 border-emerald-500/20",
        };
      case "failed":
        return {
          label: "失败",
          className: "text-destructive bg-destructive/10 border-destructive/20",
        };
      case "waiting_approval":
        return {
          label: "等待审批",
          className: "text-amber-600 bg-amber-500/10 border-amber-500/20",
        };
      default:
        return {
          label: status || "未知",
          className: "text-muted-foreground bg-muted border-border",
        };
    }
  }

  // Keep the native view mounted for archived/read-only runs as well. The
  // inspector can be reopened after a task finishes, and the view is the only
  // source of truth now that the standalone browser fallback is gone.
  const useEmbeddedSurface = $derived(embedded && !embeddedAttachFailed);

  function stopPolling() {
    if (pollInterval) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
  }

  function scheduleStateRefresh() {
    if (stateRefreshTimer) clearTimeout(stateRefreshTimer);
    stateRefreshTimer = setTimeout(() => {
      stateRefreshTimer = null;
      void loadState();
    }, 80);
  }

  async function loadState() {
    const id = effectiveRunId;
    if (!id) {
      loading = false;
      return;
    }
    if (stateRequestInFlight) {
      pendingStateRefresh = true;
      return;
    }
    stateRequestInFlight = true;
    try {
      const sess = await withTimeout(
        getBrowserSession(id),
        BROWSER_STATE_TIMEOUT_MS,
        "受控浏览器状态读取超时",
      );
      if (sess) {
        if (sess.updatedAt !== lastLoadedUpdatedAt) {
          lastLoadedUpdatedAt = sess.updatedAt;
          session = sess;
          traces = sess.traces || [];
        }
        if (isTerminalStatus(sess.status)) stopPolling();
      } else {
        const traceList = await withTimeout(
          getBrowserTraces(id),
          BROWSER_STATE_TIMEOUT_MS,
          "浏览器操作记录读取超时",
        );
        traces = traceList;
      }
    } catch {
      // ignore
    } finally {
      stateRequestInFlight = false;
      loading = false;
      if (pendingStateRefresh) {
        pendingStateRefresh = false;
        scheduleStateRefresh();
      }
    }
  }

  onMount(() => {
    const transport = getTransport();
    let destroyed = false;
    const id = effectiveRunId;
    transport.subscribeRun(id);
    void transport
      .listen<BrowserEvent>("browser-event", (event) => {
        if (event.runId === effectiveRunId) scheduleStateRefresh();
      })
      .then((unlisten) => {
        if (destroyed) {
          unlisten();
        } else {
          unlistenBrowserEvent = unlisten;
        }
      });
    pollInterval = setInterval(() => {
      void loadState();
    }, 15000);
    void loadState();

    return () => {
      destroyed = true;
      transport.unsubscribeRun(id);
    };
  });

  onDestroy(() => {
    stopPolling();
    if (stateRefreshTimer) clearTimeout(stateRefreshTimer);
    stateRefreshTimer = null;
    unlistenBrowserEvent?.();
    unlistenBrowserEvent = null;
  });

  $effect(() => {
    if (session?.currentUrl && !interacting) {
      addressInput = session.currentUrl;
    }
  });

  $effect(() => {
    const currentUrl = session?.currentUrl ?? "";
    if (currentUrl && currentUrl !== lastPageUrl) {
      lastPageUrl = currentUrl;
      pageHidden = false;
    }
  });

  async function performInteract(action: string, params: Record<string, unknown> = {}) {
    const id = effectiveRunId;
    const readOnlyTabSwitch = action === "tabs" && params.action === "switch";
    if (!id || interacting || (isTerminal && !readOnlyTabSwitch)) return;
    interacting = true;
    interactError = "";
    try {
      const updated = await browserUserInteract(id, action, params);
      if (updated) {
        lastLoadedUpdatedAt = updated.updatedAt;
        session = updated;
        if (updated.currentUrl) addressInput = updated.currentUrl;
      }
    } catch (e) {
      console.warn("Browser interact error:", e);
      interactError = e instanceof Error ? e.message : String(e);
    } finally {
      interacting = false;
    }
  }

  function handleTabsChange(tabs: EmbeddedBrowserTab[], activeId: string): void {
    browserTabs = tabs;
    activeTargetId = activeId;
  }

  function tabLabel(tab: EmbeddedBrowserTab): string {
    if (tab.title?.trim()) return tab.title.trim();
    try {
      return new URL(tab.url).hostname || "新标签页";
    } catch {
      return "新标签页";
    }
  }

  function handleAddressKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      let target = addressInput.trim();
      if (target) {
        if (!/^https?:\/\//i.test(target) && !target.startsWith("about:")) {
          target = `https://${target}`;
        }
        addressInput = target;
        void performInteract("navigate", { url: target });
      }
    }
  }

  $effect(() => {
    overlayOpen = Boolean(previewImageModal);
  });
</script>

<div class="flex h-full w-full flex-col overflow-hidden bg-background text-foreground">
  {#if embedded}
    <div
      class="flex h-9 shrink-0 items-end gap-1 overflow-hidden border-b border-border/70 bg-muted/25 px-2 pt-1"
      role="tablist"
      aria-label="浏览器标签页"
      data-testid="browser-tab-strip"
    >
      <div class="flex min-w-0 flex-1 items-end gap-1 overflow-x-auto scrollbar-none">
        {#each browserTabs as tab (tab.targetId)}
          <div
            class="group flex h-7 max-w-52 min-w-0 shrink-0 items-center rounded-t-md border border-b-0 px-2 {tab.targetId ===
            activeTargetId
              ? 'border-border/70 bg-background text-foreground'
              : 'border-transparent text-muted-foreground'}"
          >
            <button
              type="button"
              role="tab"
              aria-selected={tab.targetId === activeTargetId}
              title={tab.url ? `${tab.title || tabLabel(tab)}\n${tab.url}` : tabLabel(tab)}
              class="flex min-w-0 flex-1 items-center gap-1.5 text-left text-[11px]"
              disabled={interacting || tab.targetId === activeTargetId}
              onclick={() =>
                void performInteract("tabs", { action: "switch", target_id: tab.targetId })}
              data-testid="browser-tab"
            >
              <span class="shrink-0 text-[10px]">🌐</span>
              <span class="truncate">{tabLabel(tab)}</span>
            </button>
            {#if browserTabs.length > 1}
              <button
                type="button"
                class="ml-1 flex h-4 w-4 shrink-0 items-center justify-center rounded text-muted-foreground opacity-60 hover:bg-muted hover:text-foreground group-hover:opacity-100"
                aria-label={`关闭标签页 ${tabLabel(tab)}`}
                title="关闭标签页"
                disabled={isTerminal || interacting}
                onclick={() =>
                  void performInteract("tabs", { action: "close", target_id: tab.targetId })}
              >
                ×
              </button>
            {/if}
          </div>
        {/each}
      </div>
      <button
        type="button"
        class="mb-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded text-sm text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-40"
        aria-label="新建浏览器标签页"
        title="新建标签页"
        disabled={isTerminal || interacting || !useEmbeddedSurface}
        onclick={() => void performInteract("tabs", { action: "new" })}
        data-testid="browser-new-tab"
      >
        +
      </button>
    </div>
  {/if}

  <!-- Codex-style Browser Navigation Bar -->
  <div
    class="flex min-h-11 shrink-0 items-center gap-1.5 border-b border-border/70 bg-muted/20 px-3 py-1.5 text-xs"
  >
    <div class="flex items-center gap-0.5 shrink-0">
      <button
        type="button"
        class="h-6 w-6 rounded flex items-center justify-center text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-30 transition-colors"
        disabled={isTerminal || interacting || !session?.currentUrl}
        title="后退"
        onclick={() => void performInteract("go_back")}
      >
        ◀
      </button>
      <button
        type="button"
        class="h-6 w-6 rounded flex items-center justify-center text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-30 transition-colors"
        disabled={isTerminal || interacting || !session?.currentUrl}
        title="前进"
        onclick={() => void performInteract("go_forward")}
      >
        ▶
      </button>
      <button
        type="button"
        class="h-6 w-6 rounded flex items-center justify-center text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-30 transition-colors"
        disabled={isTerminal || interacting}
        title="刷新"
        onclick={() => void performInteract("reload")}
      >
        <span class={interacting ? "animate-spin" : ""}>🔄</span>
      </button>
    </div>

    <div
      class="flex items-center gap-1.5 flex-1 min-w-0 bg-muted/40 border border-border/70 rounded-md px-2 py-0.5 focus-within:ring-1 focus-within:ring-primary focus-within:border-primary transition-all"
    >
      <span class="text-muted-foreground text-[11px] shrink-0">🔒</span>
      <input
        type="text"
        class="bg-transparent font-mono text-[11px] text-foreground w-full outline-none select-all"
        placeholder="输入网址并回车跳转…"
        bind:value={addressInput}
        readonly={isTerminal}
        onkeydown={handleAddressKeydown}
      />
    </div>

    {#if session?.currentUrl}
      <button
        type="button"
        class="shrink-0 rounded-md border border-border/70 px-2 py-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        title={pageHidden ? "显示当前网页" : "隐藏当前网页"}
        aria-label={pageHidden ? "显示当前网页" : "隐藏当前网页"}
        onclick={() => (pageHidden = !pageHidden)}
      >
        {pageHidden ? "显示页面" : "隐藏页面"}
      </button>
    {/if}
    <span
      class="hidden shrink-0 items-center gap-1.5 rounded-full border px-2 py-1 text-[10px] text-muted-foreground sm:inline-flex"
      title={`${mode.toUpperCase()} · ${session?.surface === "embedded" ? t("browser_surfaceEmbedded") : t("browser_surfaceManaged")}`}
    >
      <span class="h-1.5 w-1.5 rounded-full {displayStatusMeta.dotClass}"></span>
      {displayStatusMeta.label}
    </span>
    {#if onClose}
      <button
        type="button"
        class="shrink-0 rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={onClose}
        aria-label="关闭浏览器"
        title="关闭浏览器"
      >
        ✕
      </button>
    {/if}
  </div>

  {#if interactError}
    <div
      class="flex shrink-0 items-center justify-between gap-2 border-b border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive"
    >
      <span class="truncate">{interactError}</span>
      <button
        type="button"
        class="shrink-0 rounded px-1 hover:bg-destructive/10"
        onclick={() => (interactError = "")}
        aria-label="关闭错误提示">✕</button
      >
    </div>
  {/if}

  {#if session?.currentAction && session.status === "running"}
    <div
      class="flex shrink-0 items-center gap-2 border-b border-blue-500/20 bg-blue-500/5 px-3 py-2 text-xs"
    >
      <span class="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-blue-500"></span>
      <span class="font-medium text-blue-700 dark:text-blue-300">Agent 正在操作</span>
      <span class="min-w-0 truncate text-foreground"
        >{sanitizeTraceText(session.currentAction)}</span
      >
    </div>
  {/if}

  <div class="relative min-h-0 flex-1 overflow-hidden bg-white dark:bg-zinc-900">
    {#if useEmbeddedSurface}
      <EmbeddedBrowserSurface
        viewId={embeddedViewId}
        runId={effectiveRunId}
        initialUrl={session?.currentUrl ?? ""}
        visible={surfaceVisible && !overlayOpen && !pageHidden}
        onTabsChange={handleTabsChange}
        onError={() => (embeddedAttachFailed = true)}
      />
      {#if pageHidden}
        <div
          class="absolute inset-0 z-10 flex flex-col items-center justify-center gap-3 bg-background px-6 text-center"
        >
          <span class="text-sm font-medium">网页已隐藏</span>
          <span class="max-w-xs text-xs text-muted-foreground"
            >浏览器会话仍保留；需要时可以重新显示当前页面。</span
          >
          <button
            type="button"
            class="rounded-md border border-border px-3 py-1.5 text-xs hover:bg-accent"
            onclick={() => (pageHidden = false)}>重新显示</button
          >
        </div>
      {/if}
    {:else if loading && !session}
      <div
        class="flex h-full min-h-64 flex-col items-center justify-center gap-2 px-6 text-center text-sm text-muted-foreground"
      >
        <span class="text-xl">🌐</span>
        <span>正在连接浏览器…</span>
      </div>
    {:else if session?.lastScreenshot}
      <div class="flex h-full items-center justify-center overflow-auto bg-muted/10 p-4">
        <img
          src={session.lastScreenshot}
          alt="受控浏览器页面"
          class="max-h-full max-w-full object-contain"
        />
      </div>
    {:else}
      <div class="flex h-full min-h-64 flex-col items-center justify-center gap-2 px-6 text-center">
        <span class="text-muted-foreground">🌐</span>
        <span class="text-sm font-medium">开始浏览</span>
        <span class="text-xs text-muted-foreground">输入网址并按回车打开页面</span>
      </div>
    {/if}
  </div>

  <details class="max-h-48 shrink-0 overflow-hidden border-t border-border/70 bg-muted/10">
    <summary
      class="flex min-h-10 cursor-pointer list-none items-center gap-2 px-3 text-xs text-muted-foreground hover:bg-muted/40"
    >
      <span class="font-medium text-foreground">操作轨迹</span>
      <span>({filteredTraces.length})</span>
      <span class="ml-auto"
        >{isTerminal ? "会话已归档 · 只读" : interacting ? "正在响应…" : "点击查看"}</span
      >
    </summary>
    <div class="max-h-36 space-y-2 overflow-y-auto border-t border-border/60 p-3">
      {#if traces.length === 0}
        <p class="py-3 text-center text-xs text-muted-foreground">暂无浏览器操作记录</p>
      {:else}
        <div class="mb-2 flex justify-end">
          <input
            type="text"
            placeholder="检索步骤…"
            aria-label="检索浏览器操作步骤"
            class="w-40 rounded-md border border-border/80 bg-background px-2 py-1 text-[10px] text-foreground outline-none focus:border-primary"
            bind:value={searchQuery}
          />
        </div>
        {#each filteredTraces as trace (trace.stepIndex)}
          {@const actMeta = formatBrowserAction(trace.actionType)}
          {@const traceStatus = traceStatusMeta(trace.status)}
          <div
            class="flex items-start gap-2 rounded-lg border border-border/70 bg-card p-2.5 text-xs"
          >
            <span
              class="flex h-5 w-5 shrink-0 items-center justify-center rounded-md bg-muted text-[11px] font-bold text-muted-foreground"
              >{trace.stepIndex}</span
            >
            <div class="min-w-0 flex-1 space-y-1">
              <div class="flex items-center justify-between gap-2">
                <span
                  class="inline-flex items-center gap-1 rounded border px-1.5 py-0.5 text-[9px] font-medium {actMeta.badgeClass}"
                >
                  <span>{actMeta.icon}</span><span>{actMeta.label}</span>
                </span>
                <span class="text-[10px] font-mono text-muted-foreground"
                  >{trace.durationMs > 0 ? `${trace.durationMs}ms` : ""}</span
                >
                <span
                  class="rounded border px-1.5 py-0.5 text-[9px] font-medium {traceStatus.className}"
                  >{traceStatus.label}</span
                >
              </div>
              <p class="break-all text-[11px] leading-snug text-foreground">
                {sanitizeTraceText(trace.description)}
              </p>
              {#if trace.selector}<div class="truncate font-mono text-[10px] text-muted-foreground">
                  Selector: {trace.selector}
                </div>{/if}
              {#if trace.error}<div
                  class="rounded bg-destructive/10 p-1 text-[10px] text-destructive"
                >
                  {trace.error}
                </div>{/if}
              {#if trace.screenshotData}
                <button
                  type="button"
                  class="mt-1 flex items-center gap-2 rounded-md border border-border/70 p-1 text-left hover:bg-accent/50"
                  aria-label={`查看第 ${trace.stepIndex} 步截图`}
                  onclick={() => (previewImageModal = trace.screenshotData ?? null)}
                >
                  <img
                    src={trace.screenshotData}
                    alt=""
                    class="h-12 w-20 rounded object-cover"
                    loading="lazy"
                  />
                  <span class="text-[10px] text-muted-foreground">查看此步页面截图</span>
                </button>
              {/if}
            </div>
          </div>
        {/each}
      {/if}
    </div>
  </details>
</div>

<!-- Screenshot Full Preview Modal -->
{#if previewImageModal}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4 backdrop-blur-xs"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onclick={() => (previewImageModal = null)}
  >
    <div
      class="max-h-[85vh] max-w-4xl overflow-hidden rounded-2xl border border-border bg-card p-3 shadow-2xl space-y-2"
    >
      <div class="flex items-center justify-between text-xs px-2">
        <span class="font-semibold text-foreground">网页快照完整大图</span>
        <button
          type="button"
          class="rounded p-1 text-muted-foreground hover:text-foreground"
          onclick={() => (previewImageModal = null)}
        >
          ✕
        </button>
      </div>
      <img
        src={previewImageModal}
        alt="页面完整快照"
        class="max-h-[75vh] w-full rounded-xl object-contain"
      />
    </div>
  </div>
{/if}
