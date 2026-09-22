import test from "node:test";
import assert from "node:assert/strict";
import { createEmbeddedCdpClient, parseRelayEndpoint } from "./embedded_cdp.mjs";

function makeSocket() {
  const sent = [];
  const handlers = { data: null, close: null, error: null };
  return {
    sent,
    handlers,
    send(data) {
      sent.push(data);
    },
    close() {
      handlers.close?.();
    },
    on(event, handler) {
      handlers[event] = handler;
    },
  };
}

function connect(socket) {
  const client = createEmbeddedCdpClient({
    endpoint: "127.0.0.1:1234",
    token: "tok_abc",
    socketFactory: () => socket,
  });
  const connected = client.connect();
  socket.handlers.data?.(Buffer.from(`${JSON.stringify({ hello: { protocol: 1, ok: true } })}\n`));
  return connected.then(() => client);
}

test("parseRelayEndpoint rejects anything that is not loopback", () => {
  assert.throws(() => parseRelayEndpoint("10.0.0.5:1234"), /not loopback/);
  assert.deepEqual(parseRelayEndpoint("127.0.0.1:1234"), { host: "127.0.0.1", port: 1234 });
});

test("handshake sends the token as the first frame", async () => {
  const socket = makeSocket();
  await connect(socket);
  assert.equal(socket.sent[0], `${JSON.stringify({ hello: { protocol: 1, token: "tok_abc" } })}\n`);
});

test("resolves a command by id and rejects on error frames", async () => {
  const socket = makeSocket();
  const client = await connect(socket);

  const ok = client.send("Runtime.evaluate", { expression: "1+1" });
  const frame = JSON.parse(socket.sent.at(-1));
  socket.handlers.data?.(Buffer.from(`${JSON.stringify({ id: frame.id, result: { value: 2 } })}\n`));
  assert.deepEqual(await ok, { value: 2 });

  const bad = client.send("Page.nonexistent", {});
  const badFrame = JSON.parse(socket.sent.at(-1));
  socket.handlers.data?.(
    Buffer.from(
      `${JSON.stringify({ id: badFrame.id, error: { code: "cdp_error", message: "no such method" } })}\n`,
    ),
  );
  await assert.rejects(bad, /no such method/);
});

test("dispatches transport events to subscribers", async () => {
  const socket = makeSocket();
  const client = await connect(socket);
  const seen = [];
  const off = client.onEvent((method, params) => seen.push({ method, params }));
  socket.handlers.data?.(
    Buffer.from(
      `${JSON.stringify({ method: "Page.frameNavigated", params: { frame: { url: "https://a.test" } } })}\n`,
    ),
  );
  assert.deepEqual(seen, [
    { method: "Page.frameNavigated", params: { frame: { url: "https://a.test" } } },
  ]);
  off();
  socket.handlers.data?.(Buffer.from(`${JSON.stringify({ method: "Page.loadEventFired", params: {} })}\n`));
  assert.equal(seen.length, 1);
});

test("rejects a handshake that is not ok", async () => {
  const socket = makeSocket();
  const client = createEmbeddedCdpClient({
    endpoint: "127.0.0.1:1234",
    token: "tok_abc",
    socketFactory: () => socket,
  });
  const connected = client.connect();
  socket.handlers.data?.(
    Buffer.from(
      `${JSON.stringify({ id: null, error: { code: "unauthorized", message: "Invalid relay token" } })}\n`,
    ),
  );
  await assert.rejects(connected, /Invalid relay token/);
});

test("rejects pending commands when the socket closes", async () => {
  const socket = makeSocket();
  const client = await connect(socket);
  const pending = client.send("Page.navigate", { url: "https://a.test" });
  socket.close();
  await assert.rejects(pending, /closed/i);
});
