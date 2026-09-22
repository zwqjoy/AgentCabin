import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  guardWorkMcpAuthTool,
  installWorkMcpApprovalBridge,
  isExplicitlyReadOnly,
  isLikelyExternalSideEffect,
} from "./pi_mcp_permissions.mjs";

test("classifies external reads separately from likely side effects", () => {
  for (const name of ["search_issues", "get_page", "list_files", "describe_tool"]) {
    assert.equal(isLikelyExternalSideEffect(name), false, name);
  }
  for (const name of ["create_issue", "updatePage", "send-email", "delete_file", "deploy_service"]) {
    assert.equal(isLikelyExternalSideEffect(name), true, name);
  }
});

test("Work MCP broker only auto-allows explicitly annotated reads", async () => {
  const listeners = new Map();
  const events = new Map();
  const confirmations = [];
  const pi = {
    on(name, handler) {
      listeners.set(name, handler);
    },
    events: {
      on(name, handler) {
        events.set(name, handler);
      },
    },
  };
  const auditDir = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-mcp-audit-"));
  const previousRunDir = process.env.AGENTCABIN_WORK_RUN_DIR;
  process.env.AGENTCABIN_WORK_RUN_DIR = auditDir;
  installWorkMcpApprovalBridge(pi);
  listeners.get("session_start")({}, {
    ui: {
      async confirm(title, message) {
        confirmations.push({ title, message });
        return true;
      },
    },
  });

  async function decision(originalToolName, annotations) {
    let handler;
    events.get("pi-mcp-adapter:tool-approval-request")({
      serverName: "docs",
      originalToolName,
      origin: "proxy",
      annotations,
      args: { title: "x", apiKey: "must-not-leak" },
      claim(candidate) {
        handler = candidate;
        return true;
      },
    });
    return handler();
  }

  assert.equal(isExplicitlyReadOnly({ annotations: { readOnlyHint: true } }), true);
  assert.equal(await decision("get_page", { readOnlyHint: true }), "allow_once");
  assert.equal(confirmations.length, 0);
  assert.equal(await decision("update_page"), "allow_once");
  assert.equal(confirmations.length, 1);
  assert.match(confirmations[0].title, /Work/);
  assert.match(confirmations[0].message, /\[redacted\]/);
  assert.doesNotMatch(confirmations[0].message, /must-not-leak/);
  assert.equal(await decision("apply"), "allow_once");
  assert.equal(confirmations.length, 2);

  const audit = fs
    .readFileSync(path.join(auditDir, "mcp-approvals.jsonl"), "utf8")
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line));
  assert.equal(audit.length, 3);
  assert.equal(audit[0].classification, "read_only");
  assert.equal(audit[1].classification, "unknown_or_mutating");
  assert.match(audit[0].args_hash, /^[a-f0-9]{64}$/);
  assert.equal(audit[0].decision, "allow_once");
  assert.equal(audit[2].tool, "apply");

  if (previousRunDir === undefined) delete process.env.AGENTCABIN_WORK_RUN_DIR;
  else process.env.AGENTCABIN_WORK_RUN_DIR = previousRunDir;
  fs.rmSync(auditDir, { recursive: true, force: true });
});

test("MCP auth changes fail closed without Work UI", async () => {
  let called = false;
  const guarded = guardWorkMcpAuthTool({
    name: "mcp",
    async execute() {
      called = true;
      return { content: [] };
    },
  });
  const result = await guarded.execute("call", { action: "auth-start", server: "docs" });
  assert.equal(called, false);
  assert.equal(result.details.confirmed, false);
});
