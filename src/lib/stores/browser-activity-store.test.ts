import { beforeEach, describe, expect, it, vi } from "vitest";

const eventState = vi.hoisted(() => ({
  handler: null as ((event: unknown) => void) | null,
}));

vi.mock("$lib/api", () => ({
  getBusEvents: vi.fn(),
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

    it("projects tool_end with title, url and screenshot", () => {
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
        status: "success",
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

    it("projects tool_end on failure with error details", () => {
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
        status: "error",
        duration_ms: 120,
        output: {
          details: {
            error: "Ref @e999 not found",
            failureCategory: "selector-not-found",
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
    it("subscribes to live events via eventMiddleware", () => {
      const store = new BrowserActivityStore();
      expect(eventState.handler).toBeDefined();

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
          status: "success",
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
