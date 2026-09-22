import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

const TYPEBOX_STUB = [
  "export const Type = {",
  "  Array: () => ({}),",
  "  Literal: (value) => value,",
  "  Object: () => ({}),",
  "  Optional: (value) => value,",
  "  String: () => ({}),",
  "};",
  "",
].join("\n");

async function loadAdapter(tempRoot) {
  const extensionDir = path.join(tempRoot, "extensions");
  const typeboxDir = path.join(tempRoot, "node_modules", "typebox");
  fs.mkdirSync(extensionDir, { recursive: true });
  fs.mkdirSync(typeboxDir, { recursive: true });
  fs.writeFileSync(path.join(tempRoot, "package.json"), '{"type":"module"}\n');
  fs.writeFileSync(
    path.join(tempRoot, "node_modules", "typebox", "package.json"),
    '{"type":"module","exports":"./index.js"}\n',
  );
  fs.writeFileSync(path.join(typeboxDir, "index.js"), TYPEBOX_STUB);
  fs.copyFileSync(
    new URL("./pi_connector_adapter.mjs", import.meta.url),
    path.join(extensionDir, "adapter.mjs"),
  );
  return import(pathToFileURL(path.join(extensionDir, "adapter.mjs")).href);
}

test("pi_connector_adapter registers and executes the Code Connector tool", async () => {
  const tempDir = fs.mkdtempSync(
    path.join(fs.realpathSync(path.resolve(".")), ".tmp_test_connector_"),
  );
  try {
    const mod = await loadAdapter(tempDir);
    const tools = new Map();
    mod.registerConnectorTools(
      { registerTool: (tool) => tools.set(tool.name, tool) },
      {
        callConnectorRuntime: async (_toolCallId, params) => ({
          success: true,
          status: "success",
          exitCode: 0,
          stdout: JSON.stringify({ ok: true, query: params.args?.[1] }),
          stderr: "",
        }),
      },
    );

    const connector = tools.get("work_run_connector_cli");
    assert.ok(connector);
    const response = await connector.execute("tool-1", {
      package_id: "feishu",
      operation: "searchUser",
      args: ["--query", "alice"],
    });
    assert.equal(response.details.ok, true);
    assert.match(response.content[0].text, /feishu\/searchUser/);
    assert.match(response.content[0].text, /alice/);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test("pi_connector_adapter reports host bridge failures without throwing", async () => {
  const tempDir = fs.mkdtempSync(
    path.join(fs.realpathSync(path.resolve(".")), ".tmp_test_connector_"),
  );
  try {
    const mod = await loadAdapter(tempDir);
    const tools = new Map();
    mod.registerConnectorTools(
      { registerTool: (tool) => tools.set(tool.name, tool) },
      {
        callConnectorRuntime: async () => ({
          success: false,
          status: "failed",
          exitCode: 1,
          stderr: "disabled",
        }),
      },
    );
    const response = await tools.get("work_run_connector_cli").execute("tool-2", {
      package_id: "feishu",
      operation: "sendMessage",
    });
    assert.equal(response.details.ok, false);
    assert.match(response.content[0].text, /disabled/);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});
