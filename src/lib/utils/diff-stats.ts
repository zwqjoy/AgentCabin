import { structuredPatch } from "diff";
import type { BusToolItem, TimelineEntry } from "$lib/types";

/** A compact per-file summary extracted from a unified diff. */
export interface UnifiedDiffFileStat {
  path: string;
  insertions: number;
  deletions: number;
}

export interface UnifiedDiffSummary {
  files: UnifiedDiffFileStat[];
  totalInsertions: number;
  totalDeletions: number;
}

interface MutableFileStat {
  hint: string;
  oldPath?: string | null;
  newPath?: string | null;
  insertions: number;
  deletions: number;
  hasGitHeader: boolean;
}

/**
 * Parse a git-style unified diff into the file-level counts used by the Chat changes popover.
 *
 * This deliberately counts only hunk lines. File headers (`---`/`+++`), hunk headers (`@@`),
 * and the "No newline" marker are not counted. Codex normally sends `diff --git` sections, but
 * the parser also accepts a plain `---`/`+++` patch so older or provider-generated diffs still
 * produce useful stats.
 */
export function parseUnifiedDiffStats(diffText: string): UnifiedDiffSummary {
  if (!diffText.trim()) return emptySummary();

  const files: UnifiedDiffFileStat[] = [];
  let current: MutableFileStat | null = null;

  const flush = () => {
    if (!current) return;
    const path = current.newPath ?? current.oldPath ?? current.hint;
    if (path) {
      files.push({
        path,
        insertions: current.insertions,
        deletions: current.deletions,
      });
    }
    current = null;
  };

  for (const line of diffText.replaceAll("\r\n", "\n").split("\n")) {
    if (line.startsWith("diff --git ")) {
      flush();
      current = {
        hint: parseGitHeaderPath(line),
        insertions: 0,
        deletions: 0,
        hasGitHeader: true,
      };
      continue;
    }

    // In a normal git section these are the two file headers. In a plain patch, a second `---`
    // pair starts the next file because there is no `diff --git` delimiter to do that for us.
    if (
      line.startsWith("--- ") &&
      (!current || current.newPath === undefined || !current.hasGitHeader)
    ) {
      if (current?.oldPath !== undefined) flush();
      current ??= {
        hint: "",
        insertions: 0,
        deletions: 0,
        hasGitHeader: false,
      };
      current.oldPath = parseFileHeaderPath(line.slice(4), "a/");
      continue;
    }

    if (
      line.startsWith("+++ ") &&
      current?.oldPath !== undefined &&
      current.newPath === undefined
    ) {
      current.newPath = parseFileHeaderPath(line.slice(4), "b/");
      continue;
    }

    // A removed source line can itself begin with `--`, producing a line that starts with `---`.
    // The header branches above only run before a git section's `+++` header, so it is safe to
    // count every remaining `-`/`+` line as hunk content.
    if (line.startsWith("+")) {
      current ??= createAnonymousFile();
      current.insertions += 1;
    } else if (line.startsWith("-")) {
      current ??= createAnonymousFile();
      current.deletions += 1;
    }
  }
  flush();

  // A diff can contain repeated sections for the same path (for example, when a provider
  // concatenates updates). Present one stable row per file while preserving first-seen order.
  const merged: UnifiedDiffFileStat[] = [];
  const byPath = new Map<string, UnifiedDiffFileStat>();
  for (const file of files) {
    const existing = byPath.get(file.path);
    if (existing) {
      existing.insertions += file.insertions;
      existing.deletions += file.deletions;
    } else {
      const copy = { ...file };
      byPath.set(copy.path, copy);
      merged.push(copy);
    }
  }

  return {
    files: merged,
    totalInsertions: merged.reduce((sum, file) => sum + file.insertions, 0),
    totalDeletions: merged.reduce((sum, file) => sum + file.deletions, 0),
  };
}

function emptySummary(): UnifiedDiffSummary {
  return { files: [], totalInsertions: 0, totalDeletions: 0 };
}

function createAnonymousFile(): MutableFileStat {
  return {
    hint: "",
    insertions: 0,
    deletions: 0,
    hasGitHeader: false,
  };
}

function parseGitHeaderPath(line: string): string {
  const match = /^diff --git a\/(.+?) b\/(.+)$/.exec(line);
  return normalizePath(match?.[2] ?? match?.[1] ?? "");
}

function parseFileHeaderPath(raw: string, prefix: string): string | null {
  const path = normalizePath(raw);
  if (!path || path === "/dev/null") return null;
  return path.startsWith(prefix) ? path.slice(prefix.length) : path;
}

function normalizePath(raw: string): string {
  let path = raw.trim().split("\t", 1)[0].trim();
  if (path.length >= 2 && path.startsWith('"') && path.endsWith('"')) {
    path = path.slice(1, -1).replaceAll('\\"', '"').replaceAll("\\\\", "\\");
  }
  return path;
}

/**
 * Build the current-turn summary for any supported agent.
 *
 * Codex app-server provides the authoritative cumulative unified diff. Claude and Pi expose
 * their edit/write arguments and structured tool results instead, so we derive the same shape
 * from the timeline after the most recent user message. This keeps the UI agent-neutral without
 * manufacturing counts when a provider only reports a file path.
 */
export function summarizeTurnChanges(timeline: TimelineEntry[], turnDiff = ""): UnifiedDiffSummary {
  const parsedTurnDiff = parseUnifiedDiffStats(turnDiff);
  if (parsedTurnDiff.files.length > 0) return parsedTurnDiff;

  return summarizeTimelineChanges(getCurrentTurnEntries(timeline));
}

/**
 * Return all timeline entries in the current (latest) turn window, i.e. after the last user entry.
 */
export function getCurrentTurnEntries(timeline: TimelineEntry[]): TimelineEntry[] {
  let lastUserIndex = -1;
  for (let i = timeline.length - 1; i >= 0; i--) {
    if (timeline[i]?.kind === "user") {
      lastUserIndex = i;
      break;
    }
  }
  return lastUserIndex >= 0 ? timeline.slice(lastUserIndex + 1) : timeline;
}

/**
 * Extract distinct file paths modified by mutation tools in a timeline fragment.
 */
export function extractTurnModifiedPaths(entries: TimelineEntry[]): string[] {
  const paths = new Set<string>();
  const summary = summarizeTimelineChanges(entries);
  for (const f of summary.files) {
    if (f.path) paths.add(f.path);
  }

  function walk(items: TimelineEntry[]): void {
    for (const entry of items) {
      if (entry.kind !== "tool") continue;
      const tool = entry.tool;
      if (isRejectedTool(tool)) continue;
      if (isMutationTool(tool)) {
        const result = asRecord(tool.tool_use_result);
        const p = extractMutationPath(tool, result);
        if (p) paths.add(p);
      }
      if (entry.subTimeline) walk(entry.subTimeline);
    }
  }

  walk(entries);
  return Array.from(paths);
}

/** Summarize Edit/Write tools in a timeline fragment, including nested subagent tools. */
export function summarizeTimelineChanges(entries: TimelineEntry[]): UnifiedDiffSummary {
  const files: UnifiedDiffFileStat[] = [];

  function walk(items: TimelineEntry[]): void {
    for (const entry of items) {
      if (entry.kind !== "tool") continue;
      const tool = entry.tool;

      // Failed/denied tools do not represent a committed file change. Running tools are included
      // because their input already contains enough information for the popover to update live.
      if (!isRejectedTool(tool)) files.push(...summarizeMutationTool(tool));
      if (entry.subTimeline) walk(entry.subTimeline);
    }
  }

  walk(entries);
  return mergeFileStats(files);
}

function isRejectedTool(tool: BusToolItem): boolean {
  return tool.status === "error" || tool.status === "denied" || tool.status === "permission_denied";
}

export function isMutationTool(tool: BusToolItem): boolean {
  const name = tool.tool_name.toLowerCase();
  const input = tool.input ?? {};
  const hasTextReplacement =
    firstString(input, ["old_string", "oldText", "oldString", "old_text"]) !== undefined &&
    firstString(input, ["new_string", "newText", "newString", "new_text"]) !== undefined;
  const isEdit =
    name === "edit" ||
    name === "edit_file" ||
    name === "search_replace" ||
    name === "str_replace_editor" ||
    (name === "grok tool" && hasTextReplacement);
  const isWrite = name === "write" || name === "write_file";
  return isEdit || isWrite;
}

function summarizeMutationTool(tool: BusToolItem): UnifiedDiffFileStat[] {
  if (!isMutationTool(tool)) {
    return [];
  }

  const name = tool.tool_name.toLowerCase();
  const isWrite = name === "write" || name === "write_file";
  const input = tool.input ?? {};
  const result = asRecord(tool.tool_use_result);
  const resultDetails = asRecord(result.details);

  // Some providers return a complete patch string on the tool result. Prefer it because it can
  // contain the exact path and handles multiple hunks without depending on tool argument names.
  // Pi RPC nests this data under `details`, while Claude returns it at the top level.
  const patchText =
    firstString(result, ["diff", "patch", "unified_diff", "unifiedDiff"]) ??
    firstString(resultDetails, ["diff", "patch", "unified_diff", "unifiedDiff"]);
  if (patchText) {
    const parsed = parseUnifiedDiffStats(patchText);
    if (parsed.files.length > 0) return parsed.files;
  }

  const path = extractMutationPath(tool, result);
  if (!path) return [];

  const patchCounts = extractStructuredPatchCounts(result);
  if (patchCounts) return [{ path, ...patchCounts }];

  const oldText =
    firstString(input, ["old_string", "oldText", "oldString", "old_text"]) ??
    firstString(result, ["old_string", "oldText", "oldString", "old_text"]);
  const newText =
    firstString(input, ["new_string", "newText", "newString", "new_text"]) ??
    firstString(result, ["new_string", "newText", "newString", "new_text"]);
  if (oldText !== undefined || newText !== undefined) {
    return [{ path, ...countTextChange(oldText ?? "", newText ?? "") }];
  }

  // Pi's native Edit tool sends an array so one tool call can replace multiple blocks.
  // Prefer the result patch above when available; this is the fallback for older Pi versions.
  if (Array.isArray(input.edits)) {
    let insertions = 0;
    let deletions = 0;
    let found = false;
    for (const edit of input.edits) {
      const item = asRecord(edit);
      const oldValue = firstString(item, ["old_string", "oldText", "oldString", "old_text"]);
      const newValue = firstString(item, ["new_string", "newText", "newString", "new_text"]);
      if (oldValue === undefined && newValue === undefined) continue;
      const counts = countTextChange(oldValue ?? "", newValue ?? "");
      insertions += counts.insertions;
      deletions += counts.deletions;
      found = true;
    }
    if (found) return [{ path, insertions, deletions }];
  }

  // A new-file Write often has only its content. This reports the known additions and leaves
  // deletions at zero; overwrite operations use structuredPatch when the provider can provide it.
  const content = firstString(input, ["content", "contents", "new_content", "newContent"]);
  if (content !== undefined && isWrite) {
    return [{ path, ...countTextChange("", content) }];
  }

  return [];
}

function extractMutationPath(tool: BusToolItem, result: Record<string, unknown>): string | null {
  const path =
    firstString(result, ["filePath", "file_path", "path", "filename"]) ??
    firstString(tool.input, ["filePath", "file_path", "path", "filename", "notebook_path"]);
  return path?.trim() || null;
}

function extractStructuredPatchCounts(
  result: Record<string, unknown>,
): Pick<UnifiedDiffFileStat, "insertions" | "deletions"> | null {
  const added = asFiniteNumber(result._patchAdded);
  const removed = asFiniteNumber(result._patchRemoved);
  if (added !== null || removed !== null) {
    return { insertions: added ?? 0, deletions: removed ?? 0 };
  }

  const raw = result.structuredPatch;
  if (!Array.isArray(raw)) return null;

  let insertions = 0;
  let deletions = 0;
  for (const hunk of raw) {
    const lines = asRecord(hunk)?.lines;
    if (!Array.isArray(lines)) continue;
    for (const line of lines) {
      if (typeof line === "string" && line.startsWith("+")) insertions++;
      else if (typeof line === "string" && line.startsWith("-")) deletions++;
    }
  }
  return { insertions, deletions };
}

function countTextChange(
  oldText: string,
  newText: string,
): Pick<UnifiedDiffFileStat, "insertions" | "deletions"> {
  const patch = structuredPatch("", "", oldText, newText, "", "", { context: 3 });
  let insertions = 0;
  let deletions = 0;
  for (const hunk of patch.hunks) {
    for (const line of hunk.lines) {
      if (line.startsWith("+")) insertions++;
      else if (line.startsWith("-")) deletions++;
    }
  }
  return { insertions, deletions };
}

function mergeFileStats(files: UnifiedDiffFileStat[]): UnifiedDiffSummary {
  const merged: UnifiedDiffFileStat[] = [];
  const byPath = new Map<string, UnifiedDiffFileStat>();
  for (const file of files) {
    const existing = byPath.get(file.path);
    if (existing) {
      existing.insertions += file.insertions;
      existing.deletions += file.deletions;
    } else {
      const copy = { ...file };
      byPath.set(copy.path, copy);
      merged.push(copy);
    }
  }
  return {
    files: merged,
    totalInsertions: merged.reduce((sum, file) => sum + file.insertions, 0),
    totalDeletions: merged.reduce((sum, file) => sum + file.deletions, 0),
  };
}

function asRecord(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function firstString(record: Record<string, unknown>, keys: string[]): string | undefined {
  for (const key of keys) {
    if (typeof record[key] === "string") return record[key] as string;
  }
  return undefined;
}

function asFiniteNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.max(0, Math.round(value))
    : null;
}
