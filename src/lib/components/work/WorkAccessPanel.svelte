<script lang="ts">
  import type {
    WorkAccessRoot,
    WorkArtifactStorageMode,
    WorkWorkspaceSummary,
  } from "$lib/types/work";

  interface Props {
    workspace: WorkWorkspaceSummary;
    roots: WorkAccessRoot[];
    busyPath?: string;
    onAdd: () => void;
    onToggle: (root: WorkAccessRoot) => void;
    onRemove: (root: WorkAccessRoot) => void;
    onOpenFolder?: (path: string) => void;
    onRelinkFolder?: () => void;
    onSetArtifactStorageMode?: (mode: WorkArtifactStorageMode) => Promise<void>;
  }

  let {
    workspace,
    roots,
    busyPath = "",
    onAdd,
    onToggle,
    onRemove,
    onOpenFolder,
    onRelinkFolder,
    onSetArtifactStorageMode,
  }: Props = $props();

  function folderName(path: string): string {
    const normalized = path.replace(/[\\/]+$/, "");
    return normalized.split(/[\\/]/).pop() || path;
  }

  function shortPath(path: string): string {
    const home = path.match(/^\/Users\/[^/]+/);
    if (home) return `~${path.slice(home[0].length)}`;
    return path;
  }

  let isLocalFolder = $derived(
    workspace.rootKind === "local_folder" && Boolean(workspace.primaryWorkRoot),
  );
  let workingFolderPath = $derived(isLocalFolder ? workspace.primaryWorkRoot! : workspace.root);
  let isWorkingFolderInvalid = $derived(workspace.workingRootValid === false);
  let artifactStorageMode = $derived(workspace.artifactStorageMode ?? "managed");
  let storageBusy = $state(false);
  let storageError = $state("");

  async function setArtifactStorageMode(mode: WorkArtifactStorageMode): Promise<void> {
    if (
      !onSetArtifactStorageMode ||
      storageBusy ||
      artifactStorageMode === mode ||
      !isLocalFolder ||
      isWorkingFolderInvalid
    ) {
      return;
    }
    storageBusy = true;
    storageError = "";
    try {
      await onSetArtifactStorageMode(mode);
    } catch (cause) {
      storageError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      storageBusy = false;
    }
  }
</script>

<div class="space-y-6">
  <!-- Section 1: 工作目录 (Working Folder) -->
  <section class="space-y-3 p-3 sm:p-4 rounded-2xl border border-border/60 bg-card/40">
    <div class="flex items-start justify-between gap-3">
      <div class="flex items-center gap-2">
        <span
          class="flex h-8 w-8 items-center justify-center rounded-lg bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
          aria-hidden="true"
        >
          <svg
            viewBox="0 0 24 24"
            class="h-4 w-4"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path
              d="M3 7.5A2.5 2.5 0 0 1 5.5 5h4l2 2h7A2.5 2.5 0 0 1 21 9.5v7A2.5 2.5 0 0 1 18.5 19h-13A2.5 2.5 0 0 1 3 16.5Z"
            />
          </svg>
        </span>
        <div>
          <h3 class="text-sm font-semibold text-foreground">工作目录</h3>
          <p class="mt-0.5 text-[11px] text-muted-foreground">
            {isLocalFolder ? "任务的工作根目录，默认拥有读写权限" : "AgentCabin 内置独立工作空间"}
          </p>
        </div>
      </div>

      {#if onRelinkFolder && isLocalFolder}
        <button
          type="button"
          class="inline-flex min-h-8 shrink-0 items-center gap-1.5 rounded-lg border border-border px-2.5 py-1 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          onclick={onRelinkFolder}
          title="更换工作目录"
        >
          <svg
            viewBox="0 0 24 24"
            class="h-3.5 w-3.5"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
            <path d="M3 3v5h5" />
            <path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" />
            <path d="M16 21h5v-5" />
          </svg>
          更换目录
        </button>
      {/if}
    </div>

    {#if isWorkingFolderInvalid}
      <div
        class="rounded-xl border border-red-500/30 bg-red-500/10 p-3 text-red-600 dark:text-red-400"
      >
        <div class="flex items-start gap-2">
          <svg
            viewBox="0 0 24 24"
            class="h-4 w-4 shrink-0 mt-0.5"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
          <div class="text-xs">
            <p class="font-semibold">本地工作目录不可用</p>
            <p class="mt-0.5 text-[11px] opacity-90">
              目标路径 <code class="rounded bg-background/50 px-1 py-0.5">{workingFolderPath}</code> 已不存在或已被移动。会话已安全阻断。
            </p>
            {#if onRelinkFolder}
              <button
                type="button"
                class="mt-2 inline-flex items-center gap-1 rounded-lg bg-red-600 px-2.5 py-1 text-xs font-semibold text-white hover:bg-red-700"
                onclick={onRelinkFolder}
              >
                重新选择关联目录
              </button>
            {/if}
          </div>
        </div>
      </div>
    {:else}
      <div class="rounded-xl border border-border/60 bg-card/60 p-3">
        <div class="flex items-start gap-2.5">
          <span
            class="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-emerald-500/10 text-emerald-600 dark:text-emerald-300"
          >
            <svg
              viewBox="0 0 24 24"
              class="h-4 w-4"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path
                d="M3 7.5A2.5 2.5 0 0 1 5.5 5h4l2 2h7A2.5 2.5 0 0 1 21 9.5v7A2.5 2.5 0 0 1 18.5 19h-13A2.5 2.5 0 0 1 3 16.5Z"
              />
            </svg>
          </span>
          <div class="min-w-0 flex-1">
            <div class="flex items-center justify-between gap-2">
              <span class="truncate text-xs font-medium text-foreground">
                {isLocalFolder ? folderName(workingFolderPath) : workspace.name}
              </span>
              <span
                class="shrink-0 rounded-md bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-300"
                >默认读写</span
              >
            </div>
            <div
              class="mt-1 truncate font-mono text-[10px] text-muted-foreground/70"
              title={workingFolderPath}
            >
              {isLocalFolder ? shortPath(workingFolderPath) : "内置独立存储空间"}
            </div>
            <div class="mt-2 flex items-center gap-2">
              {#if onOpenFolder}
                <button
                  type="button"
                  class="inline-flex items-center gap-1 rounded-md border border-border/70 px-2 py-1 text-[10px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                  onclick={() => onOpenFolder?.(workingFolderPath)}
                >
                  <svg
                    viewBox="0 0 24 24"
                    class="h-3 w-3"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                  >
                    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                    <polyline points="15 3 21 3 21 9" />
                    <line x1="10" y1="14" x2="21" y2="3" />
                  </svg>
                  在 Finder 中打开
                </button>
              {/if}
            </div>
          </div>
        </div>
      </div>
    {/if}
  </section>

  {#if isLocalFolder}
    <section class="space-y-3 rounded-2xl border border-border/60 bg-card/40 p-3 sm:p-4">
      <div class="flex items-start gap-2.5">
        <span
          class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-blue-500/10 text-blue-600 dark:text-blue-300"
          aria-hidden="true"
        >
          <svg
            viewBox="0 0 24 24"
            class="h-4 w-4"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M4 7.5h16M7 4v3.5M17 4v3.5M5.5 10.5h13v8h-13z" />
            <path d="M8.5 14h7" />
          </svg>
        </span>
        <div class="min-w-0 flex-1">
          <h3 class="text-sm font-semibold text-foreground">成果保存位置</h3>
          <p class="mt-0.5 text-[11px] leading-5 text-muted-foreground">
            选择 AgentCabin 托管成果，或直接写入所选本地文件夹的 <code class="rounded bg-muted px-1"
              >output/</code
            >。
          </p>
        </div>
      </div>

      <div class="grid gap-2 sm:grid-cols-2" role="radiogroup" aria-label="成果保存位置">
        <button
          type="button"
          role="radio"
          aria-checked={artifactStorageMode === "managed"}
          disabled={storageBusy || !onSetArtifactStorageMode}
          class="rounded-xl border p-3 text-left transition-colors disabled:cursor-not-allowed disabled:opacity-60 {artifactStorageMode ===
          'managed'
            ? 'border-blue-500/50 bg-blue-500/10'
            : 'border-border/60 bg-card/60 hover:border-blue-500/30 hover:bg-accent/30'}"
          onclick={() => void setArtifactStorageMode("managed")}
        >
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-semibold text-foreground">AgentCabin 托管</span>
            {#if artifactStorageMode === "managed"}
              <span class="text-[10px] font-semibold text-blue-600 dark:text-blue-300">当前</span>
            {/if}
          </div>
          <p class="mt-1 text-[10px] leading-4 text-muted-foreground">
            运行更隔离；需要时在成果面板复制到本地目录。
          </p>
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={artifactStorageMode === "primary_work_root"}
          disabled={storageBusy || !onSetArtifactStorageMode}
          class="rounded-xl border p-3 text-left transition-colors disabled:cursor-not-allowed disabled:opacity-60 {artifactStorageMode ===
          'primary_work_root'
            ? 'border-amber-500/50 bg-amber-500/10'
            : 'border-border/60 bg-card/60 hover:border-amber-500/30 hover:bg-accent/30'}"
          onclick={() => void setArtifactStorageMode("primary_work_root")}
        >
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-semibold text-foreground">直接保存到本地目录</span>
            {#if artifactStorageMode === "primary_work_root"}
              <span class="text-[10px] font-semibold text-amber-600 dark:text-amber-300">当前</span>
            {/if}
          </div>
          <p class="mt-1 text-[10px] leading-4 text-muted-foreground">
            新成果直接出现在 <code class="rounded bg-muted px-1"
              >{shortPath(workingFolderPath)}/output/</code
            >。
          </p>
        </button>
      </div>

      {#if artifactStorageMode === "primary_work_root"}
        <p
          class="rounded-lg border border-amber-500/20 bg-amber-500/5 px-3 py-2 text-[11px] leading-5 text-amber-700 dark:text-amber-300"
        >
          仅 <code class="rounded bg-background/60 px-1">output/</code> 改为直写；<code
            class="rounded bg-background/60 px-1">input/</code
          >、<code class="rounded bg-background/60 px-1">scratch/</code>、<code
            class="rounded bg-background/60 px-1">context/</code
          > 仍由 AgentCabin 托管。
        </p>
      {/if}
      {#if storageError}
        <p
          class="rounded-lg border border-red-500/20 bg-red-500/5 px-3 py-2 text-[11px] text-red-600 dark:text-red-400"
          role="alert"
        >
          {storageError}
        </p>
      {/if}
    </section>
  {/if}

  <!-- Section 2: 其他可访问位置 (Additional Access Roots) -->
  <section class="space-y-3 p-3 sm:p-4 rounded-2xl border border-border/60 bg-card/40">
    <div class="flex items-start justify-between gap-3">
      <div class="flex items-center gap-2">
        <span
          class="flex h-8 w-8 items-center justify-center rounded-lg bg-sky-500/10 text-sky-500"
          aria-hidden="true"
        >
          <svg
            viewBox="0 0 24 24"
            class="h-4 w-4"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M3.5 12.5 12 4l8.5 8.5" /><path d="M5.5 10.5V20h13v-9.5" />
            <path d="M9 20v-5h6v5" />
          </svg>
        </span>
        <div>
          <h3 class="text-sm font-semibold text-foreground">其他可访问位置</h3>
          <p class="mt-0.5 text-[11px] text-muted-foreground">
            授权任务访问额外的外部目录（默认只读）
          </p>
        </div>
      </div>
      <button
        type="button"
        class="inline-flex min-h-8 shrink-0 items-center gap-1.5 rounded-lg border border-primary/25 bg-primary/5 px-2.5 py-1 text-xs font-medium text-primary transition-colors hover:bg-primary/10"
        onclick={onAdd}
        title="授予任务访问一个额外目录"
      >
        <svg
          viewBox="0 0 24 24"
          class="h-3.5 w-3.5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
        >
          <path d="M12 5v14M5 12h14" />
        </svg>
        添加目录
      </button>
    </div>

    <div class="space-y-2">
      {#each roots as root (root.path)}
        <div class="rounded-xl border border-border/60 bg-card/60 p-3">
          <div class="flex items-start gap-2.5">
            <span
              class="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-sky-500/10 text-sky-600 dark:text-sky-300"
            >
              <svg
                viewBox="0 0 24 24"
                class="h-4 w-4"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path
                  d="M3 7.5A2.5 2.5 0 0 1 5.5 5h4l2 2h7A2.5 2.5 0 0 1 21 9.5v7A2.5 2.5 0 0 1 18.5 19h-13A2.5 2.5 0 0 1 3 16.5Z"
                />
              </svg>
            </span>
            <div class="min-w-0 flex-1">
              <div class="flex items-start justify-between gap-2">
                <div class="min-w-0">
                  <div class="truncate text-xs font-medium text-foreground" title={root.path}>
                    {folderName(root.path)}
                  </div>
                  <div
                    class="mt-1 truncate font-mono text-[10px] text-muted-foreground/70"
                    title={root.path}
                  >
                    {shortPath(root.path)}
                  </div>
                </div>
                <button
                  type="button"
                  class="-mr-1 -mt-1 rounded-md p-1 text-muted-foreground/60 transition-colors hover:bg-red-500/10 hover:text-red-500 disabled:cursor-wait disabled:opacity-50"
                  aria-label={`移除 ${folderName(root.path)}`}
                  title="移除目录"
                  disabled={busyPath === root.path}
                  onclick={() => onRemove(root)}
                >
                  <svg
                    viewBox="0 0 24 24"
                    class="h-3.5 w-3.5"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                  >
                    <path d="m6 6 12 12M18 6 6 18" />
                  </svg>
                </button>
              </div>

              <div class="mt-2 flex items-center justify-between gap-2">
                <span
                  class="rounded-md {root.writable
                    ? 'bg-amber-500/10 text-amber-600 dark:text-amber-300'
                    : 'bg-muted text-muted-foreground'} px-2 py-1 text-[10px] font-medium"
                  >{root.writable ? "读写" : "只读"}</span
                >
                <button
                  type="button"
                  class="min-h-7 rounded-lg border border-border/70 px-2.5 py-0.5 text-[10px] font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:cursor-wait disabled:opacity-50"
                  disabled={busyPath === root.path}
                  aria-pressed={root.writable}
                  onclick={() => onToggle(root)}
                >
                  {busyPath === root.path ? "保存中…" : root.writable ? "改为只读" : "允许写入"}
                </button>
              </div>
            </div>
          </div>
        </div>
      {:else}
        <div class="rounded-xl border border-dashed border-border/70 px-3 py-4 text-center">
          <p class="text-xs font-medium text-foreground">暂无其他授权目录</p>
          <p class="mt-1 text-[11px] leading-4 text-muted-foreground">
            如需让任务读取或修改工作目录以外的外部文件，可点击上方「添加目录」。
          </p>
        </div>
      {/each}
    </div>
  </section>
</div>
