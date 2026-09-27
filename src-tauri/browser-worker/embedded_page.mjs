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

  async function resolveTarget(ref, locator, selector) {
    const result = await evaluateValue(`
      const ref = ${JSON.stringify(String(ref || "").replace(/[^a-zA-Z0-9_-]/g, ""))};
      const locator = ${JSON.stringify(locator || {})};
      const selector = ${JSON.stringify(String(selector || ""))};
      const visible = (el) => {
        if (!el?.isConnected) return false;
        const style = getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        return style.display !== "none" && style.visibility !== "hidden" && Number(style.opacity) !== 0 && rect.width > 0 && rect.height > 0;
      };
      const roleOf = (el) => el.getAttribute("role") || (el.tagName === "A" ? "link" : el.tagName === "INPUT" ? (el.type === "checkbox" || el.type === "radio" ? el.type : el.type === "submit" || el.type === "button" ? "button" : el.type === "search" ? "searchbox" : "textbox") : el.tagName.toLowerCase());
      const nameOf = (el) => el.getAttribute("aria-label") || (el.getAttribute("aria-labelledby") || "").split(/\\s+/).map((id) => document.getElementById(id)?.innerText || "").join(" ").trim() || el.labels?.[0]?.innerText?.trim() || el.getAttribute("placeholder") || el.getAttribute("title") || el.innerText?.trim() || el.textContent?.trim() || "";
      const describe = (el) => {
        let ref = el.getAttribute("data-work-ref");
        if (!/^e\\d+$/.test(ref || "")) {
          const next = Number(window.__agentCabinWorkRefCounter) || 1;
          ref = 'e' + next;
          window.__agentCabinWorkRefCounter = next + 1;
          el.setAttribute("data-work-ref", ref);
        }
        const rect = el.getBoundingClientRect();
        return { ref, role: roleOf(el), name: nameOf(el).slice(0, 80), disabled: Boolean(el.disabled), readOnly: Boolean(el.readOnly), rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height } };
      };
      if (ref) {
        const el = document.querySelector('[data-work-ref="' + ref + '"]');
        if (el) {
          if (!visible(el)) return { error: { code: 'target_not_visible', message: 'The referenced element is no longer visible.' } };
          if (el.disabled) return { error: { code: 'target_disabled', message: 'The referenced element is disabled.' } };
          return { element: describe(el), source: 'ref' };
        }
      }
      const hasLocator = locator && Object.keys(locator).length > 0;
      if (hasLocator) {
        const candidates = Array.from(document.querySelectorAll('body *')).filter((el) => {
          if (!visible(el)) return false;
          if (locator.role && roleOf(el) !== locator.role) return false;
          if (locator.name && nameOf(el).trim() !== locator.name.trim()) return false;
          if (locator.label && !Array.from(el.labels || []).some((label) => label.innerText.trim() === locator.label.trim())) return false;
          if (locator.placeholder && el.getAttribute('placeholder')?.trim() !== locator.placeholder.trim()) return false;
          if (locator.text && (el.innerText || el.textContent || '').trim() !== locator.text.trim()) return false;
          return Boolean(locator.role || locator.name || locator.label || locator.placeholder || locator.text);
        });
        if (candidates.length > 1) return { error: { code: 'ambiguous_target', message: 'The semantic locator matches multiple visible elements.' } };
        if (candidates.length === 1) {
          if (candidates[0].disabled) return { error: { code: 'target_disabled', message: 'The semantic target is disabled.' } };
          return { element: describe(candidates[0]), source: 'locator' };
        }
      }
      if (selector) {
        let candidates;
        try { candidates = Array.from(document.querySelectorAll(selector)).filter(visible); }
        catch { return { error: { code: 'invalid_selector', message: 'The CSS selector is invalid.' } }; }
        if (candidates.length > 1) return { error: { code: 'ambiguous_target', message: 'The CSS selector matches multiple visible elements.' } };
        if (candidates.length === 1) {
          if (candidates[0].disabled) return { error: { code: 'target_disabled', message: 'The CSS target is disabled.' } };
          return { element: describe(candidates[0]), source: 'selector' };
        }
      }
      return { error: { code: ref ? 'stale_ref' : 'target_not_found', message: ref ? 'The ref is stale and no unique fallback target was found.' : 'No unique target matched the supplied locator or selector.' } };
    `);
    if (result?.error) {
      const error = new Error(result.error.message);
      error.code = result.error.code;
      throw error;
    }
    return result;
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
    async resolveTarget(ref, locator, selector) {
      return resolveTarget(ref, locator, selector);
    },
    async clearRef(ref) {
      const safeRef = String(ref).replace(/[^a-zA-Z0-9_-]/g, "");
      const cleared = await evaluateValue(`
        const el = document.querySelector('[data-work-ref="${safeRef}"]');
        if (!el) return false;
        if (el.readOnly || el.disabled) return false;
        const prototype = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
        const setter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;
        if (setter) setter.call(el, ''); else el.value = '';
        el.dispatchEvent(new Event('input', { bubbles: true }));
        el.dispatchEvent(new Event('change', { bubbles: true }));
        return true;
      `);
      if (!cleared) throw Object.assign(new Error(`The referenced target is stale, disabled, or read-only: ${ref}`), { code: "target_disabled" });
    },
    async clickRef(ref, locator, selector) {
      const target = await resolveTarget(ref, locator, selector);
      const rect = target.element.rect;
      await highlightTarget(target.element.ref ? `[data-work-ref="${target.element.ref}"]` : selector || `[data-work-ref="${String(ref || "").replace(/[^a-zA-Z0-9_-]/g, "")}"]`);
      await page.clickAt(rect.x + rect.width / 2, rect.y + rect.height / 2);
      return { target: target.element, resolvedBy: target.source };
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
    async selectOption(ref, selector, requestedValue, locator) {
      const target = await resolveTarget(ref, locator, selector);
      const targetSelector = target.element.ref
        ? '[data-work-ref="' + target.element.ref + '"]'
        : `*[data-work-ref="${String(ref || "").replace(/[^a-zA-Z0-9_-]/g, "")}"]`;
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
      return { ...outcome, target: target.element, resolvedBy: target.source };
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
