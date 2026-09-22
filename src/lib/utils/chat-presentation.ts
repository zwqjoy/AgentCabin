import type { BusToolItem, TimelineEntry, Attachment } from "$lib/types";
import {
  normalizeToolActivity,
  isInteractionTool,
  type ActivityItem,
} from "$lib/utils/tool-activity-adapter";
import { formatCodexDuration, isToolChatterAssistant } from "$lib/utils/tool-rendering";
import { currentLocale as getCurrentLocale } from "$lib/i18n/index.svelte";
import { extractTurnModifiedPaths, parseUnifiedDiffStats } from "$lib/utils/diff-stats";

export type { ActivityItem };

export type ChatProcessBlock =
  | {
      type: "reasoning";
      id: string;
      content: string;
      isStreaming: boolean;
      startedAt?: number;
      completedAt?: number;
    }
  | {
      type: "narration";
      id: string;
      content: string;
      isStreaming: boolean;
    }
  | {
      type: "activity-group";
      id: string;
      summaryLabel: string;
      iconKind: ActivityItem["iconKind"];
      activities: ActivityItem[];
      hasRunning: boolean;
      hasFailed: boolean;
    }
  | {
      type: "interaction";
      id: string;
      item: ChatInteractionItem;
      isPending: boolean;
    };

export interface ChatInteractionItem {
  id: string; // tool_use_id
  tool: BusToolItem;
  type: "permission" | "ask_user" | "exit_plan";
  subTimeline?: TimelineEntry[];
  planContent?: { content: string; fileName: string } | null;
}

export interface ChatPresentationTurn {
  id: string;
  turnIndex: number;
  userTimelineIndex?: number;
  userMessage?: {
    id: string;
    anchorId: string;
    content: string;
    timestamp: string;
    attachments?: Attachment[];
    cliUuid?: string;
  };
  durationMs: number;
  durationFormatted: string;
  isRunning: boolean;
  /** Chronologically interleaved process blocks (Reasoning -> Activities -> Interactions -> Reasoning) */
  processBlocks: ChatProcessBlock[];
  /** High priority interactive items requiring user input/approval */
  interactionBlocks: ChatInteractionItem[];
  /** Whether this turn currently has an active pending interaction requiring user response */
  hasPendingInteraction: boolean;
  /** Final assistant response */
  finalMessage?: {
    id: string;
    anchorId: string;
    content: string;
    timestamp: string;
    model?: string;
    isStreaming: boolean;
  };
  turnSummary?: {
    id: string;
    anchorId: string;
    diff: string;
    cwd: string;
    ts: string;
  };
  commandOutputs?: Array<{
    id: string;
    content: string;
    ts: string;
  }>;
  separators?: Array<{
    id: string;
    content: string;
  }>;
  hooks?: Array<{
    id: string;
    eventName: string;
    status: "running" | "success" | "failed";
    statusMessage?: string;
    durationMs?: number;
  }>;
  turnSemanticSummary: string;
  /** DSH-style live thought preview extracted from active streaming thinking line */
  activeThinkingPreview?: string;
  /** Active tool action currently running, if any */
  activeToolPreview?: string;
  /** Active narration preview extracted from live or latest narration line */
  activeNarrationPreview?: string;
  isCollapsed: boolean;
}

/**
 * Extract the latest meaningful thinking line from streaming or completed reasoning text.
 * Omits markdown formatting (e.g. asterisks, hashes, list prefixes).
 * Inspired by DeepSeek Harness (DSH) ReasoningRow latestLine logic.
 */
export function extractLatestThinkingLine(text: string): string {
  if (!text) return "";
  const visible = text.trimEnd();
  const newline = visible.lastIndexOf("\n");
  const line = newline === -1 ? visible : visible.slice(newline + 1);
  return line
    .replaceAll("**", "")
    .replaceAll("__", "")
    .replaceAll("`", "")
    .replace(/^[\s#>*\-–—]+/, "")
    .trim();
}

/**
 * Extract the latest meaningful narration line from streaming or completed narration text.
 * Omits markdown formatting (e.g. asterisks, hashes, list prefixes).
 */
export function extractLatestNarrationLine(text: string): string {
  if (!text) return "";
  const lines = text
    .split("\n")
    .map((l) =>
      l
        .replaceAll("**", "")
        .replaceAll("__", "")
        .replaceAll("`", "")
        .replace(/^[\s#>*\-–—]+/, "")
        .trim(),
    )
    .filter(Boolean);
  if (lines.length === 0) return "";
  return lines[lines.length - 1];
}

/** Formats a group of activities into a semantic summary label (e.g. "读取了 3 个文件并运行了 1 个命令"). */
export function formatActivityGroupSummary(
  activities: ActivityItem[],
  isEn: boolean,
): {
  label: string;
  iconKind: ActivityItem["iconKind"];
} {
  if (activities.length === 0) {
    return { label: isEn ? "Activities" : "执行操作", iconKind: "terminal" };
  }

  const counts: Record<string, number> = {
    read: 0,
    edit: 0,
    search: 0,
    command: 0,
    web: 0,
    agent: 0,
    skill: 0,
    artifact: 0,
    other: 0,
  };

  for (const act of activities) {
    counts[act.category] = (counts[act.category] || 0) + 1;
  }

  const parts: string[] = [];

  // Determine icon priority
  let iconKind: ActivityItem["iconKind"] = activities[0].iconKind;
  if (counts.edit > 0) iconKind = "pencil";
  else if (counts.command > 0 && counts.search === 0) iconKind = "terminal";
  else if (counts.search > 0 && counts.command === 0) iconKind = "search";
  else if (counts.read > 0 && counts.command === 0 && counts.edit === 0) iconKind = "book";
  else if (counts.artifact > 0) iconKind = "box";
  else if (counts.web > 0) iconKind = "globe";
  else if (counts.skill > 0) iconKind = "wrench";
  else if (counts.agent > 0) iconKind = "bot";

  if (isEn) {
    if (counts.read > 0) parts.push(`Read ${counts.read} ${counts.read === 1 ? "file" : "files"}`);
    if (counts.edit > 0)
      parts.push(`Edited ${counts.edit} ${counts.edit === 1 ? "file" : "files"}`);
    if (counts.search > 0) parts.push(`Searched code`);
    if (counts.command > 0)
      parts.push(`Ran ${counts.command} ${counts.command === 1 ? "command" : "commands"}`);
    if (counts.web > 0) parts.push(`Searched web`);
    if (counts.agent > 0) parts.push(`Delegated to sub-agent`);
    if (counts.skill > 0) parts.push(`Loaded skill`);
    if (counts.artifact > 0)
      parts.push(
        `Registered ${counts.artifact} ${counts.artifact === 1 ? "artifact" : "artifacts"}`,
      );
    if (counts.other > 0 && parts.length === 0)
      parts.push(`Ran ${counts.other} ${counts.other === 1 ? "tool" : "tools"}`);

    const joined = parts
      .map((p, i) => (i === 0 ? p : p.charAt(0).toLowerCase() + p.slice(1)))
      .join(" and ");
    return {
      label: joined || "Ran tools",
      iconKind,
    };
  }

  // Chinese summary
  if (counts.read > 0) parts.push(`读取了 ${counts.read} 个文件`);
  if (counts.edit > 0) parts.push(`编辑了 ${counts.edit} 个文件`);
  if (counts.search > 0) parts.push(`搜索了代码`);
  if (counts.command > 0) parts.push(`运行了 ${counts.command} 个命令`);
  if (counts.web > 0) parts.push(`搜索了网页`);
  if (counts.agent > 0) parts.push(`委派了子代理`);
  if (counts.skill > 0) parts.push(`加载了工具`);
  if (counts.artifact > 0) parts.push(`登记了 ${counts.artifact} 个成果`);
  if (counts.other > 0 && parts.length === 0) parts.push(`执行了 ${counts.other} 个操作`);

  return {
    label: parts.join("并") || "执行了操作",
    iconKind,
  };
}

/** Formats whole-turn semantic action summary for the top duration pill. */
export function formatTurnSemanticSummary(allActivities: ActivityItem[], isEn: boolean): string {
  if (allActivities.length === 0) return "";
  return formatActivityGroupSummary(allActivities, isEn).label;
}

/** Build presentation turns from the raw timeline entries and live session state. */
export function buildChatPresentationTurns(
  timeline: TimelineEntry[],
  liveState?: {
    isRunning?: boolean;
    thinkingText?: string;
    streamingText?: string;
    activeToolName?: string;
    durationMs?: number;
    streamingDisposition?: "provisional" | "final";
  },
  expandedTurns: Record<string, boolean> = {},
  planContentByToolId?: Map<string, { content: string; fileName: string } | null>,
  locale?: string,
  isCustomInteraction?: (tool: BusToolItem) => boolean,
  isToolPendingOverride?: (tool: BusToolItem) => boolean,
): ChatPresentationTurn[] {
  let curLocale = locale;
  if (!curLocale) {
    try {
      curLocale = getCurrentLocale();
    } catch {
      curLocale = "zh-CN";
    }
  }
  const isEn = Boolean(curLocale && curLocale.startsWith("en"));

  // 1. Locate all user messages
  const userIndices: number[] = [];
  timeline.forEach((entry, idx) => {
    if (entry.kind === "user") userIndices.push(idx);
  });

  const turns: ChatPresentationTurn[] = [];

  // Case: No user message yet, or initial greeting / system messages before first user prompt
  if (userIndices.length === 0 && timeline.length > 0) {
    userIndices.push(-1);
  } else if (userIndices.length > 0 && userIndices[0] > 0) {
    userIndices.unshift(-1);
  }

  for (let turnIdx = 0; turnIdx < userIndices.length; turnIdx++) {
    const userIndex = userIndices[turnIdx];
    const nextUserIndex =
      turnIdx + 1 < userIndices.length ? userIndices[turnIdx + 1] : timeline.length;

    const rawUserEntry = userIndex >= 0 ? timeline[userIndex] : undefined;
    const userEntry =
      rawUserEntry && rawUserEntry.kind === "user"
        ? (rawUserEntry as Extract<TimelineEntry, { kind: "user" }>)
        : undefined;
    const turnId = userEntry?.id ?? `turn-${turnIdx}`;
    const isLatestTurn = turnIdx === userIndices.length - 1;
    const isRunning = Boolean(isLatestTurn && liveState?.isRunning);

    // Collect all entries in this turn window
    const startIndex = userIndex >= 0 ? userIndex + 1 : 0;
    const turnEntries = timeline.slice(startIndex, nextUserIndex);

    // Find final assistant response index in turnEntries.
    // While the turn is actively running, all persisted assistant entries represent
    // intermediate execution narration / reasoning and remain inside processBlocks.
    // We only identify the final assistant response once the turn is completed (!isRunning).
    let finalAssistantIdx = -1;
    if (!isRunning) {
      for (let i = turnEntries.length - 1; i >= 0; i--) {
        const e = turnEntries[i];
        if (e.kind === "assistant" && !isToolChatterAssistant(e)) {
          finalAssistantIdx = i;
          break;
        }
      }
    }

    const processBlocks: ChatProcessBlock[] = [];
    const interactionBlocks: ChatInteractionItem[] = [];
    let hasPendingInteraction = false;
    const allActivities: ActivityItem[] = [];
    let currentActivityGroup: Extract<ChatProcessBlock, { type: "activity-group" }> | null = null;
    let turnSummaryEntry: ChatPresentationTurn["turnSummary"];
    const commandOutputs: Array<{ id: string; content: string; ts: string }> = [];
    const separators: Array<{ id: string; content: string }> = [];
    const hooks: NonNullable<ChatPresentationTurn["hooks"]> = [];

    function flushCurrentActivityGroup() {
      if (currentActivityGroup && currentActivityGroup.activities.length > 0) {
        const { label, iconKind } = formatActivityGroupSummary(
          currentActivityGroup.activities,
          isEn,
        );
        currentActivityGroup.summaryLabel = label;
        currentActivityGroup.iconKind = iconKind;
        currentActivityGroup.hasRunning = currentActivityGroup.activities.some(
          (a) => a.state === "running",
        );
        currentActivityGroup.hasFailed = currentActivityGroup.activities.some(
          (a) => a.state === "failed",
        );
        processBlocks.push(currentActivityGroup);
      }
      currentActivityGroup = null;
    }

    // Process all turn entries
    for (let i = 0; i < turnEntries.length; i++) {
      const entry = turnEntries[i];
      const isFinal = i === finalAssistantIdx;

      if (entry.kind === "tool") {
        const tool = entry.tool;
        const isCustom = isCustomInteraction ? isCustomInteraction(tool) : false;
        if (isInteractionTool(tool) || isCustom) {
          flushCurrentActivityGroup();
          let type: ChatInteractionItem["type"] = "permission";
          if (tool.tool_name === "AskUserQuestion" || tool.status === "ask_pending") {
            type = "ask_user";
          } else if (tool.tool_name === "ExitPlanMode") {
            type = "exit_plan";
          }
          const isPending = isToolPendingOverride
            ? isToolPendingOverride(tool)
            : tool.status === "permission_prompt" ||
              tool.status === "ask_pending" ||
              tool.status === "running" ||
              (tool.output === undefined && tool.error === undefined);

          const item: ChatInteractionItem = {
            id: tool.tool_use_id,
            tool,
            type,
            subTimeline: entry.subTimeline,
            planContent: planContentByToolId?.get(tool.tool_use_id),
          };
          interactionBlocks.push(item);
          processBlocks.push({
            type: "interaction",
            id: `interaction-${tool.tool_use_id}`,
            item,
            isPending,
          });
          if (isPending) {
            hasPendingInteraction = true;
          }
        } else {
          const act = normalizeToolActivity(tool, {
            subTimeline: entry.subTimeline,
            locale: curLocale,
          });
          allActivities.push(act);

          if (!currentActivityGroup) {
            currentActivityGroup = {
              type: "activity-group",
              id: `group-${tool.tool_use_id}`,
              summaryLabel: "",
              iconKind: "terminal",
              activities: [],
              hasRunning: false,
              hasFailed: false,
            };
          }
          currentActivityGroup.activities.push(act);
        }
      } else if (entry.kind === "assistant") {
        if (!isFinal) {
          // Intermediate assistant entry: extract reasoning and narration
          flushCurrentActivityGroup();
          if (entry.thinkingText && entry.thinkingText.trim()) {
            processBlocks.push({
              type: "reasoning",
              id: `${entry.id}-reasoning`,
              content: entry.thinkingText.trim(),
              isStreaming: false,
            });
          }
          if (entry.content && !isToolChatterAssistant(entry)) {
            processBlocks.push({
              type: "narration",
              id: `${entry.id}-narration`,
              content: entry.content.trim(),
              isStreaming: false,
            });
          }
        } else {
          // Final assistant entry: if it has thinking, that belongs to processBlocks
          if (entry.thinkingText && entry.thinkingText.trim()) {
            flushCurrentActivityGroup();
            processBlocks.push({
              type: "reasoning",
              id: `${entry.id}-reasoning`,
              content: entry.thinkingText.trim(),
              isStreaming: false,
            });
          }
        }
      } else if (entry.kind === "command_output") {
        flushCurrentActivityGroup();
        commandOutputs.push({
          id: entry.id,
          content: entry.content,
          ts: entry.ts,
        });
      } else if (entry.kind === "turn_summary") {
        flushCurrentActivityGroup();
        turnSummaryEntry = {
          id: entry.id,
          anchorId: entry.anchorId,
          diff: entry.diff,
          cwd: entry.cwd,
          ts: entry.ts,
        };
      } else if (entry.kind === "separator") {
        flushCurrentActivityGroup();
        separators.push({
          id: entry.id,
          content: entry.content,
        });
      } else if (entry.kind === "hook") {
        flushCurrentActivityGroup();
        hooks.push({
          id: entry.id,
          eventName: entry.eventName,
          status: (entry.status === "failed"
            ? "failed"
            : entry.status === "running"
              ? "running"
              : "success") as "running" | "success" | "failed",
          statusMessage: entry.statusMessage,
          durationMs: entry.durationMs,
        });
      }
    }

    // Flush any pending activity group
    flushCurrentActivityGroup();

    // Attach live state if active latest turn
    if (isLatestTurn && isRunning) {
      if (liveState?.thinkingText && liveState.thinkingText.trim()) {
        processBlocks.push({
          type: "reasoning",
          id: `live-thinking-${turnId}`,
          content: liveState.thinkingText.trim(),
          isStreaming: true,
        });
      }
      if (
        liveState?.streamingText &&
        liveState.streamingText.trim() &&
        liveState.streamingDisposition !== "final"
      ) {
        processBlocks.push({
          type: "narration",
          id: `live-narration-${turnId}`,
          content: liveState.streamingText.trim(),
          isStreaming: true,
        });
      }
    }

    // Determine final message
    let finalMessage: ChatPresentationTurn["finalMessage"] = undefined;
    if (finalAssistantIdx >= 0) {
      const finalEntry = turnEntries[finalAssistantIdx];
      if (finalEntry.kind === "assistant") {
        finalMessage = {
          id: finalEntry.id,
          anchorId: finalEntry.anchorId,
          content: finalEntry.content,
          timestamp: finalEntry.ts,
          model: finalEntry.model,
          isStreaming: false,
        };
      }
    } else if (
      isLatestTurn &&
      liveState?.streamingText &&
      (!isRunning || liveState.streamingDisposition === "final")
    ) {
      finalMessage = {
        id: `live-final-${turnId}`,
        anchorId: `live-final-${turnId}`,
        content: liveState.streamingText,
        timestamp: new Date().toISOString(),
        isStreaming: true,
      };
    }

    // Duration calculation
    const userTs = userEntry ? new Date(userEntry.ts).getTime() : NaN;
    let endTs = NaN;
    if (finalMessage?.timestamp) {
      endTs = new Date(finalMessage.timestamp).getTime();
    } else if (turnEntries.length > 0) {
      endTs = new Date(turnEntries[turnEntries.length - 1].ts).getTime();
    }
    // For the pseudo-turn before the first user message there is no prompt timestamp.
    // Anchor the start at the first entry of the turn instead of the Unix epoch,
    // otherwise the duration pill would show the raw epoch offset (e.g. "用时 497025h").
    const startTs = Number.isFinite(userTs)
      ? userTs
      : turnEntries.length > 0
        ? new Date(turnEntries[0].ts).getTime()
        : NaN;
    let durationMs =
      Number.isFinite(startTs) && Number.isFinite(endTs) ? Math.max(1000, endTs - startTs) : 1000;
    if (isLatestTurn && (liveState?.durationMs ?? 0) > 0) {
      durationMs = Math.max(durationMs, liveState!.durationMs!);
    }

    const turnSemanticSummary = formatTurnSemanticSummary(allActivities, isEn);

    // Collapsed rule:
    // Priority: pending interaction (must be visible) > explicit user choice > running default > completed default
    let isCollapsed = false;
    if (hasPendingInteraction) {
      isCollapsed = false;
    } else if (expandedTurns[turnId] !== undefined) {
      isCollapsed = !expandedTurns[turnId];
    } else if (isRunning) {
      isCollapsed = false;
    } else {
      isCollapsed = true;
    }

    // Active thinking / tool / narration preview (DSH style)
    let activeThinkingPreview: string | undefined = undefined;
    let activeToolPreview: string | undefined = undefined;
    let activeNarrationPreview: string | undefined = undefined;
    if (isRunning) {
      const activeReasoning = processBlocks
        .slice()
        .reverse()
        .find(
          (b): b is Extract<ChatProcessBlock, { type: "reasoning" }> =>
            b.type === "reasoning" && b.isStreaming,
        );
      if (activeReasoning?.content) {
        const line = extractLatestThinkingLine(activeReasoning.content);
        if (line) activeThinkingPreview = line;
      }
      // Use the *latest* running activity: with parallel or stale-status tools the
      // first running entry may no longer be the action the agent is performing.
      const runningAct = allActivities.findLast((a) => a.state === "running");
      if (runningAct) {
        activeToolPreview = `${runningAct.verb} ${runningAct.target}`.trim();
      }
      // Extract latest narration preview
      const activeNarration = processBlocks
        .slice()
        .reverse()
        .find((b): b is Extract<ChatProcessBlock, { type: "narration" }> => b.type === "narration");
      if (activeNarration?.content) {
        const line = extractLatestNarrationLine(activeNarration.content);
        if (line) activeNarrationPreview = line;
      }
    }

    // Sanitize turnSummary: if this turn explicitly executed tools, but NONE of the files in
    // turnSummary.diff match any file touched by mutation tools in this turn, it is a cross-session
    // Git snapshot artifact from a concurrent conversation. Suppress it so it doesn't pollute the UI.
    if (turnSummaryEntry) {
      const parsed = parseUnifiedDiffStats(turnSummaryEntry.diff);
      if (parsed.files.length === 0) {
        turnSummaryEntry = undefined;
      } else {
        const touchedPaths = extractTurnModifiedPaths(turnEntries);
        const hasTools = turnEntries.some((e) => e.kind === "tool");
        if (hasTools && touchedPaths.length === 0) {
          // The turn executed tools (e.g. read/search/bash), but no file mutation tools.
          // Any diff here is an external/cross-session Git artifact.
          turnSummaryEntry = undefined;
        } else if (touchedPaths.length > 0) {
          // Verify that diff has at least some overlap with touched files.
          const hasOverlap = parsed.files.some((f) =>
            touchedPaths.some((p) => {
              const fNorm = f.path.replace(/\\/g, "/").replace(/^\.\//, "");
              const pNorm = p.replace(/\\/g, "/").replace(/^\.\//, "");
              return fNorm === pNorm || fNorm.endsWith(`/${pNorm}`) || pNorm.endsWith(`/${fNorm}`);
            }),
          );
          if (!hasOverlap) {
            turnSummaryEntry = undefined;
          }
        }
      }
    }

    turns.push({
      id: turnId,
      turnIndex: turnIdx,
      userTimelineIndex: userIndex >= 0 ? userIndex : undefined,
      userMessage: userEntry
        ? {
            id: userEntry.id,
            anchorId: userEntry.anchorId,
            content: userEntry.content,
            timestamp: userEntry.ts,
            attachments: userEntry.attachments,
            cliUuid: userEntry.cliUuid,
          }
        : undefined,
      durationMs,
      durationFormatted: formatCodexDuration(durationMs),
      isRunning,
      processBlocks,
      interactionBlocks,
      hasPendingInteraction,
      finalMessage,
      turnSummary: turnSummaryEntry,
      commandOutputs: commandOutputs.length > 0 ? commandOutputs : undefined,
      separators: separators.length > 0 ? separators : undefined,
      hooks: hooks.length > 0 ? hooks : undefined,
      turnSemanticSummary,
      activeThinkingPreview,
      activeToolPreview,
      activeNarrationPreview,
      isCollapsed,
    });
  }

  return turns;
}
