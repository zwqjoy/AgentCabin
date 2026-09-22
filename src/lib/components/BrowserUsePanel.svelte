<script lang="ts">
  import { onMount } from "svelte";
  import { getTransport } from "$lib/transport";
  import type {
    BrowserRuntimePreparationPhase,
    BrowserRuntimePreparationProgress,
    WorkBrowserSummary,
  } from "$lib/types/work";

  interface Props {
    runtime: WorkBrowserSummary | null;
    enabled: boolean;
    onToggle: (enabled: boolean) => Promise<WorkBrowserSummary | void>;
    onPrepare: () => Promise<WorkBrowserSummary>;
  }

  let { runtime, enabled, onToggle, onPrepare }: Props = $props();

  let toggling = $state(false);
  let preparing = $state(false);
  let error = $state("");
  let success = $state("");
  let preparation = $state<BrowserRuntimePreparationProgress | null>(null);
  let preparationLogs = $state<string[]>([]);
  let preparationStartedAt = $state<number | null>(null);
  let preparationElapsedMs = $state(0);
  let elapsedTimer = $state<ReturnType<typeof setInterval> | null>(null);

  const preparationPhases: { id: BrowserRuntimePreparationPhase; label: string }[] = [
    { id: "initializing", label: "初始化" },
    { id: "verifying", label: "校验" },
    { id: "completed", label: "完成" },
  ];

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void getTransport()
      .listen<BrowserRuntimePreparationProgress>("browser-runtime-preparation", (event) => {
        handlePreparationProgress(event);
      })
      .then((cleanup) => {
        if (disposed) {
          cleanup();
        } else {
          unlisten = cleanup;
        }
      });

    return () => {
      disposed = true;
      unlisten?.();
      stopElapsedTimer();
    };
  });

  function statusLabel(): string {
    if (preparation?.status === "running") return preparationPhaseLabel(preparation.phase);
    if (preparation?.status === "failed") return "准备失败";
    if (!runtime) return "检查中";
    if (runtime.browserRuntimeAvailable) return "运行时就绪";
    if (!runtime.browserRuntimeNodeAvailable) return "缺少 Node.js";
    return "待准备";
  }

  function statusClass(): string {
    if (preparation?.status === "failed") {
      return "bg-red-500/10 text-red-700 dark:text-red-300";
    }
    if (preparation?.status === "running") {
      return "bg-blue-500/10 text-blue-700 dark:text-blue-300";
    }
    return runtime?.browserRuntimeAvailable
      ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
      : "bg-amber-500/10 text-amber-700 dark:text-amber-300";
  }

  async function toggle() {
    toggling = true;
    error = "";
    success = "";
    try {
      await onToggle(!enabled);
      success = !enabled ? "Browser Use 已启用" : "Browser Use 已停用";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      toggling = false;
    }
  }

  async function prepare() {
    preparing = true;
    error = "";
    success = "";
    preparation = null;
    preparationLogs = [];
    preparationStartedAt = Date.now();
    preparationElapsedMs = 0;
    startElapsedTimer();
    try {
      const updated = await onPrepare();
      success = updated.browserRuntimeAvailable
        ? "内置 Chromium 与原生 CDP 已准备完成"
        : updated.browserRuntimeMessage;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      preparing = false;
      stopElapsedTimer();
    }
  }

  function handlePreparationProgress(event: BrowserRuntimePreparationProgress) {
    if (event.phase === "initializing" && event.status === "running") {
      preparationLogs = [];
      preparationStartedAt = Date.now() - event.elapsedMs;
      startElapsedTimer();
    }
    if (preparationStartedAt === null) {
      preparationStartedAt = Date.now() - event.elapsedMs;
    }
    if (event.log && !preparationLogs.includes(event.log)) {
      preparationLogs = [...preparationLogs.slice(-99), event.log];
    }
    preparationElapsedMs = event.elapsedMs;
    preparation = event;
    if (event.error) {
      error = event.error;
    }
    if (event.status === "failed") {
      success = "";
      preparing = false;
      stopElapsedTimer();
    } else if (event.phase === "completed" && event.status === "completed") {
      preparing = false;
      stopElapsedTimer();
    }
  }

  function startElapsedTimer() {
    if (elapsedTimer !== null) return;
    elapsedTimer = setInterval(() => {
      if (preparationStartedAt !== null && preparation?.status === "running") {
        preparationElapsedMs = Date.now() - preparationStartedAt;
      }
    }, 250);
  }

  function stopElapsedTimer() {
    if (elapsedTimer === null) return;
    clearInterval(elapsedTimer);
    elapsedTimer = null;
  }

  function preparationPhaseLabel(phase: BrowserRuntimePreparationPhase): string {
    return preparationPhases.find((item) => item.id === phase)?.label ?? "准备运行环境";
  }

  function phaseState(
    phase: BrowserRuntimePreparationPhase,
  ): "pending" | "running" | "completed" | "failed" {
    const currentPreparation = preparation;
    if (!currentPreparation) return "pending";
    if (currentPreparation.phase === "completed") return "completed";

    if (currentPreparation.phase === "failed") {
      const failedIndex = preparationPhases.findIndex(
        (item) => item.id === currentPreparation.failedPhase,
      );
      const phaseIndex = preparationPhases.findIndex((item) => item.id === phase);
      if (phase === currentPreparation.failedPhase) return "failed";
      return phaseIndex >= 0 && failedIndex >= 0 && phaseIndex < failedIndex
        ? "completed"
        : "pending";
    }

    const currentIndex = preparationPhases.findIndex(
      (item) => item.id === currentPreparation.phase,
    );
    const phaseIndex = preparationPhases.findIndex((item) => item.id === phase);
    if (
      phaseIndex < currentIndex ||
      (phaseIndex === currentIndex && currentPreparation.status === "completed")
    ) {
      return "completed";
    }
    if (phaseIndex === currentIndex) return currentPreparation.status;
    return "pending";
  }

  function formatElapsed(milliseconds: number): string {
    const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return minutes > 0 ? `${minutes}分 ${String(seconds).padStart(2, "0")}秒` : `${seconds}秒`;
  }
</script>

<section class="space-y-4 rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5">
  <div class="flex flex-wrap items-start justify-between gap-4">
    <div class="flex items-start gap-3">
      <span
        class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-blue-500/10 text-xs font-bold text-blue-700 dark:text-blue-300"
        >B</span
      >
      <div>
        <div class="flex flex-wrap items-center gap-2">
          <h2 class="text-sm font-semibold text-foreground">浏览器操作 / Browser Use</h2>
          <span class="rounded-full px-2.5 py-1 text-[10px] font-medium {statusClass()}">
            {statusLabel()}
          </span>
        </div>
        <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
          打开真实网页并执行页面级交互：导航、页面快照、截图、标签页、点击、输入、选择和滚动。
          它与“网络访问”的搜索、抓取、引用能力分开管理。
        </p>
      </div>
    </div>
    <span class="rounded-lg border border-border/60 px-3 py-2 text-[11px] text-muted-foreground"
      >共享运行时 · 全局开关</span
    >
  </div>

  {#if error}
    <div
      class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
      role="alert"
    >
      <span class="min-w-0">{error}</span>
      {#if !preparing}
        <button
          type="button"
          class="shrink-0 rounded-md border border-red-400/30 px-2 py-1 font-medium hover:bg-red-400/10"
          onclick={() => void prepare()}
        >
          重试
        </button>
      {/if}
    </div>
  {/if}
  {#if success}
    <div
      class="rounded-lg border border-emerald-400/20 bg-emerald-400/5 px-3 py-2 text-xs text-emerald-700 dark:text-emerald-300"
      role="status"
    >
      {success}
    </div>
  {/if}

  <div class="grid gap-3 md:grid-cols-2">
    <div
      class="flex items-center justify-between gap-4 rounded-xl border border-border/60 bg-background/40 p-3.5"
    >
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-1.5">
          <span class="h-2 w-2 rounded-full {enabled ? 'bg-blue-500' : 'bg-muted-foreground/40'}"
          ></span>
          <span class="text-xs font-semibold text-foreground">全局浏览器操作</span>
        </div>
        <p class="mt-0.5 text-[11px] text-muted-foreground">
          控制是否启用内置浏览器；Chromium 由 Electron 持有，Agent 通过受保护的原生 CDP 中继操作。
        </p>
      </div>
      <button
        type="button"
        class="relative inline-flex h-7 w-12 shrink-0 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 {enabled
          ? 'bg-blue-600'
          : 'bg-muted-foreground/25'}"
        aria-label={`${enabled ? "停用" : "启用"}全局浏览器操作`}
        aria-pressed={enabled}
        disabled={toggling}
        onclick={() => void toggle()}
      >
        <span
          class="inline-block h-5 w-5 rounded-full bg-white shadow-sm transition-transform {enabled
            ? 'translate-x-6'
            : 'translate-x-1'}"
        ></span>
      </button>
    </div>

    <div class="rounded-xl border border-border/60 bg-background/40 p-3.5">
      <div class="flex items-center justify-between gap-3">
        <span class="text-xs font-semibold text-foreground">AgentCabin 内置浏览器</span>
        <button
          type="button"
          class="rounded-lg bg-primary px-3 py-1.5 text-[11px] font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-60"
          disabled={preparing || runtime?.browserRuntimeAvailable}
          onclick={() => void prepare()}
        >
          {preparing
            ? "准备中…"
            : preparation?.status === "failed"
              ? "重试"
              : runtime?.browserRuntimeAvailable
                ? "已准备"
                : "准备运行环境"}
        </button>
      </div>
      <div class="mt-2 flex flex-wrap gap-2 text-[10px]">
        <span
          class="rounded-md border border-border/60 bg-background px-2 py-1 {runtime?.browserRuntimeAvailable
            ? 'text-emerald-700 dark:text-emerald-300'
            : 'text-muted-foreground'}"
          >原生 CDP {runtime?.browserRuntimeAvailable ? "已就绪" : "未就绪"}</span
        >
      </div>
      <p class="mt-2 text-[11px] leading-5 text-muted-foreground">
        {runtime?.browserRuntimeMessage ?? "正在读取本机运行时状态…"}
      </p>
    </div>
  </div>

  {#if preparation}
    <section
      class="rounded-xl border border-border/60 bg-background/40 p-3.5"
      aria-label="Browser Use 运行环境准备进度"
      aria-live="polite"
    >
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div class="flex items-center gap-2">
            <span class="text-xs font-semibold text-foreground">运行环境准备</span>
            <span
              class="rounded-full px-2 py-0.5 text-[10px] {preparation.status === 'failed'
                ? 'bg-red-500/10 text-red-700 dark:text-red-300'
                : preparation.phase === 'completed'
                  ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                  : 'bg-blue-500/10 text-blue-700 dark:text-blue-300'}"
            >
              {preparation.status === "failed"
                ? "失败"
                : preparation.phase === "completed"
                  ? "已完成"
                  : "进行中"}
            </span>
          </div>
          <p class="mt-1 text-[11px] text-muted-foreground">{preparation.message}</p>
        </div>
        <div class="flex items-center gap-3 text-[11px] text-muted-foreground">
          <span>耗时 {formatElapsed(preparationElapsedMs)}</span>
          <span class="font-medium text-foreground">{preparation.progress}%</span>
        </div>
      </div>

      <div
        class="mt-3 h-1.5 overflow-hidden rounded-full bg-muted/70"
        role="progressbar"
        aria-label="Browser Use 运行环境准备进度"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={preparation.progress}
      >
        <div
          class="h-full rounded-full bg-primary transition-[width] duration-300"
          style={`width: ${preparation.progress}%`}
        ></div>
      </div>

      <div class="mt-3 grid gap-2 sm:grid-cols-5">
        {#each preparationPhases as phase}
          {@const state = phaseState(phase.id)}
          <div
            class="flex items-center gap-1.5 text-[10px] {state === 'pending'
              ? 'text-muted-foreground/60'
              : state === 'failed'
                ? 'text-red-600 dark:text-red-300'
                : state === 'running'
                  ? 'text-blue-600 dark:text-blue-300'
                  : 'text-emerald-600 dark:text-emerald-300'}"
          >
            <span
              class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full border text-[9px] {state ===
              'failed'
                ? 'border-red-400/50 bg-red-500/10'
                : state === 'running'
                  ? 'border-blue-400/50 bg-blue-500/10'
                  : state === 'completed'
                    ? 'border-emerald-400/50 bg-emerald-500/10'
                    : 'border-border/70'}"
              >{state === "completed"
                ? "✓"
                : state === "failed"
                  ? "!"
                  : state === "running"
                    ? "•"
                    : ""}</span
            >
            <span class="truncate">{phase.label}</span>
          </div>
        {/each}
      </div>

      {#if preparation.error}
        <div
          class="mt-3 rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-[11px] leading-5 text-red-600 dark:text-red-300"
          role="alert"
        >
          <span class="font-semibold">失败原因：</span>{preparation.error}
        </div>
      {/if}

      {#if preparationLogs.length > 0}
        <details class="mt-3 rounded-lg border border-border/50 bg-background/60" open>
          <summary class="cursor-pointer px-3 py-2 text-[11px] font-medium text-foreground">
            实时日志（{preparationLogs.length}）
          </summary>
          <div
            class="max-h-40 overflow-y-auto border-t border-border/50 px-3 py-2 font-mono text-[10px] leading-5 text-muted-foreground"
          >
            {#each preparationLogs as line}
              <div class="break-all">{line}</div>
            {/each}
          </div>
        </details>
      {/if}
    </section>
  {/if}

  <div class="grid gap-3 md:grid-cols-3">
    <div class="rounded-xl border border-border/60 bg-background/40 p-3">
      <div class="text-xs font-medium text-foreground">安全边界</div>
      <p class="mt-1 text-[11px] leading-5 text-muted-foreground">
        仅允许 HTTP/HTTPS；file、localhost、私有网段和云元数据地址默认拦截。
      </p>
    </div>
    <div class="rounded-xl border border-border/60 bg-background/40 p-3">
      <div class="text-xs font-medium text-foreground">权限策略</div>
      <p class="mt-1 text-[11px] leading-5 text-muted-foreground">
        导航与观察是读取；点击、输入、选择等交互沿用当前运行时的权限与审批策略。
      </p>
    </div>
    <div class="rounded-xl border border-border/60 bg-background/40 p-3">
      <div class="text-xs font-medium text-foreground">高风险动作</div>
      <p class="mt-1 text-[11px] leading-5 text-muted-foreground">
        点击、输入、选择和标签页变更按当前运行时的审批策略放行；明确的 standing rule 或 FullAccess
        才能持续放行，避免把登录、提交、支付、删除、上传等动作当作普通交互。
      </p>
    </div>
  </div>

  <p class="text-[10px] leading-4 text-muted-foreground">
    Browser Use 与 Web Access 共用同一个内置 Chromium 能力，启用状态全局共用； 不会要求用户手动执行
    npm 命令。
  </p>
</section>
