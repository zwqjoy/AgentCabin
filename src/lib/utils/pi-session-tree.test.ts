import { describe, expect, it } from "vitest";
import type { PiSessionTreeNode } from "$lib/types";
import { nodeContainsLeaf, treeVisualDepth } from "./pi-session-tree";

function node(id: string, children: PiSessionTreeNode[] = []): PiSessionTreeNode {
  return { entry: { id, type: "message" }, children };
}

describe("Pi session tree helpers", () => {
  const tree = node("root", [
    node("branch-a", [node("deep-a", [node("leaf-a")])]),
    node("branch-b"),
  ]);

  it("finds leaves at arbitrary depth", () => {
    expect(nodeContainsLeaf(tree, "leaf-a")).toBe(true);
    expect(nodeContainsLeaf(tree, "branch-b")).toBe(true);
    expect(nodeContainsLeaf(tree, "missing")).toBe(false);
  });

  it("supports default expansion of the active leaf path", () => {
    expect(nodeContainsLeaf(tree, "leaf-a")).toBe(true);
    expect(nodeContainsLeaf(tree.children[1], "leaf-a")).toBe(false);
  });

  it("caps visual indentation for very deep branches", () => {
    expect(treeVisualDepth(3)).toBe(3);
    expect(treeVisualDepth(30)).toBe(8);
  });
});
