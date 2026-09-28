import { semanticRole } from "./semantic_role.mjs";
import { createRefIdentityHelpers } from "./ref_identity.mjs";

/**
 * Browser Worker semantic snapshots. Refs remain attached to their DOM node;
 * revisions and comparison history are process-local and scoped to a page and
 * document, so neither tabs nor persisted application state share a cache.
 */

const snapshotsByPage = new WeakMap();
const MAX_HISTORY = 12;
const SEMANTIC_FIELDS = ["role", "name", "value", "checked", "selectedLabel", "disabled", "readOnly"];

function semanticState(refs) {
  return Object.fromEntries(Object.entries(refs || {}).map(([ref, item]) => [
    ref,
    Object.fromEntries(SEMANTIC_FIELDS.filter((field) => item[field] !== undefined).map((field) => [field, item[field]])),
  ]));
}

function makeDelta(previous, current) {
  const before = previous.state;
  const after = current.state;
  const added = [];
  const changed = [];
  const removed = [];
  for (const [ref, item] of Object.entries(after)) {
    if (!Object.hasOwn(before, ref)) added.push({ ref, ...item });
    else if (JSON.stringify(before[ref]) !== JSON.stringify(item)) {
      const beforeFields = before[ref];
      const afterFields = item;
      const fields = [...new Set([...Object.keys(beforeFields), ...Object.keys(afterFields)])];
      const beforeChange = {};
      const afterChange = {};
      for (const field of fields) {
        if (JSON.stringify(beforeFields[field]) !== JSON.stringify(afterFields[field])) {
          beforeChange[field] = beforeFields[field] ?? null;
          afterChange[field] = afterFields[field] ?? null;
        }
      }
      changed.push({ ref, before: beforeChange, after: afterChange });
    }
  }
  for (const ref of Object.keys(before)) if (!Object.hasOwn(after, ref)) removed.push(ref);
  return { added, changed, removed };
}

function payload(snapshot) {
  const { state: _state, ...publicSnapshot } = snapshot;
  return publicSnapshot;
}

export async function generatePageSnapshot(page, options = {}) {
  const { sinceRevision } = options;
  const workerInstanceId = options.workerInstanceId || page.workerInstanceId || "";
  const targetId = options.targetId || page.targetId || "";
  const title = await page.title().catch(() => "");
  const url = page.url();

  const snapshotData = await page
    .evaluate(({ roleSource, refIdentitySource, workerId }) => {
      const inferRole = new Function(`return (${roleSource});`)();
      const refIdentity = new Function(`return (${refIdentitySource})();`)();
      const documentNonce = window.__agentCabinWorkDocumentNonce ||= crypto.randomUUID();
      const refNamespace = refIdentity.namespace(workerId, documentNonce);
      const refNodes = window.__agentCabinWorkRefNodes ||= new WeakMap();
      let idCounter = Number.isSafeInteger(Number(window.__agentCabinWorkRefCounter)) && Number(window.__agentCabinWorkRefCounter) > 0
        ? Number(window.__agentCabinWorkRefCounter)
        : 1;
      const refs = {};
      const lines = [];
      const interactiveTags = new Set(["a", "button", "input", "select", "textarea", "summary"]);
      const interactiveRoles = new Set([
        "button", "link", "textbox", "searchbox", "checkbox", "radio", "combobox", "tab", "menuitem", "switch", "slider",
      ]);
      function nextRef() {
        let ref = refIdentity.makeRef(refNamespace, idCounter++);
        while (document.querySelector(`[data-work-ref="${ref}"]`)) ref = refIdentity.makeRef(refNamespace, idCounter++);
        return ref;
      }

      function isElementVisible(el) {
        if (!el || el.nodeType !== Node.ELEMENT_NODE) return false;
        const style = window.getComputedStyle(el);
        if (style.display === "none" || style.visibility === "hidden" || style.opacity === "0") return false;
        const rect = el.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0;
      }

      function getElementLabel(el) {
        const ariaLabel = el.getAttribute("aria-label");
        if (ariaLabel && ariaLabel.trim()) return ariaLabel.trim();
        const labelledBy = el.getAttribute("aria-labelledby");
        if (labelledBy) {
          const text = labelledBy.split(/\s+/).map((id) => document.getElementById(id)?.innerText || "").join(" ").trim();
          if (text) return text;
        }
        const associatedLabel = el.labels?.[0]?.innerText?.trim();
        if (associatedLabel) return associatedLabel;
        for (const attribute of ["placeholder", "title", "name"]) {
          const value = el.getAttribute(attribute);
          if (value && value.trim()) return value.trim();
        }
        return (el.innerText || el.textContent || "").slice(0, 80).trim();
      }

      function traverse(node, depth = 0) {
        if (!node || depth > 20) return;
        if (node.nodeType === Node.ELEMENT_NODE) {
          const el = node;
          const tag = el.tagName.toLowerCase();
          const role = el.getAttribute("role") || "";
          const isInteractive = interactiveTags.has(tag) || interactiveRoles.has(role) || el.hasAttribute("onclick") || el.getAttribute("tabindex") === "0" || el.isContentEditable;
          if (isInteractive && isElementVisible(el)) {
            const taggedRef = el.getAttribute("data-work-ref");
            const existingRef = refIdentity.isInNamespace(taggedRef, refNamespace) && refNodes.get(el) === taggedRef
              ? taggedRef
              : "";
            const refId = existingRef || nextRef();
            if (!existingRef) el.setAttribute("data-work-ref", refId);
            refNodes.set(el, refId);
            const label = getElementLabel(el);
            const value = el.value !== undefined ? String(el.value) : undefined;
            const finalRole = inferRole(tag, el.type, role);
            const checked = tag === "input" && (el.type === "checkbox" || el.type === "radio") ? Boolean(el.checked) : undefined;
            const selectedLabel = tag === "select" ? String(el.selectedOptions?.[0]?.textContent || "").trim() : "";
            refs[refId] = {
              ref: refId,
              role: finalRole,
              name: label,
              ...(value === undefined ? {} : { value }),
              ...(checked === undefined ? {} : { checked }),
              ...(selectedLabel ? { selectedLabel } : {}),
              ...(el.disabled ? { disabled: true } : {}),
              ...(el.readOnly ? { readOnly: true } : {}),
              tag,
            };
            const indent = "  ".repeat(Math.min(depth, 10));
            let line = `${indent}[ref=${refId}] ${finalRole}`;
            if (label) line += ` "${label}"`;
            if (value !== undefined && value !== "") line += ` value=${JSON.stringify(value)}`;
            if (selectedLabel) line += ` selected=${JSON.stringify(selectedLabel)}`;
            if (checked !== undefined) line += checked ? " checked" : " unchecked";
            if (el.disabled) line += " disabled";
            if (el.readOnly) line += " readonly";
            lines.push(line);
          } else if (["h1", "h2", "h3", "h4", "h5", "h6", "nav", "main"].includes(tag) && isElementVisible(el)) {
            const label = getElementLabel(el);
            if (label) lines.push(`${"  ".repeat(Math.min(depth, 10))}${tag} "${label}"`);
          }
        }
        for (const child of node.children) traverse(child, depth + 1);
      }

      traverse(document.body, 0);
      window.__agentCabinWorkRefCounter = idCounter;
      return { tree: lines.join("\n"), refs, documentKey: String(performance.timeOrigin) };
    }, {
      roleSource: semanticRole.toString(),
      refIdentitySource: createRefIdentityHelpers.toString(),
      workerId: workerInstanceId,
    })
    .catch((err) => ({ tree: `(Failed to capture DOM snapshot: ${err?.message})`, refs: {}, documentKey: "unavailable" }));

  let pageState = snapshotsByPage.get(page);
  if (!pageState || pageState.documentKey !== snapshotData.documentKey) {
    pageState = { documentKey: snapshotData.documentKey, documentId: `doc-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`, revision: 0, history: new Map() };
  }
  const revision = ++pageState.revision;
  const current = {
    revision,
    documentId: pageState.documentId,
    workerInstanceId,
    targetId,
    url,
    title,
    tree: snapshotData.tree || "(empty or inaccessible page)",
    refs: snapshotData.refs || {},
    state: semanticState(snapshotData.refs),
  };
  pageState.history.set(revision, {
    revision,
    documentId: current.documentId,
    state: current.state,
  });
  while (pageState.history.size > MAX_HISTORY) pageState.history.delete(pageState.history.keys().next().value);
  snapshotsByPage.set(page, pageState);

  if (sinceRevision === undefined || sinceRevision === null) return { snapshotType: "full", ...payload(current) };
  const previous = pageState.history.get(Number(sinceRevision));
  if (!previous || previous.documentId !== current.documentId || previous.revision >= current.revision) {
    return { snapshotType: "full", ...payload(current) };
  }
  const delta = makeDelta(previous, current);
  const deltaResult = {
    snapshotType: "delta",
    revision,
    baseRevision: previous.revision,
    documentId: current.documentId,
    workerInstanceId: current.workerInstanceId,
    targetId: current.targetId,
    url,
    title,
    ...delta,
  };
  if (delta.added.length + delta.changed.length + delta.removed.length === 0) {
    return { snapshotType: "unchanged", revision, baseRevision: previous.revision, documentId: current.documentId, workerInstanceId, targetId, url, title };
  }
  const fullSnapshot = { snapshotType: "full", ...payload(current) };
  const fullSize = JSON.stringify(fullSnapshot).length;
  if (delta.added.length + delta.changed.length + delta.removed.length > Math.max(25, Object.keys(current.refs).length / 2) || JSON.stringify(deltaResult).length >= fullSize) {
    return fullSnapshot;
  }
  return deltaResult;
}
