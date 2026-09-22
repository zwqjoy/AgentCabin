export interface WorkTurnActivitySummaryState {
  hasActivity: boolean;
  isRunning: boolean;
  hasAssistantReply: boolean;
  toolCount: number;
  thinkingText: string;
}

export interface LiveAssistantTurnHeaderState {
  isRunning: boolean;
  hasLatestAssistantTurnHeader: boolean;
  hasThinkingText: boolean;
  hasStreamingText: boolean;
}

/**
 * Keep the assistant identity visible while Work is running or transitions from
 * thinking to streamed text. Retained partial output also needs an identity after
 * the runtime leaves the running phase; otherwise an interrupted answer appears
 * as an unexplained blank tail in the transcript.
 */
export function shouldRenderLiveAssistantTurnHeader(state: LiveAssistantTurnHeaderState): boolean {
  return (
    !state.hasLatestAssistantTurnHeader &&
    (state.isRunning || state.hasThinkingText || state.hasStreamingText)
  );
}

/**
 * Keep the latest-turn activity summary only when it has something meaningful
 * to summarize. A terminal Pi error can be the only entry after a user
 * message; that error must remain visible in the transcript instead of being
 * hidden inside an empty summary.
 */
export function shouldRenderWorkTurnActivitySummary(state: WorkTurnActivitySummaryState): boolean {
  return (
    state.hasActivity &&
    (state.isRunning ||
      state.hasAssistantReply ||
      state.toolCount > 0 ||
      Boolean(state.thinkingText.trim()))
  );
}

/**
 * Find the index of the timeline entry that starts the assistant response within a turn.
 * Ensures the assistant identity header is anchored at the start of the turn even when
 * intermediate steps are collapsed or when there are only tool calls.
 */
export function findTurnAssistantHeaderIndex(
  turnIndices: number[],
  turnHeaderIndices: Set<number>,
  intermediateIndices: number[] = [],
  finalAssistantIndex: number | null = null,
  trailingIndices: number[] = [],
): number | null {
  for (const idx of turnIndices) {
    if (turnHeaderIndices.has(idx)) {
      return idx;
    }
  }
  if (intermediateIndices.length > 0) {
    return intermediateIndices[0];
  }
  if (finalAssistantIndex !== null) {
    return finalAssistantIndex;
  }
  if (trailingIndices.length > 0) {
    return trailingIndices[0];
  }
  return null;
}
