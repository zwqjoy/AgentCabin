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
  let childTokenExchanged = false;
  let childTokenExchangePromise = null;

  function config() {
    const port = Number(env.AGENTCABIN_WORK_BRIDGE_PORT || 0);
    const token = String(env.AGENTCABIN_WORK_BRIDGE_TOKEN || "");
    if (!Number.isInteger(port) || port <= 0 || !token) {
      throw new Error("Authenticated Work bridge is not configured for this process.");
    }
    return { baseUrl: `http://127.0.0.1:${port}`, token };
  }

  async function ensureChildToken(signal) {
    if (env.PI_SUBAGENT_CHILD !== "1" || childTokenExchanged) return;
    if (childTokenExchangePromise) return await childTokenExchangePromise;

    childTokenExchangePromise = (async () => {
      const runId = String(env.PI_SUBAGENT_RUN_ID || env.PI_SUBAGENT_CHILD_ID || "").trim();
      const role = String(env.PI_SUBAGENT_CHILD_AGENT || env.PI_SUBAGENT_ROLE || "").trim();
      const rawIndex = env.PI_SUBAGENT_CHILD_INDEX;
      const childIndex = rawIndex !== undefined && !Number.isNaN(Number(rawIndex))
        ? Number(rawIndex)
        : undefined;

      if (!runId || !role) {
        // Standalone fixtures without child metadata do not need an exchange.
        childTokenExchanged = true;
        return;
      }

      const { baseUrl, token } = config();
      const maxRetries = 10;
      for (let attempt = 1; attempt <= maxRetries; attempt++) {
        if (signal?.aborted) throw new Error("Child bridge token exchange was aborted.");
        try {
          const response = await fetchImpl(`${baseUrl}/internal/work/subagents/token`, {
            method: "POST",
            signal,
            headers: {
              "Content-Type": "application/json",
              "Authorization": `Bearer ${token}`,
            },
            body: JSON.stringify({
              agentId: runId,
              childId: env.PI_SUBAGENT_CHILD_ID || undefined,
              providerRunId: env.PI_SUBAGENT_RUN_ID || undefined,
              role,
              childIndex,
            }),
          });
          if (response.ok) {
            const data = await response.json();
            if (data?.token) {
              env.AGENTCABIN_WORK_BRIDGE_TOKEN = data.token;
              childTokenExchanged = true;
              return;
            }
          } else {
            const errPayload = await response.json().catch(() => ({}));
            console.warn(
              `[work/bridge] Child token exchange attempt ${attempt} failed (HTTP ${response.status}):`,
              errPayload,
            );
          }
        } catch (error) {
          console.warn(`[work/bridge] Child token exchange attempt ${attempt} network error:`, error);
        }
        await sleep(150);
      }
      throw new Error(`Failed to exchange child bridge token for subagent '${runId}' (${role}).`);
    })();

    try {
      await childTokenExchangePromise;
    } finally {
      childTokenExchangePromise = null;
    }
  }

  async function request(endpoint, body, signal) {
    await ensureChildToken(signal);
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
    await ensureChildToken(signal);
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
    ensureChildToken,
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
