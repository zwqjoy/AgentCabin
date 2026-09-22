<script lang="ts">
  import ArtifactPreviewModal from "./ArtifactPreviewModal.svelte";
  import WorkArtifactCard from "./WorkArtifactCard.svelte";
  import WorkArtifactProvenanceDrawer from "./WorkArtifactProvenanceDrawer.svelte";
  import { formatArtifactCategory } from "$lib/utils/work-result";
  import type {
    WorkArtifactCategory,
    WorkArtifactStorageMode,
    WorkArtifactSummary,
  } from "$lib/types/work";

  interface RunOption {
    id: string;
    label: string;
  }

  interface Props {
    artifacts: WorkArtifactSummary[];
    loading?: boolean;
    error?: string;
    /** 提供后显示“按 Run 筛选”下拉（Workspace 成果页）。 */
    runOptions?: RunOption[];
    /** 单工作区预览时使用的工作区根目录。 */
    workspaceRoot?: string;
    /** 跨工作区展示时，为每个成果解析其真实工作区根目录。 */
    getWorkspaceRoot?: (artifact: WorkArtifactSummary) => string | undefined;
    primaryWorkRoot?: string;
    artifactStorageMode?: WorkArtifactStorageMode;
    /** 窄面板中使用单列布局，避免视口断点把卡片压成窄列。 */
    compact?: boolean;
    readOnly?: boolean;
    /** 每张卡片的副标题生成器（如跨工作区展示时的工作区名）。 */
    getSubtitle?: (artifact: WorkArtifactSummary) => string | undefined;
    emptyTitle?: string;
    emptyHint?: string;
    /** 头部插槽之外的额外工具（由父级渲染）。 */
    onPreview?: (artifact: WorkArtifactSummary) => void;
    onOpen?: (artifact: WorkArtifactSummary) => void;
    onExport?: (artifact: WorkArtifactSummary) => void;
    onValidate?: (artifact: WorkArtifactSummary) => void;
    onDeliver?: (artifact: WorkArtifactSummary) => void;
    onCopyToPrimary?: (artifact: WorkArtifactSummary) => void;
    onDelete?: (artifact: WorkArtifactSummary) => void;
    onSaveOffice?: (artifactId: string, contentBase64: string) => Promise<void>;
    /** 头部左侧标题（如“成果”）。 */
    title?: string;
    showHeader?: boolean;
  }

  let {
    artifacts,
    loading = false,
    error = "",
    runOptions = [],
    workspaceRoot = "",
    getWorkspaceRoot,
    primaryWorkRoot = "",
    artifactStorageMode = "managed",
    compact = false,
    readOnly = false,
    getSubtitle,
    emptyTitle = "还没有成果",
    emptyHint = "任务生成的文件会自动出现在这里。",
    onPreview,
    onOpen,
    onExport,
    onValidate,
    onDeliver,
    onCopyToPrimary,
    onDelete,
    onSaveOffice,
    title = "成果",
    showHeader = true,
  }: Props = $props();

  let layout = $state<"grid" | "list">("grid");
  let runFilter = $state("all");
  let categoryFilter = $state<"all" | WorkArtifactCategory>("all");
  let previewArtifact = $state<WorkArtifactSummary | null>(null);
  let detailsArtifact = $state<WorkArtifactSummary | null>(null);

  const categories = $derived.by(() => {
    const seen = new Set<WorkArtifactCategory>();
    for (const artifact of artifacts) seen.add(artifact.category ?? "other");
    return Array.from(seen);
  });

  const filtered = $derived(
    artifacts.filter((artifact) => {
      if (runFilter !== "all" && (artifact.runId ?? "") !== runFilter) return false;
      if (categoryFilter !== "all" && (artifact.category ?? "other") !== categoryFilter) {
        return false;
      }
      return true;
    }),
  );

  function handlePreview(artifact: WorkArtifactSummary) {
    previewArtifact = artifact;
    onPreview?.(artifact);
  }

  function hasPreviewRoot(artifact: WorkArtifactSummary): boolean {
    return Boolean(getWorkspaceRoot?.(artifact) || workspaceRoot);
  }

  const previewWorkspaceRoot = $derived(
    previewArtifact ? getWorkspaceRoot?.(previewArtifact) || workspaceRoot : "",
  );

  const previewHandlers = {
    onClose: () => (previewArtifact = null),
    onExport: async (_artifactId: string) => {
      if (previewArtifact && onExport) await onExport(previewArtifact);
    },
    onOpenExternal: async (_artifactId: string) => {
      if (previewArtifact && onOpen) await onOpen(previewArtifact);
    },
  };
</script>

<div class="work-artifact-hub">
  {#if showHeader}
    <div class="flex flex-wrap items-center gap-2 pb-2">
      {#if title}
        <span class="mr-1 text-xs font-semibold text-foreground"
          >{title}
          <span class="ml-1 font-normal text-muted-foreground">{filtered.length}</span>
        </span>
      {/if}
      {#if runOptions.length > 1}
        <select
          class="h-7 rounded-lg border border-border bg-transparent px-2 text-[11px] text-muted-foreground outline-none focus:border-primary/60"
          bind:value={runFilter}
          aria-label="按运行筛选"
        >
          <option value="all">全部运行</option>
          {#each runOptions as run (run.id)}
            <option value={run.id}>{run.label}</option>
          {/each}
        </select>
      {/if}
      {#if categories.length > 1}
        <select
          class="h-7 rounded-lg border border-border bg-transparent px-2 text-[11px] text-muted-foreground outline-none focus:border-primary/60"
          bind:value={categoryFilter}
          aria-label="按类型筛选"
        >
          <option value="all">全部类型</option>
          {#each categories as category (category)}
            <option value={category}>{formatArtifactCategory(category)}</option>
          {/each}
        </select>
      {/if}
      <span class="flex-1"></span>
      <div
        class="flex items-center gap-0.5 rounded-lg border border-border p-0.5"
        role="group"
        aria-label="视图切换"
      >
        <button
          type="button"
          class="rounded-md px-2 py-1 text-[11px] {layout === 'grid'
            ? 'bg-accent text-foreground'
            : 'text-muted-foreground hover:text-foreground'}"
          aria-pressed={layout === "grid"}
          title="网格视图"
          onclick={() => (layout = "grid")}
        >
          网格
        </button>
        <button
          type="button"
          class="rounded-md px-2 py-1 text-[11px] {layout === 'list'
            ? 'bg-accent text-foreground'
            : 'text-muted-foreground hover:text-foreground'}"
          aria-pressed={layout === "list"}
          title="列表视图"
          onclick={() => (layout = "list")}
        >
          列表
        </button>
      </div>
    </div>
  {/if}

  {#if error}
    <div
      class="rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
      role="alert"
    >
      {error}
    </div>
  {:else if loading && artifacts.length === 0}
    <div class="flex items-center gap-2 px-1 py-4 text-xs text-muted-foreground">
      <span
        class="h-4 w-4 animate-spin rounded-full border-2 border-muted-foreground/20 border-t-primary"
      ></span>
      正在加载成果…
    </div>
  {:else if filtered.length === 0}
    <div class="rounded-xl border border-dashed border-border/70 px-4 py-6 text-center">
      <p class="text-xs font-medium text-muted-foreground">{emptyTitle}</p>
      <p class="mt-1 text-[11px] text-muted-foreground/70">{emptyHint}</p>
    </div>
  {:else}
    <div
      class={compact || layout === "list"
        ? "space-y-2"
        : "grid gap-2.5 sm:grid-cols-2 xl:grid-cols-3"}
    >
      {#each filtered as artifact (artifact.id)}
        <WorkArtifactCard
          {artifact}
          subtitle={getSubtitle?.(artifact)}
          onPreview={artifact.canPreview && hasPreviewRoot(artifact) ? handlePreview : undefined}
          {onOpen}
          {onExport}
          {onValidate}
          {onDeliver}
          {onCopyToPrimary}
          {onDelete}
          {readOnly}
          {primaryWorkRoot}
          {artifactStorageMode}
          onShowDetails={(a) => (detailsArtifact = a)}
        />
      {/each}
    </div>
  {/if}
</div>

{#if previewArtifact && previewWorkspaceRoot}
  <ArtifactPreviewModal
    artifact={previewArtifact}
    open
    onClose={previewHandlers.onClose}
    onExport={previewHandlers.onExport}
    onOpenExternal={previewHandlers.onOpenExternal}
    {onSaveOffice}
    workspaceRoot={previewWorkspaceRoot}
  />
{/if}

{#if detailsArtifact}
  <WorkArtifactProvenanceDrawer
    artifact={detailsArtifact}
    open={Boolean(detailsArtifact)}
    onClose={() => (detailsArtifact = null)}
  />
{/if}
