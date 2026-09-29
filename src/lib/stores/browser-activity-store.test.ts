import { beforeEach, describe, expect, it, vi } from "vitest";

const eventState = vi.hoisted(() => ({
  handler: null as ((event: unknown) => void) | null,
}));

vi.mock("$lib/api", () => ({
  getBusEvents: vi.fn(),
  readFileBase64: vi.fn(),
}));

vi.mock("$lib/stores/event-middleware", () => ({
  getEventMiddleware: () => ({
    subscribeEvents: (handler: (event: unknown) => void) => {
      eventState.handler = handler;
      return () => {
        if (eventState.handler === handler) eventState.handler = null;
      };
    },
  }),
}));

import * as api from "$lib/api";
import type { BusEvent } from "$lib/types";
import { BrowserActivityStore, projectAgentBrowserActivity } from "./browser-activity-store.svelte";

describe("browser-activity-store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    eventState.handler = null;
  });

  describe("projectAgentBrowserActivity", () => {
    it("projects tool_start for open navigation", () => {
      const event: BusEvent = {
        type: "tool_start",
        run_id: "run-test-1",
        tool_use_id: "tu-1",
        tool_name: "agent_browser",
        input: { args: ["open", "https://example.com"] },
      };

      const activity = projectAgentBrowserActivity(null, event);

      expect(activity.runId).toBe("run-test-1");
      expect(activity.status).toBe("running");
      expect(activity.currentUrl).toBe("https://example.com");
      expect(activity.currentAction).toContain("打开 https://example.com");
      expect(activity.traces).toHaveLength(1);
      expect(activity.traces[0].actionType).toBe("navigate");
      expect(activity.traces[0].status).toBe("started");
      expect(activity.traces[0].targetUrl).toBe("https://example.com");
    });

    it("projects tool_end with completed/success status, title, url and screenshot", () => {
      const startEv: BusEvent = {
        type: "tool_start",
        run_id: "run-test-1",
        tool_use_id: "tu-1",
        tool_name: "agent_browser",
        input: { args: ["open", "https://example.com"] },
      };
      const afterStart = projectAgentBrowserActivity(null, startEv);

      const endEv: BusEvent = {
        type: "tool_end",
        run_id: "run-test-1",
        tool_use_id: "tu-1",
        tool_name: "agent_browser",
        status: "completed",
        duration_ms: 350,
        output: {
          content: [
            {
              type: "image",
              data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
              mimeType: "image/png",
            },
          ],
          details: {
            url: "https://example.com/welcome",
            title: "Example Domain",
          },
        },
      };

      const afterEnd = projectAgentBrowserActivity(afterStart, endEv);

      expect(afterEnd.status).toBe("idle");
      expect(afterEnd.currentAction).toBeNull();
      expect(afterEnd.currentUrl).toBe("https://example.com/welcome");
      expect(afterEnd.pageTitle).toBe("Example Domain");
      expect(afterEnd.lastScreenshot).toContain("data:image/png;base64,");
      expect(afterEnd.traces[0].status).toBe("success");
      expect(afterEnd.traces[0].durationMs).toBe(350);
      expect(afterEnd.traces[0].screenshotData).toContain("data:image/png;base64,");
    });

    it("projects tool_start for semantic actions", () => {
      const event: BusEvent = {
        type: "tool_start",
        run_id: "run-test-1",
        tool_use_id: "tu-2",
        tool_name: "agent_browser",
        input: {
          semanticAction: {
            action: "fill",
            selector: "@e1",
            text: "test input",
          },
        },
      };

      const activity = projectAgentBrowserActivity(null, event);

      expect(activity.status).toBe("running");
      expect(activity.traces).toHaveLength(1);
      expect(activity.traces[0].actionType).toBe("type");
      expect(activity.traces[0].description).toContain("test input");
    });

    it("projects agent_browser_action direct payload", () => {
      const event: BusEvent = {
        type: "tool_start",
        run_id: "run-test-1",
        tool_use_id: "tu-action-1",
        tool_name: "agent_browser_action",
        input: {
          action: "click",
          selector: "#submit-btn",
        },
      };

      const activity = projectAgentBrowserActivity(null, event);
      expect(activity.status).toBe("running");
      expect(activity.traces).toHaveLength(1);
      expect(activity.traces[0].actionType).toBe("click");
      expect(activity.traces[0].selector).toBe("#submit-btn");
      expect(activity.traces[0].description).toBe("点击: #submit-btn");
    });

    it("projects agent_browser_code payload", () => {
      const event: BusEvent = {
        type: "tool_start",
        run_id: "run-test-1",
        tool_use_id: "tu-code-1",
        tool_name: "agent_browser_code",
        input: {
          code: "await browser({ args: ['snapshot'] });",
        },
      };

      const activity = projectAgentBrowserActivity(null, event);
      expect(activity.status).toBe("running");
      expect(activity.traces[0].actionType).toBe("custom");
      expect(activity.traces[0].description).toBe("执行浏览器自动化脚本");
    });

    it("projects agent_browser_qa payload", () => {
      const event: BusEvent = {
        type: "tool_start",
        run_id: "run-test-1",
        tool_use_id: "tu-qa-1",
        tool_name: "agent_browser_qa",
        input: {
          url: "https://example.com/checkout",
        },
      };

      const activity = projectAgentBrowserActivity(null, event);
      expect(activity.status).toBe("running");
      expect(activity.traces[0].actionType).toBe("navigate");
      expect(activity.traces[0].targetUrl).toBe("https://example.com/checkout");
      expect(activity.traces[0].description).toBe("QA 页面校验: https://example.com/checkout");
    });

    it("projects agent_browser_electron and agent_browser_tools payloads", () => {
      const elEv: BusEvent = {
        type: "tool_start",
        run_id: "run-test-1",
        tool_use_id: "tu-el-1",
        tool_name: "agent_browser_electron",
        input: {
          action: "probe",
        },
      };
      const elActivity = projectAgentBrowserActivity(null, elEv);
      expect(elActivity.traces[0].description).toBe("Electron: probe");

      const toolsEv: BusEvent = {
        type: "tool_start",
        run_id: "run-test-1",
        tool_use_id: "tu-tools-1",
        tool_name: "agent_browser_tools",
        input: {
          enable: ["action", "qa"],
        },
      };
      const toolsActivity = projectAgentBrowserActivity(null, toolsEv);
      expect(toolsActivity.traces[0].description).toBe("启用浏览器工具: action, qa");
    });

    it("projects tool_end with pageChangeSummary and sessionTabTarget", () => {
      const startEv: BusEvent = {
        type: "tool_start",
        run_id: "run-test-1",
        tool_use_id: "tu-sum-1",
        tool_name: "agent_browser",
        input: { args: ["click", "@e5"] },
      };
      const afterStart = projectAgentBrowserActivity(null, startEv);

      const endEv: BusEvent = {
        type: "tool_end",
        run_id: "run-test-1",
        tool_use_id: "tu-sum-1",
        tool_name: "agent_browser",
        status: "completed",
        output: {
          details: {
            pageChangeSummary: {
              url: "https://github.com/trending",
              title: "Trending Repositories",
            },
            imagePath: "/tmp/agent-browser-shots/trending.png",
          },
        },
      };

      const afterEnd = projectAgentBrowserActivity(afterStart, endEv);
      expect(afterEnd.currentUrl).toBe("https://github.com/trending");
      expect(afterEnd.pageTitle).toBe("Trending Repositories");
      expect(afterEnd.lastScreenshot).toBe("/tmp/agent-browser-shots/trending.png");
    });

    it("projects tool_end on failure with 'failed' status and error details", () => {
      const startEv: BusEvent = {
        type: "tool_start",
        run_id: "run-test-1",
        tool_use_id: "tu-3",
        tool_name: "agent_browser",
        input: { args: ["click", "@e999"] },
      };
      const afterStart = projectAgentBrowserActivity(null, startEv);

      const endEv: BusEvent = {
        type: "tool_end",
        run_id: "run-test-1",
        tool_use_id: "tu-3",
        tool_name: "agent_browser",
        status: "failed",
        duration_ms: 120,
        output: {
          details: {
            error: "Ref @e999 not found",
            failureCategory: "selector-not-found",
            succeeded: false,
          },
        },
      };

      const afterEnd = projectAgentBrowserActivity(afterStart, endEv);

      expect(afterEnd.status).toBe("failed");
      expect(afterEnd.lastError).toBe("Ref @e999 not found");
      expect(afterEnd.traces[0].status).toBe("failed");
      expect(afterEnd.traces[0].error).toBe("Ref @e999 not found");
    });

    it("projects run_state events", () => {
      const base = projectAgentBrowserActivity(null, {
        type: "tool_start",
        run_id: "run-test-1",
        tool_use_id: "tu-1",
        tool_name: "agent_browser",
        input: { args: ["open", "https://example.com"] },
      });

      const completed = projectAgentBrowserActivity(base, {
        type: "run_state",
        run_id: "run-test-1",
        state: "completed",
      });
      expect(completed.status).toBe("completed");

      const successState = projectAgentBrowserActivity(base, {
        type: "run_state",
        run_id: "run-test-1",
        state: "success",
      });
      expect(successState.status).toBe("completed");

      const failed = projectAgentBrowserActivity(base, {
        type: "run_state",
        run_id: "run-test-1",
        state: "failed",
        error: "Fatal execution crash",
      });
      expect(failed.status).toBe("failed");
      expect(failed.lastError).toBe("Fatal execution crash");
    });
  });

  describe("BrowserActivityStore instance", () => {
    it("subscribes to live events via eventMiddleware and resolves file screenshots with run cwd", async () => {
      vi.mocked(api.readFileBase64).mockResolvedValueOnce(["samplebase64", "image/png"]);
      const store = new BrowserActivityStore();
      expect(eventState.handler).toBeDefined();

      eventState.handler?.({
        type: "session_init",
        run_id: "run-sub-1",
        cwd: "/Users/cengwenqi/workspace",
        tools: [],
      } as unknown as BusEvent);

      eventState.handler?.({
        type: "tool_start",
        run_id: "run-sub-1",
        tool_use_id: "tu-1",
        tool_name: "agent_browser",
        input: { args: ["open", "https://news.ycombinator.com"] },
      } as BusEvent);

      const activity = store.getActivity("run-sub-1");
      expect(activity).not.toBeNull();
      expect(activity?.currentUrl).toBe("https://news.ycombinator.com");
      expect(activity?.status).toBe("running");

      eventState.handler?.({
        type: "tool_end",
        run_id: "run-sub-1",
        tool_use_id: "tu-1",
        tool_name: "agent_browser",
        status: "completed",
        output: {
          details: {
            imagePath: "/tmp/screenshots/hn.png",
          },
        },
      } as BusEvent);

      await vi.waitFor(() => {
        const updated = store.getActivity("run-sub-1");
        expect(updated?.lastScreenshot).toBe("data:image/png;base64,samplebase64");
      });

      expect(api.readFileBase64).toHaveBeenCalledWith(
        "/tmp/screenshots/hn.png",
        "/Users/cengwenqi/workspace",
      );

      store.destroy();
    });

    it("prefers artifact absolutePath and artifact.cwd when resolving screenshots", async () => {
      vi.mocked(api.readFileBase64).mockResolvedValueOnce(["artifactbase64", "image/png"]);
      const store = new BrowserActivityStore();

      eventState.handler?.({
        type: "tool_start",
        run_id: "run-art-1",
        tool_use_id: "tu-art",
        tool_name: "agent_browser",
        input: { args: ["screenshot"] },
      } as BusEvent);

      eventState.handler?.({
        type: "tool_end",
        run_id: "run-art-1",
        tool_use_id: "tu-art",
        tool_name: "agent_browser",
        status: "completed",
        output: {
          details: {
            artifacts: [
              {
                kind: "screenshot",
                path: "relative.png",
                absolutePath: "/custom/artifacts/screen.png",
                cwd: "/custom/artifacts",
              },
            ],
          },
        },
      } as BusEvent);

      await vi.waitFor(() => {
        const updated = store.getActivity("run-art-1");
        expect(updated?.lastScreenshot).toBe("data:image/png;base64,artifactbase64");
      });

      expect(api.readFileBase64).toHaveBeenCalledWith(
        "/custom/artifacts/screen.png",
        "/custom/artifacts",
      );

      store.destroy();
    });

    it("does not call readFileBase64 with empty cwd if run cwd is missing", async () => {
      vi.mocked(api.getBusEvents).mockResolvedValueOnce([]);
      const store = new BrowserActivityStore();

      eventState.handler?.({
        type: "tool_end",
        run_id: "run-no-cwd",
        tool_use_id: "tu-none",
        tool_name: "agent_browser",
        status: "completed",
        output: {
          details: {
            imagePath: "/tmp/screenshots/nocwd.png",
          },
        },
      } as BusEvent);

      await new Promise((r) => setTimeout(r, 50));
      expect(api.readFileBase64).not.toHaveBeenCalledWith("/tmp/screenshots/nocwd.png", "");

      store.destroy();
    });

    it("loads historical events from api.getBusEvents", async () => {
      const mockEvents: BusEvent[] = [
        {
          type: "tool_start",
          run_id: "run-hist-1",
          tool_use_id: "tu-10",
          tool_name: "agent_browser",
          input: { args: ["open", "https://example.com"] },
        },
        {
          type: "tool_end",
          run_id: "run-hist-1",
          tool_use_id: "tu-10",
          tool_name: "agent_browser",
          status: "completed",
          duration_ms: 200,
          output: {
            details: {
              url: "https://example.com",
              title: "Example Title",
            },
          },
        },
      ];

      vi.mocked(api.getBusEvents).mockResolvedValueOnce(mockEvents);

      const store = new BrowserActivityStore();
      const activity = await store.loadActivity("run-hist-1");

      expect(activity).not.toBeNull();
      expect(activity?.currentUrl).toBe("https://example.com");
      expect(activity?.pageTitle).toBe("Example Title");
      expect(activity?.traces).toHaveLength(1);
      expect(activity?.traces[0].status).toBe("success");

      store.destroy();
    });
  });
});
