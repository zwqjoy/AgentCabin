<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { trapFocus } from "$lib/utils/focus-trap";
  import ArtifactPreviewModal from "./ArtifactPreviewModal.svelte";
  import WorkArtifactHub from "./WorkArtifactHub.svelte";
  import type {
    WorkArtifactAcceptance,
    WorkArtifactRequirement,
    WorkArtifactStorageMode,
    WorkArtifactSummary,
  } from "$lib/types/work";
  import { formatArtifactStatus, formatArtifactType, formatFileSize } from "$lib/utils/work-result";

  interface Props {
    artifacts: WorkArtifactSummary[];
    requirements?: WorkArtifactRequirement[];
    requiredArtifacts?: string[];
    acceptance?: WorkArtifactAcceptance | null;
    readOnly?: boolean;
    compact?: boolean;
    showHeader?: boolean;
    workspaceRoot?: string;
    primaryWorkRoot?: string;
    artifactStorageMode?: WorkArtifactStorageMode;
    onExport: (artifactId: string) => Promise<void>;
    onOpen: (artifactId: string) => Promise<void>;
    onDelete: (artifactId: string) => Promise<void>;
    onCopyToPrimary?: (artifactId: string) => Promise<string>;
    onValidate?: (artifactId: string) => Promise<void>;
    onDeliver?: (artifactId: string) => Promise<void>;
    onOpenDirectory?: () => Promise<void>;
    onSaveOffice?: (artifactId: string, contentBase64: string) => Promise<void>;
  }

  let {
    artifacts,
    requirements = [],
    requiredArtifacts = [],
    acceptance = null,
    readOnly = false,
    compact = false,
    showHeader = true,
    workspaceRoot = "",
    primaryWorkRoot = "",
    artifactStorageMode = "managed",
    onExport,
    onOpen,
    onDelete,
    onCopyToPrimary,
    onValidate,
    onDeliver,
    onOpenDirectory,
    onSaveOffice,
  }: Props = $props();
  let busyId = $state("");
  let error = $state("");
  let notice = $state("");
  let directoryBusy = $state(false);
  let previewArtifact = $state<WorkArtifactSummary | null>(null);
  let artifactToDelete = $state<WorkArtifactSummary | null>(null);
  let deleteDialog = $state<HTMLDivElement>();
  const useUnifiedArtifactHub = true;

  let expectedRequirements = $derived.by(() => {
    const allReqs: Array<{ path: string; title: string; type: string }> = [];
    if (requirements) {
      for (const req of requirements) {
        allReqs.push({
          path: req.path,
          title: req.title || req.path,
          type: req.artifactType || "",
        });
      }
    }
    if (requiredArtifacts) {
      for (const path of requiredArtifacts) {
        if (!allReqs.some((r) => r.path === path)) {
          allReqs.push({ path, title: path, type: "" });
        }
      }
    }
    return allReqs.filter(
      (req) =>
        !artifacts.some(
          (a) => a.path.endsWith(req.path) || a.title === req.title || a.path === req.path,
        ),
    );
  });

  function getArtifactAcceptanceCheck(artifact: WorkArtifactSummary) {
    if (!acceptance || !acceptance.checks) return null;
    return (
      acceptance.checks.find(
        (c) =>
          c.artifactId === artifact.id ||
          c.requirement.path === artifact.path ||
          artifact.path.endsWith(c.requirement.path),
      ) ?? null
    );
  }

  $effect(() => {
    if (!artifactToDelete || !deleteDialog) return;
    requestAnimationFrame(() => {
      deleteDialog
        ?.querySelector<HTMLElement>("button:not([disabled]), input:not([disabled])")
        ?.focus();
    });
  });

  async function openDirectory() {
    if (!onOpenDirectory || directoryBusy) return;
    directoryBusy = true;
    error = "";
    try {
      await onOpenDirectory();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      directoryBusy = false;
    }
  }

  function handlePreview(artifact: WorkArtifactSummary) {
    previewArtifact = artifact;
  }

  function promptDelete(artifact: WorkArtifactSummary) {
    artifactToDelete = artifact;
  }

  async function confirmDeleteArtifact() {
    if (!artifactToDelete) return;
    const id = artifactToDelete.id;
    artifactToDelete = null;
    await run("delete", id);
  }

  async function run(
    action: "export" | "open" | "delete" | "validate" | "deliver" | "copy",
    artifactId: string,
  ) {
    if (readOnly && action !== "open") return;
    busyId = artifactId;
    error = "";
    notice = "";
    try {
      if (action === "export") await onExport(artifactId);
      else if (action === "delete") await onDelete(artifactId);
      else if (action === "validate") await onValidate?.(artifactId);
      else if (action === "deliver") await onDeliver?.(artifactId);
      else if (action === "copy") {
        const destination = await onCopyToPrimary?.(artifactId);
        notice = destination ? `已复制到本地工作目录：${destination}` : "已复制到本地工作目录。";
      } else await onOpen(artifactId);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = "";
    }
  }
</script>

<section
  class={compact
    ? "bg-transparent"
    : "mt-5 rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5"}
>
  {#if showHeader}
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div>
        <div class="flex items-center gap-2">
          <h2 class="text-sm font-semibold text-foreground">成果</h2>
          <span
            class="rounded-full bg-emerald-500/10 px-2 py-0.5 text-[11px] text-emerald-600 dark:text-emerald-300"
            >{artifacts.length}</span
          >
        </div>
        <p class="mt-1 text-xs leading-5 text-muted-foreground">
          Agent 生成的文件会集中显示在这里。可直接预览、导出或打开输出目录。
        </p>
        {#if primaryWorkRoot}
          <p class="mt-1 text-[11px] leading-5 text-muted-foreground/80">
            {#if artifactStorageMode === "primary_work_root"}
              当前为直写模式，成果会直接保存到本地目录的 <code class="rounded bg-muted px-1"
                >output/</code
              >。
            {:else}
              当前成果先保存在 AgentCabin 托管的 <code class="rounded bg-muted px-1">output/</code
              >；需要放回本地目录时，可对已交付成果执行“复制到本地目录”。
            {/if}
          </p>
        {/if}
      </div>
      {#if onOpenDirectory}
        <button
          type="button"
          class="inline-flex min-h-8 shrink-0 items-center gap-1.5 rounded-lg border border-border px-2.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
          disabled={directoryBusy}
          onclick={() => void openDirectory()}
        >
          <svg
            viewBox="0 0 24 24"
            class="h-3.5 w-3.5"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M3.5 6.5h6l2 2h9v9a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z" />
          </svg>
          打开目录
        </button>
      {/if}
    </div>
  {/if}

  {#if !showHeader && primaryWorkRoot}
    <div
      class="mb-3 rounded-lg border border-blue-500/20 bg-blue-500/5 px-3 py-2 text-[11px] leading-5 text-muted-foreground"
    >
      {#if artifactStorageMode === "primary_work_root"}
        成果会直接写入本地工作目录的 <code class="rounded bg-muted px-1">output/</code>。
      {:else}
        成果当前保存在 AgentCabin 托管的 <code class="rounded bg-muted px-1">output/</code
        >；已交付后可复制到本地目录。
      {/if}
    </div>
  {/if}

  {#if notice}
    <div
      class="rounded-lg border border-emerald-400/20 bg-emerald-400/5 px-3 py-2 text-xs text-emerald-600 dark:text-emerald-400"
      role="status"
    >
      {notice}
    </div>
  {/if}

  {#if error}
    <div
      class="{showHeader
        ? 'mt-3'
        : 'mt-0'} rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
      role="alert"
    >
      {error}
    </div>
  {/if}

  {#if artifacts.length === 0 && expectedRequirements.length === 0}
    <div
      class="{showHeader
        ? 'mt-4'
        : 'mt-0'} rounded-xl border border-dashed border-border/70 px-4 py-5 text-center text-xs text-muted-foreground"
    >
      任务生成并登记交付物后，会显示在这里。
    </div>
  {:else}
    <!-- Expected Requirements that haven't been created yet -->
    {#if expectedRequirements.length > 0}
      <div class="mb-3 space-y-2">
        <div class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
          预期交付物 ({expectedRequirements.length})
        </div>
        <div class={compact ? "space-y-2" : "grid gap-3 lg:grid-cols-2"}>
          {#each expectedRequirements as req (req.path)}
            <div
              class="rounded-xl border border-dashed border-border/80 bg-card/30 p-3 flex items-start gap-2.5"
            >
              <div
                class="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-muted/60 text-muted-foreground"
                aria-hidden="true"
              >
                <svg
                  viewBox="0 0 24 24"
                  class="h-4 w-4"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.8"
                >
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                  <polyline points="14 2 14 8 20 8" />
                </svg>
              </div>
              <div class="min-w-0 flex-1">
                <div class="flex items-center justify-between gap-2">
                  <h4 class="truncate text-xs font-semibold text-foreground/90">{req.title}</h4>
                  <span
                    class="rounded-full bg-muted/80 px-2 py-0.5 text-[9px] font-medium text-muted-foreground"
                  >
                    预期产出
                  </span>
                </div>
                <p class="mt-1 font-mono text-[10px] text-muted-foreground truncate">{req.path}</p>
                <p class="mt-1 text-[10px] text-muted-foreground/70">
                  {req.type ? formatArtifactType(req.type, req.path) : "待生成文件"} · 等待 Agent 任务执行
                </p>
              </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    {#if !useUnifiedArtifactHub}
      <!-- Created Artifacts List with full lifecycle -->
      {#if artifacts.length > 0}
        <div class={compact ? "mt-3 space-y-2" : "mt-4 grid gap-3 lg:grid-cols-2"}>
          {#each artifacts as artifact (artifact.id)}
            {@const check = getArtifactAcceptanceCheck(artifact)}
            {@const isInvalid =
              artifact.status === "invalid" || (check && check.status === "invalid")}
            <article
              class="group rounded-xl border bg-background/50 p-3 transition-colors hover:border-primary/40 hover:bg-accent/20 {isInvalid
                ? 'border-red-500/40 bg-red-500/[0.02]'
                : artifact.status === 'delivered'
                  ? 'border-emerald-500/30'
                  : 'border-border/60'}"
            >
              <button
                type="button"
                class="flex w-full items-start gap-2.5 cursor-pointer border-0 bg-transparent p-0 text-left"
                onclick={() => handlePreview(artifact)}
              >
                <div
                  class="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground group-hover:bg-primary/10 group-hover:text-primary transition-colors"
                  aria-hidden="true"
                >
                  <svg
                    viewBox="0 0 24 24"
                    class="h-4 w-4"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.7"
                  >
                    <path d="M6 3.75h8.25L18.5 8v12.25H6z" />
                    <path d="M14 3.75V8h4.5M8.5 12h7M8.5 15h7" />
                  </svg>
                </div>
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-center gap-2">
                    <h3
                      class="min-w-0 flex-1 truncate text-sm font-medium text-foreground group-hover:text-primary transition-colors"
                    >
                      {artifact.title}
                    </h3>
                    <span
                      class="rounded-full px-2 py-0.5 text-[10px] font-semibold {artifact.status ===
                      'delivered'
                        ? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400'
                        : artifact.status === 'validated'
                          ? 'bg-blue-500/15 text-blue-600 dark:text-blue-400'
                          : isInvalid
                            ? 'bg-red-500/15 text-red-600 dark:text-red-400'
                            : 'bg-muted text-muted-foreground'}"
                      title={formatArtifactStatus(artifact.status).tooltip}
                    >
                      {formatArtifactStatus(artifact.status).label}
                    </span>
                  </div>
                  <p class="mt-1 truncate font-mono text-[11px] text-muted-foreground">
                    {artifact.path}
                  </p>
                  <div
                    class="mt-1 flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground/70"
                  >
                    <span
                      >{formatArtifactType(artifact.artifactType, artifact.path)} · {formatFileSize(
                        artifact.size,
                      )}</span
                    >
                    {#if artifact.sha256}
                      <span
                        class="rounded bg-muted px-1.5 py-0.5 font-mono text-[9px] text-muted-foreground"
                        title="SHA-256: {artifact.sha256}"
                      >
                        SHA: {artifact.sha256.slice(0, 8)}...
                      </span>
                    {/if}
                    {#if artifact.evidence}
                      <span
                        class="rounded bg-emerald-500/10 px-1.5 py-0.5 text-[9px] font-medium text-emerald-600 dark:text-emerald-400"
                        title={artifact.validationSummary ??
                          `验证于: ${artifact.evidence.verifiedAt}`}
                      >
                        ✓ 证据已校验
                      </span>
                    {/if}
                  </div>
                  {#if (artifact.sources && artifact.sources.length > 0) || artifact.producer?.producerToolCallId}
                    <div
                      class="mt-1.5 flex flex-wrap items-center gap-1 text-[10px] text-muted-foreground"
                    >
                      {#if artifact.producer?.producerToolCallId}
                        <span class="rounded bg-primary/5 px-1 py-0.5"
                          >产出工具: {artifact.producer.producerToolCallId}</span
                        >
                      {/if}
                      {#if artifact.sources && artifact.sources.length > 0}
                        <span
                          class="rounded bg-blue-500/10 px-1 py-0.5 text-blue-600 dark:text-blue-400"
                        >
                          {artifact.sources.length} 个已验证来源
                        </span>
                      {/if}
                    </div>
                  {/if}
                  {#if check && check.message && isInvalid}
                    <div
                      class="mt-1.5 rounded bg-red-500/10 border border-red-500/20 px-2 py-1 text-[10px] text-red-600 dark:text-red-400"
                    >
                      {check.message}
                    </div>
                  {/if}
                </div>
              </button>
              <div class="mt-2 flex flex-wrap justify-end gap-1.5 border-t border-border/50 pt-2">
                <button
                  type="button"
                  class="min-h-8 rounded-lg border border-primary/40 bg-primary/10 px-2.5 text-[11px] font-medium text-primary transition-colors hover:bg-primary/20 disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={busyId === artifact.id}
                  onclick={() => handlePreview(artifact)}
                >
                  预览
                </button>
                {#if onValidate && !readOnly && (artifact.status === "ready" || artifact.status === "invalid")}
                  <button
                    type="button"
                    class="min-h-8 rounded-lg border border-amber-500/40 bg-amber-500/10 px-2.5 text-[11px] font-medium text-amber-700 transition-colors hover:bg-amber-500/20 dark:text-amber-300 disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={busyId === artifact.id}
                    onclick={() => void run("validate", artifact.id)}
                  >
                    {artifact.status === "invalid" ? "重新验证" : "验证文件"}
                  </button>
                {/if}
                {#if onDeliver && !readOnly && artifact.status === "validated"}
                  <button
                    type="button"
                    class="min-h-8 rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-2.5 text-[11px] font-medium text-emerald-700 transition-colors hover:bg-emerald-500/20 dark:text-emerald-300 disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={busyId === artifact.id}
                    onclick={() => void run("deliver", artifact.id)}
                  >
                    确认交付
                  </button>
                {/if}
                {#if onCopyToPrimary && primaryWorkRoot && artifactStorageMode !== "primary_work_root" && artifact.status === "delivered"}
                  <button
                    type="button"
                    class="min-h-8 rounded-lg border border-blue-500/40 bg-blue-500/10 px-2.5 text-[11px] font-medium text-blue-700 transition-colors hover:bg-blue-500/20 dark:text-blue-300 disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={busyId === artifact.id}
                    title="复制到所选本地工作目录的 output/，不会覆盖不同内容的文件"
                    onclick={() => void run("copy", artifact.id)}
                  >
                    复制到本地目录
                  </button>
                {/if}
                <button
                  type="button"
                  class="min-h-8 rounded-lg border border-border px-2.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={busyId === artifact.id || artifact.status !== "delivered"}
                  title={artifact.status === "delivered"
                    ? "导出已交付文件"
                    : "文件完成交付后才能导出"}
                  onclick={() => void run("export", artifact.id)}
                >
                  导出文件
                </button>
                <button
                  type="button"
                  class="min-h-8 rounded-lg border border-border px-2.5 text-[11px] font-medium text-muted-foreground transition-colors hover:border-red-400/40 hover:bg-red-400/10 hover:text-red-500 disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={readOnly || busyId === artifact.id}
                  onclick={() => promptDelete(artifact)}
                >
                  删除
                </button>
              </div>
            </article>
          {/each}
        </div>
      {/if}
    {/if}
    <WorkArtifactHub
      {artifacts}
      {workspaceRoot}
      {primaryWorkRoot}
      {artifactStorageMode}
      {compact}
      {readOnly}
      showHeader={false}
      emptyTitle="暂无已生成成果"
      emptyHint="任务生成并登记交付物后，会显示在这里。"
      onOpen={(artifact) => void run("open", artifact.id)}
      onExport={(artifact) => void run("export", artifact.id)}
      onValidate={(artifact) => void run("validate", artifact.id)}
      onDeliver={(artifact) => void run("deliver", artifact.id)}
      onCopyToPrimary={(artifact) => void run("copy", artifact.id)}
      onDelete={(artifact) => void promptDelete(artifact)}
      {onSaveOffice}
    />
  {/if}
</section>

<!-- Delete Artifact Confirmation Modal -->
{#if artifactToDelete}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
    role="dialog"
    aria-modal="true"
    aria-labelledby="delete-artifact-title"
    tabindex="-1"
    onkeydown={(event) => {
      if (event.key === "Escape" && !busyId) {
        event.preventDefault();
        event.stopPropagation();
        artifactToDelete = null;
      }
      trapFocus(event, deleteDialog ?? null);
    }}
  >
    <div
      bind:this={deleteDialog}
      class="w-full max-w-sm rounded-xl border border-red-500/30 bg-background p-5 shadow-xl space-y-3"
    >
      <div class="flex items-center gap-2 text-red-600 dark:text-red-400 font-semibold text-sm">
        <svg class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
          />
        </svg>
        <span id="delete-artifact-title">确定删除成果？</span>
      </div>
      <p class="text-xs text-muted-foreground leading-relaxed">
        确定要删除交付成果「<strong class="text-foreground">{artifactToDelete.title}</strong
        >」吗？此操作将从工作区移除该成果及对应文件，无法撤销。
      </p>
      <div class="flex justify-end gap-2 pt-2">
        <button
          type="button"
          class="rounded-lg border border-border px-3 py-1.5 text-xs font-medium hover:bg-accent"
          onclick={() => (artifactToDelete = null)}
        >
          取消
        </button>
        <button
          type="button"
          disabled={Boolean(busyId)}
          class="rounded-lg bg-red-600 px-3.5 py-1.5 text-xs font-semibold text-white hover:bg-red-700 disabled:opacity-50"
          onclick={confirmDeleteArtifact}
        >
          {busyId ? "删除中…" : "确认删除"}
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Native Multi-Format Preview Modal -->
<ArtifactPreviewModal
  artifact={previewArtifact}
  open={previewArtifact !== null}
  onClose={() => (previewArtifact = null)}
  onExport={previewArtifact?.status === "delivered" ? onExport : undefined}
  onOpenExternal={onOpen}
  onSaveOffice={readOnly ? undefined : onSaveOffice}
  {workspaceRoot}
/>
