import * as api from "$lib/api";
import { getEventMiddleware } from "$lib/stores/event-middleware";
import type { BusEvent } from "$lib/types";
import type { BrowserActionType, BrowserActivityView, BrowserTraceEntry } from "$lib/types/work";
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

  if (action === "fill" || action === "type") {
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
  } else if (action === "open" || action === "navigate" || action === "goto") {
    actionType = "navigate";
    const url = typeof semanticAction.url === "string" ? semanticAction.url : "";
    desc = url ? `打开 ${url}` : "页面导航";
  }

  return { actionType, description: desc, selector };
}

export interface ScreenshotLocation {
  path: string;
  cwd?: string | null;
}

export function extractScreenshotInfoFromOutput(
  output: Record<string, unknown> | null | undefined,
): ScreenshotLocation | null {
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
          const data = itemObj.data.startsWith("data:image/")
            ? itemObj.data
            : `data:${mime};base64,${itemObj.data}`;
          return { path: data };
        }
      }
    }
  }

  // 2. Check details / tool_use_result
  const details = (output.details ?? output) as Record<string, unknown>;
  const baseCwd = typeof details.cwd === "string" && details.cwd.length > 0 ? details.cwd : null;

  if (Array.isArray(details.screenshots) && details.screenshots.length > 0) {
    const last = details.screenshots[details.screenshots.length - 1];
    if (typeof last === "string" && last.length > 0) {
      return { path: last, cwd: baseCwd };
    }
  }

  if (Array.isArray(details.artifacts)) {
    for (const art of details.artifacts) {
      if (art && typeof art === "object") {
        const artObj = art as Record<string, unknown>;
        if (
          artObj.kind === "screenshot" ||
          artObj.kind === "image" ||
          artObj.mediaType === "image/png"
        ) {
          const artPath =
            typeof artObj.absolutePath === "string" && artObj.absolutePath.length > 0
              ? artObj.absolutePath
              : typeof artObj.path === "string" && artObj.path.length > 0
                ? artObj.path
                : null;
          if (artPath) {
            const artCwd =
              typeof artObj.cwd === "string" && artObj.cwd.length > 0 ? artObj.cwd : baseCwd;
            return { path: artPath, cwd: artCwd };
          }
        }
      }
    }
  }

  if (typeof details.imagePath === "string" && details.imagePath.length > 0) {
    return { path: details.imagePath, cwd: baseCwd };
  }

  if (Array.isArray(details.imagePaths) && details.imagePaths.length > 0) {
    const last = details.imagePaths[details.imagePaths.length - 1];
    if (typeof last === "string" && last.length > 0) {
      return { path: last, cwd: baseCwd };
    }
  }

  return null;
}

function extractScreenshotFromOutput(
  output: Record<string, unknown> | null | undefined,
): string | null {
  const info = extractScreenshotInfoFromOutput(output);
  return info?.path ?? null;
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
    const input = (event.input ?? {}) as Record<string, unknown>;
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
    } else if (
      event.tool_name === "agent_browser_electron" ||
      (input.electron && typeof input.electron === "object")
    ) {
      actionType = "custom";
      const el = (
        input.electron && typeof input.electron === "object" ? input.electron : input
      ) as Record<string, unknown>;
      description =
        typeof el.action === "string" ? `Electron: ${el.action}` : "Electron 应用自动化";
    } else if (event.tool_name === "agent_browser_tools" || Array.isArray(input.enable)) {
      actionType = "custom";
      const enable = Array.isArray(input.enable) ? input.enable.join(", ") : "";
      description = enable ? `启用浏览器工具: ${enable}` : "查询浏览器工具列表";
    } else if (
      event.tool_name === "agent_browser_code" ||
      typeof input.code === "string" ||
      typeof input.script === "string"
    ) {
      actionType = "custom";
      description = "执行浏览器自动化脚本";
    } else if (
      event.tool_name === "agent_browser_qa" ||
      (input.qa && typeof input.qa === "object")
    ) {
      actionType = "navigate";
      const qa = (input.qa && typeof input.qa === "object" ? input.qa : input) as Record<
        string,
        unknown
      >;
      targetUrl = typeof qa.url === "string" ? qa.url : null;
      description = targetUrl ? `QA 页面校验: ${targetUrl}` : "QA 页面校验";
    } else if (typeof input.action === "string") {
      const act = input.action.toLowerCase();
      actionType = parseActionTypeFromArgs(act);
      selector = typeof input.selector === "string" ? input.selector : null;
      if (act === "fill" || act === "type") {
        const text = typeof input.text === "string" ? sanitizeTraceText(input.text) : "";
        description = selector ? `在 ${selector} 输入 "${text}"` : `输入 "${text}"`;
      } else if (act === "click" || act === "check") {
        actionType = "click";
        description = selector ? `点击: ${selector}` : "点击元素";
      } else if (act === "select") {
        actionType = "select_option";
        const val = input.value ?? input.values;
        description = `选择: ${Array.isArray(val) ? val.join(", ") : String(val ?? "")}`;
      } else if (act === "open" || act === "navigate" || act === "goto") {
        actionType = "navigate";
        targetUrl = typeof input.url === "string" ? input.url : null;
        description = targetUrl ? `打开 ${targetUrl}` : "页面导航";
      } else {
        description = `${act} ${selector ?? ""}`.trim();
      }
    } else if (input.job && typeof input.job === "object") {
      actionType = "custom";
      const jobObj = input.job as Record<string, unknown>;
      const steps = Array.isArray(jobObj.steps) ? jobObj.steps.length : 0;
      description = `批处理任务 (${steps} 步)`;
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
    const statusLower = String(event.status ?? "").toLowerCase();
    const out = (event.output ?? {}) as Record<string, unknown>;
    const details =
      ((out.details ?? event.tool_use_result?.details ?? out) as Record<string, unknown>) ?? {};

    const isError =
      statusLower === "failed" || statusLower === "error" || details.succeeded === false;

    // Extract updated URL / Title
    const pageChangeSummary = (
      details.pageChangeSummary && typeof details.pageChangeSummary === "object"
        ? details.pageChangeSummary
        : null
    ) as Record<string, unknown> | null;
    const sessionTabTarget = (
      details.sessionTabTarget && typeof details.sessionTabTarget === "object"
        ? details.sessionTabTarget
        : null
    ) as Record<string, unknown> | null;
    const refSnapshotPage = (
      details.refSnapshot &&
      typeof details.refSnapshot === "object" &&
      (details.refSnapshot as Record<string, unknown>).page &&
      typeof (details.refSnapshot as Record<string, unknown>).page === "object"
        ? (details.refSnapshot as Record<string, unknown>).page
        : null
    ) as Record<string, unknown> | null;

    const newUrl =
      typeof details.url === "string" && details.url.length > 0
        ? details.url
        : typeof pageChangeSummary?.url === "string" && pageChangeSummary.url.length > 0
          ? pageChangeSummary.url
          : typeof sessionTabTarget?.url === "string" && sessionTabTarget.url.length > 0
            ? sessionTabTarget.url
            : typeof details.targetUrl === "string" && details.targetUrl.length > 0
              ? details.targetUrl
              : typeof details.pageUrl === "string" && details.pageUrl.length > 0
                ? details.pageUrl
                : typeof refSnapshotPage?.url === "string" && refSnapshotPage.url.length > 0
                  ? String(refSnapshotPage.url)
                  : null;

    const newTitle =
      typeof details.title === "string" && details.title.length > 0
        ? details.title
        : typeof pageChangeSummary?.title === "string" && pageChangeSummary.title.length > 0
          ? pageChangeSummary.title
          : typeof sessionTabTarget?.title === "string" && sessionTabTarget.title.length > 0
            ? sessionTabTarget.title
            : typeof details.pageTitle === "string" && details.pageTitle.length > 0
              ? details.pageTitle
              : typeof refSnapshotPage?.title === "string" && refSnapshotPage.title.length > 0
                ? String(refSnapshotPage.title)
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
    const stateLower = String(event.state ?? "").toLowerCase();
    if (stateLower === "completed" || stateLower === "success") {
      return {
        ...current,
        status: "completed",
        currentAction: null,
        updatedAt: new Date().toISOString(),
      };
    }
    if (stateLower === "error" || stateLower === "failed") {
      return {
        ...current,
        status: "failed",
        currentAction: null,
        lastError: event.error ?? current.lastError,
        updatedAt: new Date().toISOString(),
      };
    }
    if (stateLower === "idle" && current.status === "running") {
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
  private cwdByRunId = new Map<string, string>();
  private inFlightCwdResolutions = new Map<string, Promise<string>>();
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

  async getRunCwd(runId: string): Promise<string> {
    const existing = this.cwdByRunId.get(runId);
    if (existing) return existing;

    const inFlight = this.inFlightCwdResolutions.get(runId);
    if (inFlight) return inFlight;

    const promise = (async () => {
      try {
        const events = await api.getBusEvents(runId);
        for (const ev of events) {
          if (
            ev.type === "session_init" &&
            typeof (ev as { cwd?: string }).cwd === "string" &&
            (ev as { cwd?: string }).cwd!.length > 0
          ) {
            const cwd = (ev as { cwd: string }).cwd;
            this.cwdByRunId.set(runId, cwd);
            return cwd;
          }
        }
      } catch (err) {
        console.warn(`[BrowserActivityStore] Failed to resolve cwd for run ${runId}:`, err);
      }
      return "";
    })();

    this.inFlightCwdResolutions.set(runId, promise);
    try {
      return await promise;
    } finally {
      this.inFlightCwdResolutions.delete(runId);
    }
  }

  private async resolveScreenshot(
    runId: string,
    filePath: string,
    artifactCwd?: string | null,
    toolUseId?: string,
  ): Promise<void> {
    if (
      !filePath ||
      filePath.startsWith("data:") ||
      filePath.startsWith("http://") ||
      filePath.startsWith("https://")
    ) {
      return;
    }
    try {
      let cwd = artifactCwd || this.cwdByRunId.get(runId);
      if (!cwd) {
        cwd = await this.getRunCwd(runId);
      }
      if (!cwd) {
        console.warn(
          `[BrowserActivityStore] Skipping screenshot resolution for ${filePath}: missing cwd for run ${runId}`,
        );
        return;
      }
      const [base64, mime] = await api.readFileBase64(filePath, cwd);
      if (!base64) return;
      const dataUrl = `data:${mime || "image/png"};base64,${base64}`;
      const current = this.activityByRunId[runId];
      if (!current) return;

      const updatedTraces = current.traces.map((trace) => {
        if (
          (trace as ExtendedTraceEntry).toolUseId === toolUseId ||
          trace.screenshotData === filePath
        ) {
          return { ...trace, screenshotData: dataUrl };
        }
        return trace;
      });

      this.activityByRunId = {
        ...this.activityByRunId,
        [runId]: {
          ...current,
          lastScreenshot: current.lastScreenshot === filePath ? dataUrl : current.lastScreenshot,
          traces: updatedTraces,
        },
      };
    } catch (err) {
      console.warn(
        `[BrowserActivityStore] Failed to resolve screenshot base64 for ${filePath}:`,
        err,
      );
    }
  }

  handleBusEvent(event: BusEvent): void {
    if (!event.run_id) return;
    if (
      event.type === "session_init" &&
      typeof (event as { cwd?: string }).cwd === "string" &&
      (event as { cwd?: string }).cwd!.length > 0
    ) {
      this.cwdByRunId.set(event.run_id, (event as { cwd: string }).cwd);
    }
    if (event.type === "tool_start" || event.type === "tool_end" || event.type === "run_state") {
      const prev = this.activityByRunId[event.run_id] ?? null;
      const next = projectAgentBrowserActivity(prev, event);
      this.activityByRunId = {
        ...this.activityByRunId,
        [event.run_id]: next,
      };

      if (
        event.type === "tool_end" &&
        next.lastScreenshot &&
        !next.lastScreenshot.startsWith("data:") &&
        !next.lastScreenshot.startsWith("http")
      ) {
        const out = (event.output ?? {}) as Record<string, unknown>;
        const info =
          extractScreenshotInfoFromOutput(out) ??
          extractScreenshotInfoFromOutput(event.tool_use_result);
        if (info?.cwd) {
          this.cwdByRunId.set(event.run_id, info.cwd);
        }
        void this.resolveScreenshot(
          event.run_id,
          next.lastScreenshot,
          info?.cwd,
          event.tool_use_id,
        );
      }
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
        let lastCwd: string | undefined;
        for (const ev of events) {
          if (
            ev.type === "session_init" &&
            typeof (ev as { cwd?: string }).cwd === "string" &&
            (ev as { cwd?: string }).cwd!.length > 0
          ) {
            this.cwdByRunId.set(runId, (ev as { cwd: string }).cwd);
            lastCwd = (ev as { cwd: string }).cwd;
          }
          projection = projectAgentBrowserActivity(projection, ev);
          if (ev.type === "tool_end") {
            const out = (ev.output ?? {}) as Record<string, unknown>;
            const info =
              extractScreenshotInfoFromOutput(out) ??
              extractScreenshotInfoFromOutput(ev.tool_use_result);
            if (info?.cwd) {
              this.cwdByRunId.set(runId, info.cwd);
              lastCwd = info.cwd;
            }
          }
        }
        if (projection) {
          if (
            projection.lastScreenshot &&
            !projection.lastScreenshot.startsWith("data:") &&
            !projection.lastScreenshot.startsWith("http")
          ) {
            void this.resolveScreenshot(runId, projection.lastScreenshot, lastCwd);
          }
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
