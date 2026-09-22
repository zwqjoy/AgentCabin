#!/usr/bin/env node
/**
 * Probe the Grok Build 1.0 ACP surfaces that are not yet enabled in AgentCabin.
 *
 * This is intentionally a diagnostic harness, not an adapter implementation. It
 * records the complete JSONL exchange around session/new, an optional
 * session/set_effort candidate, and a native /goal prompt. The resulting summary
 * distinguishes advertised metadata from a live, successful protocol primitive.
 *
 * Examples:
 *   npm run grok:probe-p5 -- \
 *     --binary /path/to/grok \
 *     --expected-agent-version 1.0.0 \
 *     --probe-effort \
 *     --goal-command "/goal status" \
 *     --trace-out /tmp/grok-p5.trace.json \
 *     --summary-out /tmp/grok-p5.summary.json
 *
 * An initialize fixture can be inspected without a live binary:
 *   npm run grok:probe-p5 -- \
 *     --initialize-fixture src-tauri/src/agent/grok_session_actor/fixtures/grok_build_1_0_initialize.wire.json
 *
 * The harness never enables AgentCabin capabilities or writes a fixture. Existing
 * output files require --force before they can be overwritten.
 */

import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import process from "node:process";
import { StringDecoder } from "node:string_decoder";

const PROJECT_ROOT = resolve(import.meta.dirname, "..");
const DEFAULT_TIMEOUT_MS = 30_000;
const DEFAULT_QUIET_MS = 1_500;
const STARTUP_QUIET_MS = 300;
const TERMINATION_GRACE_MS = 1_000;
const MAX_SIGNAL_HITS = 80;

class ProbeError extends Error {
  constructor(message, capture) {
    super(message);
    this.name = "ProbeError";
    this.capture = capture;
  }
}

function usage() {
  return `Usage: node scripts/probe-grok-acp-p5.mjs [options]

Options:
  --binary <path>                  Grok executable (default: GROK_BINARY or grok)
  --cwd <path>                     Working directory for Grok (default: current directory)
  --permission-mode <mode>         Grok permission mode (default: default)
  --client-version <version>       AgentCabin version in initialize (default: Cargo version)
  --expected-agent-version <ver>   Fail unless response _meta.agentVersion matches
  --timeout-ms <milliseconds>      Timeout for each ACP response (default: ${DEFAULT_TIMEOUT_MS})
  --quiet-ms <milliseconds>        Wait for notifications after a probe (default: ${DEFAULT_QUIET_MS})
  --goal-command <command>         Prompt used for the Goal probe (default: /goal status)
  --skip-goal                      Do not send a native Goal prompt
  --probe-effort                   Send session/set_effort with the current effort level
  --effort-method <method>         Candidate effort method (default: session/set_effort)
  --effort-level <level>           Effort level for --probe-effort (default: current model level)
  --initialize-fixture <path>      Inspect an initialize result without spawning Grok
  --trace-out <path>               Write the complete JSONL exchange as JSON
  --summary-out <path>             Write the normalized probe summary as JSON
  --force                          Allow existing output files to be overwritten
  --dry-run                        Print the probe request plan without spawning Grok
  --help                           Show this help

Safety:
  --probe-effort is opt-in because a successful method may change the session's
  reasoning effort. The default Goal command is "/goal status" so the probe asks
  for state instead of creating a new autonomous goal. Use --goal-command /goal
  when an exact native command probe is needed.
`;
}

async function defaultClientVersion() {
  const cargoPath = resolve(PROJECT_ROOT, "src-tauri/Cargo.toml");
  try {
    const cargo = await readFile(cargoPath, "utf8");
    return cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? "unknown";
  } catch {
    return "unknown";
  }
}

function takeOption(args, index, name, inlineValue) {
  if (inlineValue !== undefined) {
    return { value: inlineValue, nextIndex: index };
  }
  const value = args[index + 1];
  if (!value || value.startsWith("--")) {
    throw new Error(`${name} requires a value`);
  }
  return { value, nextIndex: index + 1 };
}

async function parseArgs(argv) {
  const options = {
    binary: process.env.GROK_BINARY?.trim() || "grok",
    cwd: process.cwd(),
    permissionMode: "default",
    clientVersion: await defaultClientVersion(),
    expectedAgentVersion: undefined,
    timeoutMs: DEFAULT_TIMEOUT_MS,
    quietMs: DEFAULT_QUIET_MS,
    goalCommand: "/goal status",
    skipGoal: false,
    probeEffort: false,
    effortMethod: "session/set_effort",
    effortLevel: undefined,
    initializeFixture: undefined,
    traceOut: undefined,
    summaryOut: undefined,
    force: false,
    dryRun: false,
    help: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const [name, inlineValue] = argument.split(/=(.*)/s, 2);
    switch (name) {
      case "--help":
      case "-h":
        options.help = true;
        break;
      case "--force":
        options.force = true;
        break;
      case "--dry-run":
        options.dryRun = true;
        break;
      case "--skip-goal":
        options.skipGoal = true;
        break;
      case "--probe-effort":
        options.probeEffort = true;
        break;
      case "--binary":
      case "--cwd":
      case "--permission-mode":
      case "--client-version":
      case "--expected-agent-version":
      case "--goal-command":
      case "--effort-method":
      case "--effort-level":
      case "--initialize-fixture":
      case "--trace-out":
      case "--summary-out": {
        const result = takeOption(argv, index, name, inlineValue);
        const field = {
          "--binary": "binary",
          "--cwd": "cwd",
          "--permission-mode": "permissionMode",
          "--client-version": "clientVersion",
          "--expected-agent-version": "expectedAgentVersion",
          "--goal-command": "goalCommand",
          "--effort-method": "effortMethod",
          "--effort-level": "effortLevel",
          "--initialize-fixture": "initializeFixture",
          "--trace-out": "traceOut",
          "--summary-out": "summaryOut",
        }[name];
        options[field] = result.value;
        if (["cwd", "initializeFixture", "traceOut", "summaryOut"].includes(field)) {
          options[field] = resolve(result.value);
        }
        index = result.nextIndex;
        break;
      }
      case "--timeout-ms":
      case "--quiet-ms": {
        const result = takeOption(argv, index, name, inlineValue);
        const value = Number(result.value);
        if (!Number.isInteger(value) || value <= 0) {
          throw new Error(`${name} must be a positive integer`);
        }
        options[name === "--timeout-ms" ? "timeoutMs" : "quietMs"] = value;
        index = result.nextIndex;
        break;
      }
      default:
        throw new Error(`Unknown option: ${argument}\n\n${usage()}`);
    }
  }

  if (options.traceOut && options.summaryOut && options.traceOut === options.summaryOut) {
    throw new Error("--trace-out and --summary-out must point to different files");
  }
  if (options.initializeFixture && !options.dryRun && options.binary !== "grok") {
    throw new Error("--initialize-fixture cannot be combined with --binary");
  }
  return options;
}

function initializeRequest(clientVersion) {
  return {
    jsonrpc: "2.0",
    id: 1,
    method: "initialize",
    params: {
      protocolVersion: 1,
      clientCapabilities: {
        fs: { readTextFile: false, writeTextFile: false },
        terminal: false,
      },
      clientInfo: {
        name: "AgentCabin",
        version: clientVersion,
      },
      _meta: {
        clientType: "agentcabin",
        clientVersion,
      },
    },
  };
}

function sessionNewRequest(id, cwd, permissionMode) {
  const meta = {
    yoloMode: permissionMode === "bypassPermissions",
  };
  if (permissionMode === "auto") {
    meta.autoMode = true;
  }
  return {
    jsonrpc: "2.0",
    id,
    method: "session/new",
    params: {
      cwd,
      mcpServers: [],
      _meta: meta,
    },
  };
}

function promptRequest(id, sessionId, text) {
  return {
    jsonrpc: "2.0",
    id,
    method: "session/prompt",
    params: {
      sessionId,
      prompt: [{ type: "text", text }],
    },
  };
}

function effortRequest(id, sessionId, method, level) {
  return {
    jsonrpc: "2.0",
    id,
    method,
    params: {
      sessionId,
      effort: level,
    },
  };
}

function jsonLine(value) {
  return `${JSON.stringify(value)}\n`;
}

function parseJsonLine(raw) {
  try {
    return JSON.parse(raw.trim());
  } catch {
    return undefined;
  }
}

function idKey(id) {
  return typeof id === "string" ? id : JSON.stringify(id);
}

function appendLines(buffer, chunk, onLine) {
  let next = buffer + chunk;
  let newline;
  while ((newline = next.indexOf("\n")) !== -1) {
    onLine(next.slice(0, newline));
    next = next.slice(newline + 1);
  }
  return next;
}

function compact(value, maxLength = 1_000) {
  if (value === undefined) return null;
  if (typeof value === "string") {
    return value.length > maxLength ? `${value.slice(0, maxLength)}…` : value;
  }
  try {
    const text = JSON.stringify(value);
    return text.length > maxLength ? `${text.slice(0, maxLength)}…` : value;
  } catch {
    return String(value);
  }
}

function pathFor(parent, key) {
  if (/^[A-Za-z_$][\w$]*$/.test(String(key))) {
    return `${parent}.${key}`;
  }
  return `${parent}[${JSON.stringify(String(key))}]`;
}

function collectSignalPaths(value, terms, path = "$", hits = [], depth = 0) {
  if (depth > 8 || hits.length >= MAX_SIGNAL_HITS || value === null || value === undefined) {
    return hits;
  }
  if (Array.isArray(value)) {
    value.forEach((child, index) =>
      collectSignalPaths(child, terms, `${path}[${index}]`, hits, depth + 1),
    );
    return hits;
  }
  if (typeof value !== "object") return hits;

  for (const [key, child] of Object.entries(value)) {
    const childPath = pathFor(path, key);
    const haystack = `${key} ${typeof child === "string" ? child : ""}`.toLowerCase();
    if (terms.some((term) => haystack.includes(term))) {
      hits.push({ path: childPath, value: compact(child) });
    }
    collectSignalPaths(child, terms, childPath, hits, depth + 1);
    if (hits.length >= MAX_SIGNAL_HITS) break;
  }
  return hits;
}

function updateObject(message) {
  return message?.params?.update && typeof message.params.update === "object"
    ? message.params.update
    : undefined;
}

function updateKind(message) {
  const update = updateObject(message);
  return update?.sessionUpdate ?? update?.type ?? undefined;
}

function inboundNotifications(capture, { afterFrame = 0 } = {}) {
  return capture.frames
    .slice(afterFrame)
    .filter(
      (frame) =>
        frame.direction === "in" &&
        frame.json &&
        typeof frame.json === "object" &&
        frame.json.method &&
        frame.json.id === undefined,
    );
}

function unique(values) {
  return [...new Set(values.filter((value) => value !== undefined && value !== null))];
}

function modelStateFromInitialize(result) {
  return result?._meta?.modelState ?? result?.modelState ?? null;
}

function effortModelEvidence(result) {
  const modelState = modelStateFromInitialize(result);
  const models = Array.isArray(modelState?.availableModels) ? modelState.availableModels : [];
  const entries = models.map((model) => {
    const meta = model?._meta ?? model?.meta ?? {};
    return {
      modelId: model?.modelId ?? model?.id ?? null,
      supportsReasoningEffort:
        meta.supportsReasoningEffort ?? model?.supportsReasoningEffort ?? null,
      currentEffort: meta.reasoningEffort ?? model?.reasoningEffort ?? null,
      levels: Array.isArray(meta.reasoningEfforts)
        ? meta.reasoningEfforts.map((level) => level?.id ?? level?.value ?? level).filter(Boolean)
        : [],
    };
  });
  return {
    modelStatePresent: Boolean(modelState),
    currentModelId: modelState?.currentModelId ?? null,
    models: entries,
    metadataAdvertisesEffort: entries.some(
      (entry) => entry.supportsReasoningEffort === true || entry.levels.length > 0,
    ),
  };
}

function availableCommandsFromInitialize(result) {
  const commands = result?._meta?.availableCommands ?? result?.availableCommands;
  if (!Array.isArray(commands)) return [];
  return commands
    .filter((command) => command && typeof command === "object")
    .map((command) => ({
      name: command.name ?? null,
      description: command.description ?? null,
      inputHint: command.input?.hint ?? null,
    }));
}

function initializeEvidence(result) {
  const effortMetadataPaths = collectSignalPaths(result, [
    "effort",
    "reasoningeffort",
    "reasoning_effort",
  ]);
  const effortMethodPaths = collectSignalPaths(result, [
    "set_effort",
    "seteffort",
    "effortmutation",
    "effortmethod",
  ]);
  const goalPaths = collectSignalPaths(result, [
    "goalstate",
    "goal_status",
    "goalstatus",
    "goal_state",
  ]);
  const methodPaths = collectSignalPaths(result, [
    "set_effort",
    "seteffort",
    "goalstate",
    "mutation",
  ]);
  const commands = availableCommandsFromInitialize(result);
  return {
    protocolVersion: result?.protocolVersion ?? null,
    agentVersion: result?._meta?.agentVersion ?? null,
    model: effortModelEvidence(result),
    availableCommands: commands,
    goalCommandAdvertised: commands.some((command) => command.name === "goal"),
    effortMetadataHints: effortMetadataPaths,
    effortMethodHints: effortMethodPaths,
    goalStateHints: goalPaths,
    methodMetadataHints: methodPaths,
    promptCapabilities: result?.agentCapabilities?.promptCapabilities ?? null,
    sessionCapabilities: result?.agentCapabilities?.sessionCapabilities ?? null,
  };
}

function sessionEvidence(result) {
  const modes = result?.modes ?? null;
  const availableModes = Array.isArray(modes?.availableModes) ? modes.availableModes : [];
  const effortPaths = collectSignalPaths(result, ["effort", "reasoningeffort", "set_effort"]);
  const goalPaths = collectSignalPaths(result, [
    "goalstate",
    "goal_status",
    "goalstatus",
    "goal_state",
  ]);
  return {
    sessionId: result?.sessionId ?? null,
    modes,
    acpPlanModeAdvertised: availableModes.some((mode) => mode?.id === "plan"),
    effortHints: effortPaths,
    goalStateHints: goalPaths,
  };
}

function notificationEvidence(notifications) {
  const kinds = unique(notifications.map(updateKind));
  const planNotifications = notifications.filter((message) => updateKind(message) === "plan");
  const goalKinds = kinds.filter((kind) => /goal|objective|autonomous/i.test(kind));
  const effortKinds = kinds.filter((kind) => /effort|reasoning/i.test(kind));
  const goalPaths = notifications.flatMap((message) =>
    collectSignalPaths(message, ["goal", "goalstate", "goal_status", "verification", "progress"]),
  );
  const effortPaths = notifications.flatMap((message) =>
    collectSignalPaths(message, ["effort", "reasoningeffort", "set_effort"]),
  );
  const structuredGoalPaths = goalPaths.filter((hit) =>
    /goal|verification|progress/i.test(hit.path),
  );
  const structuredEffortPaths = effortPaths.filter((hit) => /effort|reasoning/i.test(hit.path));
  return {
    count: notifications.length,
    methods: unique(notifications.map((message) => message.json?.method)),
    updateKinds: kinds,
    structuredPlanUpdateCount: planNotifications.length,
    structuredPlanUpdates: planNotifications.map((message) => updateObject(message)),
    goalUpdateKinds: goalKinds,
    goalSignalPaths: structuredGoalPaths.slice(0, MAX_SIGNAL_HITS),
    effortUpdateKinds: effortKinds,
    effortSignalPaths: structuredEffortPaths.slice(0, MAX_SIGNAL_HITS),
    structuredGoalStateObserved: goalKinds.length > 0 || structuredGoalPaths.length > 0,
    structuredEffortStateObserved: effortKinds.length > 0 || structuredEffortPaths.length > 0,
  };
}

function responseStatus(response) {
  if (!response) return "missing";
  if (response.error) {
    return response.error.code === -32601 ? "method_not_found" : "error";
  }
  return "success";
}

function buildSummary(capture, options) {
  const initializeResult = capture.results.initialize ?? null;
  const sessionResult = capture.results.sessionNew ?? null;
  const initialize = initializeEvidence(initializeResult);
  const session = sessionEvidence(sessionResult);
  const allNotifications = inboundNotifications(capture);
  const goalNotifications = inboundNotifications(capture, {
    afterFrame: capture.goal?.beforeFrameCount ?? Number.MAX_SAFE_INTEGER,
  });
  const startupNotifications = inboundNotifications(capture, { afterFrame: 0 });
  const effortResponse = capture.results.effortProbe ?? null;
  const effortMethodRecognized = Boolean(
    effortResponse && (!effortResponse.error || effortResponse.error.code !== -32601),
  );
  const effortAccepted = Boolean(effortResponse && !effortResponse.error);
  const goal = {
    requested: !options.skipGoal,
    command: options.skipGoal ? null : options.goalCommand,
    responseStatus: responseStatus(capture.results.goalPrompt),
    before: notificationEvidence(inboundNotifications(capture)),
    after: notificationEvidence(goalNotifications),
    structuredStateObserved: notificationEvidence(goalNotifications).structuredGoalStateObserved,
  };
  return {
    format: "agentcabin.grok-acp-p5-summary.v1",
    capturedAt: capture.capturedAt,
    probeMode: capture.probeMode,
    probeError: capture.error ?? null,
    agentVersion: initialize.agentVersion,
    initialize: {
      ...initialize,
      initializeMethodMetadataObserved: initialize.methodMetadataHints.length > 0,
      effortMutationPrimitiveAdvertised: initialize.effortMethodHints.some((hit) =>
        /set_effort|seteffort|mutation/i.test(`${hit.path} ${JSON.stringify(hit.value)}`),
      ),
      goalStatePrimitiveAdvertised: initialize.goalStateHints.length > 0,
    },
    session: {
      ...session,
      startupNotifications: notificationEvidence(startupNotifications),
    },
    plan: {
      hostPlanMode: {
        supportedByAgentCabinAdapter: true,
        selectedForProbe: options.permissionMode === "plan",
      },
      acpPlanModeAdvertised: session.acpPlanModeAdvertised,
      structuredPlanUpdateCount: notificationEvidence(allNotifications).structuredPlanUpdateCount,
      structuredPlanStateObserved:
        notificationEvidence(allNotifications).structuredPlanUpdateCount > 0,
    },
    effort: {
      modelMetadataAdvertisesEffort: initialize.model.metadataAdvertisesEffort,
      methodMetadataObserved: initialize.effortMethodHints.length > 0,
      mutationAttempted: Boolean(capture.effortProbe),
      method: capture.effortProbe?.method ?? null,
      requestedLevel: capture.effortProbe?.level ?? null,
      responseStatus: responseStatus(effortResponse),
      methodRecognized: effortMethodRecognized,
      accepted: effortAccepted,
      structuredStateObserved: notificationEvidence(allNotifications).structuredEffortStateObserved,
      primitiveVerified: effortAccepted,
      note: effortAccepted
        ? "The candidate method returned success; inspect the raw trace and follow-up model/update state before enabling UI."
        : "No successful live effort mutation was observed.",
    },
    goal,
    limitations: [
      ...(capture.error ? [`Live probe stopped before completion: ${capture.error}`] : []),
      ...(capture.probeMode === "initialize-fixture"
        ? ["Fixture mode cannot observe session/new, session/prompt, or live notifications."]
        : []),
      ...(options.skipGoal ? ["Goal prompt was skipped by --skip-goal."] : []),
      ...(options.probeEffort
        ? []
        : ["Effort mutation was not attempted; pass --probe-effort for an opt-in live probe."]),
    ],
  };
}

function createCapture(options, probeMode = "live") {
  return {
    format: "agentcabin.grok-acp-p5-trace.v1",
    capturedAt: new Date().toISOString(),
    probeMode,
    command:
      probeMode === "live"
        ? {
            binary: options.binary,
            args: [
              "--no-auto-update",
              "--permission-mode",
              options.permissionMode,
              "agent",
              "stdio",
            ],
          }
        : null,
    cwd: options.cwd,
    options: {
      permissionMode: options.permissionMode,
      goalCommand: options.goalCommand,
      skipGoal: options.skipGoal,
      probeEffort: options.probeEffort,
      effortMethod: options.effortMethod,
      effortLevel: options.effortLevel ?? null,
    },
    requests: [],
    frames: [],
    autoReplies: [],
    results: {
      initialize: null,
      authenticate: null,
      sessionNew: null,
      effortProbe: null,
      goalPrompt: null,
    },
    goal: null,
    stderr: "",
    exitCode: null,
    signal: null,
    durationMs: null,
  };
}

class AcpProbeProcess {
  constructor(options, capture) {
    this.options = options;
    this.capture = capture;
    this.child = null;
    this.stdoutBuffer = "";
    this.stdoutDecoder = new StringDecoder("utf8");
    this.stderrDecoder = new StringDecoder("utf8");
    this.pending = new Map();
    this.nextId = 1;
    this.phase = "startup";
    this.lastInboundAt = Date.now();
    this.closed = false;
    this.closePromise = null;
  }

  start() {
    const args = this.capture.command.args;
    this.child = spawn(this.options.binary, args, {
      cwd: this.options.cwd,
      env: process.env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.closePromise = new Promise((resolveClose) => {
      this.child.on("close", (code, signal) => {
        this.closed = true;
        this.capture.exitCode = code;
        this.capture.signal = signal;
        for (const pending of this.pending.values()) {
          clearTimeout(pending.timer);
          pending.reject(new Error("Grok ACP process closed before the response arrived"));
        }
        this.pending.clear();
        resolveClose();
      });
    });
    this.child.on("error", (error) => {
      this.capture.processError = error.message;
      for (const pending of this.pending.values()) {
        clearTimeout(pending.timer);
        pending.reject(error);
      }
      this.pending.clear();
    });
    this.child.stdout.on("data", (chunk) => {
      this.stdoutBuffer = appendLines(this.stdoutBuffer, this.stdoutDecoder.write(chunk), (line) =>
        this.recordInbound(line),
      );
    });
    this.child.stderr.on("data", (chunk) => {
      this.capture.stderr += this.stderrDecoder.write(chunk);
    });
  }

  recordInbound(raw) {
    const json = parseJsonLine(raw);
    this.lastInboundAt = Date.now();
    this.capture.frames.push({
      direction: "in",
      phase: this.phase,
      at: new Date().toISOString(),
      raw,
      json: json ?? null,
    });
    if (!json || typeof json !== "object") return;

    if (json.id !== undefined && json.method === undefined) {
      const pending = this.pending.get(idKey(json.id));
      if (pending) {
        this.pending.delete(idKey(json.id));
        clearTimeout(pending.timer);
        pending.resolve(json);
      }
      return;
    }
    if (json.method && json.id !== undefined) {
      this.replyToServerRequest(json);
    }
  }

  replyToServerRequest(message) {
    const reply = {
      jsonrpc: "2.0",
      id: message.id,
      error: {
        code: -32001,
        message: `P5 probe does not answer server request ${message.method}`,
      },
    };
    this.capture.autoReplies.push({
      method: message.method,
      requestId: message.id,
      response: reply,
    });
    void this.write(reply, "auto_reply");
  }

  write(frame, phase = this.phase) {
    if (!this.child || this.closed) {
      return Promise.reject(new Error("Grok ACP process is not running"));
    }
    this.capture.frames.push({
      direction: "out",
      phase,
      at: new Date().toISOString(),
      raw: jsonLine(frame).trimEnd(),
      json: frame,
    });
    return new Promise((resolveWrite, rejectWrite) => {
      this.child.stdin.write(jsonLine(frame), (error) => {
        if (error) rejectWrite(error);
        else resolveWrite();
      });
    });
  }

  waitForResponse(id) {
    return new Promise((resolveResponse, rejectResponse) => {
      const timer = setTimeout(() => {
        this.pending.delete(idKey(id));
        rejectResponse(
          new Error(`Timed out after ${this.options.timeoutMs}ms waiting for ACP response ${id}`),
        );
      }, this.options.timeoutMs);
      this.pending.set(idKey(id), { resolve: resolveResponse, reject: rejectResponse, timer });
    });
  }

  async request(method, params, phase) {
    const id = this.nextId;
    this.nextId += 1;
    const frame = { jsonrpc: "2.0", id, method, params };
    const response = this.waitForResponse(id);
    this.phase = phase;
    this.capture.requests.push({ id, method, params, phase });
    await this.write(frame, phase);
    const result = await response;
    this.capture.requests.at(-1).response = result;
    return result;
  }

  async waitForQuiet() {
    const startedAt = Date.now();
    while (!this.closed) {
      const quietFor = Date.now() - this.lastInboundAt;
      const elapsed = Date.now() - startedAt;
      if (quietFor >= this.options.quietMs || elapsed >= this.options.timeoutMs) return;
      await new Promise((resolveDelay) =>
        setTimeout(resolveDelay, Math.min(100, this.options.quietMs)),
      );
    }
  }

  async stop() {
    if (!this.child || this.closed) return;
    this.child.stdin.end();
    this.child.kill("SIGTERM");
    await Promise.race([
      this.closePromise,
      new Promise((resolveDelay) => setTimeout(resolveDelay, TERMINATION_GRACE_MS)),
    ]);
    if (!this.closed) this.child.kill("SIGKILL");
    await this.closePromise;
  }

  async finish() {
    this.stdoutBuffer = appendLines(this.stdoutBuffer, this.stdoutDecoder.end(), (line) =>
      this.recordInbound(line),
    );
    if (this.stdoutBuffer) {
      this.recordInbound(this.stdoutBuffer);
      this.stdoutBuffer = "";
    }
    this.capture.stderr += this.stderrDecoder.end();
  }
}

function authMethod(result) {
  const methods = Array.isArray(result?.authMethods) ? result.authMethods : [];
  const hasApiKey = Boolean(process.env.XAI_API_KEY?.trim());
  if (hasApiKey) return null;
  for (const preferred of ["cached_token", "xai.api_key"]) {
    const match = methods.find((method) => method?.id === preferred);
    if (match?.id) return match.id;
  }
  return methods.find((method) => typeof method?.id === "string")?.id ?? null;
}

function responseResult(response, label) {
  if (!response) throw new Error(`Grok ACP did not return ${label} response`);
  if (response.error) {
    const message = response.error.message ?? JSON.stringify(response.error);
    throw new Error(`Grok ACP ${label} failed: ${message}`);
  }
  return response.result ?? {};
}

function currentEffort(initializeResult, requestedLevel) {
  if (requestedLevel) return requestedLevel;
  const evidence = effortModelEvidence(initializeResult);
  return evidence.models.find((model) => model.currentEffort)?.currentEffort ?? "medium";
}

async function runLive(options) {
  const capture = createCapture(options);
  const processProbe = new AcpProbeProcess(options, capture);
  const startedAt = Date.now();
  processProbe.start();
  try {
    processProbe.phase = "initialize";
    const initializeResponse = await processProbe.request(
      "initialize",
      initializeRequest(options.clientVersion).params,
      "initialize",
    );
    capture.results.initialize = responseResult(initializeResponse, "initialize");
    const agentVersion = capture.results.initialize?._meta?.agentVersion;
    if (options.expectedAgentVersion && agentVersion !== options.expectedAgentVersion) {
      throw new Error(
        `Expected Grok agentVersion ${options.expectedAgentVersion}, received ${agentVersion ?? "missing"}`,
      );
    }

    const selectedAuthMethod = authMethod(capture.results.initialize);
    if (selectedAuthMethod) {
      processProbe.phase = "authenticate";
      const response = await processProbe.request(
        "authenticate",
        { methodId: selectedAuthMethod },
        "authenticate",
      );
      capture.results.authenticate = response;
      responseResult(response, "authenticate");
    }

    processProbe.phase = "session_new";
    const sessionResponse = await processProbe.request(
      "session/new",
      sessionNewRequest(2, options.cwd, options.permissionMode).params,
      "session_new",
    );
    capture.results.sessionNew = responseResult(sessionResponse, "session/new");
    const sessionId = capture.results.sessionNew?.sessionId;
    if (typeof sessionId !== "string" || !sessionId) {
      throw new Error("Grok ACP session/new response did not include sessionId");
    }
    await processProbe.waitForQuiet();

    if (options.probeEffort) {
      const level = currentEffort(capture.results.initialize, options.effortLevel);
      const request = effortRequest(processProbe.nextId, sessionId, options.effortMethod, level);
      capture.effortProbe = {
        method: request.method,
        level,
        request,
      };
      processProbe.phase = "effort_probe";
      try {
        capture.results.effortProbe = await processProbe.request(
          request.method,
          request.params,
          "effort_probe",
        );
      } catch (error) {
        capture.effortProbe.error = error.message;
      }
      await processProbe.waitForQuiet();
    }

    const goalStartFrame = capture.frames.length;
    await processProbe.waitForQuiet();
    const goalBeforeFrameCount = capture.frames.length;
    capture.goal = {
      beforeStartFrame: goalStartFrame,
      beforeFrameCount: goalBeforeFrameCount,
      command: options.skipGoal ? null : options.goalCommand,
    };

    if (!options.skipGoal) {
      processProbe.phase = "goal_after_request";
      try {
        capture.results.goalPrompt = await processProbe.request(
          "session/prompt",
          promptRequest(processProbe.nextId, sessionId, options.goalCommand).params,
          "goal_after_request",
        );
      } catch (error) {
        capture.goal.error = error.message;
      }
      await processProbe.waitForQuiet();
    }
  } catch (error) {
    capture.error = error.message;
    throw new ProbeError(error.message, capture);
  } finally {
    capture.durationMs = Date.now() - startedAt;
    await processProbe.stop();
    await processProbe.finish();
  }
  capture.summary = buildSummary(capture, options);
  return capture;
}

async function loadFixture(path) {
  const raw = await readFile(path, "utf8");
  const value = JSON.parse(raw);
  if (value?.responseResult && typeof value.responseResult === "object")
    return value.responseResult;
  if (value?.response?.result && typeof value.response.result === "object")
    return value.response.result;
  return value;
}

async function runFixture(options) {
  const result = await loadFixture(options.initializeFixture);
  const capture = createCapture(options, "initialize-fixture");
  capture.results.initialize = result;
  capture.summary = buildSummary(capture, options);
  capture.summary.limitations.unshift(
    "This is an initialize-only fixture inspection; no live ACP method was called.",
  );
  return capture;
}

async function assertOutputTargetsAvailable(paths, force) {
  if (force) return;
  for (const path of paths.filter(Boolean)) {
    try {
      await access(path, constants.F_OK);
      throw new Error(`Refusing to overwrite existing file: ${path} (use --force)`);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
}

async function writeOutput(path, content) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, content, "utf8");
}

function formatJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function dryRun(options) {
  const init = initializeRequest(options.clientVersion);
  return {
    format: "agentcabin.grok-acp-p5-dry-run.v1",
    initialize: init,
    sessionNew: sessionNewRequest(2, options.cwd, options.permissionMode),
    effort: options.probeEffort
      ? effortRequest(
          3,
          "<sessionId>",
          options.effortMethod,
          options.effortLevel ?? "<current-effort>",
        )
      : null,
    goal: options.skipGoal
      ? null
      : promptRequest(options.probeEffort ? 4 : 3, "<sessionId>", options.goalCommand),
    note: "Dry run does not contact Grok or prove ACP support.",
  };
}

async function main() {
  const options = await parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    return;
  }
  await assertOutputTargetsAvailable([options.traceOut, options.summaryOut], options.force);
  if (options.dryRun) {
    process.stdout.write(formatJson(dryRun(options)));
    return;
  }

  let capture;
  try {
    capture = options.initializeFixture ? await runFixture(options) : await runLive(options);
  } catch (error) {
    capture = error instanceof ProbeError ? error.capture : undefined;
    if (capture) {
      capture.summary = buildSummary(capture, options);
      if (options.traceOut) await writeOutput(options.traceOut, formatJson(capture));
      if (options.summaryOut) await writeOutput(options.summaryOut, formatJson(capture.summary));
    }
    throw error;
  }

  if (options.traceOut) await writeOutput(options.traceOut, formatJson(capture));
  if (options.summaryOut) await writeOutput(options.summaryOut, formatJson(capture.summary));
  process.stdout.write(formatJson(capture.summary));
  console.error(
    `Grok ACP P5 probe complete: mode=${capture.probeMode}, agentVersion=${capture.summary.agentVersion ?? "unknown"}, frames=${capture.frames.length}`,
  );
}

main().catch((error) => {
  console.error(`Grok ACP P5 probe failed: ${error.message}`);
  process.exitCode = 1;
});
