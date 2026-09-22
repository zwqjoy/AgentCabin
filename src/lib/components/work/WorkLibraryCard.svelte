<script lang="ts">
  import { formatLibraryCategory } from "$lib/utils/work-library";
  import type { LibraryItemSummary, WorkWorkspaceSummary } from "$lib/types/work";

  interface Props {
    item: LibraryItemSummary;
    workspaces: WorkWorkspaceSummary[];
    onSelect?: (item: LibraryItemSummary) => void;
    onEdit?: (item: LibraryItemSummary) => void;
    onDelete?: (item: LibraryItemSummary) => void;
    onCopyContent?: (item: LibraryItemSummary) => void;
  }

  let { item, workspaces, onSelect, onEdit, onDelete, onCopyContent }: Props = $props();

  const categoryMeta = $derived(formatLibraryCategory(item.category));
  const isGlobal = $derived(!item.workspaceId);
  const workspaceName = $derived(
    isGlobal
      ? "全局通用"
      : workspaces.find((w) => w.id === item.workspaceId)?.name || item.workspaceId,
  );
</script>

<div
  class="group flex flex-col justify-between rounded-xl border border-border/70 bg-card/60 p-4 transition-all hover:border-border hover:bg-card/90 hover:shadow-sm"
>
  <!-- Card Header & Metadata -->
  <div>
    <div class="flex items-start justify-between gap-2">
      <div class="flex flex-wrap items-center gap-1.5">
        <span
          class="flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] font-semibold {categoryMeta.badgeClass}"
        >
          <span>{categoryMeta.icon}</span>
          <span>{categoryMeta.label}</span>
        </span>

        <span
          class="rounded-full px-2 py-0.5 text-[10px] font-medium {isGlobal
            ? 'bg-primary/10 text-primary border border-primary/20'
            : 'bg-muted text-muted-foreground border border-border'}"
        >
          {workspaceName}
        </span>

        {#if item.sourceArtifactId}
          <span
            class="rounded bg-emerald-500/10 px-1.5 py-0.5 text-[9px] font-medium text-emerald-600 dark:text-emerald-400 border border-emerald-500/20"
          >
            成果物沉淀
          </span>
        {/if}

        {#if item.collection}
          <span class="rounded bg-primary/10 px-1.5 py-0.5 text-[9px] font-medium text-primary">
            {item.collection}
          </span>
        {/if}
      </div>

      <div class="text-[10px] text-muted-foreground font-mono">
        {item.updatedAt.slice(0, 10)}
      </div>
    </div>

    <!-- Title & Description -->
    <h3
      class="mt-2.5 text-sm font-semibold text-foreground leading-snug truncate group-hover:text-primary transition-colors cursor-pointer"
      onclick={() => onSelect?.(item)}
      onkeydown={(e) => e.key === "Enter" && onSelect?.(item)}
      tabindex="0"
      role="button"
    >
      {item.title}
    </h3>

    {#if item.description}
      <p class="mt-1 line-clamp-2 text-xs text-muted-foreground leading-relaxed">
        {item.description}
      </p>
    {/if}

    <!-- Content Preview Box -->
    {#if item.contentPreview}
      <div
        class="mt-3 rounded-lg bg-muted/40 p-2.5 text-[11px] font-mono text-muted-foreground/90 line-clamp-3 leading-relaxed border border-border/40 select-all"
      >
        {item.contentPreview}
      </div>
    {/if}

    <!-- Tags -->
    {#if item.tags && item.tags.length > 0}
      <div class="mt-3 flex flex-wrap gap-1">
        {#each item.tags as tag (tag)}
          <span
            class="rounded bg-secondary/70 px-1.5 py-0.5 text-[9px] font-medium text-secondary-foreground"
          >
            #{tag}
          </span>
        {/each}
      </div>
    {/if}

    {#if item.sourcePath || item.citationCount > 0}
      <div class="mt-2 flex items-center gap-2 text-[10px] text-muted-foreground">
        {#if item.sourcePath}
          <span class="min-w-0 truncate font-mono" title={item.sourcePath}>来源文件</span>
        {/if}
        {#if item.citationCount > 0}
          <span>{item.citationCount} 条引用</span>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Card Actions Footer -->
  <div class="mt-4 flex items-center justify-between border-t border-border/50 pt-3">
    <div class="flex items-center gap-1.5">
      {#if onCopyContent}
        <button
          type="button"
          class="rounded-lg border border-border/70 px-2.5 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
          onclick={() => onCopyContent(item)}
        >
          复制正文
        </button>
      {/if}

      {#if onSelect}
        <button
          type="button"
          class="rounded-lg bg-primary/10 border border-primary/20 px-2.5 py-1 text-xs font-medium text-primary hover:bg-primary/20 transition-colors"
          onclick={() => onSelect(item)}
        >
          查看全文
        </button>
      {/if}
    </div>

    <div class="flex items-center gap-1">
      {#if onEdit}
        <button
          type="button"
          class="rounded p-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
          title="编辑资料"
          onclick={() => onEdit(item)}
        >
          编辑
        </button>
      {/if}
      {#if onDelete}
        <button
          type="button"
          class="rounded p-1 text-xs text-red-500 hover:bg-red-500/10 transition-colors"
          title="删除资料"
          onclick={() => onDelete(item)}
        >
          删除
        </button>
      {/if}
    </div>
  </div>
</div>
