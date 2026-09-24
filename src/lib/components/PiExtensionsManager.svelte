<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import Modal from "$lib/components/Modal.svelte";
  import type { AgentSettings, InstalledPlugin } from "$lib/types";
  import { t } from "$lib/i18n/index.svelte";
  import { dbgWarn } from "$lib/utils/debug";

  interface Props {
    showToast?: (message: string, type?: "success" | "error" | "info") => void;
  }

  let { showToast }: Props = $props();

  let installedExtensions = $state<InstalledPlugin[]>([]);
  let piAgentSettings = $state<AgentSettings | null>(null);
  let piLspEnabled = $derived(Boolean(piAgentSettings?.pi_lsp_enabled));
  let showLspConfirmModal = $state(false);
  let togglingLsp = $state(false);
  let loading = $state(false);
  let installingSource = $state<string | null>(null);
  let busyKey = $state<string | null>(null);
  let customSource = $state("");
  let localError = $state("");

  const RECOMMENDED_PI_EXTENSIONS = [
    {
      name: "@ff-labs/pi-fff",
      pkgSource: "npm:@ff-labs/pi-fff",
      badge: "模糊文件搜索",
      badgeColor: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
      descKey: "piExt_fffCardDesc",
      links: [{ label: "Pi", url: "https://pi.dev/packages/@ff-labs/pi-fff" }],
    },
    {
      name: "@narumitw/pi-caffeinate",
      pkgSource: "npm:@narumitw/pi-caffeinate",
      badge: "防休眠",
      badgeColor: "bg-amber-600/10 text-amber-700 dark:text-amber-300",
      descKey: "piExt_caffeinateCardDesc",
      links: [{ label: "Pi", url: "https://pi.dev/packages/@narumitw/pi-caffeinate" }],
    },
    {
      name: "@narumitw/pi-chrome-devtools",
      pkgSource: "npm:@narumitw/pi-chrome-devtools",
      badge: "DevTools 调试",
      badgeColor: "bg-emerald-600/10 text-emerald-700 dark:text-emerald-300",
      descKey: "piExt_chromeDevtoolsCardDesc",
      links: [{ label: "Pi", url: "https://pi.dev/packages/@narumitw/pi-chrome-devtools" }],
    },
  ];

  const PI_NATIVE_FEATURES: Array<{
    label: string;
    description: string;
    pkg: string;
  }> = [
    {
      label: "权限系统 (Permission)",
      description: "拦截高危操作并在执行前请求授权",
      pkg: "npm:@gotgenes/pi-permission-system",
    },
    {
      label: "规划模式 (Plan Mode)",
      description: "在编码前先规划技术路线并与用户确认",
      pkg: "npm:@narumitw/pi-plan-mode",
    },
    {
      label: "目标模式 (Goal Mode)",
      description: "长时间自主运行直到达成目标",
      pkg: "npm:@narumitw/pi-goal",
    },
    {
      label: "结构化提问 (Ask User Question)",
      description: "需要用户决策时一次提交清晰、可回答的问题",
      pkg: "npm:@juicesharp/rpiv-ask-user-question",
    },
    {
      label: "任务进度 (Todo)",
      description: "追踪多步任务，并在每一步完成后同步状态",
      pkg: "npm:@juicesharp/rpiv-todo",
    },
    {
      label: "代码智能 (LSP / pi-lsp)",
      description: "精准跳转定义、查找引用与实时代码诊断",
      pkg: "npm:@narumitw/pi-lsp",
    },
    {
      label: "上下文剪枝 (Context Prune)",
      description: "长对话中自动裁剪冗余上下文，节约 Token",
      pkg: "npm:pi-context-prune",
    },
    {
      label: "批量编辑 (Multi Edit)",
      description: "跨文件、跨位置执行结构化批量 Patch",
      pkg: "npm:pi-mono-multi-edit",
    },
  ];

  const WORK_NATIVE_FEATURES = [
    {
      label: "Work Runtime",
      description: "隔离运行时、策略控制、Inbox 审批与成果交付",
      pkg: "AgentCabin Work Core",
      resident: true,
    },
    {
      label: "结构化提问 (Ask User Question)",
      description: "需要用户决策时一次提交清晰、可回答的问题",
      pkg: "npm:@juicesharp/rpiv-ask-user-question",
      resident: true,
    },
    {
      label: "任务进度 (Todo)",
      description: "追踪多步任务，并在每一步完成后同步状态",
      pkg: "npm:@juicesharp/rpiv-todo",
      resident: true,
    },
    {
      label: "上下文用量 (Context Usage)",
      description: "报告会话上下文窗口与当前用量",
      pkg: "AgentCabin Context Usage",
      resident: true,
    },
    {
      label: "浏览器适配器 (Browser)",
      description: "浏览器控制与网页操作工具",
      pkg: "Work Browser adapter",
      resident: false,
    },
    {
      label: "MCP 适配器 (MCP)",
      description: "将已启用的 MCP 服务接入 Work 会话",
      pkg: "Work MCP adapter",
      resident: false,
    },
  ];

  async function loadData() {
    loading = true;
    localError = "";
    try {
      const [exts, agent] = await Promise.all([
        api.listPiSharedExtensions("code"),
        api.getAgentSettings("pi").catch(() => null),
      ]);
      installedExtensions = exts;
      piAgentSettings = agent;
    } catch (e) {
      dbgWarn("PiExtensionsManager", "load failed", e);
      localError = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  function handleToggleLsp() {
    if (togglingLsp) return;
    if (piLspEnabled) {
      void applyLspSetting(false);
    } else {
      showLspConfirmModal = true;
    }
  }

  async function applyLspSetting(enabled: boolean) {
    togglingLsp = true;
    localError = "";
    try {
      const updated = await api.updateAgentSettings("pi", {
        pi_lsp_enabled: enabled,
      });
      piAgentSettings = updated;
      if (enabled) {
        showToast?.("已启用代码智能 (LSP)", "success");
      } else {
        showToast?.("已禁用代码智能 (LSP)，首轮启动将更快速", "info");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      localError = msg;
      showToast?.(msg, "error");
    } finally {
      togglingLsp = false;
      showLspConfirmModal = false;
    }
  }

  onMount(() => {
    void loadData();
  });

  function getInstalledExtension(sourceName: string): InstalledPlugin | null {
    return (
      installedExtensions.find(
        (ext) =>
          ext.name.toLowerCase() === sourceName.toLowerCase() ||
          ext.pluginId?.toLowerCase() === sourceName.toLowerCase() ||
          ext.path?.toLowerCase().includes(sourceName.toLowerCase()),
      ) ?? null
    );
  }

  async function handleInstall(source: string) {
    if (!source.trim() || installingSource || busyKey) return;
    installingSource = source;
    busyKey = `install:${source}`;
    localError = "";
    try {
      const res = await api.installPiSharedExtension(source.trim());
      if (!res.success) throw new Error(res.message || `安装失败: ${source}`);
      showToast?.(t("piExt_installSuccess", { name: source }), "success");
      customSource = "";
      await loadData();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      localError = msg;
      showToast?.(msg, "error");
    } finally {
      installingSource = null;
      busyKey = null;
    }
  }

  async function handleUpdate(ext: InstalledPlugin) {
    const id = ext.pluginId ?? ext.name;
    if (busyKey) return;
    busyKey = `update:${id}`;
    localError = "";
    try {
      const res = await api.updatePiSharedExtension(id);
      if (!res.success) throw new Error(res.message || `更新失败: ${ext.name}`);
      showToast?.(t("plugin_updatedName", { name: ext.name }), "success");
      await loadData();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      localError = msg;
      showToast?.(msg, "error");
    } finally {
      busyKey = null;
    }
  }

  async function handleUninstall(ext: InstalledPlugin) {
    const id = ext.pluginId ?? ext.name;
    if (busyKey) return;
    if (!confirm(`确定要从 ~/.agentcabin/pi 卸载 ${ext.name} 吗？`)) return;
    busyKey = `uninstall:${id}`;
    localError = "";
    try {
      const res = await api.uninstallPiSharedExtension(id);
      if (!res.success) throw new Error(res.message || `卸载失败: ${ext.name}`);
      showToast?.(`已卸载扩展: ${ext.name}`, "success");
      await loadData();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      localError = msg;
      showToast?.(msg, "error");
    } finally {
      busyKey = null;
    }
  }

  function piExtensionDescription(key: string): string {
    return t(key as Parameters<typeof t>[0]);
  }
</script>

<div class="space-y-6">
  <!-- 1. 扩展共享目录与自定义安装 -->
  <section class="rounded-2xl border border-border/70 bg-card/70 p-5 shadow-sm">
    <div class="flex flex-wrap items-start justify-between gap-4">
      <div>
        <div class="flex items-center gap-2">
          <span class="h-2.5 w-2.5 rounded-full bg-violet-500"></span>
          <h2 class="text-sm font-semibold text-foreground">Pi 扩展管理 (Pi Extensions)</h2>
        </div>
        <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
          Pi 扩展作为 Pi Agent 运行时的底座扩展，安装在全局共享目录中，支持 npm 包、Git
          仓库或本地扩展路径。
        </p>
      </div>
      <div class="flex flex-col items-end gap-1 text-[10px] text-muted-foreground">
        <code class="rounded-md bg-background/80 px-2 py-1 text-violet-700 dark:text-violet-300">
          ~/.agentcabin/pi
        </code>
        <span>{installedExtensions.length} 个已安装扩展</span>
      </div>
    </div>

    <!-- 安装输入框 -->
    <form
      class="mt-4 flex flex-col gap-2 sm:flex-row"
      onsubmit={(e) => {
        e.preventDefault();
        void handleInstall(customSource);
      }}
    >
      <input
        bind:value={customSource}
        class="min-h-10 min-w-0 flex-1 rounded-lg border border-border bg-background px-3 font-mono text-xs text-foreground outline-none placeholder:text-muted-foreground focus:border-primary focus:ring-2 focus:ring-primary/15"
        placeholder="npm:@scope/package 或 git:github.com/org/repo.git 或 本地绝对路径"
        aria-label="Pi 共享扩展来源"
      />
      <button
        type="submit"
        class="min-h-10 rounded-lg bg-primary px-4 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
        disabled={!customSource.trim() || Boolean(busyKey)}
      >
        {installingSource === customSource.trim() ? "安装中…" : "安装扩展"}
      </button>
    </form>

    <p class="mt-2.5 text-[11px] leading-relaxed text-muted-foreground flex items-center gap-1.5">
      <span>💡</span>
      <span
        >提示：权限系统、规划 (Plan)、目标 (Goal)、待办 (Todo)、子代理 (Subagents)、代码智能 (LSP)
        与上下文剪枝已全量系统内置并自动加载，无需且请勿重复安装。</span
      >
    </p>

    {#if localError}
      <div
        class="mt-3 rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
      >
        {localError}
      </div>
    {/if}
  </section>

  <!-- 2. 已安装扩展列表 -->
  <section class="rounded-2xl border border-border/70 bg-card/70 p-5 shadow-sm">
    <div class="flex items-center justify-between gap-3">
      <div>
        <h3 class="text-sm font-semibold text-foreground">已安装扩展 (Installed)</h3>
        <p class="mt-0.5 text-xs text-muted-foreground">
          全局共享安装的 Pi 插件；可在右侧直接更新或卸载。
        </p>
      </div>
      <span class="rounded-full bg-muted px-2.5 py-0.5 text-xs text-muted-foreground">
        {installedExtensions.length} 项
      </span>
    </div>

    {#if installedExtensions.length === 0}
      <div class="mt-4 rounded-xl border border-dashed border-border/70 px-4 py-8 text-center">
        <div class="text-xs font-medium text-foreground">尚未安装任何 Pi 扩展</div>
        <div class="mt-1 text-[11px] leading-5 text-muted-foreground">
          可以在上方输入扩展来源安装，或在下方推荐扩展中一键获取。
        </div>
      </div>
    {:else}
      <div class="mt-4 space-y-2.5">
        {#each installedExtensions as item (item.pluginId || item.name)}
          {@const id = item.pluginId || item.name}
          <article
            class="flex flex-col gap-3 rounded-xl border border-border/60 bg-background/50 px-4 py-3.5 sm:flex-row sm:items-center sm:justify-between"
          >
            <div class="min-w-0">
              <div class="flex flex-wrap items-center gap-2">
                <span class="truncate text-sm font-medium text-foreground">{item.name}</span>
                <span
                  class="rounded-full bg-violet-500/10 px-1.5 py-0.5 text-[10px] text-violet-700 dark:text-violet-300"
                >
                  Pi 扩展
                </span>
                {#if item.version}
                  <span
                    class="rounded-full bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
                  >
                    v{item.version}
                  </span>
                {/if}
              </div>
              {#if item.description}
                <p class="mt-1 truncate text-[11px] leading-4 text-muted-foreground">
                  {item.description}
                </p>
              {/if}
              <p
                class="mt-1 truncate font-mono text-[10px] text-muted-foreground"
                title={item.path || item.name}
              >
                {item.path || item.name}
              </p>
            </div>
            <div class="flex shrink-0 items-center gap-2">
              <button
                type="button"
                class="min-h-8 rounded-lg border border-border px-3 text-[11px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50"
                onclick={() => void handleUpdate(item)}
                disabled={busyKey === `update:${id}`}
              >
                {busyKey === `update:${id}` ? "更新中…" : "更新"}
              </button>
              <button
                type="button"
                class="min-h-8 rounded-lg border border-destructive/30 px-3 text-[11px] text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
                onclick={() => void handleUninstall(item)}
                disabled={busyKey === `uninstall:${id}`}
              >
                {busyKey === `uninstall:${id}` ? "卸载中…" : "卸载"}
              </button>
            </div>
          </article>
        {/each}
      </div>
    {/if}
  </section>

  <!-- 3. 推荐扩展市场 -->
  <section class="rounded-2xl border border-border/70 bg-card/70 p-5 shadow-sm space-y-4">
    <div class="flex items-center justify-between gap-3">
      <div>
        <h3 class="text-sm font-semibold text-foreground">{t("piExt_recommendedSection")}</h3>
        <p class="mt-0.5 text-xs text-muted-foreground">
          精选的 Pi Agent 常用增强包，支持一键安装到全局运行时。
        </p>
      </div>
      <span class="text-[11px] text-muted-foreground">官方 & 社区精选</span>
    </div>

    <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
      {#each RECOMMENDED_PI_EXTENSIONS as ext}
        {@const installed = getInstalledExtension(ext.name)}
        <div
          class="flex flex-col justify-between gap-3 rounded-xl border border-border/60 bg-background/50 p-4"
        >
          <div>
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <h4 class="truncate text-sm font-medium text-foreground">{ext.name}</h4>
                <span
                  class="mt-1 inline-flex rounded-full px-1.5 py-0.5 text-[10px] {ext.badgeColor}"
                >
                  {ext.badge}
                </span>
              </div>
              <span
                class="shrink-0 rounded-full px-2 py-0.5 text-[10px] {installed
                  ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                  : 'bg-muted text-muted-foreground'}"
              >
                {installed ? t("piExt_installed") : "未安装"}
              </span>
            </div>
            <p class="mt-2 text-xs leading-5 text-muted-foreground">
              {piExtensionDescription(ext.descKey)}
            </p>
          </div>

          <div class="mt-2 flex items-center justify-between gap-2 border-t border-border/40 pt-3">
            <div class="flex flex-wrap gap-2">
              {#each ext.links as link}
                <a
                  href={link.url}
                  target="_blank"
                  rel="noreferrer"
                  class="text-[10px] text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
                >
                  {link.label}
                </a>
              {/each}
            </div>
            <button
              type="button"
              class="rounded-md px-3 py-1.5 text-[11px] font-medium transition-colors disabled:opacity-50 {installed
                ? 'border border-border text-muted-foreground'
                : 'bg-primary text-primary-foreground hover:bg-primary/90'}"
              onclick={() => void handleInstall(ext.pkgSource)}
              disabled={Boolean(installed) || installingSource === ext.pkgSource}
            >
              {installingSource === ext.pkgSource
                ? "安装中…"
                : installed
                  ? t("piExt_installed")
                  : t("piExt_install")}
            </button>
          </div>
        </div>
      {/each}
    </div>
  </section>

  <!-- 4. Native Pi 特性加载 -->
  <section class="rounded-2xl border border-border/70 bg-card/70 p-5 shadow-sm space-y-4">
    <div class="flex flex-col gap-1.5">
      <div class="flex flex-wrap items-center gap-2">
        <h3 class="text-sm font-semibold text-foreground">Native Pi 原生能力（Code 模式）</h3>
        <span
          class="inline-flex items-center rounded-full bg-blue-500/10 px-2 py-0.5 text-[10px] font-medium text-blue-600 dark:text-blue-400"
        >
          自动参数注入
        </span>
        <span
          class="inline-flex items-center rounded-full bg-purple-500/10 px-2 py-0.5 text-[10px] font-medium text-purple-600 dark:text-purple-400"
        >
          Code 专属
        </span>
        <span
          class="inline-flex items-center rounded-full bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-400"
        >
          系统内置常驻
        </span>
      </div>
      <p class="text-xs text-muted-foreground leading-relaxed">
        这 9 项是官方/社区标准 npm 原生扩展，作为 AgentCabin 基础能力由系统接管。其中<strong
          >代码智能 (LSP) 默认禁用以保证会话极速启动</strong
        >，支持按需开启；其余核心能力系统内置常驻。（Work 模式由 AgentCabin 自研沙箱与适配器接管）
      </p>
    </div>

    <!-- 防冲突警示卡片 -->
    <div
      class="flex items-start gap-2.5 rounded-xl border border-amber-500/30 bg-amber-500/10 p-3.5 text-xs leading-relaxed text-amber-800 dark:text-amber-200"
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="h-4 w-4 shrink-0 text-amber-600 dark:text-amber-400 mt-0.5"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z" />
        <line x1="12" y1="9" x2="12" y2="13" />
        <line x1="12" y1="17" x2="12.01" y2="17" />
      </svg>
      <div>
        <span class="font-semibold text-amber-900 dark:text-amber-100">防冲突提醒：</span>
        上述 9 项能力为 AgentCabin 底层内置常驻，<strong
          >请勿在上方输入框或终端通过 <code
            class="rounded bg-amber-500/15 px-1 py-0.5 font-mono text-[11px]">pi install</code
          > 重复安装同类扩展</strong
        >（如
        <code class="rounded bg-amber-500/15 px-1 font-mono text-[10px]"
          >@gotgenes/pi-permission-system</code
        >、<code class="rounded bg-amber-500/15 px-1 font-mono text-[10px]"
          >@narumitw/pi-plan-mode</code
        > 等）。重复安装会导致 Pi 运行时加载两份相同包，引起工具冲突、指令覆盖或启动异常。
      </div>
    </div>

    <div class="grid gap-2.5 sm:grid-cols-2 lg:grid-cols-3">
      {#each PI_NATIVE_FEATURES as feature}
        <div
          class="flex items-center justify-between gap-3 rounded-xl border border-border/60 bg-background/50 p-3"
        >
          <div class="min-w-0 flex-1">
            <span class="block text-xs font-medium text-foreground">{feature.label}</span>
            <span class="block truncate text-[10px] text-muted-foreground mt-0.5">
              {feature.description}
            </span>
            <span
              class="inline-block font-mono text-[9px] text-muted-foreground/80 mt-1 bg-muted/60 rounded px-1.5 py-0.5"
            >
              {feature.pkg}
            </span>
          </div>
          {#if feature.pkg === "npm:@narumitw/pi-lsp"}
            <div class="shrink-0 flex items-center">
              {#if piLspEnabled}
                <button
                  type="button"
                  class="inline-flex items-center gap-1.5 rounded-md border border-emerald-500/35 bg-emerald-500/10 px-2.5 py-1 text-[11px] font-medium text-emerald-600 transition-colors hover:bg-emerald-500/20 dark:text-emerald-400 disabled:opacity-50 cursor-pointer"
                  onclick={handleToggleLsp}
                  disabled={togglingLsp}
                  title="已启用（点击禁用）"
                >
                  <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="h-3 w-3"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2.5"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <polyline points="20 6 9 17 4 12" />
                  </svg>
                  已启用
                </button>
              {:else}
                <button
                  type="button"
                  class="inline-flex items-center gap-1 rounded-md border border-border bg-muted/40 px-2.5 py-1 text-[11px] font-medium text-muted-foreground transition-colors hover:border-primary/50 hover:bg-background hover:text-foreground disabled:opacity-50 cursor-pointer"
                  onclick={handleToggleLsp}
                  disabled={togglingLsp}
                  title="默认禁用以避免启动变慢（点击启用）"
                >
                  未启用
                </button>
              {/if}
            </div>
          {:else}
            <span
              class="shrink-0 inline-flex items-center gap-1 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-2 py-1 text-[11px] font-medium text-emerald-600 dark:text-emerald-400 select-none"
              title="系统底层内置能力，Code 运行时默认常驻生效"
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-3 w-3"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <polyline points="20 6 9 17 4 12" />
              </svg>
              系统内置
            </span>
          {/if}
        </div>
      {/each}
    </div>
  </section>

  <section class="rounded-2xl border border-border/70 bg-card/70 p-5 shadow-sm space-y-4">
    <div class="flex flex-col gap-1.5">
      <div class="flex flex-wrap items-center gap-2">
        <h3 class="text-sm font-semibold text-foreground">AgentCabin 原生能力（Work 模式）</h3>
        <span
          class="inline-flex items-center rounded-full bg-purple-500/10 px-2 py-0.5 text-[10px] font-medium text-purple-600 dark:text-purple-400"
        >
          Work 专属运行时
        </span>
        <span
          class="inline-flex items-center rounded-full bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-400"
        >
          沙箱与能力显式隔离
        </span>
      </div>
      <p class="text-xs text-muted-foreground leading-relaxed">
        Work 使用独立 Pi profile，由 AgentCabin Work Runtime
        注入运行时、交互、任务跟踪和委派能力；浏览器与 MCP 适配器在对应能力启用时加载。Code
        专属的权限扩展、Plan、Goal、LSP、Context Prune 和 Multi Edit 不会进入 Work。
      </p>
    </div>

    <div class="grid gap-2.5 sm:grid-cols-2 lg:grid-cols-3">
      {#each WORK_NATIVE_FEATURES as feature}
        <div
          class="flex items-center justify-between gap-3 rounded-xl border border-border/60 bg-background/50 p-3"
        >
          <div class="min-w-0 flex-1">
            <span class="block text-xs font-medium text-foreground">{feature.label}</span>
            <span class="mt-0.5 block truncate text-[10px] text-muted-foreground">
              {feature.description}
            </span>
            <span
              class="mt-1 inline-block max-w-full truncate rounded bg-muted/60 px-1.5 py-0.5 font-mono text-[9px] text-muted-foreground/80"
            >
              {feature.pkg}
            </span>
          </div>
          <span
            class="shrink-0 inline-flex items-center gap-1 rounded-md border px-2 py-1 text-[11px] font-medium {feature.resident
              ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
              : 'border-border bg-muted/40 text-muted-foreground'}"
          >
            {feature.resident ? "系统内置" : "按需加载"}
          </span>
        </div>
      {/each}
    </div>
  </section>

  <!-- 启用 LSP 确认弹窗 -->
  <Modal bind:open={showLspConfirmModal} title="启用代码智能 (LSP / pi-lsp)">
    <div class="space-y-4 p-5 max-w-md">
      <div
        class="flex items-start gap-3 rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-xs leading-relaxed text-amber-800 dark:text-amber-200"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="h-5 w-5 shrink-0 text-amber-600 dark:text-amber-400 mt-0.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z" />
          <line x1="12" y1="9" x2="12" y2="13" />
          <line x1="12" y1="17" x2="12.01" y2="17" />
        </svg>
        <div>
          <p class="font-semibold text-amber-900 dark:text-amber-100">启动耗时与性能影响提示</p>
          <p class="mt-1 text-[11px] text-amber-800/90 dark:text-amber-200/90">
            代码智能 (LSP) 会在会话初次启动时，自动扫描当前工程并拉起对应语言的 Language Server（如
            TypeScript Server、Rust Analyzer、Pyright 等）建立代码索引。
          </p>
        </div>
      </div>

      <div class="space-y-2 text-xs text-muted-foreground leading-relaxed">
        <p>
          ⚠️ <strong>开启后可能产生的影响</strong>：
        </p>
        <ul class="list-disc pl-4 space-y-1 text-[11px]">
          <li>
            <strong>首轮响应较慢</strong>：会产生额外的冷启动和工程扫描耗时（第一对话需要等待 10~20
            秒才有首条回复）。
          </li>
          <li>
            <strong>系统开销增加</strong>：后台将持续常驻各语言的语言服务器进程，消耗额外内存。
          </li>
        </ul>
        <p class="text-[11px] pt-1">
          💡 <strong>建议</strong>：大多数日常代码编写、功能修改与问答无需开启
          LSP。仅在大型项目需要编译器级跨文件精准跳转与重构时建议开启。
        </p>
      </div>

      <div class="flex justify-end gap-2.5 pt-3 border-t border-border/60">
        <button
          type="button"
          class="rounded-lg border border-border px-3.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground cursor-pointer"
          onclick={() => (showLspConfirmModal = false)}
          disabled={togglingLsp}
        >
          取消
        </button>
        <button
          type="button"
          class="rounded-lg bg-primary px-4 py-1.5 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50 cursor-pointer"
          onclick={() => void applyLspSetting(true)}
          disabled={togglingLsp}
        >
          {togglingLsp ? "保存中…" : "确认启用"}
        </button>
      </div>
    </div>
  </Modal>
</div>
