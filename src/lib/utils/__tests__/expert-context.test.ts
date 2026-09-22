import { describe, expect, it } from "vitest";
import { dshExpertSkillName, isExpertSkill, parseExpertFromText } from "../expert-context";

describe("expert context", () => {
  it("maps plugin ids to valid DSH skill names", () => {
    expect(dshExpertSkillName("market.team--cn")).toBe("agentcabin-expert-market-team-cn");
    expect(dshExpertSkillName("  ")).toBe("agentcabin-expert");
  });

  it("recognizes namespaced expert skills without exposing them as generic skills", () => {
    expect(isExpertSkill({ id: "agent-plugin--market-team--expert--market-team" })).toBe(true);
    expect(isExpertSkill({ name: "research" })).toBe(false);
  });

  it("parses the selected expert marker", () => {
    const parsed = parseExpertFromText("[当前协作专家: 市场专家团 (专家团队)]\n研究竞品");
    expect(parsed.expert?.isTeam).toBe(true);
    expect(parsed.cleanText).toBe("研究竞品");
  });
});
