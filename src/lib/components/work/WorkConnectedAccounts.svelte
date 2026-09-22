<script lang="ts">
  import type { AppAccount, ConnectionStatus } from "$lib/types/work";

  interface Props {
    appId: string;
    accounts: AppAccount[];
    defaultAccountId?: string | null;
    onDisconnectAccount: (appId: string, accountId: string) => Promise<void>;
    onSetDefaultAccount?: (appId: string, accountId: string) => Promise<void>;
  }

  let {
    appId,
    accounts,
    defaultAccountId = null,
    onDisconnectAccount,
    onSetDefaultAccount,
  }: Props = $props();
  let disconnectingId = $state<string | null>(null);
  let settingDefaultId = $state<string | null>(null);
  let error = $state("");

  function statusBadge(status: ConnectionStatus) {
    switch (status) {
      case "connected":
        return {
          label: "已连接",
          class: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20",
        };
      case "pending":
        return {
          label: "认证中",
          class: "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20",
        };
      case "expired":
        return {
          label: "已过期",
          class: "bg-orange-500/10 text-orange-600 dark:text-orange-400 border-orange-500/20",
        };
      case "error":
        return {
          label: "异常",
          class: "bg-red-500/10 text-red-600 dark:text-red-400 border-red-500/20",
        };
      default:
        return { label: "未连接", class: "bg-muted text-muted-foreground border-border" };
    }
  }

  async function handleSetDefault(accountId: string) {
    if (!onSetDefaultAccount || settingDefaultId) return;
    settingDefaultId = accountId;
    error = "";
    try {
      await onSetDefaultAccount(appId, accountId);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      settingDefaultId = null;
    }
  }

  async function handleDisconnect(accountId: string) {
    if (disconnectingId) return;
    try {
      const { confirm } = await import("$lib/platform/dialog");
      const ok = await confirm(`确定断开此账号？`, {
        title: "断开账号",
        kind: "warning",
      });
      if (!ok) return;
    } catch {
      // Fallback in web/test environment
      if (!window.confirm("确定断开此账号？")) return;
    }

    disconnectingId = accountId;
    error = "";
    try {
      await onDisconnectAccount(appId, accountId);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      disconnectingId = null;
    }
  }
</script>

<div class="mt-3 space-y-2 border-t border-border/40 pt-3">
  <div class="flex items-center justify-between text-xs text-muted-foreground">
    <span class="font-medium">已绑定账号 ({accounts.length})</span>
  </div>

  {#if error}
    <div
      class="rounded-md border border-red-400/20 bg-red-400/5 px-2.5 py-1.5 text-xs text-red-500"
    >
      {error}
    </div>
  {/if}

  {#if accounts.length === 0}
    <p class="text-xs text-muted-foreground/80 py-1">暂无绑定的账号信息。</p>
  {:else}
    <div class="space-y-1.5">
      {#each accounts as account}
        {@const badge = statusBadge(account.status)}
        {@const isDefault = defaultAccountId === account.accountId}
        <div
          class="flex items-center justify-between gap-3 rounded-lg border border-border/60 bg-muted/20 px-3 py-2 text-xs"
        >
          <div class="flex items-center gap-2.5 min-w-0">
            <span
              class="h-2 w-2 rounded-full {account.status === 'connected'
                ? 'bg-emerald-500'
                : 'bg-muted-foreground'}"
            ></span>
            <div class="truncate">
              <div class="flex items-center gap-1.5">
                <span class="font-medium text-foreground truncate">
                  {account.displayName || account.email || account.accountId}
                </span>
                {#if isDefault}
                  <span
                    class="rounded bg-primary/15 text-primary border border-primary/30 px-1.5 py-0.5 text-[10px] font-semibold"
                  >
                    默认
                  </span>
                {/if}
                {#if account.alias}
                  <span
                    class="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground font-medium"
                  >
                    {account.alias}
                  </span>
                {/if}
              </div>
              {#if account.email && account.displayName && account.displayName !== account.email}
                <div class="text-[11px] text-muted-foreground truncate">{account.email}</div>
              {/if}
            </div>
          </div>

          <div class="flex items-center gap-2 shrink-0">
            <span class="rounded-full border px-2 py-0.5 text-[10px] font-medium {badge.class}">
              {badge.label}
            </span>
            {#if !isDefault && onSetDefaultAccount}
              <button
                type="button"
                class="rounded-md border border-border/80 px-2 py-1 text-[11px] text-muted-foreground hover:text-foreground hover:bg-accent transition-colors disabled:opacity-50"
                onclick={() => handleSetDefault(account.accountId)}
                disabled={settingDefaultId === account.accountId}
              >
                设为默认
              </button>
            {/if}
            <button
              type="button"
              class="rounded-md border border-border/80 px-2 py-1 text-[11px] text-muted-foreground hover:border-red-500/40 hover:bg-red-500/10 hover:text-red-500 transition-colors disabled:opacity-50"
              onclick={() => handleDisconnect(account.accountId)}
              disabled={disconnectingId === account.accountId}
            >
              {#if disconnectingId === account.accountId}
                断开中…
              {:else}
                断开
              {/if}
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
