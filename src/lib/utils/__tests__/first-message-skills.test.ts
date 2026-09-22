import { describe, it, expect, vi } from "vitest";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("$lib/transport", () => ({
  getTransport: () => ({
    invoke: mockInvoke,
    isDesktop: () => true,
    isWeb: () => false,
  }),
}));

vi.mock("$lib/utils/debug", () => ({
  dbg: vi.fn(),
  dbgWarn: vi.fn(),
  redactSensitive: (x: unknown) => x,
}));

import * as api from "$lib/api";

describe("First message skills forwarding", () => {
  it("passes selected skills in startSession payload", async () => {
    const skills = [{ name: "hub-skill-a", path: "/Users/test/.agentcabin/skills/hub-skill-a" }];

    await api.startSession(
      "run-test-123",
      undefined,
      undefined,
      "hello world",
      undefined,
      undefined,
      undefined,
      skills,
    );

    expect(mockInvoke).toHaveBeenCalledWith("start_session", {
      runId: "run-test-123",
      mode: undefined,
      sessionId: undefined,
      initialMessage: "hello world",
      attachments: null,
      platformId: null,
      permissionModeOverride: null,
      skills,
    });
  });
});
