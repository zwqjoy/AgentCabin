import type { SessionPhase } from "$lib/stores/types";
import type { PetAggregateState, PetMode } from "./types";
import { SESSION_PHASE_TO_PET_MODE } from "./types";

export const REVIEW_HOLD_MS = 4000;
export const FAILED_HOLD_MS = 4000;
export const MIN_STATE_HOLD_MS = 600;

export function mapSessionPhaseToPetMode(phase: SessionPhase): PetMode {
  return SESSION_PHASE_TO_PET_MODE[phase] ?? "idle";
}

export function aggregatePetState(
  sessions: Array<{ phase: SessionPhase; runId?: string }>,
): PetAggregateState {
  if (sessions.length === 0) {
    return {
      mode: "idle",
      runningCount: 0,
      errorCount: 0,
      activeRunId: null,
      timestamp: Date.now(),
    };
  }

  let runningCount = 0;
  let errorCount = 0;
  let hasWaiting = false;
  let activeRunId: string | null = null;
  let activePriority = 0;

  const priority = (mode: PetMode) => {
    if (mode === "failed") return 5;
    if (mode === "running") return 4;
    if (mode === "waiting") return 3;
    if (mode === "review") return 2;
    return 1;
  };

  for (const s of sessions) {
    const mode = mapSessionPhaseToPetMode(s.phase);
    if (mode === "running") {
      runningCount++;
    } else if (mode === "failed") {
      errorCount++;
    } else if (mode === "waiting") {
      hasWaiting = true;
    }

    if (s.runId && priority(mode) > activePriority) {
      activePriority = priority(mode);
      activeRunId = s.runId;
    }
  }

  let mode: PetMode = "idle";
  if (errorCount > 0) {
    mode = "failed";
  } else if (runningCount > 0) {
    mode = "running";
  } else if (hasWaiting) {
    mode = "waiting";
  }

  return {
    mode,
    runningCount,
    errorCount,
    activeRunId,
    timestamp: Date.now(),
  };
}

export class PetStateMachine {
  private currentState: PetAggregateState | null = null;
  private lastChangeAt = 0;
  private transTimer: ReturnType<typeof setTimeout> | null = null;
  private listener: ((state: PetAggregateState) => void) | null = null;

  constructor(onUpdate?: (state: PetAggregateState) => void) {
    if (onUpdate) this.listener = onUpdate;
  }

  public setListener(onUpdate: (state: PetAggregateState) => void) {
    this.listener = onUpdate;
  }

  public getCurrentState(): PetAggregateState | null {
    return this.currentState;
  }

  private clearTransition() {
    if (this.transTimer) {
      clearTimeout(this.transTimer);
      this.transTimer = null;
    }
  }

  private setTransition(ms: number, fn: () => void) {
    this.clearTransition();
    this.transTimer = setTimeout(() => {
      this.transTimer = null;
      fn();
    }, ms);
  }

  private holdThenReturnToIdle(state: PetAggregateState, holdMs: number) {
    this.clearTransition();
    this.applyState(state);
    this.setTransition(holdMs, () => {
      if (this.currentState?.mode === state.mode) {
        this.applyState({ ...state, mode: "idle", timestamp: Date.now() });
      }
    });
  }

  public update(targetState: PetAggregateState) {
    const prev = this.currentState;
    const targetMode = targetState.mode;
    const now = Date.now();

    // A backend aggregate may send review explicitly (completed), while a
    // running session can also close with an idle event. Both paths get the
    // same short review animation.
    if (targetMode === "review") {
      if (prev?.mode === "review") {
        if (this.isSameVisibleState(prev, targetState)) return;
        this.applyState(targetState);
        return;
      }
      this.holdThenReturnToIdle({ ...targetState, mode: "review", timestamp: now }, REVIEW_HOLD_MS);
      return;
    }

    // 1. Transition: running -> idle triggers review mode for 4s
    if (targetMode === "idle" && prev?.mode === "running") {
      this.holdThenReturnToIdle({ ...targetState, mode: "review", timestamp: now }, REVIEW_HOLD_MS);
      return;
    }

    // 2. Ignore idle update if currently in review mode (waiting for review timer)
    if (targetMode === "idle" && prev?.mode === "review") {
      return;
    }

    // 3. Transition: failed mode triggers failed animation for 4s then back to idle
    if (targetMode === "failed") {
      if (prev?.mode === "failed") {
        if (!this.isSameVisibleState(prev, targetState)) this.applyState(targetState);
      } else {
        this.holdThenReturnToIdle(targetState, FAILED_HOLD_MS);
      }
      return;
    }

    // 4. Ignore idle update if currently holding failed animation
    if (targetMode === "idle" && prev?.mode === "failed") {
      return;
    }

    // 5. Anti-jitter / Hold lock
    if (prev && targetMode !== prev.mode && now - this.lastChangeAt < MIN_STATE_HOLD_MS) {
      return;
    }

    // 6. Deduplicate exact same state
    if (prev && this.isSameVisibleState(prev, targetState)) {
      return;
    }

    this.clearTransition();
    this.applyState(targetState);
  }

  private isSameVisibleState(left: PetAggregateState, right: PetAggregateState) {
    return (
      left.mode === right.mode &&
      left.runningCount === right.runningCount &&
      left.errorCount === right.errorCount &&
      left.activeRunId === right.activeRunId
    );
  }

  private applyState(state: PetAggregateState) {
    this.currentState = state;
    this.lastChangeAt = Date.now();
    if (this.listener) {
      this.listener(state);
    }
  }

  public destroy() {
    this.clearTransition();
    this.listener = null;
  }
}
