<script lang="ts">
  import type { PiSessionTreeState } from "$lib/types";
  import PiSessionTreeNode from "$lib/components/PiSessionTreeNode.svelte";

  let {
    open = $bindable(false),
    tree,
    busy = false,
    forkAvailable = true,
    cloneLabel = "Clone",
    onRefresh,
    onFork,
    onClone,
  }: {
    open?: boolean;
    tree: PiSessionTreeState;
    busy?: boolean;
    forkAvailable?: boolean;
    cloneLabel?: string;
    onRefresh?: () => void;
    onFork?: (entryId: string) => void | Promise<void>;
    onClone?: () => void | Promise<void>;
  } = $props();
</script>

{#if open}
  <div
    class="fixed inset-y-0 right-0 z-40 w-[min(420px,92vw)] border-l border-border bg-background shadow-2xl"
  >
    <div class="flex items-center justify-between border-b border-border px-4 py-3">
      <div>
        <h2 class="text-sm font-medium">Pi Session Tree</h2>
        <p class="text-[11px] text-muted-foreground">Leaf: {tree.leafId ?? "empty"}</p>
      </div>
      <div class="flex gap-1">
        <button class="rounded px-2 py-1 text-xs hover:bg-muted" onclick={onRefresh} disabled={busy}
          >Refresh</button
        >
        <button
          class="rounded px-2 py-1 text-xs hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
          onclick={onClone}
          disabled={busy || !forkAvailable}>{busy ? "Working…" : cloneLabel}</button
        >
        <button class="rounded px-2 py-1 text-xs hover:bg-muted" onclick={() => (open = false)}
          >Close</button
        >
      </div>
    </div>
    {#if tree.error}<p class="m-3 rounded bg-destructive/10 p-2 text-xs text-destructive">
        {tree.error}
      </p>{/if}
    {#if tree.loading}<p class="p-4 text-xs text-muted-foreground">Loading…</p>{:else}
      <div class="h-[calc(100%-64px)] overflow-auto p-3">
        {#if tree.roots.length === 0}<p class="text-xs text-muted-foreground">No entries.</p>{/if}
        {#each tree.roots as node (node.entry.id)}
          <PiSessionTreeNode {node} leafId={tree.leafId} {onFork} {busy} {forkAvailable} />
        {/each}
      </div>
    {/if}
  </div>
{/if}
