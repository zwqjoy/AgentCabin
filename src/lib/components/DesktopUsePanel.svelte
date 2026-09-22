<script lang="ts">
  import { getTransport } from "$lib/transport";
  import type { DesktopUseStatus } from "$lib/types/work";

  interface Props {
    status: DesktopUseStatus | null;
    loading?: boolean;
    onToggle: (enabled: boolean) => Promise<void>;
    onRefresh: () => Promise<void>;
    onRequestPermissions: () => Promise<void>;
    onOpenPermissionPane: (kind: "accessibility" | "screen-recording") => Promise<void>;
  }

  let {
    status,
    loading = false,
    onToggle,
    onRefresh,
    onRequestPermissions,
    onOpenPermissionPane,
  }: Props = $props();

  let toggling = $state(false);
  let refreshing = $state(false);
  let requestingPermissions = $state(false);
  let error = $state("");
  let success = $state("");

  let isDesktop = $derived(getTransport().isDesktop());
  let enabled = $derived(status?.enabled ?? false);
  let ready = $derived(status?.ready ?? false);
  let helperName = $derived(status?.command?.split(/[\\/]/).pop() || "agentcabin-computer-use");

  function statusLabel(): string {
    if (!isDesktop) return "仅支持 macOS";
    if (loading) return "检查中";
    if (!ready) return "未就绪";
    return enabled ? "已启用" : "已停用";
  }

  function statusClass(): string {
    if (!isDesktop || !ready) return "bg-amber-500/10 text-amber-700 dark:text-amber-300";
    return enabled
      ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
      : "bg-muted text-muted-foreground";
  }

  async function toggle() {
    if (toggling || !isDesktop) return;
    toggling = true;
    error = "";
    success = "";
    try {
      await onToggle(!enabled);
      success = "电脑控制已" + (!enabled ? "启用" : "停用") + "；新建会话时生效。";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      toggling = false;
    }
  }

  async function refresh() {
    if (refreshing) return;
    refreshing = true;
    error = "";
    success = "";
    try {
      await onRefresh();
      success = "电脑控制运行环境状态已更新。";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      refreshing = false;
    }
  }

  async function openSystemSettings(kind: "accessibility" | "screen-recording") {
    error = "";
    try {
      await onOpenPermissionPane(kind);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function requestPermissions() {
    if (requestingPermissions) return;
    requestingPermissions = true;
    error = "";
    success = "";
    try {
      await onRequestPermissions();
      success = "已向 macOS 请求权限；完成系统授权后请点击刷新状态。";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      requestingPermissions = false;
    }
  }
</script>

<section class="space-y-4 rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5">
  <div class="flex flex-wrap items-start justify-between gap-4">
    <div class="flex items-start gap-3">
      <span
        class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-violet-500/10 text-xs font-bold text-violet-700 dark:text-violet-300"
        aria-hidden="true"
      >
        <svg
          class="h-5 w-5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
        >
          <rect x="3" y="4" width="18" height="13" rx="2" />
          <path d="M8 21h8M12 17v4M7 9h.01M11 9h6M7 13h.01M11 13h6" />
        </svg>
      </span>
      <div>
        <div class="flex flex-wrap items-center gap-2">
          <h2 class="text-sm font-semibold text-foreground">电脑控制 / Desktop Use</h2>
          <span class="rounded-full px-2.5 py-1 text-[10px] font-medium {statusClass()}">
            {statusLabel()}
          </span>
        </div>
        <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
          让 Code 与 Work 观察并操作 macOS 原生应用：读取窗口、截图、点击、输入、按键和滚动。
          AgentCabin 内置 Computer Use V2，不需要安装 Skill；原生操作由 Host 托管的
          agentcabin-computer-use backend 提供。
        </p>
      </div>
    </div>
    <button
      type="button"
      class="rounded-lg border border-border/60 px-3 py-2 text-[11px] text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50"
      disabled={refreshing || loading}
      onclick={() => void refresh()}
    >
      {refreshing || loading ? "检查中…" : "刷新状态"}
    </button>
  </div>

  {#if error}
    <div
      class="rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-600 dark:text-red-300"
      role="alert"
    >
      {error}
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
          <span class="h-2 w-2 rounded-full {enabled ? 'bg-violet-500' : 'bg-muted-foreground/40'}"
          ></span>
          <span class="text-xs font-semibold text-foreground">启用电脑控制</span>
        </div>
        <p class="mt-0.5 text-[11px] leading-5 text-muted-foreground">
          开启后，Code 与 Work 的运行时才会加载桌面操作工具。
        </p>
      </div>
      <button
        type="button"
        class="relative inline-flex h-7 w-12 shrink-0 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 {enabled
          ? 'bg-violet-600'
          : 'bg-muted-foreground/25'}"
        aria-label={(enabled ? "停用" : "启用") + "电脑控制"}
        aria-pressed={enabled}
        disabled={toggling || !isDesktop}
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
        <span class="text-xs font-semibold text-foreground">AgentCabin 托管运行时</span>
        <span
          class="rounded-md px-2 py-1 text-[10px] {ready
            ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
            : 'bg-amber-500/10 text-amber-700 dark:text-amber-300'}"
        >
          {ready ? "已找到" : "未找到"}
        </span>
      </div>
      <p class="mt-2 text-[11px] leading-5 text-muted-foreground">Helper：{helperName}</p>
      <p class="mt-1 text-[11px] leading-5 text-muted-foreground">
        {status?.message ?? "正在读取本机运行时状态…"}
      </p>
    </div>
  </div>

  <div class="rounded-xl border border-border/60 bg-background/40">
    <div class="flex items-center justify-between gap-3 border-b border-border/50 px-3.5 py-3">
      <div>
        <div class="text-xs font-semibold text-foreground">macOS 系统权限</div>
        <p class="mt-1 text-[11px] leading-5 text-muted-foreground">
          Computer Use 需要以下权限读取界面并发送鼠标键盘事件。开发模式下授权对象可能显示为启动
          AgentCabin 的终端或调试宿主。
        </p>
      </div>
      <button
        type="button"
        class="shrink-0 rounded-lg border border-border/60 px-3 py-2 text-[11px] text-foreground hover:bg-accent disabled:opacity-50"
        disabled={!isDesktop || !ready || requestingPermissions}
        onclick={() => void requestPermissions()}
      >
        {requestingPermissions ? "请求中…" : "请求系统授权"}
      </button>
    </div>
    <div class="grid divide-y divide-border/50 md:grid-cols-2 md:divide-x md:divide-y-0">
      <div class="flex items-center justify-between gap-3 px-3.5 py-3">
        <div>
          <div class="flex items-center gap-2 text-xs font-medium text-foreground">
            辅助功能 / Accessibility
            <span
              class="rounded px-1.5 py-0.5 text-[9px] {status?.accessibility
                ? 'bg-emerald-500/10 text-emerald-700'
                : 'bg-red-500/10 text-red-600'}"
            >
              {status?.accessibility ? "已授权" : "未授权"}
            </span>
          </div>
          <div class="mt-0.5 text-[11px] text-muted-foreground">读取 AX 元素、发送点击和按键</div>
        </div>
        <button
          type="button"
          class="shrink-0 text-xs font-medium text-sky-600 hover:underline dark:text-sky-400"
          disabled={!isDesktop}
          onclick={() => void openSystemSettings("accessibility")}
        >
          打开设置
        </button>
      </div>
      <div class="flex items-center justify-between gap-3 px-3.5 py-3">
        <div>
          <div class="flex items-center gap-2 text-xs font-medium text-foreground">
            屏幕录制 / Screen Recording
            <span
              class="rounded px-1.5 py-0.5 text-[9px] {status?.screenRecording
                ? 'bg-emerald-500/10 text-emerald-700'
                : 'bg-red-500/10 text-red-600'}"
            >
              {status?.screenRecording ? "已授权" : "未授权"}
            </span>
          </div>
          <div class="mt-0.5 text-[11px] text-muted-foreground">读取窗口截图</div>
        </div>
        <button
          type="button"
          class="shrink-0 text-xs font-medium text-sky-600 hover:underline dark:text-sky-400"
          disabled={!isDesktop}
          onclick={() => void openSystemSettings("screen-recording")}
        >
          打开设置
        </button>
      </div>
    </div>
  </div>

  <div class="grid gap-3 md:grid-cols-3">
    <div class="rounded-xl border border-border/60 bg-background/40 p-3">
      <div class="text-xs font-medium text-foreground">观察优先</div>
      <p class="mt-1 text-[11px] leading-5 text-muted-foreground">
        每次关键动作前后重新获取截图和 AX 元素，避免使用过期坐标。
      </p>
    </div>
    <div class="rounded-xl border border-border/60 bg-background/40 p-3">
      <div class="text-xs font-medium text-foreground">会话隔离</div>
      <p class="mt-1 text-[11px] leading-5 text-muted-foreground">
        一次只允许一个 Code/Work 会话持有桌面租约，结束时自动释放。
      </p>
    </div>
    <div class="rounded-xl border border-border/60 bg-background/40 p-3">
      <div class="text-xs font-medium text-foreground">高风险动作</div>
      <p class="mt-1 text-[11px] leading-5 text-muted-foreground">
        登录、提交、支付、删除、上传等动作仍遵守当前运行时的审批策略。
      </p>
    </div>
  </div>
</section>
