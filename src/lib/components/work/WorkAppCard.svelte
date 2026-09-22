<script lang="ts">
  import type { AppCatalogItem, AppConnection } from "$lib/types/work";
  import WorkConnectedAccounts from "./WorkConnectedAccounts.svelte";

  interface Props {
    app: AppCatalogItem;
    connection?: AppConnection;
    defaultAccountId?: string | null;
    onConnect: (appId: string) => Promise<void>;
    onDisconnect: (appId: string) => Promise<void>;
    onDisconnectAccount: (appId: string, accountId: string) => Promise<void>;
    onSetDefaultAccount?: (appId: string, accountId: string) => Promise<void>;
  }

  let {
    app,
    connection,
    defaultAccountId = null,
    onConnect,
    onDisconnect,
    onDisconnectAccount,
    onSetDefaultAccount,
  }: Props = $props();

  let connecting = $state(false);
  let disconnecting = $state(false);
  let showAccounts = $state(false);
  let localError = $state("");

  let isConnected = $derived(connection?.status === "connected");
  let isPending = $derived(connection?.status === "pending");
  let isExpired = $derived(connection?.status === "expired");
  let accounts = $derived(connection?.accounts ?? []);

  function getAppColor(appId: string): string {
    switch (appId.toLowerCase()) {
      case "gmail":
        return "from-red-500/20 to-red-500/5 text-red-500 border-red-500/30";
      case "googlecalendar":
        return "from-blue-500/20 to-blue-500/5 text-blue-500 border-blue-500/30";
      case "googledrive":
        return "from-amber-500/20 to-amber-500/5 text-amber-500 border-amber-500/30";
      case "slack":
        return "from-purple-500/20 to-purple-500/5 text-purple-500 border-purple-500/30";
      case "github":
        return "from-zinc-500/20 to-zinc-500/5 text-zinc-300 border-zinc-500/30";
      case "notion":
        return "from-neutral-500/20 to-neutral-500/5 text-neutral-300 border-neutral-500/30";
      default:
        return "from-primary/20 to-primary/5 text-primary border-primary/30";
    }
  }

  function getAppIconText(appId: string): string {
    switch (appId.toLowerCase()) {
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
        return appId.slice(0, 2).toUpperCase();
    }
  }

  async function handleConnect() {
    if (connecting) return;
    connecting = true;
    localError = "";
    try {
      await onConnect(app.appId);
    } catch (e) {
      localError = e instanceof Error ? e.message : String(e);
    } finally {
      connecting = false;
    }
  }

  async function handleDisconnectAll() {
    if (disconnecting) return;
    try {
      const { confirm } = await import("$lib/platform/dialog");
      const ok = await confirm(`确定断开应用“${app.displayName}”的所有连接？`, {
        title: "断开应用",
        kind: "warning",
      });
      if (!ok) return;
    } catch {
      if (!window.confirm(`确定断开应用“${app.displayName}”的所有连接？`)) return;
    }

    disconnecting = true;
    localError = "";
    try {
      await onDisconnect(app.appId);
      showAccounts = false;
    } catch (e) {
      localError = e instanceof Error ? e.message : String(e);
    } finally {
      disconnecting = false;
    }
  }
</script>

<div
  class="flex flex-col justify-between rounded-xl border border-border/70 bg-card p-4 shadow-sm transition-all hover:border-border hover:shadow-md"
>
  <div>
    <!-- Top header: Icon, Name, Status Badge -->
    <div class="flex items-start justify-between gap-3">
      <div class="flex items-center gap-3">
        <div
          class="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl border bg-gradient-to-br font-bold text-xs tracking-wider shadow-inner {getAppColor(
            app.appId,
          )}"
        >
          {getAppIconText(app.appId)}
        </div>
        <div>
          <h3 class="text-sm font-semibold text-foreground flex items-center gap-2">
            {app.displayName}
          </h3>
          <div class="flex flex-wrap gap-1.5 mt-1">
            {#each app.categories as cat}
              <span
                class="rounded bg-muted/70 px-1.5 py-0.5 text-[10px] text-muted-foreground font-medium uppercase"
              >
                {cat}
              </span>
            {/each}
          </div>
        </div>
      </div>

      <!-- Status Indicator -->
      <div>
        {#if isConnected}
          <span
            class="inline-flex items-center gap-1.5 rounded-full bg-emerald-500/10 border border-emerald-500/20 px-2.5 py-0.5 text-[11px] font-medium text-emerald-600 dark:text-emerald-400"
          >
            <span class="h-1.5 w-1.5 rounded-full bg-emerald-500"></span>
            已连接 {accounts.length > 0 ? `(${accounts.length})` : ""}
          </span>
        {:else if isPending}
          <span
            class="inline-flex items-center gap-1.5 rounded-full bg-amber-500/10 border border-amber-500/20 px-2.5 py-0.5 text-[11px] font-medium text-amber-600 dark:text-amber-400"
          >
            <span class="h-1.5 w-1.5 rounded-full bg-amber-500 animate-pulse"></span>
            认证中
          </span>
        {:else if isExpired}
          <span
            class="inline-flex items-center gap-1.5 rounded-full bg-orange-500/10 border border-orange-500/20 px-2.5 py-0.5 text-[11px] font-medium text-orange-600 dark:text-orange-400"
          >
            <span class="h-1.5 w-1.5 rounded-full bg-orange-500"></span>
            授权已过期
          </span>
        {:else}
          <span
            class="inline-flex items-center gap-1.5 rounded-full bg-muted border border-border px-2.5 py-0.5 text-[11px] font-medium text-muted-foreground"
          >
            未连接
          </span>
        {/if}
      </div>
    </div>

    <!-- App Description -->
    <p class="mt-3 text-xs leading-relaxed text-muted-foreground line-clamp-2">
      {app.description}
    </p>

    <!-- Capabilities tags -->
    {#if app.capabilities.length > 0}
      <div class="mt-3 flex flex-wrap gap-1.5">
        {#each app.capabilities as cap}
          <span
            class="rounded-md border border-border/50 bg-muted/30 px-2 py-0.5 text-[10px] text-muted-foreground font-mono"
          >
            {cap}
          </span>
        {/each}
      </div>
    {/if}

    {#if localError}
      <div
        class="mt-2.5 rounded-md border border-red-400/20 bg-red-400/5 px-2.5 py-1.5 text-xs text-red-500"
      >
        {localError}
      </div>
    {/if}
  </div>

  <!-- Bottom Actions -->
  <div class="mt-4 pt-3 border-t border-border/50 flex items-center justify-between gap-2">
    <div>
      {#if accounts.length > 0}
        <button
          type="button"
          class="text-xs text-muted-foreground hover:text-foreground underline underline-offset-2 transition-colors"
          onclick={() => (showAccounts = !showAccounts)}
        >
          {showAccounts ? "收起账号列表" : `管理账号 (${accounts.length})`}
        </button>
      {/if}
    </div>

    <div class="flex items-center gap-2">
      {#if isConnected || accounts.length > 0}
        <button
          type="button"
          class="rounded-lg border border-border px-3 py-1.5 text-xs font-medium text-muted-foreground hover:border-red-500/40 hover:bg-red-500/10 hover:text-red-500 transition-colors disabled:opacity-50"
          onclick={handleDisconnectAll}
          disabled={disconnecting}
        >
          {disconnecting ? "断开中..." : "断开全部"}
        </button>
        <button
          type="button"
          class="rounded-lg bg-primary/10 border border-primary/20 px-3 py-1.5 text-xs font-medium text-primary hover:bg-primary/20 transition-colors disabled:opacity-50"
          onclick={handleConnect}
          disabled={connecting}
        >
          {connecting ? "授权中..." : "+ 添加账号"}
        </button>
      {:else}
        <button
          type="button"
          class="rounded-lg bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 shadow-sm transition-colors disabled:opacity-50"
          onclick={handleConnect}
          disabled={connecting}
        >
          {connecting ? "正在跳转授权..." : "连接"}
        </button>
      {/if}
    </div>
  </div>

  <!-- Expandable Connected Accounts List -->
  {#if showAccounts}
    <WorkConnectedAccounts
      appId={app.appId}
      {accounts}
      {defaultAccountId}
      {onDisconnectAccount}
      {onSetDefaultAccount}
    />
  {/if}
</div>
