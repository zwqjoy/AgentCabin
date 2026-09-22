<script lang="ts">
  import { onMount } from "svelte";
  import { listWorkLibraryItems, getWorkLibraryItem } from "$lib/api/work";
  import {
    formatLibraryCategory,
    filterLibrarySummaries,
    formatLibraryItemReference,
  } from "$lib/utils/work-library";
  import type { LibraryCategory, LibraryItemSummary } from "$lib/types/work";

  interface Props {
    workspaceId?: string;
    onInsert: (content: string, title: string) => void;
    onClose: () => void;
  }

  let { workspaceId = "", onInsert, onClose }: Props = $props();

  let items = $state<LibraryItemSummary[]>([]);
  let loading = $state(true);
  let searchQuery = $state("");
  let selectedCategory = $state<LibraryCategory | "all">("all");
  let fetchingItem = $state(false);

  let filtered = $derived(
    filterLibrarySummaries(items, selectedCategory, "all", workspaceId, searchQuery),
  );

  onMount(async () => {
    try {
      items = await listWorkLibraryItems(workspaceId || undefined);
    } catch {
      items = [];
    } finally {
      loading = false;
    }
  });

  async function handlePick(item: LibraryItemSummary) {
    fetchingItem = true;
    try {
      const full = await getWorkLibraryItem(
        item.id,
        (item.workspaceId ?? workspaceId) || undefined,
      );
      onInsert(formatLibraryItemReference(full), full.title);
      onClose();
    } catch {
      onInsert(formatLibraryItemReference(item), item.title);
      onClose();
    } finally {
      fetchingItem = false;
    }
  }
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4 backdrop-blur-[2px]"
  role="dialog"
  aria-modal="true"
  tabindex="-1"
  onclick={(e) => e.target === e.currentTarget && onClose()}
>
  <div
    class="flex h-[520px] w-full max-w-lg flex-col rounded-2xl border border-border bg-card shadow-2xl overflow-hidden"
  >
    <!-- Header -->
    <div class="flex items-center justify-between border-b border-border px-5 py-3.5">
      <div>
        <h3 class="text-sm font-semibold text-foreground">从资料库引用知识与模版</h3>
        <p class="text-[11px] text-muted-foreground">
          选择一条资料，将可追溯引用标记插入当前对话或任务
        </p>
      </div>
      <button
        type="button"
        class="rounded-lg p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
        onclick={onClose}
      >
        ✕
      </button>
    </div>

    <!-- Search and filter -->
    <div class="border-b border-border/60 bg-muted/20 p-3 space-y-2">
      <input
        type="text"
        placeholder="搜索资料库标题、正文或标签…"
        class="w-full rounded-xl border border-border bg-background px-3 py-1.5 text-xs text-foreground outline-none focus:border-primary"
        bind:value={searchQuery}
      />

      <div class="flex flex-wrap items-center gap-1">
        <button
          type="button"
          class="rounded-md px-2 py-0.5 text-[11px] font-medium {selectedCategory === 'all'
            ? 'bg-primary text-primary-foreground'
            : 'bg-card text-muted-foreground border border-border'}"
          onclick={() => (selectedCategory = "all")}
        >
          全部
        </button>
        <button
          type="button"
          class="rounded-md px-2 py-0.5 text-[11px] font-medium {selectedCategory === 'doc'
            ? 'bg-primary text-primary-foreground'
            : 'bg-card text-muted-foreground border border-border'}"
          onclick={() => (selectedCategory = "doc")}
        >
          📄 文档
        </button>
        <button
          type="button"
          class="rounded-md px-2 py-0.5 text-[11px] font-medium {selectedCategory === 'template'
            ? 'bg-primary text-primary-foreground'
            : 'bg-card text-muted-foreground border border-border'}"
          onclick={() => (selectedCategory = "template")}
        >
          📑 模版
        </button>
        <button
          type="button"
          class="rounded-md px-2 py-0.5 text-[11px] font-medium {selectedCategory === 'rule'
            ? 'bg-primary text-primary-foreground'
            : 'bg-card text-muted-foreground border border-border'}"
          onclick={() => (selectedCategory = "rule")}
        >
          ⚖️ 规则
        </button>
        <button
          type="button"
          class="rounded-md px-2 py-0.5 text-[11px] font-medium {selectedCategory === 'dataset'
            ? 'bg-primary text-primary-foreground'
            : 'bg-card text-muted-foreground border border-border'}"
          onclick={() => (selectedCategory = "dataset")}
        >
          📊 数据集
        </button>
      </div>
    </div>

    <!-- Items list -->
    <div class="flex-1 overflow-y-auto p-3 space-y-2">
      {#if loading}
        <div
          class="flex h-40 items-center justify-center text-xs text-muted-foreground animate-pulse"
        >
          正在加载资料库…
        </div>
      {:else if filtered.length === 0}
        <div class="flex h-40 flex-col items-center justify-center text-xs text-muted-foreground">
          <span>📁 未检索到匹配的资料</span>
        </div>
      {:else}
        {#each filtered as item (item.id)}
          {@const catMeta = formatLibraryCategory(item.category)}
          <button
            type="button"
            class="flex w-full flex-col rounded-xl border border-border/70 bg-card p-3 text-left transition-all hover:border-primary/60 hover:bg-primary/5 hover:shadow-xs disabled:opacity-50"
            disabled={fetchingItem}
            onclick={() => void handlePick(item)}
          >
            <div class="flex items-center justify-between gap-2">
              <div class="flex items-center gap-1.5">
                <span
                  class="rounded border px-1.5 py-0.5 text-[9px] font-medium {catMeta.badgeClass}"
                >
                  {catMeta.label}
                </span>
                <span class="text-xs font-semibold text-foreground truncate">{item.title}</span>
              </div>
              <span class="text-[10px] text-muted-foreground">
                {item.workspaceId ? "工作区私有" : "全局通用"}
              </span>
            </div>

            {#if item.description}
              <p class="mt-1 line-clamp-1 text-[11px] text-muted-foreground">
                {item.description}
              </p>
            {/if}

            {#if item.contentPreview}
              <div
                class="mt-1.5 rounded bg-muted/40 p-1.5 font-mono text-[10px] text-muted-foreground/80 line-clamp-2"
              >
                {item.contentPreview}
              </div>
            {/if}
          </button>
        {/each}
      {/if}
    </div>
  </div>
</div>
