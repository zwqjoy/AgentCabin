import { beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  handlers: new Map<string, (...args: any[]) => unknown>(),
  createdViews: [] as any[],
  relays: [] as any[],
}));
const { handlers, createdViews, relays } = state;

vi.mock("electron", () => ({
  dialog: { showMessageBox: vi.fn(async () => ({ response: 0 })) },
  ipcMain: {
    handle: vi.fn((channel: string, handler: (...args: any[]) => unknown) => {
      handlers.set(channel, handler);
    }),
  },
}));

vi.mock("../browser-view", () => ({
  EmbeddedBrowserView: class {
    destroyed = false;
    visible = false;
    url = "about:blank";
    readonly viewId: string;
    readonly webContents = {
      debugger: { isAttached: () => true, detach: vi.fn() },
      close: vi.fn(() => {
        this.destroyed = true;
      }),
    };
    constructor(_window: unknown, options: { viewId: string }) {
      this.viewId = options.viewId;
      state.createdViews.push(this);
    }
    setBounds(_bounds: unknown) {}
    setVisible(visible: boolean) {
      this.visible = visible;
    }
    async loadURL(url: string) {
      this.url = url;
    }
    getState() {
      return {
        viewId: this.viewId,
        targetId: this.viewId,
        url: this.url,
        title: this.url,
        isLoading: false,
        canGoBack: false,
        canGoForward: false,
      };
    }
    async resolveTargetId() {
      return this.viewId;
    }
    destroy() {
      this.destroyed = true;
      this.webContents.close();
    }
    goBack() {}
    goForward() {}
    reload() {}
    stop() {}
  },
  windowViewport: () => ({ width: 1200, height: 800 }),
}));

vi.mock("../browser-relay-server", () => ({
  BrowserRelayServer: class {
    closed = false;
    transport: {
      sendCommand: (method: string, params?: Record<string, unknown>) => unknown;
    } | null = null;
    constructor() {
      state.relays.push(this);
    }
    async listen(
      _token: string,
      transport: { sendCommand: (method: string, params?: Record<string, unknown>) => unknown },
    ) {
      this.transport = transport;
      return 4321 + state.relays.length;
    }
    async close() {
      this.closed = true;
    }
  },
  createRelayToken: () => "relay-token",
  debuggerTransport: () => ({
    sendCommand: async () => ({ targetInfo: { targetId: "unused" } }),
    onEvent: () => () => {},
  }),
}));

vi.mock("../browser-view-geometry", () => ({
  normalizeSurfaceBounds: (rect: unknown) => rect,
}));

import { destroyBrowserViews, destroyBrowserViewsForWindow, registerBrowserIpc } from "./browser";

const invoke = async <T>(channel: string, payload: unknown): Promise<T> => {
  const handler = handlers.get(channel);
  if (!handler) throw new Error(`No handler registered for ${channel}`);
  return (await handler({}, payload)) as T;
};

function makeWindow() {
  return {
    isDestroyed: () => false,
    contentView: { addChildView: vi.fn(), removeChildView: vi.fn() },
    webContents: {
      getZoomFactor: () => 1,
      send: vi.fn(),
    },
  } as any;
}

describe("embedded browser Run lifecycle", () => {
  const coreCalls: Array<{ method: string; params: unknown }> = [];
  const core = vi.fn(async (method: string, params?: unknown) => {
    coreCalls.push({ method, params });
    return {};
  });
  let window: ReturnType<typeof makeWindow>;

  beforeEach(async () => {
    await destroyBrowserViews();
    handlers.clear();
    createdViews.splice(0);
    relays.splice(0);
    coreCalls.splice(0);
    core.mockClear();
    window = makeWindow();
    registerBrowserIpc(() => window, core);
  });

  async function attach(
    runId: string,
    viewId = `view-${runId}`,
    bindingId = `binding-${runId}`,
    url = "https://a.test",
  ) {
    return invoke<{
      targetId: string;
      bindingId: string;
      tabs: Array<{ targetId: string; active: boolean; url: string }>;
      activeTargetId: string;
    }>("browser:attach", {
      runId,
      viewId,
      bindingId,
      url,
      rect: { x: 10, y: 10, width: 500, height: 400 },
    });
  }

  it("reuses the same Run group and ignores a changed initial URL", async () => {
    const first = await attach("run-a");
    const second = await attach("run-a", undefined, "binding-2", "https://reset.test");

    expect(createdViews).toHaveLength(1);
    expect(second.targetId).toBe(first.targetId);
    expect(second.tabs).toEqual(first.tabs);
    expect(second.tabs[0].url).toBe("https://a.test");
    expect(relays[0].closed).toBe(false);
    expect(coreCalls.filter(({ method }) => method === "unregister_embedded_browser")).toHaveLength(
      0,
    );
  });

  it("keeps tabs and active target across unbind, remount, and hide/show", async () => {
    const first = await attach("run-a");
    await relays[0].transport!.sendCommand("AgentCabin.browserTabs", {
      action: "new",
      url: "https://b.test",
    });
    const multiTab = await attach("run-a");
    const activeBefore = multiTab.activeTargetId;

    await invoke("browser:unbind", { viewId: "view-run-a", bindingId: "binding-run-a" });
    expect(createdViews.every((view) => !view.destroyed)).toBe(true);
    const remounted = await attach("run-a", undefined, "binding-remount");
    await invoke("browser:set-visible", {
      viewId: "view-run-a",
      bindingId: "binding-remount",
      visible: true,
    });

    expect(remounted.tabs).toHaveLength(2);
    expect(remounted.activeTargetId).toBe(activeBefore);
    expect(createdViews).toHaveLength(2);
    expect(createdViews.find((view) => view.viewId === activeBefore)?.visible).toBe(true);
    expect(first.targetId).not.toBe(activeBefore);
  });

  it("does not let a stale surface unbind its replacement", async () => {
    await attach("run-a");
    await attach("run-a", undefined, "replacement");
    await invoke("browser:set-visible", {
      viewId: "view-run-a",
      bindingId: "replacement",
      visible: true,
    });
    await invoke("browser:unbind", { viewId: "view-run-a", bindingId: "binding-run-a" });

    expect(createdViews[0].visible).toBe(true);
    expect(createdViews[0].destroyed).toBe(false);
  });

  it("rebinds a Run to a different presentation view without recreating its tabs", async () => {
    const first = await attach("run-a");
    const rebound = await attach("run-a", "replacement-view", "replacement-binding");
    await invoke("browser:unbind", {
      viewId: "view-run-a",
      bindingId: "binding-run-a",
    });

    expect(createdViews).toHaveLength(1);
    expect(rebound.targetId).toBe(first.targetId);
    expect(createdViews[0].destroyed).toBe(false);
    expect(createdViews[0].visible).toBe(false);
  });

  it("hides the previous Run without destroying it when another Run is shown", async () => {
    await attach("run-a");
    await invoke("browser:set-visible", {
      viewId: "view-run-a",
      bindingId: "binding-run-a",
      visible: true,
    });
    await attach("run-b");
    await invoke("browser:set-visible", {
      viewId: "view-run-b",
      bindingId: "binding-run-b",
      visible: true,
    });

    expect(createdViews[0].visible).toBe(false);
    expect(createdViews[0].destroyed).toBe(false);
    expect(createdViews[1].visible).toBe(true);
  });

  it("destroys views, relays, and Core registration only on explicit destroy", async () => {
    await attach("run-a");
    expect(coreCalls.some(({ method }) => method === "register_embedded_browser")).toBe(true);

    await invoke("browser:destroy", { runId: "run-a" });

    expect(createdViews[0].destroyed).toBe(true);
    expect(relays[0].closed).toBe(true);
    expect(coreCalls.filter(({ method }) => method === "unregister_embedded_browser")).toHaveLength(
      1,
    );
  });

  it("destroys all Run groups owned by a closing BrowserWindow", async () => {
    await attach("run-a");
    await attach("run-b");

    await destroyBrowserViewsForWindow(window);

    expect(createdViews.every((view) => view.destroyed)).toBe(true);
    expect(relays.every((relay) => relay.closed)).toBe(true);
    expect(coreCalls.filter(({ method }) => method === "unregister_embedded_browser")).toHaveLength(
      2,
    );
  });
});
