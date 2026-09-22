<script lang="ts">
  import { readTextFile, readFileBase64 } from "$lib/api";
  import CodeEditor from "$lib/components/CodeEditor.svelte";
  import CsvDataViewer from "./viewers/CsvDataViewer.svelte";
  import MermaidViewer from "./viewers/MermaidViewer.svelte";
  import MarkdownDiffViewer from "./viewers/MarkdownDiffViewer.svelte";
  import PdfViewer from "./viewers/PdfViewer.svelte";
  import HtmlArtifactViewer from "./viewers/HtmlArtifactViewer.svelte";
  import OfficePreview from "./viewers/OfficePreview.svelte";
  import OfficeEditor from "./viewers/OfficeEditor.svelte";
  import { trapFocus } from "$lib/utils/focus-trap";
  import type { OfficeDocumentExtension } from "$lib/utils/office-edit";
  import type { WorkArtifactSummary } from "$lib/types/work";

  interface Props {
    artifact: WorkArtifactSummary | null;
    open: boolean;
    onClose: () => void;
    onExport?: (artifactId: string) => Promise<void>;
    onOpenExternal?: (artifactId: string) => Promise<void>;
    onSaveOffice?: (artifactId: string, contentBase64: string) => Promise<void>;
    onShowDetails?: (artifact: WorkArtifactSummary) => void;
    /** Workspace root directory, used to resolve relative artifact paths */
    workspaceRoot?: string;
  }

  let {
    artifact,
    open,
    onClose,
    onExport,
    onOpenExternal,
    onSaveOffice,
    onShowDetails,
    workspaceRoot = "",
  }: Props = $props();

  let fileContent = $state("");
  let imageDataUrl = $state("");
  let videoDataUrl = $state("");
  let pdfBase64 = $state("");
  let officeBase64 = $state("");
  let officeEditing = $state(false);
  let loading = $state(false);
  let error = $state("");
  let isFullScreen = $state(false);
  let dialogEl = $state<HTMLDivElement>();

  let fileExt = $derived.by(() => {
    if (!artifact?.path) return "";
    const parts = artifact.path.split(".");
    return parts.length > 1 ? parts[parts.length - 1].toLowerCase() : "";
  });

  let previewKind = $derived.by(
    ():
      | "csv"
      | "mermaid"
      | "markdown"
      | "image"
      | "video"
      | "code"
      | "pdf"
      | "html"
      | "office"
      | "binary" => {
      const ext = fileExt;
      if (["mp4", "webm", "mov", "mkv", "ogg"].includes(ext)) return "video";
      if (["html", "htm"].includes(ext)) return "html";
      if (["csv", "tsv"].includes(ext)) return "csv";
      if (["mermaid", "mmd"].includes(ext)) return "mermaid";
      if (["md", "markdown", "diff", "patch"].includes(ext)) return "markdown";
      if (["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"].includes(ext)) return "image";
      // PDF — rendered inline via PDF.js
      if (ext === "pdf") return "pdf";
      // Open XML Office formats have a safe read-only content preview.
      if (["docx", "xlsx", "pptx"].includes(ext)) return "office";
      // Binary / Office formats that cannot be read as text
      if (
        [
          "xls",
          "ppt",
          "doc",
          "zip",
          "tar",
          "gz",
          "7z",
          "rar",
          "db",
          "sqlite",
          "bin",
          "exe",
          "dmg",
          "pkg",
        ].includes(ext)
      )
        return "binary";
      return "code";
    },
  );

  $effect(() => {
    if (open && artifact?.path) {
      void loadFile(artifact.path);
      officeEditing = false;
      dialogEl?.focus();
    } else {
      fileContent = "";
      imageDataUrl = "";
      videoDataUrl = "";
      pdfBase64 = "";
      officeBase64 = "";
      officeEditing = false;
      error = "";
    }
  });

  /** Resolve a possibly-relative artifact path to an absolute path */
  function resolvePath(p: string): string {
    if (!p) return p;
    // Already absolute
    if (p.startsWith("/") || /^[A-Za-z]:[/\\]/.test(p)) return p;
    // Relative: join with workspaceRoot
    if (workspaceRoot) {
      const root = workspaceRoot.endsWith("/") ? workspaceRoot.slice(0, -1) : workspaceRoot;
      return `${root}/${p}`;
    }
    return p;
  }

  async function loadFile(path: string) {
    // Binary files can't be read as text — skip loading, show placeholder
    if (previewKind === "binary") {
      loading = false;
      return;
    }
    loading = true;
    error = "";
    fileContent = "";
    imageDataUrl = "";
    pdfBase64 = "";
    officeBase64 = "";
    officeEditing = false;
    const absPath = resolvePath(path);
    try {
      if (previewKind === "image") {
        const [base64] = await readFileBase64(absPath, workspaceRoot || "");
        const mime =
          fileExt === "svg" ? "image/svg+xml" : `image/${fileExt === "jpg" ? "jpeg" : fileExt}`;
        imageDataUrl = `data:${mime};base64,${base64}`;
      } else if (previewKind === "video") {
        const [base64] = await readFileBase64(absPath, workspaceRoot || "");
        const mime = fileExt === "mp4" ? "video/mp4" : "video/webm";
        videoDataUrl = `data:${mime};base64,${base64}`;
      } else if (previewKind === "pdf") {
        const [base64] = await readFileBase64(absPath, workspaceRoot || "");
        pdfBase64 = base64;
      } else if (previewKind === "office") {
        const [base64] = await readFileBase64(absPath, workspaceRoot || "");
        officeBase64 = base64;
      } else {
        const text = await readTextFile(absPath, workspaceRoot || undefined);
        fileContent = text;
      }
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
    trapFocus(e, dialogEl ?? null);
  }

  function close(): void {
    officeEditing = false;
    onClose();
  }

  async function saveOffice(contentBase64: string): Promise<void> {
    if (!artifact || !onSaveOffice) return;
    await onSaveOffice(artifact.id, contentBase64);
    officeBase64 = contentBase64;
    officeEditing = false;
  }

  function formatSize(size: number): string {
    if (size < 1024) return `${size} B`;
    if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
    return `${(size / 1024 / 1024).toFixed(1)} MB`;
  }
</script>

{#if open && artifact}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center p-3 sm:p-6"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    bind:this={dialogEl}
    onkeydown={handleKeydown}
  >
    <!-- Backdrop -->
    <div
      class="fixed inset-0 bg-black/70 backdrop-blur-sm transition-opacity"
      onclick={close}
      role="presentation"
    ></div>

    <!-- Modal Dialog -->
    <div
      class="relative z-50 flex flex-col overflow-hidden rounded-2xl border border-border/80 bg-background shadow-2xl transition-all duration-200 {isFullScreen
        ? 'h-[96vh] w-[98vw] max-w-none'
        : 'h-[85vh] w-full max-w-5xl'}"
    >
      <!-- Header -->
      <div
        class="flex items-center justify-between gap-3 border-b border-border bg-card/80 px-4 py-3 sm:px-6"
      >
        <div class="flex min-w-0 items-center gap-3">
          <div
            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-primary"
          >
            {#if previewKind === "csv"}
              <span class="text-base">📊</span>
            {:else if previewKind === "mermaid"}
              <span class="text-base">📐</span>
            {:else if previewKind === "image"}
              <span class="text-base">🖼️</span>
            {:else if previewKind === "video"}
              <span class="text-base">🎬</span>
            {:else if previewKind === "markdown"}
              <span class="text-base">📝</span>
            {:else if previewKind === "html"}
              <span class="text-base">🌐</span>
            {:else if previewKind === "office"}
              {#if fileExt === "xlsx"}
                <span class="text-base">📊</span>
              {:else if fileExt === "pptx"}
                <span class="text-base">📽️</span>
              {:else}
                <span class="text-base">📝</span>
              {/if}
            {:else}
              <span class="text-base">📄</span>
            {/if}
          </div>

          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <h2 class="truncate text-sm font-semibold text-foreground">
                {artifact.title}
              </h2>
              <span
                class="rounded-full bg-muted px-2 py-0.5 text-[10px] font-mono text-muted-foreground uppercase"
              >
                {fileExt || artifact.artifactType}
              </span>
            </div>
            <p class="truncate font-mono text-[11px] text-muted-foreground" title={artifact.path}>
              {artifact.path} · {formatSize(artifact.size)}
            </p>
          </div>
        </div>

        <!-- Actions -->
        <div class="flex items-center gap-1.5 sm:gap-2">
          {#if previewKind === "office" && onSaveOffice && officeBase64 && !officeEditing}
            <button
              type="button"
              class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-primary/40 bg-primary/10 px-2.5 text-xs font-medium text-primary transition-colors hover:bg-primary/20"
              onclick={() => (officeEditing = true)}
              title="编辑 Office 文件"
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
                <path d="M12 20h9" />
                <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L8 18l-4 1 1-4Z" />
              </svg>
              <span class="hidden sm:inline">编辑</span>
            </button>
          {/if}

          {#if onOpenExternal}
            <button
              type="button"
              class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-2.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              onclick={() => void onOpenExternal?.(artifact.id)}
              title="在系统默认应用中打开"
            >
              <svg
                viewBox="0 0 24 24"
                class="h-3.5 w-3.5"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path>
                <polyline points="15 3 21 3 21 9"></polyline>
                <line x1="10" y1="14" x2="21" y2="3"></line>
              </svg>
              <span class="hidden sm:inline">系统应用打开</span>
            </button>
          {/if}

          {#if onExport && artifact?.status === "delivered"}
            <button
              type="button"
              class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-primary/40 bg-primary/10 px-2.5 text-xs font-medium text-primary transition-colors hover:bg-primary/20"
              onclick={() => void onExport?.(artifact.id)}
              title="导出到其他目录"
            >
              <svg
                viewBox="0 0 24 24"
                class="h-3.5 w-3.5"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
                <polyline points="7 10 12 15 17 10"></polyline>
                <line x1="12" y1="15" x2="12" y2="3"></line>
              </svg>
              <span class="hidden sm:inline">导出文件</span>
            </button>
          {/if}

          {#if onShowDetails && artifact}
            <button
              type="button"
              class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-2.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              onclick={() => onShowDetails?.(artifact)}
              title="查看产物溯源与校验详情"
            >
              <svg
                viewBox="0 0 24 24"
                class="h-3.5 w-3.5"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <circle cx="12" cy="12" r="10" />
                <line x1="12" y1="16" x2="12" y2="12" />
                <line x1="12" y1="8" x2="12.01" y2="8" />
              </svg>
              <span class="hidden sm:inline">溯源详情</span>
            </button>
          {/if}

          <button
            type="button"
            class="inline-flex h-8 w-8 items-center justify-center rounded-lg border border-border text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            onclick={() => (isFullScreen = !isFullScreen)}
            title={isFullScreen ? "恢复窗口" : "最大化"}
          >
            {#if isFullScreen}
              <svg
                viewBox="0 0 24 24"
                class="h-3.5 w-3.5"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path
                  d="M8 3v3a2 2 0 0 1-2 2H3m18 0h-3a2 2 0 0 1-2-2V3m0 18v-3a2 2 0 0 1 2-2h3M3 16h3a2 2 0 0 1 2 2v3"
                ></path>
              </svg>
            {:else}
              <svg
                viewBox="0 0 24 24"
                class="h-3.5 w-3.5"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"></path>
              </svg>
            {/if}
          </button>

          <button
            type="button"
            class="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            onclick={close}
            aria-label="关闭"
          >
            ✕
          </button>
        </div>
      </div>

      <!-- Body -->
      <div class="relative flex-1 overflow-hidden bg-background">
        {#if loading}
          <div class="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground">
            <div
              class="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent"
            ></div>
            <span class="text-xs">正在加载成果内容...</span>
          </div>
        {:else if error}
          <div
            class="flex h-full flex-col items-center justify-center gap-2 p-6 text-center text-xs text-red-500"
          >
            <div class="font-semibold">文件读取失败</div>
            <p class="max-w-md font-mono text-[11px] opacity-80">{error}</p>
          </div>
        {:else if previewKind === "csv"}
          <CsvDataViewer content={fileContent} />
        {:else if previewKind === "mermaid"}
          <MermaidViewer content={fileContent} fileName={artifact.path} />
        {:else if previewKind === "markdown"}
          <MarkdownDiffViewer
            content={fileContent}
            fileName={artifact.path}
            isDiff={["diff", "patch"].includes(fileExt)}
          />
        {:else if previewKind === "image"}
          <div
            class="flex h-full items-center justify-center overflow-auto p-6 bg-[radial-gradient(#e5e7eb_1px,transparent_1px)] dark:bg-[radial-gradient(#1f2937_1px,transparent_1px)] [background-size:16px_16px]"
          >
            <img
              src={imageDataUrl}
              alt={artifact.title}
              class="max-h-full max-w-full rounded-lg object-contain shadow-md"
            />
          </div>
        {:else if previewKind === "video"}
          <div
            class="flex h-full flex-col items-center justify-center overflow-auto p-6 bg-zinc-950/80"
          >
            {#if videoDataUrl}
              <!-- svelte-ignore a11y_media_has_caption -->
              <video
                src={videoDataUrl}
                controls
                autoplay
                class="max-h-[72vh] max-w-full rounded-lg shadow-2xl border border-zinc-800"
              >
              </video>
            {:else}
              <div class="text-xs text-muted-foreground">视频加载中...</div>
            {/if}
          </div>
        {:else if previewKind === "pdf"}
          <PdfViewer base64={pdfBase64} />
        {:else if previewKind === "html"}
          <HtmlArtifactViewer content={fileContent} fileName={artifact.path} />
        {:else if previewKind === "office"}
          {#if officeEditing}
            <OfficeEditor
              base64={officeBase64}
              fileName={artifact.path}
              fileExt={fileExt as OfficeDocumentExtension}
              onSave={saveOffice}
              onCancel={() => (officeEditing = false)}
            />
          {:else}
            <OfficePreview base64={officeBase64} fileName={artifact.path} {fileExt} />
          {/if}
        {:else if previewKind === "binary"}
          <!-- Binary / Office file: no text preview available -->
          <div class="flex h-full flex-col items-center justify-center gap-5 p-8 text-center">
            <div
              class="flex h-16 w-16 items-center justify-center rounded-2xl border border-border/60 bg-muted/60 text-3xl"
            >
              {#if ["xlsx", "xls"].includes(fileExt)}
                📊
              {:else if ["pptx", "ppt"].includes(fileExt)}
                📽️
              {:else if ["docx", "doc"].includes(fileExt)}
                📝
              {:else if ["zip", "tar", "gz", "7z", "rar"].includes(fileExt)}
                🗜️
              {:else}
                📦
              {/if}
            </div>
            <div>
              <p class="text-sm font-medium text-foreground">
                {fileExt.toUpperCase()} 文件无法内嵌预览
              </p>
              <p class="mt-1.5 max-w-xs text-xs text-muted-foreground">
                该格式为二进制文件，请使用系统应用打开或导出后查看
              </p>
            </div>
            <div class="flex items-center gap-2.5">
              {#if onOpenExternal}
                <button
                  type="button"
                  class="inline-flex h-9 items-center gap-2 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
                  onclick={() => void onOpenExternal?.(artifact.id)}
                >
                  <svg
                    class="h-4 w-4"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                  >
                    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                    <polyline points="15 3 21 3 21 9" />
                    <line x1="10" y1="14" x2="21" y2="3" />
                  </svg>
                  系统应用打开
                </button>
              {/if}
              {#if onExport}
                <button
                  type="button"
                  class="inline-flex h-9 items-center gap-2 rounded-lg border border-border px-4 text-sm font-medium text-foreground transition-colors hover:bg-accent"
                  onclick={() => void onExport?.(artifact.id)}
                >
                  <svg
                    class="h-4 w-4"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                  >
                    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                    <polyline points="7 10 12 15 17 10" />
                    <line x1="12" y1="15" x2="12" y2="3" />
                  </svg>
                  导出文件
                </button>
              {/if}
            </div>
          </div>
        {:else}
          <div class="h-full">
            <CodeEditor
              content={fileContent}
              filePath={artifact.path}
              readonly={true}
              class="h-full text-xs font-mono"
            />
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}
