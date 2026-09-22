<script lang="ts">
  import type { WorkFileSummary } from "$lib/types/work";

  interface Props {
    files: WorkFileSummary[];
    onImport: (sourcePath: string) => Promise<void>;
    onOpen: (path: string) => Promise<void>;
    onRemove: (path: string) => Promise<void>;
  }

  let { files, onImport, onOpen, onRemove }: Props = $props();
  let importing = $state(false);
  let openingPath = $state("");
  let removingPath = $state("");
  let confirmingPath = $state("");
  let error = $state("");

  function displayName(file: WorkFileSummary): string {
    const name = file.name?.trim();
    if (name) return name;
    const parts = file.path.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] || "未命名文件";
  }

  function extensionLabel(file: WorkFileSummary): string {
    const name = displayName(file);
    return name.split(".").pop()?.slice(0, 4).toUpperCase() || "FILE";
  }

  function formatSize(size: number): string {
    if (size < 1024) return `${size} B`;
    if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
    if (size < 1024 * 1024 * 1024) return `${(size / 1024 / 1024).toFixed(1)} MB`;
    return `${(size / 1024 / 1024 / 1024).toFixed(1)} GB`;
  }

  async function chooseFiles() {
    if (importing) return;
    importing = true;
    error = "";
    try {
      const { open: openDialog } = await import("$lib/platform/dialog");
      const selected = await openDialog({
        multiple: true,
        directory: false,
        title: "导入输入材料",
      });
      const paths = !selected ? [] : Array.isArray(selected) ? selected : [selected];
      for (const sourcePath of paths) {
        await onImport(sourcePath);
      }
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      importing = false;
    }
  }

  async function openFile(path: string) {
    if (openingPath || removingPath) return;
    openingPath = path;
    error = "";
    try {
      await onOpen(path);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      openingPath = "";
    }
  }

  async function removeFile(path: string) {
    if (removingPath) return;
    removingPath = path;
    confirmingPath = "";
    error = "";
    try {
      await onRemove(path);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      removingPath = "";
    }
  }
</script>

<section class="mt-5 rounded-2xl border border-border/70 bg-card/70 p-4 shadow-sm sm:p-5">
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div>
      <div class="flex items-center gap-2">
        <span class="h-2 w-2 rounded-full bg-blue-500"></span>
        <h2 class="text-sm font-semibold text-foreground">输入材料</h2>
        <span
          class="rounded-full bg-blue-500/10 px-2 py-0.5 text-[11px] text-blue-600 dark:text-blue-300"
          >{files.length}</span
        >
      </div>
      <p class="mt-1 text-xs leading-5 text-muted-foreground">
        文件会在本机复制到当前工作空间的
        input/（不是上传到云端），任务只能读取，原始文件不会被改写。PDF、Office
        和其他文件可用系统默认应用预览。
      </p>
    </div>
    <button
      type="button"
      class="inline-flex min-h-10 items-center gap-2 rounded-lg bg-primary px-3.5 py-2 text-xs font-semibold text-primary-foreground transition-all hover:-translate-y-0.5 hover:shadow-md disabled:cursor-not-allowed disabled:opacity-60"
      disabled={importing}
      onclick={() => void chooseFiles()}
    >
      {#if importing}
        <span
          class="h-3.5 w-3.5 animate-spin rounded-full border-2 border-primary-foreground/30 border-t-primary-foreground"
        ></span>
        导入中…
      {:else}
        <svg
          viewBox="0 0 24 24"
          class="h-4 w-4"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
        >
          <path d="M12 5v14M5 12h14" />
        </svg>
        添加文件
      {/if}
    </button>
  </div>

  {#if error}
    <div
      class="mt-3 rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
      role="alert"
    >
      {error}
    </div>
  {/if}

  {#if files.length === 0}
    <button
      type="button"
      class="mt-4 flex min-h-24 w-full flex-col items-center justify-center rounded-xl border border-dashed border-border/70 px-4 py-5 text-center transition-colors hover:border-primary/40 hover:bg-primary/5"
      onclick={() => void chooseFiles()}
    >
      <span class="text-xs font-medium text-foreground">把 PDF、表格或文档放进来</span>
      <span class="mt-1 text-[11px] text-muted-foreground">点击选择一个或多个本地文件</span>
    </button>
  {:else}
    <div class="mt-4 grid grid-cols-1 gap-2">
      {#each files as file (file.path)}
        <div
          class="flex min-w-0 flex-wrap items-center gap-3 rounded-xl border border-border/60 bg-background/50 px-3.5 py-3"
        >
          <div class="flex min-w-0 flex-1 items-center gap-3">
            <div
              class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-blue-500/10 text-[11px] font-semibold text-blue-600 dark:text-blue-300"
            >
              {extensionLabel(file)}
            </div>
            <div class="min-w-0 flex-1">
              <div class="truncate text-xs font-medium text-foreground" title={displayName(file)}>
                {displayName(file)}
              </div>
              <div
                class="mt-1 truncate font-mono text-[10px] text-muted-foreground"
                title={file.path}
              >
                {file.path}
              </div>
            </div>
          </div>
          <span class="shrink-0 text-[10px] text-muted-foreground">{formatSize(file.size)}</span>
          <div class="flex shrink-0 items-center gap-1.5">
            <button
              type="button"
              class="min-h-8 shrink-0 rounded-lg border border-border px-2.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
              disabled={openingPath === file.path || Boolean(removingPath)}
              onclick={() => void openFile(file.path)}
            >
              {openingPath === file.path ? "打开中…" : "预览"}
            </button>
            {#if confirmingPath === file.path}
              <button
                type="button"
                class="min-h-8 shrink-0 rounded-lg border border-red-400/40 px-2.5 text-[11px] font-medium text-red-500 transition-colors hover:bg-red-400/10 disabled:cursor-not-allowed disabled:opacity-50"
                disabled={removingPath === file.path}
                onclick={() => void removeFile(file.path)}
              >
                {removingPath === file.path ? "移除中…" : "确认移除"}
              </button>
              <button
                type="button"
                class="min-h-8 shrink-0 rounded-lg border border-border px-2.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
                disabled={Boolean(removingPath)}
                onclick={() => (confirmingPath = "")}
              >
                取消
              </button>
            {:else}
              <button
                type="button"
                class="min-h-8 shrink-0 rounded-lg border border-border px-2.5 text-[11px] font-medium text-muted-foreground transition-colors hover:border-red-400/40 hover:bg-red-400/5 hover:text-red-500 disabled:cursor-not-allowed disabled:opacity-50"
                disabled={Boolean(removingPath)}
                aria-label={`移除 ${displayName(file)}`}
                onclick={() => (confirmingPath = file.path)}
              >
                移除
              </button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</section>
