<script lang="ts">
  import { exportWorkArtifact, listWorkArtifacts, openWorkFile } from "$lib/api/work";
  import WorkArtifactHub from "./WorkArtifactHub.svelte";
  import WorkLibraryCenter from "./WorkLibraryCenter.svelte";
  import type { WorkArtifactSummary, WorkWorkspaceSummary } from "$lib/types/work";

  interface Props {
    workspaces: WorkWorkspaceSummary[];
  }

  let { workspaces }: Props = $props();
  let activeTab = $state<"library" | "artifacts">("library");
  let artifacts = $state<WorkArtifactSummary[]>([]);
  let loadingArtifacts = $state(false);
  let artifactError = $state("");
  let loadedWorkspaceKey = $state("");

  let workspaceKey = $derived(workspaces.map((workspace) => workspace.id).join("|"));

  async function loadArtifacts(workspaceSnapshot: WorkWorkspaceSummary[]): Promise<void> {
    if (workspaceSnapshot.length === 0) {
      artifacts = [];
      return;
    }

    loadingArtifacts = true;
    artifactError = "";
    try {
      const results = await Promise.all(
        workspaceSnapshot.map(async (workspace) => {
          try {
            return await listWorkArtifacts(workspace.id, null);
          } catch {
            return [];
          }
        }),
      );
      artifacts = results
        .flat()
        .sort(
          (left, right) =>
            new Date(right.updatedAt || right.createdAt).getTime() -
            new Date(left.updatedAt || left.createdAt).getTime(),
        );
    } catch (cause) {
      artifacts = [];
      artifactError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loadingArtifacts = false;
    }
  }

  $effect(() => {
    const key = workspaceKey;
    if (key === loadedWorkspaceKey) return;
    loadedWorkspaceKey = key;
    void loadArtifacts(workspaces);
  });

  function workspaceRoot(artifact: WorkArtifactSummary): string | undefined {
    return workspaces.find((workspace) => workspace.id === artifact.workspaceId)?.root;
  }

  async function handleOpen(artifact: WorkArtifactSummary): Promise<void> {
    await openWorkFile(artifact.workspaceId, artifact.path);
  }

  async function handleExport(artifact: WorkArtifactSummary): Promise<void> {
    const { save } = await import("$lib/platform/dialog");
    const destination = await save({
      defaultPath: artifact.title || "work-artifact",
      filters: artifact.artifactType
        ? [{ name: artifact.artifactType.toUpperCase(), extensions: [artifact.artifactType] }]
        : undefined,
    });
    if (!destination) return;
    await exportWorkArtifact(
      artifact.workspaceId,
      artifact.id,
      destination,
      artifact.runId || null,
    );
  }
</script>

<div class="flex h-full min-h-0 w-full flex-col overflow-hidden">
  <div class="shrink-0 border-b border-border/60 px-4 py-3 sm:px-6">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div>
        <h1 class="text-lg font-bold text-foreground">资料与产物</h1>
        <p class="mt-1 text-xs text-muted-foreground">
          管理可复用资料，并集中查看各工作空间的交付成果。
        </p>
      </div>
      <div class="flex rounded-lg border border-border/70 bg-card/60 p-0.5" role="tablist">
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "library"}
          class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {activeTab ===
          'library'
            ? 'bg-primary/10 text-primary font-semibold'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => (activeTab = "library")}
        >
          资料
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "artifacts"}
          class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {activeTab ===
          'artifacts'
            ? 'bg-primary/10 text-primary font-semibold'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => (activeTab = "artifacts")}
        >
          产物{artifacts.length > 0 ? ` (${artifacts.length})` : ""}
        </button>
      </div>
    </div>
  </div>

  <div class="min-h-0 flex-1 overflow-hidden">
    {#if activeTab === "library"}
      <WorkLibraryCenter workspaceId="" {workspaces} />
    {:else}
      <div class="h-full overflow-y-auto p-4 sm:p-6">
        {#if artifactError}
          <div
            class="mb-3 rounded-lg border border-red-400/20 bg-red-400/5 px-3 py-2 text-xs text-red-500"
            role="alert"
          >
            {artifactError}
          </div>
        {/if}
        <WorkArtifactHub
          {artifacts}
          loading={loadingArtifacts}
          title="交付成果"
          readOnly
          getWorkspaceRoot={workspaceRoot}
          getSubtitle={(artifact) =>
            `工作区 · ${workspaces.find((workspace) => workspace.id === artifact.workspaceId)?.name ?? artifact.workspaceId}`}
          onOpen={(artifact) => void handleOpen(artifact)}
          onExport={(artifact) => void handleExport(artifact)}
          emptyTitle="还没有交付成果"
          emptyHint="各工作空间生成并登记的交付文件会集中显示在这里。"
        />
      </div>
    {/if}
  </div>
</div>
