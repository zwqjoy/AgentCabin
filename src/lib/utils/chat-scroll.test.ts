import { describe, expect, it } from "vitest";
import { isNearChatBottom } from "./chat-scroll";

describe("chat scroll helpers", () => {
  it("keeps auto-scroll enabled only while the viewport is near the bottom", () => {
    expect(isNearChatBottom({ scrollHeight: 1000, scrollTop: 701, clientHeight: 260 })).toBe(true);
    expect(isNearChatBottom({ scrollHeight: 1000, scrollTop: 699, clientHeight: 260 })).toBe(false);
  });
});
