<script lang="ts">
  import { onMount } from "svelte";
  import {
    listWorkLibraryItems,
    getWorkLibraryItem,
    saveWorkLibraryItem,
    deleteWorkLibraryItem,
    addWorkLibraryFile,
    addWorkLibraryDirectory,
  } from "$lib/api/work";
  import {
    filterLibrarySummaries,
    formatLibraryCategory,
    formatLibraryItemReference,
  } from "$lib/utils/work-library";
  import WorkLibraryCard from "./WorkLibraryCard.svelte";
  import WorkLibraryModal from "./WorkLibraryModal.svelte";
  import type {
    LibraryCategory,
    LibraryItem,
    LibraryItemSummary,
    WorkWorkspaceSummary,
  } from "$lib/types/work";

  interface Props {
    workspaceId?: string;
    workspaces?: WorkWorkspaceSummary[];
  }

  let { workspaceId = "", workspaces = [] }: Props = $props();

  let items = $state<LibraryItemSummary[]>([]);
  let loading = $state(true);
  let searchQuery = $state("");
  let collectionFilter = $state("");
  let selectedCategory = $state<LibraryCategory | "all">("all");
  let scopeFilter = $state<"all" | "global" | "workspace">("all");

  // View / Edit Modal State
  let showEditModal = $state(false);
  let editingItem = $state<LibraryItem | null>(null);
  let saving = $state(false);

  // Full Details Drawer State
  let activeDetailItem = $state<LibraryItem | null>(null);

  // Delete State
  let itemToDelete = $state<LibraryItemSummary | null>(null);
  let deleting = $state(false);

  // Copy toast
  let toastMessage = $state("");
  let importing = $state(false);

  function showToast(msg: string) {
    toastMessage = msg;
    setTimeout(() => {
      if (toastMessage === msg) toastMessage = "";
    }, 2500);
  }

  function importWorkspaceId(): string | undefined {
    return workspaceId || undefined;
  }

  async function handleImport(kind: "file" | "directory") {
    const targetWorkspaceId = importWorkspaceId();
    importing = true;
    try {
      const { open } = await import("$lib/platform/dialog");
      const selected = await open({
        multiple: false,
        directory: kind === "directory",
        title: kind === "directory" ? "选择资料目录" : "选择资料文件",
      });
      const selectedPath = typeof selected === "string" ? selected : "";
      if (!selectedPath) return;
      if (kind === "file") {
        await addWorkLibraryFile(targetWorkspaceId, selectedPath, "doc");
        showToast("文件已导入资料库");
      } else {
        const imported = await addWorkLibraryDirectory(targetWorkspaceId, selectedPath, "doc");
        showToast(`已导入 ${imported.length} 个文件`);
      }
      await loadItems();
    } catch (cause) {
      showToast(cause instanceof Error ? cause.message : String(cause));
    } finally {
      importing = false;
    }
  }

  let filtered = $derived(
    filterLibrarySummaries(items, selectedCategory, scopeFilter, workspaceId, searchQuery).filter(
      (item) => !collectionFilter || item.collection === collectionFilter,
    ),
  );

  let collections = $derived(
    Array.from(
      new Set(
        items.map((item) => item.collection).filter((value): value is string => Boolean(value)),
      ),
    ).sort((left, right) => left.localeCompare(right, "zh-CN")),
  );

  let stats = $derived({
    total: items.length,
    global: items.filter((i) => !i.workspaceId).length,
    workspace: items.filter((i) => Boolean(i.workspaceId)).length,
    templates: items.filter((i) => i.category === "template").length,
    rules: items.filter((i) => i.category === "rule").length,
  });

  async function loadItems() {
    loading = true;
    try {
      items = await listWorkLibraryItems(workspaceId || undefined);
    } catch {
      items = [];
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadItems();
  });

  async function handleOpenDetail(summary: LibraryItemSummary) {
    try {
      activeDetailItem = await getWorkLibraryItem(summary.id, summary.workspaceId ?? undefined);
    } catch {
      activeDetailItem = {
        id: summary.id,
        workspaceId: summary.workspaceId,
        title: summary.title,
        description: summary.description,
        category: summary.category,
        content: summary.contentPreview,
        tags: summary.tags,
        sourcePath: summary.sourcePath ?? null,
        collection: summary.collection ?? null,
        metadata: summary.metadata ?? {},
        citations: [],
        sourceArtifactId: summary.sourceArtifactId,
        createdAt: "",
        updatedAt: summary.updatedAt,
      };
    }
  }

  async function handleOpenEdit(summary: LibraryItemSummary) {
    try {
      editingItem = await getWorkLibraryItem(summary.id, summary.workspaceId ?? undefined);
      showEditModal = true;
    } catch {
      // ignore
    }
  }

  function handleOpenCreate() {
    editingItem = null;
    showEditModal = true;
  }

  async function handleSaveItem(item: LibraryItem) {
    saving = true;
    try {
      await saveWorkLibraryItem(item);
      showEditModal = false;
      editingItem = null;
      await loadItems();
      showToast("资料已保存！");
    } finally {
      saving = false;
    }
  }

  async function confirmDelete() {
    if (!itemToDelete) return;
    const deletedId = itemToDelete.id;
    deleting = true;
    try {
      await deleteWorkLibraryItem(deletedId, itemToDelete.workspaceId ?? undefined);
      itemToDelete = null;
      if (activeDetailItem?.id === deletedId) {
        activeDetailItem = null;
      }
      await loadItems();
      showToast("资料已删除");
    } finally {
      deleting = false;
    }
  }

  async function handleCopyContent(item: LibraryItem | LibraryItemSummary) {
    if ("content" in item && item.content) {
      await navigator.clipboard.writeText(item.content);
      showToast("资料全文已复制到剪贴板！");
      return;
    }
    try {
      const full = await getWorkLibraryItem(item.id, item.workspaceId ?? undefined);
      await navigator.clipboard.writeText(full.content);
      showToast("资料全文已复制到剪贴板！");
    } catch {
      await navigator.clipboard.writeText("contentPreview" in item ? item.contentPreview : "");
      showToast("已复制内容摘要");
    }
  }

  async function handleCopyRef(item: LibraryItem | LibraryItemSummary) {
    const refText = formatLibraryItemReference(item);
    await navigator.clipboard.writeText(refText);
    showToast("引用标记已复制！");
  }
</script>

<div class="flex h-full w-full flex-col overflow-y-auto p-4 sm:p-6 space-y-6">
  <!-- Header -->
  <div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
    <div>
      <div class="flex items-center gap-2">
        <h1 class="text-lg font-bold text-foreground">资料库</h1>
        <span
          class="rounded-full bg-primary/10 border border-primary/20 px-2 py-0.5 text-[11px] font-semibold text-primary"
        >
          Library
        </span>
      </div>
      <p class="mt-1 text-xs text-muted-foreground">
        沉淀参考文档、模版、业务规则与数据集，支持在对话或任务指令中一键引用复用
      </p>
    </div>

    <div class="flex flex-wrap items-center justify-end gap-2">
      <button
        type="button"
        class="inline-flex items-center gap-1.5 rounded-xl border border-border px-3 py-2 text-xs font-medium text-muted-foreground transition-all hover:bg-accent hover:text-foreground disabled:opacity-50"
        disabled={importing}
        onclick={() => void handleImport("file")}
      >
        导入文件
      </button>
      <button
        type="button"
        class="inline-flex items-center gap-1.5 rounded-xl border border-border px-3 py-2 text-xs font-medium text-muted-foreground transition-all hover:bg-accent hover:text-foreground disabled:opacity-50"
        disabled={importing}
        onclick={() => void handleImport("directory")}
      >
        导入目录
      </button>
      <button
        type="button"
        class="inline-flex items-center gap-1.5 rounded-xl bg-primary px-4 py-2 text-xs font-semibold text-primary-foreground shadow-xs transition-all hover:bg-primary/90"
        onclick={handleOpenCreate}
      >
        <span class="text-sm font-bold">+</span>
        <span>新建资料</span>
      </button>
    </div>
  </div>

  <!-- Metric Badges -->
  <div class="grid grid-cols-2 gap-3 sm:grid-cols-5">
    <div class="rounded-xl border border-border/70 bg-card/60 p-3.5">
      <span class="text-[11px] text-muted-foreground font-medium block">资料总计</span>
      <span class="mt-1 text-xl font-bold text-foreground block">{stats.total} 篇</span>
    </div>
    <div class="rounded-xl border border-border/70 bg-card/60 p-3.5">
      <span class="text-[11px] text-muted-foreground font-medium block">🌐 全局公共</span>
      <span class="mt-1 text-xl font-bold text-primary block">{stats.global} 篇</span>
    </div>
    <div class="rounded-xl border border-border/70 bg-card/60 p-3.5">
      <span class="text-[11px] text-muted-foreground font-medium block">📁 工作区私有</span>
      <span class="mt-1 text-xl font-bold text-foreground/90 block">{stats.workspace} 篇</span>
    </div>
    <div class="rounded-xl border border-border/70 bg-card/60 p-3.5">
      <span class="text-[11px] text-muted-foreground font-medium block">📑 常用模板</span>
      <span class="mt-1 text-xl font-bold text-purple-600 dark:text-purple-400 block"
        >{stats.templates} 个</span
      >
    </div>
    <div class="rounded-xl border border-border/70 bg-card/60 p-3.5">
      <span class="text-[11px] text-muted-foreground font-medium block">⚖️ 业务规则</span>
      <span class="mt-1 text-xl font-bold text-amber-600 dark:text-amber-400 block"
        >{stats.rules} 条</span
      >
    </div>
  </div>

  <!-- Filters & Search Toolbar -->
  <div class="flex flex-col gap-3 border-t border-border/60 pt-4">
    <!-- Category Tabs -->
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div class="flex flex-wrap items-center gap-1.5">
        <button
          type="button"
          class="rounded-lg px-2.5 py-1 text-xs font-medium transition-colors {selectedCategory ===
          'all'
            ? 'bg-secondary text-secondary-foreground font-semibold'
            : 'text-muted-foreground hover:bg-accent'}"
          onclick={() => (selectedCategory = "all")}
        >
          全部资料 ({stats.total})
        </button>
        <button
          type="button"
          class="rounded-lg px-2.5 py-1 text-xs font-medium transition-colors {selectedCategory ===
          'doc'
            ? 'bg-secondary text-secondary-foreground font-semibold'
            : 'text-muted-foreground hover:bg-accent'}"
          onclick={() => (selectedCategory = "doc")}
        >
          📄 参考文档
        </button>
        <button
          type="button"
          class="rounded-lg px-2.5 py-1 text-xs font-medium transition-colors {selectedCategory ===
          'template'
            ? 'bg-secondary text-secondary-foreground font-semibold'
            : 'text-muted-foreground hover:bg-accent'}"
          onclick={() => (selectedCategory = "template")}
        >
          📑 常用模板 ({stats.templates})
        </button>
        <button
          type="button"
          class="rounded-lg px-2.5 py-1 text-xs font-medium transition-colors {selectedCategory ===
          'rule'
            ? 'bg-secondary text-secondary-foreground font-semibold'
            : 'text-muted-foreground hover:bg-accent'}"
          onclick={() => (selectedCategory = "rule")}
        >
          ⚖️ 业务规则 ({stats.rules})
        </button>
        <button
          type="button"
          class="rounded-lg px-2.5 py-1 text-xs font-medium transition-colors {selectedCategory ===
          'dataset'
            ? 'bg-secondary text-secondary-foreground font-semibold'
            : 'text-muted-foreground hover:bg-accent'}"
          onclick={() => (selectedCategory = "dataset")}
        >
          📊 数据集 / 样本
        </button>
        <button
          type="button"
          class="rounded-lg px-2.5 py-1 text-xs font-medium transition-colors {selectedCategory ===
          'link'
            ? 'bg-secondary text-secondary-foreground font-semibold'
            : 'text-muted-foreground hover:bg-accent'}"
          onclick={() => (selectedCategory = "link")}
        >
          🔗 外部链接
        </button>
      </div>

      <!-- Scope filter -->
      <div class="flex items-center gap-1">
        <button
          type="button"
          class="rounded-md border px-2 py-0.5 text-[11px] font-medium transition-colors {scopeFilter ===
          'all'
            ? 'border-primary/50 bg-primary/10 text-primary'
            : 'border-border text-muted-foreground hover:bg-accent'}"
          onclick={() => (scopeFilter = "all")}
        >
          全部范围
        </button>
        <button
          type="button"
          class="rounded-md border px-2 py-0.5 text-[11px] font-medium transition-colors {scopeFilter ===
          'global'
            ? 'border-primary/50 bg-primary/10 text-primary'
            : 'border-border text-muted-foreground hover:bg-accent'}"
          onclick={() => (scopeFilter = "global")}
        >
          🌐 仅全局
        </button>
        {#if workspaceId}
          <button
            type="button"
            class="rounded-md border px-2 py-0.5 text-[11px] font-medium transition-colors {scopeFilter ===
            'workspace'
              ? 'border-primary/50 bg-primary/10 text-primary'
              : 'border-border text-muted-foreground hover:bg-accent'}"
            onclick={() => (scopeFilter = "workspace")}
          >
            📁 仅当前工作区
          </button>
        {/if}
      </div>
    </div>

    <!-- Search Input -->
    <div class="relative w-full">
      <input
        type="text"
        placeholder="搜索资料库标题、正文描述或标签…"
        class="w-full rounded-xl border border-border bg-background px-3 py-2 text-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary focus:ring-1 focus:ring-primary/20"
        bind:value={searchQuery}
      />
    </div>
    {#if collections.length > 0}
      <select
        class="w-full rounded-xl border border-border bg-background px-3 py-2 text-xs text-foreground outline-none focus:border-primary sm:w-64"
        bind:value={collectionFilter}
        aria-label="按集合筛选"
      >
        <option value="">全部集合</option>
        {#each collections as collection (collection)}
          <option value={collection}>{collection}</option>
        {/each}
      </select>
    {/if}
  </div>

  <!-- Items Grid -->
  {#if loading}
    <div class="flex h-56 items-center justify-center text-xs text-muted-foreground">
      <span class="animate-pulse">正在加载资料库…</span>
    </div>
  {:else if filtered.length === 0}
    <div
      class="flex h-56 flex-col items-center justify-center rounded-2xl border border-dashed border-border/80 p-8 text-center text-xs text-muted-foreground"
    >
      <span class="text-2xl">📚</span>
      <span class="mt-2 font-medium">资料库中暂无匹配的条目</span>
      <p class="mt-1 text-[11px] opacity-75">
        点击右上角「新建资料」，或在任务成果物中点击一键沉淀。
      </p>
    </div>
  {:else}
    <div class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
      {#each filtered as item (item.id)}
        <WorkLibraryCard
          {item}
          {workspaces}
          onSelect={handleOpenDetail}
          onEdit={handleOpenEdit}
          onDelete={(i) => (itemToDelete = i)}
          onCopyContent={handleCopyContent}
        />
      {/each}
    </div>
  {/if}
</div>

<!-- Toast notification -->
{#if toastMessage}
  <div
    class="fixed bottom-6 right-6 z-50 rounded-xl bg-foreground px-4 py-2 text-xs font-medium text-background shadow-xl animate-in fade-in slide-in-from-bottom-2 duration-150"
  >
    {toastMessage}
  </div>
{/if}

<!-- Detail Drawer -->
{#if activeDetailItem}
  {@const catMeta = formatLibraryCategory(activeDetailItem.category)}
  <div
    class="fixed inset-0 z-50 flex items-center justify-end bg-black/30 backdrop-blur-[2px]"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onclick={(e) => e.target === e.currentTarget && (activeDetailItem = null)}
  >
    <div
      class="flex h-full w-full max-w-xl flex-col border-l border-border bg-card shadow-2xl animate-in slide-in-from-right duration-200"
    >
      <div class="flex items-center justify-between border-b border-border/80 px-5 py-4">
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-1.5">
            <span class="rounded border px-2 py-0.5 text-[10px] font-semibold {catMeta.badgeClass}">
              {catMeta.label}
            </span>
            <span class="text-[10px] text-muted-foreground">
              {activeDetailItem.workspaceId ? "工作区私有" : "全局通用"}
            </span>
          </div>
          <h2 class="mt-1.5 text-sm font-semibold text-foreground truncate">
            {activeDetailItem.title}
          </h2>
        </div>
        <button
          type="button"
          class="rounded-lg p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          onclick={() => (activeDetailItem = null)}
        >
          ✕
        </button>
      </div>

      <div class="flex-1 overflow-y-auto p-5 space-y-4">
        {#if activeDetailItem.description}
          <div class="rounded-lg bg-muted/40 p-3 text-xs text-muted-foreground">
            {activeDetailItem.description}
          </div>
        {/if}

        {#if activeDetailItem.sourcePath || activeDetailItem.collection || activeDetailItem.citations.length > 0}
          <div class="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
            {#if activeDetailItem.collection}
              <span class="rounded bg-primary/10 px-2 py-0.5 text-primary">
                集合：{activeDetailItem.collection}
              </span>
            {/if}
            {#if activeDetailItem.sourcePath}
              <span
                class="rounded bg-muted/60 px-2 py-0.5 font-mono"
                title={activeDetailItem.sourcePath}
              >
                来源：{activeDetailItem.sourcePath}
              </span>
            {/if}
            {#if activeDetailItem.citations.length > 0}
              <span class="rounded bg-muted/60 px-2 py-0.5">
                {activeDetailItem.citations.length} 条来源引用
              </span>
            {/if}
          </div>
        {/if}

        {#if Object.keys(activeDetailItem.metadata).length > 0}
          <div class="rounded-lg border border-border/60 bg-muted/20 p-3">
            <div class="mb-1.5 text-[11px] font-semibold text-foreground">Metadata</div>
            <div class="grid grid-cols-1 gap-1 text-[11px] text-muted-foreground sm:grid-cols-2">
              {#each Object.entries(activeDetailItem.metadata) as [key, value] (key)}
                <div class="min-w-0 truncate" title={`${key}: ${value}`}>
                  <span class="font-mono text-foreground/70">{key}</span> = {value}
                </div>
              {/each}
            </div>
          </div>
        {/if}

        <div>
          <div class="flex items-center justify-between mb-1.5">
            <span class="text-xs font-semibold text-foreground">资料正文与模版</span>
            <button
              type="button"
              class="text-[11px] text-primary hover:underline"
              onclick={() => void handleCopyContent(activeDetailItem!)}
            >
              复制正文
            </button>
          </div>
          <pre
            class="max-h-96 overflow-y-auto rounded-xl border border-border/70 bg-muted/20 p-4 font-mono text-xs text-foreground whitespace-pre-wrap select-all leading-relaxed">{activeDetailItem.content}</pre>
        </div>

        {#if activeDetailItem.tags.length > 0}
          <div class="flex flex-wrap gap-1.5 pt-2">
            {#each activeDetailItem.tags as tag (tag)}
              <span
                class="rounded bg-secondary px-2 py-0.5 text-[10px] text-secondary-foreground font-medium"
              >
                #{tag}
              </span>
            {/each}
          </div>
        {/if}
      </div>

      <div class="flex items-center justify-between border-t border-border/80 px-5 py-3.5 bg-card">
        <button
          type="button"
          class="rounded-lg border border-border px-3 py-1.5 text-xs text-muted-foreground hover:bg-accent"
          onclick={() => void handleCopyRef(activeDetailItem!)}
        >
          复制上下文引用标记
        </button>

        <button
          type="button"
          class="rounded-lg bg-primary px-4 py-1.5 text-xs font-semibold text-primary-foreground hover:bg-primary/90"
          onclick={() => {
            editingItem = activeDetailItem;
            activeDetailItem = null;
            showEditModal = true;
          }}
        >
          编辑资料
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Create / Edit Modal -->
{#if showEditModal}
  <WorkLibraryModal
    item={editingItem}
    {workspaces}
    initialWorkspaceId={workspaceId}
    {saving}
    onSave={handleSaveItem}
    onClose={() => {
      showEditModal = false;
      editingItem = null;
    }}
  />
{/if}

<!-- Delete Confirm Modal -->
{#if itemToDelete}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4 backdrop-blur-[2px]"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <div class="w-full max-w-sm rounded-2xl border border-border bg-card p-5 shadow-2xl space-y-4">
      <h3 class="text-sm font-semibold text-foreground">确认删除资料条目？</h3>
      <p class="text-xs text-muted-foreground">
        资料「{itemToDelete.title}」将被永久删除。
      </p>
      <div class="flex items-center justify-end gap-2 pt-2">
        <button
          type="button"
          class="rounded-lg border border-border px-3 py-1.5 text-xs text-foreground hover:bg-accent"
          onclick={() => (itemToDelete = null)}
        >
          取消
        </button>
        <button
          type="button"
          class="rounded-lg bg-red-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-red-700 disabled:opacity-50"
          disabled={deleting}
          onclick={confirmDelete}
        >
          {deleting ? "删除中…" : "确认删除"}
        </button>
      </div>
    </div>
  </div>
{/if}
