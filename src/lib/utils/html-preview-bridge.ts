/**
 * Height reporting bridge for sandboxed inline HTML previews.
 *
 * A sandboxed iframe (`sandbox="allow-scripts"`, opaque origin) cannot be measured
 * from the parent document: the preview markup is rendered by the agent and often
 * ends up shorter than a fixed frame height, which leaves a blank band under the
 * chart. The preview document therefore measures itself and reports its content
 * height through postMessage so the parent can size the frame to the content.
 */
export const PREVIEW_HEIGHT_MESSAGE = "agentcabin:html-preview:height";

/** Lowest frame height the parent will render, keeps tiny snippets readable. */
export const PREVIEW_MIN_HEIGHT = 96;

/** Height used until the preview reports its own content height. */
export const PREVIEW_DEFAULT_HEIGHT = 320;

/** Reject absurd reports before they reach layout. */
const PREVIEW_HEIGHT_LIMIT = 20000;

const BRIDGE_MARKER = "data-agentcabin-preview-bridge";

// Kept dependency free and conservative: it runs inside the sandboxed preview
// document, where only inline scripts are allowed by the injected CSP.
const BRIDGE_SCRIPT = `<script ${BRIDGE_MARKER}>
(function () {
  var MESSAGE = "${PREVIEW_HEIGHT_MESSAGE}";
  var lastHeight = -1;
  var growthTarget = -1;
  var shrinkBlocked = false;
  var growthBlocked = false;
  var timer = 0;
  var RESIZE_DELAY = 16;

  function pixels(value) {
    var parsed = parseFloat(value);
    return isFinite(parsed) ? parsed : 0;
  }

  function naturalHeight() {
    var body = document.body;
    if (!body) return 0;
    // Neutralize viewport-derived heights so the measurement reflects the
    // content itself instead of the current frame height.
    var probe = document.createElement("style");
    probe.textContent = "html,body{height:auto !important;min-height:0 !important}";
    var root = document.head || document.documentElement;
    root.appendChild(probe);
    var bodyRect = body.getBoundingClientRect();
    var style = window.getComputedStyle(body);
    var bottom = bodyRect.bottom;
    var children = body.children;
    for (var i = 0; i < children.length; i += 1) {
      var rect = children[i].getBoundingClientRect();
      if (rect.width > 0 && rect.height > 0) bottom = Math.max(bottom, rect.bottom);
    }
    var height = bottom - bodyRect.top + pixels(style.marginTop) + pixels(style.marginBottom);
    probe.remove();
    return height;
  }

  function scrollExtent() {
    var html = document.documentElement;
    var body = document.body;
    return Math.max(html.scrollHeight, body ? body.scrollHeight : 0);
  }

  function send(height) {
    var rounded = Math.round(height);
    if (rounded < 1 || Math.abs(rounded - lastHeight) < 1) return;
    lastHeight = rounded;
    window.parent.postMessage({ type: MESSAGE, height: rounded }, "*");
  }

  function measure() {
    var viewport = document.documentElement.clientHeight;
    // The frame has no layout box yet (srcdoc parsed before the parent sized it).
    if (viewport <= 0) return;
    var extent = scrollExtent();
    if (extent > viewport + 1) {
      if (growthBlocked) return;
      var natural = naturalHeight();
      // A natural measurement below the viewport means the probe collapsed a
      // viewport-dependent layout, so never shrink back to it.
      if (natural < viewport - 1) shrinkBlocked = true;
      var target = Math.round(Math.max(extent, natural));
      var applied = growthTarget > 0 && Math.abs(viewport - growthTarget) <= 1;
      // Either the frame is capped by the host CSS, or the content grows with
      // the frame: stop chasing it and let the frame scroll instead.
      if (applied || target <= growthTarget) {
        growthBlocked = true;
        return;
      }
      growthTarget = target;
      send(target);
      return;
    }
    growthTarget = -1;
    if (shrinkBlocked) return;
    send(naturalHeight());
  }

  function schedule() {
    if (timer) return;
    timer = setTimeout(function () {
      timer = 0;
      measure();
    }, RESIZE_DELAY);
  }

  function onReady() {
    schedule();
    if (typeof ResizeObserver === "function") {
      var observer = new ResizeObserver(schedule);
      observer.observe(document.documentElement);
      if (document.body) observer.observe(document.body);
    }
    if (typeof MutationObserver === "function" && document.body) {
      new MutationObserver(schedule).observe(document.body, {
        childList: true,
        subtree: true,
        characterData: true,
        attributes: true,
      });
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", onReady);
  } else {
    onReady();
  }
  window.addEventListener("load", schedule);
  window.addEventListener("resize", schedule);
  schedule();
  setTimeout(schedule, 60);
  setTimeout(schedule, 300);
  setTimeout(schedule, 1000);
})();
</script>`;

/** Add the height bridge to preview markup; safe to apply more than once. */
export function injectPreviewBridge(rawHtml: string): string {
  if (!rawHtml || rawHtml.includes(BRIDGE_MARKER)) return rawHtml;
  const lower = rawHtml.toLowerCase();
  const bodyClose = lower.lastIndexOf("</body");
  if (bodyClose !== -1)
    return rawHtml.slice(0, bodyClose) + BRIDGE_SCRIPT + rawHtml.slice(bodyClose);
  const htmlClose = lower.lastIndexOf("</html");
  if (htmlClose !== -1)
    return rawHtml.slice(0, htmlClose) + BRIDGE_SCRIPT + rawHtml.slice(htmlClose);
  return rawHtml + BRIDGE_SCRIPT;
}

/** Validate a reported preview height from a postMessage payload. */
export function parsePreviewHeightMessage(data: unknown): number | null {
  if (!data || typeof data !== "object") return null;
  const message = data as { type?: unknown; height?: unknown };
  if (message.type !== PREVIEW_HEIGHT_MESSAGE) return null;
  const height = message.height;
  if (typeof height !== "number" || !Number.isFinite(height)) return null;
  if (height < 1 || height > PREVIEW_HEIGHT_LIMIT) return null;
  return Math.round(height);
}
