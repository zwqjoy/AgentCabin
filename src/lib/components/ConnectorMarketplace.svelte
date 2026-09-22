<script lang="ts">
  import type { AppConnection, ConnectorCatalogItem, ConnectionStatus } from "$lib/types/work";
  import type { ConnectorBinding } from "$lib/api";
  import ConnectorAuthModal from "./ConnectorAuthModal.svelte";

  interface Props {
    catalog: ConnectorCatalogItem[];
    connections: AppConnection[];
    source?: "discover" | "enabled";
    bindings?: ConnectorBinding[];
    onConnect: (packageId: string) => Promise<void>;
    onDisconnect: (packageId: string) => Promise<void>;
    onToggle?: (item: ConnectorCatalogItem, enabled: boolean) => Promise<void>;
  }

  let {
    catalog,
    connections,
    source = "discover",
    bindings = [],
    onConnect,
    onDisconnect,
    onToggle,
  }: Props = $props();

  let query = $state("");
  let selectedCategory = $state("all");
  let busyId = $state("");
  let error = $state("");
  let authModalItem = $state<ConnectorCatalogItem | null>(null);
  let authModalOpen = $state(false);

  interface CategoryMeta {
    id: string;
    label: string;
    matcher: (categories: string[]) => boolean;
  }

  const MAIN_CATEGORIES: CategoryMeta[] = [
    { id: "all", label: "全部", matcher: () => true },
    {
      id: "communication",
      label: "协作沟通",
      matcher: (cats) => cats.some((c) => ["communication", "email", "messaging"].includes(c)),
    },
    {
      id: "development",
      label: "开发代码",
      matcher: (cats) => cats.some((c) => ["development", "code"].includes(c)),
    },
    {
      id: "productivity",
      label: "效率日程",
      matcher: (cats) => cats.some((c) => ["productivity", "calendar"].includes(c)),
    },
    {
      id: "knowledge",
      label: "文档知识",
      matcher: (cats) => cats.some((c) => ["knowledge", "notes"].includes(c)),
    },
    {
      id: "storage",
      label: "云端存储",
      matcher: (cats) => cats.some((c) => ["storage", "files"].includes(c)),
    },
  ];

  function connectionFor(item: ConnectorCatalogItem): AppConnection | undefined {
    const conn = connections.find(
      (connection) => connection.appId.toLowerCase() === item.packageId.toLowerCase(),
    );
    if (conn && conn.provider && conn.provider !== "native") {
      return undefined;
    }
    return conn;
  }

  function statusFor(item: ConnectorCatalogItem): ConnectionStatus {
    return connectionFor(item)?.status ?? item.connectionStatus;
  }

  function accountCountFor(item: ConnectorCatalogItem): number {
    return connectionFor(item)?.accounts.length ?? item.accountCount;
  }

  function isConnected(item: ConnectorCatalogItem): boolean {
    const status = statusFor(item);
    const count = accountCountFor(item);
    return status === "connected" && count > 0;
  }

  function isEnabled(item: ConnectorCatalogItem): boolean {
    const b = bindings.find(
      (binding) => binding.connectorId.trim().toLowerCase() === item.packageId.trim().toLowerCase(),
    );
    return b?.enabled ?? true;
  }

  async function toggle(item: ConnectorCatalogItem) {
    if (busyId) return;
    const next = !isEnabled(item);
    busyId = `toggle:${item.packageId}`;
    error = "";
    try {
      await onToggle?.(item, next);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }

  function statusLabel(item: ConnectorCatalogItem): string {
    const status = statusFor(item);
    if (status === "connected")
      return `已连接${accountCountFor(item) ? ` · ${accountCountFor(item)} 个账号` : ""}`;
    if (status === "pending") return "认证中";
    if (status === "expired") return "授权已过期";
    if (status === "error") return "连接异常";
    return "未连接";
  }

  function statusClass(item: ConnectorCatalogItem): string {
    const status = statusFor(item);
    if (status === "connected") {
      return "border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300";
    }
    if (status === "pending") {
      return "border-amber-500/20 bg-amber-500/10 text-amber-700 dark:text-amber-300";
    }
    if (status === "expired" || status === "error") {
      return "border-orange-500/20 bg-orange-500/10 text-orange-700 dark:text-orange-300";
    }
    return "border-border bg-muted/60 text-muted-foreground";
  }

  function iconClass(packageId: string): string {
    switch (packageId.toLowerCase()) {
      case "feishu":
        return "from-cyan-500/20 to-blue-500/10 text-cyan-600 border-cyan-500/25";
      case "gmail":
        return "from-red-500/20 to-red-500/5 text-red-500 border-red-500/25";
      case "googlecalendar":
        return "from-blue-500/20 to-blue-500/5 text-blue-600 border-blue-500/25";
      case "googledrive":
        return "from-amber-500/20 to-amber-500/5 text-amber-600 border-amber-500/25";
      case "slack":
        return "from-purple-500/20 to-purple-500/5 text-purple-600 border-purple-500/25";
      case "github":
        return "from-zinc-500/20 to-zinc-500/5 text-zinc-700 border-zinc-500/25 dark:text-zinc-200";
      case "notion":
        return "from-neutral-500/20 to-neutral-500/5 text-neutral-700 border-neutral-500/25 dark:text-neutral-200";
      default:
        return "from-primary/20 to-primary/5 text-primary border-primary/25";
    }
  }

  function iconText(packageId: string): string {
    switch (packageId.toLowerCase()) {
      case "feishu":
        return "飞";
      case "gmail":
        return "M";
      case "googlecalendar":
        return "CAL";
      case "googledrive":
        return "DRV";
      case "slack":
        return "#";
      case "github":
        return "GIT";
      case "notion":
        return "N";
      default:
        return packageId.slice(0, 2).toUpperCase();
    }
  }

  function runtimeLabel(runtime: string): string {
    if (runtime === "mcp") return "MCP";
    if (runtime === "cli") return "CLI";
    return "Skill";
  }

  function authLabel(item: ConnectorCatalogItem): string {
    if (item.auth.kind === "oauth2") return "OAuth";
    if (item.auth.kind === "api_key") return "API Key";
    if (item.auth.kind === "cli") return "CLI 登录";
    return "无需认证";
  }

  async function run(item: ConnectorCatalogItem, action: () => Promise<void>) {
    if (busyId) return;
    busyId = item.packageId;
    error = "";
    try {
      await action();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }

  async function disconnect(item: ConnectorCatalogItem) {
    if (busyId) return;
    try {
      const { confirm } = await import("$lib/platform/dialog");
      const ok = await confirm(`确定断开“${item.displayName}”的全部账号？`, {
        title: "断开连接器",
        kind: "warning",
      });
      if (!ok) return;
    } catch {
      if (!window.confirm(`确定断开“${item.displayName}”的全部账号？`)) return;
    }
    await run(item, () => onDisconnect(item.packageId));
  }

  let totalConnectedCount = $derived(catalog.filter(isConnected).length);

  // Filter available categories that have matching items
  let activeCategories = $derived.by(() => {
    const baseItems = source === "enabled" ? catalog.filter(isConnected) : catalog;
    return MAIN_CATEGORIES.filter((cat) => {
      if (cat.id === "all") return true;
      return baseItems.some((item) => cat.matcher(item.categories));
    });
  });

  let visibleItems = $derived.by(() => {
    const normalized = query.trim().toLowerCase();
    const currentCatMeta = MAIN_CATEGORIES.find((c) => c.id === selectedCategory);

    return catalog.filter((item) => {
      if (source === "enabled" && !isConnected(item)) return false;
      if (currentCatMeta && !currentCatMeta.matcher(item.categories)) return false;
      if (!normalized) return true;
      const haystack = [
        item.packageId,
        item.displayName,
        item.description,
        ...item.categories,
        ...item.capabilities,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(normalized);
    });
  });
</script>

<section class="rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5">
  <div class="flex flex-wrap items-start justify-between gap-4">
    <div>
      <div class="flex items-center gap-2">
        <span class="h-2 w-2 rounded-full bg-emerald-500"></span>
        <h2 class="text-sm font-semibold text-foreground">
          {source === "enabled" ? "已连接的工作应用" : "精选工作应用 (Marketplace)"}
        </h2>
        <span
          class="rounded-full bg-emerald-500/10 px-2 py-0.5 text-[11px] text-emerald-700 dark:text-emerald-300"
        >
          {source === "enabled" ? totalConnectedCount : catalog.length}
        </span>
      </div>
      <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
        {source === "enabled"
          ? "当前已授权连接的工作应用，凭据加密保存在 Host 端，Pi 只使用受控工具。"
          : "一键授权连接常用 SaaS 工作应用。凭据留在 Host 端，受控注入 MCP 运行时。"}
      </p>
    </div>
    <div class="flex items-center gap-2 text-[11px] text-muted-foreground">
      <span class="rounded-lg bg-muted px-2.5 py-1.5">内置 Registry</span>
      <span
        class="rounded-lg bg-emerald-500/10 px-2.5 py-1.5 font-medium text-emerald-700 dark:text-emerald-300"
      >
        {totalConnectedCount} 个已连接
      </span>
    </div>
  </div>

  {#if error}
    <div
      class="mt-3 rounded-lg border border-red-500/20 bg-red-500/5 px-3 py-2 text-xs text-red-600 dark:text-red-300"
      role="alert"
    >
      {error}
    </div>
  {/if}

  {#if source === "enabled" && totalConnectedCount === 0}
    <div
      class="mt-4 flex flex-col items-center justify-center rounded-xl border border-dashed border-border/70 p-10 text-center"
    >
      <div
        class="flex h-10 w-10 items-center justify-center rounded-full bg-muted/80 text-muted-foreground"
      >
        <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <rect width="18" height="18" x="3" y="3" rx="2" />
          <path d="M9 12h6" />
          <path d="M12 9v6" />
        </svg>
      </div>
      <p class="mt-3 text-xs font-medium text-foreground">还没有已连接的工作应用</p>
      <p class="mt-1 max-w-sm text-[11px] text-muted-foreground">
        切换至上方的“发现”视图，即可一键授权连接 Gmail、GitHub、Slack、Notion 等常用应用。
      </p>
    </div>
  {:else}
    <div class="mt-4 flex flex-wrap items-center justify-between gap-3">
      <label class="relative block min-w-[220px] flex-1 sm:max-w-sm">
        <span class="sr-only">搜索连接器</span>
        <svg
          class="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          aria-hidden="true"
        >
          <circle cx="11" cy="11" r="8"></circle>
          <path d="m21 21-4.3-4.3"></path>
        </svg>
        <input
          type="search"
          bind:value={query}
          placeholder="搜索连接器或功能（如 GitHub、邮件、日程…）"
          class="min-h-10 w-full rounded-lg border border-border bg-background py-2 pl-9 pr-3 text-xs text-foreground placeholder:text-muted-foreground focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring"
        />
      </label>
      {#if activeCategories.length > 1}
        <div class="flex flex-wrap items-center gap-1">
          {#each activeCategories as category}
            <button
              type="button"
              class="rounded-md px-2.5 py-1.5 text-[11px] font-medium transition-colors {selectedCategory ===
              category.id
                ? 'bg-foreground text-background shadow-xs'
                : 'bg-muted/60 text-muted-foreground hover:bg-muted hover:text-foreground'}"
              onclick={() => (selectedCategory = category.id)}
            >
              {category.label}
            </button>
          {/each}
        </div>
      {/if}
    </div>

    {#if visibleItems.length === 0}
      <div
        class="mt-4 rounded-xl border border-dashed border-border/70 px-4 py-10 text-center text-xs text-muted-foreground"
      >
        没有匹配的连接器应用。
      </div>
    {:else}
      <div class="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-3">
        {#each visibleItems as item (item.packageId)}
          {@const connected = isConnected(item)}
          {@const status = statusFor(item)}
          <article
            class="group flex min-h-[218px] flex-col justify-between rounded-xl border border-border/70 bg-background/70 p-4 transition-all duration-200 hover:-translate-y-0.5 hover:border-border hover:shadow-md"
          >
            <div>
              <div class="flex items-start justify-between gap-3">
                <div class="flex min-w-0 items-center gap-3">
                  <div
                    class="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl border bg-gradient-to-br text-[11px] font-bold tracking-wider shadow-inner {iconClass(
                      item.packageId,
                    )}"
                  >
                    {iconText(item.packageId)}
                  </div>
                  <div class="min-w-0">
                    <h3 class="truncate text-sm font-semibold text-foreground">
                      {item.displayName}
                    </h3>
                    <div class="mt-1 flex flex-wrap gap-1.5">
                      {#each item.runtimes as runtime}
                        <span
                          class="rounded bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary"
                          >{runtimeLabel(runtime)}</span
                        >
                      {/each}
                      <span class="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground"
                        >{authLabel(item)}</span
                      >
                    </div>
                  </div>
                </div>
                {#if connected}
                  <span
                    class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-emerald-500/20 bg-emerald-500/10 text-sm text-emerald-600 dark:text-emerald-300"
                    aria-label={`${item.displayName} 已连接`}
                    title="已连接"
                  >
                    <span aria-hidden="true">✓</span>
                  </span>
                {:else}
                  <button
                    type="button"
                    class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-border bg-card text-muted-foreground transition-colors hover:border-primary/40 hover:bg-primary/10 hover:text-primary disabled:cursor-wait disabled:opacity-60"
                    aria-label={`连接 ${item.displayName}`}
                    title={`连接 ${item.displayName}`}
                    disabled={Boolean(busyId)}
                    onclick={() => {
                      authModalItem = item;
                      authModalOpen = true;
                    }}
                  >
                    {#if busyId === item.packageId}
                      <span
                        class="h-3.5 w-3.5 animate-spin rounded-full border-2 border-current/25 border-t-current"
                      ></span>
                    {:else}
                      <span aria-hidden="true" class="text-lg leading-none">+</span>
                    {/if}
                  </button>
                {/if}
              </div>

              <p class="mt-3 line-clamp-3 text-xs leading-5 text-muted-foreground">
                {item.description}
              </p>

              {#if item.capabilities.length > 0}
                <div class="mt-3 flex flex-wrap gap-1.5">
                  {#each item.capabilities.slice(0, 3) as capability}
                    <span
                      class="rounded-md border border-border/50 bg-muted/30 px-2 py-0.5 text-[10px] text-muted-foreground"
                      >{capability.replaceAll("_", " ")}</span
                    >
                  {/each}
                </div>
              {/if}
            </div>

            <div
              class="mt-4 flex flex-wrap items-center justify-between gap-2 border-t border-border/50 pt-3"
            >
              <div class="flex flex-wrap items-center gap-1.5">
                <span
                  class="inline-flex items-center gap-1.5 rounded-full border px-2 py-1 text-[10px] font-medium {statusClass(
                    item,
                  )}"
                >
                  <span
                    class="h-1.5 w-1.5 rounded-full {status === 'connected'
                      ? 'bg-emerald-500'
                      : status === 'pending'
                        ? 'animate-pulse bg-amber-500'
                        : 'bg-current/50'}"
                  ></span>
                  {statusLabel(item)}
                </span>
                {#if connected}
                  <div class="flex items-center gap-1">
                    <button
                      type="button"
                      class="inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 text-[9px] font-medium transition-all disabled:opacity-50 {isEnabled(
                        item,
                      )
                        ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 font-semibold'
                        : 'border-border/70 bg-background/50 text-muted-foreground hover:bg-muted hover:text-foreground'}"
                      disabled={busyId !== ""}
                      onclick={(e) => {
                        e.stopPropagation();
                        void toggle(item);
                      }}
                      title={isEnabled(item) ? "全局停用连接器" : "全局启用连接器"}
                    >
                      <span
                        class="h-1.5 w-1.5 rounded-full {isEnabled(item)
                          ? 'bg-emerald-500'
                          : 'bg-muted-foreground/40'}"
                      ></span>
                      全局 {isEnabled(item) ? "开" : "关"}
                    </button>
                  </div>
                {/if}
              </div>
              <div class="flex items-center gap-2">
                {#if item.documentationUrl}
                  <a
                    href={item.documentationUrl}
                    target="_blank"
                    rel="noreferrer"
                    class="text-[10px] text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
                  >
                    文档
                  </a>
                {/if}
                <button
                  type="button"
                  class="rounded-md px-2.5 py-1 text-[11px] font-medium transition-colors {connected
                    ? 'border border-border bg-card text-muted-foreground hover:border-red-500/30 hover:bg-red-500/10 hover:text-red-600 dark:hover:text-red-400'
                    : 'bg-primary text-primary-foreground hover:bg-primary/90'}"
                  disabled={Boolean(busyId)}
                  onclick={() => {
                    if (connected) {
                      void disconnect(item);
                    } else {
                      authModalItem = item;
                      authModalOpen = true;
                    }
                  }}
                >
                  {#if connected}
                    断开连接
                  {:else if status === "pending"}
                    继续配置
                  {:else}
                    连接
                  {/if}
                </button>
              </div>
            </div>
          </article>
        {/each}
      </div>
    {/if}
  {/if}

  <ConnectorAuthModal
    item={authModalItem}
    open={authModalOpen}
    onClose={() => {
      authModalOpen = false;
      authModalItem = null;
    }}
    onConnected={async () => {
      if (authModalItem) {
        await onConnect(authModalItem.packageId);
      }
    }}
  />
</section>
