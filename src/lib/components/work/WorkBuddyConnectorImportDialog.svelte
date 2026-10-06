<script lang="ts">
  import { discoverWorkBuddyConnectors } from "$lib/api/work";
  import Modal from "$lib/components/Modal.svelte";
  import type { WorkBuddyConnectorDiscoveryResult } from "$lib/types/work";

  interface Props {
    open?: boolean;
    onImport: (source: string) => Promise<void>;
  }

  let { open = $bindable(false), onImport }: Props = $props();
  let result = $state<WorkBuddyConnectorDiscoveryResult | null>(null);
  let search = $state("");
  let loading = $state(false);
  let busyId = $state("");
  let error = $state("");
  let notice = $state("");
  let wasOpen = false;
  let activeRoot: string | undefined;

  let visiblePackages = $derived.by(() => {
    const query = search.trim().toLocaleLowerCase();
    return (result?.packages ?? []).filter((item) => {
      if (!query) return true;
      return [item.manifest.displayName, item.manifest.id, item.manifest.description]
        .filter(Boolean)
        .some((value) => value!.toLocaleLowerCase().includes(query));
    });
  });

  $effect(() => {
    if (open && !wasOpen) {
      wasOpen = true;
      search = "";
      error = "";
      notice = "";
      void scan();
    } else if (!open) {
      wasOpen = false;
    }
  });

  async function scan(root?: string) {
    activeRoot = root;
    loading = true;
    error = "";
    try {
      result = await discoverWorkBuddyConnectors(root);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
      result = null;
    } finally {
      loading = false;
    }
  }

  async function chooseOtherDirectory() {
    error = "";
    try {
      const { open: openDialog } = await import("$lib/platform/dialog");
      const selected = await openDialog({
        title: "选择 WorkBuddy 连接器目录",
        multiple: false,
        directory: true,
      });
      if (selected && typeof selected === "string") await scan(selected);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function importPackage(item: (typeof visiblePackages)[number]) {
    if (busyId || item.alreadyInstalled) return;
    busyId = item.manifest.id;
    error = "";
    notice = "";
    try {
      await onImport(item.path);
      notice = `已导入 ${item.manifest.displayName}，默认未信任、未启用。`;
      await scan(activeRoot);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }
</script>

<Modal bind:open title="从 WorkBuddy 导入连接器">
  <div class="space-y-3">
    <div class="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
      <p>
        {#if result?.available}
          已发现 {result.packages.length} 个连接器
        {:else}
          未发现 WorkBuddy 本地连接器目录
        {/if}
      </p>
      {#if loading}<span>扫描中…</span>{/if}
    </div>

    <label class="sr-only" for="workbuddy-connector-search">搜索连接器</label>
    <input
      id="workbuddy-connector-search"
      bind:value={search}
      class="min-h-9 w-full rounded-md border border-border bg-background px-3 text-sm outline-none placeholder:text-muted-foreground/70 focus:border-primary"
      placeholder="搜索连接器…"
      disabled={loading || !result?.available}
    />

    {#if error}
      <p
        class="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-600 dark:text-red-400"
        role="alert"
      >
        {error}
      </p>
    {/if}
    {#if notice}<p class="text-xs text-emerald-700 dark:text-emerald-300" role="status">
        {notice}
      </p>{/if}
    {#if result?.warnings.length}
      <p class="text-[11px] text-amber-700 dark:text-amber-300">
        {result.warnings.join(" ")}
      </p>
    {/if}

    {#if loading && !result}
      <div class="py-8 text-center text-sm text-muted-foreground">正在扫描 WorkBuddy 连接器…</div>
    {:else if !result?.available}
      <div class="py-6 text-center text-sm text-muted-foreground">
        <p>没有找到本地连接器目录，也可以选择其他目录。</p>
      </div>
    {:else if visiblePackages.length === 0}
      <div class="py-6 text-center text-sm text-muted-foreground">
        {search.trim() ? "没有匹配的连接器。" : "这些目录中没有可导入的连接器。"}
      </div>
    {:else}
      <div class="max-h-[min(48vh,28rem)] divide-y divide-border/70 overflow-y-auto pr-1">
        {#each visiblePackages as item (item.manifest.id)}
          <article class="flex items-start justify-between gap-3 py-3 first:pt-1 last:pb-1">
            <div class="min-w-0">
              <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
                <h3 class="truncate text-sm font-medium text-foreground">
                  {item.manifest.displayName}
                </h3>
                <span class="text-[11px] text-muted-foreground">v{item.manifest.version}</span>
                {#each item.manifest.runtimes as runtime}
                  <span class="rounded bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">
                    {runtime.toUpperCase()}
                  </span>
                {/each}
              </div>
              {#if item.manifest.description}
                <p class="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">
                  {item.manifest.description}
                </p>
              {/if}
              <p class="mt-1 text-[11px] text-muted-foreground">
                {#if item.manifest.auth.kind === "none"}
                  无需认证
                {:else if item.manifest.auth.kind === "api_key"}
                  需配置凭据（{item.manifest.authFields.length} 项）
                {:else if item.manifest.auth.kind === "oauth2"}
                  需 OAuth 认证
                {:else}
                  任务中执行 auth
                {/if}
              </p>
            </div>
            <button
              type="button"
              class="min-h-8 shrink-0 rounded-md px-3 text-xs font-medium transition-colors {item.alreadyInstalled
                ? 'bg-muted text-muted-foreground'
                : 'bg-foreground text-background hover:opacity-90'}"
              disabled={item.alreadyInstalled || Boolean(busyId) || loading}
              onclick={() => void importPackage(item)}
            >
              {#if item.alreadyInstalled}
                已导入
              {:else if busyId === item.manifest.id}
                导入中…
              {:else}
                导入
              {/if}
            </button>
          </article>
        {/each}
      </div>
    {/if}

    <div class="flex justify-end border-t border-border/70 pt-3">
      <button
        type="button"
        class="min-h-8 rounded-md px-2.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        disabled={loading || Boolean(busyId)}
        onclick={() => void chooseOtherDirectory()}
      >
        选择其他目录…
      </button>
    </div>
  </div>
</Modal>
