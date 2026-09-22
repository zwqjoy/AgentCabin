<script lang="ts">
  import { platform } from "$lib/platform";
  import type { WorkReceiptSearchQuery, WorkReceiptSource } from "$lib/types/work";

  interface Props {
    sources: WorkReceiptSource[];
    searchQueries?: WorkReceiptSearchQuery[];
  }

  let { sources, searchQueries = [] }: Props = $props();

  const accessed = $derived(sources.filter((source) => source.accessed));
  const surfacedOnly = $derived(sources.filter((source) => !source.accessed));
  let showSurfaced = $state(false);

  async function openExternal(url: string) {
    try {
      await platform.shell.openExternal(url);
    } catch {
      window.open(url, "_blank");
    }
  }

  function hostOf(url: string): string {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }

  function formatTime(iso: string | null | undefined): string {
    if (!iso) return "";
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return "";
    return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }
</script>

<div class="space-y-2">
  {#if accessed.length > 0}
    <div>
      <p class="mb-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
        已读取 · {accessed.length}
      </p>
      <ul class="space-y-1">
        {#each accessed as source (source.url)}
          {@const isLibrary = source.kinds.includes("library")}
          {@const badge = isLibrary
            ? "bg-amber-500/10 text-amber-700 dark:text-amber-300"
            : source.kinds.includes("browser")
              ? "bg-violet-500/10 text-violet-600 dark:text-violet-300"
              : "bg-blue-500/10 text-blue-600 dark:text-blue-300"}
          <li
            class="flex items-center gap-2 rounded-lg border border-border/60 bg-card/40 px-2 py-1.5"
          >
            <span class="shrink-0 rounded px-1.5 py-0.5 text-[9px] font-semibold {badge}">
              {isLibrary ? "资料库" : source.kinds.includes("browser") ? "浏览器" : "网页"}
            </span>
            {#if isLibrary}
              <div
                class="min-w-0 flex-1"
                title={source.title ? `${source.title}\n${source.url}` : source.url}
              >
                <span class="block truncate text-[11px] font-medium text-foreground">
                  {source.title || source.url}
                </span>
                <span class="block truncate font-mono text-[10px] text-muted-foreground/70">
                  {source.url}
                </span>
              </div>
            {:else}
              <button
                type="button"
                class="min-w-0 flex-1 text-left"
                title={source.title ? `${source.title}\n${source.url}` : source.url}
                onclick={() => void openExternal(source.url)}
              >
                <span class="block truncate text-[11px] font-medium text-foreground">
                  {source.title || hostOf(source.url)}
                </span>
                <span class="block truncate font-mono text-[10px] text-muted-foreground/70">
                  {source.url}
                </span>
              </button>
            {/if}
            {#if source.lastAccessedAt}
              <span class="shrink-0 text-[10px] text-muted-foreground/60">
                {formatTime(source.lastAccessedAt)}
              </span>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if surfacedOnly.length > 0}
    <div>
      <button
        type="button"
        class="flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground transition-colors hover:text-foreground"
        aria-expanded={showSurfaced}
        onclick={() => (showSurfaced = !showSurfaced)}
      >
        <span class="transition-transform {showSurfaced ? 'rotate-90' : ''}" aria-hidden="true"
          >›</span
        >
        搜索出现（未读取）· {surfacedOnly.length}
      </button>
      {#if showSurfaced}
        <ul class="mt-1 space-y-0.5">
          {#each surfacedOnly as source (source.url)}
            <li>
              <button
                type="button"
                class="w-full truncate rounded px-2 py-1 text-left font-mono text-[10px] text-muted-foreground transition-colors hover:bg-accent/40 hover:text-foreground"
                title={source.url}
                onclick={() => void openExternal(source.url)}
              >
                {source.url}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}

  {#if searchQueries.length > 0}
    <details class="text-[11px] text-muted-foreground">
      <summary class="cursor-pointer select-none text-[10px] font-semibold uppercase tracking-wide">
        搜索记录 · {searchQueries.length}
      </summary>
      <ul class="mt-1 space-y-1">
        {#each searchQueries as search (`${search.timestamp}-${search.query}`)}
          <li class="flex items-baseline gap-2">
            <span class="min-w-0 flex-1 truncate" title={search.query}>{search.query}</span>
            <span class="shrink-0 text-[10px] text-muted-foreground/60">
              {search.resultCount} 条{formatTime(search.timestamp)
                ? ` · ${formatTime(search.timestamp)}`
                : ""}
            </span>
          </li>
        {/each}
      </ul>
    </details>
  {/if}

  {#if sources.length === 0 && searchQueries.length === 0}
    <p
      class="rounded-lg border border-dashed border-border/70 px-3 py-2 text-[11px] text-muted-foreground"
    >
      本次运行没有真实访问记录。来源只统计实际成功的网页读取或资料库引用。
    </p>
  {/if}
</div>
