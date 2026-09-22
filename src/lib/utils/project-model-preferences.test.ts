import { describe, expect, it } from "vitest";
import { shouldApplyProjectModelPreference } from "./project-model-preferences";

describe("shouldApplyProjectModelPreference", () => {
  const args = {
    preference: { model: "model-b" },
    currentModel: "model-a",
    availableModelIds: ["model-a", "model-b"],
  } as const;

  it("applies the shared preference to a new conversation draft", () => {
    expect(shouldApplyProjectModelPreference({ ...args, hasConversationMessages: false })).toBe(
      true,
    );
  });

  it("does not apply the project preference to existing conversations with messages", () => {
    expect(shouldApplyProjectModelPreference({ ...args, hasConversationMessages: true })).toBe(
      false,
    );
  });

  it("does not overwrite a manual draft selection while the first message is starting", () => {
    expect(
      shouldApplyProjectModelPreference({
        ...args,
        hasConversationMessages: false,
        conversationStartPending: true,
      }),
    ).toBe(false);
  });

  it("does not apply a model outside the current catalog", () => {
    expect(
      shouldApplyProjectModelPreference({
        ...args,
        availableModelIds: ["model-a"],
        hasConversationMessages: false,
      }),
    ).toBe(false);
  });
});
