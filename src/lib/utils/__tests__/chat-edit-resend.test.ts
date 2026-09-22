import { describe, expect, it } from "vitest";
import zhCN from "../../../../messages/zh-CN.json";
import en from "../../../../messages/en.json";

function calculateRollbackTurns(totalUserTurns: number, turnIndex: number): number {
  return Math.max(1, totalUserTurns - turnIndex);
}

describe("chat edit & resend logic", () => {
  it("has matching i18n keys across zh-CN and en dictionaries", () => {
    expect(zhCN).toHaveProperty("chat_editMessage", "编辑消息");
    expect(zhCN).toHaveProperty("chat_editCancel", "取消");
    expect(zhCN).toHaveProperty("chat_editSend", "发送");

    expect(en).toHaveProperty("chat_editMessage", "Edit message");
    expect(en).toHaveProperty("chat_editCancel", "Cancel");
    expect(en).toHaveProperty("chat_editSend", "Send");
  });

  it("calculates correct number of turns to rollback", () => {
    // 5 total turns (indices 0, 1, 2, 3, 4)
    // Editing the latest turn (index 4) should drop 1 turn
    expect(calculateRollbackTurns(5, 4)).toBe(1);

    // Editing turn index 2 should drop 3 turns (indices 2, 3, 4)
    expect(calculateRollbackTurns(5, 2)).toBe(3);

    // Editing the very first turn (index 0) should drop all 5 turns
    expect(calculateRollbackTurns(5, 0)).toBe(5);

    // Guard against edge cases where turnIndex >= totalUserTurns
    expect(calculateRollbackTurns(3, 5)).toBe(1);
  });
});
