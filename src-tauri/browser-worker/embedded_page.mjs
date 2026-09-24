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
        captureBeyondViewport: true,
      });
      return result?.data ? `data:image/jpeg;base64,${result.data}` : null;
    },
    async close() {
      client.close();
    },
    _url: "",
  };

  return page;
}
