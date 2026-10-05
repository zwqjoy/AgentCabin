import assert from "node:assert/strict";
import test from "node:test";
import { applyExpertContext } from "./pi_expert_extension.mjs";

test("the current role replaces stale skill catalogs and preloads its configured skill", () => {
  const prompt = applyExpertContext("base\n<available_skills>OLD_WRITER_SKILL</available_skills>", {
    expert: { id: "reviewer" }, systemPrompt: "REVIEWER_ROLE\nPRELOADED_REVIEW_SKILL",
    skills: [{ id: "review", name: "review", path: "/skills/review", description: "reviews <documents>" }],
  });
  assert.ok(prompt.includes("REVIEWER_ROLE"));
  assert.ok(prompt.includes("PRELOADED_REVIEW_SKILL"));
  assert.ok(prompt.includes("/skills/review/SKILL.md"));
  assert.ok(prompt.includes("&lt;documents&gt;"));
  assert.ok(!prompt.includes("OLD_WRITER_SKILL"));
});

test("exit removes expert skills and directs later turns to stop following the prior role", () => {
  const prompt = applyExpertContext("base\n<available_skills>EXPERT_SKILL</available_skills>", { expert: null, systemPrompt: null, skills: [] });
  assert.ok(!prompt.includes("EXPERT_SKILL"));
  assert.ok(prompt.includes("当前未选择专家"));
  assert.ok(prompt.includes("停止遵循历史轮次"));
});
