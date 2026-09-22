<script lang="ts">
  import type { MemoryFileCandidate } from "$lib/types";
  import { filterVisibleCandidates } from "$lib/utils/memory-helpers";
  import { t } from "$lib/i18n/index.svelte";

  let {
    files,
    selectedPath,
    onSelect,
  }: {
    files: MemoryFileCandidate[];
    selectedPath: string;
    onSelect: (file: MemoryFileCandidate) => void;
  } = $props();

  let visibleFiles = $derived(filterVisibleCandidates(files, true, selectedPath));
</script>

{#each visibleFiles as file (file.path)}
  <button
    class="flex w-full items-center gap-1.5 py-1 pl-4 pr-3 text-xs transition-colors
      {selectedPath === file.path
      ? 'bg-sidebar-accent text-sidebar-foreground'
      : 'text-muted-foreground hover:bg-sidebar-accent/50 hover:text-sidebar-foreground'}"
    onclick={() => onSelect(file)}
    title={file.path}
  >
    <svg
      class="h-3 w-3 shrink-0 {file.scope === 'memory'
        ? 'text-amber-400'
        : file.exists
          ? 'text-blue-400'
          : 'text-muted-foreground/40'}"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
      ><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" /><path
        d="M14 2v4a2 2 0 0 0 2 2h4"
      /></svg
    >
    <span class="min-w-0 truncate">{file.label}</span>
    {#if !file.exists}
      <span class="ml-auto shrink-0 text-[10px] text-muted-foreground">{t("memory_new")}</span>
    {/if}
  </button>
{/each}
