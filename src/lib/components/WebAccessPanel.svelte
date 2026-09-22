<script lang="ts">
  import type { WorkBrowserHealth, WorkBrowserSummary } from "$lib/types/work";

  interface Props {
    config: WorkBrowserSummary | null;
    enabled?: boolean;
    showSharedConfiguration?: boolean;
    canToggle?: boolean;
    onSave: (input: {
      provider?: string;
      enabled: boolean;
      maxResults: number;
      apiKey: string | null;
      endpointUrl?: string | null;
      allowedHosts?: string[] | null;
    }) => Promise<void>;
    onToggle?: (enabled: boolean) => Promise<void>;
    onTest: () => Promise<WorkBrowserHealth>;
  }

  let {
    config,
    enabled = true,
    showSharedConfiguration = false,
    canToggle = true,
    onSave,
    onToggle,
    onTest,
  }: Props = $props();

  let localEnabled = $state(enabled);
  let selectedProvider = $state(config?.provider ?? "tavily");
  let maxResults = $state(config?.maxResults ?? 5);
  let endpointUrl = $state(config?.endpointUrl ?? "");
  let apiKey = $state("");
  let allowedHostsText = $state(config?.allowedHosts?.join("\n") ?? "");
  let saving = $state(false);
  let testing = $state(false);
  let clearing = $state(false);
  let toggling = $state(false);
  let health = $state<WorkBrowserHealth | null>(null);
  let error = $state("");
  let success = $state("");

  const providers = [
    {
      id: "tavily",
      name: "Tavily Search",
      authKind: "api_key",
      desc: "AI 深度搜索与摘要（需 API Key）",
    },
    {
      id: "exa",
      name: "Exa (Metaphor)",
      authKind: "api_key",
      desc: "神经语义与代码检索（需 API Key）",
    },
    {
      id: "searxng",
      name: "SearXNG",
      authKind: "self_hosted_endpoint",
      desc: "自建聚合搜索实例（需实例 URL）",
    },
  ];

  const currentProviderMeta = $derived(
    providers.find((p) => p.id === selectedProvider) ?? providers[0],
  );

  $effect(() => {
    localEnabled = enabled;
    if (!config) return;
    selectedProvider = providers.some((provider) => provider.id === config.provider)
      ? config.provider
      : "tavily";
    maxResults = config.maxResults;
    endpointUrl = config.endpointUrl || "";
    allowedHostsText = config.allowedHosts?.join("\n") ?? "";
  });

  async function save() {
    saving = true;
    error = "";
    success = "";
    try {
      const enteredApiKey = apiKey.trim();
      const enteredEndpoint = endpointUrl.trim();
      const enteredHosts = allowedHostsText
        .split(/[,\n;]/)
        .map((s) => s.trim())
        .filter(Boolean);
      await onSave({
        provider: selectedProvider,
        enabled: config?.enabled ?? true,
        maxResults,
        apiKey: enteredApiKey || null,
        endpointUrl: enteredEndpoint || null,
        allowedHosts: enteredHosts,
      });
      apiKey = "";
      health = null;
      success = "网络访问公共配置已保存。";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      saving = false;
    }
  }

  async function toggle() {
    if (toggling || !onToggle) return;
    const nextEnabled = !localEnabled;
    toggling = true;
    error = "";
    success = "";
    try {
      await onToggle(nextEnabled);
      localEnabled = nextEnabled;
      success = `网络访问已全局${nextEnabled ? "启用" : "停用"}。`;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      toggling = false;
    }
  }

  async function test() {
    testing = true;
    error = "";
    try {
      health = await onTest();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      testing = false;
    }
  }

  async function clearCredential() {
    if (clearing) return;
    clearing = true;
    error = "";
    success = "";
    try {
      await onSave({
        provider: selectedProvider,
        enabled: false,
        maxResults,
        apiKey: "",
        endpointUrl: "",
      });
      if (canToggle && localEnabled && onToggle) {
        await onToggle(false);
        localEnabled = false;
      }
      apiKey = "";
      endpointUrl = "";
      health = null;
      success = "搜索凭据已重置，网络访问已停用。";
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      clearing = false;
    }
  }

  function statusLabel(status: WorkBrowserHealth["status"]): string {
    return { disabled: "未启用", unconfigured: "未配置", healthy: "健康", failed: "失败" }[status];
  }
</script>

<div class="space-y-5">
  <!-- Top Summary & Health Status -->
  <div class="flex flex-wrap items-center justify-end gap-2 px-0.5">
    <div class="flex items-center gap-2 text-xs">
      <span
        class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-[11px] font-medium {currentProviderMeta.authKind ===
        'anonymous'
          ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
          : config?.configured
            ? 'bg-blue-500/10 text-blue-600 dark:text-blue-400'
            : 'bg-amber-500/10 text-amber-600 dark:text-amber-400'}"
      >
        <span
          class="h-1.5 w-1.5 rounded-full {currentProviderMeta.authKind === 'anonymous'
            ? 'bg-emerald-500'
            : config?.configured
              ? 'bg-blue-500'
              : 'bg-amber-500'}"
        ></span>
        {currentProviderMeta.authKind === "anonymous"
          ? "免 Key 零配置"
          : config?.configured
            ? "凭据已就绪"
            : "需配置凭据"}
      </span>
      <span
        class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-[11px] font-medium {config?.runtimeAvailable
          ? 'bg-muted text-foreground/80'
          : 'bg-muted text-muted-foreground'}"
      >
        <span
          class="h-1.5 w-1.5 rounded-full {config?.runtimeAvailable
            ? 'bg-emerald-500'
            : 'bg-muted-foreground/50'}"
        ></span>
        {config?.runtimeAvailable ? "运行时就绪" : "已停用"}
      </span>
    </div>
  </div>

  {#if error}
    <div
      class="rounded-lg border border-red-500/20 bg-red-500/10 px-3.5 py-2.5 text-xs text-red-600 dark:text-red-400"
      role="alert"
    >
      {error}
    </div>
  {/if}
  {#if success}
    <div
      class="rounded-lg border border-emerald-500/20 bg-emerald-500/10 px-3.5 py-2.5 text-xs text-emerald-600 dark:text-emerald-400"
      role="status"
    >
      {success}
    </div>
  {/if}

  <!-- Group 1: General & Provider Config (Clean Grouped Inset List) -->
  <div
    class="rounded-xl border border-border/70 bg-card shadow-xs overflow-hidden divide-y divide-border/40"
  >
    {#if canToggle}
      <div class="flex items-center justify-between gap-4 px-4 py-3 sm:px-5 sm:py-3.5">
        <div class="min-w-0 flex-1">
          <div class="text-[13px] font-medium text-foreground">全局网络访问</div>
          <p class="mt-0.5 text-xs text-muted-foreground">
            控制 Code 与 Work 运行时是否在启动时挂载网络搜索底座
          </p>
        </div>
        <div class="flex items-center gap-2.5 shrink-0">
          <button
            type="button"
            class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 {localEnabled
              ? 'bg-primary'
              : 'bg-muted-foreground/20'}"
            aria-label={`${localEnabled ? "停用" : "启用"}全局网络访问`}
            aria-pressed={localEnabled}
            disabled={toggling || saving || clearing}
            onclick={() => void toggle()}
          >
            <span
              class="inline-block h-4 w-4 rounded-full bg-white shadow-xs transition-transform {localEnabled
                ? 'translate-x-6'
                : 'translate-x-1'}"
            ></span>
          </button>
          <span
            class="text-xs w-10 text-right font-medium {localEnabled
              ? 'text-primary'
              : 'text-muted-foreground'}"
          >
            {toggling ? "…" : localEnabled ? "开启" : "关闭"}
          </span>
        </div>
      </div>
    {/if}

    {#if showSharedConfiguration}
      <!-- Provider Select Row -->
      <div
        class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 px-4 py-3 sm:px-5 sm:py-3.5"
      >
        <div class="min-w-0 flex-1">
          <div class="text-[13px] font-medium text-foreground">搜索服务商 (Provider)</div>
          <p class="mt-0.5 text-xs text-muted-foreground">{currentProviderMeta.desc}</p>
        </div>
        <div class="shrink-0 sm:w-64">
          <select
            aria-label="搜索服务商"
            class="h-9 w-full rounded-lg border border-border/80 bg-background px-3 text-xs text-foreground focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40"
            bind:value={selectedProvider}
            onchange={() => {
              health = null;
              error = "";
            }}
          >
            {#each providers as p}
              <option value={p.id}>
                {p.name} ({p.authKind === "anonymous"
                  ? "免 Key"
                  : p.authKind === "self_hosted_endpoint"
                    ? "自建端点"
                    : "API Key"})
              </option>
            {/each}
          </select>
        </div>
      </div>

      <!-- Conditional Credentials Row -->
      {#if selectedProvider === "searxng"}
        <div
          class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 px-4 py-3 sm:px-5 sm:py-3.5"
        >
          <div class="min-w-0 flex-1">
            <div class="text-[13px] font-medium text-foreground">SearXNG 实例端点 URL</div>
            <p class="mt-0.5 text-xs text-muted-foreground">
              自建 SearXNG 聚合服务地址，实现无外部依赖的多源检索
            </p>
          </div>
          <div class="shrink-0 sm:w-72">
            <input
              aria-label="SearXNG 实例端点 URL"
              class="h-9 w-full rounded-lg border border-border/80 bg-background px-3 text-xs text-foreground placeholder:text-muted-foreground/50 focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40"
              type="url"
              placeholder="http://127.0.0.1:8080"
              bind:value={endpointUrl}
            />
          </div>
        </div>
        <div
          class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 px-4 py-3 sm:px-5 sm:py-3.5"
        >
          <div class="min-w-0 flex-1">
            <div class="text-[13px] font-medium text-foreground">
              Authorization Bearer Token (可选)
            </div>
            <p class="mt-0.5 text-xs text-muted-foreground">
              如果 SearXNG 开启了身份凭据校验，请在此填入 Token
            </p>
          </div>
          <div class="shrink-0 sm:w-72">
            <input
              aria-label="Authorization Bearer Token"
              class="h-9 w-full rounded-lg border border-border/80 bg-background px-3 text-xs text-foreground placeholder:text-muted-foreground/50 focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40"
              type="password"
              autocomplete="off"
              placeholder={config?.configured && config?.provider === selectedProvider
                ? "已配置（留空保持不变）"
                : "请输入 Bearer Token..."}
              bind:value={apiKey}
            />
          </div>
        </div>
      {:else if currentProviderMeta.authKind === "api_key"}
        <div
          class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 px-4 py-3 sm:px-5 sm:py-3.5"
        >
          <div class="min-w-0 flex-1">
            <div class="text-[13px] font-medium text-foreground">
              {currentProviderMeta.name} API Key
            </div>
            <p class="mt-0.5 text-xs text-muted-foreground">
              用于调用结构化搜索引擎的鉴权密钥，安全保存在本地 secrets 中
            </p>
          </div>
          <div class="shrink-0 sm:w-72">
            <input
              aria-label="API Key"
              class="h-9 w-full rounded-lg border border-border/80 bg-background px-3 text-xs text-foreground placeholder:text-muted-foreground/50 focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40"
              type="password"
              autocomplete="off"
              placeholder={config?.configured && config?.provider === selectedProvider
                ? "已配置（留空保持不变）"
                : "请输入 API Key..."}
              bind:value={apiKey}
            />
          </div>
        </div>
      {/if}

      <!-- Max Results Row -->
      <div
        class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 px-4 py-3 sm:px-5 sm:py-3.5"
      >
        <div class="min-w-0 flex-1">
          <div class="text-[13px] font-medium text-foreground">最大返回搜索结果数</div>
          <p class="mt-0.5 text-xs text-muted-foreground">
            每次搜索提取的高相关性网页数量（推荐 3~8 条，上限 10 条）
          </p>
        </div>
        <div class="shrink-0 w-28">
          <input
            aria-label="最大返回搜索结果数"
            class="h-9 w-full rounded-lg border border-border/80 bg-background px-3 text-xs text-foreground focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40"
            type="number"
            min="1"
            max="10"
            bind:value={maxResults}
          />
        </div>
      </div>
    {/if}
  </div>

  <!-- Group 2: SSRF & Whitelist Card -->
  {#if showSharedConfiguration}
    <div class="rounded-xl border border-border/70 bg-card p-4 sm:p-5 shadow-xs space-y-2.5">
      <div class="flex items-center justify-between">
        <div>
          <span class="text-[13px] font-medium text-foreground"
            >内网域名与 IP 白名单（SSRF 放行规则）</span
          >
          <p class="mt-0.5 text-xs text-muted-foreground">
            防范 SSRF 探测；默认拦截 10.x、192.168.x、172.16-31.x 私网地址。
          </p>
        </div>
        <span class="text-[10px] text-muted-foreground font-mono">每行或逗号分隔</span>
      </div>

      <textarea
        id="allowed-hosts-input"
        aria-label="内网域名与 IP 白名单"
        class="h-20 w-full rounded-lg border border-border/80 bg-muted/20 p-2.5 font-mono text-xs leading-relaxed text-foreground placeholder:text-muted-foreground/40 focus:bg-background focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/40"
        placeholder="例如：&#10;jira.virtueit.net&#10;*.virtueit.net&#10;10.73.10.234&#10;10.73.0.0/16"
        bind:value={allowedHostsText}
      ></textarea>

      <p class="text-[11px] text-muted-foreground/80 leading-relaxed">
        如需访问企业内网服务（如 Jira、GitLab、私有 API
        或本地研发端口），请在此配置放行主机名、通配符（<code>*.company.com</code>）或
        IP/CIDR。云服务元数据 IP（169.254.169.254）始终保持严格拦截。
      </p>
    </div>

    <!-- Health check result -->
    {#if testing || health}
      <div
        class="rounded-xl border border-border/70 bg-muted/20 px-4 py-3 text-xs"
        role="status"
        aria-live="polite"
      >
        <div class="flex items-center justify-between gap-3">
          <span class="font-medium text-foreground flex items-center gap-1.5">
            <span
              class="h-2 w-2 rounded-full {health?.status === 'healthy'
                ? 'bg-emerald-500'
                : testing
                  ? 'bg-amber-500 animate-pulse'
                  : 'bg-red-500'}"
            ></span>
            健康检查 ({currentProviderMeta.name})
          </span>
          <span
            class="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium text-muted-foreground"
          >
            {testing ? "检查中…" : health ? statusLabel(health.status) : "等待中"}
          </span>
        </div>
        <p class="mt-1 text-muted-foreground">
          {#if testing}
            正在向搜索供应商发起实时检索探针，通常耗时 1~5 秒…
          {:else if health}
            {health.message}{#if health.latencyMs}
              · {health.latencyMs} ms{/if}
          {/if}
        </p>
      </div>
    {/if}

    <!-- Bottom Action Bar -->
    <div class="flex flex-wrap items-center justify-between gap-3 pt-1">
      <div>
        {#if config?.configured && currentProviderMeta.authKind !== "anonymous"}
          <button
            type="button"
            class="text-xs text-red-500 hover:text-red-600 hover:underline transition-colors disabled:opacity-50"
            disabled={clearing || saving || testing}
            onclick={() => void clearCredential()}
          >
            {clearing ? "清除中…" : "清除已存凭据"}
          </button>
        {/if}
      </div>

      <div class="flex items-center gap-2.5">
        <button
          type="button"
          class="h-8 rounded-lg border border-border/80 bg-background px-3.5 text-xs font-medium text-foreground hover:bg-muted/60 transition-colors disabled:opacity-50"
          disabled={testing || saving}
          onclick={() => void test()}
        >
          {testing ? "测试中…" : "测试当前 Provider"}
        </button>
        <button
          type="button"
          class="h-8 rounded-lg bg-primary px-4 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50 shadow-xs"
          disabled={saving || testing}
          onclick={() => void save()}
        >
          {saving ? "保存中…" : "保存配置"}
        </button>
      </div>
    </div>
  {/if}

  <!-- Group 3: Native Web Reader Note (Subtle Info) -->
  <div class="rounded-xl border border-border/50 bg-muted/20 p-3.5 text-xs">
    <div class="flex items-start gap-2.5">
      <div class="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center text-muted-foreground">
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <circle cx="12" cy="12" r="10" />
          <path d="M12 16v-4" />
          <path d="M12 8h.01" />
        </svg>
      </div>
      <div class="space-y-0.5">
        <div class="font-medium text-foreground text-xs">原生安全抓取与网页解析 (免 API Key)</div>
        <p class="text-[11px] leading-relaxed text-muted-foreground">
          页面抓取（<code>web_open</code>）、正文提取（<code>web_extract</code
          >）与引用由内置安全引擎本地直接执行，无需任何商业第三方 API Key。凭据仅保存在
          <code>~/.agentcabin/browser/secrets.json</code>。
        </p>
      </div>
    </div>
  </div>
</div>
