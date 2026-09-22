/**
 * Raw page-level CDP client for an embedded Electron browser view.
 *
 * The relay is newline-delimited JSON and is deliberately scoped to loopback
 * endpoints. Electron owns the actual CDP session; this client only exposes
 * the command/event transport needed by the worker page adapter.
 */
import net from "node:net";

const LOCAL_HOSTS = new Set(["127.0.0.1", "localhost", "::1"]);
const RELAY_PROTOCOL_VERSION = 1;
const CONNECT_TIMEOUT_MS = 5000;
const COMMAND_TIMEOUT_MS = 15000;

export function parseRelayEndpoint(raw) {
  const value = String(raw ?? "").trim();
  const separator = value.lastIndexOf(":");
  if (separator <= 0) throw new Error(`Invalid embedded relay endpoint '${value}'`);
  const host = value.slice(0, separator).replace(/^\[|\]$/g, "");
  const port = Number(value.slice(separator + 1));
  if (!LOCAL_HOSTS.has(host.toLowerCase())) {
    throw new Error(`Embedded relay host '${host}' is not loopback`);
  }
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    throw new Error(`Invalid embedded relay port in '${value}'`);
  }
  return { host, port };
}

export function createEmbeddedCdpClient(options) {
  const endpoint = parseRelayEndpoint(options.endpoint);
  const token = String(options.token ?? "");
  if (!token) throw new Error("Embedded relay token is required");

  const socket = options.socketFactory
    ? options.socketFactory(endpoint)
    : net.connect({ host: endpoint.host, port: endpoint.port });
  const write = (data) => {
    if (typeof socket.send === "function") socket.send(data);
    else socket.write(data);
  };
  let buffer = "";
  let nextId = 1;
  let ready = false;
  let closed = false;
  let handshakeResolve = null;
  let handshakeReject = null;
  let handshakeTimer = null;
  const pending = new Map();
  const eventHandlers = new Set();

  function emitLine(line) {
    let frame;
    try {
      frame = JSON.parse(line);
    } catch {
      return;
    }

    if (!ready) {
      if (frame.hello?.ok === true) {
        ready = true;
        if (handshakeTimer) clearTimeout(handshakeTimer);
        handshakeResolve?.();
        return;
      }
      handshakeReject?.(new Error(frame.error?.message || "Embedded relay handshake failed"));
      return;
    }

    if (typeof frame.method === "string") {
      for (const handler of eventHandlers) {
        try {
          handler(frame.method, frame.params ?? {});
        } catch {
          // A faulty subscriber must not break the CDP client.
        }
      }
      return;
    }

    if (typeof frame.id === "number" && pending.has(frame.id)) {
      const { resolve, reject, timer } = pending.get(frame.id);
      pending.delete(frame.id);
      clearTimeout(timer);
      if (frame.error) reject(new Error(frame.error.message || "CDP command failed"));
      else resolve(frame.result);
    }
  }

  function failAll(reason) {
    if (closed) return;
    closed = true;
    if (handshakeTimer) clearTimeout(handshakeTimer);
    handshakeReject?.(new Error(reason));
    for (const { reject, timer } of pending.values()) {
      clearTimeout(timer);
      reject(new Error(reason));
    }
    pending.clear();
  }

  socket.on("data", (chunk) => {
    buffer += chunk.toString("utf8");
    let index = buffer.indexOf("\n");
    while (index >= 0) {
      const line = buffer.slice(0, index).trim();
      buffer = buffer.slice(index + 1);
      if (line) emitLine(line);
      index = buffer.indexOf("\n");
    }
  });
  socket.on("close", () => failAll("Embedded relay socket closed"));
  socket.on("error", (error) => failAll(`Embedded relay socket error: ${error.message}`));

  return {
    async connect() {
      if (closed) throw new Error("Embedded relay is closed");
      const handshake = new Promise((resolve, reject) => {
        handshakeResolve = resolve;
        handshakeReject = reject;
      });
      write(`${JSON.stringify({ hello: { protocol: RELAY_PROTOCOL_VERSION, token } })}\n`);
      handshakeTimer = setTimeout(
        () => handshakeReject?.(new Error("Embedded relay handshake timed out")),
        CONNECT_TIMEOUT_MS,
      );
      handshakeTimer.unref?.();
      await handshake;
      return this;
    },

    send(method, params = {}) {
      if (!ready || closed) return Promise.reject(new Error("Embedded relay is not ready"));
      const id = nextId++;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(new Error(`Embedded CDP command '${method}' timed out after ${COMMAND_TIMEOUT_MS}ms`));
        }, COMMAND_TIMEOUT_MS);
        timer.unref?.();
        pending.set(id, { resolve, reject, timer });
        try {
          write(`${JSON.stringify({ id, method, params })}\n`);
        } catch (error) {
          clearTimeout(timer);
          pending.delete(id);
          reject(error);
        }
      });
    },

    onEvent(handler) {
      eventHandlers.add(handler);
      return () => eventHandlers.delete(handler);
    },

    close() {
      if (closed) return;
      closed = true;
      if (handshakeTimer) clearTimeout(handshakeTimer);
      handshakeReject?.(new Error("Embedded relay closed by caller"));
      for (const { reject } of pending.values()) reject(new Error("Embedded relay closed by caller"));
      pending.clear();
      eventHandlers.clear();
      socket.close?.();
      socket.destroy?.();
    },

    get isReady() {
      return ready && !closed;
    },
  };
}
