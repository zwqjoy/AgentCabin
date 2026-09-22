import { describe, it, expect } from "vitest";
import { sanitizeRules, filterRules } from "../permissions-helpers";

describe("sanitizeRules", () => {
  it("trims whitespace", () => {
    expect(sanitizeRules(["  rule1  ", " rule2"])).toEqual(["rule1", "rule2"]);
  });

  it("filters empty strings", () => {
    expect(sanitizeRules(["rule1", "", "   ", "rule2"])).toEqual(["rule1", "rule2"]);
  });

  it("removes duplicates (preserves order)", () => {
    expect(sanitizeRules(["a", "b", "a", "c", "b"])).toEqual(["a", "b", "c"]);
  });

  it("deduplicates after trimming", () => {
    expect(sanitizeRules(["  rule ", "rule"])).toEqual(["rule"]);
  });

  it("returns empty for all-empty input", () => {
    expect(sanitizeRules(["", "  ", ""])).toEqual([]);
  });

  it("handles empty array", () => {
    expect(sanitizeRules([])).toEqual([]);
  });
});

describe("filterRules", () => {
  const rules = ["Bash(npm test)", "Read(~/docs)", "Write(src/**)", "bash(git commit)"];

  it("returns all rules for empty search", () => {
    expect(filterRules(rules, "")).toEqual(rules);
    expect(filterRules(rules, "   ")).toEqual(rules);
  });

  it("case-insensitive substring match", () => {
    expect(filterRules(rules, "bash")).toEqual(["Bash(npm test)", "bash(git commit)"]);
  });

  it("matches partial strings", () => {
    expect(filterRules(rules, "src")).toEqual(["Write(src/**)"]);
  });

  it("returns empty for no match", () => {
    expect(filterRules(rules, "zzz")).toEqual([]);
  });
});
