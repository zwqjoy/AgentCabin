<script lang="ts">
  import type { InstalledPlugin } from "$lib/types";

  type PiProfile = "code" | "work";

  interface Props {
    mode: PiProfile;
    items: InstalledPlugin[];
    canInstall?: boolean;
    canManagePackage?: boolean;
    canToggle?: boolean;
    onInstall?: (source: string) => Promise<void>;
    onToggle: (item: InstalledPlugin, enabled: boolean) => Promise<void>;
    onUpdate?: (item: InstalledPlugin) => Promise<void>;
    onUninstall?: (item: InstalledPlugin) => Promise<void>;
  }

  let {
    mode,
    items,
    canInstall = false,
    canManagePackage = false,
    canToggle = true,
    onInstall,
    onToggle,
    onUpdate,
    onUninstall,
  }: Props = $props();

  let source = $state("");
  let busyKey = $state<string | null>(null);
  let localError = $state("");

  const profileLabel = $derived(mode === "code" ? "Pi Code" : "Pi Work");

  function extensionId(item: InstalledPlugin): string {
    return item.pluginId ?? item.name;
  }

  function packageSource(item: InstalledPlugin): string {
    const extra = item.extra as Record<string, unknown> | undefined;
    return typeof extra?.source === "string" ? extra.source : item.name;
  }

  function packagePath(item: InstalledPlugin): string | null {
    const extra = item.extra as Record<string, unknown> | undefined;
    return typeof extra?.packagePath === "string" ? extra.packagePath : null;
  }

  function workCompatible(item: InstalledPlugin): boolean {
    const extra = item.extra as Record<string, unknown> | undefined;
    return extra?.workCompatible !== false;
  }

  async function install() {
    const value = source.trim();
    if (!value || !onInstall || busyKey) return;
    busyKey = "install";
    localError = "";
    try {
      await onInstall(value);
      source = "";
    } catch (cause) {
      localError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyKey = null;
    }
  }

  async function toggle(item: InstalledPlugin) {
    const id = extensionId(item);
    if (busyKey) return;
    if (mode === "work" && !workCompatible(item)) {
      localError = "这是 Pi Code 原生功能，不能在 Work 中启用。";
      return;
    }
    busyKey = id;
    localError = "";
    try {
      await onToggle(item, item.enabled === false);
    } catch (cause) {
      localError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyKey = null;
    }
  }

  async function update(item: InstalledPlugin) {
    if (!onUpdate || busyKey) return;
    const id = extensionId(item);
    busyKey = id;
    localError = "";
    try {
      await onUpdate(item);
    } catch (cause) {
      localError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyKey = null;
    }
  }

  async function uninstall(item: InstalledPlugin) {
    if (!onUninstall || busyKey) return;
    const id = extensionId(item);
    busyKey = id;
    localError = "";
    try {
      await onUninstall(item);
    } catch (cause) {
      localError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyKey = null;
    }
  }
</script>

<section class="rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5">
  <div class="flex flex-wrap items-start justify-between gap-4">
    <div>
      <div class="flex items-center gap-2">
        <span class="h-2 w-2 rounded-full bg-violet-500"></span>
        <h2 class="text-sm font-semibold text-foreground">Pi 扩展 · 共享安装</h2>
      </div>
      <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
        {canToggle
          ? `扩展包只安装一份；${profileLabel} 只管理自己的启用状态，不会修改另一个模式的配置。`
          : "扩展包只安装一份；安装后请到 Code 或 Work 页面分别启用。"}
      </p>
    </div>
    <div class="flex flex-col items-end gap-1 text-[10px] text-muted-foreground">
      <code class="rounded-md bg-background/80 px-2 py-1 text-violet-700 dark:text-violet-300"
        >~/.agentcabin/pi</code
      >
      <span>{items.length} 个共享包</span>
    </div>
  </div>

  {#if mode === "work"}
    <div
      class="mt-4 rounded-xl border border-amber-500/20 bg-amber-500/5 px-3 py-2.5 text-[11px] leading-5 text-amber-800 dark:text-amber-200"
    >
      Work 默认不加载共享 Pi 扩展；启用后仍会经过 Work 的沙箱、权限和运行时过滤。Pi Code 原生
      Permission、Plan、Goal、Todo、Context prune 不会通过这里进入 Work。
    </div>
  {/if}

  {#if canInstall}
    <form
      class="mt-4 flex flex-col gap-2 sm:flex-row"
      onsubmit={(event) => {
        event.preventDefault();
        void install();
      }}
    >
      <input
        bind:value={source}
        class="min-h-10 min-w-0 flex-1 rounded-lg border border-border bg-background px-3 font-mono text-xs text-foreground outline-none placeholder:text-muted-foreground focus:border-primary focus:ring-2 focus:ring-primary/15"
        placeholder="npm:@scope/package 或 git:github.com/org/repo.git"
        aria-label="Pi 共享扩展来源"
      />
      <button
        type="submit"
        class="min-h-10 rounded-lg bg-primary px-4 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
        disabled={!source.trim() || busyKey === "install"}
      >
        {busyKey === "install" ? "安装中…" : "安装到共享目录"}
      </button>
    </form>
  {:else}
    <p
      class="mt-4 rounded-xl border border-border/60 bg-background/40 px-3 py-2.5 text-[11px] leading-5 text-muted-foreground"
    >
      需要新增扩展时，请在能力中心安装；这里仅配置 {profileLabel} 的启用状态。
    </p>
  {/if}

  {#if localError}
    <div
      class="mt-3 rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
      role="alert"
    >
      {localError}
    </div>
  {/if}

  {#if items.length === 0}
    <div class="mt-4 rounded-xl border border-dashed border-border/70 px-4 py-8 text-center">
      <div class="text-xs font-medium text-foreground">尚未安装共享 Pi 扩展</div>
      <div class="mt-1 text-[11px] leading-5 text-muted-foreground">
        安装后，Code 和 Work 会分别显示自己的启用状态。
      </div>
    </div>
  {:else}
    <div class="mt-4 space-y-2">
      {#each items as item (extensionId(item))}
        {@const id = extensionId(item)}
        <article
          class="flex flex-col gap-3 rounded-xl border border-border/60 bg-background/50 px-4 py-3.5 sm:flex-row sm:items-center sm:justify-between"
        >
          <div class="min-w-0">
            <div class="flex flex-wrap items-center gap-2">
              <span class="truncate text-sm font-medium text-foreground">{item.name}</span>
              <span
                class="rounded-full bg-violet-500/10 px-1.5 py-0.5 text-[10px] text-violet-700 dark:text-violet-300"
                >共享包</span
              >
              {#if canToggle}
                <span
                  class="rounded-full px-1.5 py-0.5 text-[10px] {item.enabled === false
                    ? 'bg-muted text-muted-foreground'
                    : 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'}"
                >
                  {!workCompatible(item)
                    ? "仅 Pi Code"
                    : item.enabled === false
                      ? "当前模式已停用"
                      : "当前模式已启用"}
                </span>
              {:else}
                <span class="rounded-full bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground"
                  >{workCompatible(item) ? "已安装" : "仅 Pi Code"}</span
                >
              {/if}
            </div>
            <p class="mt-1 truncate text-[11px] leading-4 text-muted-foreground">
              {item.description}
            </p>
            <p
              class="mt-1 truncate font-mono text-[10px] text-muted-foreground"
              title={packagePath(item) ?? packageSource(item)}
            >
              {packageSource(item)}{item.version ? ` · v${item.version}` : ""}
            </p>
          </div>
          <div class="flex shrink-0 flex-wrap items-center gap-2">
            {#if canToggle}
              <button
                type="button"
                class="min-h-9 rounded-lg border border-border px-3 text-[11px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50"
                onclick={() => void toggle(item)}
                disabled={busyKey === id ||
                  Boolean(busyKey) ||
                  (mode === "work" && !workCompatible(item))}
              >
                {!workCompatible(item) && mode === "work"
                  ? "不可用于 Work"
                  : item.enabled === false
                    ? "启用"
                    : "停用"}
              </button>
            {:else}
              <span class="self-center text-[11px] text-muted-foreground"
                >在 Code / Work 中启停</span
              >
            {/if}
            {#if canManagePackage && onUpdate}
              <button
                type="button"
                class="min-h-9 rounded-lg border border-border px-3 text-[11px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50"
                onclick={() => void update(item)}
                disabled={busyKey === id || Boolean(busyKey)}
              >
                更新
              </button>
            {/if}
            {#if canManagePackage && onUninstall}
              <button
                type="button"
                class="min-h-9 rounded-lg border border-destructive/30 px-3 text-[11px] text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
                onclick={() => void uninstall(item)}
                disabled={busyKey === id || Boolean(busyKey)}
              >
                卸载共享包
              </button>
            {/if}
          </div>
        </article>
      {/each}
    </div>
  {/if}
</section>
