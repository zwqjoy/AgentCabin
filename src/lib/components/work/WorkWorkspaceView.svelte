<script lang="ts">
  import WorkAccessPanel from "./WorkAccessPanel.svelte";
  import WorkArtifactPanel from "./WorkArtifactPanel.svelte";
  import WorkInputPanel from "./WorkInputPanel.svelte";
  import WorkAutomationCenter from "./WorkAutomationCenter.svelte";
  import { inboxStore } from "$lib/stores/inbox-store.svelte";
  import type {
    WorkAccessRoot,
    WorkArtifactSummary,
    WorkArtifactStorageMode,
    WorkFileSummary,
    WorkWorkspaceSummary,
  } from "$lib/types/work";

  interface Props {
    workspace: WorkWorkspaceSummary;
    inputFiles: WorkFileSummary[];
    accessRoots: WorkAccessRoot[];
    artifacts: WorkArtifactSummary[];
    accessBusyPath: string;
    onImportInputFile: (sourcePath: string) => Promise<void>;
    onOpenWorkspaceFile: (relativePath: string) => Promise<void>;
    onRemoveInputFile: (relativePath: string) => Promise<void>;
    onAddAccessRoot: () => void;
    onToggleAccessRoot: (root: WorkAccessRoot) => void;
    onRemoveAccessRoot: (root: WorkAccessRoot) => void;
    onExportArtifact: (id: string) => Promise<void>;
    onOpenArtifact: (id: string) => Promise<void>;
    onDeleteArtifact: (id: string) => Promise<void>;
    onCopyArtifactToPrimary?: (id: string) => Promise<string>;
    onValidateArtifact?: (id: string) => Promise<void>;
    onDeliverArtifact?: (id: string) => Promise<void>;
    onOpenArtifactDirectory?: () => Promise<void>;
    onOpenPrimaryFolder?: (path: string) => void;
    onRelinkPrimaryFolder?: () => void;
    onSetArtifactStorageMode?: (mode: WorkArtifactStorageMode) => Promise<void>;
    onSaveOfficeArtifact?: (id: string, contentBase64: string) => Promise<void>;
    onNewConversation: () => void;
    onRenameWorkspace: (name: string) => Promise<void>;
  }

  let {
    workspace,
    inputFiles,
    accessRoots,
    artifacts,
    accessBusyPath,
    onImportInputFile,
    onOpenWorkspaceFile,
    onRemoveInputFile,
    onAddAccessRoot,
    onToggleAccessRoot,
    onRemoveAccessRoot,
    onExportArtifact,
    onOpenArtifact,
    onDeleteArtifact,
    onCopyArtifactToPrimary,
    onValidateArtifact,
    onDeliverArtifact,
    onOpenArtifactDirectory,
    onOpenPrimaryFolder,
    onRelinkPrimaryFolder,
    onSetArtifactStorageMode,
    onSaveOfficeArtifact,
    onNewConversation,
    onRenameWorkspace,
  }: Props = $props();

  let renaming = $state(false);
  let renameValue = $state("");
  let renameBusy = $state(false);
  let renameInput = $state<HTMLInputElement>();

  $effect(() => {
    if (!renaming) return;
    requestAnimationFrame(() => renameInput?.focus());
  });

  function startRename() {
    renameValue = workspace.name;
    renaming = true;
  }

  async function confirmRename() {
    const name = renameValue.trim();
    if (!name || name === workspace.name) {
      renaming = false;
      return;
    }
    renameBusy = true;
    try {
      await onRenameWorkspace(name);
      renaming = false;
    } catch {
      // keep editing on error
    } finally {
      renameBusy = false;
    }
  }

  // 概念归并：3 tab（任务 / 资料 / 成果）
  // - 任务 = 待确认(inbox) + 自动化任务
  // - 资料 = 输入文件 + 授权目录
  // - 成果 = 交付成果
  let activeTab = $state<"tasks" | "materials" | "artifacts">("tasks");
  let workspacePendingCount = $derived(
    inboxStore.pendingItems.filter((i) => i.workspaceId === workspace.id).length,
  );
</script>

<div class="flex flex-1 flex-col min-h-0 space-y-4">
  <!-- Top Workspace Header -->
  <div
    class="flex shrink-0 flex-wrap items-center justify-between gap-4 rounded-2xl border border-border/70 bg-card/70 px-5 py-4 shadow-sm"
  >
    <div class="min-w-0">
      <div class="flex items-center gap-2.5">
        {#if renaming}
          <input
            bind:this={renameInput}
            bind:value={renameValue}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void confirmRename();
              }
              if (e.key === "Escape") renaming = false;
            }}
            class="min-h-0 w-64 rounded-lg border border-border bg-background px-2.5 py-1 text-lg font-semibold text-foreground outline-none focus:border-primary focus:ring-2 focus:ring-primary/20"
            disabled={renameBusy}
            maxlength="80"
          />
          <button
            type="button"
            class="rounded-lg p-1.5 text-emerald-600 hover:bg-emerald-500/10 disabled:opacity-50"
            title="确认"
            disabled={renameBusy}
            onclick={() => void confirmRename()}
          >
            <svg
              viewBox="0 0 24 24"
              class="h-4 w-4"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"><path d="m5 12 4 4L19 6" /></svg
            >
          </button>
          <button
            type="button"
            class="rounded-lg p-1.5 text-muted-foreground hover:bg-accent disabled:opacity-50"
            title="取消"
            disabled={renameBusy}
            onclick={() => {
              renaming = false;
            }}
          >
            <svg
              viewBox="0 0 24 24"
              class="h-4 w-4"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"><path d="m6 6 12 12M18 6 6 18" /></svg
            >
          </button>
        {:else}
          <h2 class="truncate text-lg font-semibold text-foreground">
            {workspace.name}
          </h2>
          <button
            type="button"
            class="rounded p-1 text-muted-foreground/50 transition-colors hover:bg-accent hover:text-foreground"
            title="重命名工作区"
            onclick={startRename}
          >
            <svg
              viewBox="0 0 24 24"
              class="h-3.5 w-3.5"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              ><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" /><path
                d="M18.5 2.5a2.12 2.12 0 0 1 3 3L12 15l-4 1 1-4Z"
              /></svg
            >
          </button>
        {/if}
        {#if inboxStore.pendingItems.some((i) => i.workspaceId === workspace.id)}
          <span
            class="rounded-full bg-amber-500/15 px-2.5 py-0.5 text-xs font-semibold text-amber-600 dark:text-amber-400"
            >等待你确认</span
          >
        {:else}
          <span
            class="rounded-full bg-muted px-2.5 py-0.5 text-xs font-medium text-muted-foreground"
            >已就绪</span
          >
        {/if}
      </div>
      <div class="mt-1.5 flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
        <span class="font-medium text-foreground/80">{inputFiles.length} 个资料</span>
        <span>·</span>
        <span class="font-medium text-foreground/80">{artifacts.length} 个成果</span>
        {#if accessRoots.length > 0}
          <span>·</span>
          <span>{accessRoots.length} 个授权目录</span>
        {/if}
      </div>
      <p class="mt-2 text-xs leading-5 text-muted-foreground/80">
        任务是主要入口；资料、授权和成果会在这里集中管理。
      </p>
    </div>
    <div class="flex shrink-0 items-center gap-2.5">
      <button
        type="button"
        class="inline-flex min-h-9 items-center gap-1.5 rounded-xl bg-primary px-4 py-2 text-xs font-semibold text-primary-foreground shadow-sm transition-all hover:opacity-90 hover:shadow"
        onclick={onNewConversation}
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
        新对话
      </button>
    </div>
  </div>

  <!-- Workspace Configuration View Container -->
  <div
    class="flex flex-1 flex-col min-h-0 overflow-hidden rounded-2xl border border-border/70 bg-card/50 shadow-sm"
  >
    <!-- Tab navigation -->
    <div class="shrink-0 border-b border-border/60 bg-background/40 px-4 py-2">
      <div class="flex items-center justify-between gap-4">
        <div class="flex items-center space-x-1" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === "tasks"}
            class="flex items-center gap-2 rounded-xl px-3.5 py-1.5 text-xs font-medium transition-colors {activeTab ===
            'tasks'
              ? 'bg-card text-foreground shadow-sm font-semibold'
              : 'text-muted-foreground hover:bg-card/60 hover:text-foreground'}"
            onclick={() => (activeTab = "tasks")}
          >
            <span>任务</span>
            {#if workspacePendingCount > 0}
              <span
                class="rounded-full bg-amber-500/20 px-1.5 py-0.2 text-[10px] font-bold text-amber-500"
                >{workspacePendingCount}</span
              >
            {/if}
          </button>

          <button
            type="button"
            role="tab"
            aria-selected={activeTab === "materials"}
            class="flex items-center gap-2 rounded-xl px-3.5 py-1.5 text-xs font-medium transition-colors {activeTab ===
            'materials'
              ? 'bg-card text-foreground shadow-sm font-semibold'
              : 'text-muted-foreground hover:bg-card/60 hover:text-foreground'}"
            onclick={() => (activeTab = "materials")}
          >
            <span>资料</span>
            <span class="rounded-full bg-muted/80 px-1.5 py-0.2 text-[10px] text-muted-foreground"
              >{inputFiles.length + accessRoots.length}</span
            >
          </button>

          <button
            type="button"
            role="tab"
            aria-selected={activeTab === "artifacts"}
            class="flex items-center gap-2 rounded-xl px-3.5 py-1.5 text-xs font-medium transition-colors {activeTab ===
            'artifacts'
              ? 'bg-card text-foreground shadow-sm font-semibold'
              : 'text-muted-foreground hover:bg-card/60 hover:text-foreground'}"
            onclick={() => (activeTab = "artifacts")}
          >
            <span>成果</span>
            <span class="rounded-full bg-muted/80 px-1.5 py-0.2 text-[10px] text-muted-foreground"
              >{artifacts.length}</span
            >
          </button>
        </div>
      </div>
    </div>

    <!-- Active Panel content -->
    <div class="flex-1 min-h-0 overflow-y-auto p-4 sm:p-5">
      {#if activeTab === "tasks"}
        <div class="space-y-6">
          {#if workspacePendingCount > 0}
            <div
              class="flex items-center justify-between gap-3 rounded-xl border border-amber-500/30 bg-amber-500/5 px-4 py-3 text-xs"
              role="status"
            >
              <span class="text-amber-700 dark:text-amber-300">
                有 {workspacePendingCount} 项需要你处理。
              </span>
              <a
                href="/chat/work?panel=pending"
                class="shrink-0 font-semibold text-amber-700 underline underline-offset-2 dark:text-amber-300"
                >查看待处理 →</a
              >
            </div>
          {/if}
          <div class="border-t border-border/40 pt-6">
            <WorkAutomationCenter workspaceId={workspace.id} workspaces={[workspace]} />
          </div>
        </div>
      {:else if activeTab === "materials"}
        <div class="space-y-6">
          <WorkAccessPanel
            {workspace}
            roots={accessRoots}
            busyPath={accessBusyPath}
            onAdd={onAddAccessRoot}
            onToggle={onToggleAccessRoot}
            onRemove={onRemoveAccessRoot}
            onOpenFolder={onOpenPrimaryFolder}
            onRelinkFolder={onRelinkPrimaryFolder}
            {onSetArtifactStorageMode}
          />
          <div class="border-t border-border/40 pt-6">
            <WorkInputPanel
              files={inputFiles}
              onImport={onImportInputFile}
              onOpen={onOpenWorkspaceFile}
              onRemove={onRemoveInputFile}
            />
          </div>
        </div>
      {:else if activeTab === "artifacts"}
        <WorkArtifactPanel
          {artifacts}
          workspaceRoot={workspace.root}
          primaryWorkRoot={workspace.primaryWorkRoot}
          artifactStorageMode={workspace.artifactStorageMode}
          onExport={onExportArtifact}
          onOpen={onOpenArtifact}
          onDelete={onDeleteArtifact}
          onCopyToPrimary={onCopyArtifactToPrimary}
          onValidate={onValidateArtifact}
          onDeliver={onDeliverArtifact}
          onOpenDirectory={onOpenArtifactDirectory}
          onSaveOffice={onSaveOfficeArtifact}
        />
      {/if}
    </div>
  </div>
</div>
