import * as api from "$lib/api";
import { getEventMiddleware } from "$lib/stores/event-middleware";
import type { BusEvent } from "$lib/types";
import type {
  BrowserActionType,
  BrowserActivityView,
  BrowserSessionStatus,
  BrowserTraceEntry,
} from "$lib/types/work";
import { isInteractiveBrowserToolName, sanitizeTraceText } from "$lib/utils/work-browser";

interface ExtendedTraceEntry extends BrowserTraceEntry {
  toolUseId?: string;
}

function parseActionTypeFromArgs(command: string): BrowserActionType {
  switch (command.toLowerCase()) {
    case "open":
    case "navigate":
    case "goto":
      return "navigate";
    case "click":
    case "tap":
      return "click";
    case "fill":
    case "type":
    case "inserttext":
      return "type";
    case "snapshot":
      return "snapshot";
    case "screenshot":
      return "screenshot";
    case "select":
      return "select_option";
    case "scroll":
    case "scrollintoview":
      return "scroll";
    case "wait":
      return "wait_for";
    case "tab":
    case "tabs":
    case "window":
      return "tabs";
    case "close":
    case "quit":
    case "exit":
      return "close";
    default:
      return "custom";
  }
}

function formatSemanticActionDescription(semanticAction: Record<string, unknown>): {
  actionType: BrowserActionType;
  description: string;
  selector: string | null;
} {
  const action = String(semanticAction.action ?? "click").toLowerCase();
  const selector =
    typeof semanticAction.selector === "string"
      ? semanticAction.selector
      : typeof semanticAction.name === "string"
        ? semanticAction.name
        : typeof semanticAction.value === "string"
          ? semanticAction.value
          : null;

  let actionType: BrowserActionType = "click";
  let desc = "点击目标元素";

  if (action === "fill") {
    actionType = "type";
    const text = typeof semanticAction.text === "string" ? semanticAction.text : "";
    desc = `输入: "${sanitizeTraceText(text)}"`;
  } else if (action === "select") {
    actionType = "select_option";
    const val = semanticAction.value ?? semanticAction.values;
    desc = `选择: ${Array.isArray(val) ? val.join(", ") : String(val ?? "")}`;
  } else if (action === "check" || action === "click") {
    actionType = "click";
    desc = `点击: ${selector || "元素"}`;
  }

  return { actionType, description: desc, selector };
}

function extractScreenshotFromOutput(
  output: Record<string, unknown> | null | undefined,
): string | null {
  if (!output) return null;

  // 1. Check content array (Pi ToolContent format: [{ type: "image", data: "...", mimeType: "image/png" }])
  if (Array.isArray(output.content)) {
    for (const item of output.content) {
      if (item && typeof item === "object") {
        const itemObj = item as Record<string, unknown>;
        if (
          itemObj.type === "image" &&
          typeof itemObj.data === "string" &&
          itemObj.data.length > 0
        ) {
          const mime = String(itemObj.mimeType ?? itemObj.mime_type ?? "image/png");
          if (itemObj.data.startsWith("data:image/")) {
            return itemObj.data;
          }
          return `data:${mime};base64,${itemObj.data}`;
        }
      }
    }
  }

  // 2. Check details / tool_use_result
  const details = (output.details ?? output) as Record<string, unknown>;
  if (Array.isArray(details.screenshots) && details.screenshots.length > 0) {
    const last = details.screenshots[details.screenshots.length - 1];
    if (typeof last === "string" && last.length > 0) {
      return last;
    }
  }

  if (Array.isArray(details.artifacts)) {
    for (const art of details.artifacts) {
      if (art && typeof art === "object") {
        const artObj = art as Record<string, unknown>;
        if (
          (artObj.kind === "screenshot" || artObj.mediaType === "image/png") &&
          typeof artObj.path === "string"
        ) {
          return artObj.path;
        }
      }
    }
  }

  return null;
}

/**
 * Projects a single BusEvent into the target run's BrowserActivityView.
 */
export function projectAgentBrowserActivity(
  prev: BrowserActivityView | null,
  event: BusEvent,
): BrowserActivityView {
  const current: BrowserActivityView = prev ?? {
    runId: event.run_id,
    status: "idle",
    currentUrl: null,
    pageTitle: null,
    currentAction: null,
    lastScreenshot: null,
    traces: [],
    lastError: null,
    updatedAt: new Date().toISOString(),
  };

  if (event.type === "tool_start" && isInteractiveBrowserToolName(event.tool_name)) {
    const input = event.input ?? {};
    let actionType: BrowserActionType = "custom";
    let description = event.tool_name;
    let targetUrl: string | null = null;
    let selector: string | null = null;

    if (Array.isArray(input.args) && input.args.length > 0) {
      const args = input.args.map((a) => String(a));
      const cmd = args[0];
      actionType = parseActionTypeFromArgs(cmd);

      if (actionType === "navigate") {
        targetUrl = args[1] ?? null;
        description = targetUrl ? `打开 ${targetUrl}` : "导航到页面";
      } else if (actionType === "click") {
        selector = args[1] ?? null;
        description = selector ? `点击 ${selector}` : "点击元素";
      } else if (actionType === "type") {
        selector = args[1] ?? null;
        const text = args[2] ? sanitizeTraceText(args[2]) : "";
        description = selector ? `在 ${selector} 输入 "${text}"` : `输入 "${text}"`;
      } else if (actionType === "snapshot") {
        description = "读取页面快照";
      } else if (actionType === "screenshot") {
        description = "截取页面快照";
      } else if (actionType === "scroll") {
        description = `页面滚动: ${args.slice(1).join(" ")}`;
      } else if (actionType === "wait_for") {
        description = `等待: ${args.slice(1).join(" ")}`;
      } else if (actionType === "select_option") {
        selector = args[1] ?? null;
        description = `选择选项: ${args.slice(2).join(", ")}`;
      } else if (actionType === "tabs") {
        description = `标签页管理: ${args.slice(1).join(" ")}`;
      } else if (actionType === "close") {
        description = "关闭浏览器";
      } else {
        description = args.join(" ");
      }
    } else if (input.semanticAction && typeof input.semanticAction === "object") {
      const parsed = formatSemanticActionDescription(
        input.semanticAction as Record<string, unknown>,
      );
      actionType = parsed.actionType;
      description = parsed.description;
      selector = parsed.selector;
    } else if (input.qa && typeof input.qa === "object") {
      const qa = input.qa as Record<string, unknown>;
      actionType = "navigate";
      targetUrl = typeof qa.url === "string" ? qa.url : null;
      description = targetUrl ? `QA 页面校验: ${targetUrl}` : "QA 页面校验";
    } else if (input.job && typeof input.job === "object") {
      actionType = "custom";
      const jobObj = input.job as Record<string, unknown>;
      const steps = Array.isArray(jobObj.steps) ? jobObj.steps.length : 0;
      description = `批处理任务 (${steps} 步)`;
    } else if (typeof input.script === "string") {
      actionType = "custom";
      description = "执行浏览器自动化脚本";
    }

    const newTrace: ExtendedTraceEntry = {
      stepIndex: current.traces.length + 1,
      actionType,
      description,
      targetUrl,
      selector,
      status: "started",
      durationMs: 0,
      timestamp: new Date().toISOString(),
      toolUseId: event.tool_use_id,
    };

    return {
      ...current,
      status: "running",
      currentAction: description,
      currentUrl: targetUrl || current.currentUrl,
      traces: [...current.traces, newTrace],
      updatedAt: new Date().toISOString(),
    };
  }

  if (event.type === "tool_end" && isInteractiveBrowserToolName(event.tool_name)) {
    const isError = event.status === "error";
    const out = event.output ?? {};
    const details =
      ((out.details ?? event.tool_use_result?.details ?? out) as Record<string, unknown>) ?? {};

    // Extract updated URL / Title
    const newUrl =
      typeof details.url === "string"
        ? details.url
        : typeof details.targetUrl === "string"
          ? details.targetUrl
          : typeof details.pageUrl === "string"
            ? details.pageUrl
            : (details.refSnapshot as Record<string, unknown>)?.page &&
                typeof (
                  (details.refSnapshot as Record<string, unknown>).page as Record<string, unknown>
                ).url === "string"
              ? String(
                  ((details.refSnapshot as Record<string, unknown>).page as Record<string, unknown>)
                    .url,
                )
              : null;

    const newTitle =
      typeof details.title === "string"
        ? details.title
        : typeof details.pageTitle === "string"
          ? details.pageTitle
          : (details.refSnapshot as Record<string, unknown>)?.page &&
              typeof (
                (details.refSnapshot as Record<string, unknown>).page as Record<string, unknown>
              ).title === "string"
            ? String(
                ((details.refSnapshot as Record<string, unknown>).page as Record<string, unknown>)
                  .title,
              )
            : null;

    // Extract screenshot
    const screenshot =
      extractScreenshotFromOutput(out) ?? extractScreenshotFromOutput(event.tool_use_result);

    // Extract error
    const rawError =
      details.error ?? details.errors ?? details.failureCategory ?? (isError ? out.error : null);
    const errorMessage = rawError ? String(rawError) : null;

    // Update traces
    const updatedTraces = [...current.traces];
    const matchIdx = updatedTraces.findIndex(
      (t) => (t as ExtendedTraceEntry).toolUseId === event.tool_use_id,
    );

    if (matchIdx !== -1) {
      updatedTraces[matchIdx] = {
        ...updatedTraces[matchIdx],
        status: isError ? "failed" : "success",
        durationMs: event.duration_ms ?? 0,
        error: errorMessage,
        screenshotData: screenshot || updatedTraces[matchIdx].screenshotData,
      };
    } else {
      updatedTraces.push({
        stepIndex: updatedTraces.length + 1,
        actionType: "custom",
        description: event.tool_name,
        status: isError ? "failed" : "success",
        durationMs: event.duration_ms ?? 0,
        error: errorMessage,
        screenshotData: screenshot,
        timestamp: new Date().toISOString(),
      });
    }

    return {
      ...current,
      status: isError ? "failed" : "idle",
      currentAction: null,
      currentUrl: newUrl || current.currentUrl,
      pageTitle: newTitle || current.pageTitle,
      lastScreenshot: screenshot || current.lastScreenshot,
      lastError: errorMessage ?? (isError ? current.lastError : null),
      traces: updatedTraces,
      updatedAt: new Date().toISOString(),
    };
  }

  if (event.type === "run_state") {
    if (event.state === "completed") {
      return {
        ...current,
        status: "completed",
        currentAction: null,
        updatedAt: new Date().toISOString(),
      };
    }
    if (event.state === "error" || event.state === "failed") {
      return {
        ...current,
        status: "failed",
        currentAction: null,
        lastError: event.error ?? current.lastError,
        updatedAt: new Date().toISOString(),
      };
    }
    if (event.state === "idle" && current.status === "running") {
      return {
        ...current,
        status: "idle",
        currentAction: null,
        updatedAt: new Date().toISOString(),
      };
    }
  }

  return current;
}

export class BrowserActivityStore {
  activityByRunId = $state<Record<string, BrowserActivityView>>({});
  private loadedRuns = new Set<string>();
  private inFlightLoads = new Map<string, Promise<BrowserActivityView | null>>();
  private unsubscribeMiddleware: (() => void) | null = null;

  constructor() {
    this.initSubscriber();
  }

  private initSubscriber() {
    const middleware = getEventMiddleware();
    this.unsubscribeMiddleware = middleware.subscribeEvents((event: BusEvent) => {
      this.handleBusEvent(event);
    });
  }

  getActivity(runId: string | null | undefined): BrowserActivityView | null {
    if (!runId) return null;
    return this.activityByRunId[runId] ?? null;
  }

  handleBusEvent(event: BusEvent): void {
    if (!event.run_id) return;
    if (event.type === "tool_start" || event.type === "tool_end" || event.type === "run_state") {
      const prev = this.activityByRunId[event.run_id] ?? null;
      const next = projectAgentBrowserActivity(prev, event);
      this.activityByRunId = {
        ...this.activityByRunId,
        [event.run_id]: next,
      };
    }
  }

  async loadActivity(runId: string): Promise<BrowserActivityView | null> {
    if (!runId) return null;
    if (this.loadedRuns.has(runId)) {
      return this.activityByRunId[runId] ?? null;
    }
    const inFlight = this.inFlightLoads.get(runId);
    if (inFlight) return inFlight;

    const promise = (async () => {
      try {
        const events = await api.getBusEvents(runId);
        let projection: BrowserActivityView | null = null;
        for (const ev of events) {
          projection = projectAgentBrowserActivity(projection, ev);
        }
        if (projection) {
          this.activityByRunId = {
            ...this.activityByRunId,
            [runId]: projection,
          };
        }
        this.loadedRuns.add(runId);
        return projection;
      } catch (err) {
        console.warn(`[BrowserActivityStore] Failed to load bus events for ${runId}:`, err);
        return this.activityByRunId[runId] ?? null;
      } finally {
        this.inFlightLoads.delete(runId);
      }
    })();

    this.inFlightLoads.set(runId, promise);
    return promise;
  }

  destroy() {
    this.unsubscribeMiddleware?.();
    this.unsubscribeMiddleware = null;
  }
}

export const browserActivityStore = new BrowserActivityStore();
