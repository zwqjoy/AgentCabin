<script lang="ts">
  import {
    checkLarkCli,
    connectWorkAppNative,
    getLarkAuthStatus,
    installLarkCli,
    listWorkAppsConnections,
    startLarkAuth,
    type LarkAuthStatus,
    type LarkCliInfo,
  } from "$lib/api/work";
  import type { AppConnection, ConnectorCatalogItem } from "$lib/types/work";

  interface Props {
    item: ConnectorCatalogItem | null;
    open: boolean;
    onClose: () => void;
    onConnected: (connection: AppConnection) => void;
  }

  let { item, open, onClose, onConnected }: Props = $props();

  let token = $state("");
  let alias = $state("");
  let email = $state("");
  let showToken = $state(false);
  let submitting = $state(false);
  let installingCli = $state(false);
  let larkCliInfo = $state<LarkCliInfo | null>(null);
  let authStatus = $state<LarkAuthStatus | null>(null);
  let error = $state("");

  $effect(() => {
    if (open) {
      token = "";
      alias = "";
      email = "";
      showToken = false;
      submitting = false;
      installingCli = false;
      authStatus = null;
      error = "";
      larkCliInfo = null;

      if (item?.packageId.toLowerCase() === "feishu") {
        void loadLarkCliStatus();
      }
    }
  });

  async function loadLarkCliStatus() {
    try {
      larkCliInfo = await checkLarkCli();
    } catch {
      larkCliInfo = null;
    }
  }

  async function handleInstallCli() {
    installingCli = true;
    error = "";
    try {
      await installLarkCli();
      await loadLarkCliStatus();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      installingCli = false;
    }
  }

  function wait(milliseconds: number) {
    return new Promise((resolve) => setTimeout(resolve, milliseconds));
  }

  async function completeFeishuConnection(status: LarkAuthStatus) {
    const connection =
      status.connection ??
      (await listWorkAppsConnections()).find(
        (candidate) => candidate.appId.toLowerCase() === "feishu",
      );
    if (!connection) {
      throw new Error("飞书授权已返回，但 AgentCabin 尚未读到账号连接记录，请重试状态检查。");
    }
    onConnected(connection);
    onClose();
  }

  async function handleFeishuAuth() {
    submitting = true;
    error = "";
    authStatus = null;
    try {
      if (!larkCliInfo?.installed) {
        await installLarkCli();
        await loadLarkCliStatus();
      }
      if (!larkCliInfo?.installed) {
        throw new Error("AgentCabin 应用级 lark-cli 安装未完成。");
      }

      let latest = await startLarkAuth(alias.trim() || undefined, email.trim() || undefined);
      authStatus = latest;
      if (latest.status === "authenticated") {
        await completeFeishuConnection(latest);
        return;
      }

      const taskId = latest.taskId;
      if (!taskId) {
        throw new Error("飞书授权任务未返回 taskId。");
      }

      // The Host opens the allowlisted URL as soon as lark-cli emits it. The
      // modal only polls the durable task state and never handles credentials.
      for (let attempt = 0; attempt < 610; attempt += 1) {
        await wait(1000);
        latest = await getLarkAuthStatus(taskId);
        authStatus = latest;
        if (latest.status === "authenticated") {
          await completeFeishuConnection(latest);
          return;
        }
        if (latest.status === "error") {
          throw new Error(latest.message || "飞书浏览器授权失败。");
        }
      }
      throw new Error("飞书授权等待超过 10 分钟，请重新点击连接。");
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      submitting = false;
    }
  }

  interface AppHelpInfo {
    tokenLabel: string;
    tokenPlaceholder: string;
    tokenHelp: string;
    docUrl: string;
    docText: string;
  }

  function getHelpInfo(packageId: string): AppHelpInfo {
    switch (packageId.toLowerCase()) {
      case "github":
        return {
          tokenLabel: "GitHub 个人访问令牌 (Personal Access Token)",
          tokenPlaceholder: "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxx",
          tokenHelp:
            "建议使用具有 repo、workflow、read:org 权限的 Classic Token 或 Fine-grained Token。",
          docUrl: "https://github.com/settings/tokens",
          docText: "前往 GitHub 生成 Token",
        };
      case "notion":
        return {
          tokenLabel: "Notion Internal Integration Token",
          tokenPlaceholder: "secret_xxxxxxxxxxxxxxxxxxxxxxxxxxxx",
          tokenHelp:
            "在 Notion 开发者平台创建 Internal Integration，并将需要访问的页面分享给该集成。",
          docUrl: "https://www.notion.so/my-integrations",
          docText: "前往 Notion 开发者中心",
        };
      case "slack":
        return {
          tokenLabel: "Slack Bot User OAuth Token",
          tokenPlaceholder: "xoxb-xxxxxxxxxxxxxxxxxxxxxxxxxxxx",
          tokenHelp: "在 Slack App 控制台 OAuth & Permissions 中获取以 xoxb- 开头的 Bot Token。",
          docUrl: "https://api.slack.com/apps",
          docText: "前往 Slack App 控制台",
        };
      default:
        return {
          tokenLabel: "API Key / Access Token",
          tokenPlaceholder: "输入访问密钥或 Token",
          tokenHelp: "此凭据将保存在 Host 本地，零第三方中转，直连服务商官方 API。",
          docUrl: item?.documentationUrl ?? "",
          docText: "查看官方文档",
        };
    }
  }

  async function handleSave() {
    if (!item) return;
    const isFeishu = item.packageId.toLowerCase() === "feishu";
    if (isFeishu) {
      await handleFeishuAuth();
      return;
    }

    const tokenPayload = token.trim();
    if (!tokenPayload) {
      error = "请输入有效的 Token 或 API Key";
      return;
    }

    submitting = true;
    error = "";
    try {
      const conn = await connectWorkAppNative(
        item.packageId,
        tokenPayload,
        alias.trim() || undefined,
        email.trim() || undefined,
      );
      onConnected(conn);
      onClose();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      submitting = false;
    }
  }
</script>

{#if open && item}
  {@const isFeishu = item.packageId.toLowerCase() === "feishu"}
  {@const help = getHelpInfo(item.packageId)}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-xs"
    role="dialog"
    aria-modal="true"
    aria-labelledby="auth-modal-title"
  >
    <div class="relative w-full max-w-lg rounded-2xl border border-border/80 bg-card p-6 shadow-xl">
      <div class="flex items-start justify-between gap-3">
        <div class="flex items-center gap-3">
          <div
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-border bg-muted/60 text-xs font-bold text-foreground"
          >
            {isFeishu ? "飞" : item.displayName.slice(0, 2).toUpperCase()}
          </div>
          <div>
            <h3 id="auth-modal-title" class="text-base font-semibold text-foreground">
              连接 {item.displayName}
            </h3>
            <p class="text-xs text-muted-foreground">原生直连 · 授权留在本机 · 0 第三方中转</p>
          </div>
        </div>
        <button
          type="button"
          class="rounded-lg p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          onclick={onClose}
          aria-label="关闭"
        >
          <svg
            class="h-5 w-5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      {#if error}
        <div
          class="mt-4 rounded-lg border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-600 dark:text-red-300"
          role="alert"
        >
          {error}
        </div>
      {/if}

      <form
        class="mt-4 space-y-4"
        onsubmit={(event) => {
          event.preventDefault();
          void handleSave();
        }}
      >
        {#if isFeishu}
          <div
            class="flex items-center justify-between rounded-xl border border-border/80 bg-muted/30 p-3"
          >
            <div class="flex items-center gap-2.5">
              {#if larkCliInfo?.installed}
                <span
                  class="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-emerald-500/15 text-[11px] font-bold text-emerald-600 dark:text-emerald-400"
                  >✓</span
                >
                <div>
                  <div class="text-xs font-medium text-foreground">
                    AgentCabin 应用级 lark-cli 已就绪
                  </div>
                  <div class="text-[10px] text-muted-foreground">
                    {larkCliInfo.version ?? "已安装"} · 配置使用当前用户 HOME
                  </div>
                </div>
              {:else}
                <span
                  class="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-amber-500/15 text-[11px] font-bold text-amber-600 dark:text-amber-400"
                  >!</span
                >
                <div>
                  <div class="text-xs font-medium text-foreground">
                    尚未安装 AgentCabin 应用级 CLI
                  </div>
                  <div class="text-[10px] text-muted-foreground">不会使用系统全局 lark-cli</div>
                </div>
              {/if}
            </div>
            {#if !larkCliInfo?.installed}
              <button
                type="button"
                class="rounded-lg bg-primary/10 px-2.5 py-1 text-xs font-medium text-primary transition-colors hover:bg-primary/20 disabled:opacity-60"
                disabled={installingCli}
                onclick={handleInstallCli}
              >
                {installingCli ? "安装中…" : "一键安装 CLI"}
              </button>
            {/if}
          </div>

          <div class="rounded-xl border border-cyan-500/20 bg-cyan-500/5 p-3.5">
            <div class="text-xs font-medium text-cyan-800 dark:text-cyan-200">
              浏览器授权，不需要回填 App ID / App Secret
            </div>
            <p class="mt-1 text-[11px] leading-relaxed text-muted-foreground">
              AgentCabin 会调用官方 lark-cli 打开飞书网页完成用户授权。CLI
              会自动把凭据保存到当前用户的 lark-cli 配置目录；你只需要在浏览器中确认授权。
            </p>
          </div>

          {#if authStatus && authStatus.status === "pending"}
            <div
              class="rounded-xl border border-amber-500/25 bg-amber-500/10 p-3 text-xs text-amber-800 dark:text-amber-200"
            >
              <div class="flex items-center gap-2 font-medium">
                <span class="h-2 w-2 animate-pulse rounded-full bg-amber-500"></span>
                {authStatus.message}
              </div>
              {#if authStatus.url}
                <a
                  class="mt-2 block break-all text-[11px] underline"
                  href={authStatus.url}
                  target="_blank"
                  rel="noreferrer">如果浏览器没有自动打开，点击这里继续授权 ↗</a
                >
              {/if}
            </div>
          {/if}
        {:else}
          <div>
            <div class="flex items-center justify-between gap-2">
              <label for="native-token-input" class="text-xs font-medium text-foreground">
                {help.tokenLabel} <span class="text-red-500">*</span>
              </label>
              {#if help.docUrl}
                <a
                  href={help.docUrl}
                  target="_blank"
                  rel="noreferrer"
                  class="text-[11px] text-primary hover:underline">{help.docText} ↗</a
                >
              {/if}
            </div>
            <div class="relative mt-1.5">
              <input
                id="native-token-input"
                type={showToken ? "text" : "password"}
                bind:value={token}
                placeholder={help.tokenPlaceholder}
                class="w-full rounded-lg border border-border bg-background py-2 pl-3 pr-10 text-xs text-foreground placeholder:text-muted-foreground focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring"
                required
              />
              <button
                type="button"
                class="absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                onclick={() => (showToken = !showToken)}
                aria-label={showToken ? "隐藏密钥" : "显示密钥"}
                >{showToken ? "隐藏" : "显示"}</button
              >
            </div>
            <p class="mt-1 text-[11px] text-muted-foreground">{help.tokenHelp}</p>
          </div>
        {/if}

        <div class="grid grid-cols-2 gap-3">
          <div>
            <label for="native-alias-input" class="text-xs font-medium text-foreground">
              账号备注 (可选)
            </label>
            <input
              id="native-alias-input"
              type="text"
              bind:value={alias}
              placeholder="如：个人工作号"
              class="mt-1.5 w-full rounded-lg border border-border bg-background px-3 py-2 text-xs text-foreground placeholder:text-muted-foreground focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring"
            />
          </div>
          <div>
            <label for="native-email-input" class="text-xs font-medium text-foreground">
              账号邮箱 (可选)
            </label>
            <input
              id="native-email-input"
              type="text"
              bind:value={email}
              placeholder="user@example.com"
              class="mt-1.5 w-full rounded-lg border border-border bg-background px-3 py-2 text-xs text-foreground placeholder:text-muted-foreground focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring"
            />
          </div>
        </div>

        <div class="mt-6 flex items-center justify-end gap-2.5 border-t border-border/60 pt-4">
          <button
            type="button"
            class="rounded-lg border border-border bg-card px-4 py-2 text-xs font-medium text-foreground hover:bg-muted"
            onclick={onClose}>取消</button
          >
          <button
            type="submit"
            class="inline-flex items-center gap-1.5 rounded-lg bg-primary px-4 py-2 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-60"
            disabled={submitting || (!isFeishu && !token.trim())}
          >
            {#if submitting}
              <span
                class="h-3.5 w-3.5 animate-spin rounded-full border-2 border-current/25 border-t-current"
              ></span>
              <span>{isFeishu ? "等待浏览器授权…" : "连接中…"}</span>
            {:else}
              <span>{isFeishu ? "打开浏览器并连接" : "保存并连接"}</span>
            {/if}
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}
