/**
 * AgentCabin Computer Use V3 CDP Browser Backend.
 *
 * Implements the ComputerUseBackend contract for Chromium / Browser / Electron pages
 * using the Host Browser Worker / native CDP bridge.
 */

import {
  BACKEND_CDP,
  ROOT_KIND_BROWSER_PAGE,
  ROOT_KIND_ELECTRON_PAGE,
  createUiRoot,
} from "./computer_use_v3_models.mjs";

function browserFailureMessage(response, operation) {
  if (!response || typeof response !== "object") return `${operation} failed.`;
  const text = response.error
    || response.stderr
    || response.message
    || response.details?.error
    || response.content?.find?.((block) => block?.type === "text")?.text;
  return text ? `${operation} failed: ${text}` : `${operation} failed.`;
}

export function assertBrowserSuccess(response, operation = "Browser operation") {
  if (response?.ok === false || response?.success === false || response?.details?.ok === false || response?.isError) {
    throw new Error(browserFailureMessage(response, operation));
  }
  return response;
}

export class CdpComputerUseBackend {
  constructor(invokeBrowser) {
    this.invokeBrowser = invokeBrowser;
  }

  relayParams(root) {
    return {
      rootKey: root.resourceKey,
      cdpEndpoint: root.cdpEndpoint,
      cdpTargetId: root.cdpTargetId || root.browserTargetId,
      browserTargetId: root.browserTargetId,
      cdpPageUrl: root.cdpPageUrl,
      title: root.title,
      url: root.url,
    };
  }

  async observeRelay(root, signal) {
    return assertBrowserSuccess(
      await this.invokeBrowser("cdp:relay:observe", "browser_cdp_observe", this.relayParams(root), signal),
      "browser_cdp_observe",
    );
  }

  async actRelay(root, baseState, transaction, signal) {
    const stateElements = baseState?.elements || [];
    const actions = (transaction.actions || []).map((action) => {
      const target = action.ref ? stateElements.find((element) => element.ref === action.ref) : undefined;
      const nativeRef = target?.nativeRef || target?.legacyRef;
      return { ...action, ...(nativeRef ? { ref: nativeRef } : {}) };
    });
    return assertBrowserSuccess(
      await this.invokeBrowser(
        "cdp:relay:act",
        "browser_cdp_act",
        { ...this.relayParams(root), actions },
        signal,
      ),
      "browser_cdp_act",
    );
  }

  async listRoots(_params, signal) {
    if (typeof this.invokeBrowser !== "function") return [];
    try {
      const response = assertBrowserSuccess(
        await this.invokeBrowser("cdp:list_roots", "browser_tabs", { action: "list" }, signal),
        "browser_tabs",
      );
      const tabs = Array.isArray(response?.tabs) ? response.tabs : [];
      if (!tabs.length && response?.url) {
        tabs.push({ index: 0, url: response.url, title: response.title || "Browser Page", active: true });
      }
      return tabs.map((tab, idx) => {
        const url = tab.url || "";
        const title = tab.title || "Untitled Tab";
        const isElectron = url.startsWith("file://") || url.includes("vscode-webview") || title.includes("Visual Studio Code");
        const targetId = String(tab.targetId || tab.id || tab.pageId || (tab.index !== undefined ? tab.index : idx));
        return createUiRoot({
          kind: isElectron ? ROOT_KIND_ELECTRON_PAGE : ROOT_KIND_BROWSER_PAGE,
          backend: BACKEND_CDP,
          resourceKey: `cdp-page:${targetId}:${url}`,
          title,
          appName: isElectron ? "Electron" : "Chromium",
          browserTargetId: targetId,
          url,
          isOnscreen: Boolean(tab.active ?? true),
        });
      });
    } catch {
      return [];
    }
  }

  async observe(root, options = {}, signal) {
    if (root.cdpEndpoint) return this.observeRelay(root, signal);
    const isNumericIndex = root.browserTargetId !== undefined && /^\d+$/.test(String(root.browserTargetId));
    const tabIndex = isNumericIndex ? Number(root.browserTargetId) : undefined;
    try {
      assertBrowserSuccess(await this.invokeBrowser("cdp:switch_tab", "browser_tabs", {
        action: "switch",
        targetId: root.browserTargetId,
        ...(tabIndex !== undefined ? { index: tabIndex } : {}),
      }, signal), "browser_tabs");
    } catch (err) {
      throw new Error(`stale_browser_target: Browser root '${root.ref || root.browserTargetId}' is stale (${err instanceof Error ? err.message : String(err)}). Call find_roots again.`);
    }

    const snapshot = assertBrowserSuccess(
      await this.invokeBrowser("cdp:snapshot", "browser_snapshot", {}, signal),
      "browser_snapshot",
    );
    const refsMap = snapshot?.refs || {};
    const rawElements = Object.entries(refsMap).map(([refId, item]) => {
      const role = String(item.role || item.tag || "element").toLowerCase();
      const label = String(item.name || item.title || item.label || "").trim();
      const value = item.value !== undefined ? String(item.value).trim() : undefined;
      return {
        ref: refId,
        nativeRef: refId,
        legacyRef: refId,
        backendRef: refId,
        role,
        title: label,
        name: label,
        label,
        value,
        tag: item.tag,
        rect: item.rect,
      };
    });

    let images = [];
    if (snapshot?.screenshot) {
      const img = snapshot.screenshot;
      images.push({
        type: "image",
        data: img.data || img.base64 || "",
        mimeType: img.mimeType || img.mime_type || "image/png",
      });
    } else {
      try {
        const shot = assertBrowserSuccess(
          await this.invokeBrowser("cdp:screenshot", "browser_take_screenshot", {}, signal),
          "browser_take_screenshot",
        );
        if (shot?.base64 || shot?.data) {
          images.push({
            type: "image",
            data: shot.base64 || shot.data,
            mimeType: shot.mimeType || "image/png",
          });
        }
      } catch {
        // Screenshot optional
      }
    }

    return {
      backend: BACKEND_CDP,
      title: snapshot?.title || root.title,
      url: snapshot?.url || root.url,
      elements: rawElements,
      images,
      platformState: {
        browserTargetId: root.browserTargetId,
        url: snapshot?.url || root.url,
      },
    };
  }

  async act(root, baseState, transaction, signal) {
    if (root.cdpEndpoint) return this.actRelay(root, baseState, transaction, signal);
    const isNumericIndex = root.browserTargetId !== undefined && /^\d+$/.test(String(root.browserTargetId));
    const tabIndex = isNumericIndex ? Number(root.browserTargetId) : undefined;
    try {
      assertBrowserSuccess(await this.invokeBrowser("cdp:switch_tab", "browser_tabs", {
        action: "switch",
        targetId: root.browserTargetId,
        ...(tabIndex !== undefined ? { index: tabIndex } : {}),
      }, signal), "browser_tabs");
    } catch (err) {
      throw new Error(`stale_browser_target: Browser root '${root.ref || root.browserTargetId}' is stale (${err instanceof Error ? err.message : String(err)}). Call find_roots again.`);
    }

    const actions = transaction.actions || [];
    const steps = [];
    let stoppedAt;

    for (let i = 0; i < actions.length; i++) {
      const action = actions[i];
      const targetElement = action.ref
        ? baseState.elements.find((el) => el.ref === action.ref)
        : undefined;

      const elementToken = targetElement?.nativeRef || targetElement?.legacyRef || (action.ref ? String(action.ref).replace(/^@/, "") : undefined);

      try {
        if (action.action === "press" || action.action === "click") {
          assertBrowserSuccess(await this.invokeBrowser(`cdp:click:${i}`, "browser_click", {
            ref: elementToken,
            button: action.button || "left",
            double_click: action.clickCount === 2,
          }, signal), "browser_click");
        } else if (action.action === "setText" || action.action === "typeText") {
          assertBrowserSuccess(await this.invokeBrowser(`cdp:type:${i}`, "browser_type", {
            ref: elementToken,
            text: String(action.text || ""),
          }, signal), "browser_type");
        } else if (action.action === "keypress") {
          const keys = Array.isArray(action.keys) ? action.keys : [];
          const key = keys.at(-1);
          if (!key) throw new Error("keypress requires at least one key.");
          assertBrowserSuccess(await this.invokeBrowser(`cdp:key:${i}`, "browser_press_key", { key }, signal), "browser_press_key");
        } else if (action.action === "scroll") {
          const x = Number(action.scrollX || 0);
          const y = Number(action.scrollY || 0);
          const horizontal = Math.abs(x) > Math.abs(y);
          assertBrowserSuccess(await this.invokeBrowser(`cdp:scroll:${i}`, "browser_scroll", {
            direction: horizontal ? (x < 0 ? "left" : "right") : (y < 0 ? "down" : "up"),
            amount: Math.max(1, Math.abs(horizontal ? x : y) || 5),
          }, signal), "browser_scroll");
        } else {
          throw new Error(`CDP action '${action.action}' is not supported.`);
        }
        steps.push({ outcome: "worked" });
      } catch (error) {
        steps.push({ outcome: "didnt", error: error instanceof Error ? error.message : String(error) });
        stoppedAt = i;
        break;
      }
    }

    const outcome = steps.some((s) => s.outcome === "didnt") ? "didnt" : "worked";
    const observation = await this.observe(root, {}, signal);

    return {
      outcome,
      execution: {
        outcome,
        steps,
        ...(stoppedAt !== undefined ? { stoppedAt } : {}),
      },
      observation,
    };
  }
}
