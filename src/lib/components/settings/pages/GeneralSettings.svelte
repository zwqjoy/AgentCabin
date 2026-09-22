<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import { platform } from "$lib/platform";
  import { getTransport } from "$lib/transport";
  import { t, LOCALE_REGISTRY, currentLocale, switchLocale } from "$lib/i18n/index.svelte";
  import type { UserSettings, RemoteHost, RemoteTestResult } from "$lib/types";
  import SettingsCard from "../SettingsCard.svelte";
  import SettingsRow from "../SettingsRow.svelte";
  import SettingsToggleRow from "../SettingsToggleRow.svelte";
  import SettingsSelectRow from "../SettingsSelectRow.svelte";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsStatusBadge from "../SettingsStatusBadge.svelte";
  import {
    isDebugMode,
    setDebugMode,
    copyDebugLogs,
    getDebugLogCount,
    clearDebugLogs,
    dbg,
    dbgWarn,
  } from "$lib/utils/debug";

  interface Props {
    settings: UserSettings;
    appVersion?: string;
    onUpdateSettings: (patch: Partial<UserSettings>) => Promise<void>;
    remoteHosts?: RemoteHost[];
    onSaveRemoteHost?: (host: RemoteHost) => Promise<void>;
    onDeleteRemoteHost?: (name: string) => Promise<void>;
  }

  let {
    settings,
    appVersion = "",
    onUpdateSettings,
    remoteHosts = [],
    onSaveRemoteHost = async () => {},
    onDeleteRemoteHost = async () => {},
  }: Props = $props();

  // Working directory picker state & handlers
  let pickingDirectory = $state(false);

  async function handlePickWorkingDirectory() {
    if (!getTransport().isDesktop()) return;
    try {
      pickingDirectory = true;
      const selected = await platform.dialog.open({
        directory: true,
        multiple: false,
        title: "选择默认工作空间目录",
        defaultPath: settings.working_directory || undefined,
      });
      if (typeof selected === "string" && selected.trim()) {
        await onUpdateSettings({ working_directory: selected.trim() });
      }
    } catch (e) {
      dbgWarn("settings", "pick working_directory failed", e);
    } finally {
      pickingDirectory = false;
    }
  }

  async function handleResetWorkingDirectory() {
    await onUpdateSettings({ working_directory: "" });
  }

  // Debug state
  let debugActive = $state(isDebugMode());
  let debugLogCount = $state(getDebugLogCount());
  let debugCopied = $state(false);

  function toggleDebugMode(checked: boolean) {
    debugActive = checked;
    setDebugMode(checked);
    debugLogCount = getDebugLogCount();
  }

  async function handleCopyLogs() {
    await copyDebugLogs();
    debugCopied = true;
    setTimeout(() => (debugCopied = false), 1500);
  }

  function handleClearLogs() {
    clearDebugLogs();
    debugLogCount = getDebugLogCount();
  }

  // Remote host form state
  let showRemoteForm = $state(false);
  let editingRemoteName = $state<string | null>(null);
  let remoteName = $state("");
  let remoteHost = $state("");
  let remoteUser = $state("");
  let remotePort = $state(22);
  let remoteKeyPath = $state("");
  let remoteTesting = $state(false);
  let remoteTestResult = $state<RemoteTestResult | null>(null);

  function openNewRemoteForm() {
    editingRemoteName = null;
    remoteName = "";
    remoteHost = "";
    remoteUser = "";
    remotePort = 22;
    remoteKeyPath = "";
    remoteTestResult = null;
    showRemoteForm = true;
  }

  function editRemote(host: RemoteHost) {
    editingRemoteName = host.name;
    remoteName = host.name;
    remoteHost = host.host;
    remoteUser = host.user;
    remotePort = host.port;
    remoteKeyPath = host.key_path ?? "";
    remoteTestResult = null;
    showRemoteForm = true;
  }

  async function submitRemoteHost() {
    if (!remoteName.trim() || !remoteHost.trim() || !remoteUser.trim()) return;
    const item: RemoteHost = {
      name: remoteName.trim(),
      host: remoteHost.trim(),
      user: remoteUser.trim(),
      port: remotePort || 22,
      key_path: remoteKeyPath.trim() || undefined,
      forward_api_key: false,
    };
    await onSaveRemoteHost(item);
    showRemoteForm = false;
  }

  async function testRemote() {
    if (!remoteHost.trim() || !remoteUser.trim()) return;
    remoteTesting = true;
    remoteTestResult = null;
    try {
      remoteTestResult = await api.testRemoteHost(
        remoteHost.trim(),
        remoteUser.trim(),
        remotePort || 22,
        remoteKeyPath.trim() || undefined,
      );
    } catch (e) {
      remoteTestResult = { ssh_ok: false, cli_found: false, error: String(e) };
    } finally {
      remoteTesting = false;
    }
  }

  onMount(() => {
    debugLogCount = getDebugLogCount();
  });
</script>

<div class="space-y-8">
  <!-- 1. 关于与应用信息 -->
  <SettingsSection
    title="关于与应用信息"
    description="查看当前客户端核心版本与本地配置文件存储位置"
  >
    <SettingsCard>
      <SettingsRow
        title="AgentCabin"
        description="版本 v{appVersion || '0.1.0'} {getTransport().isDesktop()
          ? '· 桌面原生版'
          : '· 网页版'}"
      >
        {#snippet icon()}
          <img src="/logo.png?v=2" alt="AgentCabin Logo" class="h-8 w-8 rounded-lg shadow-2xs" />
        {/snippet}
        <SettingsStatusBadge status="ok" text="运行正常" />
      </SettingsRow>

      <SettingsRow title="应用配置文件" description="系统核心参数存储在本地 JSON 规范文件中">
        <code
          class="rounded-md border border-border/60 bg-muted/40 px-2 py-1 font-mono text-[11px] text-foreground"
        >
          ~/.agentcabin/settings.json
        </code>
      </SettingsRow>

      <SettingsRow title="能力中心存储根目录" description="用于存放 Skills、MCP 运行时及插件扩展">
        <code
          class="rounded-md border border-border/60 bg-muted/40 px-2 py-1 font-mono text-[11px] text-foreground"
        >
          ~/.agentcabin
        </code>
      </SettingsRow>
    </SettingsCard>
  </SettingsSection>

  <!-- 2. 常规偏好 -->
  <SettingsSection title="常规偏好" description="界面语言、工作区默认位置与系统集成选项">
    <SettingsCard>
      <SettingsSelectRow
        id="language-select"
        title="界面语言 (Language)"
        description="选择应用显示语言，切换后立即生效"
        value={currentLocale()}
        options={LOCALE_REGISTRY.map((loc) => ({
          value: loc.code,
          label: loc.nativeName,
        }))}
        onchange={(val) => switchLocale(val as any)}
      />

      <SettingsRow
        id="workspace-cwd"
        title="默认工作空间目录"
        description={settings.working_directory || "未指定（默认使用用户的主目录）"}
      >
        <div class="flex items-center gap-2">
          {#if settings.working_directory}
            <button
              type="button"
              class="rounded-lg border border-border/70 bg-background/80 px-2.5 py-1 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
              onclick={handleResetWorkingDirectory}
            >
              重置
            </button>
          {/if}
          {#if getTransport().isDesktop()}
            <button
              type="button"
              class="rounded-lg border border-border/70 bg-background/80 px-2.5 py-1 text-xs font-medium text-foreground hover:bg-accent transition-colors"
              disabled={pickingDirectory}
              onclick={handlePickWorkingDirectory}
            >
              {pickingDirectory
                ? "选择中..."
                : settings.working_directory
                  ? "更改目录"
                  : "选择目录"}
            </button>
          {/if}
        </div>
      </SettingsRow>
    </SettingsCard>
  </SettingsSection>

  <!-- 5. 远程连接 (Remote Hosts & SSH) -->
  <SettingsSection
    title="远程主机与 SSH"
    description="配置远程开发机连接信息，支持通过 SSH 隧道挂载远程运行时"
  >
    {#snippet action()}
      <button
        type="button"
        class="flex items-center gap-1 rounded-lg border border-border/70 bg-background/80 px-2.5 py-1 text-xs font-medium text-foreground hover:bg-accent/60 transition-colors"
        onclick={openNewRemoteForm}
      >
        + 添加主机
      </button>
    {/snippet}

    {#if showRemoteForm}
      <div class="rounded-xl border border-primary/30 bg-primary/[0.02] p-4 space-y-4">
        <div class="flex items-center justify-between">
          <h4 class="text-xs font-semibold text-foreground">
            {editingRemoteName ? "编辑远程主机" : "添加新远程主机"}
          </h4>
          <button
            type="button"
            class="text-xs text-muted-foreground hover:text-foreground"
            onclick={() => (showRemoteForm = false)}
          >
            取消
          </button>
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs">
          <div>
            <label class="block text-[11px] text-muted-foreground mb-1">名称 (Alias)</label>
            <input
              type="text"
              class="h-8 w-full rounded-lg border border-border/70 bg-background px-2.5 text-xs text-foreground focus:border-primary focus:outline-none"
              placeholder="e.g. dev-server"
              bind:value={remoteName}
            />
          </div>
          <div>
            <label class="block text-[11px] text-muted-foreground mb-1">主机地址 (IP / Host)</label>
            <input
              type="text"
              class="h-8 w-full rounded-lg border border-border/70 bg-background px-2.5 text-xs text-foreground focus:border-primary focus:outline-none"
              placeholder="192.168.1.100 或 dev.domain.com"
              bind:value={remoteHost}
            />
          </div>
          <div>
            <label class="block text-[11px] text-muted-foreground mb-1">用户名 (User)</label>
            <input
              type="text"
              class="h-8 w-full rounded-lg border border-border/70 bg-background px-2.5 text-xs text-foreground focus:border-primary focus:outline-none"
              placeholder="ubuntu / root"
              bind:value={remoteUser}
            />
          </div>
          <div>
            <label class="block text-[11px] text-muted-foreground mb-1">SSH 端口</label>
            <input
              type="number"
              class="h-8 w-full rounded-lg border border-border/70 bg-background px-2.5 text-xs text-foreground focus:border-primary focus:outline-none"
              bind:value={remotePort}
            />
          </div>
          <div class="sm:col-span-2">
            <label class="block text-[11px] text-muted-foreground mb-1">SSH 私钥路径 (可选)</label>
            <input
              type="text"
              class="h-8 w-full rounded-lg border border-border/70 bg-background px-2.5 text-xs text-foreground focus:border-primary focus:outline-none"
              placeholder="~/.ssh/id_ed25519"
              bind:value={remoteKeyPath}
            />
          </div>
        </div>

        {#if remoteTestResult}
          <div
            class="rounded-lg p-2.5 text-xs {remoteTestResult.ssh_ok
              ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
              : 'bg-rose-500/10 text-rose-600 dark:text-rose-400'}"
          >
            {remoteTestResult.ssh_ok
              ? "连通性测试成功！"
              : `连接失败: ${remoteTestResult.error || "未知错误"}`}
          </div>
        {/if}

        <div class="flex items-center justify-end gap-2 pt-1">
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-1.5 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-foreground transition-colors disabled:opacity-50"
            disabled={remoteTesting}
            onclick={testRemote}
          >
            {remoteTesting ? "测试中..." : "测试连接"}
          </button>
          <button
            type="button"
            class="rounded-lg bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:opacity-90 transition-opacity"
            onclick={submitRemoteHost}
          >
            保存配置
          </button>
        </div>
      </div>
    {/if}

    <SettingsCard>
      {#if remoteHosts.length === 0}
        <div class="px-5 py-6 text-center text-xs text-muted-foreground">
          暂未配置远程主机。点击右上角 "+ 添加主机" 开始配置。
        </div>
      {:else}
        {#each remoteHosts as host (host.name)}
          <SettingsRow title={host.name} description="{host.user}@{host.host}:{host.port}">
            <div class="flex items-center gap-2">
              <button
                type="button"
                class="rounded-md border border-border/70 px-2 py-1 text-xs text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
                onclick={() => editRemote(host)}
              >
                编辑
              </button>
              <button
                type="button"
                class="rounded-md border border-border/70 px-2 py-1 text-xs text-rose-500 hover:bg-rose-500/10 transition-colors"
                onclick={() => onDeleteRemoteHost(host.name)}
              >
                删除
              </button>
            </div>
          </SettingsRow>
        {/each}
      {/if}
    </SettingsCard>
  </SettingsSection>

  <!-- 6. 调试与日志 -->
  <SettingsSection title="开发者与调试" description="启用详细日志跟踪与诊断数据导出">
    <SettingsCard>
      <SettingsToggleRow
        id="debug-mode"
        title="开发者调试模式 (Debug Mode)"
        description="启用后将在内存中记录详细 IPC、网络与工具执行调用链路"
        checked={debugActive}
        onchange={toggleDebugMode}
      />

      <SettingsRow title="日志缓冲区" description="当前已记录 {debugLogCount} 条调试日志事件">
        <div class="flex items-center gap-2">
          <button
            type="button"
            class="rounded-lg border border-border/70 bg-background px-2.5 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
            onclick={handleCopyLogs}
          >
            {debugCopied ? "已复制！" : "复制全部日志"}
          </button>
          <button
            type="button"
            class="rounded-lg border border-border/70 bg-background px-2.5 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
            onclick={handleClearLogs}
          >
            清空
          </button>
        </div>
      </SettingsRow>
    </SettingsCard>
  </SettingsSection>
</div>
