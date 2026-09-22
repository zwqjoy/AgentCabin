import { Type } from "typebox";

/**
 * AgentCabin shared Browser Operator Adapter.
 *
 * Work passes registerWorkTool/callToolPipeline, so its policy checks, Inbox
 * approvals, activeTools filtering, and runtime ledger remain authoritative.
 * Code loads the same adapter as a normal Pi extension and uses the separate
 * authenticated Browser Runtime bridge. Both modes expose the same protocol.
 */

const NavigateSchema = Type.Object({
  url: Type.String({ description: "The HTTP/HTTPS URL to navigate to." }),
  wait_ms: Type.Optional(Type.Number({ description: "Optional extra wait time after DOM load (ms)." })),
});

const SnapshotSchema = Type.Object({});

const ScreenshotSchema = Type.Object({
  filename: Type.Optional(Type.String({ description: "Optional filename or relative path inside output/ (e.g. 'page.png'). If omitted, image base64 data is returned directly." })),
  full_page: Type.Optional(Type.Boolean({ description: "Whether to capture the entire scrollable page (default false)." })),
});

const WaitSchema = Type.Object({
  ms: Type.Optional(Type.Number({ description: "Milliseconds to wait." })),
  load_state: Type.Optional(Type.Union([
    Type.Literal("domcontentloaded"),
    Type.Literal("load"),
    Type.Literal("networkidle"),
  ], { description: "Page lifecycle load state to wait for." })),
});

const TabsSchema = Type.Object({
  action: Type.Union([
    Type.Literal("list"),
    Type.Literal("new"),
    Type.Literal("switch"),
    Type.Literal("close"),
  ], { description: "Tab action: list all tabs, open a new tab, switch active tab, or close a tab." }),
  index: Type.Optional(Type.Number({ description: "Tab index for switch or close." })),
  url: Type.Optional(Type.String({ description: "Target URL when opening a new tab." })),
});

const CloseSchema = Type.Object({});

const ClickSchema = Type.Object({
  ref: Type.Optional(Type.String({ description: "Semantic ref from browser_snapshot (e.g. 'e4'). Strongly recommended over CSS selector." })),
  selector: Type.Optional(Type.String({ description: "Optional CSS selector or visible text as fallback." })),
  button: Type.Optional(Type.Union([
    Type.Literal("left"),
    Type.Literal("right"),
    Type.Literal("middle"),
  ], { description: "Mouse button to click with (default 'left')." })),
  double_click: Type.Optional(Type.Boolean({ description: "Whether to perform a double click (default false)." })),
});

const TypeSchema = Type.Object({
  ref: Type.Optional(Type.String({ description: "Semantic ref from browser_snapshot (e.g. 'e2'). Strongly recommended." })),
  text: Type.String({ description: "Text content to type into the input field." }),
  selector: Type.Optional(Type.String({ description: "Optional CSS selector as fallback." })),
  clear: Type.Optional(Type.Boolean({ description: "Whether to clear existing text before typing (default true)." })),
  press_enter: Type.Optional(Type.Boolean({ description: "Whether to press Enter key after typing (default false)." })),
});

const SelectSchema = Type.Object({
  ref: Type.Optional(Type.String({ description: "Semantic ref of the <select> element from browser_snapshot." })),
  value: Type.String({ description: "Value or visible label of the option to select." }),
  selector: Type.Optional(Type.String({ description: "Optional CSS selector as fallback." })),
});

const ScrollSchema = Type.Object({
  direction: Type.Optional(Type.Union([
    Type.Literal("up"),
    Type.Literal("down"),
    Type.Literal("top"),
    Type.Literal("bottom"),
  ], { description: "Direction to scroll (default 'down')." })),
  amount: Type.Optional(Type.Number({ description: "Pixels to scroll when scrolling up/down (default 500)." })),
  ref: Type.Optional(Type.String({ description: "Optional semantic ref of a specific scrollable container." })),
});

function result(text, details = {}) {
  return { content: [{ type: "text", text }], details };
}

function fail(message, details = {}) {
  return result(message, { ok: false, ...details });
}

export function registerBrowserOperatorTools(pi, options = {}) {
  const registerTool = options.registerTool || options.registerWorkTool || ((t) => pi.registerTool(t));
  const callPipeline = options.callToolPipeline;
  const waitForApproval = options.waitForWorkInboxResolution;
  const callRuntime = options.callBrowserRuntime || callBrowserRuntime;

  if (typeof callPipeline !== "function" && typeof callRuntime !== "function") {
    return;
  }

  async function callBrowserRuntime(toolCallId, toolName, params, signal) {
    const port = Number(process.env.AGENTCABIN_BROWSER_BRIDGE_PORT || 0);
    const token = String(process.env.AGENTCABIN_BROWSER_BRIDGE_TOKEN || "").trim();
    if (!port || !token) {
      return fail("Shared Browser Runtime is not configured for this session.", { status: "unconfigured" });
    }
    try {
      const response = await fetch(`http://127.0.0.1:${port}/internal/browser/call`, {
        method: "POST",
        headers: {
          authorization: `Bearer ${token}`,
          "content-type": "application/json",
        },
        body: JSON.stringify({ toolCallId, method: toolName, params: params || {} }),
        signal,
      });
      const payload = await response.json();
      if (!response.ok) {
        return fail(payload?.error || `Browser Runtime request failed (${response.status})`, payload);
      }
      return payload;
    } catch (error) {
      return fail(error instanceof Error ? error.message : String(error), { status: "failed" });
    }
  }

  async function callOperationWithApproval(toolCallId, toolName, action, params, signal) {
    if (typeof callPipeline !== "function") {
      return callRuntime(toolCallId, toolName, params, signal);
    }
    let res = await callPipeline(toolCallId, toolName, action, params || {}, signal);
    while (res.status === "waiting_approval") {
      if (!res.interactionId || typeof waitForApproval !== "function") {
        return fail("Browser operation entered WaitingApproval without an approval resolver.", {
          status: res.status,
        });
      }
      const resolution = await waitForApproval(res.interactionId, signal);
      const approved = resolution.status === "approved" || resolution.status === "answered";
      if (!approved) {
        return fail(`Browser operation was ${resolution.status || "rejected"} in Inbox.`, {
          confirmed: false,
          interaction_id: res.interactionId,
        });
      }
      res = await callPipeline(toolCallId, toolName, action, params || {}, signal);
    }
    return res;
  }

  // 1. Navigate
  registerTool({
    name: "browser_navigate",
    label: "browser_navigate",
    description: "Navigate the active browser page to a public HTTP/HTTPS URL. For generated HTML, use a file:// URL under the current WorkRun output/ directory; other local files, localhost, and private LAN networks remain blocked for SSRF protection.",
    parameters: NavigateSchema,
    async execute(toolCallId, params, signal) {
      const res = await callOperationWithApproval(toolCallId, "browser_navigate", "navigate", params, signal);
      if (!res.success) return fail(res.stderr || res.error || "Navigation failed", res);
      let parsed = {};
      try { parsed = JSON.parse(res.stdout || "{}"); } catch {}
      return result(`Navigated to ${parsed.url || params?.url}\nTitle: ${parsed.title || "(no title)"}`, { ok: true, ...parsed });
    },
  });

  // 2. Snapshot
  registerTool({
    name: "browser_snapshot",
    label: "browser_snapshot",
    description: "Capture the current page's semantic accessibility tree with stable [ref=eX] identifiers. Use these refs for element click/type/select operations.",
    parameters: SnapshotSchema,
    async execute(toolCallId, params, signal) {
      const res = await callOperationWithApproval(toolCallId, "browser_snapshot", "snapshot", params, signal);
      if (!res.success) return fail(res.stderr || res.error || "Snapshot failed", res);
      let parsed = {};
      try { parsed = JSON.parse(res.stdout || "{}"); } catch {}
      return result(`Page: ${parsed.title || "(untitled)"} (${parsed.url})\n\n${parsed.tree || "(empty)"}`, { ok: true, ...parsed });
    },
  });

  // 3. Screenshot
  registerTool({
    name: "browser_take_screenshot",
    label: "browser_take_screenshot",
    description: "Take a screenshot of the current page as visual verification. Can return base64 or save to output/.",
    parameters: ScreenshotSchema,
    async execute(toolCallId, params, signal) {
      const res = await callOperationWithApproval(toolCallId, "browser_take_screenshot", "screenshot", params, signal);
      if (!res.success) return fail(res.stderr || res.error || "Screenshot failed", res);
      let parsed = {};
      try { parsed = JSON.parse(res.stdout || "{}"); } catch {}
      if (parsed.path) {
        return result(`Screenshot saved to ${parsed.path}`, { ok: true, path: parsed.path });
      }
      return result(`Screenshot captured (${parsed.base64 ? Math.round(parsed.base64.length * 0.75) : 0} bytes)`, {
        ok: true,
        mimeType: parsed.mimeType || "image/png",
        base64: parsed.base64,
      });
    },
  });

  // 4. Wait
  registerTool({
    name: "browser_wait_for",
    label: "browser_wait_for",
    description: "Wait for a specified duration in ms, or wait until the page reaches a specific lifecycle state (domcontentloaded, load, networkidle).",
    parameters: WaitSchema,
    async execute(toolCallId, params, signal) {
      const res = await callOperationWithApproval(toolCallId, "browser_wait_for", "wait", params, signal);
      if (!res.success) return fail(res.stderr || res.error || "Wait failed", res);
      let parsed = {};
      try { parsed = JSON.parse(res.stdout || "{}"); } catch {}
      return result(`Wait completed on ${parsed.url || "page"}`, { ok: true, ...parsed });
    },
  });

  // 5. Tabs
  registerTool({
    name: "browser_tabs",
    label: "browser_tabs",
    description: "Manage browser tabs in the current WorkRun session (list, new, switch, close).",
    parameters: TabsSchema,
    async execute(toolCallId, params, signal) {
      const action = params?.action || "list";
      const res = await callOperationWithApproval(toolCallId, "browser_tabs", action, params, signal);
      if (!res.success) return fail(res.stderr || res.error || "Tab operation failed", res);
      let parsed = {};
      try { parsed = JSON.parse(res.stdout || "{}"); } catch {}
      if (action === "list") {
        const tabLines = (parsed.tabs || []).map((t) => `[Tab ${t.index}] ${t.active ? "*ACTIVE* " : ""}${t.title || "(no title)"} - ${t.url}`);
        return result(`${tabLines.length} open tab(s):\n${tabLines.join("\n")}`, { ok: true, tabs: parsed.tabs });
      }
      return result(`Tab action '${action}' completed.`, { ok: true, ...parsed });
    },
  });

  // 6. Close
  registerTool({
    name: "browser_close",
    label: "browser_close",
    description: "Explicitly close the browser context for the current WorkRun session to free memory and resources.",
    parameters: CloseSchema,
    async execute(toolCallId, params, signal) {
      const res = await callOperationWithApproval(toolCallId, "browser_close", "close", params, signal);
      if (!res.success) return fail(res.stderr || res.error || "Browser close failed", res);
      return result("Browser context closed successfully.", { ok: true });
    },
  });

  // 7. Click (Interactive)
  registerTool({
    name: "browser_click",
    label: "browser_click",
    description: "Click an interactive element on the page. Pass 'ref' (from browser_snapshot) for accurate semantic targeting, or 'selector' as fallback.",
    parameters: ClickSchema,
    async execute(toolCallId, params, signal) {
      const res = await callOperationWithApproval(toolCallId, "browser_click", "click", params, signal);
      if (!res.success) return fail(res.stderr || res.error || "Click failed", res);
      return result(`Clicked element ${params?.ref ? `[ref=${params.ref}]` : params?.selector}`, { ok: true, ...res });
    },
  });

  // 8. Type (Interactive)
  registerTool({
    name: "browser_type",
    label: "browser_type",
    description: "Type text into an input field or textarea. Pass 'ref' (from browser_snapshot) for accurate targeting.",
    parameters: TypeSchema,
    async execute(toolCallId, params, signal) {
      const res = await callOperationWithApproval(toolCallId, "browser_type", "type", params, signal);
      if (!res.success) return fail(res.stderr || res.error || "Type failed", res);
      return result(`Typed into ${params?.ref ? `[ref=${params.ref}]` : params?.selector}`, { ok: true, ...res });
    },
  });

  // 9. Select (Interactive)
  registerTool({
    name: "browser_select_option",
    label: "browser_select_option",
    description: "Select an option from a <select> dropdown by option value or visible text.",
    parameters: SelectSchema,
    async execute(toolCallId, params, signal) {
      const res = await callOperationWithApproval(toolCallId, "browser_select_option", "select", params, signal);
      if (!res.success) return fail(res.stderr || res.error || "Select failed", res);
      return result(`Selected '${params?.value}' in ${params?.ref ? `[ref=${params.ref}]` : params?.selector}`, { ok: true, ...res });
    },
  });

  // 10. Scroll (Interactive)
  registerTool({
    name: "browser_scroll",
    label: "browser_scroll",
    description: "Scroll the page or a scrollable element container in a given direction.",
    parameters: ScrollSchema,
    async execute(toolCallId, params, signal) {
      const res = await callOperationWithApproval(toolCallId, "browser_scroll", "scroll", params, signal);
      if (!res.success) return fail(res.stderr || res.error || "Scroll failed", res);
      return result(`Scrolled ${params?.direction || "down"}`, { ok: true, ...res });
    },
  });
}

export function createBrowserInvoker(options = {}) {
  const callPipeline = options.callToolPipeline;
  const waitForApproval = options.waitForWorkInboxResolution;
  const port = Number(process.env.AGENTCABIN_BROWSER_BRIDGE_PORT || 0);
  const token = String(process.env.AGENTCABIN_BROWSER_BRIDGE_TOKEN || "").trim();

  if (typeof callPipeline !== "function" && (!port || !token)) {
    return undefined;
  }

  const assertBrowserSuccess = (response, operation) => {
    if (response?.ok === false || response?.success === false || response?.details?.ok === false || response?.isError) {
      const message = response.error
        || response.stderr
        || response.message
        || response.details?.error
        || response.content?.find?.((block) => block?.type === "text")?.text
        || `${operation} failed.`;
      throw new Error(`${operation} failed: ${message}`);
    }
    return response;
  };

  return async function invokeBrowser(toolCallId, toolName, params = {}, signal) {
    let action = "execute";
    if (toolName === "browser_tabs") action = params?.action || "list";
    else if (toolName === "browser_snapshot") action = "snapshot";
    else if (toolName === "browser_take_screenshot") action = "screenshot";
    else if (toolName === "browser_click") action = "click";
    else if (toolName === "browser_type") action = "type";
    else if (toolName === "browser_press_key") action = "press_key";
    else if (toolName === "browser_scroll") action = "scroll";
    else if (toolName === "browser_navigate") action = "navigate";
    else if (toolName === "browser_wait_for") action = "wait";
    else if (toolName === "browser_close") action = "close";

    let res;
    if (typeof callPipeline === "function") {
      res = await callPipeline(toolCallId, toolName, action, params, signal);
      while (res?.status === "waiting_approval") {
        if (!res.interactionId || typeof waitForApproval !== "function") {
          throw new Error("Browser operation entered WaitingApproval without an approval resolver.");
        }
        const resolution = await waitForApproval(res.interactionId, signal);
        const approved = resolution?.status === "approved" || resolution?.status === "answered";
        if (!approved) {
          throw new Error(`Browser operation was ${resolution?.status || "rejected"} in Inbox.`);
        }
        res = await callPipeline(toolCallId, toolName, action, params, signal);
      }
    } else {
      const response = await fetch(`http://127.0.0.1:${port}/internal/browser/call`, {
        method: "POST",
        headers: {
          authorization: `Bearer ${token}`,
          "content-type": "application/json",
        },
        body: JSON.stringify({ toolCallId, method: toolName, params }),
        signal,
      });
      res = await response.json();
      if (!response.ok) {
        throw new Error(res?.error || `Browser Runtime request failed (${response.status})`);
      }
    }

    if (res && res.stdout && typeof res.stdout === "string") {
      let parsed;
      try {
        parsed = JSON.parse(res.stdout);
      } catch {
        // stdout may be human-readable for a successful legacy operation. Keep
        // that compatibility, but never hide a structured failure envelope.
        return assertBrowserSuccess(res, toolName);
      }
      return assertBrowserSuccess(parsed, toolName);
    }
    return assertBrowserSuccess(res, toolName);
  };
}

export default function agentCabinBrowserOperatorExtension(pi, dependencies = {}) {
  registerBrowserOperatorTools(pi, dependencies);
}
