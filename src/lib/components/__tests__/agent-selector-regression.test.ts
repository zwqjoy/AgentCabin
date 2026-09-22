import { describe, it, expect, vi } from "vitest";
import { render } from "svelte/server";
import AgentSelector from "../AgentSelector.svelte";
import PromptInput from "../PromptInput.svelte";
import { getAgentCapabilities } from "$lib/utils/agent-capabilities";

// Mock Tauri/browser API
vi.mock("$app/navigation", () => ({
  goto: vi.fn(),
  replaceState: vi.fn(),
}));

vi.mock("$lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/api")>();
  return {
    ...actual,
    getUserSettings: vi.fn().mockResolvedValue({}),
    getGitBranch: vi.fn().mockResolvedValue({ current_branch: "main", branches: [] }),
    getGitBranches: vi.fn().mockResolvedValue([]),
    listDirectory: vi.fn().mockResolvedValue([]),
  };
});

describe("AgentSelector Regression Test", () => {
  it("renders compact mode with dot and label but without prefix '运行时:'", () => {
    const result = render(AgentSelector, {
      props: {
        value: "pi",
        enabledAgents: ["pi", "dsh"],
        locked: false,
        compact: true,
      },
    });

    expect(result.body).toContain("Pi Agent");
    expect(result.body).not.toContain("运行时:");
    // Should have dropdown chevron when not locked and agents > 1
    expect(result.body).toContain("<svg");
  });

  it("renders non-compact mode with prefix '运行时:'", () => {
    const result = render(AgentSelector, {
      props: {
        value: "pi",
        enabledAgents: ["pi", "dsh"],
        locked: false,
        compact: false,
      },
    });

    expect(result.body).toContain("Pi Agent");
    expect(result.body).toContain("运行时:");
  });

  it("renders locked state without chevron and with lock title", () => {
    const result = render(AgentSelector, {
      props: {
        value: "pi",
        enabledAgents: ["pi", "dsh"],
        locked: true,
        compact: true,
      },
    });

    expect(result.body).toContain("Pi Agent");
    expect(result.body).toContain("当前会话已绑定 Pi Agent，新建会话后可切换运行时");
    expect(result.body).toContain("cursor-default");
    // Locked selector must NOT render the chevron svg
    expect(result.body).not.toContain("<svg");
  });

  it("renders AgentSelector inside PromptInput when unlocked", () => {
    const onAgentChange = vi.fn();
    const result = render(PromptInput, {
      props: {
        agent: "pi",
        enabledAgents: ["pi", "dsh"],
        capabilities: getAgentCapabilities("pi"),
        hasRun: false,
        sessionAlive: false,
        running: false,
        onAgentChange,
        onSend: vi.fn(),
      },
    });

    expect(result.body).toContain("data-agent-selector");
    expect(result.body).toContain("Pi Agent");
  });

  it("renders AgentSelector locked inside PromptInput when hasRun is true", () => {
    const onAgentChange = vi.fn();
    const result = render(PromptInput, {
      props: {
        agent: "pi",
        enabledAgents: ["pi", "dsh"],
        capabilities: getAgentCapabilities("pi"),
        hasRun: true,
        sessionAlive: false,
        running: false,
        onAgentChange,
        onSend: vi.fn(),
      },
    });

    expect(result.body).toContain("data-agent-selector");
    expect(result.body).toContain("Pi Agent");
    expect(result.body).toContain("当前会话已绑定 Pi Agent，新建会话后可切换运行时");
  });

  it("renders AgentSelector locked inside PromptInput when running is true", () => {
    const onAgentChange = vi.fn();
    const result = render(PromptInput, {
      props: {
        agent: "pi",
        enabledAgents: ["pi", "dsh"],
        capabilities: getAgentCapabilities("pi"),
        hasRun: false,
        sessionAlive: false,
        running: true,
        onAgentChange,
        onSend: vi.fn(),
      },
    });

    expect(result.body).toContain("data-agent-selector");
    expect(result.body).toContain("当前会话已绑定 Pi Agent，新建会话后可切换运行时");
  });

  it("renders AgentSelector locked inside PromptInput when sessionAlive is true", () => {
    const onAgentChange = vi.fn();
    const result = render(PromptInput, {
      props: {
        agent: "pi",
        enabledAgents: ["pi", "dsh"],
        capabilities: getAgentCapabilities("pi"),
        hasRun: false,
        sessionAlive: true,
        running: false,
        onAgentChange,
        onSend: vi.fn(),
      },
    });

    expect(result.body).toContain("data-agent-selector");
    expect(result.body).toContain("当前会话已绑定 Pi Agent，新建会话后可切换运行时");
  });

  it("renders AgentSelector locked when onAgentChange is not provided", () => {
    const result = render(PromptInput, {
      props: {
        agent: "pi",
        enabledAgents: ["pi", "dsh"],
        capabilities: getAgentCapabilities("pi"),
        hasRun: false,
        sessionAlive: false,
        running: false,
        onSend: vi.fn(),
      },
    });

    expect(result.body).toContain("data-agent-selector");
    expect(result.body).toContain("当前会话已绑定 Pi Agent，新建会话后可切换运行时");
  });

  it("renders DeepSeek DSH correctly when selected", () => {
    const result = render(AgentSelector, {
      props: {
        value: "dsh",
        enabledAgents: ["pi", "dsh"],
        locked: false,
        compact: true,
      },
    });

    expect(result.body).toContain("DeepSeek Harness (DSH)");
    expect(result.body).not.toContain("运行时:");
  });

  describe("Lock semantics and rules", () => {
    it("evaluates locked to true when session is active or running", () => {
      // PromptInput uses: locked={hasRun || sessionAlive || running || !onAgentChange}
      const computeLocked = (
        hasRun: boolean,
        sessionAlive: boolean,
        running: boolean,
        hasHandler: boolean,
      ) => hasRun || sessionAlive || running || !hasHandler;

      expect(computeLocked(false, false, false, true)).toBe(false);
      expect(computeLocked(true, false, false, true)).toBe(true);
      expect(computeLocked(false, true, false, true)).toBe(true);
      expect(computeLocked(false, false, true, true)).toBe(true);
      expect(computeLocked(false, false, false, false)).toBe(true);
    });
  });
});
