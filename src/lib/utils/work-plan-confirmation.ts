export type WorkPlanDecision = "approve" | "revise" | "cancel";

/** Detect the plain-text confirmation format emitted by older or tool-light Pi runs. */
export function detectWorkPlanConfirmation(content: string): boolean {
  const text = String(content || "");
  return (
    /(?:请确认|是否同意|确认.*计划)/s.test(text) &&
    text.includes("确认执行") &&
    text.includes("修改计划") &&
    text.includes("取消")
  );
}

/** Parse the compact replies users commonly use for a numbered plan prompt. */
export function parseWorkPlanDecision(input: string): WorkPlanDecision | null {
  const text = String(input || "")
    .trim()
    .toLocaleLowerCase();
  if (/^(?:1[.。]?|确认执行|同意执行?|继续|yes|y)$/.test(text)) return "approve";
  if (/^(?:2[.。]?|修改计划|修改|调整方案?)$/.test(text)) return "revise";
  if (/^(?:3[.。]?|取消|放弃|不用了)$/.test(text)) return "cancel";
  return null;
}
