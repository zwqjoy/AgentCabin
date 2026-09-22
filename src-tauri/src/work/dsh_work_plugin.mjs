// AgentCabin Work native Cordis plugin for DSH.
//
// This is deliberately a thin registration layer. The bridge implementation
// lives in dsh_work_mcp_adapter.mjs so the legacy MCP transport and the native
// DSH transport cannot drift in policy, approval, or path handling.

import {
  browserTools,
  callTool,
  desktopTools,
  tools,
} from "./dsh_work_mcp_adapter.mjs";

export const name = "agentcabin-work-plugin";
export const inject = ["tools"];

const outputSchema = {
  type: "object",
  additionalProperties: false,
  properties: {
    text: {
      type: "string",
    },
    isError: {
      type: "boolean",
    },
  },
  required: ["text", "isError"],
};

function enabledTools() {
  const result = [...tools];
  if (process.env.AGENTCABIN_WORK_BROWSER_ENABLED === "1") {
    result.push(...browserTools.filter((tool) => !tool.name.startsWith("browser_")));
  }
  if (process.env.AGENTCABIN_WORK_BROWSER_USE_ENABLED === "1") {
    result.push(...browserTools.filter((tool) => tool.name.startsWith("browser_")));
  }
  if (process.env.AGENTCABIN_WORK_DESKTOP_USE_ENABLED === "1") {
    result.push(...desktopTools);
  }
  return result;
}

function resultText(result) {
  return (result?.content || [])
    .filter((block) => block?.type === "text")
    .map((block) => String(block.text ?? ""))
    .join("\n") || "(empty)";
}

function nativeDefinition(tool) {
  return {
    name: tool.name,
    description: tool.description,
    parameters: tool.inputSchema,
    output: {
      schema: outputSchema,
      render: (_args, value) => [{ type: "text", text: value.text }],
    },
    async execute(args, exec) {
      const result = await callTool(
        tool.name,
        args || {},
        exec?.toolCallId,
        exec?.signal,
      );
      const text = resultText(result);
      if (result?.isError === true) {
        throw new Error(text);
      }
      return { text, isError: false };
    },
  };
}

export function apply(ctx) {
  if (process.env.AGENTCABIN_WORK_RUNTIME !== "dsh") {
    throw new Error("AgentCabin Work plugin requires the DSH Work runtime boundary.");
  }
  if (!process.env.AGENTCABIN_WORK_BRIDGE_PORT || !process.env.AGENTCABIN_WORK_BRIDGE_TOKEN) {
    throw new Error("AgentCabin Work plugin requires an authenticated Work bridge.");
  }
  for (const tool of enabledTools()) {
    ctx.tools.register(nativeDefinition(tool));
  }
}
