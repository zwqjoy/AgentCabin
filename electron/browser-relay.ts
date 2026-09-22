/**
 * Token-authenticated loopback relay that exposes one CDP session.
 *
 * The relay deliberately knows nothing about `net` or Electron: it consumes a
 * line-oriented connection and a command/event transport, so every protocol
 * rule (handshake, framing, error mapping, teardown) is unit-testable.
 *
 * Wire format — newline-delimited JSON, one frame per line:
 *   client -> server  {"hello":{"protocol":1,"token":"..."}}
 *   server -> client  {"hello":{"protocol":1,"ok":true}}
 *   client -> server  {"id":7,"method":"Page.navigate","params":{...}}
 *   server -> client  {"id":7,"result":{...}} | {"id":7,"error":{...}}
 *   server -> client  {"method":"Page.frameNavigated","params":{...}}
 */

export const RELAY_PROTOCOL_VERSION = 1;

export interface RelayTransport {
  sendCommand(method: string, params?: Record<string, unknown>): Promise<unknown>;
  onEvent(handler: (method: string, params: unknown) => void): () => void;
}

export interface RelayConnection {
  write(line: string): void;
  end(): void;
  onLine(handler: (line: string) => void): void;
  onClose(handler: () => void): void;
}

export interface CdpRelayOptions {
  token: string;
  transport: RelayTransport;
  log?: (message: string, detail?: unknown) => void;
}

interface RelayErrorFrame {
  id: number | null;
  error: { code: string; message: string };
}

export class CdpRelayCore {
  private readonly token: string;
  private readonly transport: RelayTransport;
  private readonly log: (message: string, detail?: unknown) => void;
  private readonly connections = new Set<RelayConnection>();

  constructor(options: CdpRelayOptions) {
    this.token = options.token;
    this.transport = options.transport;
    this.log = options.log ?? (() => {});
  }

  handleConnection(connection: RelayConnection): void {
    this.connections.add(connection);
    let authenticated = false;
    let unsubscribe: (() => void) | null = null;

    connection.onClose(() => {
      unsubscribe?.();
      unsubscribe = null;
      this.connections.delete(connection);
    });

    connection.onLine((line) => {
      let frame: Record<string, unknown>;
      try {
        frame = JSON.parse(line) as Record<string, unknown>;
      } catch {
        // A malformed frame before the handshake is treated as noise rather
        // than an authentication attempt.
        if (!authenticated) return;
        connection.write(this.encode(this.badRequest("Malformed relay frame")));
        return;
      }

      if (!authenticated) {
        // Anything that is not an explicit handshake attempt is ignored: the
        // connection stays open but no command reaches the CDP session. Only a
        // real `hello` frame with the wrong token is rejected outright.
        if (!("hello" in frame)) return;
        const hello = frame.hello as { protocol?: number; token?: string } | undefined;
        if (!hello || hello.protocol !== RELAY_PROTOCOL_VERSION || hello.token !== this.token) {
          this.log("relay: rejected handshake");
          connection.write(
            this.encode({
              id: null,
              error: { code: "unauthorized", message: "Invalid relay token" },
            }),
          );
          connection.end();
          return;
        }
        authenticated = true;
        unsubscribe = this.transport.onEvent((method, params) => {
          connection.write(this.encode({ method, params }));
        });
        connection.write(this.encode({ hello: { protocol: RELAY_PROTOCOL_VERSION, ok: true } }));
        return;
      }

      const id = typeof frame.id === "number" ? frame.id : null;
      const method = typeof frame.method === "string" ? frame.method : "";
      if (id === null || !method) {
        connection.write(this.encode(this.badRequest("Malformed relay frame")));
        return;
      }

      const params =
        frame.params && typeof frame.params === "object"
          ? (frame.params as Record<string, unknown>)
          : undefined;

      void this.transport
        .sendCommand(method, params)
        .then((result) => {
          connection.write(this.encode({ id, result }));
        })
        .catch((error: unknown) => {
          connection.write(
            this.encode({
              id,
              error: {
                code: "cdp_error",
                message: error instanceof Error ? error.message : String(error),
              },
            }),
          );
        });
    });
  }

  closeAll(): void {
    for (const connection of this.connections) connection.end();
    this.connections.clear();
  }

  private badRequest(message: string): RelayErrorFrame {
    return { id: null, error: { code: "bad_request", message } };
  }

  private encode(frame: unknown): string {
    return `${JSON.stringify(frame)}\n`;
  }
}
