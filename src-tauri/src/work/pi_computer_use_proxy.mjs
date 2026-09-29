import fs from "node:fs";
import path from "node:path";
import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";
import { createComputerUseToolPipelineInvoker } from "./computer_use_tool_pipeline.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

function loadToolDefinitions() {
  const candidates = [
    path.join(__dirname, "pi_computer_use_definitions.json"),
    path.join(__dirname, "../pi_computer_use_definitions.json"),
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      try {
        return JSON.parse(fs.readFileSync(candidate, "utf8"));
      } catch {}
    }
  }
  return [];
}

export const LAUNCH_APP_TOOL_DEFINITION = {
  name: "launch_app",
  label: "Launch Native App",
  description: "Launch a native application and immediately return its observed UI state.",
  promptSnippet: "Use to launch or focus a native macOS desktop application before interacting with it.",
  promptGuidelines: [],
  parameters: {
    type: "object",
    properties: {
      name: {
        type: "string",
        description: "Application name (e.g. Notes, Calculator, TextEdit, Finder)",
      },
      bundleId: {
        type: "string",
        description: "Exact bundle id (e.g. com.apple.Notes)",
      },
      createsNewApplicationInstance: {
        type: "boolean",
        description: "Whether to launch a new application instance",
      },
    },
  },
};

/**
 * Register thin Computer Use proxy tools with Pi.
 *
 * All execution semantics belong to upstream @injaneity/pi-computer-use running
 * on the AgentCabin Host. This proxy only maps the Pi tool surface to the
 * authenticated Work ToolPipeline (for Work) or Code Host bridge (for Code).
 */
export function registerComputerUseProxy(pi, options = {}) {
  const registerTool =
    options.registerTool || options.registerWorkTool || ((tool) => pi.registerTool(tool));
  const invoker = createComputerUseToolPipelineInvoker(options);
  const rawDefs = options.toolDefinitions || loadToolDefinitions();
  const toolDefinitions = [...rawDefs];
  if (!toolDefinitions.some((def) => def.name === "launch_app")) {
    toolDefinitions.push(LAUNCH_APP_TOOL_DEFINITION);
  }

  let desktopUsed = false;
  const releaseDesktop = async () => {
    if (!desktopUsed) return;
    desktopUsed = false;
    try {
      await invoker(`release-${randomUUID()}`, "desktop_release", "release", {}, undefined);
    } catch {}
  };

  if (typeof pi.on === "function") {
    pi.on("agent_end", releaseDesktop);
    pi.on("session_shutdown", releaseDesktop);
  }

  for (const def of toolDefinitions) {
    registerTool({
      name: def.name,
      label: def.label || def.name,
      description: def.description || "",
      parameters: def.parameters,
      execute: async (toolCallId, params, signal) => {
        desktopUsed = true;
        const response = await invoker(
          toolCallId || `cu-${randomUUID()}`,
          def.name,
          def.name,
          params || {},
          signal
        );

        if (!response) {
          throw new Error(`Computer Use tool '${def.name}' returned no response.`);
        }

        if (response.success === false) {
          const message =
            response.stderr || response.error || `Computer Use tool '${def.name}' failed.`;
          throw new Error(message);
        }

        if (response.result && typeof response.result === "object") {
          return response.result;
        }

        if (typeof response.stdout === "string" && response.stdout.trim()) {
          try {
            const parsed = JSON.parse(response.stdout);
            if (parsed && typeof parsed === "object") {
              return parsed.result || parsed;
            }
          } catch {}
          return { content: [{ type: "text", text: response.stdout }] };
        }

        return { content: [{ type: "text", text: "Operation completed." }] };
      },
    });
  }

  return {
    releaseDesktop,
  };
}

export default function agentCabinComputerUseProxyExtension(pi, dependencies = {}) {
  return registerComputerUseProxy(pi, dependencies);
}
