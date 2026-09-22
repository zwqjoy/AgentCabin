import type { WorkScheduleConfig } from "$lib/types/work";

export type WorkScheduleEditorKind = "daily" | "weekdays" | "custom_weekly" | "custom_cron";

export interface ParsedWorkSchedule {
  enabled: boolean;
  kind: WorkScheduleEditorKind;
  time: string;
  days: number[];
  customCron: string;
}

export const WORK_WEEKDAY_OPTIONS = [
  { label: "周一", value: 1 },
  { label: "周二", value: 2 },
  { label: "周三", value: 3 },
  { label: "周四", value: 4 },
  { label: "周五", value: 5 },
  { label: "周六", value: 6 },
  { label: "周日", value: 7 },
] as const;

export function buildWorkSchedule(
  enabled: boolean,
  kind: WorkScheduleEditorKind,
  time: string,
  days: number[],
  customCron: string,
  timezone: string,
): WorkScheduleConfig | null {
  if (!enabled) return null;
  return {
    enabled: true,
    kind: "cron",
    timezone: timezone || "UTC",
    cronExpression: buildCronExpression(kind, time, days, customCron),
    runOnStartup: false,
    maxRuns: null,
  };
}

function buildCronExpression(
  kind: WorkScheduleEditorKind,
  time: string,
  days: number[],
  customCron: string,
): string {
  if (kind === "custom_cron") {
    return customCron.trim() || "0 9 * * *";
  }
  const [h, m] = time.split(":").map(Number);
  const hh = h ?? 9;
  const mm = m ?? 0;
  if (kind === "weekdays") return `${mm} ${hh} * * 1-5`;
  if (kind === "custom_weekly") {
    const sorted = [...days].sort((a, b) => a - b);
    const daysStr = sorted.length > 0 ? sorted.map((day) => (day === 7 ? 0 : day)).join(",") : "1";
    return `${mm} ${hh} * * ${daysStr}`;
  }
  return `${mm} ${hh} * * *`;
}

export function parseWorkSchedule(schedule: WorkScheduleConfig | null): ParsedWorkSchedule {
  if (!schedule || !schedule.enabled) {
    return { enabled: false, kind: "daily", time: "09:00", days: [1], customCron: "" };
  }
  const cron = (schedule.cronExpression || "").trim();
  const parts = cron.split(/\s+/);
  if (parts.length === 5) {
    const m = (parts[0] ?? "0").padStart(2, "0");
    const h = (parts[1] ?? "9").padStart(2, "0");
    const dom = parts[2];
    const month = parts[3];
    const dow = parts[4];
    if (dom === "*" && month === "*") {
      if (dow === "*") {
        return {
          enabled: true,
          kind: "daily",
          time: `${h}:${m}`,
          days: [1],
          customCron: cron,
        };
      }
      if (dow === "1-5" || dow === "1,2,3,4,5") {
        return {
          enabled: true,
          kind: "weekdays",
          time: `${h}:${m}`,
          days: [1, 2, 3, 4, 5],
          customCron: cron,
        };
      }
      const dowItems = dow
        .split(",")
        .map(Number)
        .filter((n) => !isNaN(n) && n >= 0 && n <= 7)
        .map((n) => (n === 0 ? 7 : n));
      if (dowItems.length > 0 && dow.split(",").every((s) => /^[0-7]$/.test(s.trim()))) {
        return {
          enabled: true,
          kind: "custom_weekly",
          time: `${h}:${m}`,
          days: dowItems,
          customCron: cron,
        };
      }
    }
  }
  return {
    enabled: true,
    kind: "custom_cron",
    time: "09:00",
    days: [1],
    customCron: cron,
  };
}

export function formatWorkSchedule(schedule: WorkScheduleConfig | null): string {
  if (!schedule || !schedule.enabled) return "未启用";
  const parsed = parseWorkSchedule(schedule);
  if (parsed.kind === "daily") return `每天 ${parsed.time}`;
  if (parsed.kind === "weekdays") return `工作日 (周一至周五) ${parsed.time}`;
  if (parsed.kind === "custom_weekly") {
    const dayNames = parsed.days
      .map(
        (day) => WORK_WEEKDAY_OPTIONS.find((option) => option.value === day)?.label || `周${day}`,
      )
      .join("、");
    return `每周 (${dayNames}) ${parsed.time}`;
  }
  return `Cron: ${parsed.customCron}`;
}
