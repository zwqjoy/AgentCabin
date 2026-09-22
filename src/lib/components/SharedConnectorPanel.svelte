<script lang="ts">
  import type { ConnectorBinding, ConnectorCatalogItem } from "$lib/api";

  interface Props {
    items: ConnectorCatalogItem[];
    bindings?: ConnectorBinding[];
    canToggle?: boolean;
    onToggle?: (item: ConnectorCatalogItem, enabled: boolean) => Promise<void>;
  }

  let { items, bindings = [], canToggle = true, onToggle }: Props = $props();

  let busyKey = $state<string | null>(null);
  let localError = $state("");

  function isEnabled(item: ConnectorCatalogItem): boolean {
    const b = bindings.find(
      (binding) => binding.connectorId.trim().toLowerCase() === item.id.trim().toLowerCase(),
    );
    return b?.enabled ?? true;
  }

  function connectorKind(item: ConnectorCatalogItem): string {
    if (item.mcpServer) return "MCP";
    if (item.skills.length > 0) return "Skill";
    return "Connector";
  }

  async function toggle(item: ConnectorCatalogItem) {
    if (busyKey) return;
    const next = !isEnabled(item);
    busyKey = item.id;
    localError = "";
    try {
      await onToggle?.(item, next);
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
        <span class="h-2 w-2 rounded-full bg-emerald-500"></span>
        <h2 class="text-sm font-semibold text-foreground">连接器 · 共享安装</h2>
      </div>
      <p class="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
        连接器包安装到全局共享目录中；下方开关控制所有运行时。
      </p>
    </div>
    <div class="flex flex-col items-end gap-1 text-[10px] text-muted-foreground">
      <code class="rounded-md bg-background/80 px-2 py-1 text-emerald-700 dark:text-emerald-300">
        ~/.agentcabin/connectors/catalog
      </code>
      <span>{items.length} 个共享连接器</span>
    </div>
  </div>

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
      <div class="text-xs font-medium text-foreground">尚未安装共享连接器</div>
      <div class="mt-1 text-[11px] leading-5 text-muted-foreground">
        连接器包安装后默认启用，需要时可全局停用。
      </div>
    </div>
  {:else}
    <div class="mt-4 space-y-2">
      {#each items as item (item.id)}
        {@const enabled = isEnabled(item)}
        <article
          class="flex flex-col gap-3 rounded-xl border border-border/60 bg-background/50 px-4 py-3.5 sm:flex-row sm:items-center sm:justify-between"
        >
          <div class="min-w-0">
            <div class="flex flex-wrap items-center gap-2">
              <span class="truncate text-sm font-medium text-foreground">{item.name}</span>
              <span
                class="rounded-full bg-emerald-500/10 px-1.5 py-0.5 text-[10px] text-emerald-700 dark:text-emerald-300"
              >
                共享包
              </span>
              <span class="rounded-full bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {connectorKind(item)}
              </span>
              <span
                class="rounded-full px-1.5 py-0.5 text-[10px] {enabled
                  ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                  : 'bg-muted text-muted-foreground'}"
              >
                全局：{enabled ? "启用" : "停用"}
              </span>
            </div>
            <p class="mt-1 truncate text-[11px] leading-4 text-muted-foreground">
              {item.description || item.entrypoint || item.id}
            </p>
            <p class="mt-1 truncate font-mono text-[10px] text-muted-foreground">
              {item.id} · v{item.version}
            </p>
          </div>
          <div class="flex shrink-0 items-center gap-2">
            {#if canToggle}
              <button
                type="button"
                class="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1 text-[10px] font-medium transition-all disabled:opacity-50 {enabled
                  ? 'border-border text-muted-foreground hover:bg-muted hover:text-foreground'
                  : 'border-primary/40 bg-primary/10 text-primary hover:bg-primary/20'}"
                disabled={busyKey !== null}
                onclick={() => void toggle(item)}
                title={enabled ? "全局停用连接器" : "全局启用连接器"}
              >
                <span
                  class="h-1.5 w-1.5 rounded-full {enabled
                    ? 'bg-emerald-500'
                    : 'bg-muted-foreground/40'}"
                ></span>
                {busyKey === item.id ? "保存中…" : enabled ? "全局开" : "全局关"}
              </button>
            {/if}
          </div>
        </article>
      {/each}
    </div>
  {/if}
</section>
