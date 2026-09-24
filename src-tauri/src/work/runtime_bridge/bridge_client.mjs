/**
 * Runtime-neutral Work bridge client.
 *
 * The client owns only authenticated transport and Inbox polling. It does not
 * know how a runtime registers tools or how Work persists authoritative state;
 * Pi, DSH, and test registrars can all call the same bridge contract.
 */

const DEFAULT_INBOX_TIMEOUT_MS = 600000;
const DEFAULT_POLL_INTERVAL_MS = 750;

function defaultSleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function defaultFetch(url, init) {
  return fetch(url, init);
}

export function buildWorkToolPipelinePayload(toolCallId, toolName, action, args) {
  const expectedOutputs = Array.isArray(args?.expected_outputs)
    ? args.expected_outputs.map(String)
    : [];
  return {
    toolCallId: toolCallId ? String(toolCallId) : `call-${Date.now()}`,
    toolName: String(toolName),
    action: String(action || ""),
    arguments: args || {},
    expectedOutputs,
  };
}

export function createWorkBridgeClient({
  env = process.env,
  fetchImpl = defaultFetch,
  sleep = defaultSleep,
  now = () => Date.now(),
} = {}) {
  function config() {
    const port = Number(env.AGENTCABIN_WORK_BRIDGE_PORT || 0);
    const token = String(env.AGENTCABIN_WORK_BRIDGE_TOKEN || "");
    if (!Number.isInteger(port) || port <= 0 || !token) {
      throw new Error("Authenticated Work bridge is not configured for this process.");
    }
    return { baseUrl: `http://127.0.0.1:${port}`, token };
  }

  async function request(endpoint, body, signal) {
    const { baseUrl, token } = config();
    const response = await fetchImpl(`${baseUrl}${endpoint}`, {
      method: "POST",
      signal,
      headers: {
        "Content-Type": "application/json",
        "Authorization": `Bearer ${token}`,
      },
      body: JSON.stringify(body),
    });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) {
      throw new Error(data.error || `Work bridge request failed (${response.status}).`);
    }
    return data;
  }

  async function waitForInboxResolution(itemId, signal, timeoutMs = DEFAULT_INBOX_TIMEOUT_MS) {
    const { baseUrl, token } = config();
    const startTime = now();
    for (;;) {
      if (signal?.aborted) {
        throw new Error("Work execution was cancelled while waiting for Inbox approval.");
      }
      if (now() - startTime > timeoutMs) {
        throw new Error(`等待 Inbox 审批超时（已等待超过 ${Math.round(timeoutMs / 1000)} 秒）。`);
      }
      const response = await fetchImpl(
        `${baseUrl}/internal/work/inbox_status/${encodeURIComponent(itemId)}`,
        { signal, headers: { Authorization: `Bearer ${token}` } },
      );
      const data = await response.json().catch(() => ({}));
      if (!response.ok) {
        throw new Error(data.error || `Work Inbox status request failed (${response.status}).`);
      }
      if (data.status !== "pending") return data;
      await sleep(DEFAULT_POLL_INTERVAL_MS);
    }
  }

  return {
    config,
    request,
    waitForInboxResolution,
  };
}

export async function callWorkToolPipeline(client, toolCallId, toolName, action, args, signal) {
  return client.request(
    "/internal/work/tool_pipeline",
    buildWorkToolPipelinePayload(toolCallId, toolName, action, args),
    signal,
  );
}
