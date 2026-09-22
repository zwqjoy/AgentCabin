<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import Button from "$lib/components/Button.svelte";
  import Card from "$lib/components/Card.svelte";
  import { getTransport } from "$lib/transport";
  import { platform } from "$lib/platform";

  type WebServerStatus = Awaited<ReturnType<typeof api.getWebServerStatus>>;
  type LinkKind = "local" | "tunnel" | "token";

  let status = $state<WebServerStatus | null>(null);
  let token = $state<string | null>(null);
  let portInput = $state("9476");
  let bindValue = $state("127.0.0.1");
  let origins = $state<string[]>([]);
  let originInput = $state("");
  let tunnelUrl = $state("");
  let appliedTunnelUrl = $state("");
  let lanIp = $state<string | null>(null);
  let loading = $state(true);
  let saving = $state(false);
  let regenerating = $state(false);
  let showToken = $state(false);
  let copied = $state<LinkKind | null>(null);
  let message = $state("");
  let error = $state("");

  let enabled = $state(false);
  let localAccessUrl = $derived(buildLocalAccessUrl());
  let tunnelAccessUrl = $derived(buildTunnelAccessUrl());
  let bindExposesLan = $derived(
    bindValue === "0.0.0.0" || bindValue === "::" || bindValue === "[::]",
  );

  onMount(() => {
    void load();
  });

  async function load() {
    if (!getTransport().isDesktop()) {
      error = "远程访问服务目前仅支持桌面版。";
      loading = false;
      return;
    }
    loading = true;
    error = "";
    try {
      const [settings, nextStatus, nextToken] = await Promise.all([
        api.getUserSettings(),
        api.getWebServerStatus(),
        api.getWebServerToken(),
      ]);
      status = nextStatus;
      enabled = nextStatus.enabled;
      token = nextToken;
      portInput = String(settings.web_server_port ?? nextStatus.port ?? 9476);
      bindValue = settings.web_server_bind ?? nextStatus.bind ?? "127.0.0.1";
      origins = [...(settings.web_server_allowed_origins ?? [])];
      tunnelUrl = settings.web_server_tunnel_url ?? "";
      appliedTunnelUrl = tunnelUrl;
      if (nextStatus.running) await refreshLanIp(nextStatus.bind);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading = false;
    }
  }

  async function refreshLanIp(bind: string) {
    const isLanBind = bind === "0.0.0.0" || bind === "::" || bind === "[::]";
    if (!isLanBind) {
      lanIp = null;
      return;
    }
    lanIp = await api.getLocalIp(bind === "::" || bind === "[::]").catch(() => null);
  }

  async function save() {
    const port = Number.parseInt(portInput, 10);
    if (!Number.isInteger(port) || port < 1024 || port > 65535) {
      error = "端口必须是 1024 到 65535 之间的整数。";
      return;
    }

    saving = true;
    error = "";
    message = "";
    try {
      const result = await api.restartWebServer({
        enabled,
        port,
        bind: bindValue,
        allowed_origins: origins.length > 0 ? origins : null,
        tunnel_url: tunnelUrl.trim() || null,
      });
      status = await api.getWebServerStatus();
      enabled = status.enabled;
      appliedTunnelUrl = tunnelUrl.trim();
      if (status.running) await refreshLanIp(status.bind);
      message = result.config_saved
        ? "远程访问设置已保存并生效。"
        : "服务已启动，但设置持久化失败。";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
      status = await api.getWebServerStatus().catch(() => status);
    } finally {
      saving = false;
    }
  }

  async function regenerateToken() {
    regenerating = true;
    error = "";
    message = "";
    try {
      token = await api.regenerateWebServerToken();
      message = "Token 已重置，旧链接已失效。";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      regenerating = false;
    }
  }

  function addOrigin() {
    const value = originInput.trim().replace(/\/+$/, "");
    if (!value) return;
    try {
      const url = new URL(value);
      if (url.protocol !== "http:" && url.protocol !== "https:") throw new Error();
      if (!origins.includes(url.origin)) origins = [...origins, url.origin];
      originInput = "";
      error = "";
    } catch {
      error = "允许来源必须是 http 或 https 地址。";
    }
  }

  function removeOrigin(origin: string) {
    origins = origins.filter((item) => item !== origin);
  }

  function buildLocalAccessUrl(): string | null {
    if (!status?.running || !token) return null;
    const isLanBind = status.bind === "0.0.0.0" || status.bind === "::" || status.bind === "[::]";
    const rawHost = isLanBind ? lanIp : status.bind;
    if (!rawHost) return null;
    const host = rawHost.includes(":") ? `[${rawHost}]` : rawHost;
    return `http://${host}:${status.port}/login#token=${token}`;
  }

  function buildTunnelAccessUrl(): string | null {
    if (!status?.running || !token || !appliedTunnelUrl) return null;
    try {
      return `${new URL(appliedTunnelUrl).origin}/login?token=${token}`;
    } catch {
      return null;
    }
  }

  async function copy(value: string, kind: LinkKind) {
    try {
      await navigator.clipboard.writeText(value);
      copied = kind;
      setTimeout(() => {
        if (copied === kind) copied = null;
      }, 1500);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function open(value: string) {
    try {
      await platform.shell.openExternal(value);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }
</script>

{#if loading}
  <Card class="p-6 text-sm text-muted-foreground">正在读取远程访问状态...</Card>
{:else if error && !status}
  <Card class="p-6 space-y-3">
    <h2 class="text-sm font-semibold">无法读取远程访问状态</h2>
    <p class="text-xs text-destructive">{error}</p>
    <Button size="sm" variant="outline" onclick={() => void load()}>重试</Button>
  </Card>
{:else}
  <div class="space-y-6">
    <Card class="space-y-5 p-6">
      <div class="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h2 class="text-sm font-semibold">远程浏览器访问</h2>
          <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            启动内置 Web Server 后，可从局域网浏览器或 HTTP 隧道访问 AgentCabin。访问链接包含
            Token，请只分享给可信用户。
          </p>
        </div>
        <span
          class="rounded-full px-2.5 py-1 text-[11px] font-medium {status?.running
            ? 'bg-emerald-500/10 text-emerald-500'
            : 'bg-muted text-muted-foreground'}"
        >
          {status?.running ? "运行中" : "未运行"}
        </span>
      </div>

      <label class="flex items-center gap-2 text-sm">
        <input class="h-4 w-4 accent-primary" type="checkbox" bind:checked={enabled} />
        <span>启用远程访问服务</span>
      </label>

      <div class="grid gap-4 sm:grid-cols-2">
        <label class="space-y-1.5 text-xs">
          <span class="font-medium text-foreground">端口</span>
          <input
            class="w-full rounded-md border bg-transparent px-3 py-2 text-sm focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring"
            type="number"
            min="1024"
            max="65535"
            bind:value={portInput}
          />
        </label>
        <label class="space-y-1.5 text-xs">
          <span class="font-medium text-foreground">绑定地址</span>
          <select
            class="w-full rounded-md border bg-background px-3 py-2 text-sm focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring"
            bind:value={bindValue}
          >
            <option value="127.0.0.1">仅本机 (127.0.0.1)</option>
            <option value="0.0.0.0">局域网 IPv4 (0.0.0.0)</option>
            <option value="::">局域网 IPv6 (::)</option>
          </select>
        </label>
      </div>

      {#if bindExposesLan}
        <p
          class="rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-600 dark:text-amber-300"
        >
          当前绑定地址会暴露到局域网。请确认系统防火墙和 Token 策略符合预期。
        </p>
      {/if}

      <div class="space-y-2">
        <div class="flex items-center justify-between gap-3">
          <span class="text-xs font-medium">允许的来源地址</span>
          <span class="text-[11px] text-muted-foreground">可选，用于 CORS</span>
        </div>
        <div class="flex gap-2">
          <input
            class="min-w-0 flex-1 rounded-md border bg-transparent px-3 py-2 text-sm focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring"
            placeholder="https://example.com"
            bind:value={originInput}
            onkeydown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                addOrigin();
              }
            }}
          />
          <Button size="sm" variant="outline" onclick={addOrigin}>添加</Button>
        </div>
        {#if origins.length > 0}
          <div class="flex flex-wrap gap-1.5">
            {#each origins as origin (origin)}
              <button
                type="button"
                class="rounded-full border px-2.5 py-1 text-[11px] text-muted-foreground hover:border-destructive/50 hover:text-destructive"
                onclick={() => removeOrigin(origin)}
                title="移除来源"
              >
                {origin} ×
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <label class="space-y-1.5 text-xs">
        <span class="font-medium text-foreground">HTTP 隧道地址</span>
        <input
          class="w-full rounded-md border bg-transparent px-3 py-2 text-sm focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring"
          placeholder="https://your-tunnel.example.com"
          bind:value={tunnelUrl}
        />
        <span class="block text-[11px] text-muted-foreground"
          >填写 ngrok、cloudflared 等隧道的公开 HTTPS 地址。</span
        >
      </label>

      {#if error}
        <p class="text-xs text-destructive">{error}</p>
      {/if}
      {#if message}
        <p class="text-xs text-emerald-600 dark:text-emerald-400">{message}</p>
      {/if}
      {#if status?.warning}
        <p class="whitespace-pre-line text-xs text-amber-600 dark:text-amber-300">
          {status.warning}
        </p>
      {/if}

      <div class="flex justify-end">
        <Button loading={saving} onclick={() => void save()}>保存并重启服务</Button>
      </div>
    </Card>

    <Card class="space-y-4 p-6">
      <div>
        <h2 class="text-sm font-semibold">访问凭据</h2>
        <p class="mt-1 text-xs text-muted-foreground">重置 Token 会立即使已有访问链接失效。</p>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <code class="min-w-0 flex-1 rounded-md bg-muted px-3 py-2 text-xs"
          >{showToken ? (token ?? "未生成") : "••••••••••••••••"}</code
        >
        <Button size="sm" variant="outline" onclick={() => (showToken = !showToken)}>
          {showToken ? "隐藏" : "显示"}
        </Button>
        {#if token}
          <Button size="sm" variant="outline" onclick={() => void copy(token!, "token")}>
            {copied === "token" ? "已复制" : "复制"}
          </Button>
        {/if}
        <Button
          size="sm"
          variant="outline"
          loading={regenerating}
          onclick={() => void regenerateToken()}
        >
          重置 Token
        </Button>
      </div>
    </Card>

    <Card class="space-y-4 p-6">
      <div>
        <h2 class="text-sm font-semibold">访问链接</h2>
        <p class="mt-1 text-xs text-muted-foreground">
          服务运行后生成。局域网链接使用浏览器片段传递 Token，隧道链接使用查询参数。
        </p>
      </div>
      {#if localAccessUrl}
        <div class="space-y-2">
          <span class="text-xs font-medium">局域网链接</span>
          <div class="flex gap-2">
            <code class="min-w-0 flex-1 break-all rounded-md bg-muted px-3 py-2 text-xs"
              >{localAccessUrl}</code
            >
            <Button size="sm" variant="outline" onclick={() => void copy(localAccessUrl, "local")}>
              {copied === "local" ? "已复制" : "复制"}
            </Button>
            <Button size="sm" variant="outline" onclick={() => void open(localAccessUrl)}
              >打开</Button
            >
          </div>
        </div>
      {/if}
      {#if tunnelAccessUrl}
        <div class="space-y-2">
          <span class="text-xs font-medium">隧道链接</span>
          <div class="flex gap-2">
            <code class="min-w-0 flex-1 break-all rounded-md bg-muted px-3 py-2 text-xs"
              >{tunnelAccessUrl}</code
            >
            <Button
              size="sm"
              variant="outline"
              onclick={() => void copy(tunnelAccessUrl, "tunnel")}
            >
              {copied === "tunnel" ? "已复制" : "复制"}
            </Button>
            <Button size="sm" variant="outline" onclick={() => void open(tunnelAccessUrl)}
              >打开</Button
            >
          </div>
        </div>
      {/if}
      {#if !localAccessUrl && !tunnelAccessUrl}
        <p class="text-xs text-muted-foreground">启用并启动服务后，这里会显示可用访问链接。</p>
      {/if}
    </Card>
  </div>
{/if}
