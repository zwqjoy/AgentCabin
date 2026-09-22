<script lang="ts">
  import type { ConnectorPackageSummary } from "$lib/types/work";

  type CatalogSource = "discover" | "enabled";

  interface Props {
    packages: ConnectorPackageSummary[];
    source?: CatalogSource;
    showAll?: boolean;
    canEnable?: boolean;
    onInstall: (source: string) => Promise<void>;
    onTrust: (packageId: string, trusted: boolean) => Promise<void>;
    onEnable: (packageId: string, enabled: boolean) => Promise<void>;
    onUninstall: (packageId: string) => Promise<void>;
    onAuthorize: (packageId: string) => Promise<void>;
    onConfigureToken: (packageId: string, values: Record<string, string>) => Promise<void>;
  }

  let {
    packages,
    source = "discover",
    showAll = false,
    canEnable = true,
    onInstall,
    onTrust,
    onEnable,
    onUninstall,
    onAuthorize,
    onConfigureToken,
  }: Props = $props();
  let busyId = $state("");
  let error = $state("");
  let editingTokenId = $state("");
  let tokenValues = $state<Record<string, Record<string, string>>>({});

  let visiblePackages = $derived(
    source === "enabled" && !showAll ? packages.filter((item) => item.state.enabled) : packages,
  );

  function runtimeLabel(runtime: string): string {
    if (runtime === "mcp") return "MCP";
    if (runtime === "cli") return "CLI";
    return "Skill";
  }

  function needsCredentialAuth(item: ConnectorPackageSummary): boolean {
    return (
      item.manifest.auth.kind !== "none" &&
      item.manifest.auth.kind !== "cli" &&
      item.state.authStatus !== "not_required"
    );
  }

  function isCredentialAuthenticated(item: ConnectorPackageSummary): boolean {
    return item.state.authStatus === "authenticated";
  }

  function statusLabel(item: ConnectorPackageSummary): string {
    if (!item.state.trusted) return "需要信任";
    if (needsCredentialAuth(item) && !isCredentialAuthenticated(item)) {
      return item.state.authStatus === "pending" ? "认证中" : "未配置凭据";
    }
    if (!item.state.enabled) return "已停用";
    if (item.state.runtimeStatus === "ready") return "已就绪";
    if (item.state.runtimeStatus === "failed") return "运行时失败";
    return "已启用 · 等待运行时";
  }

  async function handleToggleEnable(item: ConnectorPackageSummary) {
    if (!item.state.enabled && needsCredentialAuth(item) && !isCredentialAuthenticated(item)) {
      if (item.manifest.auth.kind === "oauth2") {
        await run(item.manifest.id, onAuthorize);
      } else {
        if (editingTokenId !== item.manifest.id) {
          beginTokenSetup(item);
        }
        error = `连接器“${item.manifest.displayName}”尚未配置凭据，请先配置凭据后再启用`;
      }
      return;
    }
    await run(item.manifest.id, (id) => onEnable(id, !item.state.enabled));
  }

  async function choosePackage() {
    error = "";
    try {
      const { open } = await import("$lib/platform/dialog");
      const selected = await open({
        title: "导入 WorkBuddy 连接器",
        multiple: false,
        directory: true,
      });
      if (!selected || typeof selected !== "string") return;
      await onInstall(selected);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function run(packageId: string, action: (packageId: string) => Promise<void>) {
    if (busyId) return;
    busyId = packageId;
    error = "";
    try {
      await action(packageId);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }

  async function uninstall(item: ConnectorPackageSummary) {
    if (busyId) return;
    const { confirm } = await import("$lib/platform/dialog");
    const ok = await confirm("卸载 WorkBuddy 连接器“" + item.manifest.displayName + "”？", {
      title: "卸载 WorkBuddy 连接器",
      kind: "warning",
    });
    if (!ok) return;
    await run(item.manifest.id, () => onUninstall(item.manifest.id));
  }

  function beginTokenSetup(item: ConnectorPackageSummary) {
    editingTokenId = editingTokenId === item.manifest.id ? "" : item.manifest.id;
    tokenValues[item.manifest.id] ??= Object.fromEntries(
      item.manifest.authFields.map((field) => [field.key, ""]),
    );
  }

  async function saveToken(item: ConnectorPackageSummary) {
    await run(item.manifest.id, async (packageId) => {
      await onConfigureToken(packageId, tokenValues[packageId] ?? {});
      editingTokenId = "";
      tokenValues[packageId] = Object.fromEntries(
        item.manifest.authFields.map((field) => [field.key, ""]),
      );
    });
  }
</script>

<section class="rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5">
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div>
      <div class="flex items-center gap-2">
        <span class="h-2 w-2 rounded-full bg-amber-500"></span>
        <h2 class="text-sm font-semibold text-foreground">WorkBuddy 连接器</h2>
        <span
          class="rounded-full bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-700 dark:text-amber-300"
          >{packages.length}</span
        >
      </div>
      <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
        仅接受 WorkBuddy connector-meta.json 格式，可组合 MCP、CLI 和
        Skill。导入后默认不信任、不启用。
      </p>
      <p class="mt-1 max-w-2xl text-[11px] leading-5 text-muted-foreground/80">
        CLI 会在信任并启用后安装到 AgentCabin 应用级运行时，不修改系统全局 npm。
      </p>
    </div>
    {#if source === "discover"}
      <button
        type="button"
        class="min-h-9 rounded-lg bg-foreground px-3 text-[11px] font-semibold text-background transition-opacity hover:opacity-90 disabled:opacity-50"
        disabled={Boolean(busyId)}
        onclick={() => void choosePackage()}
      >
        导入 WorkBuddy 连接器
      </button>
    {/if}
  </div>

  {#if error}
    <div
      class="mt-3 rounded-lg border border-red-500/20 bg-red-500/5 px-3 py-2 text-xs text-red-600 dark:text-red-300"
    >
      {error}
    </div>
  {/if}

  {#if visiblePackages.length === 0}
    <div
      class="mt-4 rounded-xl border border-dashed border-border/70 px-4 py-6 text-center text-xs text-muted-foreground"
    >
      {source === "enabled" ? "目前没有已启用的 WorkBuddy 连接器。" : "尚未导入 WorkBuddy 连接器。"}
    </div>
  {:else}
    <div class="mt-4 space-y-2">
      {#each visiblePackages as item (item.manifest.id)}
        <div class="rounded-xl border border-border/60 bg-background/50 p-3">
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="min-w-0">
              <div class="flex flex-wrap items-center gap-2">
                <span class="truncate text-sm font-semibold text-foreground"
                  >{item.manifest.displayName}</span
                >
                <span class="text-[10px] text-muted-foreground">v{item.manifest.version}</span>
                {#each item.manifest.runtimes as runtime}
                  <span
                    class="rounded bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary"
                    >{runtimeLabel(runtime)}</span
                  >
                {/each}
              </div>
              {#if item.manifest.description}
                <p class="mt-1 text-xs leading-5 text-muted-foreground">
                  {item.manifest.description}
                </p>
              {/if}
              <div class="mt-2 flex flex-wrap items-center gap-2 text-[10px] text-muted-foreground">
                <span
                  class={item.state.trusted && item.state.enabled
                    ? "text-emerald-600 dark:text-emerald-300"
                    : "text-amber-600 dark:text-amber-300"}
                >
                  {canEnable ? statusLabel(item) : item.state.trusted ? "已安装" : "需要信任"}
                </span>
                <span>来源：{item.manifest.origin}</span>
                {#if item.state.authStatus !== "not_required"}
                  <span>认证：{item.state.authStatus}</span>
                {/if}
              </div>
            </div>
            <div class="flex shrink-0 flex-wrap items-center justify-end gap-2">
              {#if !item.state.trusted}
                <button
                  type="button"
                  disabled={Boolean(busyId)}
                  class="rounded-lg border border-amber-500/40 bg-amber-500/10 px-2.5 py-1.5 text-[11px] font-medium text-amber-700 hover:bg-amber-500/20 disabled:opacity-50 dark:text-amber-300"
                  onclick={() => void run(item.manifest.id, (id) => onTrust(id, true))}
                >
                  信任连接器
                </button>
              {:else}
                <button
                  type="button"
                  disabled={Boolean(busyId)}
                  class="rounded-lg border border-border px-2.5 py-1.5 text-[11px] font-medium text-foreground hover:bg-accent disabled:opacity-50"
                  onclick={() => void run(item.manifest.id, (id) => onTrust(id, false))}
                >
                  撤销信任
                </button>
              {/if}
              {#if item.state.trusted && canEnable}
                <button
                  type="button"
                  disabled={Boolean(busyId)}
                  class="rounded-lg border border-primary/40 bg-primary/10 px-2.5 py-1.5 text-[11px] font-medium text-primary hover:bg-primary/20 disabled:opacity-50"
                  onclick={() => void handleToggleEnable(item)}
                >
                  {item.state.enabled ? "停用" : "启用"}
                </button>
              {/if}
              {#if item.state.trusted && item.state.authStatus !== "not_required"}
                {#if item.manifest.auth.kind === "oauth2"}
                  <button
                    type="button"
                    disabled={Boolean(busyId) || item.state.authStatus === "pending"}
                    class="rounded-lg border border-indigo-500/40 bg-indigo-500/10 px-2.5 py-1.5 text-[11px] font-medium text-indigo-700 hover:bg-indigo-500/20 disabled:opacity-50 dark:text-indigo-300"
                    onclick={() => void run(item.manifest.id, onAuthorize)}
                  >
                    {item.state.authStatus === "authenticated" ? "重新连接" : "连接认证"}
                  </button>
                {:else if item.manifest.auth.kind === "cli"}
                  <span
                    class="rounded-lg border border-border/60 px-2.5 py-1.5 text-[11px] text-muted-foreground"
                  >
                    任务中执行 auth
                  </span>
                {:else}
                  <button
                    type="button"
                    disabled={Boolean(busyId)}
                    class="rounded-lg border border-indigo-500/40 bg-indigo-500/10 px-2.5 py-1.5 text-[11px] font-medium text-indigo-700 hover:bg-indigo-500/20 disabled:opacity-50 dark:text-indigo-300"
                    onclick={() => beginTokenSetup(item)}
                  >
                    {item.state.authStatus === "authenticated" ? "更新凭据" : "配置凭据"}
                  </button>
                {/if}
              {/if}
              <button
                type="button"
                disabled={Boolean(busyId)}
                class="rounded-lg border border-red-500/30 px-2.5 py-1.5 text-[11px] font-medium text-red-600 hover:bg-red-500/10 disabled:opacity-50 dark:text-red-300"
                onclick={() => void uninstall(item)}
              >
                卸载
              </button>
            </div>
          </div>
          {#if editingTokenId === item.manifest.id && item.manifest.auth.kind === "api_key"}
            <div class="mt-3 space-y-3 rounded-xl border border-border/60 bg-muted/20 p-3">
              {#each item.manifest.authFields as field (field.key)}
                <label class="block">
                  <span class="mb-1 block text-[11px] font-medium text-foreground">
                    {field.label}{field.required ? " *" : ""}
                  </span>
                  <input
                    type={field.fieldType === "text" ? "text" : "password"}
                    autocomplete="off"
                    class="h-9 w-full rounded-lg border border-border bg-background px-3 text-xs text-foreground outline-none focus:border-primary"
                    placeholder={field.placeholder}
                    value={tokenValues[item.manifest.id]?.[field.key] ?? ""}
                    oninput={(event) => {
                      tokenValues[item.manifest.id] ??= {};
                      tokenValues[item.manifest.id][field.key] = event.currentTarget.value;
                    }}
                  />
                  {#if field.description}
                    <span class="mt-1 block text-[10px] text-muted-foreground">
                      {field.description}
                    </span>
                  {/if}
                </label>
              {/each}
              <div class="flex justify-end gap-2">
                <button
                  type="button"
                  class="rounded-lg border border-border px-3 py-1.5 text-[11px] text-muted-foreground"
                  onclick={() => (editingTokenId = "")}>取消</button
                >
                <button
                  type="button"
                  disabled={Boolean(busyId)}
                  class="rounded-lg bg-primary px-3 py-1.5 text-[11px] font-medium text-primary-foreground disabled:opacity-50"
                  onclick={() => void saveToken(item)}>保存凭据</button
                >
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <p class="mt-3 text-[10px] leading-4 text-muted-foreground">
    已信任且启用的 MCP、CLI、Skill runtime 会在下一次 Work 会话加载；认证凭据只留在 Host，运行时
    只看到声明的工具和技能。
  </p>
</section>
