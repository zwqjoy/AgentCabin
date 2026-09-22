<script lang="ts">
  import { onMount } from "svelte";
  import { getWorkAppsProviderConfig, saveWorkAppsProviderConfig } from "$lib/api/work";
  import type { AppCatalogItem, AppConnection, ComposioProviderConfig } from "$lib/types/work";
  import WorkAppCard from "./WorkAppCard.svelte";

  type CatalogSource = "discover" | "enabled";

  interface Props {
    catalog: AppCatalogItem[];
    connections: AppConnection[];
    source?: CatalogSource;
    onConnect: (appId: string) => Promise<void>;
    onDisconnect: (appId: string) => Promise<void>;
    onDisconnectAccount: (appId: string, accountId: string) => Promise<void>;
    onSetDefaultAccount?: (appId: string, accountId: string) => Promise<void>;
  }

  let {
    catalog,
    connections,
    source = "discover",
    onConnect,
    onDisconnect,
    onDisconnectAccount,
    onSetDefaultAccount,
  }: Props = $props();

  let query = $state("");
  let selectedCategory = $state<string>("all");
  let providerConfig = $state<ComposioProviderConfig | null>(null);
  let showConfigDrawer = $state(false);
  let apiKeyInput = $state("");
  let savingKey = $state(false);
  let keyMessage = $state("");
  let keyError = $state("");

  onMount(async () => {
    try {
      providerConfig = await getWorkAppsProviderConfig();
    } catch {
      // ignore
    }
  });

  async function handleSaveKey() {
    if (!apiKeyInput.trim() || savingKey) return;
    savingKey = true;
    keyError = "";
    keyMessage = "";
    try {
      await saveWorkAppsProviderConfig(apiKeyInput.trim());
      providerConfig = await getWorkAppsProviderConfig();
      keyMessage = "Composio API Key 保存成功！现在可以开始连接应用。";
      apiKeyInput = "";
      setTimeout(() => {
        showConfigDrawer = false;
        keyMessage = "";
      }, 1500);
    } catch (e) {
      keyError = e instanceof Error ? e.message : String(e);
    } finally {
      savingKey = false;
    }
  }

  let connectedAppIds = $derived(
    new Set(
      connections
        .filter((c) => c.status === "connected" || c.accounts.length > 0)
        .map((c) => c.appId.toLowerCase()),
    ),
  );

  let allCategories = $derived.by(() => {
    const cats = new Set<string>();
    for (const app of catalog) {
      for (const cat of app.categories) {
        cats.add(cat);
      }
    }
    return Array.from(cats);
  });

  let visibleApps = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return catalog.filter((app) => {
      const isConnected = connectedAppIds.has(app.appId.toLowerCase());
      if (source === "enabled" && !isConnected) return false;
      if (selectedCategory !== "all" && !app.categories.includes(selectedCategory)) return false;
      if (!q) return true;
      const haystack = [
        app.appId,
        app.displayName,
        app.description,
        ...app.categories,
        ...app.capabilities,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(q);
    });
  });

  function getConnectionForApp(appId: string): AppConnection | undefined {
    return connections.find((c) => c.appId.toLowerCase() === appId.toLowerCase());
  }
</script>

<section class="rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5">
  <!-- Header -->
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div>
      <div class="flex items-center gap-2">
        <span class="h-2 w-2 rounded-full bg-indigo-500"></span>
        <h2 class="text-sm font-semibold text-foreground">应用连接</h2>
        <span
          class="rounded-full bg-indigo-500/10 px-2 py-0.5 text-[11px] text-indigo-700 dark:text-indigo-300 font-medium"
        >
          {catalog.length} 个应用
        </span>
      </div>
      <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
        {source === "discover"
          ? "安全连接外部常用 SaaS 应用。凭据仅在 Host 端保管，Work Runtime 不会直接读取。"
          : "已连接的外部应用，可以在任务中供 Agent 直接调用相关工具。"}
      </p>
    </div>

    <div class="flex items-center gap-2 text-[11px] text-muted-foreground">
      <button
        type="button"
        class="flex items-center gap-1.5 rounded-lg border border-border/80 px-2.5 py-1.5 font-medium text-foreground hover:bg-accent transition-colors"
        onclick={() => (showConfigDrawer = !showConfigDrawer)}
      >
        <span
          class="h-2 w-2 rounded-full {providerConfig?.hasApiKey
            ? 'bg-emerald-500'
            : 'bg-amber-500'}"
        ></span>
        <span
          >{providerConfig?.hasApiKey
            ? "Composio Provider (已配置)"
            : "配置 Composio API Key"}</span
        >
      </button>
      <span class="rounded-lg bg-muted px-2.5 py-1.5">{catalog.length} 个可用</span>
      <span
        class="rounded-lg bg-indigo-500/10 px-2.5 py-1.5 text-indigo-700 dark:text-indigo-300 font-medium"
      >
        {connectedAppIds.size} 个已连接
      </span>
    </div>
  </div>

  <!-- Provider Settings Drawer / Banner -->
  {#if showConfigDrawer || (providerConfig && !providerConfig.hasApiKey)}
    <div class="mt-3.5 rounded-xl border border-indigo-500/30 bg-indigo-500/5 p-3.5 text-xs">
      <div class="flex items-start justify-between gap-3">
        <div>
          <div class="flex items-center gap-2">
            <span class="font-semibold text-foreground">Composio Provider 配置</span>
            <span
              class="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground font-mono"
            >
              Host 私有文件 (0600)
            </span>
          </div>
          <p class="mt-1 text-muted-foreground">
            Composio 为 Agent 提供 Gmail、Calendar、Slack、GitHub 等应用的 OAuth 授权服务。 获取 API
            Key: <a
              href="https://app.composio.dev/settings"
              target="_blank"
              rel="noreferrer"
              class="text-primary hover:underline">app.composio.dev ↗</a
            >
          </p>
        </div>
        {#if providerConfig?.hasApiKey && showConfigDrawer}
          <button
            type="button"
            class="text-muted-foreground hover:text-foreground text-xs"
            onclick={() => (showConfigDrawer = false)}
          >
            收起
          </button>
        {/if}
      </div>

      <div class="mt-3 flex flex-wrap items-center gap-2">
        <input
          type="password"
          bind:value={apiKeyInput}
          placeholder={providerConfig?.hasApiKey
            ? "•••••••••••••••••••••••• (输入新 Key 以更新)"
            : "输入 Composio API Key (例如 comp_...)"}
          class="min-w-[280px] flex-1 rounded-lg border border-border bg-background px-3 py-1.5 text-xs text-foreground placeholder:text-muted-foreground focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring"
        />
        <button
          type="button"
          class="rounded-lg bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
          onclick={handleSaveKey}
          disabled={!apiKeyInput.trim() || savingKey}
        >
          {savingKey ? "保存中..." : "保存 API Key"}
        </button>
      </div>

      {#if keyMessage}
        <div class="mt-2 text-xs font-medium text-emerald-600 dark:text-emerald-400">
          {keyMessage}
        </div>
      {/if}
      {#if keyError}
        <div class="mt-2 text-xs font-medium text-red-500">{keyError}</div>
      {/if}
    </div>
  {/if}

  <!-- Search & Category Filter bar -->
  <div class="mt-4 flex flex-wrap items-center justify-between gap-3">
    <label class="relative block w-full max-w-sm">
      <span class="sr-only">搜索应用</span>
      <svg
        class="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <circle cx="11" cy="11" r="8" />
        <path d="m21 21-4.3-4.3" />
      </svg>
      <input
        type="search"
        bind:value={query}
        placeholder="搜索应用名称、描述或能力..."
        class="w-full rounded-lg border border-border bg-background py-1.5 pl-9 pr-3 text-xs text-foreground placeholder:text-muted-foreground focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring"
      />
    </label>

    <!-- Categories filter pills -->
    {#if allCategories.length > 0}
      <div class="flex flex-wrap items-center gap-1">
        <button
          type="button"
          class="rounded-md px-2.5 py-1 text-xs transition-colors {selectedCategory === 'all'
            ? 'bg-primary text-primary-foreground font-medium'
            : 'bg-muted/50 text-muted-foreground hover:bg-muted hover:text-foreground'}"
          onclick={() => (selectedCategory = "all")}
        >
          全部
        </button>
        {#each allCategories as cat}
          <button
            type="button"
            class="rounded-md px-2.5 py-1 text-xs capitalize transition-colors {selectedCategory ===
            cat
              ? 'bg-primary text-primary-foreground font-medium'
              : 'bg-muted/50 text-muted-foreground hover:bg-muted hover:text-foreground'}"
            onclick={() => (selectedCategory = cat)}
          >
            {cat}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <!-- Apps Grid -->
  {#if visibleApps.length === 0}
    <div class="mt-8 rounded-xl border border-dashed border-border/80 p-8 text-center">
      <p class="text-sm font-medium text-foreground">没有找到匹配的应用</p>
      <p class="mt-1 text-xs text-muted-foreground">
        {source === "enabled"
          ? "当前暂无已连接的应用，请切换到“发现”视图连接应用。"
          : "请尝试使用其他关键词搜索。"}
      </p>
    </div>
  {:else}
    <div class="mt-4 grid grid-cols-1 gap-3.5 sm:grid-cols-2 lg:grid-cols-3">
      {#each visibleApps as app (app.appId)}
        <WorkAppCard
          {app}
          connection={getConnectionForApp(app.appId)}
          {onConnect}
          {onDisconnect}
          {onDisconnectAccount}
          {onSetDefaultAccount}
        />
      {/each}
    </div>
  {/if}
</section>
