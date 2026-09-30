const WORK_BROWSER_PROXY_ENV = "AGENTCABIN_WORK_BROWSER_HOST_PROXY";
const WORK_BRIDGE_PORT_ENV = "AGENTCABIN_WORK_BRIDGE_PORT";
const WORK_BRIDGE_TOKEN_ENV = "AGENTCABIN_WORK_BRIDGE_TOKEN";
const FORWARDED_BROWSER_ENV = new Set([
  "AGENT_BROWSER_NAMESPACE",
  "AGENT_BROWSER_DEFAULT_TIMEOUT",
  "AGENT_BROWSER_IDLE_TIMEOUT_MS",
  "AGENT_BROWSER_CONFIG",
  "AGENT_BROWSER_USER_AGENT",
  "AGENT_BROWSER_RESTORE",
  "AGENT_BROWSER_AUTOSAVE_INTERVAL_MS",
]);

export function isWorkBrowserHostProxyEnabled(env = process.env) {
  return env[WORK_BROWSER_PROXY_ENV] === "1";
}

export async function runWorkBrowserHostCommand(options, dependencies = {}) {
  const env = options.env ?? process.env;
  const port = Number(env[WORK_BRIDGE_PORT_ENV]);
  const token = env[WORK_BRIDGE_TOKEN_ENV];
  if (!Number.isInteger(port) || port < 1 || port > 65535 || !token) {
    return failedResult(new Error("Work browser host bridge is unavailable"));
  }

  const fetchImpl = dependencies.fetch ?? globalThis.fetch;
  const wait =
    dependencies.wait ??
    ((ms, signal) =>
      new Promise((resolve, reject) => {
        const onAbort = () => {
          clearTimeout(timer);
          signal?.removeEventListener("abort", onAbort);
          reject(signal.reason ?? new Error("Browser host command was aborted"));
        };
        const timer = setTimeout(() => {
          signal?.removeEventListener("abort", onAbort);
          resolve();
        }, ms);
        if (signal?.aborted) onAbort();
        else signal?.addEventListener("abort", onAbort, { once: true });
      }));
  const controller = new AbortController();
  const timeoutMs = options.timeoutMs ?? 35_000;
  const timedOut = { value: false };
  let timeout;
  const startTimeout = (durationMs) => {
    clearTimeout(timeout);
    timeout =
      durationMs > 0
        ? setTimeout(() => {
            timedOut.value = true;
            controller.abort(new Error("Browser host command timed out"));
          }, durationMs)
        : undefined;
  };
  startTimeout(timeoutMs);
  const onAbort = () =>
    controller.abort(options.signal?.reason ?? new Error("Browser host command was aborted"));
  options.signal?.addEventListener("abort", onAbort, { once: true });
  if (options.signal?.aborted) onAbort();

  const base = `http://127.0.0.1:${port}/internal/work`;
  const headers = { authorization: `Bearer ${token}`, "content-type": "application/json" };
  const browserEnv = Object.fromEntries(
    Object.entries(env).filter(([key]) => FORWARDED_BROWSER_ENV.has(key)),
  );
  const body = JSON.stringify({
    args: options.args,
    cwd: options.cwd,
    stdin: options.stdin ?? "",
    env: browserEnv,
  });
  const request = async (url, init = {}) => {
    const response = await fetchImpl(url, { ...init, headers, signal: controller.signal });
    const result = await response.json();
    if (!response.ok)
      throw new Error(result.error || `Browser host bridge HTTP ${response.status}`);
    return result;
  };

  try {
    while (true) {
      const result = await request(`${base}/browser_host_command`, { method: "POST", body });
      if (result.status === "waiting_approval") {
        const id = encodeURIComponent(result.inboxItemId);
        const approvalTimeoutMs = Math.min(options.approvalTimeoutMs ?? 300_000, 300_000);
        const deadline = Date.now() + approvalTimeoutMs;
        startTimeout(approvalTimeoutMs);
        while (Date.now() < deadline) {
          await wait(500, controller.signal);
          const item = await request(`${base}/inbox_status/${id}`);
          if (item.status === "approved") break;
          if (item.status !== "pending") throw new Error(`Browser host execution ${item.status}`);
        }
        if (Date.now() >= deadline) throw new Error("Browser host approval timed out");
        startTimeout(timeoutMs);
        continue;
      }
      return {
        aborted: false,
        agentBrowserStarted: true,
        exitCode: result.exitCode ?? 1,
        stderr: result.stderr ?? "",
        stdout: result.stdout ?? "",
        timedOut: false,
      };
    }
  } catch (error) {
    if (controller.signal.aborted) {
      return {
        aborted: !timedOut.value,
        agentBrowserStarted: false,
        exitCode: timedOut.value ? 124 : 1,
        stderr: "",
        stdout: "",
        timedOut: timedOut.value,
        ...(timedOut.value ? { timeoutMs } : {}),
      };
    }
    return failedResult(error);
  } finally {
    if (timeout) clearTimeout(timeout);
    options.signal?.removeEventListener("abort", onAbort);
  }
}

function failedResult(error) {
  return {
    aborted: false,
    agentBrowserStarted: false,
    exitCode: 1,
    spawnError: error instanceof Error ? error : new Error(String(error)),
    stderr: "",
    stdout: "",
    timedOut: false,
  };
}
