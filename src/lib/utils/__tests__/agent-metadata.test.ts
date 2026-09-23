import { describe, it, expect } from "vitest";
import { isKnownAgent, getAgentDisplayName, getAssistantDisplayName } from "../agent-metadata";

describe("isKnownAgent", () => {
  it("recognizes all first-party agents", () => {
    expect(isKnownAgent("claude")).toBe(true);
    expect(isKnownAgent("codex")).toBe(true);
    expect(isKnownAgent("pi")).toBe(true);
    expect(isKnownAgent("grok")).toBe(true);
  });

  it("returns false for unknown agents", () => {
    expect(isKnownAgent("gemini")).toBe(false);
    expect(isKnownAgent("")).toBe(false);
  });
});

describe("getAssistantDisplayName", () => {
  it("uses the canonical Pi Agent label for live and completed Pi messages", () => {
    expect(getAssistantDisplayName("pi", "Claude")).toBe("Pi Agent");
  });

  it("uses the canonical Claude Code label for Claude", () => {
    expect(getAssistantDisplayName("claude", "Claude (localized)")).toBe("Claude Code");
  });
});

describe("getAgentDisplayName", () => {
  it("returns one stable label for every known agent", () => {
    expect(getAgentDisplayName("claude")).toBe("Claude Code");
    expect(getAgentDisplayName("codex")).toBe("Codex");
    expect(getAgentDisplayName("pi")).toBe("Pi Agent");
    expect(getAgentDisplayName("grok")).toBe("Grok");
  });

  it("preserves unknown identifiers as a safe fallback", () => {
    expect(getAgentDisplayName("custom-agent")).toBe("custom-agent");
  });
});

describe("default runtime providers", () => {
  it("defaults only pi agent as enabled and default provider", async () => {
    const {
      DEFAULT_ENABLED_RUNTIME_PROVIDERS,
      DEFAULT_RUNTIME_PROVIDER,
      VISIBLE_RUNTIME_PROVIDERS,
    } = await import("../agent-metadata");
    expect(DEFAULT_ENABLED_RUNTIME_PROVIDERS).toEqual(["pi"]);
    expect(DEFAULT_RUNTIME_PROVIDER).toBe("pi");
    expect(VISIBLE_RUNTIME_PROVIDERS).toEqual(["pi"]);
  });
});
