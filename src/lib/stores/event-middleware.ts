/**
 * EventMiddleware: unified Tauri event listener management.
 *
 * - Registers Tauri event listeners once
 * - Routes events by run_id to the subscribed SessionStore
 * - Microbatches bus-events (16ms) to reduce reactive updates
 * - Pipe events go through handler callbacks (DOM-bound)
 */
import { dbg, dbgWarn } from "$lib/utils/debug";
import type { BusEvent, HookEvent } from "$lib/types";
import type { SessionStore } from "./session-store.svelte";
import { markAttention, clearAttention } from "./attention-store.svelte";
import { getTransport } from "$lib/transport";

const LISTENER_REGISTRATION_TIMEOUT_MS = 5_000;
const LOW_PRIORITY_EVENT_TYPES = new Set(["thinking_delta"]);
// A Work response can emit dozens of thinking chunks per second. Keep the
// stream visibly progressive, but leave enough idle time for WebView input,
// navigation, and sidebar clicks to be dispatched between renders.
const LOW_PRIORITY_FLUSH_INTERVAL_MS = 120;

// ── Handler interfaces (page-level DOM callbacks) ──

export interface PipeHandler {
  onDelta(delta: { text: string }): void;
  onDone(done: { ok: boolean; code: number; error?: string }): void;
}

export interface RunEventHandler {
  onRunEvent(event: { run_id: string; type: string; text: string }): void;
}

// ── Middleware ──

export class EventMiddleware {
  private _unlisteners: (() => void)[] = [];
  private _subscriptions = new Map<string, SessionStore>();
  private _currentRunId: string | null = null;
  private _currentStore: SessionStore | null = null;

  // Handler callbacks (set by page component)
  private _pipeHandler: PipeHandler | null = null;
  private _runEventHandler: RunEventHandler | null = null;
  private _collaborationHandler: ((event: BusEvent) => void) | null = null;
  /** Lightweight fan-out for stores that need live projections without owning a SessionStore. */
  private _eventSubscribers = new Set<(event: BusEvent) => void>();

  // Microbatch buffer for bus events
  private _batchBuffer = new Map<string, BusEvent[]>();
  private _flushScheduled = false;
  private _BATCH_INTERVAL = 16; // ~1 frame
  private _MAX_BUFFER_SIZE = 500; // per-run overflow threshold
  private _lastFlushTime = 0; // track last flush for idle detection
  private _IDLE_GAP_MS = 100; // gap above which the next token starts a fresh burst
  private _lowPriorityFlushTimer: ReturnType<typeof setTimeout> | null = null;

  // Idempotent start guard
  private _started = false;

  // Debounce guard for _full_reload
  private _reloadingRuns = new Set<string>();

  // ── Lifecycle ──

  async start(): Promise<void> {
    if (this._started) {
      dbg("middleware", "start skipped (already started)");
      return;
    }
    this._started = true;
    dbg("middleware", "starting event listeners");
    const ul = this._unlisteners;

    const transport = getTransport();

    // Helper: register a single listener via transport (works for both Tauri + WS).
    // Transport.listen delivers payload directly (TauriTransport unwraps the envelope).
    // If one listener fails to register, the rest still get set up (partial degradation).
    const reg = async <T>(name: string, handler: (payload: T) => void) => {
      let timedOut = false;
      let timer: ReturnType<typeof setTimeout> | undefined;
      try {
        const listenPromise = transport.listen<T>(name, handler);
        const unlisten = await new Promise<() => void>((resolve, reject) => {
          timer = setTimeout(() => {
            timedOut = true;
            reject(
              new Error(
                `event listener registration timed out after ${LISTENER_REGISTRATION_TIMEOUT_MS}ms`,
              ),
            );
          }, LISTENER_REGISTRATION_TIMEOUT_MS);

          listenPromise.then(
            (lateUnlisten) => {
              if (timedOut) {
                // A late Tauri registration must not leak after the UI has
                // continued without it.
                try {
                  lateUnlisten();
                } catch (error) {
                  dbgWarn("middleware", `late listener cleanup failed for "${name}":`, error);
                }
                return;
              }
              resolve(lateUnlisten);
            },
            (error) => {
              if (!timedOut) reject(error);
              else dbgWarn("middleware", `late listener registration failed for "${name}":`, error);
            },
          );
        });
        ul.push(unlisten);
      } catch (e) {
        dbgWarn("middleware", `failed to register listener for "${name}":`, e);
      } finally {
        if (timer) clearTimeout(timer);
      }
    };

    // Register independently so one stuck Tauri event command cannot prevent
    // the Work session from loading and leave the whole page in a spinner.
    await Promise.all([
      // 1. Bus events (stream session mode) — microbatched
      reg<BusEvent>("bus-event", (ev) => {
        dbg("middleware", "bus-event", { type: ev.type, run_id: ev.run_id });
        this._handleBusEvent(ev);
      }),

      // 2. Chat delta (pipe mode)
      reg<{ text: string }>("chat-delta", (payload) => {
        dbg("middleware", "chat-delta", { len: payload.text.length });
        this._pipeHandler?.onDelta(payload);
      }),

      // 5. Chat done (pipe mode)
      reg<{ ok: boolean; code: number; error?: string }>("chat-done", (payload) => {
        dbg("middleware", "chat-done", payload);
        this._pipeHandler?.onDone(payload);
      }),

      // 6. Run events (pipe mode stderr)
      reg<{ run_id: string; type: string; text: string }>("run-event", (payload) => {
        dbg("middleware", "run-event", { run_id: payload.run_id, type: payload.type });
        this._runEventHandler?.onRunEvent(payload);
      }),

      // 7. Hook events
      reg<HookEvent>("hook-event", (payload) => {
        dbg("middleware", "hook-event", {
          hook_type: payload.hook_type,
          tool: payload.tool_name,
        });
        this._handleHookEvent(payload);
      }),

      // 8. Hook usage
      reg<{ run_id: string; input_tokens: number; output_tokens: number; cost: number }>(
        "hook-usage",
        (payload) => {
          dbg("middleware", "hook-usage", payload);
          this._handleHookUsage(payload);
        },
      ),
    ]);

    // 9. Full reload (WS-only — Tauri transport won't emit this event)
    if (!transport.isDesktop()) {
      await reg<{ run_id: string }>("_full_reload", (payload) => {
        const runId = payload.run_id;
        dbgWarn("middleware", "_full_reload", { runId });
        if (this._reloadingRuns.has(runId)) {
          dbg("middleware", "_full_reload debounced", { runId });
          return;
        }
        const store = this._subscriptions.get(runId);
        if (store) {
          this._reloadingRuns.add(runId);
          void store.loadRun(runId).finally(() => {
            this._reloadingRuns.delete(runId);
          });
        }
      });
    }

    dbg("middleware", "all listeners registered:", ul.length);
  }

  destroy(): void {
    dbg("middleware", "destroying, unregistering", this._unlisteners.length, "listeners");
    // Unsubscribe all runs from transport before cleanup
    const transport = getTransport();
    for (const runId of this._subscriptions.keys()) {
      transport.unsubscribeRun(runId);
    }
    for (const fn of this._unlisteners) fn();
    this._unlisteners = [];
    this._subscriptions.clear();
    this._currentRunId = null;
    this._currentStore = null;
    this._batchBuffer.clear();
    if (this._lowPriorityFlushTimer !== null) {
      clearTimeout(this._lowPriorityFlushTimer);
      this._lowPriorityFlushTimer = null;
    }
    this._reloadingRuns.clear();
    this._started = false;
  }

  // ── Subscriptions ──

  /** Subscribe a store for a run_id. Clears previous subscription (single-session mode). */
  subscribeCurrent(runId: string, store: SessionStore): void {
    // Idempotent: skip if already subscribed for the same run + store.
    // Re-subscribing for the same pair would clear the batch buffer,
    // dropping in-flight events (e.g. RunState(idle) after resume).
    if (runId && this._currentRunId === runId && this._currentStore === store) {
      return;
    }

    // A delayed low-priority flush belongs to the old surface. Reset its
    // scheduler before switching runs so a new task cannot inherit the old
    // timer and appear stuck until it fires.
    if (this._currentRunId !== runId || this._currentStore !== store) {
      this._flushScheduled = false;
      if (this._lowPriorityFlushTimer !== null) {
        clearTimeout(this._lowPriorityFlushTimer);
        this._lowPriorityFlushTimer = null;
      }
    }

    // Clear old subscription (different run or different store)
    if (this._currentRunId) {
      getTransport().unsubscribeRun(this._currentRunId);
      this._subscriptions.delete(this._currentRunId);
      this._batchBuffer.delete(this._currentRunId);
    }
    if (runId) {
      this._currentRunId = runId;
      this._currentStore = store;
      this._subscriptions.set(runId, store);
    } else {
      // Empty runId = clear all (navigating to new chat)
      this._currentRunId = null;
      this._currentStore = null;
    }
    dbg("middleware", "subscribeCurrent", runId || "(cleared)");
  }

  /** Multi-session subscribe (for future subagent support). */
  subscribe(runId: string, store: SessionStore): void {
    this._subscriptions.set(runId, store);
    dbg("middleware", "subscribe", runId);
  }

  unsubscribe(runId: string, store?: SessionStore): void {
    const subscribedStore = this._subscriptions.get(runId);
    // A keyed chat surface can be replaced while the old surface is still
    // running its teardown. Do not let that stale surface remove the new
    // surface's subscription for the same run.
    if (
      store &&
      ((subscribedStore && subscribedStore !== store) ||
        (this._currentRunId === runId && this._currentStore !== store))
    ) {
      dbg("middleware", "unsubscribe skipped for stale store", runId);
      return;
    }
    getTransport().unsubscribeRun(runId);
    this._subscriptions.delete(runId);
    this._batchBuffer.delete(runId);
    if (this._currentRunId === runId) {
      this._currentRunId = null;
      this._currentStore = null;
      this._flushScheduled = false;
      if (this._lowPriorityFlushTimer !== null) {
        clearTimeout(this._lowPriorityFlushTimer);
        this._lowPriorityFlushTimer = null;
      }
    }
    dbg("middleware", "unsubscribe", runId);
  }

  // ── Handler setters ──

  setPipeHandler(handler: PipeHandler | null): void {
    this._pipeHandler = handler;
  }

  setRunEventHandler(handler: RunEventHandler | null): void {
    this._runEventHandler = handler;
  }

  /** Receives normalized bus events independently of the currently open chat. */
  setCollaborationHandler(handler: ((event: BusEvent) => void) | null): void {
    this._collaborationHandler = handler;
  }

  /**
   * Subscribe to the normalized bus stream without subscribing a session reducer.
   *
   * Work projections (Inbox/progress/subagents) use this hook to refresh their
   * durable snapshots on the same event that updates the transcript. Callers
   * should debounce their own network work when they receive token-level events.
   */
  subscribeEvents(handler: (event: BusEvent) => void): () => void {
    this._eventSubscribers.add(handler);
    return () => this._eventSubscribers.delete(handler);
  }

  // ── Internal ──

  private _handleBusEvent(ev: BusEvent): void {
    this._trackAttention(ev);
    for (const subscriber of this._eventSubscribers) {
      try {
        subscriber(ev);
      } catch (error) {
        dbgWarn("middleware", "event subscriber failed:", error);
      }
    }
    this._collaborationHandler?.(ev);

    const store = this._subscriptions.get(ev.run_id);
    if (!store) return;

    // Push to batch buffer
    let buf = this._batchBuffer.get(ev.run_id);
    if (!buf) {
      buf = [];
      this._batchBuffer.set(ev.run_id, buf);
    }
    buf.push(ev);

    // Overflow protection: flush synchronously if buffer grows too large
    if (buf.length >= this._MAX_BUFFER_SIZE) {
      dbgWarn(
        "middleware",
        `buffer overflow for ${ev.run_id} (${buf.length} events), flushing synchronously`,
      );
      this._flush();
      return;
    }

    this._scheduleFlush();
  }

  private _trackAttention(ev: BusEvent): void {
    switch (ev.type) {
      case "permission_prompt":
      case "elicitation_prompt":
        markAttention(ev.run_id, "permission");
        break;
      case "tool_end":
        if (ev.tool_name === "AskUserQuestion" && ev.status === "error") {
          markAttention(ev.run_id, "ask");
        }
        break;
      case "permission_denied":
        clearAttention(ev.run_id, "permission");
        // AskUserQuestion denied: tool_end(error) arrives first and marks ask,
        // but the question was denied, not pending — clear ask too.
        if (ev.tool_name === "AskUserQuestion") {
          clearAttention(ev.run_id, "ask");
        }
        break;
      case "control_cancelled":
        clearAttention(ev.run_id, "permission");
        break;
      case "user_message":
        clearAttention(ev.run_id, "ask");
        break;
      case "run_state":
        switch (ev.state) {
          case "spawning":
          case "idle":
            clearAttention(ev.run_id, "permission");
            break;
          case "running":
            clearAttention(ev.run_id, "ask");
            break;
          case "stopped":
          case "completed":
          case "failed":
            clearAttention(ev.run_id);
            break;
        }
        break;
    }
  }

  private _handleHookEvent(event: HookEvent): void {
    const store = this._subscriptions.get(event.run_id);
    if (!store) return;
    store.applyHookEvent(event);
  }

  private _handleHookUsage(usage: {
    run_id: string;
    input_tokens: number;
    output_tokens: number;
    cost: number;
  }): void {
    const store = this._subscriptions.get(usage.run_id);
    if (!store) return;
    store.applyHookUsage(usage);
  }

  private _scheduleFlush(): void {
    const now = performance.now();
    const idleGap = now - this._lastFlushTime;

    // Token deltas can arrive much faster than the UI can paint. Keep the first
    // token of a burst immediate, then coalesce the rest at a lower cadence so
    // Work's transcript and inspector do not consume every animation frame.
    const onlyLowPriorityEvents = [...this._batchBuffer.values()].every((events) =>
      events.every((event) => LOW_PRIORITY_EVENT_TYPES.has(event.type)),
    );

    // A control/tool event arriving while a delayed token flush is pending must
    // promote that flush immediately; otherwise it would wait behind the
    // throttling timer and make stop/permission state look stale.
    if (this._flushScheduled) {
      if (this._lowPriorityFlushTimer !== null && !onlyLowPriorityEvents) {
        clearTimeout(this._lowPriorityFlushTimer);
        this._lowPriorityFlushTimer = null;
        if (typeof requestAnimationFrame !== "undefined") {
          requestAnimationFrame(() => this._flush());
        } else {
          setTimeout(() => this._flush(), this._BATCH_INTERVAL);
        }
      }
      return;
    }

    this._flushScheduled = true;
    if (onlyLowPriorityEvents && idleGap <= this._IDLE_GAP_MS) {
      this._lowPriorityFlushTimer = setTimeout(() => {
        this._lowPriorityFlushTimer = null;
        this._flush();
      }, LOW_PRIORITY_FLUSH_INTERVAL_MS);
      return;
    }

    if (this._lowPriorityFlushTimer !== null) {
      clearTimeout(this._lowPriorityFlushTimer);
      this._lowPriorityFlushTimer = null;
    }

    // If idle for >_IDLE_GAP_MS, this is the first token of a new burst — flush
    // immediately via microtask so the user sees output without a ~16ms gap.
    if (idleGap > this._IDLE_GAP_MS) {
      queueMicrotask(() => this._flush());
    } else if (typeof requestAnimationFrame !== "undefined") {
      requestAnimationFrame(() => this._flush());
    } else {
      setTimeout(() => this._flush(), this._BATCH_INTERVAL);
    }
  }

  private _flush(): void {
    if (this._lowPriorityFlushTimer !== null) {
      clearTimeout(this._lowPriorityFlushTimer);
      this._lowPriorityFlushTimer = null;
    }
    this._flushScheduled = false;
    this._lastFlushTime = performance.now();
    for (const [runId, events] of this._batchBuffer) {
      const store = this._subscriptions.get(runId);
      if (!store) continue;
      try {
        if (events.length === 1) {
          store.applyEvent(events[0]);
        } else if (events.length > 1) {
          store.applyEventBatch(events);
        }
      } catch (e) {
        dbgWarn("middleware", `flush error for run ${runId}:`, e);
      }
    }
    this._batchBuffer.clear();
  }
}

// ── Module-level singleton ──

let _instance: EventMiddleware | null = null;

export function getEventMiddleware(): EventMiddleware {
  if (!_instance) {
    _instance = new EventMiddleware();
  }
  return _instance;
}
