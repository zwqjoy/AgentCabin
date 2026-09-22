import assert from "node:assert/strict";
import test from "node:test";

import { buildContextUsageSnapshot } from "./pi_context_usage_extension.mjs";

test("classifies Pi context into prompt, tools, messages, and free space", () => {
  const snapshot = buildContextUsageSnapshot({
    systemPrompt: [
      "base system prompt",
      "<project_context>workspace instructions</project_context>",
      "The following skills provide specialized instructions\nskill text\n</available_skills>",
      "\nAvailable tools:\n\n",
    ].join("\n"),
    messages: [
      { role: "user", content: "inspect the repository" },
      { role: "assistant", content: [{ type: "thinking", thinking: "plan" }, { type: "text", text: "I will inspect it" }, { type: "toolCall", name: "read", arguments: {} }] },
      { role: "toolResult", content: [{ type: "text", text: "file contents" }] },
      { role: "custom", content: "extension state" },
    ],
    allTools: [
      { name: "read", description: "Read a file", parameters: {} },
      { name: "mcp_search", description: "Search MCP", parameters: {}, source: "mcp" },
      { name: "custom_tool", description: "Custom tool", parameters: {} },
    ],
    activeToolNames: ["read", "mcp_search", "custom_tool"],
    context: {
      getContextUsage: () => ({ tokens: 100, contextWindow: 200, percent: 50 }),
    },
  });

  assert.equal(snapshot.version, 1);
  assert.equal(snapshot.usedTokens, 100);
  assert.equal(snapshot.contextWindow, 200);
  assert.equal(snapshot.reportedAggregate, true);
  assert.equal(snapshot.estimated, true);
  assert.deepEqual(
    snapshot.categories.map(({ id }) => id),
    [
      "system_prompt",
      "instructions",
      "skills",
      "tool_prompt",
      "system_tools",
      "custom_tools",
      "mcp_tools",
      "user_messages",
      "assistant_text",
      "assistant_thinking",
      "tool_calls",
      "tool_results",
      "extension_messages",
      "free_space",
    ],
  );
  assert.equal(
    snapshot.categories.reduce((sum, category) => sum + category.tokens, 0),
    snapshot.contextWindow,
  );
});

test("does not fail when Pi has no aggregate usage yet", () => {
  const snapshot = buildContextUsageSnapshot({
    systemPrompt: "system",
    messages: [{ role: "user", content: "hello" }],
    allTools: [],
    activeToolNames: [],
    context: { getContextUsage: () => ({}) },
  });

  assert.equal(snapshot.reportedAggregate, false);
  assert.equal(snapshot.contextWindow, 0);
  assert.ok(snapshot.usedTokens > 0);
  assert.ok(snapshot.categories.every((category) => category.tokens > 0));
});
