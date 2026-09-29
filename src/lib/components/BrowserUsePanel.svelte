<script lang="ts">
  import type { WorkBrowserSummary } from "$lib/types/work";

  interface Props {
    runtime?: WorkBrowserSummary | null;
    enabled: boolean;
    onToggle: (enabled: boolean) => Promise<WorkBrowserSummary | void>;
    onPrepare?: () => Promise<WorkBrowserSummary>;
  }

  let { runtime = null, enabled, onToggle, onPrepare }: Props = $props();

  let toggling = $state(false);
  let error = $state("");
  let success = $state("");

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
          <span
            class="rounded-full px-2.5 py-1 text-[10px] font-medium {enabled
              ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
              : 'bg-muted text-muted-foreground'}"
          >
            {enabled ? "已就绪" : "未启用"}
          </span>
        </div>
        <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
          由 Pi 原生浏览器扩展负责自动化执行；AgentCabin 负责 capability、Host 集成与 UX 展示。
        </p>
      </div>
    </div>
    <span class="rounded-lg border border-border/60 px-3 py-2 text-[11px] text-muted-foreground"
      >Pi 原生能力 · 全局开关</span
    >
  </div>

  {#if error}
    <div
      class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
      role="alert"
    >
      <span class="min-w-0">{error}</span>
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
          <span class="text-xs font-semibold text-foreground">全局浏览器自动化</span>
        </div>
        <p class="mt-0.5 text-[11px] text-muted-foreground">
          开启后，Code 模式与 Work 模式将自动接入 Pi 原生浏览器自动化执行栈。
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
        <span class="text-xs font-semibold text-foreground">执行栈闭包</span>
        <span
          class="rounded-md border border-border/60 bg-background px-2 py-0.5 text-[10px] {enabled
            ? 'text-emerald-700 dark:text-emerald-300'
            : 'text-muted-foreground'}"
        >
          {enabled ? "已就绪" : "待启用"}
        </span>
      </div>
      <div class="mt-2 flex flex-wrap gap-1.5 text-[10px]">
        <span class="rounded bg-muted px-2 py-0.5 font-mono text-muted-foreground"
          >pi-agent-browser-native 0.8.2</span
        >
        <span class="rounded bg-muted px-2 py-0.5 font-mono text-muted-foreground"
          >agent-browser 0.37.0</span
        >
        <span class="rounded bg-muted px-2 py-0.5 font-mono text-muted-foreground">Pi 0.87.1</span>
      </div>
      <p class="mt-2 text-[11px] leading-5 text-muted-foreground">
        由 Pi 执行浏览器自动化；AgentCabin 负责启用、打包和展示执行状态。
      </p>
    </div>
  </div>

  <div class="grid gap-3 md:grid-cols-2">
    <div class="rounded-xl border border-border/60 bg-background/40 p-3">
      <div class="text-xs font-medium text-foreground">原生工具接入</div>
      <p class="mt-1 text-[11px] leading-5 text-muted-foreground">
        模型直接调用 <code class="rounded bg-muted px-1 font-mono text-[10px]">agent_browser</code
        >、
        <code class="rounded bg-muted px-1 font-mono text-[10px]">agent_browser_code</code> 与
        <code class="rounded bg-muted px-1 font-mono text-[10px]">agent_browser_tools</code> 原生入口工具，
        高级能力按需由扩展动态激活。
      </p>
    </div>
    <div class="rounded-xl border border-border/60 bg-background/40 p-3">
      <div class="text-xs font-medium text-foreground">状态与观测</div>
      <p class="mt-1 text-[11px] leading-5 text-muted-foreground">
        执行过程中的实时截图、页面状态变化（URL /
        标题）以及控制流指令将同步回传至活动面板与浏览器检查器展示。
      </p>
    </div>
  </div>
</section>
