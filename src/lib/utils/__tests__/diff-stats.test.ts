import { describe, expect, it } from "vitest";
import {
  extractTurnModifiedPaths,
  getCurrentTurnEntries,
  parseUnifiedDiffStats,
  summarizeTurnChanges,
} from "../diff-stats";
import type { TimelineEntry } from "$lib/types";

describe("parseUnifiedDiffStats", () => {
  it("counts multiple git diff sections and ignores headers", () => {
    const diff = `diff --git a/src/app.ts b/src/app.ts
index 111..222 100644
--- a/src/app.ts
+++ b/src/app.ts
@@ -1,3 +1,4 @@
 const keep = true;
-const oldValue = 1;
+const newValue = 2;
+const anotherValue = 3;
\\ No newline at end of file
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-old docs
+new docs`;

    expect(parseUnifiedDiffStats(diff)).toEqual({
      files: [
        { path: "src/app.ts", insertions: 2, deletions: 1 },
        { path: "README.md", insertions: 1, deletions: 1 },
      ],
      totalInsertions: 3,
      totalDeletions: 2,
    });
  });

  it("uses the non-null side for new and deleted files", () => {
    const diff = `diff --git a/new.ts b/new.ts
new file mode 100644
--- /dev/null
+++ b/new.ts
@@ -0,0 +1,2 @@
+one
+two
diff --git a/old.ts b/old.ts
deleted file mode 100644
--- a/old.ts
+++ /dev/null
@@ -1,2 +0,0 @@
-one
-two`;

    expect(parseUnifiedDiffStats(diff).files).toEqual([
      { path: "new.ts", insertions: 2, deletions: 0 },
      { path: "old.ts", insertions: 0, deletions: 2 },
    ]);
  });

  it("supports a plain patch without diff --git delimiters", () => {
    const diff = `--- a/one.txt
+++ b/one.txt
@@ -1 +1,2 @@
-one
+one updated
+extra
--- a/two.txt
+++ b/two.txt
@@ -1 +1 @@
-two
+two updated`;

    expect(parseUnifiedDiffStats(diff)).toMatchObject({
      files: [
        { path: "one.txt", insertions: 2, deletions: 1 },
        { path: "two.txt", insertions: 1, deletions: 1 },
      ],
      totalInsertions: 3,
      totalDeletions: 2,
    });
  });

  it("merges repeated sections for the same file", () => {
    const diff = `diff --git a/a.ts b/a.ts
--- a/a.ts
+++ b/a.ts
@@ -1 +1 @@
-a
+b
diff --git a/a.ts b/a.ts
--- a/a.ts
+++ b/a.ts
@@ -3 +3 @@
-c
+d`;

    expect(parseUnifiedDiffStats(diff).files).toEqual([
      { path: "a.ts", insertions: 2, deletions: 2 },
    ]);
  });

  it("returns an empty summary for empty or non-diff text", () => {
    expect(parseUnifiedDiffStats("")).toEqual({
      files: [],
      totalInsertions: 0,
      totalDeletions: 0,
    });
    expect(parseUnifiedDiffStats("no diff here").files).toEqual([]);
  });

  it("uses the Codex turn diff before timeline fallbacks", () => {
    const timeline = [toolEntry("Edit", { file_path: "old.ts", old_string: "a", new_string: "b" })];
    const result = summarizeTurnChanges(
      timeline,
      `diff --git a/new.ts b/new.ts
--- a/new.ts
+++ b/new.ts
@@ -1 +1,2 @@
-a
+b
+c`,
    );

    expect(result.files).toEqual([{ path: "new.ts", insertions: 2, deletions: 1 }]);
  });

  it("aggregates Claude/Pi Edit and Write argument shapes after the latest user turn", () => {
    const timeline: TimelineEntry[] = [
      userEntry("previous turn"),
      toolEntry("Edit", { file_path: "old.ts", old_string: "old\nline", new_string: "new" }),
      userEntry("current turn"),
      toolEntry("Edit", { path: "src/pi.ts", oldText: "one\ntwo", newText: "one\nupdated" }),
      toolEntry(
        "Write",
        { file_path: "new.ts", content: "one\ntwo\n" },
        {
          result: { filePath: "new.ts", structuredPatch: [{ lines: ["+one", "+two"] }] },
        },
      ),
    ];

    expect(summarizeTurnChanges(timeline)).toEqual({
      files: [
        { path: "src/pi.ts", insertions: 1, deletions: 1 },
        { path: "new.ts", insertions: 2, deletions: 0 },
      ],
      totalInsertions: 3,
      totalDeletions: 1,
    });
  });

  it("parses the nested patch emitted by a real Pi RPC Edit", () => {
    const timeline: TimelineEntry[] = [
      userEntry("current turn"),
      toolEntry(
        "Edit",
        {
          path: "README.md",
          edits: [{ oldText: "Copyright", newText: "Copyright\n\n<!-- test -->" }],
        },
        {
          result: {
            details: {
              patch: "--- README.md\n+++ README.md\n@@ -1 +1,3 @@\n Copyright\n+\n+<!-- test -->",
            },
          },
        },
      ),
    ];

    expect(summarizeTurnChanges(timeline)).toEqual({
      files: [{ path: "README.md", insertions: 2, deletions: 0 }],
      totalInsertions: 2,
      totalDeletions: 0,
    });
  });

  it("recognizes replayed Grok search_replace payloads despite the generic completion name", () => {
    const timeline: TimelineEntry[] = [
      userEntry("current turn"),
      toolEntry("Grok Tool", {
        file_path: "README.md",
        old_string: "<!-- old -->",
        new_string: "<!-- old -->\n<!-- test -->",
      }),
    ];

    expect(summarizeTurnChanges(timeline)).toEqual({
      files: [{ path: "README.md", insertions: 2, deletions: 1 }],
      totalInsertions: 2,
      totalDeletions: 1,
    });
  });
});

function userEntry(content: string): TimelineEntry {
  return {
    kind: "user",
    id: `user-${content}`,
    anchorId: `user-${content}`,
    content,
    ts: new Date().toISOString(),
  };
}

function toolEntry(
  tool_name: string,
  input: Record<string, unknown>,
  options?: { result?: Record<string, unknown> },
): TimelineEntry {
  return {
    kind: "tool",
    id: `tool-${tool_name}-${input.path ?? input.file_path ?? "file"}`,
    anchorId: `tool-${tool_name}-${input.path ?? input.file_path ?? "file"}`,
    ts: new Date().toISOString(),
    tool: {
      tool_use_id: `tool-use-${tool_name}`,
      tool_name,
      input,
      status: "success",
      tool_use_result: options?.result,
    },
  };
}

describe("extractTurnModifiedPaths and getCurrentTurnEntries", () => {
  it("isolates entries in the latest turn window", () => {
    const timeline: TimelineEntry[] = [
      userEntry("first question"),
      toolEntry("Edit", { path: "old.ts", old_string: "a", new_string: "b" }),
      userEntry("second question"),
      toolEntry("Read", { path: "read.ts" }),
    ];

    const currentEntries = getCurrentTurnEntries(timeline);
    expect(currentEntries).toHaveLength(1);
    expect(currentEntries[0]).toMatchObject({ kind: "tool" });
  });

  it("returns empty paths when turn has no mutation tools", () => {
    const turnEntries: TimelineEntry[] = [
      toolEntry("Read", { path: "src/capabilities.rs" }),
      toolEntry("Grep", { path: "src/capabilities.rs" }),
    ];

    expect(extractTurnModifiedPaths(turnEntries)).toEqual([]);
  });

  it("returns modified file paths for edit and write tools", () => {
    const turnEntries: TimelineEntry[] = [
      toolEntry("Read", { path: "read.ts" }),
      toolEntry("Edit", { path: "src/capabilities.rs", old_string: "a", new_string: "b" }),
      toolEntry("write", { file_path: "src/new-file.ts" }),
    ];

    expect(extractTurnModifiedPaths(turnEntries).sort()).toEqual([
      "src/capabilities.rs",
      "src/new-file.ts",
    ]);
  });
});
