import { randomUUID } from "node:crypto";
import {
  BACKEND_MACOS_AX,
  BACKEND_WINDOWS_UIA,
  BACKEND_CDP,
  ROOT_KIND_DESKTOP_WINDOW,
  ROOT_KIND_BROWSER_PAGE,
  ROOT_KIND_ELECTRON_PAGE,
  ROOT_KIND_DIALOG,
  ROOT_KIND_SHEET,
  ROOT_KIND_POPOVER,
  createUiRoot,
  createUiElement,
  StateStore,
} from "./computer_use_v3_models.mjs";
import {
  DesktopComputerUseBackend,
  decodeLegacyResponse,
  probeElectronCdp,
} from "./desktop_computer_use_backend.mjs";
import {
  CdpComputerUseBackend,
} from "./cdp_computer_use_backend.mjs";
import {
  VisualGroundingBackend,
  MODE_SEMANTIC,
  MODE_VISUAL,
  MODE_FUSED,
  computePerceptualDiff,
  defaultOcrProvider,
  resolveOcrHelper,
} from "./visual_grounding_backend.mjs";

export {
  BACKEND_MACOS_AX,
  BACKEND_WINDOWS_UIA,
  BACKEND_CDP,
  ROOT_KIND_DESKTOP_WINDOW,
  ROOT_KIND_BROWSER_PAGE,
  ROOT_KIND_ELECTRON_PAGE,
  ROOT_KIND_DIALOG,
  ROOT_KIND_SHEET,
  ROOT_KIND_POPOVER,
  createUiRoot,
  createUiElement,
  StateStore,
  DesktopComputerUseBackend,
  decodeLegacyResponse,
  probeElectronCdp,
  CdpComputerUseBackend,
  VisualGroundingBackend,
  MODE_SEMANTIC,
  MODE_VISUAL,
  MODE_FUSED,
  computePerceptualDiff,
  defaultOcrProvider,
  resolveOcrHelper,
};

const STATE_LIMIT = 128;

// Accept the native spelling too, but only resolve inside the supplied state.
function canonicalElementRef(ref) {
  return typeof ref === "string" && /^e\d+$/.test(ref) ? `@${ref}` : ref;
}

function textOf(element) {
  return [element?.title, element?.label, element?.name, element?.value, element?.description]
    .filter((value) => typeof value === "string" && value.trim())
    .join(" ");
}

function publicElementRef(value, index) {
  const raw = String(value || `e${index + 1}`).replace(/^@/, "");
  return `@${raw}`;
}

function rectOf(element) {
  const rect = element?.rect;
  if (!rect || typeof rect !== "object") return undefined;
  const x = Number(rect.x);
  const y = Number(rect.y);
  const width = Number(rect.width ?? rect.w);
  const height = Number(rect.height ?? rect.h);
  if (![x, y, width, height].every(Number.isFinite) || width < 0 || height < 0) return undefined;
  return { x, y, width, height };
}

function rectSimilarity(left, right) {
  const a = rectOf(left);
  const b = rectOf(right);
  if (!a || !b) return 0;
  const intersectionWidth = Math.max(0, Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x));
  const intersectionHeight = Math.max(0, Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y));
  const intersection = intersectionWidth * intersectionHeight;
  const union = a.width * a.height + b.width * b.height - intersection;
  if (union > 0) return intersection / union;
  return Math.hypot(a.x - b.x, a.y - b.y) < 2 ? 1 : 0;
}

export class ElementIdentity {
  static extract(element) {
    return {
      role: String(element?.role || element?.type || "").trim().toLowerCase(),
      subrole: String(element?.subrole || "").trim().toLowerCase(),
      identifier: String(element?.identifier || element?.id || "").trim(),
      title: String(element?.title || element?.label || element?.name || "").trim(),
      value: element?.value !== undefined && element?.value !== null ? String(element.value).trim() : undefined,
      description: String(element?.description || "").trim(),
      rect: rectOf(element),
      nativeRef: String(element?.nativeRef || element?.legacyRef || element?.ref || "").replace(/^@/, ""),
    };
  }

  static score(previous, next, previousIndex, nextIndex) {
    if (previous.role && next.role && previous.role !== next.role) return -Infinity;
    if (previous.identifier && next.identifier && previous.identifier !== next.identifier) return -Infinity;
    if (previous.title && next.title && previous.title !== next.title && !previous.identifier) return -Infinity;

    let score = 0;
    // Same native wire ref from the same backend session
    if (previous.nativeRef && previous.nativeRef === next.nativeRef) score += 1000;
    if (previous.identifier && previous.identifier === next.identifier) score += 90;
    if (previous.role && previous.role === next.role) score += 20;
    if (previous.subrole && previous.subrole === next.subrole) score += 15;
    if (previous.title && previous.title === next.title) score += 45;
    if (previous.value !== undefined && previous.value === next.value) score += 20;
    if (previous.description && previous.description === next.description) score += 15;

    const rSim = rectSimilarity(previous, next);
    score += Math.round(rSim * 35);

    // Only consider index proximity when elements have distinguishing geometry,
    // unique identifiers, or matching wire refs. Identical abstract controls without
    // coordinates must not arbitrarily claim refs based on order alone.
    const hasDistinguishingFeatures = Boolean(
      (previous.identifier && next.identifier) ||
      (previous.rect && next.rect) ||
      (previous.nativeRef && previous.nativeRef === next.nativeRef)
    );
    if (hasDistinguishingFeatures && previousIndex !== undefined && nextIndex !== undefined) {
      const idxDiff = Math.abs(previousIndex - nextIndex);
      if (idxDiff === 0) score += 15;
      else if (idxDiff <= 2) score += 10;
      else if (idxDiff <= 5) score += 5;
    }
    return score;
  }
}

export class RefStabilizer {
  constructor() {
    this.nextRefNum = 1;
  }

  stabilize(nextElements, baseState) {
    const previous = Array.isArray(baseState?.elements) ? baseState.elements : [];
    const usedPrevious = new Set();
    const usedPublicRefs = new Set();

    const prevIdentities = previous.map((el) => ElementIdentity.extract(el));
    const nextIdentities = nextElements.map((el) => ElementIdentity.extract(el));

    // Compute score matrix: scores[i][j]
    const scores = [];
    for (let i = 0; i < nextElements.length; i++) {
      scores[i] = [];
      for (let j = 0; j < previous.length; j++) {
        scores[i][j] = ElementIdentity.score(prevIdentities[j], nextIdentities[i], j, i);
      }
    }

    // 1. Identify ambiguous candidates (ties for top score)
    const ambiguousPrev = new Set();
    for (let j = 0; j < previous.length; j++) {
      let maxScore = -Infinity;
      let topCount = 0;
      for (let i = 0; i < nextElements.length; i++) {
        const s = scores[i][j];
        if (s >= 45) {
          if (s > maxScore) {
            maxScore = s;
            topCount = 1;
          } else if (s === maxScore) {
            topCount++;
          }
        }
      }
      if (topCount > 1) {
        ambiguousPrev.add(j);
      }
    }

    const ambiguousNext = new Set();
    for (let i = 0; i < nextElements.length; i++) {
      let maxScore = -Infinity;
      let topCount = 0;
      for (let j = 0; j < previous.length; j++) {
        const s = scores[i][j];
        if (s >= 45) {
          if (s > maxScore) {
            maxScore = s;
            topCount = 1;
          } else if (s === maxScore) {
            topCount++;
          }
        }
      }
      if (topCount > 1) {
        ambiguousNext.add(i);
      }
    }

    // 2. Build candidate pairs and sort descending by score for greedy 1-to-1 matching
    const candidatePairs = [];
    for (let i = 0; i < nextElements.length; i++) {
      for (let j = 0; j < previous.length; j++) {
        const s = scores[i][j];
        if (s >= 45 && !ambiguousPrev.has(j) && !ambiguousNext.has(i)) {
          candidatePairs.push({ nextIndex: i, prevIndex: j, score: s });
        }
      }
    }
    candidatePairs.sort((a, b) => b.score - a.score);

    // 3. Greedily match highest score pairs enforcing 1-to-1 matching
    const matchedNext = new Map();
    for (const pair of candidatePairs) {
      if (matchedNext.has(pair.nextIndex) || usedPrevious.has(pair.prevIndex)) {
        continue;
      }
      matchedNext.set(pair.nextIndex, {
        previousElement: previous[pair.prevIndex],
        prevIndex: pair.prevIndex,
        score: pair.score,
      });
      usedPrevious.add(pair.prevIndex);
    }

    // Determine highest existing public ref index to avoid reuse collisions
    for (const el of previous) {
      const match = el.ref && /^@e(\d+)$/.exec(el.ref);
      if (match) {
        this.nextRefNum = Math.max(this.nextRefNum, parseInt(match[1], 10) + 1);
      }
    }

    const stabilized = [];
    for (let i = 0; i < nextElements.length; i++) {
      const rawElement = nextElements[i];
      const match = matchedNext.get(i);
      const nativeRef = String(rawElement?.ref || rawElement?.nativeRef || `e${i + 1}`).replace(/^@/, "");

      let publicRef;
      if (match && match.previousElement?.ref && !usedPublicRefs.has(match.previousElement.ref)) {
        publicRef = match.previousElement.ref;
      } else {
        // If the element already has a valid public @eN format and baseState is empty, retain its number
        const ownRefMatch = typeof rawElement?.ref === "string" && /^@?e(\d+)$/i.exec(rawElement.ref);
        if (ownRefMatch && !baseState && !usedPublicRefs.has(`@e${ownRefMatch[1]}`)) {
          const num = parseInt(ownRefMatch[1], 10);
          publicRef = `@e${num}`;
          this.nextRefNum = Math.max(this.nextRefNum, num + 1);
        } else {
          while (usedPublicRefs.has(`@e${this.nextRefNum}`)) {
            this.nextRefNum++;
          }
          publicRef = `@e${this.nextRefNum++}`;
        }
      }
      usedPublicRefs.add(publicRef);

      stabilized.push({
        ...rawElement,
        ref: publicRef,
        legacyRef: nativeRef,
        nativeRef,
      });
    }

    return stabilized;
  }
}

function normalizeElements(structured, baseState) {
  const elements = Array.isArray(structured?.elements) ? structured.elements : [];
  const stabilizer = new RefStabilizer();
  return stabilizer.stabilize(elements, baseState);
}

function normalizeImages(parsed) {
  if (Array.isArray(parsed?.images)) {
    return parsed.images
      .filter((img) => img && typeof (img.data || img.base64) === "string")
      .map((img) => ({
        type: "image",
        data: img.data || img.base64,
        mimeType: img.mimeType || img.mime_type || "image/png",
      }));
  }
  return Array.isArray(parsed?.content)
    ? parsed.content
      .filter((block) => block?.type === "image" && typeof block?.data === "string")
      .map((block) => ({ type: "image", data: block.data, mimeType: block.mimeType || block.mime_type || "image/png" }))
    : [];
}

function result(text, details = {}, images = []) {
  return { content: [{ type: "text", text }, ...images], details };
}

function failure(error, details = {}) {
  const message = error instanceof Error ? error.message : String(error);
  return result(message, { ok: false, ...details });
}

function valueAt(object, ...keys) {
  for (const key of keys) {
    if (object?.[key] !== undefined && object?.[key] !== null) return object[key];
  }
  return undefined;
}

class ResourceScheduler {
  constructor() {
    this.resources = new Map();
  }

  epoch(resourceKey) {
    return this.resource(resourceKey).epoch;
  }

  async readAt(resourceKey, expectedEpoch, work) {
    return this.enqueue(resourceKey, async (record) => {
      if (record.epoch !== expectedEpoch) {
        throw new Error(`State is stale for ${resourceKey}: expected epoch ${expectedEpoch}, current epoch ${record.epoch}. Observe again.`);
      }
      return work(record.epoch);
    });
  }

  async write(resourceKey, expectedEpoch, work) {
    return this.enqueue(resourceKey, async (record) => {
      if (record.epoch !== expectedEpoch) {
        throw new Error(`State is stale for ${resourceKey}: expected epoch ${expectedEpoch}, current epoch ${record.epoch}. Observe again.`);
      }
      record.epoch += 1;
      return work(record.epoch);
    });
  }

  resource(resourceKey) {
    let record = this.resources.get(resourceKey);
    if (!record) {
      record = { epoch: 0, tail: Promise.resolve() };
      this.resources.set(resourceKey, record);
    }
    return record;
  }

  async enqueue(resourceKey, work) {
    const record = this.resource(resourceKey);
    const previous = record.tail;
    let release;
    const current = new Promise((resolve) => { release = resolve; });
    record.tail = previous.catch(() => undefined).then(() => current);
    await previous.catch(() => undefined);
    try {
      return await work(record);
    } finally {
      release();
    }
  }
}

export class ComputerUseV2Session {
  constructor(invokeBackend, options = {}) {
    if (typeof invokeBackend !== "function" && !(invokeBackend instanceof DesktopComputerUseBackend)) {
      throw new TypeError("ComputerUseV2Session requires a native backend invoker.");
    }
    this.invokeBackend = typeof invokeBackend === "function" ? invokeBackend : invokeBackend?.invokeBackend;
    const desktopOptions = {
      ...(options.desktop || options.desktopBackendOptions || {}),
      enableExternalCdp: options.enableElectronCdp
        ?? options.desktop?.enableExternalCdp
        ?? options.desktopBackendOptions?.enableExternalCdp
        ?? true,
    };
    this.desktopBackend = options.desktopBackend
      || (invokeBackend instanceof DesktopComputerUseBackend
        ? invokeBackend
        : new DesktopComputerUseBackend(this.invokeBackend, desktopOptions));

    if (options.cdpBackend instanceof CdpComputerUseBackend || (options.cdpBackend && typeof options.cdpBackend.listRoots === "function")) {
      this.cdpBackend = options.cdpBackend;
    } else if (typeof options.invokeBrowser === "function" || options.enableElectronCdp !== false) {
      this.cdpBackend = new CdpComputerUseBackend(options.invokeBrowser, {
        ...(options.cdp || options.cdpBackendOptions || {}),
      });
    } else {
      this.cdpBackend = undefined;
    }

    this.visualBackend = options.visualBackend instanceof VisualGroundingBackend
      ? options.visualBackend
      : new VisualGroundingBackend(options.visualGrounding || {});

    this.stateLimit = options.stateLimit || STATE_LIMIT;
    this.stateStore = new StateStore(this.stateLimit);
    this.roots = new Map();
    this.rootKeys = new Map();
    this.nextRoot = 1;
    this.lastRootRef = undefined;
    this.scheduler = new ResourceScheduler();
  }

  get states() {
    return this.stateStore.states;
  }

  get latestStates() {
    return this.stateStore.latestStates;
  }

  saveState(structured, parsed, epoch, baseState, publicRootRef, backendKind) {
    const backend = backendKind || structured?.backend || baseState?.backend || BACKEND_MACOS_AX;
    const isCdp = backend === BACKEND_CDP;
    const pid = Number(valueAt(structured, "pid") || valueAt(baseState, "pid") || 0);
    const windowId = Number(valueAt(structured, "window_id", "windowId") || valueAt(baseState, "windowId") || 0);
    const rootRef = publicRootRef || baseState?.rootRef;
    const nativeRootRef = String(valueAt(structured, "root_ref", "rootRef") || baseState?.nativeRootRef || "");
    const resourceKey = isCdp
      ? (rootRef ? `cdp-root:${rootRef}` : `cdp-page:${valueAt(structured, "url") || valueAt(baseState, "url") || "active"}`)
      : (rootRef
        ? `desktop-root:${rootRef}`
        : nativeRootRef
          ? `desktop-native-root:${nativeRootRef}`
          : pid > 0
            ? `desktop-pid:${pid}:${windowId}`
            : String(baseState?.resourceKey || "desktop:current"));
    const state = {
      stateId: randomUUID(),
      resourceKey,
      epoch: epoch ?? this.scheduler.epoch(resourceKey),
      pid: pid > 0 ? pid : undefined,
      windowId: windowId > 0 ? windowId : undefined,
      rootRef,
      nativeRootRef: nativeRootRef || undefined,
      observationId: valueAt(structured, "observation_id", "observationId"),
      backend,
      strategy: isCdp ? "cdp" : (structured?.strategy || "native"),
      elements: normalizeElements(structured, baseState),
      structured,
      images: normalizeImages(parsed || structured),
      capturedAt: Date.now(),
      platformState: structured?.platformState || structured,
    };
    this.stateStore.saveState(state);
    return state;
  }

  state(stateId) {
    return this.stateStore.getState(stateId);
  }

  registerRoot(rawRoot) {
    const isCdp = rawRoot.backend === BACKEND_CDP || rawRoot.kind === ROOT_KIND_BROWSER_PAGE || rawRoot.kind === ROOT_KIND_ELECTRON_PAGE;
    const pid = Number(valueAt(rawRoot, "pid") || 0);
    const windowId = Number(valueAt(rawRoot, "window_id", "windowId") || 0);
    const nativeRootRef = valueAt(rawRoot, "nativeRootRef", "root_ref", "rootRef");
    const browserTargetId = valueAt(rawRoot, "browserTargetId", "targetId");
    const url = valueAt(rawRoot, "url");

    const key = isCdp
      ? `cdp:${pid || ""}:${windowId || ""}:${browserTargetId ?? url ?? ""}`
      : `${pid}:${windowId}:${nativeRootRef || ""}`;

    let ref = this.rootKeys.get(key);
    if (!ref) {
      ref = `@r${this.nextRoot++}`;
      this.rootKeys.set(key, ref);
    }

    const appName = String(valueAt(rawRoot, "appName", "owner", "app") || (isCdp ? "Chromium" : ""));
    const title = String(valueAt(rawRoot, "title") || "");
    const kind = rawRoot.kind || (isCdp ? ROOT_KIND_BROWSER_PAGE : ROOT_KIND_DESKTOP_WINDOW);
    const backend = isCdp ? BACKEND_CDP : (rawRoot.backend || BACKEND_MACOS_AX);

    const uiRoot = createUiRoot({
      ref,
      kind,
      backend,
      title,
      appName,
      pid: pid > 0 ? pid : undefined,
      windowId: windowId > 0 ? windowId : undefined,
      nativeRootRef,
      browserTargetId,
      cdpEndpoint: valueAt(rawRoot, "cdpEndpoint"),
      cdpTargetId: valueAt(rawRoot, "cdpTargetId"),
      cdpPageUrl: valueAt(rawRoot, "cdpPageUrl"),
      url,
      frame: rawRoot.frame,
      isOnscreen: Boolean(valueAt(rawRoot, "isOnscreen", "on_screen", "onScreen") ?? true),
    });

    this.roots.set(ref, uiRoot);
    return uiRoot;
  }

  rootRef(window) {
    return this.registerRoot(window);
  }

  async backend(toolCallId, toolName, action, params, signal) {
    return decodeLegacyResponse(
      await this.invokeBackend(toolCallId, toolName, action, params || {}, signal),
      `${toolName} failed.`,
    );
  }

  async findRoots(toolCallId, params, signal) {
    try {
      const query = String(params?.text || params?.app || "").trim().toLowerCase();
      const discoveredRoots = [];

      if (this.desktopBackend) {
        try {
          const desktopRoots = await this.desktopBackend.listRoots(toolCallId, params, signal);
          for (const root of desktopRoots) {
            discoveredRoots.push(this.registerRoot(root));
          }
        } catch {
          // Desktop discovery error ignored
        }
      }

      if (this.cdpBackend) {
        try {
          const cdpRoots = await this.cdpBackend.listRoots(params, signal);
          for (const root of cdpRoots) {
            discoveredRoots.push(this.registerRoot(root));
          }
        } catch {
          // CDP discovery error ignored
        }
      }

      const filtered = discoveredRoots
        .filter((root) => !query || `${root.appName} ${root.title} ${root.url || ""}`.toLowerCase().includes(query))
        .slice(0, 50);

      const lines = filtered.map((root) => {
        if (root.backend === BACKEND_CDP) {
          return `${root.ref} ${root.appName} — ${root.title || "(untitled)"} (${root.url || "about:blank"})`;
        }
        return `${root.ref} ${root.appName} — ${root.title || "(untitled)"} pid=${root.pid || 0}`;
      });

      return result(
        lines.length ? lines.join("\n") : "No matching controllable roots found.",
        { ok: true, roots: filtered },
      );
    } catch (error) {
      return failure(error, { tool: "find_roots" });
    }
  }

  async launchApp(toolCallId, params, signal) {
    try {
      const url = params?.url || (params?.name && /^https?:\/\//i.test(params.name) ? params.name : undefined);
      const isBrowserRequest = Boolean(url || (params?.name && /^(browser|builtin_browser|safari|chrome|chromium|web)$/i.test(String(params.name).trim())));

      if (isBrowserRequest && this.cdpBackend) {
        const targetUrl = url || "about:blank";
        const navRes = await this.cdpBackend.invokeBrowser(`${toolCallId}:navigate`, "browser_navigate", { url: targetUrl }, signal);
        if (navRes && navRes.ok === false) {
          throw new Error(navRes.error || "browser_navigate failed");
        }
        const rootsRes = await this.cdpBackend.listRoots({}, signal);
        const pageRoot = Array.isArray(rootsRes) ? rootsRes[0] : rootsRes?.roots?.[0];
        if (pageRoot) {
          const root = this.registerRoot(pageRoot);
          this.lastRootRef = root.ref;
          return await this.observeUi(`${toolCallId}:initial`, { root: root.ref }, signal);
        }
        throw new Error("Failed to initialize browser page target. No active CDP page found.");
      }

      const launched = await this.backend(`${toolCallId}:launch`, "desktop_open_app", "launch", {
        name: params?.name,
        bundle_id: params?.bundleId || params?.bundle_id,
        creates_new_application_instance: params?.createsNewApplicationInstance,
      }, signal);
      const target = launched?.structuredContent || launched || {};
      // A launch response usually contains only an AX root reference. Refresh
      // the desktop inventory once so an Electron app launched in this call can
      // be promoted to its real CDP page root before the first observation.
      let discoveredTarget;
      if (this.desktopBackend) {
        try {
          const discovered = await this.desktopBackend.listRoots(`${toolCallId}:roots`, {}, signal);
          const launchedPid = Number(target.pid || 0);
          const launchedName = String(target.app_name || target.appName || params?.name || "").toLowerCase();
          discoveredTarget = discovered.find((candidate) => (
            launchedPid > 0 && candidate.pid === launchedPid
          ) || (
            launchedName && String(candidate.appName || "").toLowerCase().includes(launchedName)
          ));
        } catch {
          // The launch result remains a valid AX fallback if inventory refresh
          // or Electron probing is unavailable.
        }
      }
      const root = this.registerRoot(discoveredTarget || {
        pid: target.pid,
        window_id: target.window_id,
        root_ref: target.root_ref,
        owner: target.app_name,
        title: target.title,
      });
      this.lastRootRef = root.ref;
      return await this.observeUi(`${toolCallId}:initial`, { root: root.ref }, signal);
    } catch (error) {
      return failure(error, { tool: "launch_app" });
    }
  }

  async enrichWithVisualGrounding(raw, parsed, mode = MODE_SEMANTIC, root = undefined) {
    if (!this.visualBackend) return raw;
    const images = normalizeImages(parsed || raw);
    const screenshot = images[0];
    if (!screenshot || !this.visualBackend.shouldTrigger(raw?.elements, mode)) {
      return raw;
    }
    const visualElements = await this.visualBackend.ground(screenshot, { mode, root });
    if (!visualElements || visualElements.length === 0) {
      return raw;
    }
    const elements = Array.isArray(raw?.elements) ? [...raw.elements] : [];
    if (mode === MODE_VISUAL || elements.length === 0) {
      return { ...raw, elements: visualElements };
    }
    const semanticTexts = new Set(elements.map((e) => textOf(e).toLowerCase()).filter(Boolean));
    const additional = visualElements.filter((v) => {
      const vt = textOf(v).toLowerCase();
      return !vt || !semanticTexts.has(vt);
    });
    return { ...raw, elements: [...elements, ...additional] };
  }

  async observeUi(toolCallId, params, signal) {
    try {
      const requestedRoot = params?.root || this.lastRootRef;
      const root = requestedRoot ? this.roots.get(requestedRoot) : undefined;
      if (params?.root && !root) throw new Error(`Unknown root '${params.root}'. Call find_roots again.`);
      if (requestedRoot && !root) throw new Error(`Unknown root '${requestedRoot}'. Call find_roots again.`);
      if (root) this.lastRootRef = root.ref;

      const isCdp = root?.backend === BACKEND_CDP;

      const capture = async () => {
        let raw;
        let parsed;
        if (isCdp) {
          if (!this.cdpBackend) throw new Error("CDP Browser backend is not configured for this session.");
          raw = await this.cdpBackend.observe(root, { maxElements: 1000 }, signal);
          parsed = raw;
        } else {
          parsed = await this.backend(`${toolCallId}:observe`, "desktop_observe", "observe", {
            ...(root ? { pid: root.pid, window_id: root.windowId, root_ref: root.nativeRootRef } : {}),
            max_elements: 1000,
          }, signal);
          raw = parsed?.structuredContent || parsed || {};
          raw.content = parsed?.content;
        }

        raw = await this.enrichWithVisualGrounding(raw, parsed, params?.mode, root);

        const pid = Number(valueAt(raw, "pid") || root?.pid || 0);
        const resolvedKey = isCdp
          ? (root?.ref ? `cdp-root:${root.ref}` : `cdp-page:${valueAt(raw, "url") || "active"}`)
          : (root?.ref
            ? `desktop-root:${root.ref}`
            : pid > 0
              ? `desktop-pid:${pid}:${Number(valueAt(raw, "window_id", "windowId") || 0)}`
              : "desktop:current");

        const previous = this.stateStore.getLatestState(resolvedKey);
        const state = this.saveState(raw, parsed, this.scheduler.epoch(resolvedKey), previous, root?.ref, isCdp ? BACKEND_CDP : BACKEND_MACOS_AX);
        return this.stateResult("Observed UI", state, previous, parsed);
      };

      if (!root) return await capture();
      const resourceKey = isCdp ? `cdp-root:${root.ref}` : `desktop-root:${root.ref}`;
      return await this.scheduler.readAt(resourceKey, this.scheduler.epoch(resourceKey), capture);
    } catch (error) {
      return failure(error, { tool: "observe_ui" });
    }
  }

  stateResult(prefix, state, baseState, parsed) {
    const before = new Map((baseState?.elements || []).map((element) => [element.ref, element]));
    const after = new Map(state.elements.map((element) => [element.ref, element]));
    const changes = [];
    if (baseState) {
      for (const [ref, element] of after) {
        const previous = before.get(ref);
        if (!previous) changes.push({ type: "added", ref, element });
        else if (textOf(previous) !== textOf(element)) changes.push({ type: "updated", ref, element });
      }
      for (const [ref, element] of before) if (!after.has(ref)) changes.push({ type: "removed", ref, element });
    }
    const outline = state.elements.slice(0, 80).map((element) => `${element.ref} ${element.role || element.type || "element"} ${textOf(element)}`.trim());
    const body = baseState
      ? `${prefix}. Successor state ${state.stateId}; ${changes.length} semantic change(s).\n${changes.slice(0, 40).map((change) => `${change.type === "added" ? "+" : change.type === "removed" ? "-" : "~"} ${change.ref} ${textOf(change.element)}`).join("\n") || "(no semantic element changes)"}`
      : `${prefix}. stateId ${state.stateId}.\n${outline.join("\n") || "(no accessibility elements)"}`;
    return result(body, {
      ok: true,
      stateId: state.stateId,
      resourceKey: state.resourceKey,
      epoch: state.epoch,
      baseStateId: baseState?.stateId,
      changes,
      elements: state.elements,
      backend: state.backend,
      compatibilityBackend: true,
    }, (state.images && state.images.length > 0) ? state.images : (parsed ? normalizeImages(parsed) : []));
  }

  cachedElements(params) {
    return this.state(params?.stateId).elements;
  }

  searchUi(_toolCallId, params) {
    try {
      const state = this.state(params?.stateId);
      const text = String(params?.text || "").trim().toLowerCase();
      const role = String(params?.role || "").trim().toLowerCase();
      if (!text && !role) throw new Error("search_ui requires text or role.");
      const matches = state.elements.filter((element) => {
        return (!text || textOf(element).toLowerCase().includes(text)) && (!role || String(element.role || element.type || "").toLowerCase() === role);
      }).slice(0, 50);
      return result(matches.map((element) => `${element.ref} ${element.role || element.type || "element"} ${textOf(element)}`.trim()).join("\n") || "No matching elements.", { ok: true, stateId: state.stateId, matches });
    } catch (error) {
      return failure(error, { tool: "search_ui" });
    }
  }

  inspectUi(_toolCallId, params) {
    try {
      const state = this.state(params?.stateId);
      const element = state.elements.find((candidate) => candidate.ref === canonicalElementRef(params?.ref));
      if (!element) throw new Error(`Element '${params?.ref}' is not owned by state ${state.stateId}.`);
      return result(JSON.stringify(element, null, 2), { ok: true, stateId: state.stateId, element });
    } catch (error) {
      return failure(error, { tool: "inspect_ui" });
    }
  }

  expandUi(toolCallId, params) {
    try {
      const state = this.state(params?.stateId);
      const index = state.elements.findIndex((candidate) => candidate.ref === canonicalElementRef(params?.ref));
      if (index < 0) throw new Error(`Element '${params?.ref}' is not owned by state ${state.stateId}.`);
      const radius = Math.max(1, Math.min(8, Number(params?.depth || 3)));
      const elements = state.elements.slice(Math.max(0, index - radius), index + radius + 1);
      return result(elements.map((element) => `${element.ref} ${element.role || element.type || "element"} ${textOf(element)}`.trim()).join("\n"), { ok: true, stateId: state.stateId, elements });
    } catch (error) {
      return failure(error, { tool: "expand_ui", toolCallId });
    }
  }

  readText(_toolCallId, params) {
    try {
      const state = this.state(params?.stateId);
      const element = state.elements.find((candidate) => candidate.ref === canonicalElementRef(params?.ref));
      if (!element) throw new Error(`Element '${params?.ref}' is not owned by state ${state.stateId}.`);
      const text = textOf(element);
      const offset = Math.max(0, Number(params?.offset || 0));
      const page = text.slice(offset, offset + 16_384);
      return result(page, { ok: true, stateId: state.stateId, offset, totalChars: text.length, hasMore: offset + page.length < text.length });
    } catch (error) {
      return failure(error, { tool: "read_text" });
    }
  }

  conditionMatches(state, condition = {}) {
    const ref = canonicalElementRef(condition.ref);
    const candidates = ref ? state.elements.filter((element) => element.ref === ref) : state.elements;
    const present = candidates.some((element) => {
      if (condition.text && !textOf(element).toLowerCase().includes(String(condition.text).toLowerCase())) return false;
      if (condition.role && String(element.role || element.type || "").toLowerCase() !== String(condition.role).toLowerCase()) return false;
      if (condition.value !== undefined && String(element.value ?? "") !== String(condition.value)) return false;
      return true;
    });
    return condition.until === "absent" ? !present : present;
  }

  async checkCondition(baseState, current, condition = {}) {
    const matched = this.conditionMatches(current, condition);
    if (!matched) return false;
    if (condition.requireVisualDiff && this.visualBackend) {
      return await this.visualBackend.verifyVisualChange(baseState, current, condition);
    }
    return true;
  }

  conditionTimeoutMs(condition = {}) {
    return Math.max(100, Math.min(60_000, Number(condition.timeoutMs || 10_000)));
  }

  async waitForCondition(toolCallId, baseState, condition, epoch, signal) {
    const timeoutMs = this.conditionTimeoutMs(condition);
    const deadline = Date.now() + timeoutMs;
    let current = baseState;
    let pollIndex = 0;
    while (!(await this.checkCondition(baseState, current, condition)) && Date.now() < deadline) {
      await new Promise((resolve) => setTimeout(resolve, Math.min(250, Math.max(0, deadline - Date.now()))));
      if (current.backend === BACKEND_CDP) {
        const root = this.roots.get(current.rootRef) || {
          ref: current.rootRef,
          backend: BACKEND_CDP,
          browserTargetId: valueAt(current.platformState, "browserTargetId"),
          url: valueAt(current.platformState, "url"),
        };
        let raw = await this.cdpBackend.observe(root, { maxElements: 1000 }, signal);
        raw = await this.enrichWithVisualGrounding(raw, raw, condition?.mode, root);
        current = this.saveState(raw, raw, epoch, current, current.rootRef, BACKEND_CDP);
      } else {
        const parsed = await this.backend(`${toolCallId}:poll:${pollIndex++}`, "desktop_observe", "observe", {
          pid: baseState.pid,
          window_id: baseState.windowId,
          root_ref: baseState.nativeRootRef,
          max_elements: 1000,
        }, signal);
        let raw = parsed?.structuredContent || {};
        raw = await this.enrichWithVisualGrounding(raw, parsed, condition?.mode, baseState.rootRef ? this.roots.get(baseState.rootRef) : undefined);
        current = this.saveState(raw, parsed, epoch, current, current.rootRef, BACKEND_MACOS_AX);
      }
    }
    const found = await this.checkCondition(baseState, current, condition);
    return { current, found, timedOut: !found, polls: pollIndex };
  }

  legacyAction(state, action) {
    if (action.stateId && action.stateId !== state.stateId) {
      throw new Error(`Coordinate action target stateId '${action.stateId}' does not match active state '${state.stateId}'. Cross-state coordinate reuse is prohibited.`);
    }
    const element = action.ref ? state.elements.find((candidate) => candidate.ref === canonicalElementRef(action.ref)) : undefined;
    if (action.ref && !element) throw new Error(`Element '${action.ref}' is not owned by state ${state.stateId}.`);

    let resolvedX = action.x;
    let resolvedY = action.y;
    const isVisual = Boolean(element?.evidence?.visual);

    if (element && isVisual) {
      const coords = this.visualBackend.resolveElementCoordinates(state, action.ref);
      resolvedX = coords.x;
      resolvedY = coords.y;
    }

    const common = {
      pid: state.pid || undefined,
      window_id: state.windowId || undefined,
      root_ref: state.nativeRootRef || undefined,
      observation_id: state.observationId,
      element_token: isVisual ? undefined : element?.legacyRef,
    };
    if (action.action === "press" || action.action === "click") {
      return { tool: "desktop_click", intent: "click", params: { ...common, native_action: action.action, x: resolvedX, y: resolvedY, button: action.button, count: action.clickCount } };
    }
    if (action.action === "setText" || action.action === "typeText") {
      return { tool: "desktop_type", intent: "type_text", params: { ...common, native_action: action.action, text: action.text, x: resolvedX, y: resolvedY } };
    }
    if (action.action === "keypress") {
      const keys = Array.isArray(action.keys) ? action.keys.map(String) : [];
      const key = keys.at(-1);
      if (!key) throw new Error("keypress requires at least one key.");
      return { tool: "desktop_key", intent: "press_key", params: { ...common, key, modifiers: keys.slice(0, -1) } };
    }
    if (action.action === "scroll") {
      const x = Number(action.scrollX || 0);
      const y = Number(action.scrollY || 0);
      const horizontal = Math.abs(x) > Math.abs(y);
      return { tool: "desktop_scroll", intent: "scroll", params: { ...common, direction: horizontal ? (x < 0 ? "left" : "right") : (y < 0 ? "down" : "up"), amount: Math.max(1, Math.abs(horizontal ? x : y) || 6) } };
    }
    throw new Error(`Action '${action.action}' is not supported by the current native backend.`);
  }

  async successorFromAction(parsed, epoch, baseState) {
    const structured = parsed?.structuredContent || parsed || {};
    let observation = structured?.observation || parsed?.observation;
    if (!observation || typeof observation !== "object") {
      throw new Error("The native backend executed the action but did not return a successor observation. Observe again before continuing.");
    }
    observation = await this.enrichWithVisualGrounding(observation, parsed, baseState?.mode, baseState?.rootRef ? this.roots.get(baseState.rootRef) : undefined);
    return this.saveState(observation, parsed, epoch, baseState, baseState?.rootRef, BACKEND_MACOS_AX);
  }

  async actUi(toolCallId, params, signal) {
    let baseState;
    try {
      baseState = this.state(params?.stateId);
      const actions = Array.isArray(params?.actions) ? params.actions : [];
      if (!actions.length || actions.length > 20) throw new Error("act_ui requires 1-20 actions.");
      const preexisting = params?.expect ? this.conditionMatches(baseState, params.expect) : false;
      return await this.scheduler.write(baseState.resourceKey, baseState.epoch, async (nextEpoch) => {
        const isCdp = baseState.backend === BACKEND_CDP;
        let current;
        let outcome = "unknown";
        let steps = [];
        let stoppedAt;
        let rawParsed;

        if (isCdp) {
          if (!this.cdpBackend) throw new Error("CDP Browser backend is not configured for this session.");
          const root = this.roots.get(baseState.rootRef) || {
            ref: baseState.rootRef,
            backend: BACKEND_CDP,
            browserTargetId: valueAt(baseState.platformState, "browserTargetId"),
            url: valueAt(baseState.platformState, "url"),
          };
          const res = await this.cdpBackend.act(root, baseState, { actions }, signal);
          outcome = res.outcome || "unknown";
          steps = res.execution?.steps || [];
          stoppedAt = res.execution?.stoppedAt;
          const enrichedObs = await this.enrichWithVisualGrounding(res.observation, res.observation, baseState?.mode, root);
          current = this.saveState(enrichedObs, res.observation, nextEpoch, baseState, baseState.rootRef, BACKEND_CDP);
          rawParsed = res.observation;
        } else {
          const mappedActions = actions.map((action) => this.legacyAction(baseState, action));
          const lastParsed = await this.backend(toolCallId, "desktop_act_batch", "act_batch", {
            pid: baseState.pid,
            window_id: baseState.windowId,
            root_ref: baseState.nativeRootRef,
            observation_id: baseState.observationId,
            actions: mappedActions.map((mapped) => ({
              tool: mapped.tool,
              intent: mapped.intent,
              params: mapped.params,
            })),
          }, signal);
          current = await this.successorFromAction(lastParsed, nextEpoch, baseState);
          outcome = lastParsed?.structuredContent?.verification?.outcome || "unknown";
          steps = Array.isArray(lastParsed?.structuredContent?.execution?.steps)
            ? lastParsed.structuredContent.execution.steps
            : [];
          stoppedAt = lastParsed?.structuredContent?.execution?.stoppedAt;
          rawParsed = lastParsed;
        }

        let verification;
        if (params?.expect) {
          let matched = await this.checkCondition(baseState, current, params.expect);
          let timedOut = false;
          let polls = 0;
          if (!matched && outcome !== "didnt") {
            const waited = await this.waitForCondition(`${toolCallId}:expect`, current, params.expect, nextEpoch, signal);
            current = waited.current;
            matched = waited.found;
            timedOut = waited.timedOut;
            polls = waited.polls;
          }
          verification = {
            status: matched ? (preexisting ? "preexisting" : "verified") : "failed",
            condition: params.expect,
            polls,
            ...(timedOut ? { timedOut: true } : {}),
          };
          if (matched) {
            outcome = "worked";
          } else {
            outcome = "didnt";
          }
        }
        const response = this.stateResult(`Action outcome=${outcome}`, current, baseState, rawParsed);
        response.details.execution = {
          outcome,
          steps,
          verification,
          ...(stoppedAt !== undefined ? { stoppedAt } : {}),
        };
        return response;
      });
    } catch (error) {
      return failure(error, { tool: "act_ui", stateId: baseState?.stateId });
    }
  }

  async waitFor(toolCallId, params, signal) {
    let baseState;
    try {
      baseState = this.state(params?.stateId);
      return await this.scheduler.readAt(baseState.resourceKey, baseState.epoch, async (epoch) => {
        const waited = await this.waitForCondition(toolCallId, baseState, params, epoch, signal);
        const response = this.stateResult(waited.found ? "Condition satisfied" : "Condition timed out", waited.current, baseState);
        response.details.found = waited.found;
        response.details.timedOut = waited.timedOut;
        response.details.polls = waited.polls;
        return response;
      });
    } catch (error) {
      return failure(error, { tool: "wait_for", stateId: baseState?.stateId });
    }
  }
}

export { failure, result };
