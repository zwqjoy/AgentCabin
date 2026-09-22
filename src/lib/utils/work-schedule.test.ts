import { describe, expect, it } from "vitest";
import type { WorkScheduleConfig } from "$lib/types/work";
import { buildWorkSchedule, formatWorkSchedule, parseWorkSchedule } from "$lib/utils/work-schedule";

describe("work schedule helpers", () => {
  it("returns no schedule when scheduling is disabled", () => {
    expect(buildWorkSchedule(false, "daily", "09:00", [1], "", "Asia/Shanghai")).toBeNull();
    expect(formatWorkSchedule(null)).toBe("未启用");
  });

  it("builds daily and weekday cron schedules with their timezone", () => {
    expect(buildWorkSchedule(true, "daily", "09:30", [], "", "Asia/Shanghai")).toEqual({
      enabled: true,
      kind: "cron",
      timezone: "Asia/Shanghai",
      cronExpression: "30 9 * * *",
      runOnStartup: false,
      maxRuns: null,
    });
    expect(buildWorkSchedule(true, "weekdays", "17:05", [], "", "")).toMatchObject({
      timezone: "UTC",
      cronExpression: "5 17 * * 1-5",
    });
  });

  it("sorts custom weekdays and converts Sunday to cron day zero", () => {
    const schedule = buildWorkSchedule(true, "custom_weekly", "08:00", [7, 3, 1], "", "UTC");

    expect(schedule?.cronExpression).toBe("0 8 * * 1,3,0");
    expect(formatWorkSchedule(schedule)).toBe("每周 (周一、周三、周日) 08:00");
  });

  it("uses a safe default for an empty custom cron expression", () => {
    expect(buildWorkSchedule(true, "custom_cron", "12:00", [], "  ", "UTC")?.cronExpression).toBe(
      "0 9 * * *",
    );
  });

  it("parses editor-supported cron forms back into schedule fields", () => {
    expect(parseWorkSchedule({ enabled: true, cronExpression: "5 7 * * *" })).toEqual({
      enabled: true,
      kind: "daily",
      time: "07:05",
      days: [1],
      customCron: "5 7 * * *",
    });
    expect(parseWorkSchedule({ enabled: true, cronExpression: "30 17 * * 1-5" })).toMatchObject({
      kind: "weekdays",
      time: "17:30",
      days: [1, 2, 3, 4, 5],
    });
    expect(parseWorkSchedule({ enabled: true, cronExpression: "0 8 * * 0,2,7" })).toMatchObject({
      kind: "custom_weekly",
      days: [7, 2, 7],
    });
  });

  it("keeps unsupported or legacy schedule values as custom cron", () => {
    const legacySchedule: WorkScheduleConfig = {
      enabled: true,
      kind: "once",
      fireAt: "2026-08-29T09:00:00Z",
      cronExpression: "0 0 1 * *",
    };

    expect(parseWorkSchedule(legacySchedule)).toEqual({
      enabled: true,
      kind: "custom_cron",
      time: "09:00",
      days: [1],
      customCron: "0 0 1 * *",
    });
    expect(formatWorkSchedule({ enabled: true, cronExpression: "0 0 1 * *" })).toBe(
      "Cron: 0 0 1 * *",
    );
  });
});
