import { parseUnifiedDiffStats } from "$lib/utils/diff-stats";

export type TurnReviewRowKind = "context" | "change" | "addition" | "deletion" | "meta";

export interface TurnReviewRow {
  kind: TurnReviewRowKind;
  oldLine: number | null;
  newLine: number | null;
  oldText: string;
  newText: string;
}

export interface TurnReviewHunk {
  header: string;
  rows: TurnReviewRow[];
}

export interface TurnReviewFile {
  path: string;
  insertions: number;
  deletions: number;
  hunks: TurnReviewHunk[];
}

/** Parse a unified Git patch into side-by-side rows for the turn review panel. */
export function parseTurnReviewDiff(diffText: string): TurnReviewFile[] {
  const normalized = diffText.replaceAll("\r\n", "\n");
  if (!normalized.trim()) return [];

  const lines = normalized.split("\n");
  const starts: number[] = [];
  for (let index = 0; index < lines.length; index++) {
    if (lines[index].startsWith("diff --git ")) starts.push(index);
  }

  const chunks =
    starts.length > 0
      ? starts.map((start, index) => lines.slice(start, starts[index + 1] ?? lines.length))
      : [lines];

  return chunks.map(parseFileChunk).filter((file): file is TurnReviewFile => file !== null);
}

function parseFileChunk(lines: string[]): TurnReviewFile | null {
  const raw = lines.join("\n");
  const stats = parseUnifiedDiffStats(raw).files[0];
  const path = stats?.path ?? fallbackPath(lines);
  if (!path) return null;

  const hunks: TurnReviewHunk[] = [];
  let index = 0;
  while (index < lines.length) {
    const header = lines[index];
    if (!header.startsWith("@@")) {
      index++;
      continue;
    }

    const match = header.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
    let oldLine = match ? Number(match[1]) : 0;
    let newLine = match ? Number(match[2]) : 0;
    const body: string[] = [];
    index++;
    while (index < lines.length && !lines[index].startsWith("@@")) {
      if (lines[index].startsWith("diff --git ")) break;
      body.push(lines[index]);
      index++;
    }

    const rows: TurnReviewRow[] = [];
    for (let bodyIndex = 0; bodyIndex < body.length; ) {
      const line = body[bodyIndex];
      if (line.startsWith("-")) {
        const removed: string[] = [];
        while (bodyIndex < body.length && body[bodyIndex].startsWith("-")) {
          removed.push(body[bodyIndex].slice(1));
          bodyIndex++;
        }
        const added: string[] = [];
        while (bodyIndex < body.length && body[bodyIndex].startsWith("+")) {
          added.push(body[bodyIndex].slice(1));
          bodyIndex++;
        }
        const count = Math.max(removed.length, added.length);
        for (let rowIndex = 0; rowIndex < count; rowIndex++) {
          const oldText = removed[rowIndex];
          const newText = added[rowIndex];
          rows.push({
            kind:
              oldText !== undefined && newText !== undefined
                ? "change"
                : oldText !== undefined
                  ? "deletion"
                  : "addition",
            oldLine: oldText === undefined ? null : oldLine++,
            newLine: newText === undefined ? null : newLine++,
            oldText: oldText ?? "",
            newText: newText ?? "",
          });
        }
        continue;
      }

      if (line.startsWith("+")) {
        rows.push({
          kind: "addition",
          oldLine: null,
          newLine: newLine++,
          oldText: "",
          newText: line.slice(1),
        });
      } else if (line.startsWith(" ")) {
        rows.push({
          kind: "context",
          oldLine: oldLine++,
          newLine: newLine++,
          oldText: line.slice(1),
          newText: line.slice(1),
        });
      } else if (line.startsWith("\\ No newline")) {
        rows.push({
          kind: "meta",
          oldLine: null,
          newLine: null,
          oldText: line,
          newText: line,
        });
      }
      bodyIndex++;
    }
    hunks.push({ header, rows });
  }

  return {
    path,
    insertions: stats?.insertions ?? 0,
    deletions: stats?.deletions ?? 0,
    hunks,
  };
}

function fallbackPath(lines: string[]): string {
  const marker =
    lines
      .find((line) => line.startsWith("+++ "))
      ?.slice(4)
      .trim() ?? "";
  return marker.replace(/^b\//, "").split("\t", 1)[0];
}
