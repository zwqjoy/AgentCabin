import { describe, expect, it } from "vitest";
import { searchSettings } from "./search-index";

describe("Settings Search Index", () => {
  it("finds Codex related settings", () => {
    const results = searchSettings("Codex");
    expect(results.length).toBeGreaterThan(0);
    expect(results.some((r) => r.tab === "code" || r.keywords.includes("codex"))).toBe(true);
  });

  it("finds Provider related settings", () => {
    const results = searchSettings("Provider");
    expect(results.length).toBeGreaterThan(0);
    expect(results.some((r) => r.tab === "models")).toBe(true);
  });

  it("finds Browser related settings", () => {
    const results = searchSettings("Browser");
    expect(results.length).toBeGreaterThan(0);
    expect(results.some((r) => r.tab === "browser-use")).toBe(true);
  });

  it("finds MCP related settings", () => {
    const results = searchSettings("MCP");
    expect(results.length).toBeGreaterThan(0);
    expect(results.some((r) => r.tab === "mcp" || r.keywords.includes("mcp"))).toBe(true);
  });

  it("finds Pet related settings", () => {
    const results = searchSettings("Pet");
    expect(results.length).toBeGreaterThan(0);
    expect(results.some((r) => r.tab === "pet")).toBe(true);
  });

  it("finds Language related settings", () => {
    const results = searchSettings("language");
    expect(results.length).toBeGreaterThan(0);
    expect(results.some((r) => r.tab === "general")).toBe(true);
  });

  it("finds Appearance / Theme related settings", () => {
    const results = searchSettings("theme");
    expect(results.length).toBeGreaterThan(0);
    expect(results.some((r) => r.tab === "appearance")).toBe(true);
  });
});
