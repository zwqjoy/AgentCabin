/**
 * Utility functions for rendering tool inputs/outputs in the chat UI.
 */

/** Extract plain text from an array of content blocks (Anthropic format). */
export function extractTextFromBlocks(blocks: unknown[]): string {
  if (!Array.isArray(blocks)) return "";
  return blocks
    .filter((b): b is { type: "text"; text: string } => {
      return typeof b === "object" && b !== null && (b as Record<string, unknown>).type === "text";
    })
    .map((b) => b.text)
    .join("\n");
}

/** Extract display text from opaque tool output (handles string/object/array/null). */
export function extractOutputText(output: unknown): string {
  if (output == null) return "";
  if (typeof output === "string") return output;
  if (typeof output !== "object") return String(output);

  const obj = output as Record<string, unknown>;

  // Content blocks array (Anthropic API format)
  if (Array.isArray(obj.content)) {
    const text = extractTextFromBlocks(obj.content);
    if (text) return text;
  }
  // Direct content string
  if (typeof obj.content === "string" && obj.content) return obj.content;
  // Error fallback
  if (typeof obj.error === "string" && obj.error) return obj.error;
  // Array of content blocks at top level
  if (Array.isArray(output)) {
    const text = extractTextFromBlocks(output);
    if (text) return text;
  }
  // Last resort: JSON stringify
  try {
    return JSON.stringify(output);
  } catch {
    return "[unrenderable output]";
  }
}

/** Extract image content blocks (base64) from tool output, if any. */
export function extractImageBlocks(
  output: unknown,
): Array<{ type: "image"; source: { type: string; media_type: string; data: string } }> {
  if (output == null || typeof output !== "object") return [];
  const obj = output as Record<string, unknown>;
  const blocks = Array.isArray(obj.content) ? obj.content : Array.isArray(output) ? output : [];
  return blocks.filter(
    (b): b is { type: "image"; source: { type: string; media_type: string; data: string } } => {
      return typeof b === "object" && b !== null && (b as Record<string, unknown>).type === "image";
    },
  );
}

const EXT_LANG_MAP: Record<string, string> = {
  ts: "typescript",
  tsx: "typescript",
  js: "javascript",
  jsx: "javascript",
  py: "python",
  rs: "rust",
  go: "go",
  rb: "ruby",
  java: "java",
  kt: "kotlin",
  swift: "swift",
  c: "c",
  cpp: "cpp",
  h: "c",
  hpp: "cpp",
  cs: "csharp",
  css: "css",
  scss: "scss",
  html: "html",
  json: "json",
  yaml: "yaml",
  yml: "yaml",
  md: "markdown",
  sql: "sql",
  sh: "bash",
  bash: "bash",
  zsh: "bash",
  toml: "toml",
  xml: "xml",
  svelte: "html",
  vue: "html",
};

/** Map a file path's extension to a highlight.js language name. */
export function getLanguageFromPath(filePath: string): string {
  const dot = filePath.lastIndexOf(".");
  if (dot < 0) return "";
  const ext = filePath.slice(dot + 1).toLowerCase();
  return EXT_LANG_MAP[ext] ?? "";
}

const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif"]);

/** Check if a file path refers to an image type. */
export function isImagePath(filePath: string): boolean {
  const dot = filePath.lastIndexOf(".");
  if (dot < 0) return false;
  return IMAGE_EXTS.has(filePath.slice(dot + 1).toLowerCase());
}

/**
 * Extract structured data from tool.output for team tools (TaskList, etc.).
 * Handles string-wrapped JSON, content blocks, and direct arrays/objects.
 */
export function extractStructuredOutput(output: unknown): unknown {
  if (!output) return null;
  if (typeof output === "string") {
    try {
      return JSON.parse(output);
    } catch {
      return output;
    }
  }
  if (Array.isArray(output)) return output;
  const obj = output as Record<string, unknown>;
  if (obj.content != null) {
    if (typeof obj.content === "string") {
      try {
        return JSON.parse(obj.content);
      } catch {
        return obj.content;
      }
    }
    return obj.content;
  }
  return output;
}

const FRIENDLY_TOOL_NAMES: Record<string, string> = {
  Bash: "Run commands",
  Read: "Read files",
  Write: "Write files",
  Edit: "Edit files",
  Glob: "Find files",
  Grep: "Search content",
  WebFetch: "Fetch URLs",
  WebSearch: "Search web",
  // Upstream renamed the subagent tool Task → Agent; keep Task for legacy run replay.
  Agent: "Run sub-agent",
  Task: "Run sub-agent",
  NotebookEdit: "Edit notebook",
  PowerShell: "Run PowerShell",
  Monitor: "Monitor events",
  CronCreate: "Schedule task",
  CronList: "List scheduled",
  CronDelete: "Cancel scheduled",
  ScheduleWakeup: "Schedule wakeup",
};

/** Map a tool name to a human-readable description. Falls back to the original name. */
export function friendlyToolName(name: string): string {
  return FRIENDLY_TOOL_NAMES[name] ?? name;
}

export type ToolActivityKind = "read" | "search" | "command" | "edit" | "web" | "skill" | "other";

/** Group raw tool names into the small set of actions used by compact activity summaries. */
export function getToolActivityKind(toolName: string): ToolActivityKind {
  switch (toolName.toLowerCase()) {
    case "read":
    case "read_file":
      return "read";
    case "glob":
    case "grep":
    case "list_directory":
    case "search_files":
      return "search";
    case "bash":
    case "powershell":
    case "shell":
      return "command";
    case "edit":
    case "edit_file":
    case "write":
    case "write_file":
    case "notebookedit":
      return "edit";
    case "webfetch":
    case "websearch":
      return "web";
    case "skill":
      return "skill";
    default:
      return "other";
  }
}

/** Single source of truth for the subagent-spawn tool name.
 *  Upstream renamed Task → Agent; "Task" is kept only for replay of legacy stored runs. */
export function isSubagentTool(name: string): boolean {
  return name === "Agent" || name === "Task";
}

/**
 * Detect if a file path targets a Claude plan file (~/.claude/plans/*.md).
 * Matches both absolute paths (/.claude/plans/) and relative paths (.claude/plans/).
 */
export function isPlanFilePath(filePath: string): boolean {
  if (!filePath) return false;
  const normalized = filePath.replaceAll("\\", "/");
  return (
    (normalized.includes("/.claude/plans/") || normalized.startsWith(".claude/plans/")) &&
    normalized.endsWith(".md")
  );
}

/** Extract short plan name from a plan file path. Returns null if not a plan file. */
export function planFileName(filePath: string): string | null {
  if (!isPlanFilePath(filePath)) return null;
  const normalized = filePath.replaceAll("\\", "/");
  const name = normalized.split("/").pop()!.replace(/\.md$/, "");
  return name;
}

// ── Task (subagent) tool metadata extraction ──

export interface TaskToolMeta {
  subagentType: string;
  description?: string;
  model?: string;
  isolation?: string;
  prompt?: string;
}

/** Extract agent metadata from a Task tool's input object. Returns null if not a Task tool input. */
export function extractTaskToolMeta(input: unknown): TaskToolMeta | null {
  if (input == null || typeof input !== "object") return null;
  const obj = input as Record<string, unknown>;
  const subagentType = obj.subagent_type ?? obj.subagentType;
  if (typeof subagentType !== "string") return null;
  return {
    subagentType,
    description: typeof obj.description === "string" ? obj.description : undefined,
    model: typeof obj.model === "string" ? obj.model : undefined,
    isolation: typeof obj.isolation === "string" ? obj.isolation : undefined,
    prompt:
      typeof obj.prompt === "string"
        ? obj.prompt.length > 200
          ? obj.prompt.slice(0, 200) + "…"
          : obj.prompt
        : undefined,
  };
}

// ── Batch / subagent status helpers ──

import type { BusToolItem } from "$lib/types";
import { dbg } from "$lib/utils/debug";

/** Tool is in a terminal state — no further status changes expected. */
export function isToolTerminal(status: BusToolItem["status"]): boolean {
  return (
    status === "success" ||
    status === "error" ||
    status === "denied" ||
    status === "permission_denied"
  );
}

/** Tool is actively working or awaiting interaction. */
export function isToolActive(status: BusToolItem["status"]): boolean {
  return status === "running" || status === "ask_pending" || status === "permission_prompt";
}

/** Whether a tool's subTimeline should be visible by default (no user override).
 *  All tools with subTimelines auto-collapse when in terminal state. */
export function shouldShowSubTimeline(
  status: BusToolItem["status"],
  hasSubTimeline: boolean,
): boolean {
  if (!hasSubTimeline) return false;
  return !isToolTerminal(status);
}

/** Aggregate batch tool statuses in a single pass. */
export function aggregateBatchStatus(tools: BusToolItem[]): {
  completed: number;
  failed: number;
  running: number;
  total: number;
} {
  let completed = 0,
    failed = 0,
    running = 0;
  for (const t of tools) {
    if (t.status === "success") completed++;
    else if (isToolTerminal(t.status)) failed++;
    else if (isToolActive(t.status)) running++;
  }
  return { completed, failed, running, total: tools.length };
}

/**
 * Pi may emit an empty completion or a literal ellipsis as an assistant message
 * between tool calls. It is useful as a transport/progress signal, but it has
 * no user-facing content and should not break a visual tool burst or render a
 * full assistant row in the chat transcript.
 */
export function isToolChatterAssistant(entry: {
  kind: string;
  content?: string;
  thinkingText?: string;
}): boolean {
  if (entry.kind !== "assistant" || entry.thinkingText?.trim()) return false;
  const content = entry.content?.trim() ?? "";
  return content.length === 0 || /^(?:\.{3,}|…+)$/.test(content);
}

/** Whether an assistant message is progress narration immediately followed by a tool call. */
export function isToolProgressAssistant(
  entry: { kind: string; content?: string; thinkingText?: string },
  next?: { kind: string },
): boolean {
  if (entry.kind !== "assistant" || next?.kind !== "tool") return false;
  if (isToolChatterAssistant(entry)) return false;
  return Boolean(entry.content?.trim() || entry.thinkingText?.trim());
}

/**
 * Whether an assistant entry is part of the tool-driven progress phase of its turn.
 *
 * A streamed agent can emit several complete narration messages before and between
 * tool calls. Looking only at the immediately following entry misses messages when
 * the reducer inserts another progress entry or a non-visual event between them.
 * Stop at the next user message so a later turn cannot reclassify a final answer.
 */
export function isToolProgressAssistantAt(
  timeline: Array<{ kind: string; content?: string; thinkingText?: string }>,
  index: number,
): boolean {
  const entry = timeline[index];
  if (
    !entry ||
    entry.kind !== "assistant" ||
    isToolChatterAssistant(entry) ||
    !(entry.content?.trim() || entry.thinkingText?.trim())
  ) {
    return false;
  }

  for (let i = index + 1; i < timeline.length; i++) {
    const next = timeline[i];
    if (next.kind === "user") return false;
    if (next.kind === "tool") return true;
  }
  return false;
}

export interface ToolProgressGroup {
  startIndex: number;
  assistantIndices: number[];
  entries: Array<{ content: string; thinkingText?: string }>;
}

/** Group tool-driven assistant narration by user turn for the chat transcript. */
export function detectToolProgressGroups(
  timeline: Array<{ kind: string; content?: string; thinkingText?: string }>,
): Map<number, ToolProgressGroup> {
  const groups = new Map<number, ToolProgressGroup>();
  let turnStart = 0;

  const flushTurn = (turnEnd: number) => {
    const assistantIndices: number[] = [];
    for (let i = turnStart; i < turnEnd; i++) {
      if (isToolProgressAssistantAt(timeline, i)) assistantIndices.push(i);
    }
    if (assistantIndices.length === 0) return;

    const entries = assistantIndices.map((index) => ({
      content: timeline[index].content?.trim() ?? "",
      ...(timeline[index].thinkingText?.trim()
        ? { thinkingText: timeline[index].thinkingText!.trim() }
        : {}),
    }));
    groups.set(assistantIndices[0], {
      startIndex: assistantIndices[0],
      assistantIndices,
      entries,
    });
  };

  for (let i = 0; i <= timeline.length; i++) {
    if (i === timeline.length || timeline[i].kind === "user") {
      flushTurn(i);
      turnStart = i + 1;
    }
  }
  return groups;
}

/** Detect consecutive runs of Task tools (≥3) in a timeline for batch progress display.
 *  Returns Map<startIndex, BusToolItem[]>. */
export function detectBatchGroups(
  timeline: Array<{ kind: string; tool?: BusToolItem }>,
): Map<number, BusToolItem[]> {
  const groups = new Map<number, BusToolItem[]>();
  let i = 0;
  while (i < timeline.length) {
    const entry = timeline[i];
    if (entry.kind === "tool" && isSubagentTool(entry.tool?.tool_name ?? "")) {
      const start = i;
      const tools: BusToolItem[] = [];
      while (
        i < timeline.length &&
        timeline[i].kind === "tool" &&
        isSubagentTool(timeline[i].tool?.tool_name ?? "")
      ) {
        tools.push(timeline[i].tool!);
        i++;
      }
      if (tools.length >= 3) groups.set(start, tools);
    } else {
      i++;
    }
  }
  return groups;
}

// ── Tool Burst Collapse ──

export interface ToolBurst {
  /** Stable key: first tool's tool_use_id (survives timeline index shifts). */
  key: string;
  startIndex: number;
  endIndex: number; // inclusive
  tools: BusToolItem[];
  /** Per-tool_name count summary, ordered by first appearance. */
  summary: Array<{ toolName: string; count: number }>;
  stats: { completed: number; failed: number; running: number; total: number };
}

// Agent/Task (subagent) excluded — they own a subTimeline and must not collapse into a burst.
const BURST_EXCLUDE = new Set([
  "Agent",
  "Task",
  "AskUserQuestion",
  "ExitPlanMode",
  "EnterPlanMode",
]);

/**
 * Detect "tool burst" segments: tool entries (regardless of tool_name) in the timeline,
 * optionally separated by empty/ellipsis assistant chatter. Task tools (handled
 * by BatchProgressBar) and interactive tools are excluded. Returns Map<startIndex, ToolBurst>.
 */
export function detectToolBursts(
  timeline: Array<{
    kind: string;
    tool?: BusToolItem;
    content?: string;
    thinkingText?: string;
  }>,
  minSize = 3,
): Map<number, ToolBurst> {
  const bursts = new Map<number, ToolBurst>();
  let i = 0;
  while (i < timeline.length) {
    const entry = timeline[i];
    if (entry.kind === "tool" && entry.tool && !BURST_EXCLUDE.has(entry.tool.tool_name)) {
      const start = i;
      const tools: BusToolItem[] = [];
      let lastToolIndex = i;
      while (i < timeline.length) {
        const current = timeline[i];
        if (current.kind === "tool" && current.tool && !BURST_EXCLUDE.has(current.tool.tool_name)) {
          tools.push(current.tool);
          lastToolIndex = i;
          i++;
          continue;
        }
        // Empty/ellipsis chatter has no user-facing content; keep it transparent for grouping.
        // Non-empty progress narration is a visual boundary and must remain outside the burst.
        if (tools.length > 0 && isToolChatterAssistant(current)) {
          i++;
          continue;
        }
        break;
      }
      // Skip burst at index 0 — may be truncated by renderLimit, key would be unstable
      if (tools.length >= minSize && start > 0) {
        const seen = new Map<string, number>();
        for (const t of tools) {
          seen.set(t.tool_name, (seen.get(t.tool_name) ?? 0) + 1);
        }
        const summary = Array.from(seen, ([toolName, count]) => ({ toolName, count }));
        bursts.set(start, {
          key: tools[0].tool_use_id,
          startIndex: start,
          endIndex: lastToolIndex,
          tools,
          summary,
          stats: aggregateBatchStatus(tools),
        });
      }
    } else {
      i++;
    }
  }
  return bursts;
}

/** Extract the /.claude/plans/<name>.md suffix from a plan file path.
 *  Returns null if not a plan file. Works for both absolute and relative paths. */
export function planFileSuffix(filePath: string): string | null {
  if (!filePath) return null;
  const normalized = filePath.replaceAll("\\", "/");
  const idx = normalized.lastIndexOf("/.claude/plans/");
  if (idx >= 0 && normalized.endsWith(".md")) return normalized.slice(idx);
  if (normalized.startsWith(".claude/plans/") && normalized.endsWith(".md"))
    return "/" + normalized;
  return null;
}

/** Flatten timeline entries, inlining subTimeline tool entries from Agent/subagent
 *  tools so that Write/Edit operations inside subagents are visible to plan extraction. */
function flattenToolEntries(
  timeline: Array<{
    kind: string;
    tool?: BusToolItem;
    subTimeline?: Array<{
      kind: string;
      tool?: BusToolItem;
      subTimeline?: Array<{ kind: string; tool?: BusToolItem }>;
    }>;
  }>,
  endIndex: number,
): Array<{ kind: string; tool?: BusToolItem }> {
  const result: Array<{ kind: string; tool?: BusToolItem }> = [];
  for (let i = 0; i < endIndex; i++) {
    const entry = timeline[i];
    result.push(entry);
    // Inline subTimeline tool entries (from Agent/subagent tools)
    if (entry.kind === "tool" && entry.subTimeline) {
      for (const sub of entry.subTimeline) {
        if (sub.kind === "tool") result.push(sub);
        // Recurse one more level for nested subagents
        if (sub.kind === "tool" && sub.subTimeline) {
          for (const subsub of sub.subTimeline) {
            if (subsub.kind === "tool") result.push(subsub);
          }
        }
      }
    }
  }
  return result;
}

/** Extract final plan content from timeline entries before a given index.
 *  Finds the latest successful Write to a plan file, then applies
 *  subsequent successful Edits to the same file.
 *  Searches inside Agent/subagent subTimeline as well.
 *  Stops at any prior ExitPlanMode to avoid crossing plan rounds. */
export function extractPlanContent(
  timeline: Array<{
    kind: string;
    tool?: BusToolItem;
    subTimeline?: Array<{
      kind: string;
      tool?: BusToolItem;
      subTimeline?: Array<{ kind: string; tool?: BusToolItem }>;
    }>;
  }>,
  beforeIndex: number,
): { content: string; fileName: string } | null {
  // Flatten timeline: inline subTimeline entries so Write/Edit inside agents are visible
  const flat = flattenToolEntries(timeline, beforeIndex);

  // 1. Search backwards for latest successful plan Write, stop at completed ExitPlanMode
  let writeIndex = -1;
  let baseContent: string | null = null;
  let baseSuffix: string | null = null;
  let baseName: string | null = null;

  for (let i = flat.length - 1; i >= 0; i--) {
    const entry = flat[i];
    if (entry.kind !== "tool" || !entry.tool) continue;

    // Boundary: completed ExitPlanMode (previous round)
    // Use its tool_use_result.plan as base content if available (cross-round editing)
    if (entry.tool.tool_name === "ExitPlanMode" && entry.tool.status === "success") {
      const result = entry.tool.tool_use_result as { plan?: string; filePath?: string } | undefined;
      if (result?.plan && typeof result.plan === "string") {
        const fp = result.filePath ?? "";
        writeIndex = i;
        baseContent = result.plan;
        baseSuffix = isPlanFilePath(fp) ? planFileSuffix(fp) : null;
        baseName = isPlanFilePath(fp) ? planFileName(fp) : "plan";
        dbg("plan", "extractPlanContent: using plan from completed ExitPlanMode", {
          i,
          name: baseName,
        });
      } else {
        dbg("plan", "extractPlanContent: hit completed ExitPlanMode without plan content", { i });
      }
      break;
    }

    if (entry.tool.status !== "success") continue;
    const fp = String(entry.tool.input?.file_path ?? entry.tool.input?.path ?? "");
    if (!isPlanFilePath(fp)) continue;

    if (entry.tool.tool_name === "Write" && typeof entry.tool.input?.content === "string") {
      writeIndex = i;
      baseContent = entry.tool.input.content as string;
      baseSuffix = planFileSuffix(fp);
      baseName = planFileName(fp);
      dbg("plan", "extractPlanContent: found base Write", { i, name: baseName });
      break;
    }
  }

  if (writeIndex < 0 || !baseContent || !baseName) return null;

  // 2. Apply subsequent successful Edits to the same plan file
  let content = baseContent;
  for (let i = writeIndex + 1; i < flat.length; i++) {
    const entry = flat[i];
    if (entry.kind !== "tool" || !entry.tool) continue;
    if (entry.tool.status !== "success") continue;
    const fp = String(entry.tool.input?.file_path ?? entry.tool.input?.path ?? "");
    if (!isPlanFilePath(fp)) continue;
    // Compare suffix path (/.claude/plans/<name>.md) for cross-format compatibility
    // When baseSuffix is null (e.g. from ExitPlanMode without filePath), accept any plan file
    if (baseSuffix && planFileSuffix(fp) !== baseSuffix) continue;

    if (entry.tool.tool_name === "Write" && typeof entry.tool.input?.content === "string") {
      content = entry.tool.input.content as string;
      dbg("plan", "extractPlanContent: overwrite by later Write", { i });
    } else if (
      entry.tool.tool_name === "Edit" &&
      typeof entry.tool.input?.old_string === "string"
    ) {
      const oldStr = entry.tool.input.old_string as string;
      const newStr = (entry.tool.input?.new_string as string) ?? "";
      if (content.includes(oldStr)) {
        content = content.replace(oldStr, newStr);
        dbg("plan", "extractPlanContent: applied Edit", { i });
      } else {
        dbg("plan", "extractPlanContent: Edit old_string not found, skipped", { i });
      }
    }
  }

  return { content, fileName: baseName };
}

/** Apply forward Edits to an approved plan's content.
 *  Starting after the given index, scan forward for successful
 *  Write/Edit to the same plan file and apply them.
 *  This keeps the approved plan card up-to-date when the plan file
 *  is edited after approval in the same session. */
export function applyPlanEditsForward(
  timeline: Array<{
    kind: string;
    tool?: BusToolItem;
    subTimeline?: Array<{
      kind: string;
      tool?: BusToolItem;
      subTimeline?: Array<{ kind: string; tool?: BusToolItem }>;
    }>;
  }>,
  afterIndex: number,
  basePlan: string,
  planFilePath?: string,
): string {
  const baseSuffix =
    planFilePath && isPlanFilePath(planFilePath) ? planFileSuffix(planFilePath) : null;
  let content = basePlan;

  function applyTool(tool: BusToolItem): void {
    if (tool.status !== "success") return;
    const fp = String(tool.input?.file_path ?? tool.input?.path ?? "");
    if (!isPlanFilePath(fp)) return;
    if (baseSuffix && planFileSuffix(fp) !== baseSuffix) return;

    if (tool.tool_name === "Write" && typeof tool.input?.content === "string") {
      content = tool.input.content as string;
      dbg("plan", "applyPlanEditsForward: overwrite by later Write");
    } else if (tool.tool_name === "Edit" && typeof tool.input?.old_string === "string") {
      const oldStr = tool.input.old_string as string;
      const newStr = (tool.input?.new_string as string) ?? "";
      if (content.includes(oldStr)) {
        content = content.replace(oldStr, newStr);
        dbg("plan", "applyPlanEditsForward: applied Edit");
      }
    }
  }

  for (let i = afterIndex + 1; i < timeline.length; i++) {
    const entry = timeline[i];
    if (entry.kind !== "tool" || !entry.tool) continue;
    applyTool(entry.tool);
    // Also check subTimeline (agent/subagent)
    if (entry.subTimeline) {
      for (const sub of entry.subTimeline) {
        if (sub.kind === "tool" && sub.tool) applyTool(sub.tool);
        if (sub.subTimeline) {
          for (const subsub of sub.subTimeline) {
            if (subsub.kind === "tool" && subsub.tool) applyTool(subsub.tool);
          }
        }
      }
    }
  }

  return content;
}

// ── Tool Render Level ──

/** Tools whose output is the primary content (auto-expand, accent border). */
const LEVEL_2_TOOLS = new Set(["Bash", "bash", "Edit", "edit_file", "Write", "write_file"]);

/**
 * Determine the render level for a tool card.
 * Level 1 = one-liner (info tools), Level 2 = inline content (output tools), Level 3 = interactive card.
 */
export function getToolRenderLevel(toolName: string, status: BusToolItem["status"]): 1 | 2 | 3 {
  // AskUserQuestion is always Level 3 (all states: active, done, denied)
  if (toolName === "AskUserQuestion") return 3;
  // Interactive statuses: user must approve/deny (also covers ExitPlanMode plan approval)
  if (status === "permission_prompt") return 3;
  // Output-focused tools (including cross-provider aliases)
  if (LEVEL_2_TOOLS.has(toolName)) return 2;
  // Everything else
  return 1;
}

// ── Permission / tool input utilities ──

import type { PermissionSuggestion } from "$lib/types";

/** Tool names whose tool_end updates scheduledTasks state. */
export const SCHEDULING_TOOLS = new Set(["CronCreate", "CronDelete"]);

/** First defined string value at any of the keys in `input`. */
function pickString(
  input: Record<string, unknown> | undefined,
  keys: readonly string[],
): string | undefined {
  if (!input) return undefined;
  for (const k of keys) {
    const v = input[k];
    if (typeof v === "string" && v.length > 0) return v;
  }
  return undefined;
}

/** Cron-expression field on CronCreate input (CLI naming has drifted historically). */
export function pickSchedule(input: Record<string, unknown> | undefined): string | undefined {
  return pickString(input, ["cron", "schedule", "expression"]);
}

/** Cron-task id field on CronDelete input (CLI naming has drifted historically). */
export function pickCronId(input: Record<string, unknown> | undefined): string | undefined {
  return pickString(input, ["id", "task_id", "cronId"]);
}

/** Extract a human-readable detail string from tool input (file path, command, pattern, etc.). */
export function getToolDetail(input: Record<string, unknown> | undefined): string {
  if (!input || Object.keys(input).length === 0) return "";
  const schedule = pickSchedule(input);
  if (schedule) {
    const prompt = typeof input.prompt === "string" ? input.prompt : "";
    return prompt ? `${schedule} — ${prompt}` : schedule;
  }
  return (
    (input.file_path as string) ??
    (input.notebook_path as string) ??
    (input.path as string) ??
    (input.command as string) ??
    (input.pattern as string) ??
    (input.query as string) ??
    (input.url as string) ??
    (input.description as string) ??
    (input.prompt as string) ??
    (input.team_name as string) ??
    (input.subject as string) ??
    (input.taskId != null || input.task_id != null
      ? `#${input.taskId ?? input.task_id}`
      : undefined) ??
    (input.skill as string) ??
    (input.recipient as string) ??
    ""
  );
}

/** Format a permission suggestion label for display.
 *  Requires a `t` translation function since this runs outside Svelte component context. */
export function formatSuggestionLabel(
  s: PermissionSuggestion,
  t: (key: string, params?: Record<string, string>) => string,
): string {
  if (s.type === "addRules" && s.rules?.length && s.behavior === "allow") {
    return t("inline_alwaysAllow") + ` ${s.rules[0]}`;
  }
  if (s.type === "setMode" && s.mode) {
    return t("inline_switchToMode", { mode: s.mode });
  }
  if (s.type === "addDirectories" && s.directories?.length) {
    return t("inline_addDirectory", { dir: s.directories[0] });
  }
  if (s.type === "additionalContext") {
    return t("inline_applyHookContext");
  }
  return `Apply: ${s.type}`;
}

/** Copy text to clipboard with legacy fallback for Tauri WebView. */
export async function copyToClipboard(text: string): Promise<void> {
  if (navigator.clipboard) {
    await navigator.clipboard.writeText(text);
  } else {
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.style.position = "fixed";
    textarea.style.opacity = "0";
    document.body.appendChild(textarea);
    textarea.select();
    document.execCommand("copy");
    document.body.removeChild(textarea);
  }
}

/**
 * Format duration in Codex style:
 * < 60s: "用时 12s"
 * < 60m: "用时 14m 8s" (or "用时 14m" if seconds === 0)
 * >= 60m: "用时 1h 14m 8s"
 */
export function formatCodexDuration(ms: number): string {
  const totalSeconds = Math.max(1, Math.round(ms / 1000));
  if (totalSeconds < 60) return `用时 ${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  if (minutes < 60) {
    return seconds > 0 ? `用时 ${minutes}m ${seconds}s` : `用时 ${minutes}m`;
  }
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  if (remainingMinutes === 0 && seconds === 0) return `用时 ${hours}h`;
  if (seconds === 0) return `用时 ${hours}h ${remainingMinutes}m`;
  return `用时 ${hours}h ${remainingMinutes}m ${seconds}s`;
}

export type CodexToolActionKind =
  | "skill"
  | "edit"
  | "read"
  | "command"
  | "search"
  | "web"
  | "agent"
  | "artifact"
  | "other";

export function getToolActionCategory(toolName: string): CodexToolActionKind {
  const base = (toolName.split("__").pop() || toolName).toLowerCase();
  if (base.includes("artifact") || base.includes("deliver") || base.includes("validate_artifact")) {
    return "artifact";
  }
  if (
    base.includes("skill") ||
    base.includes("load_tool") ||
    base.includes("loadtool") ||
    base.includes("tool_load")
  ) {
    return "skill";
  }
  if (
    base.includes("edit") ||
    base.includes("write") ||
    base.includes("replace") ||
    base.includes("notebookedit") ||
    base.includes("patch")
  ) {
    return "edit";
  }
  if (
    base.includes("read") ||
    base.includes("cat") ||
    base.includes("view_file") ||
    base.includes("viewfile")
  ) {
    return "read";
  }
  if (
    base.includes("grep") ||
    base.includes("glob") ||
    base.includes("find") ||
    base.includes("search")
  ) {
    return "search";
  }
  if (
    base.includes("bash") ||
    base.includes("command") ||
    base.includes("exec") ||
    base.includes("terminal") ||
    base.includes("run")
  ) {
    return "command";
  }
  if (
    base.includes("web") ||
    base.includes("fetch") ||
    base.includes("browse") ||
    base.includes("http")
  ) {
    return "web";
  }
  if (
    base.includes("agent") ||
    base.includes("task") ||
    base.includes("subagent") ||
    base.includes("delegate")
  ) {
    return "agent";
  }
  return "other";
}

export interface ToolGroupSemanticSummary {
  iconKind: "wrench" | "pencil" | "search" | "book" | "globe" | "bot" | "terminal" | "box";
  label: string;
  activeLabel: string;
  hasActive: boolean;
  hasPermissionOrAsk: boolean;
  filesRead: string[];
  filesEdited: string[];
  commands: string[];
  categories: CodexToolActionKind[];
}

/**
 * Generate Codex-aligned semantic action summary for a group of consecutive tools.
 * e.g.:
 * - "加载了工具读取文件运行了命令已搜索网页" (matches Codex screenshot 4)
 * - "已读取文件"
 * - "已读取文件运行了命令"
 * - "编辑了文件"
 * - "编辑了文件读取文件运行了命令"
 */
export function getToolGroupSemanticSummary(tools: BusToolItem[]): ToolGroupSemanticSummary {
  const categories: CodexToolActionKind[] = [];
  const filesReadSet = new Set<string>();
  const filesEditedSet = new Set<string>();
  const commandsList: string[] = [];
  let hasActive = false;
  let hasPermissionOrAsk = false;

  for (const t of tools) {
    if (!isToolTerminal(t.status)) hasActive = true;
    if (
      t.status === "permission_prompt" ||
      t.status === "ask_pending" ||
      t.tool_name === "AskUserQuestion"
    ) {
      hasPermissionOrAsk = true;
    }

    const cat = getToolActionCategory(t.tool_name);
    if (!categories.includes(cat)) categories.push(cat);

    const input = (t.input ?? t.tool_input ?? {}) as Record<string, unknown>;
    const path = (input.file_path ??
      input.path ??
      input.notebook_path ??
      input.filePath ??
      input.filename ??
      input.AbsolutePath ??
      input.TargetFile ??
      input.target_file) as string | undefined;
    const cmd = (input.command ??
      input.cmd ??
      input.CommandLine ??
      input.commandLine ??
      input.instruction ??
      input.exec) as string | undefined;

    if (cat === "read" && path) filesReadSet.add(path);
    if (cat === "edit" && path) filesEditedSet.add(path);
    if (cat === "command" && cmd) commandsList.push(cmd);
  }

  // Determine icon priority: skill > edit > command/search (order-preserving) > read > web > agent > terminal
  let iconKind: ToolGroupSemanticSummary["iconKind"] = "terminal";
  if (categories[0] === "skill") {
    iconKind = "wrench";
  } else if (categories.includes("edit")) {
    iconKind = "pencil";
  } else if (categories.includes("command") && !categories.includes("search")) {
    iconKind = "terminal";
  } else if (categories.includes("search") && !categories.includes("command")) {
    iconKind = "search";
  } else if (categories.includes("search") && categories.includes("command")) {
    iconKind = categories.indexOf("search") < categories.indexOf("command") ? "search" : "terminal";
  } else if (categories.includes("read")) {
    iconKind = "book";
  } else if (categories.includes("web")) {
    iconKind = "globe";
  } else if (categories.includes("agent")) {
    iconKind = "bot";
  } else if (categories.includes("skill")) {
    iconKind = "wrench";
  }

  // Preserve appearance order if skill is first, else standard priority
  const orderedCats: CodexToolActionKind[] = [];
  for (const c of categories) {
    if (!orderedCats.includes(c)) orderedCats.push(c);
  }

  const FIRST_PHRASES: Record<CodexToolActionKind, string> = {
    skill: "加载了工具",
    edit: "编辑了文件",
    read: "已读取文件",
    search: "搜索了代码",
    command: "运行了命令",
    web: "已搜索网页",
    agent: "委派了子代理",
    artifact: "登记了成果",
    other: "执行了操作",
  };

  const SUBSEQUENT_PHRASES: Record<CodexToolActionKind, string> = {
    skill: "加载工具",
    edit: "编辑文件",
    read: "读取文件",
    search: "搜索代码",
    command: "运行了命令",
    web: "已搜索网页",
    agent: "委派子代理",
    artifact: "登记成果",
    other: "执行操作",
  };

  const ACTIVE_PHRASES: Record<CodexToolActionKind, string> = {
    skill: "正在加载工具...",
    edit: "正在编辑文件...",
    read: "正在读取文件...",
    search: "正在搜索代码...",
    command: "正在执行命令...",
    web: "正在访问网页...",
    agent: "正在等待子代理...",
    artifact: "正在登记成果...",
    other: "正在执行操作...",
  };

  let label = "";
  if (orderedCats.length === 0) {
    label = "执行了操作";
  } else {
    label = FIRST_PHRASES[orderedCats[0]];
    for (let i = 1; i < orderedCats.length; i++) {
      label += SUBSEQUENT_PHRASES[orderedCats[i]];
    }
  }

  // Active label: show what is currently running
  const activeTool = [...tools].reverse().find((t) => !isToolTerminal(t.status));
  const activeCat = activeTool
    ? getToolActionCategory(activeTool.tool_name)
    : (orderedCats[0] ?? "other");
  const activeLabel = ACTIVE_PHRASES[activeCat];

  return {
    iconKind,
    label,
    activeLabel,
    hasActive,
    hasPermissionOrAsk,
    filesRead: Array.from(filesReadSet),
    filesEdited: Array.from(filesEditedSet),
    commands: commandsList,
    categories: orderedCats,
  };
}
