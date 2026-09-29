import path from "node:path";
import { fileURLToPath } from "node:url";
import fs from "node:fs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

function resolveExtensionsNodeModules() {
  const candidates = [
    process.env.AGENTCABIN_PI_EXTENSIONS_DIR,
    path.resolve(__dirname, "../../runtime/extensions/node_modules"),
    path.resolve(__dirname, "../runtime/extensions/node_modules"),
    path.resolve(__dirname, "runtime/extensions/node_modules"),
  ].filter(Boolean);

  for (const candidate of candidates) {
    if (fs.existsSync(path.join(candidate, "@injaneity/pi-computer-use"))) {
      return candidate;
    }
  }
  return path.resolve(__dirname, "../../runtime/extensions/node_modules");
}

export async function createComputerUseHostRuntime(options = {}) {
  const extensionsDir = options.extensionsDir || resolveExtensionsNodeModules();
  const jitiCandidates = [
    path.join(extensionsDir, "jiti/lib/jiti.mjs"),
    path.join(extensionsDir, "@earendil-works/pi-coding-agent/node_modules/jiti/lib/jiti.mjs"),
  ];
  const actualJitiPath = jitiCandidates.find((p) => fs.existsSync(p));
  const { createJiti } = actualJitiPath ? await import(actualJitiPath) : await import("jiti");

  const shimPath = path.resolve(__dirname, "./pi_coding_agent_shim.mjs");
  const realPiAgentPath = path.resolve(
    __dirname,
    "../../../runtime-build/pi/node_modules/@earendil-works/pi-coding-agent"
  );
  const piAgentTarget = fs.existsSync(realPiAgentPath) ? realPiAgentPath : shimPath;

  const jiti = createJiti(import.meta.url, {
    alias: {
      "@earendil-works/pi-coding-agent": piAgentTarget,
    },
  });

  const extensionEntry = path.join(
    extensionsDir,
    "@injaneity/pi-computer-use/extensions/computer-use.ts"
  );

  const extensionModule = await jiti.import(extensionEntry);
  const registerExtension = extensionModule.default || extensionModule;

  const tools = new Map();
  const commands = new Map();
  const eventHandlers = new Map();

  const fakePi = {
    registerTool(tool) {
      tools.set(tool.name, tool);
    },
    registerCommand(name, config) {
      commands.set(name, config);
    },
    on(event, handler) {
      if (!eventHandlers.has(event)) eventHandlers.set(event, []);
      eventHandlers.get(event).push(handler);
    },
  };

  registerExtension(fakePi);

  let helperClient = null;
  if (process.platform === "darwin") {
    try {
      const helperMod = await jiti.import(
        path.join(extensionsDir, "@injaneity/pi-computer-use/src/platform/macos/helper.ts")
      );
      helperClient = helperMod.macosHelper;
    } catch {}
  }

  function getToolDefinitions() {
    return Array.from(tools.values()).map((tool) => ({
      name: tool.name,
      label: tool.label || tool.name,
      description: tool.description || "",
      promptSnippet: tool.promptSnippet || "",
      promptGuidelines: tool.promptGuidelines || [],
      parameters: tool.parameters,
    }));
  }

  function createContextForRun(runId, runOptions = {}) {
    return {
      cwd: runOptions.cwd || process.cwd(),
      hasUI: Boolean(runOptions.hasUI),
      sessionManager: {
        getBranch: () => runOptions.branch || [],
      },
      ui: {
        notify: (message, type) => {
          if (options.onNotify) options.onNotify({ runId, message, type });
        },
        select: async () => "Cancel",
      },
    };
  }

  const activeControllers = new Map();

  function cancelTool(toolCallId) {
    if (!toolCallId) return false;
    const controller = activeControllers.get(toolCallId);
    if (controller) {
      controller.abort();
      activeControllers.delete(toolCallId);
      return true;
    }
    return false;
  }

  async function executeTool(runId, toolName, toolCallId, params, signal, runOptions = {}) {
    const tool = tools.get(toolName);
    if (!tool) {
      throw new Error(`Unknown computer use tool: ${toolName}`);
    }
    const ctx = createContextForRun(runId, runOptions);
    const startTime = Date.now();
    const effectiveCallId = toolCallId || `call-${Date.now()}`;
    let ownController = null;
    let effectiveSignal = signal;
    if (!effectiveSignal) {
      ownController = new AbortController();
      effectiveSignal = ownController.signal;
      activeControllers.set(effectiveCallId, ownController);
    } else {
      activeControllers.set(effectiveCallId, { abort: () => {} });
    }

    try {
      const result = await tool.execute(
        effectiveCallId,
        params || {},
        effectiveSignal,
        undefined,
        ctx
      );
      const durationMs = Date.now() - startTime;
      return {
        success: true,
        status: "success",
        durationMs,
        toolCallId: effectiveCallId,
        toolName,
        result,
      };
    } catch (error) {
      const durationMs = Date.now() - startTime;
      return {
        success: false,
        status: "failed",
        durationMs,
        toolCallId: effectiveCallId,
        toolName,
        stderr: error instanceof Error ? error.message : String(error),
      };
    } finally {
      activeControllers.delete(effectiveCallId);
    }
  }

  async function shutdownSession(runId) {
    const handlers = eventHandlers.get("session_shutdown") || [];
    for (const handler of handlers) {
      try {
        await handler({ runId });
      } catch {}
    }
  }

  async function getStatus() {
    if (!helperClient) {
      return {
        enabled: true,
        ready: false,
        command: "agentcabin-computer-use",
        message: "Computer Use helper unavailable on this platform.",
        accessibility: false,
        screenRecording: false,
      };
    }
    try {
      await helperClient.ensureInstalled();
      const daemonReady = await helperClient.ensureDaemon();
      if (!daemonReady) {
        return {
          enabled: true,
          ready: false,
          command: "agentcabin-computer-use",
          message: "agentcabin-computer-use bridge daemon not running on socket",
          accessibility: false,
          screenRecording: false,
        };
      }
      const perms = await helperClient.command("checkPermissions", {}).catch(() => ({}));
      const diag = await helperClient.diagnosticsCommand().catch(() => ({}));
      const accessibility = Boolean(perms.accessibility ?? diag.accessibility);
      const screenRecording = Boolean(
        perms.screenRecordingCapturable ?? perms.screenRecording ?? diag.screenRecording
      );
      return {
        enabled: true,
        ready: daemonReady && diag.protocolVersion === 6,
        command: "agentcabin-computer-use",
        message:
          accessibility && screenRecording
            ? "agentcabin-computer-use macOS bridge 与系统权限均已就绪。"
            : "agentcabin-computer-use macOS bridge 已启动；请完成辅助功能与屏幕录制授权。",
        accessibility,
        screenRecording,
      };
    } catch (err) {
      return {
        enabled: true,
        ready: false,
        command: "agentcabin-computer-use",
        message: err.message,
        accessibility: false,
        screenRecording: false,
      };
    }
  }

  async function requestPermissions() {
    if (!helperClient) return getStatus();
    try {
      await helperClient.command("registerPermissions", {
        promptAccessibility: true,
        promptScreenRecording: true,
      });
    } catch {}
    return getStatus();
  }

  async function openPermissionPane(kind) {
    if (!helperClient) return;
    const nativeKind =
      kind === "screen-recording" || kind === "screenRecording"
        ? "screenRecording"
        : "accessibility";
    try {
      await helperClient.command("registerPermissions", {
        promptAccessibility: false,
        promptScreenRecording: true,
      });
      await helperClient.command("openPermissionPane", { kind: nativeKind });
    } catch {}
  }

  return {
    tools,
    getToolDefinitions,
    executeTool,
    cancelTool,
    shutdownSession,
    getStatus,
    requestPermissions,
    openPermissionPane,
  };
}

if (process.argv.includes("--serve")) {
  import("node:readline").then(({ default: readline }) => {
    createComputerUseHostRuntime().then((runtime) => {
      const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout,
        terminal: false,
      });

      rl.on("line", async (line) => {
        if (!line.trim()) return;
        let req;
        try {
          req = JSON.parse(line);
        } catch {
          process.stdout.write(JSON.stringify({ ok: false, error: "Invalid JSON" }) + "\n");
          return;
        }

        const { id, method, runId, toolName, toolCallId, params, kind } = req;
        try {
          if (method === "get_tool_definitions") {
            const defs = runtime.getToolDefinitions();
            process.stdout.write(JSON.stringify({ id, ok: true, result: defs }) + "\n");
          } else if (method === "execute") {
            const result = await runtime.executeTool(runId, toolName, toolCallId, params);
            process.stdout.write(JSON.stringify({ id, ok: true, result }) + "\n");
          } else if (method === "cancel") {
            const cancelled = runtime.cancelTool(toolCallId);
            process.stdout.write(
              JSON.stringify({ id, ok: true, result: { cancelled } }) + "\n"
            );
          } else if (method === "release" || method === "shutdown") {
            await runtime.shutdownSession(runId);
            process.stdout.write(
              JSON.stringify({ id, ok: true, result: { released: true } }) + "\n"
            );
          } else if (method === "status") {
            const result = await runtime.getStatus();
            process.stdout.write(JSON.stringify({ id, ok: true, result }) + "\n");
          } else if (method === "request_permissions") {
            const result = await runtime.requestPermissions();
            process.stdout.write(JSON.stringify({ id, ok: true, result }) + "\n");
          } else if (method === "open_permission_pane") {
            await runtime.openPermissionPane(kind);
            process.stdout.write(
              JSON.stringify({ id, ok: true, result: { opened: true } }) + "\n"
            );
          } else {
            process.stdout.write(
              JSON.stringify({ id, ok: false, error: `Unknown method: ${method}` }) + "\n"
            );
          }
        } catch (err) {
          process.stdout.write(
            JSON.stringify({ id, ok: false, error: err.message || String(err) }) + "\n"
          );
        }
      });
    });
  });
}
