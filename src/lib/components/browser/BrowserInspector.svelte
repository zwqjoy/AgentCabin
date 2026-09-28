<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getBrowserSession, getBrowserTraces } from "$lib/api/work";
  import { getTransport } from "$lib/transport";
  import { platform } from "$lib/platform";
  import { withTimeout } from "$lib/utils/async-utils";
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
  let searchQuery = $state("");
  let previewImageModal = $state<string | null>(null);
  let copiedUrl = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  let pollInterval: ReturnType<typeof setInterval> | null = null;
  let unlistenBrowserEvent: (() => void) | null = null;
  let stateRequestInFlight = false;
  let pendingStateRefresh = false;
  let stateRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  let lastLoadedUpdatedAt = "";
  const BROWSER_STATE_TIMEOUT_MS = 5_000;

  function isTerminalStatus(status: string | undefined): boolean {
    return status === "completed" || status === "failed" || status === "closed";
  }

  const isTerminal = $derived(readOnly || isTaskCompleted);
  const effectiveRunId = $derived(runId?.trim() || "default-browser");

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

  async function copyCurrentUrl() {
    if (!session?.currentUrl) return;
    try {
      await navigator.clipboard.writeText(session.currentUrl);
      copiedUrl = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copiedUrl = false;
      }, 2000);
    } catch (e) {
      console.warn("Failed to copy URL:", e);
    }
  }

  async function openInExternalBrowser() {
    if (!session?.currentUrl) return;
    try {
      await platform.shell.openExternal(session.currentUrl);
    } catch (e) {
      console.warn("Failed to open external browser:", e);
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
    if (copyTimer) clearTimeout(copyTimer);
    unlistenBrowserEvent?.();
    unlistenBrowserEvent = null;
  });
</script>

<div class="flex h-full w-full flex-col overflow-hidden bg-background text-foreground">
  <!-- Observation Panel Top Header -->
  <div
    class="flex min-h-11 shrink-0 items-center justify-between gap-2 border-b border-border/70 bg-muted/20 px-3 py-1.5 text-xs"
  >
    <div class="flex items-center gap-2 min-w-0 flex-1">
      <div class="flex items-center gap-1.5 shrink-0 text-muted-foreground font-medium">
        <span class="text-sm">🌐</span>
        <span class="hidden sm:inline">浏览器</span>
      </div>

      <!-- Current URL display with external actions -->
      {#if session?.currentUrl}
        <div
          class="flex items-center gap-1.5 min-w-0 flex-1 bg-muted/40 border border-border/70 rounded-md px-2 py-1"
        >
          <span class="text-muted-foreground text-[11px] shrink-0">🔒</span>
          <span
            class="font-mono text-[11px] text-foreground truncate select-all flex-1"
            title={session.currentUrl}
          >
            {session.currentUrl}
          </span>
          <button
            type="button"
            class="shrink-0 p-0.5 rounded text-muted-foreground hover:text-foreground hover:bg-muted transition-colors text-[10px]"
            title={copiedUrl ? "已复制" : "复制网址"}
            aria-label="复制网址"
            onclick={copyCurrentUrl}
          >
            {#if copiedUrl}
              <span class="text-emerald-500">✓</span>
            {:else}
              <span>📋</span>
            {/if}
          </button>
          <button
            type="button"
            class="shrink-0 p-0.5 rounded text-muted-foreground hover:text-foreground hover:bg-muted transition-colors text-[10px]"
            title="在系统浏览器中打开"
            aria-label="在系统浏览器中打开"
            onclick={openInExternalBrowser}
          >
            ↗
          </button>
        </div>
      {:else}
        <div
          class="flex items-center gap-1.5 min-w-0 flex-1 bg-muted/20 border border-border/40 rounded-md px-2 py-1 text-[11px] text-muted-foreground"
        >
          <span>等待导航目标…</span>
        </div>
      {/if}
    </div>

    <!-- Status badge and controls -->
    <div class="flex items-center gap-1.5 shrink-0">
      <span
        class="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[10px] {displayStatusMeta.colorClass}"
        title={`模式：${mode.toUpperCase()}`}
      >
        <span class="h-1.5 w-1.5 rounded-full {displayStatusMeta.dotClass}"></span>
        {displayStatusMeta.label}
      </span>

      {#if onClose}
        <button
          type="button"
          class="shrink-0 rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          onclick={onClose}
          aria-label="关闭浏览器面板"
          title="关闭浏览器面板"
        >
          ✕
        </button>
      {/if}
    </div>
  </div>

  <!-- Agent Active Action Banner -->
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

  <!-- Observation Visual Screen Area -->
  <div
    class="relative min-h-0 flex-1 overflow-hidden bg-white dark:bg-zinc-900 flex flex-col items-center justify-center p-4"
  >
    {#if loading && !session}
      <div
        class="flex flex-col items-center justify-center gap-2 text-center text-sm text-muted-foreground"
      >
        <span class="text-2xl animate-pulse">🌐</span>
        <span>正在读取浏览器状态…</span>
      </div>
    {:else if session?.lastScreenshot}
      <div class="relative flex h-full w-full items-center justify-center overflow-auto group">
        <button
          type="button"
          class="relative flex h-full w-full items-center justify-center cursor-zoom-in"
          onclick={() => (previewImageModal = session?.lastScreenshot ?? null)}
          aria-label="点击放大查看页面快照"
        >
          <img
            src={session.lastScreenshot}
            alt="受控浏览器最新截图"
            class="max-h-full max-w-full rounded-md object-contain shadow-sm border border-border/40"
          />
          <span
            class="absolute bottom-3 right-3 rounded-md bg-black/60 px-2 py-1 text-[10px] text-white opacity-0 transition-opacity group-hover:opacity-100 backdrop-blur-xs"
          >
            🔍 点击放大
          </span>
        </button>
      </div>
    {:else}
      <div
        class="flex max-w-sm flex-col items-center justify-center gap-2 text-center text-muted-foreground"
      >
        <span class="text-3xl">🖥️</span>
        <span class="text-sm font-medium text-foreground">浏览器观察面板</span>
        <p class="text-xs leading-relaxed text-muted-foreground">
          由 Pi 浏览器扩展负责自动化操作。页面快照与操作轨迹将在此实时更新。
        </p>
        {#if session?.status === "running"}
          <span
            class="mt-2 inline-flex items-center gap-1.5 rounded-full bg-blue-500/10 px-2.5 py-0.5 text-[10px] text-blue-600 dark:text-blue-400"
          >
            <span class="h-1.5 w-1.5 rounded-full bg-blue-500 animate-pulse"></span>
            浏览器任务执行中…
          </span>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Action Traces Footer (Collapsible) -->
  <details open class="max-h-56 shrink-0 overflow-hidden border-t border-border/70 bg-muted/10">
    <summary
      class="flex min-h-10 cursor-pointer list-none items-center gap-2 px-3 text-xs text-muted-foreground hover:bg-muted/40 transition-colors"
    >
      <span class="font-medium text-foreground">操作轨迹</span>
      <span class="rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-mono">
        {filteredTraces.length}
      </span>
      <span class="ml-auto text-[11px]">
        {isTerminal ? "会话已结束 · 只读" : "实时记录"}
      </span>
    </summary>

    <div class="max-h-44 space-y-2 overflow-y-auto border-t border-border/60 p-3">
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
            class="flex items-start gap-2 rounded-lg border border-border/70 bg-card p-2.5 text-xs shadow-xs"
          >
            <span
              class="flex h-5 w-5 shrink-0 items-center justify-center rounded-md bg-muted text-[11px] font-bold text-muted-foreground"
            >
              {trace.stepIndex}
            </span>
            <div class="min-w-0 flex-1 space-y-1">
              <div class="flex items-center justify-between gap-2">
                <span
                  class="inline-flex items-center gap-1 rounded border px-1.5 py-0.5 text-[9px] font-medium {actMeta.badgeClass}"
                >
                  <span>{actMeta.icon}</span><span>{actMeta.label}</span>
                </span>
                <span class="text-[10px] font-mono text-muted-foreground">
                  {trace.durationMs > 0 ? `${trace.durationMs}ms` : ""}
                </span>
                <span
                  class="rounded border px-1.5 py-0.5 text-[9px] font-medium {traceStatus.className}"
                >
                  {traceStatus.label}
                </span>
              </div>
              <p class="break-all text-[11px] leading-snug text-foreground">
                {sanitizeTraceText(trace.description)}
              </p>
              {#if trace.selector}
                <div class="truncate font-mono text-[10px] text-muted-foreground">
                  Selector: {trace.selector}
                </div>
              {/if}
              {#if trace.error}
                <div class="rounded bg-destructive/10 p-1 text-[10px] text-destructive">
                  {trace.error}
                </div>
              {/if}
              {#if trace.screenshotData}
                <button
                  type="button"
                  class="mt-1 flex items-center gap-2 rounded-md border border-border/70 p-1 text-left hover:bg-accent/50 transition-colors"
                  aria-label={`查看第 ${trace.stepIndex} 步截图`}
                  onclick={() => (previewImageModal = trace.screenshotData ?? null)}
                >
                  <img
                    src={trace.screenshotData}
                    alt=""
                    class="h-12 w-20 rounded object-cover border border-border/40"
                    loading="lazy"
                  />
                  <span class="text-[10px] text-muted-foreground hover:text-foreground">
                    查看此步页面截图 🔍
                  </span>
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
          class="rounded p-1 text-muted-foreground hover:text-foreground transition-colors"
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
