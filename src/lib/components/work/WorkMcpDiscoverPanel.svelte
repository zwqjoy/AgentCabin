<script lang="ts">
  import { onMount } from "svelte";
  import { checkMcpRegistryHealth, searchMcpRegistry } from "$lib/api";
  import type { McpRegistryServer, ProviderHealth } from "$lib/types";
  import type { WorkConnectorSummary } from "$lib/types/work";

  interface ConnectorInput {
    name: string;
    transport: string;
    command: string | null;
    args: string[];
    url: string | null;
    envVars: Record<string, string>;
    headers: Record<string, string>;
  }

  interface Props {
    connectors: WorkConnectorSummary[];
    onSave: (input: ConnectorInput) => Promise<void>;
  }

  let { connectors, onSave }: Props = $props();
  let health = $state<ProviderHealth | null>(null);
  let query = $state("");
  let popular = $state<McpRegistryServer[]>([]);
  let results = $state<McpRegistryServer[]>([]);
  let selected = $state<McpRegistryServer | null>(null);
  let envValues = $state<Record<string, string>>({});
  let headerValues = $state<Record<string, string>>({});
  let loading = $state(true);
  let searching = $state(false);
  let installing = $state(false);
  let error = $state("");
  let success = $state("");
  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  const categories = ["GitHub", "database", "browser", "search", "productivity"];
  let displayResults = $derived(query.trim().length >= 2 ? results : popular);

  function localName(server: McpRegistryServer): string {
    const raw = server.title?.trim() || server.name.split(/[/.]/).filter(Boolean).at(-1) || "mcp";
    const normalized = raw
      .toLocaleLowerCase()
      .replace(/[^a-z0-9_-]/g, "-")
      .replace(/-+/g, "-")
      .replace(/^-|-$/g, "")
      .slice(0, 80);
    return normalized || "mcp-server";
  }

  function transportLabel(server: McpRegistryServer): string {
    if (server.remotes.length > 0) return "HTTP";
    return server.packages[0]?.registryType || "stdio";
  }

  function isConfigured(server: McpRegistryServer): boolean {
    const remoteUrl = server.remotes[0]?.url;
    const packageId = server.packages[0]?.identifier;
    return connectors.some((connector) => {
      if (remoteUrl && connector.url === remoteUrl) return true;
      if (packageId && connector.args.some((arg) => arg === packageId || arg.includes(packageId))) {
        return true;
      }
      return connector.name === localName(server);
    });
  }

  function formatErrorMessage(cause: unknown): string {
    const msg = cause instanceof Error ? cause.message : String(cause);
    if (
      msg.includes("registry.modelcontextprotocol.io") ||
      msg.includes("Search request failed") ||
      msg.includes("error sending request")
    ) {
      return "无法连接到官方 MCP Registry 服务（网络请求超时或连接受阻），请检查网络或系统代理配置。";
    }
    return msg;
  }

  async function loadPopular() {
    loading = true;
    error = "";
    try {
      const [nextHealth, response] = await Promise.allSettled([
        checkMcpRegistryHealth(),
        searchMcpRegistry("server", 18),
      ]);
      if (nextHealth.status === "fulfilled") {
        health = nextHealth.value;
      }
      if (response.status === "fulfilled") {
        popular = response.value.servers;
      } else {
        error = formatErrorMessage(response.reason);
      }
    } catch (cause) {
      error = formatErrorMessage(cause);
    } finally {
      loading = false;
    }
  }

  function scheduleSearch() {
    if (searchTimer) clearTimeout(searchTimer);
    const value = query.trim();
    if (value.length < 2) {
      results = [];
      return;
    }
    searchTimer = setTimeout(() => void search(value), 300);
  }

  async function search(value = query.trim()) {
    if (value.length < 2) return;
    searching = true;
    error = "";
    try {
      results = (await searchMcpRegistry(value, 30)).servers;
    } catch (cause) {
      error = formatErrorMessage(cause);
      results = [];
    } finally {
      searching = false;
    }
  }

  function chooseCategory(category: string) {
    query = category;
    void search(category);
  }

  function selectServer(server: McpRegistryServer) {
    selected = server;
    success = "";
    error = "";
    envValues = Object.fromEntries(
      (server.packages[0]?.environmentVariables ?? []).map((variable) => [variable.name, ""]),
    );
    headerValues = Object.fromEntries(
      (server.remotes[0]?.headers ?? []).map((header) => [header.name, header.value ?? ""]),
    );
  }

  function missingRequiredCredential(server: McpRegistryServer): string | null {
    for (const variable of server.packages[0]?.environmentVariables ?? []) {
      if (variable.isRequired && !envValues[variable.name]?.trim()) return variable.name;
    }
    for (const header of server.remotes[0]?.headers ?? []) {
      if (header.isRequired && !headerValues[header.name]?.trim()) return header.name;
    }
    return null;
  }

  async function install() {
    const server = selected;
    if (!server || installing || isConfigured(server)) return;
    const missing = missingRequiredCredential(server);
    if (missing) {
      error = `请先填写必填凭据 ${missing}`;
      return;
    }
    installing = true;
    error = "";
    success = "";
    try {
      const remote = server.remotes[0];
      const pkg = server.packages[0];
      await onSave({
        name: localName(server),
        transport: remote ? (remote.type === "sse" ? "sse" : "streamable-http") : "stdio",
        command: pkg ? (pkg.registryType === "pypi" ? "uvx" : "npx") : null,
        args: pkg ? ["-y", pkg.identifier] : [],
        url: remote?.url ?? null,
        envVars: Object.fromEntries(
          Object.entries(envValues).filter(([, value]) => value.trim().length > 0),
        ),
        headers: Object.fromEntries(
          Object.entries(headerValues).filter(([, value]) => value.trim().length > 0),
        ),
      });
      success = `已把“${server.title || server.name}”添加到 Work，下次会话生效。`;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      installing = false;
    }
  }

  onMount(() => {
    void loadPopular();
    return () => {
      if (searchTimer) clearTimeout(searchTimer);
    };
  });
</script>

<section class="rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5">
  <div class="flex flex-wrap items-start justify-between gap-4">
    <div>
      <div class="flex items-center gap-2">
        <span class="h-2 w-2 rounded-full bg-cyan-500"></span>
        <h2 class="text-sm font-semibold text-foreground">MCP Registry</h2>
        <span
          class="h-2 w-2 rounded-full {health === null
            ? 'bg-muted-foreground/40'
            : health.available
              ? 'bg-emerald-500'
              : 'bg-red-500'}"
          title={health?.reason ?? "正在检查 Registry 状态"}
        ></span>
      </div>
      <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
        复用 Code 的官方 MCP Registry 数据源，安装时转换为 Work 连接器并把凭据存入 Work 密钥库。
      </p>
    </div>
    <button
      type="button"
      class="min-h-9 rounded-lg border border-border px-3 text-[11px] font-medium text-foreground transition-colors hover:bg-accent disabled:opacity-50"
      disabled={loading}
      onclick={() => void loadPopular()}
    >
      {loading ? "加载中…" : "刷新 Registry"}
    </button>
  </div>

  <div class="mt-4 rounded-xl border border-border/60 bg-background/35 p-3">
    <label class="relative block max-w-xl">
      <span class="sr-only">搜索 MCP Registry</span>
      <svg
        class="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        aria-hidden="true"
      >
        <circle cx="11" cy="11" r="7"></circle><path d="m20 20-3.2-3.2"></path>
      </svg>
      <input
        class="min-h-10 w-full rounded-lg border border-border bg-background pl-9 pr-3 text-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground focus:border-primary focus:ring-2 focus:ring-primary/15"
        bind:value={query}
        oninput={scheduleSearch}
        placeholder="搜索 GitHub、数据库、浏览器…"
      />
    </label>
    <div class="mt-2 flex flex-wrap gap-1.5">
      {#each categories as category}
        <button
          type="button"
          class="min-h-8 rounded-lg bg-muted px-2.5 text-[10px] text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          onclick={() => chooseCategory(category)}>{category}</button
        >
      {/each}
    </div>
  </div>

  {#if error}
    <div
      class="mt-3 flex items-center justify-between gap-3 rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
      role="alert"
    >
      <span>{error}</span>
      <button
        type="button"
        class="shrink-0 font-medium underline hover:text-red-700 dark:hover:text-red-300"
        onclick={() => void loadPopular()}
      >
        重试
      </button>
    </div>
  {/if}
  {#if success}
    <div
      class="mt-3 rounded-lg border border-emerald-400/20 bg-emerald-400/5 px-3 py-2 text-xs text-emerald-700 dark:text-emerald-300"
      role="status"
    >
      {success}
    </div>
  {/if}

  {#if selected}
    <div class="mt-4 rounded-xl border border-cyan-500/25 bg-cyan-500/5 p-4">
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 class="text-xs font-semibold text-foreground">
            添加 {selected.title || selected.name}
          </h3>
          <p class="mt-1 text-[10px] text-muted-foreground">
            {transportLabel(selected)} · {localName(selected)}
          </p>
        </div>
        <button
          type="button"
          class="text-[10px] text-muted-foreground hover:text-foreground"
          onclick={() => (selected = null)}>取消</button
        >
      </div>
      {#if Object.keys(envValues).length > 0 || Object.keys(headerValues).length > 0}
        <div class="mt-3 grid gap-2 sm:grid-cols-2">
          {#each selected.packages[0]?.environmentVariables ?? [] as variable}
            <label class="text-[10px] font-medium text-foreground">
              {variable.name}{variable.isRequired ? " *" : ""}
              <input
                class="mt-1 min-h-9 w-full rounded-lg border border-border bg-background px-3 font-mono text-[10px] outline-none focus:border-primary"
                type={variable.isSecret ? "password" : "text"}
                bind:value={envValues[variable.name]}
                placeholder={variable.description ?? "环境变量"}
              />
            </label>
          {/each}
          {#each selected.remotes[0]?.headers ?? [] as header}
            <label class="text-[10px] font-medium text-foreground">
              {header.name}{header.isRequired ? " *" : ""}
              <input
                class="mt-1 min-h-9 w-full rounded-lg border border-border bg-background px-3 font-mono text-[10px] outline-none focus:border-primary"
                type={header.isSecret ? "password" : "text"}
                bind:value={headerValues[header.name]}
                placeholder={header.description ?? "请求头"}
              />
            </label>
          {/each}
        </div>
      {/if}
      <div class="mt-3 flex justify-end">
        <button
          type="button"
          class="min-h-9 rounded-lg bg-foreground px-3 text-[10px] font-semibold text-background disabled:opacity-60"
          disabled={installing || isConfigured(selected)}
          onclick={() => void install()}
        >
          {isConfigured(selected) ? "已配置" : installing ? "添加中…" : "添加到 Work"}
        </button>
      </div>
    </div>
  {/if}

  {#if loading || searching}
    <div class="flex items-center justify-center gap-2 py-12 text-xs text-muted-foreground">
      <span class="h-4 w-4 animate-spin rounded-full border-2 border-primary/25 border-t-primary"
      ></span>{searching ? "正在搜索 MCP…" : "正在加载 Registry…"}
    </div>
  {:else if displayResults.length === 0}
    <div
      class="mt-4 rounded-xl border border-dashed border-border/70 px-4 py-8 text-center text-xs text-muted-foreground"
    >
      没有找到匹配的 MCP Server。
    </div>
  {:else}
    <div class="mt-4 grid gap-3 lg:grid-cols-3">
      {#each displayResults as server (server.name + server.version)}
        <article
          class="flex min-h-48 flex-col rounded-xl border border-border/60 bg-background/50 p-4 transition-colors hover:border-border"
        >
          <div class="flex items-start justify-between gap-3">
            <span
              class="flex h-9 w-9 items-center justify-center rounded-xl bg-cyan-500/10 text-[9px] font-bold text-cyan-700 dark:text-cyan-300"
              >MCP</span
            ><span class="rounded-md bg-muted px-2 py-1 text-[9px] text-muted-foreground"
              >{transportLabel(server)}</span
            >
          </div>
          <h3 class="mt-3 line-clamp-2 text-xs font-semibold text-foreground">
            {server.title || server.name}
          </h3>
          <p class="mt-1 line-clamp-3 text-[10px] leading-4 text-muted-foreground">
            {server.description || server.name}
          </p>
          <div class="mt-auto flex items-center justify-between gap-3 pt-4">
            <span class="text-[9px] text-muted-foreground"
              >{(server.packages[0]?.environmentVariables.length ?? 0) +
                (server.remotes[0]?.headers.length ?? 0)} 个凭据项</span
            >
            <button
              type="button"
              class="min-h-9 rounded-lg px-3 text-[10px] font-semibold transition-colors {isConfigured(
                server,
              )
                ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                : 'bg-foreground text-background hover:opacity-90'}"
              disabled={isConfigured(server)}
              onclick={() => selectServer(server)}
              >{isConfigured(server) ? "已配置" : "配置"}</button
            >
          </div>
        </article>
      {/each}
    </div>
  {/if}
</section>
