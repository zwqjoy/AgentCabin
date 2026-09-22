<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getBrowserSession, getBrowserTraces, browserUserInteract } from "$lib/api/work";
  import { getTransport } from "$lib/transport";
  import { withTimeout } from "$lib/utils/async-utils";
  import EmbeddedBrowserSurface from "$lib/components/browser/EmbeddedBrowserSurface.svelte";
  import { isEmbeddedBrowserAvailable } from "$lib/platform/browser";
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
  let searchQuery = $state("");
  let previewImageModal = $state<string | null>(null);
  let embeddedAttachFailed = $state(false);
  let pollInterval: ReturnType<typeof setInterval> | null = null;
  let unlistenBrowserEvent: (() => void) | null = null;
  let stateRequestInFlight = false;
  const BROWSER_STATE_TIMEOUT_MS = 5_000;

  let interactError = $state("");

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

  async function loadState() {
    const id = effectiveRunId;
    if (!id) {
      loading = false;
      return;
    }
    if (stateRequestInFlight) return;
    stateRequestInFlight = true;
    try {
      const sess = await withTimeout(
        getBrowserSession(id),
        BROWSER_STATE_TIMEOUT_MS,
        "受控浏览器状态读取超时",
      );
      if (sess) {
        session = sess;
        traces = sess.traces || [];
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
    }
  }

  onMount(() => {
    const transport = getTransport();
    let destroyed = false;
    const id = effectiveRunId;
    transport.subscribeRun(id);
    void transport
      .listen<BrowserEvent>("browser-event", (event) => {
        if (event.runId === effectiveRunId) void loadState();
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
    }, 2000);
    void loadState();

    return () => {
      destroyed = true;
      transport.unsubscribeRun(id);
    };
  });

  onDestroy(() => {
    stopPolling();
    unlistenBrowserEvent?.();
    unlistenBrowserEvent = null;
  });

  $effect(() => {
    if (session?.currentUrl && !interacting) {
      addressInput = session.currentUrl;
    }
  });

  async function performInteract(action: string, params: Record<string, unknown> = {}) {
    const id = effectiveRunId;
    if (!id || interacting || isTerminal) return;
    interacting = true;
    interactError = "";
    try {
      const updated = await browserUserInteract(id, action, params);
      if (updated) {
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

<div class="flex h-full w-full flex-col overflow-hidden bg-card text-foreground">
  <!-- Header Bar -->
  <div class="flex items-center justify-between border-b border-border/70 px-4 py-3 bg-muted/20">
    <div class="flex items-center gap-2.5 min-w-0">
      <span
        class="flex h-7 w-7 items-center justify-center rounded-lg bg-blue-500/10 text-xs font-bold text-blue-600 dark:text-blue-400"
      >
        🌐
      </span>
      <div class="min-w-0">
        <div class="flex items-center gap-2">
          <span class="text-xs font-semibold text-foreground truncate">
            {session?.pageTitle || "受控浏览器会话"}
          </span>
          <span
            class="inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[9px] font-medium {displayStatusMeta.colorClass}"
          >
            <span class="h-1.5 w-1.5 rounded-full {displayStatusMeta.dotClass}"></span>
            <span>{displayStatusMeta.label}</span>
          </span>
          <span
            class="rounded bg-secondary/80 px-1.5 py-0.2 text-[9px] font-mono text-muted-foreground uppercase"
          >
            {mode}
          </span>
          <span
            class="rounded bg-blue-500/10 px-1.5 py-0.2 text-[9px] font-medium text-blue-600 dark:text-blue-400"
          >
            {session?.surface === "embedded"
              ? t("browser_surfaceEmbedded")
              : t("browser_surfaceManaged")}
          </span>
        </div>
      </div>
    </div>

    {#if onClose}
      <button
        type="button"
        class="rounded-lg p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
        onclick={onClose}
      >
        ✕
      </button>
    {/if}
  </div>

  <!-- Codex-style Browser Navigation Bar -->
  <div
    class="flex items-center gap-1.5 border-b border-border/60 bg-background/80 px-3 py-1.5 text-xs"
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
        class="text-[10px] text-primary hover:underline shrink-0 px-1"
        title="复制完整网址"
        onclick={() => navigator.clipboard.writeText(session?.currentUrl || "")}
      >
        复制
      </button>
    {/if}
  </div>

  {#if interactError}
    <div
      class="mx-3 my-1.5 flex items-center justify-between rounded-lg border border-red-500/20 bg-red-500/10 px-2.5 py-1.5 text-xs text-red-600 dark:text-red-400"
    >
      <span class="truncate">{interactError}</span>
      <button
        type="button"
        class="ml-2 shrink-0 font-bold hover:opacity-80"
        onclick={() => (interactError = "")}
      >
        ✕
      </button>
    </div>
  {/if}

  <!-- Main Split: Live Interactive Viewport & Action Traces -->
  <div class="flex-1 overflow-y-auto p-4 space-y-4">
    <!-- Live Interactive Canvas Card -->
    <div class="rounded-xl border border-border/80 bg-background/60 p-2.5 shadow-xs space-y-2">
      <div class="flex items-center justify-between text-xs px-1">
        <div class="flex items-center gap-1.5">
          <span class="font-semibold text-foreground flex items-center gap-1">
            <span>🖥️</span>
            <span>内嵌交互画布</span>
          </span>
          {#if interacting}
            <span
              class="inline-flex items-center gap-1 text-[10px] text-primary animate-pulse font-medium"
            >
              <span class="h-1.5 w-1.5 rounded-full bg-primary animate-ping"></span>
              响应中…
            </span>
          {/if}
        </div>
        {#if !useEmbeddedSurface && session?.lastScreenshot}
          <div class="flex items-center gap-2">
            <button
              type="button"
              class="text-[11px] text-primary hover:underline"
              onclick={() => (previewImageModal = session?.lastScreenshot || null)}
            >
              放大查看
            </button>
          </div>
        {/if}
      </div>

      {#if useEmbeddedSurface}
        <EmbeddedBrowserSurface
          viewId={embeddedViewId}
          runId={effectiveRunId}
          initialUrl={session?.currentUrl ?? ""}
          visible={surfaceVisible && !overlayOpen}
          onError={() => (embeddedAttachFailed = true)}
        />
      {:else if loading && !session}
        <div
          class="flex h-48 flex-col items-center justify-center rounded-lg border border-dashed border-border/70 text-xs text-muted-foreground bg-muted/10 space-y-1"
        >
          <span class="text-xl">🖥️</span>
          <span>正在连接受控浏览器…</span>
        </div>
      {:else if session?.lastScreenshot}
        <div
          class="relative w-full overflow-hidden rounded-lg border border-border/70 bg-black/5 aspect-[16/10] select-none"
        >
          <img
            src={session.lastScreenshot}
            alt="受控浏览器页面"
            class="h-full w-full object-contain bg-white dark:bg-zinc-900 pointer-events-none"
          />
        </div>
      {:else}
        <div
          class="flex h-48 flex-col items-center justify-center rounded-lg border border-dashed border-border/70 text-xs text-muted-foreground bg-muted/10 space-y-1.5 text-center px-4"
        >
          <span class="text-2xl">🌐</span>
          <span class="font-medium text-foreground">等待网页加载</span>
          <span class="text-[10px] text-muted-foreground"
            >在上方地址栏输入网址按回车，或向 Agent 发出浏览指令</span
          >
        </div>
      {/if}
    </div>

    <!-- Actions Control Bar -->
    {#if isTerminal}
      <div
        class="flex items-center justify-between gap-2 rounded-xl border border-border/80 bg-muted/30 p-2.5 text-xs shadow-xs"
      >
        <div class="flex items-center gap-2.5 min-w-0">
          <span
            class="flex h-6 w-6 items-center justify-center rounded-lg bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 font-bold text-xs shrink-0"
          >
            ✓
          </span>
          <div class="min-w-0">
            <div class="font-medium text-foreground text-xs truncate">
              任务已完成 · 浏览器会话已归档
            </div>
            <div class="text-[11px] text-muted-foreground truncate">
              当前为最终页面快照与时序操作轨迹回顾（只读）
            </div>
          </div>
        </div>
      </div>
    {:else}
      <div
        class="flex items-center justify-between gap-2 rounded-xl border border-border/80 bg-muted/30 p-2 text-xs"
      >
        <div class="flex items-center gap-2 min-w-0 px-1">
          <span class="text-muted-foreground text-[11px] truncate"
            >💡 可直接在上方画面中点击或滚动交互</span
          >
        </div>
      </div>
    {/if}

    <!-- Action Traces Timeline -->
    <div class="space-y-2.5">
      <div class="flex items-center justify-between">
        <span class="text-xs font-semibold text-foreground flex items-center gap-1.5">
          <span>⚡</span>
          <span>操作轨迹时序 ({filteredTraces.length})</span>
        </span>
        <input
          type="text"
          placeholder="检索步骤…"
          class="w-32 rounded-lg border border-border/80 bg-background px-2 py-0.5 text-[10px] text-foreground outline-none focus:border-primary"
          bind:value={searchQuery}
        />
      </div>

      {#if traces.length === 0}
        <div class="flex h-24 items-center justify-center text-xs text-muted-foreground">
          等待浏览器动作触发…
        </div>
      {:else}
        <div class="space-y-2">
          {#each filteredTraces as trace (trace.stepIndex)}
            {@const actMeta = formatBrowserAction(trace.actionType)}
            <div
              class="flex items-start gap-2 rounded-xl border border-border/70 bg-card p-2.5 text-xs transition-colors hover:bg-muted/20"
            >
              <span
                class="flex h-5 w-5 shrink-0 items-center justify-center rounded-md text-[11px] font-bold bg-muted text-muted-foreground"
              >
                {trace.stepIndex}
              </span>
              <div class="min-w-0 flex-1 space-y-0.5">
                <div class="flex items-center justify-between gap-1">
                  <span
                    class="inline-flex items-center gap-1 rounded border px-1.5 py-0.2 text-[9px] font-medium {actMeta.badgeClass}"
                  >
                    <span>{actMeta.icon}</span>
                    <span>{actMeta.label}</span>
                  </span>
                  <span class="text-[10px] font-mono text-muted-foreground">
                    {trace.durationMs > 0 ? `${trace.durationMs}ms` : ""}
                  </span>
                </div>
                <p class="font-medium text-foreground leading-snug break-all text-[11px]">
                  {sanitizeTraceText(trace.description)}
                </p>
                {#if trace.selector}
                  <div class="font-mono text-[10px] text-muted-foreground/80 truncate">
                    Selector: {trace.selector}
                  </div>
                {/if}
                {#if trace.error}
                  <div class="rounded bg-red-500/10 p-1 text-[10px] text-red-600 dark:text-red-400">
                    {trace.error}
                  </div>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
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
