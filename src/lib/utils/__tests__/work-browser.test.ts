import { describe, expect, it } from "vitest";
import {
  filterBrowserTraces,
  formatBrowserAction,
  formatBrowserStatus,
  isBrowserToolName,
  sanitizeTraceText,
} from "../work-browser";
import type { BrowserTraceEntry } from "$lib/types/work";

describe("work-browser utility", () => {
  it("formats action labels properly", () => {
    expect(formatBrowserAction("navigate").label).toBe("页面导航");
    expect(formatBrowserAction("click").label).toBe("点击元素");
    expect(formatBrowserAction("type").label).toBe("文本输入");
    expect(formatBrowserAction("screenshot").label).toBe("页面截屏");
    expect(formatBrowserAction("takeover").label).toBe("用户人工接管");
  });

  it("formats status styles properly", () => {
    expect(formatBrowserStatus("running").label).toBe("执行中");
    expect(formatBrowserStatus("taking_over").label).toBe("人工接管中");
    expect(formatBrowserStatus("paused").label).toBe("已暂停");
    expect(formatBrowserStatus("waiting_approval").label).toBe("等待审批");
  });

  it("recognizes browser and web runtime tools", () => {
    expect(isBrowserToolName("web_search")).toBe(true);
    expect(isBrowserToolName("work_browser_navigate")).toBe(true);
    expect(isBrowserToolName("work_read_file")).toBe(false);
    expect(isBrowserToolName(null)).toBe(false);
  });

  it("sanitizes passwords and secrets correctly", () => {
    const raw =
      'Login with password="mySecretPassword123" and Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.xyz';
    const sanitized = sanitizeTraceText(raw);
    expect(sanitized).not.toContain("mySecretPassword123");
    expect(sanitized).toContain('password="******"');
    expect(sanitized).toContain("Bearer [REDACTED]");
  });

  it("filters traces by keyword", () => {
    const traces: BrowserTraceEntry[] = [
      {
        stepIndex: 1,
        actionType: "navigate",
        description: "导航至 https://github.com/login",
        targetUrl: "https://github.com/login",
        selector: null,
        status: "success",
        screenshotData: null,
        durationMs: 350,
        error: null,
        timestamp: "2026-08-30T00:00:00Z",
      },
      {
        stepIndex: 2,
        actionType: "click",
        description: "点击登录按钮",
        targetUrl: null,
        selector: "button[type='submit']",
        status: "success",
        screenshotData: null,
        durationMs: 40,
        error: null,
        timestamp: "2026-08-30T00:00:01Z",
      },
    ];

    expect(filterBrowserTraces(traces, "github").length).toBe(1);
    expect(filterBrowserTraces(traces, "submit").length).toBe(1);
    expect(filterBrowserTraces(traces, "missing").length).toBe(0);
  });
});
