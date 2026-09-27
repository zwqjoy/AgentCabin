import test from "node:test";
import assert from "node:assert/strict";
import { semanticRole } from "./semantic_role.mjs";

test("snapshot and locator share native and semantic role inference", () => {
  const elements = [
    ["input", "checkbox", "checkbox"],
    ["input", "radio", "radio"],
    ["input", "search", "searchbox"],
    ["input", "text", "textbox"],
    ["input", "button", "button"],
    ["input", "submit", "button"],
    ["button", "", "button"],
    ["a", "", "link"],
    ["select", "", "select"],
    ["textarea", "", "textarea"],
  ];
  for (const [tag, type, expected] of elements) {
    const snapshotRole = semanticRole(tag, type);
    const locatorRole = semanticRole(tag, type);
    assert.equal(snapshotRole, expected, `${tag}[type=${type}] snapshot`);
    assert.equal(locatorRole, snapshotRole, `${tag}[type=${type}] locator`);
  }
  assert.equal(semanticRole("input", "checkbox", "switch"), "switch");
});
