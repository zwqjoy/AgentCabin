<script lang="ts">
  import { onMount } from "svelte";
  import Card from "$lib/components/Card.svelte";
  import Button from "$lib/components/Button.svelte";
  import { RUNTIME_PROVIDERS_CONFIG, type RuntimeProviderId } from "$lib/utils/agent-metadata";
  import {
    fetchRuntimeProviderStatus,
    isRuntimeProviderReady,
    runtimeProviderAuthLabel,
    type RuntimeProviderStatus,
  } from "$lib/utils/runtime-status";
  import type { UserSettings } from "$lib/types";

  let {
    providerId,
    providerName,
    settings = null,
    onNavigate,
  }: {
    providerId: RuntimeProviderId;
    providerName: string;
    settings?: UserSettings | null;
    onNavigate: (tab: string) => void;
  } = $props();

  let status = $state<RuntimeProviderStatus>({
    installed: true,
    authenticated: false,
  });

  const meta = $derived(RUNTIME_PROVIDERS_CONFIG[providerId]);
  const isEnabled = $derived((settings?.enabled_agents ?? ["pi"]).includes(providerId));
  const isWorkSupported = $derived(providerId === "pi" || providerId === "dsh");

  async function loadStatus() {
    try {
      status = await fetchRuntimeProviderStatus(providerId, settings);
    } catch {
      // fallback
    }
  }

  onMount(() => {
    void loadStatus();
  });
</script>

<div class="space-y-4">
  <!-- Provider 概览与全局状态 Card -->
  <Card class="p-6">
    <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
      <div class="flex items-center gap-3 min-w-0">
        <span class="h-3.5 w-3.5 rounded-full {meta?.dotClass ?? 'bg-primary'} shrink-0"></span>
        <div class="min-w-0">
          <div class="flex items-center gap-2">
            <h2 class="text-base font-bold text-foreground truncate">{providerName}</h2>
            <span
              class="rounded-md px-2 py-0.5 text-[11px] font-medium border shrink-0 {isEnabled
                ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
                : 'border-border bg-muted text-muted-foreground'}"
            >
              {isEnabled ? "已启用" : "未启用"}
            </span>
          </div>
          <div class="flex items-center gap-2 mt-1 text-xs text-muted-foreground flex-wrap">
            <span
              class="font-medium {status.installed
                ? 'text-emerald-600 dark:text-emerald-400'
                : 'text-amber-500'}"
            >
              {status.installed ? "CLI 已安装" : "未检测到 CLI"}
            </span>
            <span class="text-foreground/30">&middot;</span>
            <span
              class="font-medium {isRuntimeProviderReady(status)
                ? 'text-blue-600 dark:text-blue-400'
                : 'text-amber-500'}"
            >
              {runtimeProviderAuthLabel(status)}
            </span>
            {#if status.reason}
              <span class="text-muted-foreground text-[11px]">({status.reason})</span>
            {/if}
          </div>
        </div>
      </div>

      <div class="flex items-center gap-2 shrink-0">
        <Button variant="outline" size="sm" onclick={() => onNavigate("runtime-overview")}>
          前往 Runtime Provider 总览 &rarr;
        </Button>
      </div>
    </div>
  </Card>

  <!-- 工作载体绑定 (Harness Bindings) -->
  <Card class="p-6 space-y-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
          工作载体绑定
        </h2>
        <p class="mt-1 text-xs text-muted-foreground">
          查看 {providerName} 在 Code 与 Work 工作载体中的启用与 Profile 关联状态。
        </p>
      </div>
    </div>

    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
      <!-- Code Harness Binding -->
      <div
        class="flex flex-col justify-between p-4 rounded-xl border border-border/60 bg-muted/20 space-y-3"
      >
        <div class="flex items-start justify-between">
          <div class="flex items-center gap-2">
            <span class="h-2.5 w-2.5 rounded-full bg-teal-500"></span>
            <span class="text-sm font-semibold text-foreground">Code 工作载体</span>
          </div>
          <div class="flex items-center gap-1.5">
            <span
              class="rounded-md bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground border border-border/40"
            >
              引擎专有
            </span>
            <span
              class="rounded-md bg-teal-500/10 px-2 py-0.5 text-[10px] font-medium text-teal-600 dark:text-teal-400 border border-teal-500/20"
            >
              已支持
            </span>
          </div>
        </div>
        <div class="text-xs text-muted-foreground space-y-1 font-mono">
          <div
            class="text-[11px] text-muted-foreground/80 font-sans flex items-center justify-between"
          >
            <span>绑定 Profile：</span>
            <span class="text-[10px] font-normal text-muted-foreground/70">独立环境与扩展</span>
          </div>
          <div
            class="truncate bg-background/50 px-2 py-1 rounded border border-border/40 text-foreground"
          >
            ~/.agentcabin/profiles/code/{providerId}
          </div>
        </div>
        <div class="pt-1">
          <button
            type="button"
            class="text-xs font-medium text-primary hover:underline inline-flex items-center gap-1"
            onclick={() => onNavigate("pi-code")}
          >
            前往 Code 设置 &rarr;
          </button>
        </div>
      </div>

      <!-- Work Harness Binding -->
      <div
        class="flex flex-col justify-between p-4 rounded-xl border border-border/60 bg-muted/20 space-y-3"
      >
        <div class="flex items-start justify-between">
          <div class="flex items-center gap-2">
            <span
              class="h-2.5 w-2.5 rounded-full {isWorkSupported ? 'bg-blue-500' : 'bg-amber-500/80'}"
            ></span>
            <span class="text-sm font-semibold text-foreground">Work 工作载体</span>
          </div>
          <div class="flex items-center gap-1.5">
            {#if isWorkSupported}
              <span
                class="rounded-md bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground border border-border/40"
              >
                共享载体
              </span>
              <span
                class="rounded-md bg-blue-500/10 px-2 py-0.5 text-[10px] font-medium text-blue-600 dark:text-blue-400 border border-blue-500/20"
              >
                已支持
              </span>
            {:else}
              <span
                class="rounded-md bg-amber-500/10 px-2 py-0.5 text-[10px] font-medium text-amber-600 dark:text-amber-400 border border-amber-500/20"
              >
                规划中 (fail-closed)
              </span>
            {/if}
          </div>
        </div>
        <div class="text-xs text-muted-foreground space-y-1 font-mono">
          <div
            class="text-[11px] text-muted-foreground/80 font-sans flex items-center justify-between"
          >
            <span>绑定 Profile：</span>
            <span class="text-[10px] font-normal text-muted-foreground/70">跨 Runtime 共享</span>
          </div>
          <div
            class="truncate bg-background/50 px-2 py-1 rounded border border-border/40 text-foreground"
          >
            {isWorkSupported ? "~/.agentcabin/profiles/work" : "尚未接入 (严格防静默降级)"}
          </div>
        </div>
        <div class="pt-1">
          {#if isWorkSupported}
            <button
              type="button"
              class="text-xs font-medium text-primary hover:underline inline-flex items-center gap-1"
              onclick={() => onNavigate("pi-work")}
            >
              前往 Work 设置 &rarr;
            </button>
          {:else}
            <span class="text-[11px] text-muted-foreground"> 当前版本仅 Code 模式开放接入 </span>
          {/if}
        </div>
      </div>
    </div>
    <p class="text-[11px] text-muted-foreground/80 leading-relaxed border-t border-border/40 pt-3">
      提示：Work 载体集中托管所有 Connectors、企业应用、审批策略与业务规则，各 Runtime
      共享同一套工作台配置，底层运行沙箱彼此严格隔离。
    </p>
  </Card>

  <!-- 能力来源 (Capability Source) -->
  <Card class="p-6 space-y-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
          能力来源
        </h2>
        <p class="mt-1 text-xs text-muted-foreground">
          {providerName} 的所有 Skills、MCP 与连接器配置严格由能力中心集中管控并全局启用，再动态投影到隔离运行时；绝不读取原生宿主或未托管项目目录。
        </p>
      </div>
      <Button variant="outline" size="sm" onclick={() => onNavigate("capability-center")}>
        打开能力中心
      </Button>
    </div>

    <div class="rounded-lg border border-border/50 bg-background/40 p-4 space-y-2">
      <div class="flex items-center justify-between text-xs">
        <span class="text-muted-foreground">配置根目录</span>
        <span class="font-mono font-medium text-foreground">~/.agentcabin</span>
      </div>
      <div class="flex items-center justify-between text-xs">
        <span class="text-muted-foreground">隔离运行目录</span>
        <span class="font-mono text-muted-foreground"
          >~/.agentcabin/runtime/{providerId}/&lt;run-id&gt;/</span
        >
      </div>
      <div class="flex items-center justify-between text-xs">
        <span class="text-muted-foreground">原生目录隔离</span>
        <span class="text-emerald-600 dark:text-emerald-400 font-medium"
          >已启用严格隔离 (Fail-Closed 保护)</span
        >
      </div>
    </div>
  </Card>
</div>
