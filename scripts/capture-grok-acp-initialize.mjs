#!/usr/bin/env node
/**
 * Capture one Grok Build ACP initialize exchange.
 *
 * The actor sends the same request from `grok_session_actor::initialize_params`.
 * This tool stops the child process immediately after the matching initialize
 * response arrives, so it does not create a session or send a prompt.
 *
 * Examples:
 *   node scripts/capture-grok-acp-initialize.mjs \
 *     --binary /path/to/grok \
 *     --expected-agent-version 1.0.0 \
 *     --response-out src-tauri/src/agent/grok_session_actor/fixtures/grok_build_1_0_initialize.wire.json \
 *     --trace-out /tmp/grok-build-1.0-initialize.trace.json
 *
 * The response output contains only JSON-RPC `result`, which is the shape used
 * by the checked-in source-derived fixture. The trace output is an envelope
 * containing the request, every stdout line, stderr, and the full response.
 * Existing output files are never overwritten unless --force is supplied.
 */

import { spawn } from "node:child_process";
import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { dirname, resolve } from "node:path";
import process from "node:process";
import { StringDecoder } from "node:string_decoder";

const PROJECT_ROOT = resolve(import.meta.dirname, "..");
const DEFAULT_TIMEOUT_MS = 30_000;
const TERMINATION_GRACE_MS = 1_000;

class CaptureError extends Error {
  constructor(message, capture) {
    super(message);
    this.name = "CaptureError";
    this.capture = capture;
  }
}

function usage() {
  return `Usage: node scripts/capture-grok-acp-initialize.mjs [options]

Options:
  --binary <path>                  Grok executable (default: GROK_BINARY or grok)
  --cwd <path>                     Working directory for Grok (default: current directory)
  --permission-mode <mode>         Grok permission mode (default: default)
  --client-version <version>       AgentCabin version in initialize (default: Cargo version)
  --expected-agent-version <ver>   Fail unless response _meta.agentVersion matches
  --timeout-ms <milliseconds>      Capture timeout (default: ${DEFAULT_TIMEOUT_MS})
  --response-out <path>            Write response.result as formatted JSON
  --trace-out <path>               Write request/response/raw-line capture as JSON
  --force                          Allow existing output files to be overwritten
  --dry-run                        Print the initialize request without spawning Grok
  --help                           Show this help

When --response-out is omitted, the response.result is printed to stdout. Use
--trace-out to retain the raw exchange for review and provenance.
`;
}

async function defaultClientVersion() {
  const cargoPath = resolve(PROJECT_ROOT, "src-tauri/Cargo.toml");
  try {
    const cargo = await readFile(cargoPath, "utf8");
    const match = cargo.match(/^version\s*=\s*"([^"]+)"/m);
    return match?.[1] ?? "unknown";
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
    responseOut: undefined,
    traceOut: undefined,
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
      case "--binary": {
        const result = takeOption(argv, index, name, inlineValue);
        options.binary = result.value;
        index = result.nextIndex;
        break;
      }
      case "--cwd": {
        const result = takeOption(argv, index, name, inlineValue);
        options.cwd = resolve(result.value);
        index = result.nextIndex;
        break;
      }
      case "--permission-mode": {
        const result = takeOption(argv, index, name, inlineValue);
        options.permissionMode = result.value;
        index = result.nextIndex;
        break;
      }
      case "--client-version": {
        const result = takeOption(argv, index, name, inlineValue);
        options.clientVersion = result.value;
        index = result.nextIndex;
        break;
      }
      case "--expected-agent-version": {
        const result = takeOption(argv, index, name, inlineValue);
        options.expectedAgentVersion = result.value;
        index = result.nextIndex;
        break;
      }
      case "--timeout-ms": {
        const result = takeOption(argv, index, name, inlineValue);
        options.timeoutMs = Number(result.value);
        if (!Number.isInteger(options.timeoutMs) || options.timeoutMs <= 0) {
          throw new Error("--timeout-ms must be a positive integer");
        }
        index = result.nextIndex;
        break;
      }
      case "--response-out": {
        const result = takeOption(argv, index, name, inlineValue);
        options.responseOut = resolve(result.value);
        index = result.nextIndex;
        break;
      }
      case "--trace-out": {
        const result = takeOption(argv, index, name, inlineValue);
        options.traceOut = resolve(result.value);
        index = result.nextIndex;
        break;
      }
      default:
        throw new Error(`Unknown option: ${argument}\n\n${usage()}`);
    }
  }

  if (options.responseOut && options.traceOut && options.responseOut === options.traceOut) {
    throw new Error("--response-out and --trace-out must point to different files");
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

function isInitializeResponse(message) {
  return (
    message &&
    typeof message === "object" &&
    !Array.isArray(message) &&
    message.id === 1 &&
    message.method === undefined
  );
}

function responseError(message) {
  const error = message?.error;
  if (!error || typeof error !== "object") {
    return undefined;
  }
  const code = error.code === undefined ? "unknown" : error.code;
  const text = typeof error.message === "string" ? error.message : JSON.stringify(error);
  return `Grok ACP initialize returned error ${code}: ${text}`;
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

function buildCapture({ request, command, cwd }) {
  return {
    format: "agentcabin.grok-acp-initialize.v1",
    capturedAt: new Date().toISOString(),
    command,
    cwd,
    request,
    requestRaw: jsonLine(request).trimEnd(),
    stdout: [],
    stderr: "",
    response: null,
    responseResult: null,
    exitCode: null,
    signal: null,
    durationMs: null,
  };
}

function captureInitialize({ binary, args, cwd, request, timeoutMs }) {
  const command = { binary, args };
  const capture = buildCapture({ request, command, cwd });
  const startedAt = Date.now();
  const child = spawn(binary, args, {
    cwd,
    env: process.env,
    stdio: ["pipe", "pipe", "pipe"],
  });
  let stdoutBuffer = "";
  let responseSeen = false;
  let terminationTimer;
  let timeoutTimer;
  let settled = false;
  const stdoutDecoder = new StringDecoder("utf8");
  const stderrDecoder = new StringDecoder("utf8");

  const terminate = () => {
    const isRunning = child.exitCode === null && child.signalCode === null;
    if (isRunning) {
      child.kill("SIGTERM");
    }
    terminationTimer = setTimeout(() => {
      if (child.exitCode === null && child.signalCode === null) {
        child.kill("SIGKILL");
      }
    }, TERMINATION_GRACE_MS);
  };

  const finishCapture = () => {
    capture.durationMs = Date.now() - startedAt;
    capture.exitCode = child.exitCode;
    capture.signal = child.signalCode;
    if (terminationTimer) {
      clearTimeout(terminationTimer);
    }
  };

  const promise = new Promise((resolveCapture, rejectCapture) => {
    const fail = (message) => {
      if (settled) return;
      settled = true;
      finishCapture();
      rejectCapture(new CaptureError(message, capture));
    };

    child.on("error", (error) => {
      fail(`Failed to start Grok ACP process: ${error.message}`);
    });

    child.stdin.on("error", (error) => {
      if (!responseSeen && !settled) {
        fail(`Failed to write Grok ACP initialize request: ${error.message}`);
      }
    });

    const recordStdoutLine = (line) => {
      const message = parseJsonLine(line);
      capture.stdout.push({ raw: line, json: message ?? null });
      if (!responseSeen && message && isInitializeResponse(message)) {
        responseSeen = true;
        capture.response = message;
        capture.responseResult = message.result ?? null;
        const error = responseError(message);
        if (error) {
          terminate();
          fail(error);
          return;
        }
        if (
          !message.result ||
          typeof message.result !== "object" ||
          Array.isArray(message.result)
        ) {
          terminate();
          fail("Grok ACP initialize response did not contain an object result");
          return;
        }
        child.stdin.end();
        terminate();
      }
    };

    child.stdout.on("data", (chunk) => {
      stdoutBuffer = appendLines(stdoutBuffer, stdoutDecoder.write(chunk), recordStdoutLine);
    });

    child.stderr.on("data", (chunk) => {
      capture.stderr += stderrDecoder.write(chunk);
    });

    child.on("close", (code, signal) => {
      stdoutBuffer = appendLines(stdoutBuffer, stdoutDecoder.end(), recordStdoutLine);
      if (stdoutBuffer) {
        recordStdoutLine(stdoutBuffer);
        stdoutBuffer = "";
      }
      capture.stderr += stderrDecoder.end();
      capture.exitCode = code;
      capture.signal = signal;
      finishCapture();
      if (settled) return;
      settled = true;
      if (!responseSeen) {
        rejectCapture(
          new CaptureError(
            `Grok ACP exited before initialize response (code ${code ?? "null"}, signal ${signal ?? "null"})`,
            capture,
          ),
        );
        return;
      }
      resolveCapture(capture);
    });

    timeoutTimer = setTimeout(() => {
      terminate();
      fail(`Timed out after ${timeoutMs}ms waiting for Grok ACP initialize response`);
    }, timeoutMs);

    const requestLine = jsonLine(request);
    child.stdin.write(requestLine, (error) => {
      if (error && !responseSeen && !settled) {
        fail(`Failed to write Grok ACP initialize request: ${error.message}`);
      }
    });
  });

  return promise.finally(() => {
    if (timeoutTimer) {
      clearTimeout(timeoutTimer);
    }
    if (terminationTimer) {
      clearTimeout(terminationTimer);
    }
  });
}

async function writeOutput(path, content, force) {
  if (!force) {
    try {
      await access(path, constants.F_OK);
      throw new Error(`Refusing to overwrite existing file: ${path} (use --force)`);
    } catch (error) {
      if (error.code !== "ENOENT") {
        throw error;
      }
    }
  }
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, content, "utf8");
}

async function assertOutputTargetsAvailable(paths, force) {
  if (force) return;
  for (const path of paths) {
    try {
      await access(path, constants.F_OK);
      throw new Error(`Refusing to overwrite existing file: ${path} (use --force)`);
    } catch (error) {
      if (error.code !== "ENOENT") {
        throw error;
      }
    }
  }
}

function formatResponse(result) {
  return `${JSON.stringify(result, null, 2)}\n`;
}

function formatTrace(capture, error) {
  const trace = error ? { ...capture, error: error.message } : capture;
  return `${JSON.stringify(trace, null, 2)}\n`;
}

async function main() {
  const options = await parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    return;
  }

  const request = initializeRequest(options.clientVersion);
  if (options.dryRun) {
    process.stdout.write(formatResponse(request));
    return;
  }

  await assertOutputTargetsAvailable(
    [options.responseOut, options.traceOut].filter(Boolean),
    options.force,
  );

  const args = ["--no-auto-update", "--permission-mode", options.permissionMode, "agent", "stdio"];

  let capture;
  try {
    capture = await captureInitialize({
      binary: options.binary,
      args,
      cwd: options.cwd,
      request,
      timeoutMs: options.timeoutMs,
    });
  } catch (error) {
    capture = error instanceof CaptureError ? error.capture : undefined;
    if (options.traceOut && capture) {
      await writeOutput(options.traceOut, formatTrace(capture, error), options.force);
      console.error(`Wrote failed capture trace: ${options.traceOut}`);
    }
    throw error;
  }

  const result = capture.responseResult;
  const agentVersion = result?._meta?.agentVersion;
  if (options.expectedAgentVersion && agentVersion !== options.expectedAgentVersion) {
    const error = new CaptureError(
      `Expected Grok agentVersion ${options.expectedAgentVersion}, received ${agentVersion ?? "missing"}`,
      capture,
    );
    if (options.traceOut) {
      await writeOutput(options.traceOut, formatTrace(capture, error), options.force);
      console.error(`Wrote failed capture trace: ${options.traceOut}`);
    }
    throw error;
  }

  if (options.responseOut) {
    await writeOutput(options.responseOut, formatResponse(result), options.force);
    console.error(`Wrote initialize result: ${options.responseOut}`);
  } else {
    process.stdout.write(formatResponse(result));
  }
  if (options.traceOut) {
    await writeOutput(options.traceOut, formatTrace(capture), options.force);
    console.error(`Wrote raw initialize trace: ${options.traceOut}`);
  }

  const lineCount = capture.stdout.length;
  console.error(
    `Captured Grok ACP initialize: agentVersion=${agentVersion ?? "unknown"}, stdoutLines=${lineCount}, durationMs=${capture.durationMs ?? "unknown"}`,
  );
}

main().catch((error) => {
  console.error(`Grok ACP capture failed: ${error.message}`);
  process.exitCode = 1;
});
