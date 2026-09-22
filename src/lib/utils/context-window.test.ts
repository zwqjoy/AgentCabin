import { describe, expect, it } from "vitest";
import { DEFAULT_CUSTOM_CONTEXT_WINDOW, resolveContextWindow } from "./context-window";

describe("resolveContextWindow", () => {
  it("prefers a window reported by the active session", () => {
    expect(
      resolveContextWindow({
        reportedWindow: 200_000,
        model: "claude-opus",
        models: [{ value: "claude-opus", displayName: "Opus", description: "", contextWindow: 1 }],
      }),
    ).toEqual({ contextWindow: 200_000, estimated: false });
  });

  it("reads Grok live model metadata", () => {
    expect(
      resolveContextWindow({
        model: "grok-4.5",
        models: [
          { value: "grok-4.5", displayName: "Grok 4.5", description: "", contextWindow: 500_000 },
        ],
      }),
    ).toEqual({ contextWindow: 500_000, estimated: false });
  });

  it("matches qualified and unqualified model aliases", () => {
    expect(
      resolveContextWindow({
        model: "DeepSeek-V4-Flash",
        models: [
          {
            value: "deepseek/deepseek-v4-flash",
            displayName: "DeepSeek V4 Flash",
            description: "",
            contextWindow: 128_000,
          },
        ],
      }),
    ).toEqual({ contextWindow: 128_000, estimated: false });
  });

  it("uses the shared estimated fallback for custom providers without metadata", () => {
    expect(resolveContextWindow({ model: "custom-model", customProvider: true })).toEqual({
      contextWindow: DEFAULT_CUSTOM_CONTEXT_WINDOW,
      estimated: true,
    });
  });

  it("keeps native models unknown instead of inventing a window", () => {
    expect(resolveContextWindow({ model: "unknown-native-model" })).toEqual({
      contextWindow: 0,
      estimated: false,
    });
  });
});
