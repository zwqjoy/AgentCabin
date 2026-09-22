import React, { useState, useEffect, useRef, useMemo } from "react";
import {
  MarkdownText,
  CodeBlock,
  TerminalBlock,
  DiffBlock,
  DisclosureRow,
  Tooltip,
  Pill,
  Button,
  IconDatabaseOutline16,
  IconChevronDownOutline14,
  IconCopyOutline16,
  IconCheckOutline16,
} from "@deepseek-ai/dsh-client-ui-primitives";
import { renderMarkdown } from "../utils/markdown";

export interface DshTurnItem {
  turnIndex: number;
  id: string;
  userPrompt?: string;
  assistantReply?: string;
  model?: string;
  isRunning?: boolean;
  isStreaming?: boolean;
  durationMs?: number;
  reasoningText?: string;
  isReasoningStreaming?: boolean;
  tools?: Array<{
    id: string;
    name: string;
    summary?: string;
    state: "running" | "completed" | "failed";
    output?: string;
  }>;
  usage?: {
    totalTokens?: number;
    inputTokens?: number;
    outputTokens?: number;
    cacheReadTokens?: number;
    cacheWriteTokens?: number;
    reasoningTokens?: number;
    model?: string;
  };
}

export interface DshChatContainerProps {
  turns: DshTurnItem[];
  isRunning?: boolean;
  onOpenFile?: (path: string) => void;
  onCopyText?: (text: string) => void;
}

export function DshChatContainer({
  turns,
  isRunning = false,
  onOpenFile,
  onCopyText,
}: DshChatContainerProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [atBottom, setAtBottom] = useState(true);
  const [activeTurn, setActiveTurn] = useState<number>(0);
  const [openProcesses, setOpenProcesses] = useState<Record<number, boolean>>({});
  const [openUsageDialog, setOpenUsageDialog] = useState<number | null>(null);
  const [copiedTurn, setCopiedTurn] = useState<number | null>(null);

  // Monitor scroll to toggle ToBottomButton
  const handleScroll = () => {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    const isBottom = scrollHeight - scrollTop - clientHeight < 60;
    setAtBottom(isBottom);
  };

  const scrollToBottom = () => {
    if (scrollRef.current) {
      scrollRef.current.scrollTo({
        top: scrollRef.current.scrollHeight,
        behavior: "smooth",
      });
      setAtBottom(true);
    }
  };

  const scrollToTurn = (turnIndex: number) => {
    const el = document.getElementById(`dsh-turn-${turnIndex}`);
    if (el && scrollRef.current) {
      el.scrollIntoView({ behavior: "smooth", block: "start" });
      setActiveTurn(turnIndex);
    }
  };

  const toggleProcess = (turnIndex: number) => {
    setOpenProcesses((prev) => ({
      ...prev,
      [turnIndex]: !prev[turnIndex],
    }));
  };

  const handleCopy = (turnIndex: number, text: string) => {
    if (onCopyText) {
      onCopyText(text);
    } else {
      navigator.clipboard.writeText(text);
    }
    setCopiedTurn(turnIndex);
    setTimeout(() => setCopiedTurn(null), 1500);
  };

  // Follow bottom when streaming
  useEffect(() => {
    if (isRunning && atBottom && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [turns, isRunning, atBottom]);

  return (
    <div className="relative flex flex-col w-full h-full min-h-0 bg-[var(--dsw-alias-bg-base)] text-[var(--dsw-alias-label-primary)]">
      {/* Scroll View */}
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="flex-1 w-full overflow-y-auto px-4 sm:px-6 py-4 space-y-6"
      >
        <div className="max-w-[var(--dsh-chat-content-width,768px)] mx-auto w-full space-y-6">
          {turns.map((turn) => {
            const isProcessOpen = Boolean(openProcesses[turn.turnIndex]);
            const hasProcess =
              Boolean(turn.reasoningText) ||
              (turn.tools && turn.tools.length > 0) ||
              turn.isRunning;
            const toolCount = turn.tools?.length || 0;
            const processLabel =
              toolCount > 0
                ? `已完成 ${toolCount} 个工具调用`
                : turn.reasoningText
                  ? "已思考"
                  : "执行处理";

            return (
              <div
                key={turn.id || turn.turnIndex}
                id={`dsh-turn-${turn.turnIndex}`}
                className="space-y-4"
              >
                {/* User Message Bubble */}
                {turn.userPrompt && (
                  <div className="flex justify-end">
                    <div className="max-w-[85%] rounded-2xl bg-[var(--dsw-alias-bg-layer-2,#f7f8fa)] dark:bg-[var(--dsw-alias-bg-layer-2,#1e2026)] px-4 py-2.5 text-sm text-[var(--dsw-alias-label-primary)] shadow-xs border border-[var(--dsw-alias-border-l1)]">
                      <p className="whitespace-pre-wrap leading-relaxed">{turn.userPrompt}</p>
                    </div>
                  </div>
                )}

                {/* Turn Process Disclosure Header (TurnProcessNodeView) */}
                {hasProcess && (
                  <div className="w-full border-b border-[var(--dsw-alias-border-l2)] pb-1">
                    <button
                      type="button"
                      onClick={() => toggleProcess(turn.turnIndex)}
                      className="w-full h-[33px] flex items-center justify-between text-left cursor-pointer text-sm text-[var(--dsw-alias-label-secondary)] hover:text-[var(--dsw-alias-label-primary)] transition-colors select-none"
                    >
                      <div className="flex items-center gap-2 truncate">
                        <span className="truncate">{processLabel}</span>
                        {turn.isRunning && (
                          <span className="h-1.5 w-1.5 rounded-full bg-[var(--dsw-alias-brand-primary)] animate-pulse shrink-0" />
                        )}
                      </div>
                      <IconChevronDownOutline14
                        className={`transition-transform duration-150 shrink-0 ${
                          isProcessOpen ? "rotate-0" : "-rotate-90"
                        }`}
                      />
                    </button>

                    {/* Collapsible Process Content */}
                    {isProcessOpen && (
                      <div className="py-2 space-y-2 animate-fade-in text-xs text-[var(--dsw-alias-label-secondary)]">
                        {/* Reasoning block */}
                        {turn.reasoningText && (
                          <div className="rounded-lg bg-[var(--dsw-alias-markdown-code-block)] p-3 border border-[var(--dsw-alias-border-l1)]">
                            <div className="flex items-center gap-1.5 font-medium text-[var(--dsw-alias-label-primary)] mb-1">
                              <span>思考过程</span>
                              {turn.isReasoningStreaming && (
                                <span className="text-[10px] text-blue-500 animate-pulse">
                                  正在输出…
                                </span>
                              )}
                            </div>
                            <p className="whitespace-pre-wrap leading-relaxed opacity-85">
                              {turn.reasoningText}
                            </p>
                          </div>
                        )}

                        {/* Tool Calls with Sweep Light Effect */}
                        {turn.tools?.map((tool) => (
                          <div
                            key={tool.id}
                            className={`rounded-lg border border-[var(--dsw-alias-border-l1)] bg-[var(--dsw-alias-bg-layer-1)] p-2.5 space-y-1.5 ${
                              tool.state === "running" ? "dsh-command-sweep" : ""
                            }`}
                          >
                            <div className="flex items-center justify-between">
                              <span className="font-mono font-medium text-[var(--dsw-alias-label-primary)]">
                                {tool.name}
                              </span>
                              <span
                                className={`text-[10px] ${
                                  tool.state === "running"
                                    ? "text-blue-500 animate-pulse"
                                    : tool.state === "failed"
                                      ? "text-red-500"
                                      : "text-emerald-500"
                                }`}
                              >
                                {tool.state}
                              </span>
                            </div>
                            {tool.summary && (
                              <p className="text-[11px] text-[var(--dsw-alias-label-caption)] font-mono truncate">
                                {tool.summary}
                              </p>
                            )}
                            {tool.output && (
                              <pre className="max-h-40 overflow-auto rounded bg-[var(--dsw-alias-markdown-code-block)] p-2 font-mono text-[10px] leading-relaxed">
                                {tool.output}
                              </pre>
                            )}
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )}

                {/* Assistant Final Reply with MarkdownText */}
                {turn.assistantReply && (
                  <div
                    className="space-y-2 text-sm leading-relaxed"
                    onClick={(e) => {
                      const target = e.target as HTMLElement | null;
                      const fileEl = target?.closest<HTMLElement>("[data-file-path]");
                      if (fileEl && onOpenFile) {
                        const p = fileEl.getAttribute("data-file-path");
                        if (p) {
                          e.preventDefault();
                          e.stopPropagation();
                          onOpenFile(p);
                        }
                      }
                    }}
                  >
                    <div
                      className="prose dark:prose-invert max-w-none prose-chat"
                      dangerouslySetInnerHTML={{ __html: renderMarkdown(turn.assistantReply) }}
                    />

                    {/* Turn Tail Actions (Usage Pill + Copy Button) */}
                    <div className="flex items-center gap-3 pt-1 text-xs text-[var(--dsw-alias-label-caption)] select-none">
                      {turn.usage?.totalTokens !== undefined && (
                        <div className="relative inline-flex">
                          <button
                            type="button"
                            onClick={() =>
                              setOpenUsageDialog(
                                openUsageDialog === turn.turnIndex ? null : turn.turnIndex,
                              )
                            }
                            className="inline-flex items-center gap-1 rounded-full px-2 py-0.5 font-mono text-[11.5px] transition-colors hover:bg-[var(--dsw-alias-bg-layer-2)] hover:text-[var(--dsw-alias-label-primary)] cursor-pointer border border-[var(--dsw-alias-border-l1)]"
                          >
                            <IconDatabaseOutline16 />
                            <span>消耗 {turn.usage.totalTokens} Tokens</span>
                          </button>

                          {/* DSH Usage Dialog Popover */}
                          {openUsageDialog === turn.turnIndex && (
                            <div className="absolute bottom-full left-0 mb-2 z-50 min-w-[280px] rounded-xl border border-[var(--dsw-alias-border-l2)] bg-[var(--dsw-alias-bg-layer-1)] p-3.5 shadow-xl backdrop-blur-md text-xs space-y-2">
                              <div className="flex items-center justify-between font-medium border-b border-[var(--dsw-alias-border-l1)] pb-1.5">
                                <span className="flex items-center gap-1.5">
                                  <IconDatabaseOutline16 /> 本轮消耗
                                </span>
                                <span className="font-mono">{turn.usage.totalTokens} tok</span>
                              </div>
                              <dl className="grid grid-cols-2 gap-y-1.5 text-[11px]">
                                {turn.usage.model && (
                                  <>
                                    <dt className="text-[var(--dsw-alias-label-caption)]">模型</dt>
                                    <dd className="text-right font-mono truncate">
                                      {turn.usage.model}
                                    </dd>
                                  </>
                                )}
                                <dt className="text-[var(--dsw-alias-label-caption)]">
                                  输入 Token
                                </dt>
                                <dd className="text-right font-mono">
                                  {turn.usage.inputTokens || 0}
                                </dd>
                                <dt className="text-[var(--dsw-alias-label-caption)]">读取缓存</dt>
                                <dd className="text-right font-mono">
                                  {turn.usage.cacheReadTokens || 0}
                                </dd>
                                <dt className="text-[var(--dsw-alias-label-caption)]">写入缓存</dt>
                                <dd className="text-right font-mono">
                                  {turn.usage.cacheWriteTokens || 0}
                                </dd>
                                <dt className="text-[var(--dsw-alias-label-caption)]">
                                  输出 Token
                                </dt>
                                <dd className="text-right font-mono">
                                  {turn.usage.outputTokens || 0}
                                </dd>
                              </dl>
                            </div>
                          )}
                        </div>
                      )}

                      {/* Copy Action */}
                      <button
                        type="button"
                        onClick={() => handleCopy(turn.turnIndex, turn.assistantReply || "")}
                        className="inline-flex items-center gap-1 p-1 hover:text-[var(--dsw-alias-label-primary)] transition-colors cursor-pointer"
                        title="复制回答"
                      >
                        {copiedTurn === turn.turnIndex ? (
                          <IconCheckOutline16 className="text-emerald-500" />
                        ) : (
                          <IconCopyOutline16 />
                        )}
                      </button>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>

      {/* DSH ToBottomButton (sticky floating button) */}
      {!atBottom && (
        <div className="absolute bottom-4 right-6 z-20">
          <button
            type="button"
            onClick={scrollToBottom}
            className="flex h-8 w-8 items-center justify-center rounded-full border border-[var(--dsw-alias-border-l3)] bg-[var(--dsw-alias-button-floating-fill)] text-[var(--dsw-alias-label-primary)] shadow-md hover:bg-[var(--dsw-alias-button-floating-hover)] transition-transform hover:scale-105 active:scale-95 cursor-pointer"
            aria-label="回到底部"
          >
            <IconChevronDownOutline14 />
          </button>
        </div>
      )}
    </div>
  );
}
