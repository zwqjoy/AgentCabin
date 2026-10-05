import { describe, expect, it, vi } from "vitest";
import { SessionExpertStore } from "./session-expert-store.svelte";
const expert = { id: "writer", name: "writer", title: "文档专家", icon: "文", isTeam: false };
function host() {
  return {
    getSessionExpert: vi.fn().mockResolvedValue(expert),
    setSessionExpert: vi.fn().mockImplementation(async (_run, id) => (id ? expert : null)),
    resolveSessionExpert: vi.fn().mockResolvedValue(expert),
  };
}
describe("conversation expert configuration", () => {
  it("keeps the draft selection on first-run adoption and restores it in a new store", async () => {
    const api = host();
    const store = new SessionExpertStore(api);
    await store.load("");
    await store.select(expert);
    store.adopt("run-a", store.selected);
    await store.load("run-a");
    expect(store.selected?.id).toBe("writer");
    const reopened = new SessionExpertStore(api);
    await reopened.load("run-a");
    expect(reopened.selected?.id).toBe("writer");
    await store.load("");
    expect(store.selected).toBeNull();
  });
  it("persists explicit exit and restores it without resurrecting historical selection", async () => {
    const api = host();
    const store = new SessionExpertStore(api);
    await store.load("run-a");
    await store.select(null);
    expect(api.setSessionExpert).toHaveBeenCalledWith("run-a", null);
    expect(store.selected).toBeNull();
    api.getSessionExpert.mockResolvedValue(null);
    const reopened = new SessionExpertStore(api);
    await reopened.load("run-a");
    expect(reopened.selected).toBeNull();
  });
  it("retains the previous selection when host activation fails", async () => {
    const api = host();
    const store = new SessionExpertStore(api);
    await store.load("run-a");
    api.setSessionExpert.mockRejectedValue(new Error("dependency missing"));
    await expect(store.select(null)).rejects.toThrow("dependency missing");
    expect(store.selected?.id).toBe("writer");
    expect(store.changing).toBe(false);
  });
  it("ignores late hydration after switching conversations", async () => {
    const api = host();
    let resolve!: (value: typeof expert) => void;
    api.getSessionExpert.mockImplementationOnce(
      () =>
        new Promise((r) => {
          resolve = r;
        }),
    );
    const store = new SessionExpertStore(api);
    const previous = store.load("run-a");
    api.getSessionExpert.mockResolvedValue(null);
    await store.load("run-b");
    resolve(expert);
    await previous;
    expect(store.selected).toBeNull();
  });
});
