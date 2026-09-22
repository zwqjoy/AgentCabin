// AgentCabin DSH plugin for conversation-native HTML previews.

export const name = "agentcabin-conversation-display";
export const inject = ["systemPrompt"];

export function apply(ctx) {
  const text = String(process.env.AGENTCABIN_DISPLAY_GUIDANCE || "").trim();
  if (!text) return;
  ctx.systemPrompt.section({
    name: "agentcabin:conversation-display",
    // This is an AgentCabin-owned section, not a built-in DSH order key.
    // Use a finite value so older Harness profiles do not reject the plugin.
    order: 1000,
    text,
  });
}
