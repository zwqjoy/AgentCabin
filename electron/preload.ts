/**
 * AgentCabin preload script.
 *
 * Runs in a sandboxed, context-isolated world. Only an allow-listed,
 * promise-based bridge is exposed on `window.agentcabinDesktop`. The renderer
 * must never gain direct access to ipcRenderer, Node, or Electron internals.
 */
import { contextBridge, ipcRenderer } from "electron";
import { EVENT_CHANNELS, INVOKE_CHANNELS } from "./types";
import type {
  AgentCabinDesktopBridge,
  ConfirmOptions,
  CoreEventEnvelope,
  InvokeChannel,
  OpenDialogOptions,
  SaveDialogOptions,
} from "./types";

const invokeChannels: ReadonlySet<string> = new Set<string>(INVOKE_CHANNELS);
const eventChannels: ReadonlySet<string> = new Set<string>(EVENT_CHANNELS);

function assertAllowed(channel: string, allowed: ReadonlySet<string>): void {
  if (!allowed.has(channel)) {
    throw new Error(`agentcabinDesktop: channel "${channel}" is not allowed`);
  }
}

function invoke<T = unknown>(channel: InvokeChannel, payload?: unknown): Promise<T> {
  assertAllowed(channel, invokeChannels);
  return ipcRenderer.invoke(channel, payload) as Promise<T>;
}

/** Local dispatcher for core events pushed by the main process. */
const coreHandlers = new Map<string, Set<(payload: unknown) => void>>();

ipcRenderer.on("core:event", (_event, envelope: CoreEventEnvelope) => {
  const handlers = coreHandlers.get(envelope.event);
  if (!handlers) return;
  for (const handler of handlers) {
    try {
      handler(envelope.payload);
    } catch {
      // A faulty subscriber must not break the dispatcher.
    }
  }
});

const bridge: AgentCabinDesktopBridge = {
  invoke,

  listen<T = unknown>(channel: string, handler: (payload: T) => void): Promise<() => void> {
    assertAllowed(channel, eventChannels);
    const listener = (_event: unknown, payload: T) => handler(payload);
    ipcRenderer.on(channel, listener);
    return Promise.resolve(() => {
      ipcRenderer.removeListener(channel, listener);
    });
  },

  async emit(_channel: string, _payload?: unknown): Promise<void> {
    // Renderer -> main push events are not part of the bridge yet.
  },

  window: {
    minimize: () => invoke("window:minimize"),
    toggleMaximize: () => invoke("window:toggleMaximize"),
    close: () => invoke("window:close"),
    focus: () => invoke("window:focus"),
    setZoom: (factor: number) => invoke("window:set-zoom", { factor }),
    outerPosition: () => invoke("window:outer-position"),
  },

  app: {
    version: () => invoke("app:version"),
    platform: () => invoke("app:platform"),
  },

  core: {
    invoke: async (method, params) => {
      const res = await invoke<{ ok: boolean; result?: unknown; error?: string }>("core:invoke", {
        method,
        params,
      });
      if (!res.ok) {
        throw new Error(res.error ?? "invoke failed");
      }
      return res.result;
    },
    listen: (event, handler) => {
      let handlers = coreHandlers.get(event);
      if (!handlers) {
        handlers = new Set();
        coreHandlers.set(event, handlers);
      }
      handlers.add(handler);
      return () => {
        const set = coreHandlers.get(event);
        if (!set) return;
        set.delete(handler);
        if (set.size === 0) coreHandlers.delete(event);
      };
    },
    emit: (event, payload) => invoke("core:emit", { event, payload }),
  },

  dialog: {
    open: (options: OpenDialogOptions) => invoke("dialog:open", options),
    save: (options: SaveDialogOptions) => invoke("dialog:save", options),
    confirm: (message: string, options: ConfirmOptions) =>
      invoke("dialog:confirm", { message, options }),
  },

  shell: {
    openExternal: (url: string) => invoke("shell:open-external", url),
    openPath: (path: string) => invoke("shell:open-path", path),
  },

  path: {
    homeDir: () => invoke("path:home"),
    join: (...segments: string[]) => invoke("path:join", segments),
  },

  browser: {
    attach: (payload) => invoke("browser:attach", payload),
    setBounds: (payload) => invoke("browser:set-bounds", payload),
    setVisible: (payload) => invoke("browser:set-visible", payload),
    command: (payload) => invoke("browser:command", payload),
    getEndpoint: (payload) => invoke("browser:get-endpoint", payload),
    unbind: (payload) => invoke("browser:unbind", payload),
    destroy: (payload) => invoke("browser:destroy", payload),
  },
};

contextBridge.exposeInMainWorld("agentcabinDesktop", bridge);
