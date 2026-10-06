import { afterEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("./transport", () => ({
  getTransport: () => ({ invoke }),
}));

import { discoverWorkBuddyExperts } from "./api";

describe("discoverWorkBuddyExperts", () => {
  afterEach(() => invoke.mockReset());

  it("uses the Host default WorkBuddy directory when no root is provided", async () => {
    const response = { available: false, packages: [], warnings: [] };
    invoke.mockResolvedValue(response);

    await expect(discoverWorkBuddyExperts()).resolves.toBe(response);
    expect(invoke).toHaveBeenCalledWith("discover_workbuddy_experts", { root: null });
  });

  it("passes an explicitly selected directory to the Host scanner", async () => {
    invoke.mockResolvedValue({ available: true, root: "/tmp/experts", packages: [], warnings: [] });

    await discoverWorkBuddyExperts("/tmp/experts");

    expect(invoke).toHaveBeenCalledWith("discover_workbuddy_experts", { root: "/tmp/experts" });
  });
});
