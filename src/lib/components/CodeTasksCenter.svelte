<script lang="ts">
  import { onMount, getContext } from "svelte";
  import { page } from "$app/stores";
  import { workWorkspaceStore } from "$lib/stores/work-workspace-store.svelte";
  import WorkAutomationCenter from "$lib/components/work/WorkAutomationCenter.svelte";
  import { listRuns } from "$lib/api";
  import { isWorkRun } from "$lib/utils/agent-target";
  import { createWorkspaceFromFolder } from "$lib/api/work";
  import { cwdDisplayLabel } from "$lib/utils/format";
  import { normalizeCwd, type ProjectFolder } from "$lib/utils/sidebar-groups";
  import { loadRemovedCwds } from "$lib/utils/removed-cwds";
  import { getSavedPinnedCwds, getSavedProjectCwd } from "$lib/utils/project-cwd";
  import type { WorkWorkspaceSummary } from "$lib/types/work";

  const getSelectableFolders = getContext<(() => ProjectFolder[]) | undefined>(
    "codeSelectableFolders",
  );

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

      // 1. Gather all candidate project paths from every available source
      const allCandidateCwds = new Set<string>();

      // Prefer sidebar's exact selectableFolders if available from context
      const sidebarFolders = getSelectableFolders ? getSelectableFolders() : [];
      for (const folder of sidebarFolders) {
        const c = normalizeCwd(folder.cwd);
        if (c && !removedSet.has(c)) {
          allCandidateCwds.add(c);
        }
      }

      // Also include pinned cwds across both native & pi realms
      const pinned = [...getSavedPinnedCwds("native"), ...getSavedPinnedCwds("pi")];
      for (const p of pinned) {
        const c = normalizeCwd(p);
        if (c && !removedSet.has(c)) {
          allCandidateCwds.add(c);
        }
      }

      // Also include distinct project cwds from code runs (including archived runs)
      try {
        const runs = await listRuns();
        for (const run of runs) {
          if (!isWorkRun(run) && !run.remote_host_name && !run.code_standalone_task) {
            const c = normalizeCwd(run.cwd);
            if (c && !removedSet.has(c)) {
              allCandidateCwds.add(c);
            }
          }
        }
      } catch {
        // ignore
      }

      // Also include current persisted / active project in Code mode
      for (const realm of ["native", "pi"] as const) {
        const saved = normalizeCwd(getSavedProjectCwd(realm));
        if (saved && !removedSet.has(saved)) {
          allCandidateCwds.add(saved);
        }
      }
      const fromUrl = normalizeCwd($page.url.searchParams.get("folder") || "");
      if (fromUrl && !removedSet.has(fromUrl)) {
        allCandidateCwds.add(fromUrl);
      }

      const matched: WorkWorkspaceSummary[] = [];
      let neededRefresh = false;

      // 2. For each candidate project, match or create workspace
      for (const cwd of allCandidateCwds) {
        const label = cwdDisplayLabel(cwd);
        let ws = allWorkspaces.find(
          (w) =>
            normalizeCwd(w.primaryWorkRoot || (w as any).primary_work_root || w.root) === cwd ||
            w.name.toLowerCase() === label.toLowerCase(),
        );

        if (!ws) {
          try {
            ws = await createWorkspaceFromFolder(cwd, label);
            neededRefresh = true;
          } catch {
            // fallback: create synthetic summary so project is never missing in UI
            ws = {
              id: cwd,
              name: label,
              root: cwd,
              inputDir: `${cwd}/input`,
              scratchDir: `${cwd}/scratch`,
              outputDir: `${cwd}/output`,
              contextDir: `${cwd}/context`,
              createdAt: new Date().toISOString(),
              updatedAt: new Date().toISOString(),
              artifactCount: 0,
              archived: false,
              accessRoots: [],
              defaultPolicy: {
                executionMode: "auto",
                maxAutomatedSteps: 50,
                allowExternalConnectors: false,
                standingRules: [],
              },
              primaryWorkRoot: cwd,
              rootKind: "local_folder",
            };
          }
        }

        if (
          ws &&
          !matched.some(
            (m) =>
              m.id === ws!.id ||
              m.name.toLowerCase() === ws!.name.toLowerCase() ||
              normalizeCwd(m.primaryWorkRoot || (m as any).primary_work_root || m.root) ===
                normalizeCwd(ws!.primaryWorkRoot || (ws as any).primary_work_root || ws!.root),
          )
        ) {
          matched.push(ws);
        }
      }

      // Also include any pre-existing local_folder workspaces not already present
      for (const ws of allWorkspaces) {
        const rootPath = normalizeCwd(ws.primaryWorkRoot || (ws as any).primary_work_root);
        if (
          rootPath &&
          !removedSet.has(rootPath) &&
          !matched.some(
            (m) =>
              m.id === ws.id ||
              m.name.toLowerCase() === ws.name.toLowerCase() ||
              normalizeCwd(m.primaryWorkRoot || (m as any).primary_work_root || m.root) ===
                rootPath,
          )
        ) {
          matched.push(ws);
        }
      }

      if (neededRefresh) {
        void workWorkspaceStore.fetchWorkspaces(true);
      }

      codeWorkspaces = matched;

      // Determine active project ID
      const currentActive =
        fromUrl ||
        normalizeCwd((typeof window !== "undefined" ? getSavedProjectCwd("native") : "") || "");

      if (currentActive) {
        const found = matched.find(
          (w) =>
            normalizeCwd(w.primaryWorkRoot || (w as any).primary_work_root || w.root) ===
              currentActive ||
            w.name.toLowerCase() === cwdDisplayLabel(currentActive).toLowerCase(),
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
