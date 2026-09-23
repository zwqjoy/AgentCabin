<script lang="ts">
  import { onMount } from "svelte";
  import Card from "$lib/components/Card.svelte";
  import Button from "$lib/components/Button.svelte";
  import * as api from "$lib/api";
  import { getWorkProfile } from "$lib/api/work";
  import type { UserSettings } from "$lib/types";
  import {
    ALL_RUNTIME_PROVIDERS,
    VISIBLE_RUNTIME_PROVIDERS,
    RUNTIME_PROVIDERS_CONFIG,
    DEFAULT_ENABLED_RUNTIME_PROVIDERS,
    type RuntimeProviderId,
  } from "$lib/utils/agent-metadata";

  import {
    fetchRuntimeProviderStatus,
    isRuntimeProviderReady,
    runtimeProviderAuthLabel,
    type RuntimeProviderStatus,
  } from "$lib/utils/runtime-status";

  let {
    mode,
    settings,
    onSaveSettings,
    onNavigate,
  }: {
    mode: "code" | "work";
    settings: UserSettings | null;
    onSaveSettings?: (patch: Partial<UserSettings>) => Promise<void>;
    onNavigate: (tab: string, section?: string) => void;
  } = $props();

  // Only show providers that are globally enabled in the overview page.
  let enabledProviders = $derived.by((): RuntimeProviderId[] => {
    const enabled = settings?.enabled_agents ?? DEFAULT_ENABLED_RUNTIME_PROVIDERS;
    const enabledSet = new Set(enabled);
    enabledSet.add("pi");
    const filtered = VISIBLE_RUNTIME_PROVIDERS.filter((p) => enabledSet.has(p));
    return filtered.length > 0 ? (filtered as RuntimeProviderId[]) : ["pi"];
  });

  let selectedRuntime = $state<string>("pi");
  let providerStatuses = $state<Record<string, RuntimeProviderStatus>>({
    pi: { installed: true, authenticated: true },
    codex: { installed: true, authenticated: false },
    claude: { installed: true, authenticated: false },
    grok: { installed: true, authenticated: false },
  });

  let capabilityStats = $state<{
    skillsCount: number;
    mcpCount: number;
    connectorsCount: number;
    networkEnabled: boolean;
    browserUseEnabled: boolean;
  }>({
    skillsCount: 0,
    mcpCount: 0,
    connectorsCount: 0,
    networkEnabled: true,
    browserUseEnabled: true,
  });
  let legacyWorkRuntime = $state<string | null>(null);

  async function loadCapabilitiesSummary() {
    try {
      const [
        skills,
        mcpCatalog,
        mcpBindings,
        connectorCatalog,
        connectorBindings,
        webAccess,
        browserUse,
      ] = await Promise.all([
        api.listSkills(),
        api.listMcpCatalog().catch(() => []),
        api.getMcpBindings().catch(() => []),
        api.listConnectorCatalog().catch(() => []),
        api.getConnectorBindings().catch(() => []),
        api.getWebAccessBinding().catch(() => true),
        api.getBrowserUseBinding().catch(() => true),
      ]);
      const isEnabledByDefault = <T extends { id: string }>(
        catalog: T[],
        bindings: Array<{ serverId?: string; connectorId?: string; enabled: boolean }>,
        key: "serverId" | "connectorId",
      ) =>
        catalog.filter((item) => {
          const binding = bindings.find(
            (candidate) => candidate[key]?.trim().toLowerCase() === item.id.trim().toLowerCase(),
          );
          return binding?.enabled ?? true;
        }).length;

      capabilityStats = {
        skillsCount: (skills ?? []).filter((s) => s.enabled).length,
        mcpCount: isEnabledByDefault(mcpCatalog ?? [], mcpBindings ?? [], "serverId"),
        connectorsCount: isEnabledByDefault(
          connectorCatalog ?? [],
          connectorBindings ?? [],
          "connectorId",
        ),
        networkEnabled: webAccess,
        browserUseEnabled: browserUse,
      };
    } catch {
      // fallback
    }
  }

  async function loadProviderStatuses() {
    for (const pid of ALL_RUNTIME_PROVIDERS) {
      try {
        const st = await fetchRuntimeProviderStatus(pid, settings);
        providerStatuses[pid] = st;
      } catch {
        // keep fallback
      }
    }
  }

  $effect(() => {
    if (settings && !userSelected) {
      if (mode === "code") {
        const candidate = settings.code_default_runtime ?? settings.default_agent ?? "pi";
        selectedRuntime = (enabledProviders as readonly string[]).includes(candidate)
          ? candidate
          : (enabledProviders[0] ?? "pi");
      } else {
        selectedRuntime = settings.work_default_runtime ?? legacyWorkRuntime ?? "pi";
      }
    }
  });

  let userSelected = $state(false);

  async function handleRuntimeChange(runtimeId: string) {
    userSelected = true;
    selectedRuntime = runtimeId;
    if (onSaveSettings) {
      if (mode === "code") {
        await onSaveSettings({
          code_default_runtime: runtimeId,
        });
      } else {
        await onSaveSettings({
          work_default_runtime: runtimeId,
        });
      }
    }
  }

  onMount(() => {
    if (mode === "work") {
      void getWorkProfile()
        .then((profile) => {
          legacyWorkRuntime = profile.runtime;
        })
        .catch(() => {
          // The settings value or compatibility default remains usable.
        });
    }
    void loadCapabilitiesSummary();
    void loadProviderStatuses();
  });
</script>

<div class="space-y-5">
  <!-- Mode-Specific Notice Banner -->
  <div
    class="rounded-xl border p-4 text-xs leading-relaxed {mode === 'work'
      ? 'border-blue-500/20 bg-blue-500/5 text-blue-900 dark:text-blue-200'
      : 'border-teal-500/20 bg-teal-500/5 text-teal-900 dark:text-teal-200'}"
  >
    <div class="flex items-center gap-2 font-semibold">
      <span class="h-2 w-2 rounded-full {mode === 'work' ? 'bg-blue-500' : 'bg-teal-500'}"></span>
      <span>{mode === "code" ? "Code 工作载体 Profile" : "Work 工作载体 Profile 与沙箱防护"}</span>
    </div>
    <p class="mt-1 opacity-90">
      Skills、MCP 与 Connector
      由能力中心统一管理；网络、浏览器与电脑控制在一级工具设置中管理。当前工作载体仍保留独立的权限、审批、沙箱与工作区边界。
    </p>
  </div>

  <!-- Runtime Provider 选择卡片 -->
  <Card class="p-6 space-y-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
          {mode === "code" ? "Code" : "Work"} 默认 Runtime Provider
        </h2>
        <p class="mt-1 text-xs text-muted-foreground">
          为 {mode === "code" ? "Code" : "Work"} 工作载体指定新建会话时默认执行的 Runtime Provider。
        </p>
      </div>
    </div>

    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
      {#each enabledProviders as pid (pid)}
        {@const meta = RUNTIME_PROVIDERS_CONFIG[pid]}
        {@const isWorkUnsupported = mode === "work" && pid !== "pi"}
        {@const isSelected = selectedRuntime === pid}
        {@const status = providerStatuses[pid] ?? { installed: true, authenticated: false }}
        <button
          type="button"
          disabled={isWorkUnsupported}
          class="flex flex-col justify-between p-3.5 rounded-xl border text-left transition-all {isWorkUnsupported
            ? 'opacity-40 cursor-not-allowed border-border/30 bg-muted/5'
            : isSelected
              ? 'border-primary bg-primary/5 ring-1 ring-primary shadow-sm'
              : 'border-border/60 bg-muted/20 hover:border-border hover:bg-accent/40'}"
          onclick={() => !isWorkUnsupported && handleRuntimeChange(pid)}
        >
          <div>
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <span class="h-2.5 w-2.5 rounded-full {meta.dotClass} shrink-0"></span>
                <span class="text-xs font-semibold text-foreground">{meta.name}</span>
              </div>
              {#if isSelected}
                <span
                  class="rounded bg-primary/20 px-2 py-0.5 text-[10px] font-medium text-primary"
                >
                  当前默认
                </span>
              {:else if isWorkUnsupported}
                <span
                  class="rounded bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20 px-1.5 py-0.5 text-[10px] font-medium"
                >
                  当前 Work Runtime 尚未适配
                </span>
              {/if}
            </div>
            <p class="mt-1.5 text-[11px] text-muted-foreground line-clamp-1">
              {meta.desc}
            </p>
          </div>

          <div
            class="flex items-center justify-between gap-2 mt-3 pt-2 border-t border-border/40 text-[10px]"
          >
            <div class="flex items-center gap-1.5">
              <span
                class={status.installed
                  ? "text-emerald-600 dark:text-emerald-400 font-medium"
                  : "text-amber-500"}
              >
                {status.installed ? "已安装" : "未安装"}
              </span>
              <span class="text-foreground/30">&middot;</span>
              <span
                class={isRuntimeProviderReady(status)
                  ? "text-blue-600 dark:text-blue-400 font-medium"
                  : "text-muted-foreground"}
              >
                {runtimeProviderAuthLabel(status)}
              </span>
            </div>
            <span class="text-muted-foreground/80">
              {mode === "code"
                ? "支持 Code Profile"
                : isWorkUnsupported
                  ? "暂未适配 Work"
                  : "受管 Work 沙箱"}
            </span>
          </div>
        </button>
      {/each}
    </div>
  </Card>

  <!-- Work 专属状态卡片 (仅 Work 模式显示) -->
  {#if mode === "work"}
    <Card class="p-6 space-y-4">
      <div>
        <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
          Work 沙箱与安全执行架构
        </h2>
        <p class="mt-1 text-xs text-muted-foreground">
          Work 工作载体通过 5 大核心机制确保各 Runtime Provider 安全可靠地执行多工作区任务。
        </p>
      </div>

      <div class="grid grid-cols-1 sm:grid-cols-5 gap-2.5 text-xs">
        <div class="p-3 rounded-lg border border-border/50 bg-background/50 space-y-1">
          <div class="flex items-center gap-1.5 font-semibold text-blue-600 dark:text-blue-400">
            <span class="h-1.5 w-1.5 rounded-full bg-blue-500"></span>
            <span>Sandbox</span>
          </div>
          <p class="text-[11px] text-muted-foreground leading-tight">进程与文件系统严格沙箱隔离</p>
        </div>

        <div class="p-3 rounded-lg border border-border/50 bg-background/50 space-y-1">
          <div
            class="flex items-center gap-1.5 font-semibold text-emerald-600 dark:text-emerald-400"
          >
            <span class="h-1.5 w-1.5 rounded-full bg-emerald-500"></span>
            <span>审批机制</span>
          </div>
          <p class="text-[11px] text-muted-foreground leading-tight">敏感操作动态权限拦截与确认</p>
        </div>

        <div class="p-3 rounded-lg border border-border/50 bg-background/50 space-y-1">
          <div class="flex items-center gap-1.5 font-semibold text-purple-600 dark:text-purple-400">
            <span class="h-1.5 w-1.5 rounded-full bg-purple-500"></span>
            <span>Bridge</span>
          </div>
          <p class="text-[11px] text-muted-foreground leading-tight">
            统一 ACP / JSON-RPC 状态桥接
          </p>
        </div>

        <div class="p-3 rounded-lg border border-border/50 bg-background/50 space-y-1">
          <div class="flex items-center gap-1.5 font-semibold text-amber-600 dark:text-amber-400">
            <span class="h-1.5 w-1.5 rounded-full bg-amber-500"></span>
            <span>ToolPipeline</span>
          </div>
          <p class="text-[11px] text-muted-foreground leading-tight">
            多阶段工具调用安全审计与过滤
          </p>
        </div>

        <div class="p-3 rounded-lg border border-border/50 bg-background/50 space-y-1">
          <div class="flex items-center gap-1.5 font-semibold text-teal-600 dark:text-teal-400">
            <span class="h-1.5 w-1.5 rounded-full bg-teal-500"></span>
            <span>Artifacts</span>
          </div>
          <p class="text-[11px] text-muted-foreground leading-tight">
            按任务多工作区成果物独立隔离
          </p>
        </div>
      </div>
    </Card>
  {/if}

  <!-- 能力与 Profile 摘要卡片 -->
  <Card class="p-6 space-y-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
          {mode === "code" ? "Code" : "Work"} 能力摘要 (中央能力中心托管)
        </h2>
        <p class="mt-1 text-xs text-muted-foreground">
          Skills、MCP 与 Connector 由能力中心维护；网络、浏览器和电脑控制由独立的一级工具设置管理。
        </p>
      </div>
      <Button variant="outline" size="sm" onclick={() => onNavigate("capability-center")}>
        前往能力中心管理
      </Button>
    </div>

    <!-- 摘要网格 -->
    <div class="grid grid-cols-2 sm:grid-cols-5 gap-3">
      <!-- Skills -->
      <div
        class="flex flex-col justify-between p-3 rounded-lg border border-border/50 bg-background/50 text-center"
      >
        <span class="text-[11px] text-muted-foreground">已启用 Skills</span>
        <span class="text-lg font-bold text-foreground my-1">{capabilityStats.skillsCount}</span>
        <button
          type="button"
          class="text-[11px] text-primary hover:underline"
          onclick={() => onNavigate("capability-center", "skills")}
        >
          管理 &rarr;
        </button>
      </div>

      <!-- MCP -->
      <div
        class="flex flex-col justify-between p-3 rounded-lg border border-border/50 bg-background/50 text-center"
      >
        <span class="text-[11px] text-muted-foreground">已启用 MCP</span>
        <span class="text-lg font-bold text-foreground my-1">{capabilityStats.mcpCount}</span>
        <button
          type="button"
          class="text-[11px] text-primary hover:underline"
          onclick={() => onNavigate("capability-center", "mcp")}
        >
          管理 &rarr;
        </button>
      </div>

      <!-- Connectors -->
      <div
        class="flex flex-col justify-between p-3 rounded-lg border border-border/50 bg-background/50 text-center"
      >
        <span class="text-[11px] text-muted-foreground">已启用 Connector</span>
        <span class="text-lg font-bold text-foreground my-1">{capabilityStats.connectorsCount}</span
        >
        <button
          type="button"
          class="text-[11px] text-primary hover:underline"
          onclick={() => onNavigate("capability-center", "connectors")}
        >
          管理 &rarr;
        </button>
      </div>

      <!-- Network -->
      <div
        class="flex flex-col justify-between p-3 rounded-lg border border-border/50 bg-background/50 text-center"
      >
        <span class="text-[11px] text-muted-foreground">网络访问</span>
        <span class="text-xs font-semibold text-emerald-600 dark:text-emerald-400 my-2">
          {capabilityStats.networkEnabled ? "已启用" : "已停用"}
        </span>
        <button
          type="button"
          class="text-[11px] text-primary hover:underline"
          onclick={() => onNavigate("web-access")}
        >
          管理 &rarr;
        </button>
      </div>

      <!-- Browser -->
      <div
        class="flex flex-col justify-between p-3 rounded-lg border border-border/50 bg-background/50 text-center"
      >
        <span class="text-[11px] text-muted-foreground">受管浏览器</span>
        <span class="text-xs font-semibold text-blue-600 dark:text-blue-400 my-2">
          {capabilityStats.browserUseEnabled ? "已启用" : "已停用"}
        </span>
        <button
          type="button"
          class="text-[11px] text-primary hover:underline"
          onclick={() => onNavigate("browser-use")}
        >
          管理 &rarr;
        </button>
      </div>
    </div>
  </Card>
</div>
