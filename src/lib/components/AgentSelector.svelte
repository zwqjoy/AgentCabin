<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import * as api from "$lib/api";
  import { getAgentCapabilities } from "$lib/utils/agent-capabilities";
  import {
    ALL_RUNTIME_PROVIDERS,
    VISIBLE_RUNTIME_PROVIDERS,
    RUNTIME_PROVIDERS_CONFIG,
    getAgentDisplayName,
  } from "$lib/utils/agent-metadata";
  import {
    fetchRuntimeProviderStatus,
    isRuntimeProviderReady,
    runtimeProviderAuthLabel,
    type RuntimeProviderSettings,
    type RuntimeProviderStatus,
  } from "$lib/utils/runtime-status";

  let {
    value = $bindable("pi"),
    enabledAgents = ["pi"],
    isRemote = false,
    remoteSupported = true,
    locked = false,
    lockReason,
    compact = false,
    class: className = "",
    onchange,
  }: {
    value?: string;
    enabledAgents?: string[];
    isRemote?: boolean;
    remoteSupported?: boolean;
    locked?: boolean;
    lockReason?: string;
    compact?: boolean;
    class?: string;
    onchange?: (agent: string) => void;
  } = $props();

  let open = $state(false);
  let statuses = $state<Record<string, RuntimeProviderStatus>>({});
  let runtimeSettings = $state<RuntimeProviderSettings | null>(null);

  // Runtime Provider metadata sourced from canonical configuration
  const ALL_AGENTS = ALL_RUNTIME_PROVIDERS.map((id) => {
    const meta = RUNTIME_PROVIDERS_CONFIG[id];
    return {
      id,
      label: meta.name,
      dot: meta.dotClass,
      desc: meta.desc,
    };
  });

  const agents = $derived.by(() => {
    const enabledSet = new Set(enabledAgents ?? ["pi"]);
    enabledSet.add("pi");
    return ALL_AGENTS.filter(
      (a) =>
        enabledSet.has(a.id) && (VISIBLE_RUNTIME_PROVIDERS as readonly string[]).includes(a.id),
    );
  });

  let isCurrentKnown = $derived((ALL_RUNTIME_PROVIDERS as readonly string[]).includes(value));

  let currentAgent = $derived(
    isCurrentKnown
      ? (ALL_AGENTS.find((a) => a.id === value) ?? ALL_AGENTS[0])
      : {
          id: value,
          label: `${getAgentDisplayName(value)} (不可用)`,
          dot: "bg-red-500",
          desc: "未知或不可用的 Runtime Provider",
        },
  );

  const effectiveLockReason = $derived(
    lockReason ?? `当前会话已绑定 ${currentAgent.label}，新建会话后可切换运行时`,
  );

  async function loadStatuses() {
    runtimeSettings = await api.getUserSettings().catch(() => null);
    await Promise.all(
      ALL_RUNTIME_PROVIDERS.map(async (pid) => {
        try {
          const st = await fetchRuntimeProviderStatus(pid, runtimeSettings);
          statuses[pid] = st;
        } catch {
          // fallback
        }
      }),
    );
  }

  onMount(() => {
    void loadStatuses();
  });

  function isRemoteUnsupported(id: string): boolean {
    if (!isRemote) return false;
    return id === value ? !remoteSupported : !getAgentCapabilities(id).runtime.remote;
  }

  const tabMap: Record<string, string> = {
    pi: "pi-common",
    codex: "codex",
    claude: "claude",
    grok: "grok",
    dsh: "native-dsh",
  };

  function select(id: string) {
    if (locked) return;
    if (isRemoteUnsupported(id)) return;
    const st = statuses[id];
    if (st && !isRuntimeProviderReady(st)) {
      open = false;
      const tab = tabMap[id] ?? id;
      void goto(`/settings?tab=${tab}`);
      return;
    }
    value = id;
    open = false;
    onchange?.(id);
  }

  function handleTriggerClick() {
    if (locked) return;
    if (agents.length <= 1) return;
    open = !open;
    if (open) {
      void loadStatuses();
    }
  }

  function handleClickOutside(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (!target.closest("[data-agent-selector]")) {
      open = false;
    }
  }
</script>

<svelte:window onclick={handleClickOutside} />

<div class="relative {compact ? 'inline-flex items-center' : ''} {className}" data-agent-selector>
  <button
    class="ui-agent-selector-trigger items-center rounded-md text-xs font-medium transition-colors {compact
      ? 'inline-flex gap-1 px-1.5 py-1'
      : 'flex min-h-7 gap-1.5 px-1.5'} {locked
      ? compact
        ? 'text-muted-foreground cursor-default'
        : 'text-foreground/70 cursor-default opacity-90'
      : agents.length <= 1
        ? 'text-muted-foreground cursor-default'
        : compact
          ? 'text-muted-foreground hover:bg-accent/60 hover:text-foreground'
          : 'text-foreground/80 hover:text-foreground hover:bg-accent/70'}"
    onclick={handleTriggerClick}
    disabled={locked ? false : agents.length <= 1}
    aria-label={locked ? `${currentAgent.label} (已锁定)` : "选择 Runtime Provider"}
    aria-expanded={open}
    aria-haspopup="menu"
    title={locked
      ? effectiveLockReason
      : agents.length <= 1
        ? `${currentAgent.label} (当前环境仅支持此运行时)`
        : "选择 Runtime Provider"}
  >
    <span class="h-1.5 w-1.5 rounded-full {currentAgent.dot} shrink-0"></span>
    {#if !compact}
      <span class="text-muted-foreground font-normal">运行时:</span>
    {/if}
    <span class="truncate font-medium">{currentAgent.label}</span>
    {#if !locked && agents.length > 1}
      <svg
        class="h-2.5 w-2.5 opacity-50 shrink-0"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"><path d="m6 9 6 6 6-6" /></svg
      >
    {/if}
  </button>

  {#if open && !locked && agents.length > 1}
    <div
      data-composer-menu
      class="absolute bottom-full left-0 mb-1.5 w-[280px] rounded-xl border border-border/60 bg-popover p-1.5 shadow-lg animate-fade-in z-50 space-y-1"
    >
      <div
        class="px-2 py-1 text-[10px] font-semibold text-muted-foreground uppercase tracking-wider"
      >
        切换运行时 Provider
      </div>
      {#each agents as agent (agent.id)}
        {@const disabled = isRemoteUnsupported(agent.id)}
        {@const st = statuses[agent.id]}
        {@const isUninstalled = st && !st.installed}
        {@const isReady = !st || isRuntimeProviderReady(st)}
        {@const isManaged = st?.authSource === "agentcabin"}
        {@const isManagedReady = Boolean(isManaged && isReady)}
        {@const isUnauthenticated = st && st.installed && !isReady && !isManaged}
        {@const isManagedUnavailable = st && st.installed && !isReady && isManaged}
        {@const isUnavailable = isUninstalled || isUnauthenticated || isManagedUnavailable}
        <button
          class="flex w-full items-start gap-2.5 p-2 rounded-lg text-xs text-foreground/80 hover:bg-accent/60 hover:text-foreground transition-colors group text-left {value ===
          agent.id
            ? 'bg-accent/40 font-medium'
            : ''} {disabled ? 'opacity-50 cursor-not-allowed hover:bg-transparent' : ''}"
          {disabled}
          title={disabled
            ? `${agent.label} 暂不支持 SSH 远程主机`
            : isUninstalled
              ? `${agent.label} 未检测到 CLI，点击前往配置`
              : isManagedUnavailable
                ? `${agent.label} AgentCabin 配置不完整，点击前往设置`
                : isUnauthenticated
                  ? `${agent.label} 待认证，点击前往设置`
                  : agent.desc}
          onclick={() => select(agent.id)}
        >
          <span class="h-2 w-2 rounded-full {agent.dot} shrink-0 mt-1"></span>
          <div class="flex-1 min-w-0">
            <div class="flex items-center justify-between gap-1.5">
              <div class="flex items-center gap-1.5 min-w-0">
                <span class="font-medium text-foreground truncate">{agent.label}</span>
                {#if isManagedReady}
                  <span
                    class="text-[10px] text-emerald-600 dark:text-emerald-400 bg-emerald-500/10 px-1 py-0.2 rounded font-normal shrink-0"
                  >
                    AgentCabin 已配置
                  </span>
                {:else if isUninstalled}
                  <span
                    class="text-[10px] text-amber-600 dark:text-amber-400 bg-amber-500/10 px-1 py-0.2 rounded font-normal shrink-0"
                  >
                    未安装
                  </span>
                {:else if isManagedUnavailable}
                  <span
                    class="text-[10px] text-amber-600 dark:text-amber-400 bg-amber-500/10 px-1 py-0.2 rounded font-normal shrink-0"
                  >
                    需配置
                  </span>
                {:else if isUnauthenticated}
                  <span
                    class="text-[10px] text-blue-600 dark:text-blue-400 bg-blue-500/10 px-1 py-0.2 rounded font-normal shrink-0"
                  >
                    {runtimeProviderAuthLabel(st)}
                  </span>
                {/if}
                {#if disabled}
                  <span class="text-[10px] text-amber-500 font-normal shrink-0">(仅 Local)</span>
                {/if}
              </div>

              {#if value === agent.id}
                <svg
                  class="h-3.5 w-3.5 text-primary shrink-0"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"><polyline points="20 6 9 17 4 12" /></svg
                >
              {:else if isUnavailable}
                <span class="text-[10px] text-primary group-hover:underline shrink-0 font-normal">
                  设置 &rarr;
                </span>
              {/if}
            </div>
            <p class="text-[11px] text-muted-foreground line-clamp-2 mt-0.5">{agent.desc}</p>
          </div>
        </button>
      {/each}
    </div>
  {/if}
</div>
