import { describe, expect, it } from "vitest";
import { formatPetError } from "./pet-errors";

describe("formatPetError", () => {
  it("preserves Tauri string rejection messages", () => {
    expect(formatPetError("Command create_custom_pet not found", "fallback")).toBe(
      "Command create_custom_pet not found",
    );
  });

  it("preserves Error messages and falls back for empty values", () => {
    expect(formatPetError(new Error("permission denied"), "fallback")).toBe("permission denied");
    expect(formatPetError("", "fallback")).toBe("fallback");
    expect(formatPetError(null, "fallback")).toBe("fallback");
  });
});
