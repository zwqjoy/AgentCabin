const CONTEXT_STATUS_KEY = "agentcabin-context-usage";
const SNAPSHOT_VERSION = 1;
const BUILTIN_TOOL_NAMES = new Set(["bash", "read", "write", "edit", "grep", "find", "ls"]);

function tokenEstimate(value) {
  const text = typeof value === "string" ? value : JSON.stringify(value ?? "");
  return charsToTokens(text.length);
}

function charsToTokens(chars) {
  return Math.max(0, Math.ceil(Math.max(0, chars) / 4));
}

function contextUsageOf(context) {
  try {
    const usage = context.getContextUsage?.();
    if (!usage || typeof usage !== "object") return {};
    return {
      tokens: Number.isFinite(usage.tokens) ? Math.max(0, usage.tokens) : 0,
      contextWindow: Number.isFinite(usage.contextWindow) ? Math.max(0, usage.contextWindow) : 0,
      percent: Number.isFinite(usage.percent) ? Math.max(0, usage.percent) : undefined,
    };
  } catch {
    return {};
  }
}

function findSpan(text, startMarker, endMarker, from = 0) {
  const start = text.indexOf(startMarker, from);
  if (start < 0) return undefined;
  const contentStart = start + startMarker.length;
  const end = endMarker ? text.indexOf(endMarker, contentStart) : text.length;
  return { start, end: end < 0 ? text.length : end + endMarker.length };
}

function mergeSpans(spans) {
  const sorted = spans.filter(Boolean).sort((a, b) => a.start - b.start);
  const merged = [];
  for (const span of sorted) {
    const previous = merged[merged.length - 1];
    if (previous && span.start <= previous.end) previous.end = Math.max(previous.end, span.end);
    else merged.push({ ...span });
  }
  return merged;
}

function promptBreakdown(systemPrompt) {
  const text = typeof systemPrompt === "string" ? systemPrompt : "";
  const instructions = findSpan(text, "<project_context>", "</project_context>");
  const skills = findSpan(text, "The following skills provide specialized instructions", "</available_skills>");
  const toolPrompt = findSpan(text, "\nAvailable tools:\n", "\n\n");
  const guidelines = findSpan(text, "\nGuidelines:\n", "\n\n");
  const spans = mergeSpans([instructions, skills, toolPrompt, guidelines]);
  const reserved = spans.reduce((sum, span) => sum + span.end - span.start, 0);
  return [
    { id: "system_prompt", chars: Math.max(0, text.length - reserved) },
    { id: "instructions", chars: instructions ? instructions.end - instructions.start : 0 },
    { id: "skills", chars: skills ? skills.end - skills.start : 0 },
    { id: "tool_prompt", chars: mergeSpans([toolPrompt, guidelines]).reduce((sum, span) => sum + span.end - span.start, 0) },
  ];
}

function toolBucket(tool) {
  const name = String(tool?.name ?? "");
  const source = String(tool?.sourceInfo?.source ?? tool?.source ?? "");
  if (/mcp/i.test(`${name} ${source}`)) return "mcp_tools";
  if (BUILTIN_TOOL_NAMES.has(name) || source === "pi") return "system_tools";
  return "custom_tools";
}

function toolBreakdown(allTools, activeToolNames) {
  const byName = new Map((Array.isArray(allTools) ? allTools : []).map((tool) => [tool?.name, tool]));
  const buckets = new Map([
    ["system_tools", 0],
    ["custom_tools", 0],
    ["mcp_tools", 0],
  ]);
  for (const name of new Set(Array.isArray(activeToolNames) ? activeToolNames : [])) {
    const tool = byName.get(name);
    if (!tool) continue;
    const size = tokenEstimate({
      name: tool.name,
      description: tool.description,
      parameters: tool.parameters,
    });
    buckets.set(toolBucket(tool), (buckets.get(toolBucket(tool)) ?? 0) + size);
  }
  return [...buckets].map(([id, tokens]) => ({ id, tokens }));
}

function messageBreakdown(messages) {
  const buckets = new Map([
    ["user_messages", 0],
    ["assistant_text", 0],
    ["assistant_thinking", 0],
    ["tool_calls", 0],
    ["tool_results", 0],
    ["extension_messages", 0],
    ["other_messages", 0],
  ]);
  for (const message of Array.isArray(messages) ? messages : []) {
    if (message?.role === "user") {
      buckets.set("user_messages", buckets.get("user_messages") + tokenEstimate(message));
    } else if (message?.role === "toolResult") {
      buckets.set("tool_results", buckets.get("tool_results") + tokenEstimate(message));
    } else if (message?.role === "custom") {
      buckets.set("extension_messages", buckets.get("extension_messages") + tokenEstimate(message));
    } else if (message?.role === "assistant" && Array.isArray(message.content)) {
      for (const block of message.content) {
        const type = String(block?.type ?? "text");
        const id = type === "thinking" ? "assistant_thinking" : type === "toolCall" ? "tool_calls" : "assistant_text";
        buckets.set(id, buckets.get(id) + tokenEstimate(block));
      }
    } else {
      buckets.set("other_messages", buckets.get("other_messages") + tokenEstimate(message));
    }
  }
  return [...buckets].map(([id, tokens]) => ({ id, tokens }));
}

function scaleToReported(categories, reportedTokens) {
  const positive = categories.filter((category) => category.tokens > 0);
  const estimatedTotal = positive.reduce((sum, category) => sum + category.tokens, 0);
  if (!reportedTokens || estimatedTotal <= 0) return categories;
  const scaled = positive.map((category) => ({
    ...category,
    tokens: Math.floor((category.tokens / estimatedTotal) * reportedTokens),
  }));
  const remainder = reportedTokens - scaled.reduce((sum, category) => sum + category.tokens, 0);
  if (remainder > 0) scaled[0].tokens += remainder;
  const byId = new Map(scaled.map((category) => [category.id, category.tokens]));
  return categories.map((category) => ({ ...category, tokens: byId.get(category.id) ?? 0 }));
}

export function buildContextUsageSnapshot({ systemPrompt, messages, allTools, activeToolNames, context }) {
  const reported = contextUsageOf(context);
  const raw = [
    ...promptBreakdown(systemPrompt).map(({ id, chars }) => ({ id, tokens: charsToTokens(chars) })),
    ...toolBreakdown(allTools, activeToolNames),
    ...messageBreakdown(messages),
  ];
  const categories = scaleToReported(raw, reported.tokens).filter((category) => category.tokens > 0);
  const estimatedTokens = categories.reduce((sum, category) => sum + category.tokens, 0);
  const usedTokens = reported.tokens || estimatedTokens;
  const contextWindow = reported.contextWindow || 0;
  if (contextWindow > usedTokens) categories.push({ id: "free_space", tokens: contextWindow - usedTokens });
  return {
    version: SNAPSHOT_VERSION,
    source: "agentcabin-pi-context-view",
    measuredAt: Date.now(),
    contextWindow,
    usedTokens,
    // Pi exposes this field as a percentage in the 0..100 range.
    percent: reported.percent ?? (contextWindow > 0 ? (usedTokens / contextWindow) * 100 : 0),
    estimated: true,
    reportedAggregate: Boolean(reported.tokens),
    categories,
  };
}

function publish(context, snapshot) {
  try {
    context.ui?.setStatus?.(CONTEXT_STATUS_KEY, JSON.stringify(snapshot));
  } catch {
    // Context reporting must never affect the agent turn.
  }
}

export default function agentcabinContextUsage(pi) {
  pi.on("context", (event, context) => {
    publish(
      context,
      buildContextUsageSnapshot({
        systemPrompt: context.getSystemPrompt?.() ?? "",
        messages: event.messages,
        allTools: pi.getAllTools?.() ?? [],
        activeToolNames: pi.getActiveTools?.() ?? [],
        context,
      }),
    );
  });
}
