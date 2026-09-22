import { Type } from "typebox";

const ConnectorCliSchema = Type.Object({
  package_id: Type.String({
    minLength: 1,
    maxLength: 64,
    description: "Installed Connector Package id.",
  }),
  operation: Type.String({
    minLength: 1,
    maxLength: 64,
    pattern: "^[A-Za-z0-9._-]+$",
    description: "An operation declared by the Connector Package cli.json.",
  }),
  args: Type.Optional(
    Type.Array(Type.String({ maxLength: 8192 }), { maxItems: 64 }),
  ),
});

function result(text, details = {}) {
  return { content: [{ type: "text", text }], details };
}

function fail(message, details = {}) {
  return result(message, { ok: false, ...details });
}

function connectorRuntimeConfig() {
  const port = Number(process.env.AGENTCABIN_CODE_CONNECTOR_BRIDGE_PORT || 0);
  const token = String(process.env.AGENTCABIN_CODE_CONNECTOR_BRIDGE_TOKEN || "").trim();
  if (!Number.isInteger(port) || port < 1 || !token) return null;
  return { port, token };
}

async function callConnectorRuntime(toolCallId, params, signal) {
  const config = connectorRuntimeConfig();
  if (!config) {
    return fail("Code Connector Runtime is not configured for this session.", {
      status: "unconfigured",
    });
  }
  try {
    const response = await fetch(
      "http://127.0.0.1:" + config.port + "/internal/code/connector_cli",
      {
        method: "POST",
        headers: {
          authorization: "Bearer " + config.token,
          "content-type": "application/json",
        },
        body: JSON.stringify({
          toolCallId: toolCallId || undefined,
          packageId: String(params?.package_id || "").trim(),
          operation: String(params?.operation || "").trim(),
          args: Array.isArray(params?.args) ? params.args.map(String) : [],
        }),
        signal,
      },
    );
    let payload = {};
    try {
      payload = await response.json();
    } catch {
      payload = { error: "Connector Runtime returned invalid JSON (" + response.status + ")" };
    }
    if (!response.ok) {
      return fail(
        payload?.error || "Connector Runtime request failed (" + response.status + ")",
        payload,
      );
    }
    return payload;
  } catch (error) {
    return fail(error instanceof Error ? error.message : String(error), {
      status: "failed",
    });
  }
}

export function registerConnectorTools(pi, options = {}) {
  const registerTool = options.registerTool || ((tool) => pi.registerTool(tool));
  const callRuntime = options.callConnectorRuntime || callConnectorRuntime;

  registerTool({
    name: "work_run_connector_cli",
    label: "work_run_connector_cli",
    description:
      "Run one declared operation from a trusted and enabled Connector Package. The AgentCabin Host selects the managed executable, validates argv, keeps credentials on the Host, and redacts credential-shaped output before returning it.",
    parameters: ConnectorCliSchema,
    async execute(toolCallId, params, signal) {
      const packageId = String(params?.package_id || "").trim();
      const operation = String(params?.operation || "").trim();
      if (!packageId || !operation) {
        return fail("package_id and operation are required.");
      }
      const response = await callRuntime(toolCallId, params, signal);
      const exitCode = response?.exitCode ?? response?.exit_code ?? -1;
      const success = response?.success === true && response?.status === "success";
      if (!success) {
        return fail(
          "Connector CLI '" +
            packageId +
            "/" +
            operation +
            "' failed (exit " +
            exitCode +
            "): " +
            (response?.stderr || response?.error || response?.stdout || "unknown error"),
          {
            status: response?.status,
            exit_code: exitCode,
            package_id: packageId,
            operation,
          },
        );
      }
      return result(
        "Connector CLI '" +
          packageId +
          "/" +
          operation +
          "' completed (exit " +
          exitCode +
          ").\n\nstdout:\n" +
          (response?.stdout || "(empty)"),
        {
          ok: true,
          package_id: packageId,
          operation,
          exit_code: exitCode,
          stdout: response?.stdout || "",
          stderr: response?.stderr || "",
        },
      );
    },
  });
}

export default function agentCabinConnectorExtension(pi, dependencies = {}) {
  registerConnectorTools(pi, dependencies);
}
