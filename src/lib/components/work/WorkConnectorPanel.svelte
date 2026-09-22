<script lang="ts">
  import type { WorkConnectorHealth, WorkConnectorSummary } from "$lib/types/work";

  type CatalogSource = "discover" | "enabled";

  interface Props {
    connectors: WorkConnectorSummary[];
    onSave: (input: {
      name: string;
      transport: string;
      command: string | null;
      args: string[];
      url: string | null;
      envVars: Record<string, string>;
      headers: Record<string, string>;
    }) => Promise<void>;
    onToggle: (name: string, enabled: boolean) => Promise<void>;
    onRemove: (name: string) => Promise<void>;
    onTest: (name: string) => Promise<WorkConnectorHealth>;
    source?: CatalogSource;
    showAll?: boolean;
    canToggle?: boolean;
  }

  let {
    connectors,
    onSave,
    onToggle,
    onRemove,
    onTest,
    source = "discover",
    showAll = false,
    canToggle = true,
  }: Props = $props();
  let name = $state("");
  let transport = $state<"stdio" | "streamable-http" | "sse">("stdio");
  let command = $state("");
  let argsText = $state("");
  let url = $state("");
  let envText = $state("");
  let headersText = $state("");
  let saving = $state(false);
  let busyName = $state("");
  let testingName = $state("");
  let healthByName = $state<Record<string, WorkConnectorHealth>>({});
  let error = $state("");
  let query = $state("");

  let enabledCount = $derived(connectors.filter((connector) => connector.enabled).length);
  let visibleConnectors = $derived.by(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    return connectors.filter((connector) => {
      if (source === "enabled" && !showAll && !connector.enabled) return false;
      if (!normalizedQuery) return true;
      return [connector.name, connector.transport, connector.command ?? "", connector.url ?? ""]
        .join(" ")
        .toLocaleLowerCase()
        .includes(normalizedQuery);
    });
  });

  function parseLines(value: string, label: string): Record<string, string> {
    const result: Record<string, string> = {};
    for (const line of value.split("\n")) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      const separator = trimmed.indexOf("=");
      if (separator <= 0) throw new Error(`${label}请使用 KEY=VALUE 格式`);
      const key = trimmed.slice(0, separator).trim();
      if (!key) throw new Error(`${label}名称不能为空`);
      result[key] = trimmed.slice(separator + 1);
    }
    return result;
  }

  function resetForm() {
    name = "";
    transport = "stdio";
    command = "";
    argsText = "";
    url = "";
    envText = "";
    headersText = "";
  }

  async function save() {
    saving = true;
    error = "";
    try {
      const trimmedName = name.trim();
      if (!trimmedName) throw new Error("请输入 MCP 服务名称");
      const trimmedCommand = command.trim();
      const trimmedUrl = url.trim();
      if (transport === "stdio" && !trimmedCommand) throw new Error("stdio MCP 服务需要 command");
      if (transport !== "stdio" && !trimmedUrl) throw new Error("HTTP MCP 服务需要 URL");
      await onSave({
        name: trimmedName,
        transport,
        command: trimmedCommand || null,
        args: argsText
          .split("\n")
          .map((value) => value.trim())
          .filter(Boolean),
        url: trimmedUrl || null,
        envVars: parseLines(envText, "环境变量"),
        headers: parseLines(headersText, "请求头"),
      });
      resetForm();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      saving = false;
    }
  }

  async function toggle(connector: WorkConnectorSummary) {
    if (busyName) return;
    busyName = connector.name;
    error = "";
    try {
      await onToggle(connector.name, !connector.enabled);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyName = "";
    }
  }

  async function remove(connector: WorkConnectorSummary) {
    if (busyName) return;
    const { confirm } = await import("$lib/platform/dialog");
    const ok = await confirm(`移除自定义 MCP 服务“${connector.name}”？`, {
      title: "移除 MCP 服务",
      kind: "warning",
    });
    if (!ok) return;
    busyName = connector.name;
    error = "";
    try {
      await onRemove(connector.name);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyName = "";
    }
  }

  async function test(connector: WorkConnectorSummary) {
    if (testingName || busyName) return;
    testingName = connector.name;
    error = "";
    try {
      const health = await onTest(connector.name);
      healthByName = { ...healthByName, [connector.name]: health };
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      testingName = "";
    }
  }

  function healthClass(status: WorkConnectorHealth["status"]): string {
    if (status === "healthy") return "text-emerald-600 dark:text-emerald-300";
    if (status === "failed") return "text-red-500 dark:text-red-300";
    return "text-muted-foreground";
  }

  function credentialCount(connector: WorkConnectorSummary): number {
    return connector.envKeys.length + connector.headerKeys.length;
  }

  function openCustomConfig() {
    const details = document.getElementById("custom-mcp-config");
    if (!(details instanceof HTMLDetailsElement)) return;
    details.open = true;
    details.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }
</script>

<section class="rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5">
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div>
      <div class="flex items-center gap-2">
        <span class="h-2 w-2 rounded-full bg-cyan-500"></span>
        <h2 class="text-sm font-semibold text-foreground">自定义 MCP 服务器</h2>
        <span
          class="rounded-full bg-cyan-500/10 px-2 py-0.5 text-[11px] text-cyan-700 dark:text-cyan-300"
          >{connectors.length}</span
        >
      </div>
      <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
        {source === "discover"
          ? "手动添加 MCP 服务器，可使用本地命令或远程 URL；凭据值单独加密保存。"
          : "已配置的自定义 MCP 服务器，可检查连通性并查看实际工具。"}
      </p>
    </div>
    <div class="flex items-center gap-2 text-[11px] text-muted-foreground">
      <span class="rounded-lg bg-muted px-2.5 py-1.5">{connectors.length} 个已配置</span>
      <button
        type="button"
        class="rounded-lg border border-primary/30 px-2.5 py-1.5 text-primary transition-colors hover:bg-primary/10"
        onclick={openCustomConfig}
      >
        添加自定义 MCP 服务器
      </button>
      {#if canToggle}
        <span class="rounded-lg bg-cyan-500/10 px-2.5 py-1.5 text-cyan-700 dark:text-cyan-300"
          >{enabledCount} 个已启用</span
        >
      {/if}
    </div>
  </div>

  {#if error}
    <div
      class="mt-3 rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
      role="alert"
    >
      {error}
    </div>
  {/if}

  <label class="relative mt-4 block max-w-sm">
    <span class="sr-only">搜索自定义 MCP 服务器</span>
    <svg
      class="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      aria-hidden="true"
    >
      <circle cx="11" cy="11" r="7"></circle>
      <path d="m20 20-3.2-3.2"></path>
    </svg>
    <input
      class="min-h-10 w-full rounded-lg border border-border bg-background pl-9 pr-3 text-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground focus:border-primary focus:ring-2 focus:ring-primary/15"
      bind:value={query}
      placeholder="搜索 MCP 服务器"
    />
  </label>

  {#if visibleConnectors.length === 0}
    <div class="mt-4 rounded-xl border border-dashed border-border/70 px-4 py-8 text-center">
      <div class="text-xs font-medium text-foreground">
        {query.trim()
          ? "没有匹配的 MCP 服务器"
          : source === "enabled"
            ? "暂无可用的自定义 MCP 服务器"
            : "尚未添加自定义 MCP 服务器"}
      </div>
      <p class="mt-1 text-[11px] text-muted-foreground">
        {source === "discover"
          ? "可以在下方直接添加，也可以使用上方 MCP 的“发现”页安装服务器。"
          : "可以切换到“发现”安装服务器，或点击“添加自定义 MCP 服务器”。"}
      </p>
    </div>
  {:else}
    <div class="mt-4 grid gap-2">
      {#each visibleConnectors as connector (connector.name)}
        {@const piRuntimeReady = connector.piRuntimeAvailable ?? connector.adapterInstalled}
        {@const dshRuntimeReady = connector.dshRuntimeAvailable ?? false}
        <div
          class="flex flex-wrap items-center gap-3 rounded-xl border border-border/60 bg-background/50 px-3.5 py-3"
        >
          <span
            class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-cyan-500/10 text-[11px] font-semibold text-cyan-700 dark:text-cyan-300"
            >MCP</span
          >
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-2">
              <span class="text-xs font-medium text-foreground">{connector.name}</span>
              <span class="rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground"
                >{connector.transport}</span
              >
              <span
                class="rounded-full px-2 py-0.5 text-[9px] {piRuntimeReady || dshRuntimeReady
                  ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                  : 'bg-amber-500/10 text-amber-700 dark:text-amber-300'}"
              >
                {#if piRuntimeReady && dshRuntimeReady}
                  Pi / DSH 可用
                {:else if dshRuntimeReady}
                  DSH Bridge 可用
                {:else if piRuntimeReady}
                  Pi Adapter 可用
                {:else}
                  不可用 · 缺少 Runtime
                {/if}
              </span>
              <span
                class="rounded-full px-2 py-0.5 text-[9px] {credentialCount(connector) > 0
                  ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                  : 'bg-muted text-muted-foreground'}"
              >
                {credentialCount(connector) > 0
                  ? `凭据已保存 · ${credentialCount(connector)}`
                  : "未配置凭据"}
              </span>
            </div>
            <div class="mt-1 truncate font-mono text-[10px] text-muted-foreground">
              {connector.command || connector.url || "未设置入口"}
              {#if canToggle}
                · {connector.enabled ? "已启用，下次会话生效" : "已配置，未启用"}
              {/if}
            </div>
            {#if healthByName[connector.name]}
              {@const health = healthByName[connector.name]}
              <div
                class="mt-1 flex flex-wrap items-center gap-2 text-[10px] {healthClass(
                  health.status,
                )}"
              >
                <span>{health.message}</span>
                {#if health.status === "healthy"}
                  <span>· {health.latencyMs} ms · {health.toolCount} 个工具</span>
                {/if}
              </div>
              {#if health.toolNames.length > 0}
                <div class="mt-1 flex flex-wrap gap-1">
                  {#each health.toolNames.slice(0, 8) as toolName}
                    <span
                      class="rounded bg-muted px-1.5 py-0.5 font-mono text-[9px] text-muted-foreground"
                      >{toolName}</span
                    >
                  {/each}
                  {#if health.toolNames.length > 8}
                    <span class="text-[9px] text-muted-foreground"
                      >+{health.toolNames.length - 8}</span
                    >
                  {/if}
                </div>
              {/if}
            {/if}
          </div>
          <div class="flex shrink-0 items-center gap-1.5">
            {#if canToggle}
              <button
                type="button"
                class="min-h-8 rounded-lg border border-border px-2 text-[11px] text-foreground transition-colors hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                disabled={testingName !== "" || busyName !== "" || !connector.enabled}
                onclick={() => void test(connector)}
              >
                {testingName === connector.name ? "检测中…" : "测试连接"}
              </button>
            {/if}
            {#if canToggle}
              <button
                type="button"
                class="relative inline-flex h-7 w-12 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 {connector.enabled
                  ? 'bg-primary'
                  : 'bg-muted-foreground/25'}"
                aria-label={`${connector.enabled ? "停用" : "启用"} ${connector.name}`}
                aria-pressed={connector.enabled}
                disabled={busyName !== ""}
                onclick={() => void toggle(connector)}
              >
                <span
                  class="inline-block h-5 w-5 rounded-full bg-white shadow-sm transition-transform {connector.enabled
                    ? 'translate-x-6'
                    : 'translate-x-1'}"
                ></span>
              </button>
            {/if}
            <button
              type="button"
              class="min-h-8 rounded-lg px-2 text-[11px] text-muted-foreground transition-colors hover:bg-red-400/10 hover:text-red-500 disabled:opacity-50"
              disabled={busyName !== ""}
              onclick={() => void remove(connector)}
            >
              移除
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <details
    id="custom-mcp-config"
    class="mt-4 rounded-xl border border-dashed border-border/70 px-3.5 py-3"
  >
    <summary class="cursor-pointer list-none text-xs font-medium text-foreground"
      >添加自定义 MCP 服务器</summary
    >
    <form
      class="mt-4 grid gap-3 sm:grid-cols-2"
      onsubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <label class="text-xs font-medium text-foreground">
        名称
        <input
          class="mt-1.5 min-h-10 w-full rounded-lg border border-border bg-background px-3 text-xs outline-none focus:border-primary focus:ring-2 focus:ring-primary/15"
          bind:value={name}
          placeholder="例如：research"
          maxlength="80"
        />
      </label>
      <label class="text-xs font-medium text-foreground">
        传输
        <select
          class="mt-1.5 min-h-10 w-full rounded-lg border border-border bg-background px-3 text-xs outline-none focus:border-primary focus:ring-2 focus:ring-primary/15"
          bind:value={transport}
        >
          <option value="stdio">stdio · 本地进程</option>
          <option value="streamable-http">streamable-http</option>
          <option value="sse">SSE</option>
        </select>
      </label>
      {#if transport === "stdio"}
        <label class="text-xs font-medium text-foreground">
          command
          <input
            class="mt-1.5 min-h-10 w-full rounded-lg border border-border bg-background px-3 font-mono text-xs outline-none focus:border-primary focus:ring-2 focus:ring-primary/15"
            bind:value={command}
            placeholder="例如：npx"
          />
        </label>
        <label class="text-xs font-medium text-foreground">
          参数（每行一个）
          <textarea
            class="mt-1.5 min-h-10 w-full resize-y rounded-lg border border-border bg-background px-3 py-2 font-mono text-xs outline-none focus:border-primary focus:ring-2 focus:ring-primary/15"
            bind:value={argsText}
            placeholder="-y&#10;@acme/mcp-server"
          ></textarea>
        </label>
      {:else}
        <label class="text-xs font-medium text-foreground sm:col-span-2">
          URL
          <input
            class="mt-1.5 min-h-10 w-full rounded-lg border border-border bg-background px-3 font-mono text-xs outline-none focus:border-primary focus:ring-2 focus:ring-primary/15"
            bind:value={url}
            placeholder="https://example.com/mcp"
          />
        </label>
      {/if}
      <label class="text-xs font-medium text-foreground">
        环境变量（可选，KEY=VALUE）
        <textarea
          class="mt-1.5 min-h-20 w-full resize-y rounded-lg border border-border bg-background px-3 py-2 font-mono text-xs outline-none focus:border-primary focus:ring-2 focus:ring-primary/15"
          bind:value={envText}
          placeholder="API_TOKEN=…"
        ></textarea>
      </label>
      <label class="text-xs font-medium text-foreground">
        请求头（可选，KEY=VALUE）
        <textarea
          class="mt-1.5 min-h-20 w-full resize-y rounded-lg border border-border bg-background px-3 py-2 font-mono text-xs outline-none focus:border-primary focus:ring-2 focus:ring-primary/15"
          bind:value={headersText}
          placeholder="Authorization=Bearer …"
        ></textarea>
      </label>
      <div class="flex items-end justify-end sm:col-span-2">
        <button
          type="submit"
          class="min-h-10 rounded-lg bg-primary px-4 py-2 text-xs font-semibold text-primary-foreground transition-all hover:shadow-md disabled:cursor-not-allowed disabled:opacity-60"
          disabled={saving}
        >
          {saving ? "保存中…" : "保存 MCP 服务器"}
        </button>
      </div>
    </form>
  </details>
</section>
