import { describe, expect, it, vi } from "vitest";
import { CdpRelayCore, type RelayConnection, type RelayTransport } from "./browser-relay";

class FakeConnection implements RelayConnection {
  written: string[] = [];
  ended = false;
  private lineHandler: ((line: string) => void) | null = null;
  private closeHandler: (() => void) | null = null;

  write(line: string): void {
    this.written.push(line);
  }

  end(): void {
    this.ended = true;
  }

  onLine(handler: (line: string) => void): void {
    this.lineHandler = handler;
  }

  onClose(handler: () => void): void {
    this.closeHandler = handler;
  }

  feed(line: string): void {
    this.lineHandler?.(line);
  }

  closeFromPeer(): void {
    this.closeHandler?.();
  }
}

function makeTransport() {
  const emitted: Array<{ method: string; params: unknown }> = [];
  let eventHandler: ((method: string, params: unknown) => void) | null = null;
  const transport: RelayTransport = {
    sendCommand: vi.fn(async (method: string, params?: Record<string, unknown>) => {
      if (method === "Page.navigate") return { frameId: "F1", url: params?.url };
      throw new Error(`CDP method ${method} failed: no such target`);
    }),
    onEvent: (handler) => {
      eventHandler = handler;
      return () => {
        eventHandler = null;
      };
    },
  };
  return {
    transport,
    emitted,
    emitEvent: (method: string, params: unknown) => {
      emitted.push({ method, params });
      eventHandler?.(method, params);
    },
  };
}

const TOKEN = "tok_abc";

async function connect(core: CdpRelayCore) {
  const conn = new FakeConnection();
  core.handleConnection(conn);
  conn.feed(JSON.stringify({ hello: { protocol: 1, token: TOKEN } }));
  await vi.waitFor(() => expect(conn.written.length).toBe(1));
  return conn;
}

describe("CdpRelayCore", () => {
  it("rejects a bad token and closes the socket", async () => {
    const { transport } = makeTransport();
    const core = new CdpRelayCore({ token: TOKEN, transport });
    const conn = new FakeConnection();
    core.handleConnection(conn);
    conn.feed(JSON.stringify({ hello: { protocol: 1, token: "wrong" } }));
    await vi.waitFor(() => expect(conn.ended).toBe(true));
    expect(JSON.parse(conn.written[0]).error.code).toBe("unauthorized");
  });

  it("ignores commands sent before the handshake", async () => {
    const { transport } = makeTransport();
    const core = new CdpRelayCore({ token: TOKEN, transport });
    const conn = new FakeConnection();
    core.handleConnection(conn);
    conn.feed(JSON.stringify({ id: 1, method: "Page.navigate" }));
    expect(conn.written).toEqual([]);
    expect(transport.sendCommand).not.toHaveBeenCalled();
  });

  it("acknowledges the handshake", async () => {
    const { transport } = makeTransport();
    const core = new CdpRelayCore({ token: TOKEN, transport });
    const conn = await connect(core);
    expect(JSON.parse(conn.written[0])).toEqual({ hello: { protocol: 1, ok: true } });
  });

  it("forwards commands and returns results", async () => {
    const { transport } = makeTransport();
    const core = new CdpRelayCore({ token: TOKEN, transport });
    const conn = await connect(core);
    conn.feed(
      JSON.stringify({ id: 7, method: "Page.navigate", params: { url: "https://a.test" } }),
    );
    await vi.waitFor(() => expect(conn.written.length).toBe(2));
    expect(JSON.parse(conn.written[1])).toEqual({
      id: 7,
      result: { frameId: "F1", url: "https://a.test" },
    });
  });

  it("maps a CDP failure to a relay error frame", async () => {
    const { transport } = makeTransport();
    const core = new CdpRelayCore({ token: TOKEN, transport });
    const conn = await connect(core);
    conn.feed(JSON.stringify({ id: 8, method: "Page.nonexistent" }));
    await vi.waitFor(() => expect(conn.written.length).toBe(2));
    expect(JSON.parse(conn.written[1]).id).toBe(8);
    expect(JSON.parse(conn.written[1]).error.message).toContain("no such target");
  });

  it("pushes transport events to the client", async () => {
    const { transport, emitEvent } = makeTransport();
    const core = new CdpRelayCore({ token: TOKEN, transport });
    const conn = await connect(core);
    emitEvent("Page.frameNavigated", { frame: { url: "https://b.test" } });
    await vi.waitFor(() => expect(conn.written.length).toBe(2));
    expect(JSON.parse(conn.written[1])).toEqual({
      method: "Page.frameNavigated",
      params: { frame: { url: "https://b.test" } },
    });
  });

  it("reports a malformed frame without dropping the connection", async () => {
    const { transport } = makeTransport();
    const core = new CdpRelayCore({ token: TOKEN, transport });
    const conn = await connect(core);
    conn.feed("not json");
    await vi.waitFor(() => expect(conn.written.length).toBe(2));
    expect(JSON.parse(conn.written[1])).toEqual({
      id: null,
      error: { code: "bad_request", message: "Malformed relay frame" },
    });
    expect(conn.ended).toBe(false);
  });

  it("stops forwarding events after the client disconnects", async () => {
    const { transport, emitEvent } = makeTransport();
    const core = new CdpRelayCore({ token: TOKEN, transport });
    const conn = await connect(core);
    conn.closeFromPeer();
    emitEvent("Page.loadEventFired", { timestamp: 1 });
    expect(conn.written.length).toBe(1);
  });
});
