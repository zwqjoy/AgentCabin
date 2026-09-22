<script lang="ts">
  import type { PiSessionTreeNode as TreeNode } from "$lib/types";
  import { nodeContainsLeaf, treeVisualDepth } from "$lib/utils/pi-session-tree";

  let {
    node,
    leafId,
    depth = 0,
    onFork,
    busy = false,
    forkAvailable = true,
  }: {
    node: TreeNode;
    leafId: string | null;
    depth?: number;
    onFork?: (entryId: string) => void | Promise<void>;
    busy?: boolean;
    forkAvailable?: boolean;
  } = $props();

  let expanded = $state(false);
  let initializedLeaf = $state<string | null>(null);
  const hasChildren = $derived(node.children.length > 0);
  const onLeafPath = $derived(nodeContainsLeaf(node, leafId));
  const isLeaf = $derived(node.entry.id === leafId);
  const visualDepth = $derived(treeVisualDepth(depth));

  $effect(() => {
    if (initializedLeaf !== leafId) {
      expanded = onLeafPath;
      initializedLeaf = leafId;
    }
  });

  const summary = (entry: TreeNode["entry"]) => {
    const message = entry.message as { content?: unknown } | undefined;
    const text =
      typeof message?.content === "string" ? message.content : (entry.customType ?? entry.type);
    return String(text).replace(/\s+/g, " ").slice(0, 90);
  };
</script>

<div class="space-y-1" style={`padding-left: ${visualDepth * 14}px`}>
  <div
    class="rounded border p-2 text-xs {isLeaf
      ? 'border-primary/60 bg-primary/10'
      : 'border-border/60'}"
  >
    <div class="flex items-start gap-1.5">
      {#if hasChildren}
        <button
          class="mt-0.5 shrink-0 text-muted-foreground hover:text-foreground"
          aria-label={expanded ? "Collapse branch" : "Expand branch"}
          onclick={() => (expanded = !expanded)}
        >
          {expanded ? "▾" : "▸"}
        </button>
      {:else}
        <span class="w-3.5 shrink-0"></span>
      {/if}
      <span class="min-w-0 flex-1 truncate">{summary(node.entry)}</span>
      <button
        class="shrink-0 text-primary hover:underline disabled:cursor-not-allowed disabled:opacity-50"
        onclick={() => void onFork?.(node.entry.id)}
        disabled={busy || !forkAvailable}
        title={forkAvailable ? "Fork" : "当前会话正在执行中"}
      >
        {busy ? "Working…" : "Fork"}
      </button>
    </div>
    <div class="mt-1 pl-5 text-[10px] text-muted-foreground">
      {node.entry.type} · {node.entry.id}
    </div>
  </div>

  {#if expanded}
    {#each node.children as child (child.entry.id)}
      <svelte:self node={child} {leafId} depth={depth + 1} {onFork} {busy} {forkAvailable} />
    {/each}
  {/if}
</div>
