import type { BusToolItem, TimelineEntry } from "$lib/types";
import {
  extractOutputText,
  getToolActionCategory,
  friendlyToolName,
  getToolDetail,
  type CodexToolActionKind,
} from "$lib/utils/tool-rendering";
import { currentLocale as getCurrentLocale } from "$lib/i18n/index.svelte";

export type ActivityCategory = CodexToolActionKind;

export type ActivityState = "pending" | "running" | "success" | "failed";

export interface ActivityDetail {
  command?: string;
  exitCode?: number;
  isError?: boolean;
  filePath?: string;
  diff?: string;
  pattern?: string;
  path?: string;
  matchesCount?: number;
  matches?: string[];
  prompt?: string;
  subagentType?: string;
  output?: string;
  durationMs?: number;
  subTimeline?: TimelineEntry[];
}

export interface ActivityItem {
  id: string;
  tool: BusToolItem;
  category: ActivityCategory;
  state: ActivityState;
  iconKind: "terminal" | "book" | "pencil" | "search" | "globe" | "bot" | "wrench" | "box";
  verb: string;
  target: string;
  detail: ActivityDetail;
}

/** Check if a tool requires user interaction (permission prompt, question, plan approval). */
export function isInteractionTool(tool: BusToolItem): boolean {
  return (
    tool.status === "permission_prompt" ||
    tool.status === "ask_pending" ||
    tool.tool_name === "AskUserQuestion" ||
    tool.tool_name === "ExitPlanMode"
  );
}

/** Determine normalized activity state. */
export function getActivityState(tool: BusToolItem): ActivityState {
  if (tool.status === "running") return "running";
  if (tool.status === "error" || tool.status === "denied" || tool.status === "permission_denied") {
    return "failed";
  }
  if (tool.status === "success") return "success";
  return "pending";
}

/** Extract exit code if present in result/output or input. */
export function extractExitCode(
  outputObj: Record<string, unknown> | undefined,
  outputStr: string,
): number | undefined {
  if (outputObj && typeof outputObj.exit_code === "number") return outputObj.exit_code;
  if (outputObj && typeof outputObj.exitCode === "number") return outputObj.exitCode;
  if (outputObj && typeof outputObj.code === "number") return outputObj.code;

  // Regex check on text: e.g. "exit code 1" or "exited with code 1"
  const m = outputStr.match(/exit(?:ed)?\s+(?:with\s+)?(?:status|code)\s+([0-9]+)/i);
  if (m) {
    const parsed = parseInt(m[1], 10);
    if (!Number.isNaN(parsed)) return parsed;
  }
  return undefined;
}

/** Generate unified diff snippet from edit input or output if available. */
export function extractEditDiff(
  input: Record<string, unknown>,
  output: Record<string, unknown> | undefined,
): string | undefined {
  if (output && typeof output.diff === "string" && output.diff.trim()) {
    return output.diff.trim();
  }
  if (input && typeof input.diff === "string" && input.diff.trim()) {
    return input.diff.trim();
  }

  const oldStr = (input.old_string ?? input.oldText ?? input.TargetContent) as string | undefined;
  const newStr = (input.new_string ?? input.newText ?? input.ReplacementContent) as
    | string
    | undefined;

  if (typeof oldStr === "string" && typeof newStr === "string") {
    const oldLines = oldStr.split("\n").map((l) => `- ${l}`);
    const newLines = newStr.split("\n").map((l) => `+ ${l}`);
    return [...oldLines, ...newLines].join("\n");
  }

  return undefined;
}

/** Extract search matches list from search output. */
export function extractSearchMatches(outputStr: string): { count: number; matches: string[] } {
  if (!outputStr) return { count: 0, matches: [] };

  const lines = outputStr
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
  const matchedLines: string[] = [];

  for (const line of lines) {
    // ripgrep match pattern: filename:line:content or JSON
    if (line.includes(":") && !line.startsWith("{") && !line.startsWith("[")) {
      matchedLines.push(line);
    } else if (!line.startsWith("{") && line.length < 200) {
      matchedLines.push(line);
    }
  }

  return {
    count: matchedLines.length || (lines.length > 0 ? lines.length : 0),
    matches: matchedLines.slice(0, 30),
  };
}

/** Map category + state to clean verb (avoids repeating "已" in Chinese). */
export function getActivityVerb(
  category: ActivityCategory,
  state: ActivityState,
  isEn: boolean,
): string {
  if (isEn) {
    if (state === "running" || state === "pending") {
      switch (category) {
        case "read":
          return "Reading";
        case "edit":
          return "Editing";
        case "search":
          return "Searching";
        case "command":
          return "Running";
        case "web":
          return "Searching web";
        case "agent":
          return "Sub-agent working";
        case "skill":
          return "Loading skill";
        case "artifact":
          return "Registering artifact";
        default:
          return "Running";
      }
    }
    if (state === "failed") {
      switch (category) {
        case "read":
          return "Failed to read";
        case "edit":
          return "Failed to edit";
        case "search":
          return "Search failed";
        case "command":
          return "Failed to run";
        case "web":
          return "Web request failed";
        case "agent":
          return "Sub-agent failed";
        case "skill":
          return "Failed to load skill";
        case "artifact":
          return "Failed to register artifact";
        default:
          return "Failed";
      }
    }
    // Success
    switch (category) {
      case "read":
        return "Read";
      case "edit":
        return "Edited";
      case "search":
        return "Searched";
      case "command":
        return "Ran";
      case "web":
        return "Searched web for";
      case "agent":
        return "Delegated to sub-agent";
      case "skill":
        return "Loaded skill";
      case "artifact":
        return "Registered artifact";
      default:
        return "Ran";
    }
  }

  // Chinese mode: concise verbs, NO "已" flood
  if (state === "running" || state === "pending") {
    switch (category) {
      case "read":
        return "正在读取";
      case "edit":
        return "正在编辑";
      case "search":
        return "正在搜索";
      case "command":
        return "正在运行";
      case "web":
        return "正在访问网页";
      case "agent":
        return "子代理正在执行";
      case "skill":
        return "正在加载技能";
      case "artifact":
        return "正在登记成果";
      default:
        return "正在执行";
    }
  }
  if (state === "failed") {
    switch (category) {
      case "read":
        return "读取失败";
      case "edit":
        return "编辑失败";
      case "search":
        return "搜索失败";
      case "command":
        return "运行失败";
      case "web":
        return "访问失败";
      case "agent":
        return "子代理失败";
      case "skill":
        return "加载失败";
      case "artifact":
        return "登记失败";
      default:
        return "执行失败";
    }
  }
  // Success
  switch (category) {
    case "read":
      return "读取";
    case "edit":
      return "编辑";
    case "search":
      return "搜索";
    case "command":
      return "运行";
    case "web":
      return "访问网页";
    case "agent":
      return "委派子代理";
    case "skill":
      return "加载技能";
    case "artifact":
      return "登记成果";
    default:
      return "执行";
  }
}

/** Normalize any tool into a unified ActivityItem. */
export function normalizeToolActivity(
  tool: BusToolItem,
  extra?: {
    subTimeline?: TimelineEntry[];
    locale?: string;
  },
): ActivityItem {
  const input = (tool.input ?? tool.tool_input ?? {}) as Record<string, unknown>;
  const resultObj = (tool.output ?? tool.tool_use_result ?? {}) as Record<string, unknown>;
  const toolName = tool.tool_name || "";
  const category = getToolActionCategory(toolName);
  const state = getActivityState(tool);

  // Locale determination
  let currentLocale = extra?.locale;
  if (!currentLocale) {
    try {
      currentLocale = getCurrentLocale();
    } catch {
      currentLocale = "zh-CN";
    }
  }
  const isEn = Boolean(currentLocale && currentLocale.startsWith("en"));

  // Extract raw output string
  const outputStr = extractOutputText(tool.output ?? tool.tool_use_result);
  const exitCode = extractExitCode(resultObj, outputStr);

  const detail: ActivityDetail = {
    output: outputStr,
    durationMs: tool.duration_ms,
    subTimeline: extra?.subTimeline,
  };

  let target = "";
  let iconKind: ActivityItem["iconKind"] = "terminal";

  if (category === "command") {
    iconKind = "terminal";
    const rawCmd = (input.command ??
      input.cmd ??
      input.CommandLine ??
      input.commandLine ??
      input.instruction ??
      input.exec ??
      tool.summary ??
      getToolDetail(input) ??
      "") as string;
    const cmd = typeof rawCmd === "string" ? rawCmd.trim() : "";
    target = cmd || (toolName ? friendlyToolName(toolName) : isEn ? "command" : "命令");
    detail.command = cmd;
    detail.exitCode = exitCode;
    detail.isError = state === "failed" || (exitCode !== undefined && exitCode !== 0);
  } else if (category === "read") {
    iconKind = "book";
    const rawPath = (input.file_path ??
      input.path ??
      input.notebook_path ??
      input.filePath ??
      input.filename ??
      input.AbsolutePath ??
      input.target_file ??
      "") as string;
    const baseName = rawPath ? rawPath.split("/").pop() || rawPath : "";
    target = baseName || rawPath || (isEn ? "file" : "文件");
    detail.filePath = rawPath;
  } else if (category === "edit") {
    iconKind = "pencil";
    const rawPath = (input.file_path ??
      input.path ??
      input.notebook_path ??
      input.filePath ??
      input.TargetFile ??
      input.target_file ??
      input.filename ??
      "") as string;
    const baseName = rawPath ? rawPath.split("/").pop() || rawPath : "";
    target = baseName || rawPath || (isEn ? "file" : "文件");
    detail.filePath = rawPath;
    detail.diff = extractEditDiff(input, resultObj);
  } else if (category === "search") {
    iconKind = "search";
    const pattern = (input.pattern ??
      input.query ??
      input.Query ??
      input.Pattern ??
      input.regex ??
      input.search_term ??
      "") as string;
    const rawPath = (input.path ??
      input.SearchPath ??
      input.SearchDirectory ??
      input.directory ??
      input.dir ??
      "") as string;
    const scope = rawPath ? rawPath.split("/").pop() || rawPath : "";

    if (pattern && scope) {
      target = `"${pattern}" in ${scope}`;
    } else if (pattern) {
      target = `"${pattern}"`;
    } else if (scope) {
      target = scope;
    } else {
      target = isEn ? "codebase" : "代码";
    }

    const { count, matches } = extractSearchMatches(outputStr);
    detail.pattern = pattern;
    detail.path = rawPath;
    detail.matchesCount = count;
    detail.matches = matches;
  } else if (category === "web") {
    iconKind = "globe";
    const query = (input.query ?? input.url ?? input.Url ?? input.targetUrl ?? "") as string;
    target = query || (isEn ? "web" : "网页");
  } else if (category === "agent") {
    iconKind = "bot";
    const prompt = (input.prompt ??
      input.task ??
      input.Prompt ??
      input.subagent_type ??
      input.description ??
      "") as string;
    target = prompt
      ? prompt.length > 60
        ? prompt.slice(0, 60) + "…"
        : prompt
      : isEn
        ? "sub-agent"
        : "子代理";
    detail.prompt = prompt;
    detail.subagentType = (input.subagent_type ?? input.subagentType) as string | undefined;
  } else if (category === "skill") {
    iconKind = "wrench";
    const skillName = (input.skill ??
      input.name ??
      input.skill_name ??
      input.SkillName ??
      "") as string;
    target = skillName ? `${skillName}` : isEn ? "skill" : "技能";
  } else if (category === "artifact") {
    iconKind = "box";
    const title = (input.title ??
      input.name ??
      input.artifact_id ??
      input.path ??
      input.target ??
      "") as string;
    target = title || (isEn ? "artifact" : "成果");
    detail.path = (input.path ?? input.filePath ?? input.target) as string | undefined;
  } else {
    iconKind = "terminal";
    target =
      getToolDetail(input) || (toolName ? friendlyToolName(toolName) : isEn ? "action" : "操作");
  }

  const verb = getActivityVerb(category, state, isEn);

  return {
    id: tool.tool_use_id,
    tool,
    category,
    state,
    iconKind,
    verb,
    target,
    detail,
  };
}
