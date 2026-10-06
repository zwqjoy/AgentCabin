import { afterEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("./transport", () => ({
  getTransport: () => ({ invoke }),
}));

import { discoverWorkBuddyConnectors } from "$lib/api/work";

describe("discoverWorkBuddyConnectors", () => {
  afterEach(() => invoke.mockReset());

  it("uses the WorkBuddy marketplace roots by default", async () => {
    const response = { available: false, roots: [], packages: [], warnings: [] };
    invoke.mockResolvedValue(response);

    await expect(discoverWorkBuddyConnectors()).resolves.toBe(response);
    expect(invoke).toHaveBeenCalledWith("work_discover_workbuddy_connectors", { root: null });
  });

  it("passes a user-selected directory to the Host scanner", async () => {
    invoke.mockResolvedValue({
      available: true,
      roots: ["/tmp/connectors"],
      packages: [],
      warnings: [],
    });

    await discoverWorkBuddyConnectors("/tmp/connectors");

    expect(invoke).toHaveBeenCalledWith("work_discover_workbuddy_connectors", {
      root: "/tmp/connectors",
    });
  });
});
