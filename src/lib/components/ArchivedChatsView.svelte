<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import * as api from "$lib/api";
  import type { ConversationGroup, ProjectFolder } from "$lib/utils/sidebar-groups";
  import { buildProjectFolders } from "$lib/utils/sidebar-groups";
  import { cwdDisplayLabel, relativeTime } from "$lib/utils/format";
  import { t } from "$lib/i18n/index.svelte";
  import { dbgWarn } from "$lib/utils/debug";
  import {
    applyRunMutation,
    dispatchRunMutation,
    RUNS_CHANGED_EVENT,
    type RunMutation,
  } from "$lib/utils/run-mutations";
  import Card from "./Card.svelte";
  import { deleteSnapshot } from "$lib/utils/snapshot-cache";
  import {
    getRunRoute,
    getAgentTarget,
    isWorkRun,
    isNativeTarget,
    isPiTarget,
  } from "$lib/utils/agent-target";
  import {
    listArchivedWorkspaces,
    listStandaloneWorkSessions,
    listWorkSessions,
    listWorkspaces,
  } from "$lib/api/work";
  import type { TaskRun } from "$lib/types";

  let {
    realm = "all",
  }: {
    realm?: "code" | "work" | "all" | "native" | "pi";
  } = $props();

  let runs = $state<TaskRun[]>([]);
  let loading = $state(true);
  let query = $state("");
  let projectFilter = $state("");
  let busyId = $state("");
  let error = $state("");

  async function loadArchivedRuns() {
    try {
      if (realm === "work") {
        const [activeWs, archivedWs, standalones] = await Promise.all([
          listWorkspaces().catch(() => []),
          listArchivedWorkspaces().catch(() => []),
          listStandaloneWorkSessions().catch(() => []),
        ]);
        const wsSessionsList = await Promise.all(
          [...activeWs, ...archivedWs].map(async (ws) => {
            try {
              const sessions = await listWorkSessions(ws.id);
              return sessions.map((s) => ({
                ...s,
                cwd: ws.name || "工作区任务",
                workspace_id: ws.id,
              }));
            } catch {
              return [];
            }
          }),
        );
        const mappedStandalones = standalones.map((s) => ({
          ...s,
          cwd:
            !s.workspace_id || s.cwd?.includes(".agentcabin/standalone_tasks")
              ? "独立任务"
              : s.cwd || "独立任务",
        }));
        runs = [...mappedStandalones, ...wsSessionsList.flat()];
      } else if (realm === "code" || realm === "native" || realm === "pi") {
        runs = await api.listRuns();
      } else {
        const [normalRuns, activeWs, archivedWs, standalones] = await Promise.all([
          api.listRuns().catch(() => []),
          listWorkspaces().catch(() => []),
          listArchivedWorkspaces().catch(() => []),
          listStandaloneWorkSessions().catch(() => []),
        ]);
        const wsSessionsList = await Promise.all(
          [...activeWs, ...archivedWs].map(async (ws) => {
            try {
              const sessions = await listWorkSessions(ws.id);
              return sessions.map((s) => ({
                ...s,
                cwd: ws.name || "工作区任务",
                workspace_id: ws.id,
              }));
            } catch {
              return [];
            }
          }),
        );
        const mappedStandalones = standalones.map((s) => ({
          ...s,
          cwd:
            !s.workspace_id || s.cwd?.includes(".agentcabin/standalone_tasks")
              ? "独立任务"
              : s.cwd || "独立任务",
        }));
        runs = [...normalRuns, ...mappedStandalones, ...wsSessionsList.flat()];
      }
      error = "";
    } catch (e) {
      error = String(e);
      dbgWarn("archived-chats", "load runs failed", e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    // Re-load when realm changes
    const _ = realm;
    void loadArchivedRuns();
  });

  onMount(() => {
    const onRunsChanged = (event: Event) => {
      const mutation = (event as CustomEvent<RunMutation>).detail;
      if (mutation?.kind === "update" || mutation?.kind === "delete") {
        runs = applyRunMutation(runs, mutation);
      } else {
        void loadArchivedRuns();
      }
    };
    window.addEventListener(RUNS_CHANGED_EVENT, onRunsChanged);
    return () => window.removeEventListener(RUNS_CHANGED_EVENT, onRunsChanged);
  });

  const scopedRuns = $derived.by(() => {
    if (realm === "work") {
      return runs.filter((run) => isWorkRun(run));
    }
    if (realm === "code" || realm === "native" || realm === "pi") {
      return runs.filter((run) => !isWorkRun(run));
    }
    return runs;
  });

  const archivedFolders = $derived.by(() => {
    const folders = buildProjectFolders(scopedRuns, new Set(), [], [], true);
    return folders
      .map((folder): ProjectFolder => {
        const conversations = folder.conversations.filter((conversation) => conversation.archived);
        return { ...folder, conversations, conversationCount: conversations.length };
      })
      .filter((folder) => folder.conversations.length > 0);
  });

  const projectOptions = $derived.by(() =>
    archivedFolders.map((folder) => ({
      cwd: folder.cwd,
      label: folder.isUncategorized ? t("sidebar_uncategorized") : cwdDisplayLabel(folder.cwd),
    })),
  );

  const filteredFolders = $derived.by(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    return archivedFolders
      .filter((folder) => !projectFilter || folder.cwd === projectFilter)
      .map((folder): ProjectFolder => {
        const conversations = folder.conversations.filter((conversation) => {
          if (!normalizedQuery) return true;
          const run = conversation.latestRun;
          return [
            conversation.title,
            run.prompt,
            run.last_message_preview,
            run.cwd,
            run.agent,
            run.model,
          ]
            .filter(Boolean)
            .join(" ")
            .toLocaleLowerCase()
            .includes(normalizedQuery);
        });
        return { ...folder, conversations, conversationCount: conversations.length };
      })
      .filter((folder) => folder.conversations.length > 0);
  });

  const archivedCount = $derived(
    archivedFolders.reduce((count, folder) => count + folder.conversations.length, 0),
  );
  const visibleCount = $derived(
    filteredFolders.reduce((count, folder) => count + folder.conversations.length, 0),
  );

  function conversationLabel(conversation: ConversationGroup): string {
    const run = conversation.latestRun;
    return conversation.title || run.name || run.prompt || t("sidebar_uncategorized");
  }

  function openConversation(conversation: ConversationGroup) {
    const run = conversation.latestRun;
    if (isWorkRun(run) || realm === "work") {
      if (run.workspace_id) {
        goto(
          `/chat/work?workspace=${encodeURIComponent(run.workspace_id)}&run=${encodeURIComponent(run.id)}`,
        );
      } else {
        goto(`/chat/work?run=${encodeURIComponent(run.id)}`);
      }
      return;
    }
    goto(getRunRoute(run, { runId: run.id }));
  }

  async function unarchiveConversation(conversation: ConversationGroup) {
    if (busyId) return;
    busyId = conversation.groupKey;
    error = "";
    try {
      await api.setRunFlags(conversation.latestRun.id, { archived: false });
      dispatchRunMutation({
        kind: "update",
        runId: conversation.latestRun.id,
        patch: { archived: false },
      });
    } catch (e) {
      error = String(e);
      dbgWarn("archived-chats", "unarchive failed", e);
    } finally {
      busyId = "";
    }
  }

  async function deleteConversations(conversations: ConversationGroup[], confirmMessage: string) {
    if (busyId || conversations.length === 0) return;
    const { confirm } = await import("$lib/platform/dialog");
    const ok = await confirm(confirmMessage, {
      title: "删除对话",
      kind: "warning",
    });
    if (!ok) return;
    busyId = conversations.length === 1 ? conversations[0].groupKey : "__all__";
    error = "";
    try {
      const ids = [
        ...new Set(conversations.flatMap((conversation) => conversation.runs.map((run) => run.id))),
      ];
      await api.deleteRuns(ids);
      await Promise.all(ids.map((id) => deleteSnapshot(id)));
      dispatchRunMutation({ kind: "delete", runIds: ids });
    } catch (e) {
      error = String(e);
      dbgWarn("archived-chats", "delete archived conversations failed", e);
    } finally {
      busyId = "";
    }
  }
</script>

<div class="mx-auto w-full max-w-5xl px-6 py-10 lg:px-10">
  <div class="mb-7 flex items-start justify-between gap-4">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight text-foreground">
        {t("settings_archived_title")}
      </h1>
      <p class="mt-2 text-sm text-muted-foreground">{t("settings_archived_desc")}</p>
    </div>
    {#if archivedCount > 0}
      <button
        class="inline-flex shrink-0 items-center gap-1.5 rounded-lg border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs font-medium text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
        disabled={busyId !== ""}
        onclick={() =>
          deleteConversations(
            archivedFolders.flatMap((folder) => folder.conversations),
            t("settings_archived_deleteAllConfirm"),
          )}
      >
        <svg
          viewBox="0 0 24 24"
          class="h-3.5 w-3.5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="M3 6h18" />
          <path d="M19 6v14c0 1.1-.9 2-2 2H7c-1.1 0-2-.9-2-2V6" />
          <path d="M8 6V4c0-1.1.9-2 2-2h4c1.1 0 2 .9 2 2v2" />
        </svg>
        {t("settings_archived_deleteAll")}
      </button>
    {/if}
  </div>

  <Card class="overflow-hidden p-0">
    <div class="flex flex-col gap-3 border-b border-border/60 p-4 sm:flex-row">
      <label class="relative min-w-0 flex-1">
        <svg
          class="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <circle cx="11" cy="11" r="7" />
          <path d="m20 20-4-4" />
        </svg>
        <input
          bind:value={query}
          type="search"
          class="h-9 w-full rounded-lg border border-input bg-background pl-9 pr-3 text-sm outline-none transition-colors placeholder:text-muted-foreground/70 focus:border-ring focus:ring-2 focus:ring-ring/20"
          placeholder={t("settings_archived_search")}
          aria-label={t("settings_archived_search")}
        />
      </label>
      <select
        bind:value={projectFilter}
        class="h-9 min-w-40 rounded-lg border border-input bg-background px-3 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20"
        aria-label={t("settings_archived_projectFilter")}
      >
        <option value="">{t("settings_archived_allProjects")}</option>
        {#each projectOptions as project (project.cwd)}
          <option value={project.cwd}>{project.label}</option>
        {/each}
      </select>
    </div>

    {#if error}
      <div
        class="border-b border-destructive/20 bg-destructive/5 px-4 py-3 text-xs text-destructive"
      >
        {error}
      </div>
    {/if}

    {#if loading}
      <div class="flex items-center justify-center px-4 py-16 text-sm text-muted-foreground">
        {t("common_loading")}
      </div>
    {:else if archivedCount === 0}
      <div class="flex flex-col items-center justify-center px-4 py-20 text-center">
        <svg
          class="mb-3 h-9 w-9 text-muted-foreground/40"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <rect x="2" y="3" width="20" height="5" rx="1" />
          <path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8" />
          <path d="M10 12h4" />
        </svg>
        <p class="text-sm font-medium text-foreground">{t("settings_archived_empty")}</p>
        <p class="mt-1 text-xs text-muted-foreground">{t("settings_archived_emptyDesc")}</p>
      </div>
    {:else if visibleCount === 0}
      <div class="px-4 py-16 text-center text-sm text-muted-foreground">
        {t("settings_archived_noMatch")}
      </div>
    {:else}
      <div class="divide-y divide-border/60">
        {#each filteredFolders as folder (folder.folderKey)}
          <section>
            <div class="flex items-center gap-2 bg-muted/20 px-4 py-3">
              <svg
                class="h-4 w-4 shrink-0 text-muted-foreground"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
              >
                <path d="M3 7h5l2 2h11v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
                <path d="M3 7V5a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v2" />
              </svg>
              <span class="min-w-0 flex-1 truncate text-sm font-medium text-foreground">
                {folder.isUncategorized ? t("sidebar_uncategorized") : cwdDisplayLabel(folder.cwd)}
              </span>
              <span class="text-xs text-muted-foreground">
                {t("settings_archived_count", { count: String(folder.conversations.length) })}
              </span>
            </div>

            <div class="divide-y divide-border/40">
              {#each folder.conversations as conversation (conversation.groupKey)}
                <div
                  class="group flex items-center gap-3 px-4 py-3 transition-colors hover:bg-muted/30"
                >
                  <button
                    class="min-w-0 flex-1 text-left"
                    onclick={() => openConversation(conversation)}
                    title={conversationLabel(conversation)}
                  >
                    <p class="truncate text-sm font-medium text-foreground">
                      {conversationLabel(conversation)}
                    </p>
                    <div class="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
                      <span
                        >{relativeTime(
                          conversation.latestRun.last_activity_at ??
                            conversation.latestRun.started_at,
                        )}</span
                      >
                      <span aria-hidden="true">·</span>
                      <span class="capitalize"
                        >{conversation.latestRun.agent === "pi"
                          ? "Pi Agent"
                          : conversation.latestRun.agent}</span
                      >
                    </div>
                  </button>
                  <button
                    class="shrink-0 rounded-lg px-2.5 py-1.5 text-xs font-medium text-muted-foreground opacity-70 transition-colors hover:bg-accent hover:text-foreground group-hover:opacity-100 disabled:opacity-40"
                    disabled={busyId !== ""}
                    onclick={() => unarchiveConversation(conversation)}
                  >
                    {busyId === conversation.groupKey
                      ? t("common_loading")
                      : t("settings_archived_unarchive")}
                  </button>
                  <button
                    class="shrink-0 rounded-lg p-1.5 text-muted-foreground/70 transition-colors hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100 disabled:opacity-40"
                    disabled={busyId !== ""}
                    onclick={() =>
                      deleteConversations(
                        [conversation],
                        t("settings_archived_deleteConfirm", {
                          title: conversationLabel(conversation),
                        }),
                      )}
                    aria-label={t("settings_archived_delete")}
                    title={t("settings_archived_delete")}
                  >
                    <svg
                      viewBox="0 0 24 24"
                      class="h-4 w-4"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                    >
                      <path d="M3 6h18" />
                      <path d="M19 6v14c0 1.1-.9 2-2 2H7c-1.1 0-2-.9-2-2V6" />
                      <path d="M8 6V4c0-1.1.9-2 2-2h4c1.1 0 2 .9 2 2v2" />
                    </svg>
                  </button>
                </div>
              {/each}
            </div>
          </section>
        {/each}
      </div>
    {/if}
  </Card>
</div>
