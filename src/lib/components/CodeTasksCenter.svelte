<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/stores";
  import { workWorkspaceStore } from "$lib/stores/work-workspace-store.svelte";
  import WorkAutomationCenter from "$lib/components/work/WorkAutomationCenter.svelte";
  import { listRuns } from "$lib/api";
  import { isWorkRun } from "$lib/utils/agent-target";
  import { createWorkspaceFromFolder } from "$lib/api/work";
  import { cwdDisplayLabel } from "$lib/utils/format";
  import { buildProjectFolders, normalizeCwd } from "$lib/utils/sidebar-groups";
  import { loadRemovedCwds } from "$lib/utils/removed-cwds";
  import { getSavedProjectCwd } from "$lib/utils/project-cwd";
  import type { WorkWorkspaceSummary } from "$lib/types/work";

  let codeWorkspaces = $state<WorkWorkspaceSummary[]>([]);
  let activeProjectId = $state<string>("");
  let initialized = $state(false);

  async function syncCodeWorkspaces() {
    const allWorkspaces = await workWorkspaceStore.fetchWorkspaces();
    try {
      const removedSet = new Set(
        (typeof localStorage !== "undefined" ? loadRemovedCwds() : []).map(normalizeCwd),
      );
      removedSet.delete("");

      const runs = await listRuns();
      const codeRuns = runs.filter(
        (run) => !isWorkRun(run) && !run.remote_host_name && !run.code_standalone_task,
      );
      const folders = buildProjectFolders(codeRuns, new Set<string>(), [], [...removedSet], false);

      // Active project folders from sidebar (excluding removed cwds)
      const activeProjectCwds = [
        ...new Set(
          folders
            .filter((folder) => !folder.isUncategorized)
            .map((folder) => normalizeCwd(folder.cwd))
            .filter((cwd) => Boolean(cwd && !removedSet.has(cwd))),
        ),
      ];

      // Current active/saved project in Code mode
      const currentPersisted = normalizeCwd(
        $page.url.searchParams.get("folder") ||
          (typeof window !== "undefined" ? getSavedProjectCwd("native") : "") ||
          "",
      );
      if (
        currentPersisted &&
        !removedSet.has(currentPersisted) &&
        !activeProjectCwds.includes(currentPersisted)
      ) {
        activeProjectCwds.unshift(currentPersisted);
      }

      const matched: WorkWorkspaceSummary[] = [];
      let neededRefresh = false;

      for (const cwd of activeProjectCwds) {
        let ws = allWorkspaces.find(
          (w) =>
            normalizeCwd(w.primaryWorkRoot || (w as any).primary_work_root || w.root) === cwd ||
            w.name === cwdDisplayLabel(cwd),
        );
        if (!ws) {
          try {
            ws = await createWorkspaceFromFolder(cwd, cwdDisplayLabel(cwd));
            neededRefresh = true;
          } catch {
            // ignore
          }
        }
        if (ws && !matched.some((m) => m.id === ws!.id)) {
          matched.push(ws);
        }
      }

      if (neededRefresh) {
        await workWorkspaceStore.fetchWorkspaces(true);
      }

      codeWorkspaces = matched;

      // Determine active project ID
      if (currentPersisted) {
        const found = matched.find(
          (w) =>
            normalizeCwd(w.primaryWorkRoot || (w as any).primary_work_root || w.root) ===
            currentPersisted,
        );
        if (found) activeProjectId = found.id;
      }
      if (!activeProjectId && matched.length > 0) {
        activeProjectId = matched[0].id;
      }
    } catch {
      // fallback
      codeWorkspaces = allWorkspaces;
    } finally {
      initialized = true;
    }
  }

  onMount(() => {
    void syncCodeWorkspaces();
  });
</script>

{#if initialized}
  <WorkAutomationCenter workspaceId="" workspaces={codeWorkspaces} mode="code" {activeProjectId} />
{/if}
