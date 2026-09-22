import { describe, expect, it } from "vitest";
import { parseTurnReviewDiff } from "$lib/utils/turn-review";

describe("parseTurnReviewDiff", () => {
  it("creates paired side-by-side rows for a changed file", () => {
    const files = parseTurnReviewDiff(`diff --git a/src/a.ts b/src/a.ts
index 1111111..2222222 100644
--- a/src/a.ts
+++ b/src/a.ts
@@ -4,3 +4,4 @@
 keep
-old value
+new value
+added value
 tail
`);

    expect(files).toHaveLength(1);
    expect(files[0]).toMatchObject({ path: "src/a.ts", insertions: 2, deletions: 1 });
    expect(files[0].hunks[0].rows).toEqual([
      { kind: "context", oldLine: 4, newLine: 4, oldText: "keep", newText: "keep" },
      { kind: "change", oldLine: 5, newLine: 5, oldText: "old value", newText: "new value" },
      { kind: "addition", oldLine: null, newLine: 6, oldText: "", newText: "added value" },
      { kind: "context", oldLine: 6, newLine: 7, oldText: "tail", newText: "tail" },
    ]);
  });

  it("splits a multi-file turn patch", () => {
    const files = parseTurnReviewDiff(`diff --git a/a.txt b/a.txt
--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-a
+A
diff --git a/b.txt b/b.txt
new file mode 100644
--- /dev/null
+++ b/b.txt
@@ -0,0 +1 @@
+b
`);

    expect(files.map((file) => file.path)).toEqual(["a.txt", "b.txt"]);
    expect(files[1]).toMatchObject({ insertions: 1, deletions: 0 });
  });
});
