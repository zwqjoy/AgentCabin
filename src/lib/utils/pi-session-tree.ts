import type { PiSessionTreeNode } from "$lib/types";

/** True when a node is on the path to the authoritative Pi leaf. */
export function nodeContainsLeaf(node: PiSessionTreeNode, leafId: string | null): boolean {
  if (!leafId) return false;
  return node.entry.id === leafId || node.children.some((child) => nodeContainsLeaf(child, leafId));
}

/** Keep deep trees readable without hiding the actual hierarchy. */
export function treeVisualDepth(depth: number, maxDepth = 8): number {
  return Math.min(Math.max(depth, 0), maxDepth);
}
