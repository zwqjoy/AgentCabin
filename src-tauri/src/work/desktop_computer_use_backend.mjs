/**
 * AgentCabin Computer Use V3 Desktop Native Backend.
 *
 * Encapsulates macOS AX / Native Bridge calls and Electron debugging detection.
 */

import {
  BACKEND_MACOS_AX,
  BACKEND_CDP,
  ROOT_KIND_DESKTOP_WINDOW,
  ROOT_KIND_ELECTRON_PAGE,
  createUiRoot,
} from "./computer_use_v3_models.mjs";

export function decodeLegacyResponse(response, fallback = "Operation failed.") {
  if (response && typeof response === "object") {
    if (response.isError) {
      throw new Error(response.content?.find((block) => block?.type === "text")?.text || fallback);
    }
    if (response.structuredContent || response.windows || response.observation || response.elements) {
      return response;
    }
    if (response.stdout && typeof response.stdout === "object") {
      if (response.stdout.isError) {
        throw new Error(response.stdout.content?.find((block) => block?.type === "text")?.text || fallback);
      }
      return response.stdout;
    }
  }
  let parsed;
  try {
    parsed = JSON.parse(String(response?.stdout ?? (typeof response === "string" ? response : "")));
  } catch {
    if (typeof response === "object" && response !== null && !response.stdout && !response.stderr) {
      return response;
    }
    throw new Error(response?.stderr || "AgentCabin desktop runtime returned invalid JSON.");
  }
  if (parsed?.isError) {
    throw new Error(parsed.content?.find((block) => block?.type === "text")?.text || fallback);
  }
  return parsed;
}

/**
 * Real Electron CDP probe:
 * Inspects process command line for --remote-debugging-port or probes port hints.
 * Performs real HTTP checks to /json/version and /json/list. If reachable, returns
 * browser and page CDP metadata. If unreachable or not an open CDP port, returns null.
 * If unreachable or not an open CDP port, returns null (fallback to native AX).
 */
export async function probeElectronCdp(pid, portHint, timeoutMs = 200, fetcher = fetch) {
  let port = portHint ? Number(portHint) : undefined;

  if (!port && pid) {
    try {
      const { execFileSync } = await import("node:child_process");
      const out = execFileSync("ps", ["-o", "command=", "-p", String(pid)], {
        encoding: "utf8",
        timeout: 500,
      });
      const match = out.match(/--remote-debugging-port=(\d+)/);
      if (match) {
        port = Number(match[1]);
      }
    } catch {
      // Process inspect unavailable or forbidden
    }
  }

  if (!port || port <= 0 || port > 65535) {
    return null;
  }

  try {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    const res = await fetcher(`http://127.0.0.1:${port}/json/version`, {
      signal: controller.signal,
    });
    clearTimeout(timer);
    if (res?.ok) {
      const data = await res.json();
      if (data?.webSocketDebuggerUrl || data?.Browser) {
        let targets = [];
        // Keep injected test fetchers backward-compatible; the real fetch path
        // discovers page target IDs needed by connectOverCDP.
        if (fetcher === fetch || typeof fetcher?.discoverTargets === "function") {
          try {
            const targetResponse = await fetcher(`http://127.0.0.1:${port}/json/list`, {
              signal: controller.signal,
            });
            if (targetResponse?.ok) {
              const targetData = await targetResponse.json();
              targets = Array.isArray(targetData)
                ? targetData
                  .filter((target) => target?.type === "page" && target?.webSocketDebuggerUrl)
                  .map((target) => ({
                    targetId: target.id ? String(target.id) : undefined,
                    title: target.title || "",
                    url: target.url || "",
                    webSocketDebuggerUrl: target.webSocketDebuggerUrl,
                  }))
                : [];
            }
          } catch {
            // /json/version is sufficient to report a candidate; target discovery
            // can fail transiently and the native AX root remains available.
          }
        }
        return {
          port,
          webSocketDebuggerUrl: data.webSocketDebuggerUrl,
          browser: data.Browser,
          targets,
        };
      }
    }
  } catch {
    // Port probe failed / connection refused
  }
  return null;
}

export class DesktopComputerUseBackend {
  constructor(invokeBackend, options = {}) {
    if (typeof invokeBackend !== "function") {
      throw new TypeError("DesktopComputerUseBackend requires an invokeBackend function.");
    }
    this.invokeBackend = invokeBackend;
    this.options = options;
  }

  async invoke(toolCallId, toolName, action, params, signal) {
    const raw = await this.invokeBackend(toolCallId, toolName, action, params || {}, signal);
    return decodeLegacyResponse(raw, `${toolName} failed.`);
  }

  async listRoots(toolCallId = "roots:list", params = {}, signal) {
    let callId = toolCallId;
    let actualParams = params;
    if (typeof toolCallId === "object" && toolCallId !== null) {
      actualParams = toolCallId;
      callId = "roots:list";
    }
    const parsed = await this.invoke(callId || "roots:list", "desktop_list_apps", "list_apps", actualParams, signal);
    const windows = Array.isArray(parsed?.structuredContent?.windows)
      ? parsed.structuredContent.windows
      : (Array.isArray(parsed?.windows) ? parsed.windows : []);
    const roots = [];

    for (const win of windows) {
      const pid = Number(win.pid || 0);
      const windowId = Number(win.window_id || win.windowId || 0);
      const rootRef = win.root_ref || win.rootRef;
      const appName = win.appName || win.app_name || win.owner || win.app || "";
      const title = win.title || "";

      // Real Electron probing: unconditionally probe PID for --remote-debugging-port or check port hints
      let electronCdp = null;
      if (pid || win.debuggingPort || win.cdpPort) {
        electronCdp = await probeElectronCdp(
          pid,
          win.debuggingPort || win.cdpPort,
          this.options.probeTimeoutMs || 200,
          this.options.fetcher || fetch,
        );
      }

      if (electronCdp && this.options.enableExternalCdp !== false) {
        const targets = electronCdp.targets?.length
          ? electronCdp.targets
          : [{ title, url: "", webSocketDebuggerUrl: electronCdp.webSocketDebuggerUrl }];
        for (const target of targets) {
          roots.push(createUiRoot({
            kind: ROOT_KIND_ELECTRON_PAGE,
            backend: BACKEND_CDP,
            title: target.title || title,
            appName: appName || "Electron",
            pid,
            windowId,
            nativeRootRef: rootRef,
            browserTargetId: target.targetId,
            cdpEndpoint: electronCdp.webSocketDebuggerUrl,
            cdpTargetId: target.targetId,
            cdpPageUrl: target.url,
            url: target.url,
            isOnscreen: win.isOnscreen ?? true,
          }));
        }
      } else {
        // Default or Fallback to Native AX Window
        roots.push(createUiRoot({
          kind: ROOT_KIND_DESKTOP_WINDOW,
          backend: BACKEND_MACOS_AX,
          title,
          appName,
          pid,
          windowId,
          nativeRootRef: rootRef,
          isOnscreen: win.isOnscreen ?? true,
        }));
      }
    }

    return roots;
  }
}
