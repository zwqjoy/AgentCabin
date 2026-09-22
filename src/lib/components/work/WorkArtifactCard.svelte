<script lang="ts">
  import { formatArtifactType, formatFileSize } from "$lib/utils/work-result";
  import WorkArtifactValidationBadge from "./WorkArtifactValidationBadge.svelte";
  import type { WorkArtifactSummary } from "$lib/types/work";

  interface Props {
    artifact: WorkArtifactSummary;
    /** 预览由 Hub 统一处理；缺省时隐藏预览按钮。 */
    onPreview?: (artifact: WorkArtifactSummary) => void;
    onOpen?: (artifact: WorkArtifactSummary) => void;
    onExport?: (artifact: WorkArtifactSummary) => void;
    onValidate?: (artifact: WorkArtifactSummary) => void;
    onDeliver?: (artifact: WorkArtifactSummary) => void;
    onCopyToPrimary?: (artifact: WorkArtifactSummary) => void;
    onDelete?: (artifact: WorkArtifactSummary) => void;
    onShowDetails?: (artifact: WorkArtifactSummary) => void;
    readOnly?: boolean;
    primaryWorkRoot?: string;
    artifactStorageMode?: "managed" | "primary_work_root";
    /** 替代路径行的副标题（如跨工作区展示时的工作区名）。 */
    subtitle?: string;
    /** 选中态（列表模式下高亮当前项）。 */
    selected?: boolean;
  }

  let {
    artifact,
    onPreview,
    onOpen,
    onExport,
    onValidate,
    onDeliver,
    onCopyToPrimary,
    onDelete,
    onShowDetails,
    readOnly = false,
    primaryWorkRoot = "",
    artifactStorageMode = "managed",
    subtitle,
    selected = false,
  }: Props = $props();

  const typeLabel = $derived(formatArtifactType(artifact.artifactType, artifact.path));
  const canPreview = $derived(Boolean(onPreview) && artifact.canPreview);
</script>

<div
  class="group rounded-xl border {selected
    ? 'border-primary/50 bg-primary/5'
    : 'border-border/70 bg-card/40'} px-3 py-2.5 transition-colors hover:border-border"
>
  <div class="flex items-start justify-between gap-2">
    <div class="min-w-0">
      <p class="truncate text-sm font-medium text-foreground" title={artifact.title}>
        {artifact.title}
      </p>
      <p
        class="mt-0.5 truncate font-mono text-[11px] text-muted-foreground"
        title={subtitle ?? artifact.path}
      >
        {subtitle ?? artifact.path}
      </p>
    </div>
    <WorkArtifactValidationBadge {artifact} />
  </div>
  <div class="mt-1.5 flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground/80">
    <span>{typeLabel} · {formatFileSize(artifact.size)}</span>
    {#if artifact.version > 1}
      <span
        class="rounded bg-muted px-1.5 py-0.5 text-[9px]"
        title="内容已第 {artifact.version} 次变更"
      >
        v{artifact.version}
      </span>
    {/if}
    {#if artifact.validationSummary}
      <span
        class="rounded bg-muted/60 px-1.5 py-0.5 text-[9px] text-muted-foreground"
        title={artifact.validationSummary}
      >
        {artifact.validationSummary}
      </span>
    {/if}
    {#if artifact.sources && artifact.sources.length > 0}
      <span
        class="rounded bg-blue-500/10 px-1.5 py-0.5 text-[9px] text-blue-600 dark:text-blue-400"
      >
        {artifact.sources.length} 个来源
      </span>
    {/if}
  </div>
  <div class="mt-2 flex flex-wrap justify-end gap-1.5 border-t border-border/50 pt-2">
    {#if onShowDetails}
      <button
        type="button"
        class="min-h-8 rounded-lg border border-border px-2.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        title="查看产物溯源与校验详情"
        onclick={() => onShowDetails?.(artifact)}
      >
        详情
      </button>
    {/if}
    {#if canPreview}
      <button
        type="button"
        class="min-h-8 rounded-lg border border-primary/40 bg-primary/10 px-2.5 text-[11px] font-medium text-primary transition-colors hover:bg-primary/20"
        onclick={() => onPreview?.(artifact)}
      >
        预览
      </button>
    {/if}
    {#if onOpen}
      <button
        type="button"
        class="min-h-8 rounded-lg border border-border px-2.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        title="用系统默认应用打开"
        onclick={() => onOpen?.(artifact)}
      >
        打开
      </button>
    {/if}
    {#if onExport}
      <button
        type="button"
        class="min-h-8 rounded-lg border border-border px-2.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
        disabled={artifact.status !== "delivered"}
        title={artifact.status === "delivered"
          ? "另存为 / 导出已交付文件"
          : "文件完成交付后才能导出"}
        onclick={() => onExport?.(artifact)}
      >
        另存为
      </button>
    {/if}
    {#if onValidate && !readOnly && (artifact.status === "ready" || artifact.status === "invalid")}
      <button
        type="button"
        class="min-h-8 rounded-lg border border-amber-500/40 bg-amber-500/10 px-2.5 text-[11px] font-medium text-amber-700 transition-colors hover:bg-amber-500/20 dark:text-amber-300"
        onclick={() => onValidate?.(artifact)}
      >
        {artifact.status === "invalid" ? "重新验证" : "验证文件"}
      </button>
    {/if}
    {#if onDeliver && !readOnly && artifact.status === "validated"}
      <button
        type="button"
        class="min-h-8 rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-2.5 text-[11px] font-medium text-emerald-700 transition-colors hover:bg-emerald-500/20 dark:text-emerald-300"
        onclick={() => onDeliver?.(artifact)}
      >
        确认交付
      </button>
    {/if}
    {#if onCopyToPrimary && primaryWorkRoot && artifactStorageMode !== "primary_work_root" && artifact.status === "delivered"}
      <button
        type="button"
        class="min-h-8 rounded-lg border border-blue-500/40 bg-blue-500/10 px-2.5 text-[11px] font-medium text-blue-700 transition-colors hover:bg-blue-500/20 dark:text-blue-300"
        onclick={() => onCopyToPrimary?.(artifact)}
      >
        复制到本地
      </button>
    {/if}
    {#if onDelete && !readOnly}
      <button
        type="button"
        class="min-h-8 rounded-lg border border-border px-2.5 text-[11px] font-medium text-muted-foreground transition-colors hover:border-red-400/40 hover:bg-red-400/10 hover:text-red-500"
        onclick={() => onDelete?.(artifact)}
      >
        删除
      </button>
    {/if}
  </div>
</div>
