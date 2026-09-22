<script lang="ts">
  import { onMount } from "svelte";
  import type { UserSettings } from "$lib/types";
  import {
    ALL_RUNTIME_PROVIDERS,
    VISIBLE_RUNTIME_PROVIDERS,
    RUNTIME_PROVIDERS_CONFIG,
    type RuntimeProviderId,
  } from "$lib/utils/agent-metadata";
  import {
    fetchRuntimeProviderStatus,
    runtimeProviderAuthLabel,
    isRuntimeProviderReady,
    type RuntimeProviderStatus,
  } from "$lib/utils/runtime-status";
  import SettingsCard from "../SettingsCard.svelte";
  import SettingsRow from "../SettingsRow.svelte";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsStatusBadge from "../SettingsStatusBadge.svelte";

  interface Props {
    settings: UserSettings;
    onToggleAgent: (agent: RuntimeProviderId) => void;
    onNavigate: (tab: string, section?: string) => void;
    onOpenWizard: () => void;
  }

  let { settings, onToggleAgent, onNavigate, onOpenWizard }: Props = $props();

  let loading = $state(false);
  let statuses = $state<Record<string, RuntimeProviderStatus>>({});

  async function refreshStatuses() {
    loading = true;
    try {
      const results = await Promise.all(
        ALL_RUNTIME_PROVIDERS.map(async (pid) => {
          const res = await fetchRuntimeProviderStatus(pid);
          return [pid, res] as const;
        }),
      );
      statuses = Object.fromEntries(results);
    } catch (e) {
      console.error("[Doctor] refresh failed", e);
    } finally {
      loading = false;
    }
  }

  function isEnabled(pid: string): boolean {
    const list = settings.enabled_agents ?? ["codex", "claude", "pi", "dsh", "grok"];
    return list.includes(pid);
  }

  onMount(() => {
    refreshStatuses();
  });
</script>

<div class="space-y-8">
  <div class="flex items-start justify-between gap-4">
    <div>
      <h2 class="text-sm font-semibold uppercase tracking-wider text-muted-foreground">
        CLI 引擎检测 (Doctor)
      </h2>
      <p class="mt-1 text-xs text-muted-foreground">
        深度体检本地 AI CLI 引擎、二进制可执行文件路径、认证就绪度与系统健康状态。
      </p>
    </div>
    <div class="flex items-center gap-2">
      <button
        type="button"
        class="flex items-center gap-1.5 rounded-lg border border-border/70 bg-background px-3 py-1.5 text-xs font-medium text-foreground hover:bg-accent transition-colors disabled:opacity-50"
        disabled={loading}
        onclick={refreshStatuses}
      >
        {#if loading}
          <span class="h-3 w-3 animate-spin rounded-full border border-primary border-t-transparent"
          ></span>
          <span>检测中...</span>
        {:else}
          <svg
            class="h-3.5 w-3.5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            ><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67" /></svg
          >
          <span>刷新检测</span>
        {/if}
      </button>
      <button
        type="button"
        class="rounded-lg border border-border/70 bg-background px-3 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
        onclick={onOpenWizard}
      >
        运行向导
      </button>
    </div>
  </div>

  <!-- Runtime Provider Status Cards Grid -->
  <div class="grid grid-cols-1 sm:grid-cols-2 gap-3.5">
    {#each VISIBLE_RUNTIME_PROVIDERS as pid (pid)}
      {@const meta = RUNTIME_PROVIDERS_CONFIG[pid]}
      {@const enabled = isEnabled(pid)}
      {@const status = statuses[pid] ?? { installed: false, authenticated: false }}
      {@const ready = isRuntimeProviderReady(status)}

      <div
        class="flex flex-col justify-between rounded-xl border p-4 transition-all {enabled
          ? 'border-border/80 bg-card shadow-xs'
          : 'border-border/40 bg-muted/10 opacity-70'}"
      >
        <div>
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-2.5">
              <span class="h-2.5 w-2.5 rounded-full {meta.dotClass} shrink-0"></span>
              <span class="text-sm font-semibold text-foreground tracking-tight">{meta.name}</span>
            </div>
            <div class="flex items-center gap-1.5">
              <SettingsStatusBadge
                status={status.installed ? (ready ? "ok" : "warning") : "error"}
                text={status.installed ? (ready ? "就绪" : "待配置") : "未安装"}
              />
              {#if pid === "pi"}
                <span class="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground"
                  >核心</span
                >
              {:else}
                <button
                  type="button"
                  class="rounded-md border px-2 py-0.5 text-[11px] font-medium transition-colors {enabled
                    ? 'border-emerald-500/40 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
                    : 'border-border text-muted-foreground hover:bg-accent'}"
                  onclick={() => onToggleAgent(pid)}
                >
                  {enabled ? "已启用" : "未启用"}
                </button>
              {/if}
            </div>
          </div>

          <p class="mt-2 text-xs text-muted-foreground line-clamp-2 leading-relaxed">
            {meta.desc}
          </p>
        </div>

        <div class="mt-4 pt-3 border-t border-border/40 space-y-1.5 text-xs">
          <div class="flex items-center justify-between text-[11px]">
            <span class="text-muted-foreground">认证状态</span>
            <span class="font-medium text-foreground">{runtimeProviderAuthLabel(status)}</span>
          </div>

          {#if status.reason}
            <div
              class="rounded-md bg-amber-500/10 p-2 text-[10px] text-amber-600 dark:text-amber-400 mt-2"
            >
              {status.reason}
            </div>
          {/if}

          <div class="pt-2 flex justify-end">
            <button
              type="button"
              class="text-xs font-medium text-primary hover:underline"
              onclick={() => onNavigate("code", pid)}
            >
              配置此运行时 &rarr;
            </button>
          </div>
        </div>
      </div>
    {/each}
  </div>
</div>
