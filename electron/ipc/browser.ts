/**
 * Main-process ownership boundary for embedded browser views.
 *
 * The renderer may attach, position, show/hide, and command a view. URL
 * navigation still goes through the Rust browser policy and worker path.
 */
import { dialog, ipcMain, type BrowserWindow } from "electron";
import { EmbeddedBrowserView, windowViewport, type BrowserViewState } from "../browser-view";
import { BrowserRelayServer, createRelayToken, debuggerTransport } from "../browser-relay-server";
import { normalizeSurfaceBounds } from "../browser-view-geometry";

type Endpoint = { host: string; port: number; token: string; targetId: string };

interface Registry {
  windows: Map<string, BrowserWindow>;
  views: Map<string, EmbeddedBrowserView>;
  endpoints: Map<string, Endpoint>;
  relays: Map<string, BrowserRelayServer>;
}

const registry: Registry = {
  windows: new Map(),
  views: new Map(),
  endpoints: new Map(),
  relays: new Map(),
};

const BROWSER_PARTITION = "persist:agentcabin-browser";

function log(message: string, detail?: unknown): void {
  // eslint-disable-next-line no-console
  console.warn(`[browser-ipc] ${message}`, detail ?? "");
}

async function disposeView(
  viewId: string,
  invokeCore?: (method: string, params?: unknown) => Promise<unknown>,
) {
  registry.views.get(viewId)?.destroy();
  registry.views.delete(viewId);
  registry.windows.delete(viewId);
  const endpoint = registry.endpoints.get(viewId);
  registry.endpoints.delete(viewId);
  const relay = registry.relays.get(viewId);
  registry.relays.delete(viewId);
  await relay?.close();
  if (endpoint && invokeCore) {
    try {
      await invokeCore("unregister_embedded_browser", {
        endpoint: `${endpoint.host}:${endpoint.port}`,
      });
    } catch (error) {
      log("failed to unregister embedded browser from core", error);
    }
  }
}

export function browserViewsForWindow(window: BrowserWindow): EmbeddedBrowserView[] {
  return [...registry.views.entries()]
    .filter(([viewId]) => registry.windows.get(viewId) === window)
    .map(([, view]) => view);
}

export function destroyBrowserViews(
  invokeCore?: (method: string, params?: unknown) => Promise<unknown>,
): void {
  for (const viewId of registry.views.keys()) void disposeView(viewId, invokeCore);
}

export function registerBrowserIpc(
  getWindow: () => BrowserWindow | null,
  invokeCore?: (method: string, params?: unknown) => Promise<unknown>,
): void {
  ipcMain.handle(
    "browser:attach",
    async (
      _event,
      payload: {
        viewId: string;
        runId: string;
        url?: string;
        rect?: { x: number; y: number; width: number; height: number };
      },
    ) => {
      if (process.env.AGENTCABIN_EMBEDDED_BROWSER === "0") {
        throw new Error("Embedded browser disabled via AGENTCABIN_EMBEDDED_BROWSER=0");
      }
      const window = getWindow();
      if (!window) throw new Error("No main window");
      if (!payload?.viewId || !payload.runId) throw new Error("viewId and runId are required");

      await disposeView(payload.viewId, invokeCore);
      const relay = new BrowserRelayServer(log);
      let state: BrowserViewState | null = null;
      const view = new EmbeddedBrowserView(window, {
        viewId: payload.viewId,
        partition: BROWSER_PARTITION,
        allowedHosts: [],
        onStateChange: (next) => {
          state = next;
          if (!window.isDestroyed()) window.webContents.send("browser:state-changed", next);
        },
        onNavigationAsk: async (host, url) => {
          const { response } = await dialog.showMessageBox(window, {
            type: "question",
            buttons: ["打开", "取消"],
            defaultId: 0,
            cancelId: 1,
            message: `允许在 AgentCabin 内置浏览器中打开 ${host}？`,
            detail: url,
          });
          return response === 0;
        },
        log,
      });

      try {
        if (payload.rect) {
          view.setBounds(
            normalizeSurfaceBounds(
              payload.rect,
              windowViewport(window),
              window.webContents.getZoomFactor(),
            ),
          );
        }
        // Chromium may leave debugger.sendCommand pending when the target has
        // not navigated yet. Complete the initial navigation before attaching
        // the debugger and exposing the relay to the worker.
        await view.loadURL(payload.url || "about:blank");
        const token = createRelayToken();
        const port = await relay.listen(token, debuggerTransport(view.webContents));
        const targetId = await view.resolveTargetId();
        await view.fitPageToBounds();
        const endpoint: Endpoint = { host: "127.0.0.1", port, token, targetId };
        registry.views.set(payload.viewId, view);
        registry.windows.set(payload.viewId, window);
        registry.relays.set(payload.viewId, relay);
        registry.endpoints.set(payload.viewId, endpoint);

        if (invokeCore) {
          try {
            await invokeCore("register_embedded_browser", {
              payload: {
                endpoint: `${endpoint.host}:${endpoint.port}`,
                token: endpoint.token,
                targets: [{ targetId, runId: payload.runId, url: payload.url ?? "" }],
              },
            });
          } catch (error) {
            log("failed to register embedded browser with core", error);
            throw error;
          }
        }
        return {
          viewId: payload.viewId,
          targetId,
          endpoint: { host: endpoint.host, port: endpoint.port, token: endpoint.token },
          state,
        };
      } catch (error) {
        view.destroy();
        await relay.close();
        throw error;
      }
    },
  );

  ipcMain.handle("browser:set-bounds", (_event, payload: { viewId: string; rect: unknown }) => {
    const window = getWindow();
    const view = registry.views.get(payload?.viewId);
    if (!window || !view) return;
    const bounds = normalizeSurfaceBounds(
      payload.rect,
      windowViewport(window),
      window.webContents.getZoomFactor(),
    );
    view.setBounds(bounds);
  });

  ipcMain.handle("browser:set-visible", (_event, payload: { viewId: string; visible: boolean }) => {
    registry.views.get(payload?.viewId)?.setVisible(Boolean(payload?.visible));
  });

  ipcMain.handle("browser:command", (_event, payload: { viewId: string; action: string }) => {
    const view = registry.views.get(payload?.viewId);
    if (!view) return;
    if (payload.action === "back") view.goBack();
    else if (payload.action === "forward") view.goForward();
    else if (payload.action === "reload") view.reload();
    else if (payload.action === "stop") view.stop();
  });

  ipcMain.handle("browser:get-endpoint", (_event, payload: { viewId: string }) => {
    return registry.endpoints.get(payload?.viewId) ?? null;
  });

  ipcMain.handle("browser:detach", (_event, payload: { viewId: string }) =>
    disposeView(payload?.viewId, invokeCore),
  );
}
