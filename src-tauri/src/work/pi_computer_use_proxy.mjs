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
  const toolDefinitions = options.toolDefinitions || loadToolDefinitions();

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
