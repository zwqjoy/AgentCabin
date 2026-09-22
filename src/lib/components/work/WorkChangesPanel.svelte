<script lang="ts">
  import type { WorkFileChangeKind, WorkReceiptFileChange } from "$lib/types/work";

  interface Props {
    changes: WorkReceiptFileChange[];
    inputFiles?: string[];
  }

  let { changes, inputFiles = [] }: Props = $props();

  const groups = $derived.by(() => {
    const created = changes.filter((c) => c.changeKind === "created");
    const modified = changes.filter((c) => c.changeKind === "modified");
    const deleted = changes.filter((c) => c.changeKind === "deleted");
    return { created, modified, deleted };
  });

  const kindMeta: Record<WorkFileChangeKind, { label: string; className: string }> = {
    created: { label: "新建", className: "text-emerald-600 dark:text-emerald-400" },
    modified: { label: "修改", className: "text-amber-600 dark:text-amber-400" },
    deleted: { label: "删除", className: "text-red-600 dark:text-red-400" },
  };

  const kindOrder: WorkFileChangeKind[] = ["created", "modified", "deleted"];

  let showInput = $state(false);
</script>

<div class="space-y-2">
  {#if changes.length === 0}
    <p
      class="rounded-lg border border-dashed border-border/70 px-3 py-2 text-[11px] text-muted-foreground"
    >
      本次运行没有文件变更记录。
    </p>
  {:else}
    <div class="flex items-center gap-3 text-[11px]">
      {#each kindOrder as kind (kind)}
        <span class={kindMeta[kind].className}>
          {kindMeta[kind].label}
          {groups[kind].length}
        </span>
      {/each}
    </div>
    <ul class="space-y-0.5">
      {#each changes as change (`${change.path}:${change.changeKind}`)}
        <li class="flex items-center gap-2 rounded px-2 py-1 hover:bg-accent/30">
          <span
            class="w-8 shrink-0 text-[10px] font-semibold {kindMeta[change.changeKind].className}"
          >
            {#if change.changeKind === "created"}+{:else if change.changeKind === "deleted"}−{:else}~{/if}
            <span class="sr-only">{kindMeta[change.changeKind].label}</span>
          </span>
          <span
            class="min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground"
            title={change.path}
          >
            {change.path}
          </span>
        </li>
      {/each}
    </ul>
  {/if}

  {#if inputFiles.length > 0}
    <div>
      <button
        type="button"
        class="flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground transition-colors hover:text-foreground"
        aria-expanded={showInput}
        onclick={() => (showInput = !showInput)}
      >
        <span class="transition-transform {showInput ? 'rotate-90' : ''}" aria-hidden="true">›</span
        >
        读取的输入文件 · {inputFiles.length}
      </button>
      {#if showInput}
        <ul class="mt-1 space-y-0.5">
          {#each inputFiles as file (file)}
            <li>
              <span
                class="block truncate rounded px-2 py-1 font-mono text-[10px] text-muted-foreground"
                title={file}
              >
                {file}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</div>
