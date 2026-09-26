/**
 * Page adapter backed by the raw embedded CDP client.
 * Only operations that can be expressed safely through page-level CDP are
 * exposed; callers receive an explicit unsupported_action error otherwise.
 */
function evaluateExpression(body) {
  return {
    expression: `(async () => { ${body} })()`,
    awaitPromise: true,
    returnByValue: true,
  };
}

const SCREENSHOT_TIMEOUT_MS = 4000;

export function createEmbeddedPage({ client }) {
  async function evaluateValue(body) {
    const result = await client.send("Runtime.evaluate", evaluateExpression(body));
    if (result?.exceptionDetails) {
      throw new Error(result.exceptionDetails.text || "Page evaluation failed");
    }
    return result?.result?.value;
  }

  async function highlightTarget(selector) {
    const expression = JSON.stringify(selector);
    try {
      const found = await evaluateValue(`
        const el = document.querySelector(${expression});
        if (!el) return false;
        const outline = [el.style.getPropertyValue('outline'), el.style.getPropertyPriority('outline')];
        const offset = [el.style.getPropertyValue('outline-offset'), el.style.getPropertyPriority('outline-offset')];
        el.style.setProperty('outline', '3px solid #3b82f6', 'important');
        el.style.setProperty('outline-offset', '2px', 'important');
        window.setTimeout(() => {
          if (!el.isConnected) return;
          if (outline[0]) el.style.setProperty('outline', outline[0], outline[1]);
          else el.style.removeProperty('outline');
          if (offset[0]) el.style.setProperty('outline-offset', offset[0], offset[1]);
          else el.style.removeProperty('outline-offset');
        }, 1200);
        return true;
      `);
      if (found) await new Promise((resolve) => setTimeout(resolve, 220));
    } catch {
      // Highlight is a visual aid; a cross-origin or stale target must not
      // prevent the actual browser action from reporting its own result.
    }
  }

  const page = {
    async title() {
      return (await evaluateValue("return document.title;")) ?? "";
    },
    url() {
      return page._url ?? "";
    },
    async evaluate(fn, arg) {
      const source = typeof fn === "string" ? fn : `(${fn.toString()})(${arg === undefined ? "undefined" : JSON.stringify(arg)})`;
      return evaluateValue(`return ${source};`);
    },
    async refreshLocation() {
      page._url = (await evaluateValue("return location.href;")) ?? "";
      return page._url;
    },
    async navigate(url) {
      const result = await client.send("Page.navigate", { url });
      page._url = url;
      return result;
    },
    async goBack() {
      const history = await client.send("Page.getNavigationHistory");
      const index = history?.currentIndex ?? 0;
      const entry = history?.entries?.[index - 1];
      if (!entry) throw new Error("No previous history entry");
      await client.send("Page.navigateToHistoryEntry", { entryId: entry.id });
      page._url = entry.url;
      return { url: entry.url };
    },
    async goForward() {
      const history = await client.send("Page.getNavigationHistory");
      const index = history?.currentIndex ?? 0;
      const entry = history?.entries?.[index + 1];
      if (!entry) throw new Error("No next history entry");
      await client.send("Page.navigateToHistoryEntry", { entryId: entry.id });
      page._url = entry.url;
      return { url: entry.url };
    },
    async reload() {
      await client.send("Page.reload", {});
      return page.refreshLocation();
    },
    async clickAt(x, y) {
      const base = { x: Number(x), y: Number(y), button: "left", clickCount: 1 };
      await client.send("Input.dispatchMouseEvent", { ...base, type: "mousePressed" });
      await client.send("Input.dispatchMouseEvent", { ...base, type: "mouseReleased" });
      return { ok: true };
    },
    async clickRef(ref) {
      const safeRef = String(ref).replace(/[^a-zA-Z0-9_-]/g, "");
      await highlightTarget(`[data-work-ref="${safeRef}"]`);
      const rect = await evaluateValue(
        `const el = document.querySelector('[data-work-ref="${safeRef}"]');
         if (!el) return null;
         const r = el.getBoundingClientRect();
         return { x: r.x, y: r.y, width: r.width, height: r.height };`,
      );
      if (!rect) throw new Error(`stale ref: ${ref} is no longer in the page`);
      return page.clickAt(rect.x + rect.width / 2, rect.y + rect.height / 2);
    },
    async clickSelector(selector) {
      const safeSelector = JSON.stringify(String(selector));
      await highlightTarget(String(selector));
      const rect = await evaluateValue(
        `const el = document.querySelector(${safeSelector});
         if (!el) return null;
         const r = el.getBoundingClientRect();
         return { x: r.x, y: r.y, width: r.width, height: r.height };`,
      );
      if (!rect) throw new Error(`stale selector: ${selector} is no longer in the page`);
      return page.clickAt(rect.x + rect.width / 2, rect.y + rect.height / 2);
    },
    async selectOption(ref, selector, requestedValue) {
      const safeRef = String(ref || "").replace(/[^a-zA-Z0-9_-]/g, "");
      const targetSelector = safeRef
        ? '[data-work-ref="' + safeRef + '"]'
        : String(selector || "");
      if (!targetSelector) throw new Error("Select requires a ref or CSS selector");

      await highlightTarget(targetSelector);
      const outcome = await evaluateValue(
        "const selector = " + JSON.stringify(targetSelector) + ";\n" +
        "const requested = " + JSON.stringify(String(requestedValue)) + ";\n" +
        "const select = document.querySelector(selector);\n" +
        "if (!select) return { error: 'Select target was not found: ' + selector };\n" +
        "if (select.tagName !== 'SELECT') return { error: 'Select target is not a native <select> element' };\n" +
        "if (select.multiple) return { error: 'Selecting options in a multi-select is not supported by this action' };\n" +
        "const option = Array.from(select.options).find((item) => item.value === requested) || " +
          "Array.from(select.options).find((item) => item.textContent.trim() === requested);\n" +
        "if (!option) return { error: 'No <select> option matches value or visible text: ' + requested };\n" +
        "if (option.disabled || option.closest('optgroup')?.disabled || select.disabled) return { error: 'The requested <select> option is disabled' };\n" +
        "const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set;\n" +
        "if (setter) setter.call(select, option.value); else select.value = option.value;\n" +
        "select.dispatchEvent(new Event('input', { bubbles: true }));\n" +
        "select.dispatchEvent(new Event('change', { bubbles: true }));\n" +
        "return { selectedValue: select.value, selectedLabel: option.textContent.trim(), selected: select.value === option.value };",
      );
      if (outcome?.error) throw new Error(outcome.error);
      if (!outcome?.selected) throw new Error("The requested <select> option was not selected");
      return outcome;
    },
    async typeText(text) {
      for (const char of String(text)) {
        await client.send("Input.dispatchKeyEvent", { type: "keyDown", text: char });
        await client.send("Input.dispatchKeyEvent", { type: "keyUp", text: char });
      }
      return { ok: true };
    },
    async pressKey(key) {
      await client.send("Input.dispatchKeyEvent", { type: "keyDown", key });
      await client.send("Input.dispatchKeyEvent", { type: "keyUp", key });
      return { ok: true };
    },
    async scroll(deltaY, deltaX = 0) {
      await client.send("Input.dispatchMouseEvent", {
        type: "mouseWheel",
        x: 10,
        y: 10,
        deltaX,
        deltaY,
      });
      return { ok: true };
    },
    async screenshot() {
      const result = await client.send("Page.captureScreenshot", {
        format: "jpeg",
        quality: 75,
        fromSurface: true,
        // Browser observations should represent the visible viewport. Capturing
        // the full document on every click/type/scroll can create multi-megabyte
        // images and stall the host while traces are serialized and rendered.
        captureBeyondViewport: false,
      }, SCREENSHOT_TIMEOUT_MS);
      return result?.data ? `data:image/jpeg;base64,${result.data}` : null;
    },
    async close() {
      client.close();
    },
    _url: "",
  };

  return page;
}
