<script lang="ts">
  import type { GitProjectInfo, GitWorktreeInfo } from "$lib/types";
  import { t } from "$lib/i18n/index.svelte";
  import { IS_MAC } from "$lib/utils/platform";

  let {
    open = $bindable(false),
    project,
    worktrees,
    busy = false,
    onRefresh,
    onCreate,
    onRemove,
  }: {
    open?: boolean;
    project: GitProjectInfo | null;
    worktrees: GitWorktreeInfo[];
    busy?: boolean;
    onRefresh?: () => void;
    onCreate?: (branch: string, path: string) => void;
    onRemove?: (path: string, force: boolean) => void;
  } = $props();

  let branch = $state("");
  let path = $state("");
</script>

{#if open}
  <div
    class="fixed inset-y-0 left-0 z-40 flex w-[min(440px,92vw)] flex-col border-r border-border bg-background shadow-2xl"
  >
    <div
      class="flex flex-none items-start justify-between gap-4 border-b border-border px-4 {IS_MAC
        ? 'pb-3 pt-10'
        : 'py-3'}"
    >
      <div class="min-w-0 flex-1">
        <h2 class="text-sm font-medium">{t("chat_worktree_panelTitle")}</h2>
        <p class="mt-0.5 max-w-[300px] text-[10px] leading-4 text-muted-foreground">
          {t("chat_worktree_panelDesc")}
        </p>
        <p class="text-[11px] text-muted-foreground truncate max-w-[300px]">
          {project?.projectRoot ?? t("chat_worktree_notGit")}
        </p>
      </div>
      <div class="flex shrink-0 gap-1 pt-0.5">
        <button class="rounded px-2 py-1 text-xs hover:bg-muted" onclick={onRefresh} disabled={busy}
          >{t("chat_worktree_refresh")}</button
        >
        <button class="rounded px-2 py-1 text-xs hover:bg-muted" onclick={() => (open = false)}
          >{t("chat_worktree_close")}</button
        >
      </div>
    </div>

    <div class="flex-none space-y-2 border-b border-border p-3">
      <div class="flex gap-2">
        <input
          class="min-w-0 flex-1 rounded border border-border bg-background px-2 py-1 text-xs"
          placeholder={t("chat_worktree_branchPlaceholder")}
          bind:value={branch}
        />
        <input
          class="min-w-0 flex-1 rounded border border-border bg-background px-2 py-1 text-xs"
          placeholder={t("chat_worktree_pathPlaceholder")}
          bind:value={path}
        />
      </div>
      <button
        class="rounded bg-primary px-2.5 py-1 text-xs text-primary-foreground disabled:opacity-50"
        disabled={busy || !branch.trim() || !project}
        onclick={() => {
          onCreate?.(branch.trim(), path.trim());
          branch = "";
          path = "";
        }}>{t("chat_worktree_create")}</button
      >
    </div>

    <div class="min-h-0 flex-1 space-y-2 overflow-auto p-3">
      {#if worktrees.length === 0}
        <p class="text-xs text-muted-foreground">{t("chat_worktree_empty")}</p>
      {:else}
        {#each worktrees as item (item.path)}
          <div class="rounded border border-border/60 p-2 text-xs">
            <div class="flex items-start justify-between gap-2">
              <div class="min-w-0">
                <div class="truncate font-medium">{item.branch ?? t("chat_worktree_detached")}</div>
                <div class="truncate text-[10px] text-muted-foreground">{item.path}</div>
              </div>
              {#if !item.isMain}
                <div class="flex shrink-0 gap-1">
                  <button
                    class="text-destructive hover:underline disabled:opacity-50"
                    disabled={busy || item.isDirty}
                    onclick={() => onRemove?.(item.path, false)}>{t("chat_worktree_remove")}</button
                  >
                  {#if item.isDirty}<button
                      class="text-amber-500 hover:underline"
                      disabled={busy}
                      onclick={() => onRemove?.(item.path, true)}
                      >{t("chat_worktree_forceRemove")}</button
                    >{/if}
                </div>
              {/if}
            </div>
            <div class="mt-1 text-[10px] text-muted-foreground">
              {item.isCurrent ? t("chat_worktree_current") : ""}{item.isDirty
                ? ` · ${t("chat_worktree_dirty")}`
                : ` · ${t("chat_worktree_clean")}`}{item.isPrunable
                ? ` · ${t("chat_worktree_prunable")}`
                : ""}
            </div>
          </div>
        {/each}
      {/if}
    </div>
  </div>
{/if}
