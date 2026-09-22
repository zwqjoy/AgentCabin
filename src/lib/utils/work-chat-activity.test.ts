import { describe, expect, it } from "vitest";
import {
  findTurnAssistantHeaderIndex,
  shouldRenderLiveAssistantTurnHeader,
  shouldRenderWorkTurnActivitySummary,
} from "./work-chat-activity";

describe("shouldRenderLiveAssistantTurnHeader", () => {
  it("shows the identity while running before thinking or streamed text arrives", () => {
    expect(
      shouldRenderLiveAssistantTurnHeader({
        isRunning: true,
        hasLatestAssistantTurnHeader: false,
        hasThinkingText: false,
        hasStreamingText: false,
      }),
    ).toBe(true);
  });

  it("keeps the identity visible when streamed text starts before thinking is cleared", () => {
    expect(
      shouldRenderLiveAssistantTurnHeader({
        isRunning: true,
        hasLatestAssistantTurnHeader: false,
        hasThinkingText: true,
        hasStreamingText: true,
      }),
    ).toBe(true);
  });

  it("does not add a duplicate identity or show one without retained output", () => {
    expect(
      shouldRenderLiveAssistantTurnHeader({
        isRunning: true,
        hasLatestAssistantTurnHeader: true,
        hasThinkingText: false,
        hasStreamingText: true,
      }),
    ).toBe(false);
    expect(
      shouldRenderLiveAssistantTurnHeader({
        isRunning: false,
        hasLatestAssistantTurnHeader: false,
        hasThinkingText: true,
        hasStreamingText: false,
      }),
    ).toBe(true);
    expect(
      shouldRenderLiveAssistantTurnHeader({
        isRunning: false,
        hasLatestAssistantTurnHeader: false,
        hasThinkingText: false,
        hasStreamingText: true,
      }),
    ).toBe(true);
    expect(
      shouldRenderLiveAssistantTurnHeader({
        isRunning: false,
        hasLatestAssistantTurnHeader: false,
        hasThinkingText: false,
        hasStreamingText: false,
      }),
    ).toBe(false);
  });
});

describe("shouldRenderWorkTurnActivitySummary", () => {
  it("does not hide a Pi error when there is no assistant reply", () => {
    expect(
      shouldRenderWorkTurnActivitySummary({
        hasActivity: true,
        isRunning: false,
        hasAssistantReply: false,
        toolCount: 0,
        thinkingText: "",
      }),
    ).toBe(false);
  });

  it("keeps the activity summary for a running turn", () => {
    expect(
      shouldRenderWorkTurnActivitySummary({
        hasActivity: true,
        isRunning: true,
        hasAssistantReply: false,
        toolCount: 0,
        thinkingText: "",
      }),
    ).toBe(true);
  });

  it("keeps the activity summary when tools or thinking are present", () => {
    expect(
      shouldRenderWorkTurnActivitySummary({
        hasActivity: true,
        isRunning: false,
        hasAssistantReply: false,
        toolCount: 1,
        thinkingText: "",
      }),
    ).toBe(true);
    expect(
      shouldRenderWorkTurnActivitySummary({
        hasActivity: true,
        isRunning: false,
        hasAssistantReply: false,
        toolCount: 0,
        thinkingText: "正在分析",
      }),
    ).toBe(true);
  });
});

describe("findTurnAssistantHeaderIndex", () => {
  it("anchors header on the first matching turn header index", () => {
    const turnIndices = [1, 2, 3];
    const turnHeaderIndices = new Set([1]);
    expect(findTurnAssistantHeaderIndex(turnIndices, turnHeaderIndices, [1, 2], 3)).toBe(1);
  });

  it("identifies header index when a turn has only tool calls and no final assistant", () => {
    // Like turn 1 in Image 1: tool calls were executed but no final assistant text
    const turnIndices = [1];
    const turnHeaderIndices = new Set([1]);
    expect(findTurnAssistantHeaderIndex(turnIndices, turnHeaderIndices, [1], null)).toBe(1);
  });

  it("identifies header index when a turn only has a final assistant response", () => {
    const turnIndices = [1];
    const turnHeaderIndices = new Set([1]);
    expect(findTurnAssistantHeaderIndex(turnIndices, turnHeaderIndices, [], 1)).toBe(1);
  });

  it("falls back to intermediate indices if turnHeaderIndices did not capture it", () => {
    const turnIndices = [1, 2];
    const turnHeaderIndices = new Set<number>();
    expect(findTurnAssistantHeaderIndex(turnIndices, turnHeaderIndices, [1, 2], null)).toBe(1);
  });

  it("falls back to final assistant index if intermediate indices are empty", () => {
    const turnIndices = [1];
    const turnHeaderIndices = new Set<number>();
    expect(findTurnAssistantHeaderIndex(turnIndices, turnHeaderIndices, [], 1)).toBe(1);
  });

  it("returns null when turn has no assistant entries yet", () => {
    const turnIndices: number[] = [];
    const turnHeaderIndices = new Set<number>();
    expect(findTurnAssistantHeaderIndex(turnIndices, turnHeaderIndices, [], null)).toBe(null);
  });
});
