/**
 * Loopback TCP server that exposes one WebContentsView's CDP session.
 *
 * The protocol is newline-delimited JSON and the first frame must contain the
 * per-process token. Keeping the socket/server wrapper here means the relay
 * protocol itself remains independent of Node's net module and Electron.
 */
import { randomBytes } from "node:crypto";
import net from "node:net";
import type { WebContents } from "electron";
import { CdpRelayCore, type RelayConnection, type RelayTransport } from "./browser-relay";

export interface RelayEndpoint {
  host: "127.0.0.1";
  port: number;
  token: string;
  targetId: string;
}

export function createRelayToken(): string {
  return randomBytes(24).toString("hex");
}

export function debuggerTransport(contents: WebContents): RelayTransport {
  const debuggerApi = contents.debugger;
  const ensureAttached = (): void => {
    if (!debuggerApi.isAttached()) debuggerApi.attach("1.3");
  };

  return {
    sendCommand: async (method, params) => {
      ensureAttached();
      return debuggerApi.sendCommand(method, params ?? {});
    },
    onEvent: (handler) => {
      ensureAttached();
      const listener = (_event: unknown, method: string, params: unknown) =>
        handler(method, params);
      debuggerApi.on("message", listener);
      return () => debuggerApi.removeListener("message", listener);
    },
  };
}

export function isLoopbackAddress(address: string | undefined): boolean {
  if (!address) return false;
  const normalized = address.replace(/^::ffff:/, "");
  return normalized === "127.0.0.1" || normalized === "::1";
}

export class BrowserRelayServer {
  private server: net.Server | null = null;
  private readonly cores = new Set<CdpRelayCore>();

  constructor(private readonly log: (message: string, detail?: unknown) => void = () => {}) {}

  async listen(token: string, transport: RelayTransport): Promise<number> {
    await this.close();
    const core = new CdpRelayCore({ token, transport, log: this.log });
    this.cores.add(core);

    const server = net.createServer((socket) => {
      if (!isLoopbackAddress(socket.remoteAddress)) {
        this.log("relay: rejected non-loopback peer", socket.remoteAddress);
        socket.destroy();
        return;
      }

      const connection: RelayConnection = {
        write: (line) => {
          if (!socket.destroyed) socket.write(line);
        },
        end: () => socket.end(),
        onLine: (handler) => {
          let buffer = "";
          socket.on("data", (chunk: Buffer | string) => {
            buffer += chunk.toString();
            let index = buffer.indexOf("\n");
            while (index >= 0) {
              const line = buffer.slice(0, index).trim();
              buffer = buffer.slice(index + 1);
              if (line) handler(line);
              index = buffer.indexOf("\n");
            }
          });
        },
        onClose: (handler) => {
          socket.once("close", handler);
          socket.once("error", handler);
        },
      };
      core.handleConnection(connection);
    });

    this.server = server;
    const port = await new Promise<number>((resolve, reject) => {
      server.once("error", reject);
      server.listen(0, "127.0.0.1", () => {
        const address = server.address();
        if (typeof address === "object" && address) resolve(address.port);
        else reject(new Error("Relay server did not expose a port"));
      });
    });
    return port;
  }

  async close(): Promise<void> {
    for (const core of this.cores) core.closeAll();
    this.cores.clear();
    const server = this.server;
    this.server = null;
    if (!server) return;
    if (!server.listening) return;
    await new Promise<void>((resolve) => server.close(() => resolve()));
  }
}
