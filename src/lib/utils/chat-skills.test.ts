import { describe, expect, it } from "vitest";
import type { StandaloneSkill } from "$lib/types";
import {
  getEnabledCapabilitySkills,
  toCapabilitySkillItems,
  toCodexCapabilitySkillItems,
} from "./chat-skills";

function skill(overrides: Partial<StandaloneSkill> = {}): StandaloneSkill {
  return {
    name: "docs",
    description: "Document helpers",
    path: "/Users/test/.agentcabin/skills/docs/SKILL.md",
    agent: "pi",
    enabled: true,
    ...overrides,
  };
}

describe("chat capability skills", () => {
  it("keeps only skills enabled by the global capability binding", () => {
    const skills = [skill(), skill({ name: "disabled", enabled: false })];

    expect(getEnabledCapabilitySkills(skills).map((item) => item.name)).toEqual(["docs"]);
    expect(toCapabilitySkillItems(skills)).toEqual([
      { name: "docs", description: "Document helpers" },
    ]);
  });

  it("keeps paths for Codex structured skill inputs and drops unusable entries", () => {
    const skills = [
      skill(),
      skill({ name: "disabled", enabled: false }),
      skill({ name: "missing-path", path: "" }),
    ];

    expect(toCodexCapabilitySkillItems(skills)).toEqual([
      {
        name: "docs",
        path: "/Users/test/.agentcabin/skills/docs/SKILL.md",
        description: "Document helpers",
      },
    ]);
  });
});
