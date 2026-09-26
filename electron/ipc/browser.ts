/**
 * Main-process ownership boundary for embedded browser tab groups.
 *
 * A renderer attaches one run-scoped surface. Electron owns every native page
 * view in that surface and exposes an authenticated CDP relay per tab. The
 * Rust registry points the Browser Worker at the active tab.
 */
import { dialog, ipcMain, type BrowserWindow } from "electron";
import { EmbeddedBrowserView, windowViewport } from "../browser-view";
import {
  BrowserRelayServer,
  createRelayToken,
  debuggerTransport,
  type RelayEndpoint,
} from "../browser-relay-server";
import { normalizeSurfaceBounds, type Rect } from "../browser-view-geometry";

type Endpoint = Omit<RelayEndpoint, "host"> & { host: "127.0.0.1" };

interface BrowserTabRecord {
  viewId: string;
  targetId: string;
  view: EmbeddedBrowserView;
  relay: BrowserRelayServer;
  endpoint: Endpoint;
}

interface BrowserGroup {
  viewId: string;
  runId: string;
  surfaceBindingId: string;
  window: BrowserWindow;
  bounds: Rect;
  visible: boolean;
  activeTargetId: string;
  nextTabSequence: number;
  registrationEndpoint: string;
  registrationToken: string;
  invokeCore?: (method: string, params?: unknown) => Promise<unknown>;
  tabs: Map<string, BrowserTabRecord>;
  tabsByViewId: Map<string, BrowserTabRecord>;
}

interface BrowserTabSummary {
  id: string;
  targetId: string;
  index: number;
  url: string;
  title: string;
  active: boolean;
}

interface Registry {
  windows: Map<string, BrowserWindow>;
  views: Map<string, EmbeddedBrowserView>;
  endpoints: Map<string, Endpoint>;
  relays: Map<string, BrowserRelayServer>;
  groups: Map<string, BrowserGroup>;
  groupsByRunId: Map<string, BrowserGroup>;
  tabGroups: Map<string, BrowserGroup>;
  viewOperations: Map<string, Promise<void>>;
}

const registry: Registry = {
  windows: new Map(),
  views: new Map(),
  endpoints: new Map(),
  relays: new Map(),
  groups: new Map(),
  groupsByRunId: new Map(),
  tabGroups: new Map(),
  viewOperations: new Map(),
};

async function serializeViewOperation<T>(viewId: string, operation: () => Promise<T>): Promise<T> {
  const previous = registry.viewOperations.get(viewId) ?? Promise.resolve();
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const current = previous.catch(() => {}).then(() => gate);
  registry.viewOperations.set(viewId, current);
  await previous.catch(() => {});
  try {
    return await operation();
  } finally {
    release();
    if (registry.viewOperations.get(viewId) === current) registry.viewOperations.delete(viewId);
  }
}

const BROWSER_PARTITION = "persist:agentcabin-browser";

function log(message: string, detail?: unknown): void {
  // eslint-disable-next-line no-console
  console.warn(`[browser-ipc] ${message}`, detail ?? "");
}

function browserTabSummaries(group: BrowserGroup): BrowserTabSummary[] {
  return [...group.tabs.values()].map((tab, index) => {
    const state = tab.view.getState();
    return {
      id: tab.targetId,
      targetId: tab.targetId,
      index,
      url: state.url,
      title: state.title,
      active: tab.targetId === group.activeTargetId,
    };
  });
}

function emitTabs(group: BrowserGroup): BrowserTabSummary[] {
  const tabs = browserTabSummaries(group);
  if (!group.window.isDestroyed()) {
    group.window.webContents.send("browser:tabs-changed", {
      viewId: group.viewId,
      runId: group.runId,
      activeTargetId: group.activeTargetId,
      tabs,
    });
  }
  return tabs;
}

function activeTab(group: BrowserGroup): BrowserTabRecord | undefined {
  return group.tabs.get(group.activeTargetId) ?? group.tabs.values().next().value;
}

async function registerGroup(group: BrowserGroup): Promise<void> {
  if (!group.invokeCore) return;
  const tabs = [...group.tabs.values()];
  const anchor = tabs[0];
  if (!anchor) return;
  const payload = {
    endpoint: group.registrationEndpoint,
    token: group.registrationToken,
    targets: tabs.map((tab) => ({
      targetId: tab.targetId,
      runId: group.runId,
      url: tab.view.getState().url,
      endpoint: `${tab.endpoint.host}:${tab.endpoint.port}`,
      token: tab.endpoint.token,
      active: tab.targetId === group.activeTargetId,
    })),
  };
  await group.invokeCore("register_embedded_browser", { payload });
}

async function disposeTab(group: BrowserGroup, tab: BrowserTabRecord): Promise<void> {
  group.tabs.delete(tab.targetId);
  group.tabsByViewId.delete(tab.viewId);
  registry.views.delete(tab.viewId);
  registry.windows.delete(tab.viewId);
  registry.endpoints.delete(tab.viewId);
  registry.relays.delete(tab.viewId);
  registry.tabGroups.delete(tab.viewId);
  tab.view.destroy();
  await tab.relay.close();
}

async function destroyGroup(group: BrowserGroup): Promise<void> {
  if (registry.groups.get(group.viewId) === group) registry.groups.delete(group.viewId);
  if (registry.groupsByRunId.get(group.runId) === group) {
    registry.groupsByRunId.delete(group.runId);
  }
  for (const tab of [...new Set(group.tabsByViewId.values())]) await disposeTab(group, tab);
  if (group.invokeCore) {
    try {
      await group.invokeCore("unregister_embedded_browser", {
        endpoint: group.registrationEndpoint,
      });
    } catch (error) {
      log("failed to unregister embedded browser from core", error);
    }
  }
}

async function disposeView(viewId: string): Promise<void> {
  const group = registry.groups.get(viewId);
  if (!group) {
    const view = registry.views.get(viewId);
    const relay = registry.relays.get(viewId);
    view?.destroy();
    registry.views.delete(viewId);
    registry.windows.delete(viewId);
    registry.endpoints.delete(viewId);
    registry.relays.delete(viewId);
    registry.tabGroups.delete(viewId);
    await relay?.close();
    return;
  }

  await destroyGroup(group);
}

export function browserViewsForWindow(window: BrowserWindow): EmbeddedBrowserView[] {
  return [...registry.views.entries()]
    .filter(([viewId]) => registry.windows.get(viewId) === window)
    .map(([, view]) => view);
}

export async function destroyBrowserViews(): Promise<void> {
  await Promise.all(
    [...registry.groupsByRunId.keys()].map((runId) =>
      serializeViewOperation(runId, async () => {
        const group = registry.groupsByRunId.get(runId);
        if (group) await destroyGroup(group);
      }),
    ),
  );
}

export async function destroyBrowserGroup(runId: string): Promise<void> {
  await serializeViewOperation(runId, async () => {
    const group = registry.groupsByRunId.get(runId);
    if (group) await destroyGroup(group);
  });
}

export async function destroyBrowserViewsForWindow(window: BrowserWindow): Promise<void> {
  await Promise.all(
    [...registry.groupsByRunId.keys()].map((runId) =>
      serializeViewOperation(runId, async () => {
        const group = registry.groupsByRunId.get(runId);
        if (group?.window === window) await destroyGroup(group);
      }),
    ),
  );
}

async function activateTab(group: BrowserGroup, targetId: string): Promise<void> {
  const tab = group.tabs.get(targetId);
  if (!tab) throw new Error(`Browser tab '${targetId}' is not open`);
  group.activeTargetId = targetId;
  for (const [id, item] of group.tabs) {
    item.view.setBounds(group.bounds);
    item.view.setVisible(group.visible && id === targetId);
  }
  await registerGroup(group);
  emitTabs(group);
}

async function createTab(group: BrowserGroup, url = "about:blank"): Promise<BrowserTabRecord> {
  const viewId = `${group.viewId}::tab-${++group.nextTabSequence}`;
  const relay = new BrowserRelayServer(log);
  const view = new EmbeddedBrowserView(group.window, {
    viewId,
    partition: BROWSER_PARTITION,
    allowedHosts: [],
    onStateChange: (state) => {
      if (!group.window.isDestroyed()) {
        group.window.webContents.send("browser:state-changed", state);
      }
      emitTabs(group);
    },
    onNavigationAsk: async (host, targetUrl) => {
      const { response } = await dialog.showMessageBox(group.window, {
        type: "question",
        buttons: ["打开", "取消"],
        defaultId: 0,
        cancelId: 1,
        message: `允许在 AgentCabin 内置浏览器中打开 ${host}？`,
        detail: targetUrl,
      });
      return response === 0;
    },
    log,
  });
  view.setBounds(group.bounds);
  view.setVisible(false);
  group.tabsByViewId.set(viewId, {
    viewId,
    targetId: "",
    view,
    relay,
    endpoint: { host: "127.0.0.1", port: 0, token: "", targetId: "" },
  });
  registry.views.set(viewId, view);
  registry.windows.set(viewId, group.window);
  registry.relays.set(viewId, relay);
  registry.tabGroups.set(viewId, group);

  try {
    // Initial navigation is awaited before attaching the debugger, avoiding
    // Chromium's pending Target.getTargetInfo race on a blank target.
    await view.loadURL(url || "about:blank");
    const token = createRelayToken();
    const baseTransport = debuggerTransport(view.webContents);
    const transport = {
      sendCommand: (method: string, params?: Record<string, unknown>) => {
        if (method === "AgentCabin.browserTabs") {
          return performTabAction(group, params ?? {});
        }
        return baseTransport.sendCommand(method, params);
      },
      onEvent: (handler: (method: string, params: unknown) => void) =>
        baseTransport.onEvent(handler),
    };
    const port = await relay.listen(token, transport);
    const targetId = await view.resolveTargetId();
    const endpoint: Endpoint = { host: "127.0.0.1", port, token, targetId };
    const tab: BrowserTabRecord = {
      viewId,
      targetId,
      view,
      relay,
      endpoint,
    };
    group.tabsByViewId.set(viewId, tab);
    group.tabs.set(targetId, tab);
    registry.endpoints.set(viewId, endpoint);
    if (!group.registrationEndpoint) {
      group.registrationEndpoint = `${endpoint.host}:${endpoint.port}`;
      group.registrationToken = endpoint.token;
    }
    return tab;
  } catch (error) {
    group.tabsByViewId.delete(viewId);
    registry.views.delete(viewId);
    registry.windows.delete(viewId);
    registry.relays.delete(viewId);
    registry.tabGroups.delete(viewId);
    view.destroy();
    await relay.close();
    throw error;
  }
}

async function performTabAction(
  group: BrowserGroup,
  params: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const action = typeof params.action === "string" ? params.action : "list";
  if (action === "new") {
    const url = typeof params.url === "string" ? params.url : "about:blank";
    const tab = await createTab(group, url);
    await activateTab(group, tab.targetId);
  } else if (action === "switch") {
    const targetId = String(params.targetId ?? params.target_id ?? params.id ?? "");
    const index =
      typeof params.index === "number" && Number.isInteger(params.index) ? params.index : -1;
    const target = targetId ? group.tabs.get(targetId) : [...group.tabs.values()][index];
    if (!target) throw new Error("Browser tab switch requires an open targetId or valid index");
    await activateTab(group, target.targetId);
  } else if (action === "close") {
    const requestedId = params.targetId ?? params.target_id ?? params.id;
    const requestedIndex =
      typeof params.index === "number" && Number.isInteger(params.index) ? params.index : -1;
    const indexedTarget =
      requestedIndex >= 0 ? [...group.tabs.values()][requestedIndex] : undefined;
    if (requestedIndex >= 0 && !indexedTarget && requestedId === undefined) {
      throw new Error(`Browser tab index ${requestedIndex} is not open`);
    }
    const targetId = String(requestedId ?? indexedTarget?.targetId ?? group.activeTargetId);
    const tab = group.tabs.get(targetId);
    if (!tab) throw new Error(`Browser tab '${targetId}' is not open`);
    if (group.tabs.size <= 1) {
      throw new Error(
        "Cannot close the only browser tab; use browser_close to close this session.",
      );
    }
    const tabs = [...group.tabs.values()];
    const closedIndex = tabs.findIndex((item) => item.targetId === tab.targetId);
    const fallback = tabs[closedIndex - 1] ?? tabs[closedIndex + 1];
    if (group.activeTargetId === targetId) {
      // The close request itself is travelling over this tab's relay. Keep
      // that socket alive until its response has been written, then tear the
      // WebContentsView down on the next macrotask.
      tab.view.setVisible(false);
      group.tabs.delete(tab.targetId);
      group.activeTargetId = fallback?.targetId ?? "";
      if (fallback) {
        for (const [id, item] of group.tabs) {
          item.view.setBounds(group.bounds);
          item.view.setVisible(group.visible && id === fallback.targetId);
        }
      }
      await registerGroup(group);
      emitTabs(group);
      setTimeout(() => void disposeTab(group, tab), 0);
    } else {
      await disposeTab(group, tab);
      await registerGroup(group);
      emitTabs(group);
    }
  } else if (action !== "list") {
    throw new Error(`Unsupported browser tab action '${action}'`);
  }

  const tabs = emitTabs(group);
  const current = tabs.find((tab) => tab.active) ?? tabs[0];
  return {
    ok: true,
    action,
    tabs,
    activeTargetId: group.activeTargetId,
    url: current?.url ?? "",
    title: current?.title ?? "",
  };
}

export function registerBrowserIpc(
  getWindow: () => BrowserWindow | null,
  invokeCore?: (method: string, params?: unknown) => Promise<unknown>,
): void {
  ipcMain.handle(
    "browser:attach",
    (
      _event,
      payload: {
        viewId: string;
        runId: string;
        bindingId: string;
        url?: string;
        rect?: { x: number; y: number; width: number; height: number };
      },
    ) => {
      if (process.env.AGENTCABIN_EMBEDDED_BROWSER === "0") {
        throw new Error("Embedded browser disabled via AGENTCABIN_EMBEDDED_BROWSER=0");
      }
      if (!payload?.viewId || !payload.runId || !payload.bindingId) {
        throw new Error("viewId, runId and bindingId are required");
      }

      return serializeViewOperation(payload.runId, async () => {
        const window = getWindow();
        if (!window || window.isDestroyed()) throw new Error("No main window");
        let group = registry.groupsByRunId.get(payload.runId);
        if (group && (group.window !== window || registry.groups.get(group.viewId) !== group)) {
          await destroyGroup(group);
          group = undefined;
        }
        const bounds = payload.rect
          ? normalizeSurfaceBounds(
              payload.rect,
              windowViewport(window),
              window.webContents.getZoomFactor(),
            )
          : { x: 0, y: 0, width: 0, height: 0 };
        if (group) {
          if (group.viewId !== payload.viewId) registry.groups.delete(group.viewId);
          group.viewId = payload.viewId;
          group.surfaceBindingId = payload.bindingId;
          group.bounds = bounds;
          group.visible = false;
          group.invokeCore = invokeCore;
          registry.groups.set(payload.viewId, group);
          for (const tab of group.tabs.values()) {
            tab.view.setBounds(bounds);
            tab.view.setVisible(false);
          }
          const tab = activeTab(group);
          if (!tab) throw new Error(`Browser group for run '${payload.runId}' has no tabs`);
          return {
            viewId: payload.viewId,
            bindingId: payload.bindingId,
            targetId: tab.targetId,
            endpoint: {
              host: tab.endpoint.host,
              port: tab.endpoint.port,
              token: tab.endpoint.token,
            },
            state: tab.view.getState(),
            tabs: browserTabSummaries(group),
            activeTargetId: group.activeTargetId,
          };
        }
        group = {
          viewId: payload.viewId,
          runId: payload.runId,
          surfaceBindingId: payload.bindingId,
          window,
          bounds,
          visible: false,
          activeTargetId: "",
          nextTabSequence: 0,
          registrationEndpoint: "",
          registrationToken: "",
          invokeCore,
          tabs: new Map(),
          tabsByViewId: new Map(),
        };
        registry.groups.set(payload.viewId, group);
        registry.groupsByRunId.set(payload.runId, group);

        try {
          const tab = await createTab(group, payload.url || "about:blank");
          await activateTab(group, tab.targetId);
          return {
            viewId: payload.viewId,
            bindingId: payload.bindingId,
            targetId: tab.targetId,
            endpoint: {
              host: tab.endpoint.host,
              port: tab.endpoint.port,
              token: tab.endpoint.token,
            },
            state: tab.view.getState(),
            tabs: browserTabSummaries(group),
            activeTargetId: group.activeTargetId,
          };
        } catch (error) {
          await disposeView(payload.viewId);
          throw error;
        }
      });
    },
  );

  ipcMain.handle(
    "browser:set-bounds",
    (_event, payload: { viewId: string; bindingId: string; rect: unknown }) => {
      const window = getWindow();
      const group = registry.groups.get(payload?.viewId);
      if (!window || !group || payload.bindingId !== group.surfaceBindingId) return;
      group.bounds = normalizeSurfaceBounds(
        payload.rect,
        windowViewport(window),
        window.webContents.getZoomFactor(),
      );
      for (const tab of group.tabs.values()) tab.view.setBounds(group.bounds);
    },
  );

  ipcMain.handle(
    "browser:set-visible",
    (_event, payload: { viewId: string; bindingId: string; visible: boolean }) => {
      const group = registry.groups.get(payload?.viewId);
      if (!group || payload.bindingId !== group.surfaceBindingId) return;
      group.visible = Boolean(payload?.visible);
      if (group.visible) {
        // A Work run can be replaced while the browser aside remains mounted.
        // Electron draws child views above the renderer, so ensure that only the
        // currently selected run owns a visible native browser surface.
        for (const other of registry.groups.values()) {
          if (other === group || other.window !== group.window) continue;
          other.visible = false;
          for (const tab of other.tabs.values()) tab.view.setVisible(false);
        }
      }
      for (const [targetId, tab] of group.tabs) {
        tab.view.setVisible(group.visible && targetId === group.activeTargetId);
      }
    },
  );

  ipcMain.handle("browser:command", (_event, payload: { viewId: string; action: string }) => {
    const group = registry.groups.get(payload?.viewId);
    const tab = group ? activeTab(group) : undefined;
    if (!tab) return;
    if (payload.action === "back") tab.view.goBack();
    else if (payload.action === "forward") tab.view.goForward();
    else if (payload.action === "reload") tab.view.reload();
    else if (payload.action === "stop") tab.view.stop();
  });

  ipcMain.handle("browser:get-endpoint", (_event, payload: { viewId: string }) => {
    const group = registry.groups.get(payload?.viewId) ?? registry.tabGroups.get(payload?.viewId);
    const tab = group
      ? payload?.viewId === group.viewId
        ? activeTab(group)
        : group.tabsByViewId.get(payload?.viewId)
      : undefined;
    return tab?.endpoint ?? null;
  });

  ipcMain.handle("browser:unbind", (_event, payload: { viewId: string; bindingId: string }) => {
    if (!payload?.viewId || !payload.bindingId) return;
    const group = registry.groups.get(payload.viewId);
    if (!group || group.surfaceBindingId !== payload.bindingId) return;
    group.visible = false;
    for (const tab of group.tabs.values()) tab.view.setVisible(false);
  });

  ipcMain.handle("browser:destroy", (_event, payload: { runId: string }) => {
    if (!payload?.runId) return;
    return destroyBrowserGroup(payload.runId);
  });
}
