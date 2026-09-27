export function createComputerUseToolPipelineInvoker(options = {}) {
  const callPipeline = options.callToolPipeline;
  const callRuntime = options.callDesktopRuntime || (async (toolCallId, toolName, params, signal) => {
    const port = Number(process.env.AGENTCABIN_DESKTOP_BRIDGE_PORT || 0);
    const token = String(process.env.AGENTCABIN_DESKTOP_BRIDGE_TOKEN || "").trim();
    if (!port || !token) return { success: false, stderr: "Code Computer Use runtime is not configured for this session." };
    try {
      const response = await fetch(`http://127.0.0.1:${port}/internal/desktop/call`, {
        method: "POST",
        headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
        body: JSON.stringify({ toolCallId, method: toolName, params: params || {} }),
        signal,
      });
      const payload = await response.json();
      return response.ok ? payload : { success: false, stderr: payload?.error || `Code Computer Use runtime failed (${response.status}).` };
    } catch (error) {
      return { success: false, stderr: error instanceof Error ? error.message : String(error) };
    }
  });
  const waitForApproval = options.waitForWorkInboxResolution;

  return async (toolCallId, toolName, action, params, signal) => {
    if (typeof callPipeline !== "function") {
      return callRuntime(toolCallId, toolName, params, signal);
    }
    let response = await callPipeline(toolCallId, toolName, action, params, signal);
    while (response?.status === "waiting_approval") {
      if (!response.interactionId || typeof waitForApproval !== "function") {
        return { success: false, stderr: "Computer Use entered WaitingApproval without an Inbox resolver." };
      }
      const resolution = await waitForApproval(response.interactionId, signal);
      if (!["approved", "answered"].includes(resolution?.status)) {
        return { success: false, stderr: `Computer Use operation was ${resolution?.status || "rejected"} in Inbox.` };
      }
      response = await callPipeline(toolCallId, toolName, action, params, signal);
    }
    return response;
  };
}
