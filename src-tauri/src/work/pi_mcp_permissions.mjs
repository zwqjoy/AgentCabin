import crypto from "node:crypto";
import fs from "node:fs";

const APPROVAL_EVENT = "pi-mcp-adapter:tool-approval-request";
const AUDIT_FILENAME = "mcp-approvals.jsonl";

const SIDE_EFFECT_VERBS = new Set([
  "add",
  "append",
  "approve",
  "archive",
  "assign",
  "book",
  "cancel",
  "charge",
  "close",
  "comment",
  "commit",
  "copy",
  "create",
  "delete",
  "deploy",
  "disable",
  "edit",
  "email",
  "enable",
  "execute",
  "grant",
  "insert",
  "invite",
  "like",
  "lock",
  "mark",
  "merge",
  "message",
  "modify",
  "move",
  "pay",
  "post",
  "publish",
  "purchase",
  "react",
  "reject",
  "remove",
  "rename",
  "reopen",
  "replace",
  "reply",
  "restart",
  "restore",
  "refund",
  "revoke",
  "run",
  "schedule",
  "send",
  "set",
  "start",
  "stop",
  "submit",
  "sync",
  "trigger",
  "unarchive",
  "unassign",
  "unfollow",
  "unlock",
  "update",
  "upload",
  "upsert",
  "follow",
  "transfer",
  "write",
]);

function nameTokens(value) {
  return String(value || "")
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
}

export function isLikelyExternalSideEffect(toolName) {
  return nameTokens(toolName).some((token) => SIDE_EFFECT_VERBS.has(token));
}

export function isExplicitlyReadOnly(request) {
  const annotations = request?.annotations ?? request?.toolAnnotations ?? {};
  return annotations.readOnlyHint === true || annotations.read_only_hint === true;
}

function redactArgs(value, depth = 0) {
  if (depth > 4) return "[nested]";
  if (Array.isArray(value)) return value.slice(0, 20).map((item) => redactArgs(item, depth + 1));
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.entries(value)
      .slice(0, 40)
      .map(([key, item]) => [
        key,
        /(?:secret|token|password|credential|api[_-]?key|authorization)/i.test(key)
          ? "[redacted]"
          : redactArgs(item, depth + 1),
      ]),
  );
}

function previewArgs(args) {
  const json = JSON.stringify(redactArgs(args ?? {}), null, 2);
  return json.length > 1600 ? `${json.slice(0, 1600)}\n…` : json;
}

function argsHash(args) {
  return crypto
    .createHash("sha256")
    .update(JSON.stringify(redactArgs(args ?? {})))
    .digest("hex");
}

function recordApprovalAudit(request, decision) {
  const runDir = String(process.env.AGENTCABIN_WORK_RUN_DIR || "").trim();
  if (!runDir) return;
  try {
    fs.mkdirSync(runDir, { recursive: true });
    const audit = {
      timestamp: new Date().toISOString(),
      server: String(request?.serverName || ""),
      tool: String(request?.originalToolName || ""),
      origin: String(request?.origin || ""),
      classification: isExplicitlyReadOnly(request) ? "read_only" : "unknown_or_mutating",
      decision,
      args_hash: argsHash(request?.args),
    };
    const target = `${runDir}/${AUDIT_FILENAME}`;
    fs.appendFileSync(target, `${JSON.stringify(audit)}\n`, { encoding: "utf8", mode: 0o600 });
    try {
      fs.chmodSync(target, 0o600);
    } catch {
      // Best effort on platforms that do not expose POSIX file modes.
    }
  } catch {
    // Approval must not become an unsafe allow because audit storage failed.
    // The decision itself remains governed by the explicit policy below.
  }
}

/**
 * Work owns MCP approval policy. Only a tool with an explicit MCP read-only
 * annotation may bypass confirmation. Missing or unknown metadata is treated
 * as potentially mutating and requires an explicit, one-shot confirmation.
 */
export function installWorkMcpApprovalBridge(pi) {
  let currentUi;
  pi.on("session_start", (_event, context) => {
    currentUi = context.ui;
  });
  pi.on("session_shutdown", () => {
    currentUi = undefined;
  });
  pi.events.on(APPROVAL_EVENT, (request) => {
    request.claim(async () => {
      let decision = "deny";
      if (isExplicitlyReadOnly(request)) {
        decision = "allow_once";
      } else if (currentUi && typeof currentUi.confirm === "function") {
        const confirmed = await currentUi.confirm(
          "确认 Work 外部操作？",
          [
            `连接器：${request.serverName}`,
            `工具：${request.originalToolName}`,
            `来源：${request.origin}`,
            `分类：${isLikelyExternalSideEffect(request.originalToolName) ? "可能修改外部系统" : "未知操作，默认需要确认"}`,
            "",
            "参数（敏感字段已隐去）：",
            previewArgs(request.args),
            "",
            "该操作可能修改外部系统，仅在你确认后执行。",
          ].join("\n"),
        );
        decision = confirmed ? "allow_once" : "deny";
      }
      recordApprovalAudit(request, decision);
      return decision;
    });
  });
}

export function guardWorkMcpAuthTool(tool) {
  if (!tool || tool.name !== "mcp" || typeof tool.execute !== "function") return tool;
  const execute = tool.execute;
  return {
    ...tool,
    async execute(toolCallId, params, signal, onUpdate, ctx) {
      if (params?.action === "auth-start" || params?.action === "auth-complete") {
        return {
          content: [{
            type: "text",
            text: "Work MCP authentication is managed by AgentCabin Connector/App authorization in Inbox; direct Pi MCP OAuth is disabled.",
          }],
          details: { ok: false, confirmed: false, action: params.action, managedBy: "work-host" },
        };
      }
      return execute.call(tool, toolCallId, params, signal, onUpdate, ctx);
    },
  };
}
