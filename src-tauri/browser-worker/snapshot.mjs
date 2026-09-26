/**
 * Browser Worker Snapshot Generator.
 * Converts Accessibility Tree and DOM structure into semantic references ([ref=e1], [ref=e2]).
 * Refs stay attached to their DOM node for the lifetime of the current document.
 */

export async function generatePageSnapshot(page) {
  const title = await page.title().catch(() => "");
  const url = page.url();

  // Tag DOM elements with unique data-work-ref attributes and extract semantic tree
  const snapshotData = await page
    .evaluate(() => {
      // Keep refs attached to existing nodes so a fresh observation does not
      // silently point an older ref at a different element.
      let idCounter = Number(window.__agentCabinWorkRefCounter) || 1;
      const refs = {};
      const lines = [];

      const interactiveTags = new Set(["a", "button", "input", "select", "textarea", "summary"]);
      const interactiveRoles = new Set([
        "button",
        "link",
        "textbox",
        "searchbox",
        "checkbox",
        "radio",
        "combobox",
        "tab",
        "menuitem",
        "switch",
        "slider",
      ]);

      function isElementVisible(el) {
        if (!el || el.nodeType !== Node.ELEMENT_NODE) return false;
        const style = window.getComputedStyle(el);
        if (style.display === "none" || style.visibility === "hidden" || style.opacity === "0") {
          return false;
        }
        const rect = el.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0;
      }

      function getElementLabel(el) {
        const ariaLabel = el.getAttribute("aria-label");
        if (ariaLabel && ariaLabel.trim()) return ariaLabel.trim();
        const placeholder = el.getAttribute("placeholder");
        if (placeholder && placeholder.trim()) return placeholder.trim();
        const title = el.getAttribute("title");
        if (title && title.trim()) return title.trim();
        const name = el.getAttribute("name");
        if (name && name.trim()) return name.trim();

        // Check text content (first 80 chars)
        const text = (el.innerText || el.textContent || "").slice(0, 80).trim();
        return text;
      }

      function traverse(node, depth = 0) {
        if (!node || depth > 20) return;
        if (node.nodeType === Node.ELEMENT_NODE) {
          const el = node;
          const tag = el.tagName.toLowerCase();
          const role = el.getAttribute("role") || "";
          const explicitRole = role || (tag === "input" && el.type === "search" ? "searchbox" : tag === "input" ? "textbox" : tag);

          const isInteractive =
            interactiveTags.has(tag) ||
            interactiveRoles.has(role) ||
            el.hasAttribute("onclick") ||
            el.getAttribute("tabindex") === "0" ||
            el.isContentEditable;

          if (isInteractive && isElementVisible(el)) {
            const taggedRef = el.getAttribute("data-work-ref");
            const existingRef = /^e\d+$/.test(taggedRef || "") ? taggedRef : "";
            const refId = existingRef || ("e" + idCounter++);
            if (!existingRef) el.setAttribute("data-work-ref", refId);

            const label = getElementLabel(el);
            const value = el.value !== undefined ? String(el.value).trim() : "";
            const finalRole = role || (tag === "a" ? "link" : tag === "input" ? (el.type === "submit" || el.type === "button" ? "button" : "textbox") : tag);
            const checked = tag === "input" && (el.type === "checkbox" || el.type === "radio")
              ? Boolean(el.checked)
              : undefined;
            const selectedLabel = tag === "select"
              ? String(el.selectedOptions?.[0]?.textContent || "").trim()
              : "";

            refs[refId] = {
              ref: refId,
              role: finalRole,
              name: label,
              value: value || undefined,
              ...(checked === undefined ? {} : { checked }),
              ...(selectedLabel ? { selectedLabel } : {}),
              ...(el.disabled ? { disabled: true } : {}),
              ...(el.readOnly ? { readOnly: true } : {}),
              tag,
            };

            const indent = "  ".repeat(Math.min(depth, 10));
            let line = `${indent}[ref=${refId}] ${finalRole}`;
            if (label) line += ` "${label}"`;
            if (value) line += ` value="${value}"`;
            if (selectedLabel) line += ` selected=${JSON.stringify(selectedLabel)}`;
            if (checked !== undefined) line += checked ? " checked" : " unchecked";
            if (el.disabled) line += " disabled";
            if (el.readOnly) line += " readonly";
            lines.push(line);
          } else if (["h1", "h2", "h3", "h4", "h5", "h6", "nav", "main"].includes(tag) && isElementVisible(el)) {
            const label = getElementLabel(el);
            if (label) {
              const indent = "  ".repeat(Math.min(depth, 10));
              lines.push(`${indent}${tag} "${label}"`);
            }
          }
        }

        for (const child of node.children) {
          traverse(child, depth + 1);
        }
      }

      traverse(document.body, 0);
      window.__agentCabinWorkRefCounter = idCounter;

      return {
        tree: lines.join("\n"),
        refs,
      };
    })
    .catch((err) => ({
      tree: `(Failed to capture DOM snapshot: ${err?.message})`,
      refs: {},
    }));

  return {
    title,
    url,
    tree: snapshotData.tree || "(empty or inaccessible page)",
    refs: snapshotData.refs || {},
  };
}
