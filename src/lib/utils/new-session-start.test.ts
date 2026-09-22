import { describe, expect, it } from "vitest";
import { resolveNewSessionStartMode } from "./new-session-start";

describe("resolveNewSessionStartMode", () => {
  it("starts the first normal Pi composer message as a prompt turn", () => {
    expect(resolveNewSessionStartMode()).toBe("prompt");
  });

  it("keeps explicit Pi shell starts for discovery and feature controls", () => {
    expect(resolveNewSessionStartMode(true)).toBe("shell");
  });
});
