import { PI_THINKING_LEVELS } from "$lib/utils/pi-provider-presets";

/** Pi's supported thinking levels, shared by settings and chat controls. */
export const PI_THINKING_LEVEL_OPTIONS = PI_THINKING_LEVELS.map((value) => ({
  value,
  label:
    value === "off"
      ? "关闭"
      : value === "minimal"
        ? "极少"
        : value === "low"
          ? "低"
          : value === "medium"
            ? "中"
            : value === "high"
              ? "高"
              : "极高",
})) as ReadonlyArray<{ value: (typeof PI_THINKING_LEVELS)[number]; label: string }>;

/** Remove legacy aliases that Pi may report but that should not be separate UI choices. */
export function filterPiThinkingLevelsForUi(levels: readonly string[]): string[] {
  return levels.filter((level) => level !== "none" && level !== "minimal");
}

export type PiThinkingLevel = (typeof PI_THINKING_LEVEL_OPTIONS)[number]["value"];
