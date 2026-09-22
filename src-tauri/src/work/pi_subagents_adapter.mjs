import crypto from "node:crypto";
import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import {
  isWorkFullAccess,
  loadWorkAccessRoots,
  normalizeWorkPath,
} from "./pi_workspace_paths.mjs";
import {
  ALL_SUBAGENT_TOOLS,
  CANONICAL_AGENT_NAMES,
  TERMINAL_CHILD_STATUSES,
  roleTools,
} from "./work_subagent_policy.mjs";
import { createWorkSubagentTools } from "./work_subagent_tools.mjs";
const SUBAGENT_ASYNC_COMPLETE_EVENT = "subagent:async-complete";
const SUBAGENT_RPC_READY_EVENT = "subagents:rpc:v1:ready";

function workBridgeConfig() {
  const port = Number.parseInt(String(process.env.AGENTCABIN_WORK_BRIDGE_PORT || ""), 10);
  const token = String(process.env.AGENTCABIN_WORK_BRIDGE_TOKEN || "").trim();
  if (!Number.isInteger(port) || port <= 0 || !token) {
    return null;
  }
  return { baseUrl: `http://127.0.0.1:${port}`, token };
}

async function workBridgePost(endpoint, body) {
  const config = workBridgeConfig();
  if (!config) {
    throw new Error("AgentCabin Work bridge is not configured; refusing to update subagent authority state.");
  }
  const response = await fetch(`${config.baseUrl}${endpoint}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${config.token}`,
    },
    body: JSON.stringify(body),
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(
      `Work bridge ${endpoint} failed with HTTP ${response.status}: ${payload?.error || payload?.message || "unknown error"}`,
    );
  }
  if (payload && payload.ok === false) {
    throw new Error(`Work bridge ${endpoint} rejected the request: ${payload.error || payload.message || "unknown error"}`);
  }
  return payload;
}

async function workBridgeGet(endpoint) {
  const config = workBridgeConfig();
  if (!config) {
    throw new Error("AgentCabin Work bridge is not configured; refusing to query subagent authority state.");
  }
  const response = await fetch(`${config.baseUrl}${endpoint}`, {
    method: "GET",
    headers: {
      Authorization: `Bearer ${config.token}`,
    },
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(
      `Work bridge ${endpoint} failed with HTTP ${response.status}: ${payload?.error || payload?.message || "unknown error"}`,
    );
  }
  return payload;
}

async function fetchSubagentBridgeRecord(agentId) {
  if (!agentId) return null;
  try {
    const data = await workBridgeGet(`/internal/work/subagents/record?agentId=${encodeURIComponent(agentId)}`);
    return data?.record || null;
  } catch (_err) {
    return null;
  }
}

function expectedWorkProfileDir() {
  const configured = String(process.env.AGENTCABIN_WORK_PROFILE_DIR || process.env.PI_CODING_AGENT_DIR || "").trim();
  return configured ? path.resolve(configured) : null;
}

function expectedAgentRuntimeDir() {
  const configured = String(process.env.PI_CODING_AGENT_DIR || process.env.AGENTCABIN_WORK_PROFILE_DIR || "").trim();
  return configured ? path.resolve(configured) : null;
}

function expectedSystemAgentPath(canonicalName) {
  // Pi launches Work with a per-run PI_CODING_AGENT_DIR. Agent definitions
  // are copied there by the runtime adapter, while the base Work profile is
  // kept as the authority for the expected digest and Work Core extension.
  const profileDir = expectedAgentRuntimeDir();
  return profileDir ? path.join(profileDir, "agents", `${canonicalName}.md`) : null;
}

function expectedSystemAgentFileDigest(canonicalName) {
  try {
    const parsed = JSON.parse(String(process.env.AGENTCABIN_WORK_EXPECTED_AGENT_FILE_DIGESTS || ""));
    const digest = parsed?.[canonicalName];
    return typeof digest === "string" && /^[a-f0-9]{64}$/i.test(digest) ? digest.toLowerCase() : null;
  } catch (_) {
    return null;
  }
}

function expectedWorkCoreExtensionPath() {
  const profileDir = expectedWorkProfileDir();
  return profileDir ? path.join(profileDir, "extensions", "agentcabin-work-core.mjs") : null;
}

function sha256File(filePath) {
  return crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");
}

function sortedStrings(values) {
  return [...new Set(Array.isArray(values) ? values.map((value) => String(value)) : [])].sort();
}

async function importPiSubagentsModule(subpath) {
  const profileDir = expectedWorkProfileDir();
  const specifier = `pi-subagents/${subpath}`;
  if (profileDir) {
    const candidateBases = [
      path.join(profileDir, "npm", "node_modules", "pi-subagents"),
      path.join(profileDir, "node_modules", "pi-subagents"),
      path.join(profileDir, "..", "node_modules", "pi-subagents"),
    ];
    for (const base of candidateBases) {
      if (fs.existsSync(base)) {
        try {
          const req = createRequire(path.join(base, "package.json"));
          const resolved = req.resolve(specifier);
          return await import(pathToFileURL(resolved).href);
        } catch (_) {
          for (const ext of [".js", ".mjs", "/index.js"]) {
            const direct = path.join(base, `${subpath}${ext}`);
            if (fs.existsSync(direct)) {
              return await import(pathToFileURL(direct).href);
            }
          }
        }
      }
    }
    const npmBase = path.join(profileDir, "npm", "package.json");
    if (fs.existsSync(npmBase)) {
      try {
        const req = createRequire(npmBase);
        const resolved = req.resolve(specifier);
        return await import(pathToFileURL(resolved).href);
      } catch (_) {
        // ignore
      }
    }
  }
  return await import(specifier);
}

function sameStringSet(left, right) {
  const a = sortedStrings(left);
  const b = sortedStrings(right);
  return a.length === b.length && a.every((value, index) => value === b[index]);
}

/**
 * Map a pi-subagents 0.51 async-complete payload to a Work terminal status.
 *
 * Returns null for non-terminal or unrecognised states (fail-closed: the
 * caller must not write a Completed/Failed/Stopped fact for those).
 *
 * pi-subagents 0.51 states:
 *   queued | running | complete | failed | paused | stopped | rejected
 */
function resolveTerminalStatus(state) {
  switch (state) {
    case "complete":
    case "completed":
    case "success":
      return "completed";

    case "failed":
    case "error":
    case "rejected":
      return "failed";

    case "stopped":
    case "cancelled":
      return "stopped";

    case "paused":
    case "queued":
    case "running":
      // Non-terminal – do not touch the host registry.
      return null;

    default:
      // Unknown state: fail-closed, do not assume completed.
      return null;
  }
}

const MAX_SUBAGENT_OUTPUT_BYTES = 1024 * 1024; // 1 MB upper bound for text outputs

function readAuthorizedOutputFile(candidatePath) {
  if (typeof candidatePath !== "string" || !candidatePath.trim()) return null;
  const raw = candidatePath.trim();
  if (raw.includes("\0") || raw.includes("://")) return null;

  const wsRoot = String(process.env.AGENTCABIN_WORKSPACE_ROOT || "").trim();
  if (!wsRoot) {
    // Fail closed if Workspace root is not configured
    return null;
  }

  try {
    const manifestPath = path.join(wsRoot, "manifest.json");
    const accessRoots = loadWorkAccessRoots({
      manifestPath,
      fallback: process.env.AGENTCABIN_WORK_ACCESS_ROOTS,
    });

    const trustedReadRoots = [];
    const rawProfileDir = String(process.env.AGENTCABIN_WORK_PROFILE_DIR || "").trim();
    if (rawProfileDir) {
      trustedReadRoots.push(path.join(rawProfileDir, "skills"));
    }
    const rawPluginsDir = String(
      process.env.AGENTCABIN_AGENT_PLUGINS_DIR ||
        path.join(os.homedir(), ".agentcabin", "agent-plugins"),
    ).trim();
    if (rawPluginsDir) {
      trustedReadRoots.push(rawPluginsDir);
    }
    const cabinSkills = path.join(os.homedir(), ".agentcabin", "skills");
    trustedReadRoots.push(cabinSkills);

    const target = normalizeWorkPath(raw, {
      workspaceRoot: wsRoot,
      accessRoots,
      trustedReadRoots,
      writable: false,
      allowUnrestricted: isWorkFullAccess(),
    });

    if (!fs.existsSync(target.absolute)) return null;

    const stat = fs.statSync(target.absolute);
    if (!stat.isFile()) return null;

    if (stat.size > MAX_SUBAGENT_OUTPUT_BYTES) {
      console.warn(`[work/subagents] Output file rejected: '${target.absolute}' exceeds maximum size limit (${stat.size} > ${MAX_SUBAGENT_OUTPUT_BYTES} bytes).`);
      return null;
    }

    const content = fs.readFileSync(target.absolute, "utf8");
    if (content.trim()) {
      return content.trim();
    }
    return null;
  } catch (_) {
    // Rejected by canonical Work path authority
    return null;
  }
}

function collectOutputFileCandidates(payload) {
  const details = payload?.details || payload?.result || {};
  const text = typeof payload?.summary === "string"
    ? payload.summary
    : typeof payload?.result === "string"
      ? payload.result
      : toolResultText(payload);
  const candidates = [
    payload?.outputFile,
    payload?.output_file,
    payload?.outputPath,
    payload?.output_path,
    details?.outputFile,
    details?.output_file,
    details?.outputPath,
    details?.output_path,
    details?.resultFile,
    details?.result_file,
  ];

  const matches = String(text).match(/(?:\/[\w.-]+)+\/\S+_output\.md/g);
  if (matches) {
    candidates.push(...matches);
  }
  const outputLines = [...String(text).matchAll(/(?:^|\n)\s*Output(?: artifact)?:\s+(.+_output\.md)\s*$/gm)];
  candidates.push(...outputLines.map((match) => match[1].trim()));

  return candidates.filter((c) => typeof c === "string" && c.trim());
}

function extractOutputFileContent(payload, extraCandidates = []) {
  const candidates = [
    ...extraCandidates,
    ...collectOutputFileCandidates(payload),
  ];
  for (const candidate of candidates) {
    const content = readAuthorizedOutputFile(candidate);
    if (content) return content;
  }
  return null;
}

function toolResultText(result) {
  if (typeof result?.text === "string") return result.text;
  if (!Array.isArray(result?.content)) return "";
  return result.content
    .filter((part) => part && part.type === "text" && typeof part.text === "string")
    .map((part) => part.text)
    .join("\n");
}

function extractStatusState(statusReply) {
  const details = statusReply?.details || {};
  const structuredState = [
    details.state,
    details.status,
    statusReply?.state,
    statusReply?.status,
  ].find((value) => typeof value === "string" && value.trim());
  if (structuredState) return String(structuredState).trim().toLowerCase();

  // pi-subagents' RPC status response exposes lifecycle state in its
  // human-readable text (`State: complete`) while its structured `details`
  // only contains mode/results. Keep the adapter tolerant of both shapes.
  const match = toolResultText(statusReply).match(/(?:^|\n)\s*State\s*:\s*([a-z][a-z_-]*)/i);
  return match?.[1]?.toLowerCase() || "";
}

function extractAndEmbedOutputFile(statusReply, extraCandidates = []) {
  if (!statusReply) return statusReply;
  let text = toolResultText(statusReply);
  const details = statusReply.details || {};
  const candidates = [
    ...extraCandidates,
    ...collectOutputFileCandidates(statusReply),
  ];

  for (const candidate of candidates) {
    const content = readAuthorizedOutputFile(candidate);
    if (content) {
      return {
        ...statusReply,
        text: `${text}\n\n【子代理输出内容】:\n${content}`,
        details: {
          ...details,
          output_content: content,
        },
      };
    }
  }
  return statusReply;
}

function summarizeAsyncComplete(payload, extraCandidates = []) {
  const state = String(payload?.status || payload?.state || payload?.result?.status || "").toLowerCase();
  const status = resolveTerminalStatus(state);
  if (!status) {
    // Non-terminal or unrecognised state – signal to callers via null.
    return null;
  }
  const rawSummary = payload?.summary ?? payload?.result?.text ?? payload?.result?.output ?? payload?.result;
  let summary = typeof rawSummary === "string"
    ? rawSummary
    : rawSummary === undefined || rawSummary === null ? "Subagent completed."
      : JSON.stringify(rawSummary);
  const embeddedContent = extractOutputFileContent(payload, extraCandidates);
  if (embeddedContent && !summary.includes(embeddedContent)) {
    summary = `${summary}\n\n【子代理输出内容】:\n${embeddedContent}`;
  }
  const rawError = payload?.error ?? payload?.result?.error;
  return {
    status,
    summary: summary.slice(0, 4000),
    error: rawError ? String(rawError).slice(0, 1000) : null,
    reason: status === "stopped" ? "Subagent stopped" : null,
  };
}

export default function agentCabinWorkSubagentsAdapter(pi) {
  // If this process is a child subagent, NEVER load delegation tools.
  if (process.env.PI_SUBAGENT_CHILD === "1") {
    return;
  }

  const activeChildren = new Map(); // agentId -> { role, createdAt, outputPath }
  let turnSpawnCount = 0;
  let totalSpawnCount = 0;
  let capabilityCeilingHandle = null;
  let capabilityCeilingSessionId = null;
  let sessionStarted = false;
  let rpcReady = false;
  const terminalUpdatePromises = new Map();
  const terminalChildIds = new Set();

  // Race buffer: child may emit async-complete *before* register_spawn returns
  // and activeChildren is committed. Track provisional IDs so we can buffer
  // the event and replay it once the spawn is fully registered.
  const pendingSpawns = new Set();
  const pendingTerminalPayloads = new Map(); // runId -> payload

  pi.on("turn_start", () => {
    turnSpawnCount = 0;
  });

  const disposeRpcReady = pi.events.on(SUBAGENT_RPC_READY_EVENT, (payload) => {
    if (payload?.version === 1 && Array.isArray(payload?.methods) && payload.methods.includes("spawn")) {
      rpcReady = true;
    }
  });

  pi.on("session_start", () => {
    sessionStarted = true;
  });

  function callRpc(method, params, timeoutMs = 30000) {
    return new Promise((resolve, reject) => {
      const requestId = crypto.randomUUID();
      const replyEvent = `subagents:rpc:v1:reply:${requestId}`;
      let timer = null;

      const unsubscribe = pi.events.on(replyEvent, (reply) => {
        if (timer) clearTimeout(timer);
        try {
          if (typeof unsubscribe === "function") unsubscribe();
        } catch (_) {}
        if (reply?.version !== 1 || reply?.requestId !== requestId) {
          reject(new Error(`RPC ${method} returned an invalid response envelope`));
        } else if (reply.success === true && Object.hasOwn(reply, "data")) {
          resolve(reply.data);
        } else {
          const errMsg = reply?.error?.message || `RPC ${method} failed`;
          reject(new Error(errMsg));
        }
      });

      timer = setTimeout(() => {
        try {
          if (typeof unsubscribe === "function") unsubscribe();
        } catch (_) {}
        reject(new Error(`Subagent RPC call '${method}' timed out after ${timeoutMs}ms`));
      }, timeoutMs);

      pi.events.emit("subagents:rpc:v1:request", {
        version: 1,
        requestId,
        method,
        params,
        source: { extension: "agentcabin-work-subagents-adapter" },
      });
    });
  }

  // Preflight is part of the security boundary. Missing or malformed package
  // APIs are a hard failure; a synthetic digest must never authorize a spawn.
  async function preflightValidate(role, task, cwd, capabilityCeiling) {
    const canonicalName = CANONICAL_AGENT_NAMES[role];
    if (!canonicalName) return { ok: false, error: `Invalid subagent role: '${role}'` };
    const expectedAgentPath = expectedSystemAgentPath(canonicalName);
    const expectedCorePath = expectedWorkCoreExtensionPath();
    const expectedAgentFileDigest = expectedSystemAgentFileDigest(canonicalName);
    if (!expectedAgentPath || !expectedCorePath) {
      return { ok: false, error: "Preflight failed: Work profile paths are not configured." };
    }
    if (!expectedAgentFileDigest) {
      return { ok: false, error: "Preflight failed: expected AgentCabin system-agent digest is unavailable." };
    }

    let definitionFileDigest;
    try {
      definitionFileDigest = sha256File(expectedAgentPath);
    } catch (error) {
      return { ok: false, error: `Preflight failed: exact system agent file is unavailable (${error.message}).` };
    }
    if (definitionFileDigest !== expectedAgentFileDigest) {
      return { ok: false, error: `Preflight rejected: '${canonicalName}' does not match the AgentCabin system-agent digest.` };
    }

    try {
      const preflightModule = await importPiSubagentsModule("preflight");
      if (typeof preflightModule.resolveSubagentLaunchContract !== "function") {
        return { ok: false, error: "Preflight failed: pi-subagents/preflight API is unavailable." };
      }
      const contractResult = await preflightModule.resolveSubagentLaunchContract({
        agent: canonicalName,
        task,
        context: "fresh",
        cwd: cwd || process.cwd(),
        capabilityCeiling,
      });
      if (!contractResult?.ok) {
        return { ok: false, error: `Preflight failed for '${role}': ${contractResult?.message || "unknown error"}` };
      }

      const contract = contractResult.contract;
      const agent = contract?.agent;
      const tools = contract?.tools;
      const expectedTools = roleTools(role);
      if (!agent || agent.name !== canonicalName || agent.source !== "user" || path.resolve(agent.filePath || "") !== expectedAgentPath) {
        return { ok: false, error: `Preflight rejected: '${canonicalName}' is not bound to the exact Work system agent file.` };
      }
      if (!agent.definitionDigest || !contract.launchContractDigest || !contract.digest) {
        return { ok: false, error: `Preflight rejected: '${canonicalName}' has incomplete launch identity digests.` };
      }
      if (agent.shadowedCandidates?.some((candidate) => candidate.source === "project" || candidate.source === "user")) {
        return { ok: false, error: `Preflight rejected: '${canonicalName}' has an unauthorized shadowing candidate.` };
      }
      if (!sameStringSet(tools?.effectiveAllowlist, expectedTools) || !sameStringSet(tools?.requiredChildTools, expectedTools)) {
        return { ok: false, error: `Preflight rejected: effective child tools do not exactly match role '${role}'.` };
      }
      if (tools.effectiveMcpTools?.length || tools.toolExtensionPaths?.length || tools.fanoutAuthorized) {
        return { ok: false, error: `Preflight rejected: '${canonicalName}' has unauthorized MCP, tool extension, or fanout capability.` };
      }
      const runtimeExtensions = sortedStrings(tools.runtimeExtensions);
      const extensionArgs = sortedStrings(tools.extensionArgs);
      const configuredExtensions = sortedStrings(tools.configuredExtensions);
      const nonRuntimeExtensionArgs = extensionArgs.filter((extension) => !runtimeExtensions.includes(extension));
      if (tools.disableAmbientExtensions !== true || !sameStringSet(configuredExtensions, [expectedCorePath]) || !sameStringSet(nonRuntimeExtensionArgs, [expectedCorePath])) {
        return { ok: false, error: `Preflight rejected: '${canonicalName}' is not explicitly bound to the Work Core extension.` };
      }
      if (!capabilityCeiling || capabilityCeiling.denyExtensions !== false
        || !sameStringSet(capabilityCeiling.allowedAgents, Object.values(CANONICAL_AGENT_NAMES))
        || !sameStringSet(capabilityCeiling.allowedTools, ALL_SUBAGENT_TOOLS)) {
        return { ok: false, error: "Preflight rejected: capability ceiling is missing or widened." };
      }
      if (definitionFileDigest !== sha256File(expectedAgentPath) || definitionFileDigest !== expectedAgentFileDigest) {
        return { ok: false, error: "Preflight rejected: the exact system agent file changed during validation." };
      }
      return {
        ok: true,
        contract,
        canonicalName,
        digest: contract.launchContractDigest,
        definitionDigest: agent.definitionDigest,
      };
    } catch (error) {
      return { ok: false, error: `Preflight failed closed: ${error instanceof Error ? error.message : String(error)}` };
    }
  }

  // The ceiling is registered before preflight and spawn, then resolved again
  // so both the host and the launch contract see the same authority boundary.
  async function applyCapabilityCeiling(sessionId) {
    if (!sessionId) throw new Error("Cannot delegate without the current Work Pi session ID.");
    const ceilingModule = await importPiSubagentsModule("capability-ceiling");
    if (typeof ceilingModule.registerSubagentCapabilityCeiling !== "function"
      || typeof ceilingModule.resolveCurrentSubagentCapabilityCeiling !== "function") {
      throw new Error("pi-subagents capability-ceiling API is unavailable.");
    }
    if (capabilityCeilingSessionId !== sessionId) {
      capabilityCeilingHandle?.dispose?.();
      capabilityCeilingHandle = ceilingModule.registerSubagentCapabilityCeiling({
        sessionId,
        source: "agentcabin-work",
        ceiling: {
          allowedAgents: Object.values(CANONICAL_AGENT_NAMES),
          allowedTools: ALL_SUBAGENT_TOOLS,
          denyExtensions: false,
        },
      });
      capabilityCeilingSessionId = sessionId;
    }
    const resolved = ceilingModule.resolveCurrentSubagentCapabilityCeiling(sessionId);
    if (!resolved) throw new Error("Capability ceiling registration could not be resolved.");
    return resolved;
  }

  function clearActiveChild(agentId) {
    activeChildren.delete(agentId);
  }

  async function finalizeChildStatus(agentId, status, summary = null, error = null, reason = null) {
    if (!TERMINAL_CHILD_STATUSES.has(status)) throw new Error(`Invalid terminal subagent status '${status}'.`);
    if (terminalChildIds.has(agentId)) return { alreadyFinalized: true };
    const existing = terminalUpdatePromises.get(agentId);
    if (existing) return existing;
    const update = (async () => {
      await workBridgePost("/internal/work/subagents/update_status", {
        agentId,
        status,
        summary: status === "completed" ? summary : null,
        error: status === "failed" ? error : null,
        reason: reason || (status === "stopped" ? "Subagent stopped" : null),
      });
      terminalChildIds.add(agentId);
      clearActiveChild(agentId);
      return { alreadyFinalized: false };
    })();
    terminalUpdatePromises.set(agentId, update);
    try {
      return await update;
    } finally {
      terminalUpdatePromises.delete(agentId);
    }
  }

  pi.events.on(SUBAGENT_ASYNC_COMPLETE_EVENT, (payload) => {
    const runId = payload?.runId;
    if (!runId) return;
    // Check against active children first.
    if (activeChildren.has(runId)) {
      const child = activeChildren.get(runId);
      const terminal = summarizeAsyncComplete(
        payload,
        child?.outputPath ? [child.outputPath] : [],
      );
      if (!terminal) {
        // Non-terminal state (paused, queued, running, unknown) – do not touch registry.
        console.warn(`[work/subagents] Non-terminal async-complete state received for ${runId}; ignoring.`);
        return;
      }
      void finalizeChildStatus(runId, terminal.status, terminal.summary, terminal.error, terminal.reason)
        .catch((error) => console.error("[work/subagents] Async terminal bridge update failed:", error));
      return;
    }
    // Race window: spawn reply received but child not yet registered in activeChildren.
    // Buffer the event so the spawn path can drain it post-registration.
    if (pendingSpawns.has(runId)) {
      pendingTerminalPayloads.set(runId, payload);
    }
  });

  pi.on("session_shutdown", async () => {
    sessionStarted = false;
    rpcReady = false;
    disposeRpcReady?.();
    const childrenToStop = Array.from(activeChildren.keys());
    for (const childId of childrenToStop) {
      try {
        await callRpc("stop", { id: childId });
        await finalizeChildStatus(childId, "stopped", null, null, "Parent Work session shut down");
      } catch (error) {
        console.error(`[work/subagents] Failed to stop child ${childId} during parent shutdown:`, error);
      }
    }
  });



  async function spawnChild({ role, task, ctx, preflight }) {
    if (sessionStarted && !rpcReady) {
      throw new Error(
        "Work Subagent RPC bridge is not ready; pi-subagents did not announce the spawn capability.",
      );
    }
    if (!preflight) {
      const cwd = ctx?.cwd || process.cwd();
      const sessionId = ctx?.sessionManager?.getSessionId?.() || "";
      const capabilityCeiling = await applyCapabilityCeiling(sessionId);
      preflight = await preflightValidate(role, task, cwd, capabilityCeiling);
      if (!preflight.ok) {
        throw new Error(preflight.error);
      }
    }

    const canonicalName = preflight.canonicalName;
    // Persist the complete child response inside the Work scratch area. The
    // status RPC only returns a display preview, while Work intentionally
    // blocks direct reads from pi-subagents' private async directories.
    const outputPath = `scratch/agentcabin-${canonicalName}-${crypto.randomUUID()}_output.md`;
    const workflowScript = `return runs.run("main", { agent: ${JSON.stringify(canonicalName)}, task: ${JSON.stringify(task)}, output: ${JSON.stringify(outputPath)} })`;
    const spawnReply = await callRpc("spawn", {
      workflowScript,
      async: true,
    });

    const agentId = typeof spawnReply?.details?.runId === "string" ? spawnReply.details.runId.trim() : "";
    if (!agentId) throw new Error("Spawn RPC did not return authoritative data.details.runId.");

    // Set pending spawn ID before awaiting register_spawn so the event handler
    // can buffer a completion that arrives during the registration window.
    pendingSpawns.add(agentId);

    const taskDigest = crypto.createHash("sha256").update(task).digest("hex").slice(0, 16);
    let registerReply;
    try {
      registerReply = await workBridgePost("/internal/work/subagents/register_spawn", {
        agentId,
        providerRunId: agentId,
        childIndex: totalSpawnCount,
        role,
        taskDigest,
        launchContractDigest: preflight.contract.launchContractDigest,
      });
    } catch (bridgeError) {
      pendingSpawns.delete(agentId);
      pendingTerminalPayloads.delete(agentId);
      let stopOk = false;
      let stopErrMsg = null;
      try {
        await callRpc("stop", { id: agentId });
        stopOk = true;
      } catch (stopError) {
        stopErrMsg = stopError instanceof Error ? stopError.message : String(stopError);
      }
      if (stopOk) {
        try {
          await finalizeChildStatus(agentId, "stopped", null, null, "Compensating stop due to register_spawn failure");
        } catch (_) {}
      }
      const err = new Error(
        stopOk
          ? `${bridgeError.message}; child was stopped as compensation.`
          : `${bridgeError.message}; compensating stop also failed: ${stopErrMsg}`
      );
      err.compensated = stopOk;
      err.failedAgentId = agentId;
      throw err;
    }

    const bootstrapToken = registerReply?.bootstrapToken;
    if (!bootstrapToken) {
      pendingSpawns.delete(agentId);
      pendingTerminalPayloads.delete(agentId);
      let stopOk = false;
      try {
        await callRpc("stop", { id: agentId });
        stopOk = true;
      } catch (_) {}
      if (stopOk) {
        try {
          await finalizeChildStatus(agentId, "stopped", null, null, "Compensating stop due to missing bootstrapToken");
        } catch (_) {}
      }
      const err = new Error("Work bridge did not return a valid bootstrapToken for child subagent.");
      err.compensated = stopOk;
      err.failedAgentId = agentId;
      throw err;
    }

    // Bridge authority is durable. Commit the active child identity.
    activeChildren.set(agentId, { role, createdAt: Date.now(), outputPath });
    pendingSpawns.delete(agentId);
    turnSpawnCount += 1;
    totalSpawnCount += 1;

    // Drain any completion that arrived during the register_spawn window.
    const buffered = pendingTerminalPayloads.get(agentId);
    pendingTerminalPayloads.delete(agentId);
    if (buffered && buffered.runId === agentId) {
      const terminal = summarizeAsyncComplete(buffered, [outputPath]);
      if (terminal) {
        void finalizeChildStatus(agentId, terminal.status, terminal.summary, terminal.error, terminal.reason)
          .catch((error) => console.error("[work/subagents] Buffered terminal bridge update failed:", error));
      }
    }

    return { agentId, role };
  }

  async function waitForChild({ agentId, timeoutSeconds = 120, signal }) {
    const deadline = Date.now() + timeoutSeconds * 1000;
    let lastStatus = "running";
    let resultText = "";
    let outputContent = null;
    let errorText = null;

    while (Date.now() < deadline) {
      if (signal?.aborted) {
        return {
          agentId,
          status: "running",
          wait_aborted: true,
          result: "等待已取消，子代理仍在后台运行",
        };
      }

      try {
        const rawReply = await callRpc("status", { id: agentId });
        const outputPath = activeChildren.get(agentId)?.outputPath;
        const statusReply = extractAndEmbedOutputFile(
          rawReply,
          outputPath ? [outputPath] : [],
        );
        const details = statusReply?.details || {};
        if (details.output_content) {
          outputContent = details.output_content;
        }
        const rawState = extractStatusState(statusReply);
        const resolvedStatus = resolveTerminalStatus(rawState);

        if (resolvedStatus === "completed") {
          lastStatus = "completed";
          resultText = details.output_content
            || details.result
            || details.summary
            || toolResultText(statusReply)
            || "子代理已完成任务。";
          break;
        } else if (resolvedStatus === "failed") {
          lastStatus = "failed";
          resultText = details.error || toolResultText(statusReply) || "子代理执行失败。";
          errorText = details.error || resultText;
          break;
        } else if (resolvedStatus === "stopped") {
          lastStatus = "stopped";
          resultText = toolResultText(statusReply) || "子代理已被停止。";
          break;
        }
      } catch (err) {
        const bridgeRecord = await fetchSubagentBridgeRecord(agentId);
        if (bridgeRecord) {
          const resolvedBridgeStatus = resolveTerminalStatus(bridgeRecord.status);
          if (TERMINAL_CHILD_STATUSES.has(resolvedBridgeStatus) || bridgeRecord.status === "interrupted") {
            lastStatus = resolvedBridgeStatus || bridgeRecord.status;
            resultText = bridgeRecord.error || bridgeRecord.resultSummary || `子代理处于终态 ${lastStatus}（会话重启或已中断）。`;
            if (lastStatus === "failed") {
              errorText = bridgeRecord.error || resultText;
            }
            break;
          }
        }
        console.warn("[work/subagents] Wait polling error:", err);
      }

      await new Promise((r) => setTimeout(r, 1000));
    }

    if (lastStatus === "interrupted") {
      clearActiveChild(agentId);
      return {
        agentId,
        status: "interrupted",
        interrupted: true,
        recoverable: true,
        result: resultText,
        output_content: outputContent,
      };
    }

    if (lastStatus === "running") {
      return {
        agentId,
        status: "running",
        timeout: true,
        result: `等待超时（${timeoutSeconds}秒）。子代理仍在后台运行，可稍后再次调用 work_agent_wait 或 work_agent_status。`,
        output_content: outputContent,
      };
    }

    try {
      await finalizeChildStatus(
        agentId,
        lastStatus,
        lastStatus === "completed" ? resultText.slice(0, 1000) : null,
        lastStatus === "failed" ? (errorText || resultText).slice(0, 1000) : null,
        lastStatus === "stopped" ? "Stopped by user/agent" : null,
      );
    } catch (finalizeErr) {
      console.error(`[work/subagents] Failed to finalize child ${agentId} status:`, finalizeErr);
      return {
        agentId,
        status: "authority_error",
        observed_status: lastStatus,
        result: resultText,
        output_content: outputContent,
        error: `记录子代理终态失败: ${finalizeErr instanceof Error ? finalizeErr.message : String(finalizeErr)}`,
      };
    }

    return {
      agentId,
      status: lastStatus,
      result: resultText,
      output_content: outputContent,
      error: errorText,
    };
  }

  const runtime = {
    get activeChildren() {
      return activeChildren;
    },
    get turnSpawnCount() {
      return turnSpawnCount;
    },
    get totalSpawnCount() {
      return totalSpawnCount;
    },
    applyCapabilityCeiling,
    preflightValidate,
    spawnChild,
    waitForChild,
    callRpc,
    finalizeChildStatus,
    fetchSubagentBridgeRecord,
    extractAndEmbedOutputFile,
  };

  for (const tool of createWorkSubagentTools(runtime)) {
    pi.registerTool(tool);
  }
}
