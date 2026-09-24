import { Lexer } from "marked";

export interface ConversationPart {
  kind: "markdown" | "html" | "echarts" | "mermaid";
  content: string;
  pending?: boolean;
}

const ECHARTS_SERIES_TYPES = new Set([
  "line",
  "bar",
  "pie",
  "scatter",
  "radar",
  "heatmap",
  "boxplot",
  "tree",
  "treemap",
  "sunburst",
  "sankey",
  "funnel",
  "candlestick",
  "gauge",
  "graph",
  "lines",
  "effectScatter",
]);

/** Recognize JSON-formatted ECharts options without treating ordinary JSON as executable. */
function isEchartsJsonOption(content: string): boolean {
  try {
    const parsed: unknown = JSON.parse(content);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return false;
    const option = parsed as Record<string, unknown>;
    return (
      Array.isArray(option.series) &&
      option.series.length > 0 &&
      option.series.every(
        (series) =>
          !!series &&
          typeof series === "object" &&
          !Array.isArray(series) &&
          typeof (series as Record<string, unknown>).type === "string" &&
          ECHARTS_SERIES_TYPES.has((series as Record<string, string>).type),
      )
    );
  } catch {
    return false;
  }
}

/** Only top-level, explicitly labelled fences are executable previews. */
export function conversationParts(text: string, streaming = false): ConversationPart[] {
  const parts: ConversationPart[] = [];
  const tokens = Lexer.lex(text);
  for (const token of tokens) {
    const opening = /^ {0,3}(`{3,}|~{3,})([^\n]*)\n/.exec(token.raw);
    const language = token.type === "code" ? (token.lang?.trim() ?? "") : "";
    const isEchartsOption =
      /^echarts$/i.test(language) ||
      (/^json$/i.test(language) && isEchartsJsonOption(token.type === "code" ? token.text : ""));
    if (token.type === "code" && opening && isEchartsOption) {
      const fence = opening[1];
      const closed = new RegExp(`\\n {0,3}${fence[0]}{${fence.length},}[ \\t]*(?:\\n)?$`).test(
        token.raw,
      );
      parts.push({ kind: "echarts", content: token.text, pending: streaming && !closed });
    } else if (token.type === "code" && opening && /^mermaid$/i.test(language)) {
      const fence = opening[1];
      const closed = new RegExp(`\\n {0,3}${fence[0]}{${fence.length},}[ \\t]*(?:\\n)?$`).test(
        token.raw,
      );
      parts.push({ kind: "mermaid", content: token.text, pending: streaming && !closed });
    } else if (
      token.type === "code" &&
      opening &&
      /^(?:html-preview|html(?:\s+type=(["'])renderer\1)?)$/i.test(language)
    ) {
      const fence = opening[1];
      const closed = new RegExp(`\\n {0,3}${fence[0]}{${fence.length},}[ \\t]*(?:\\n)?$`).test(
        token.raw,
      );
      parts.push({ kind: "html", content: token.text, pending: streaming && !closed });
    } else {
      const previous = parts.at(-1);
      if (previous?.kind === "markdown") previous.content += token.raw;
      else parts.push({ kind: "markdown", content: token.raw });
    }
  }
  // Keep references available in each prose segment after splitting around previews.
  const definitions = Object.entries(tokens.links)
    .map(
      ([label, link]) =>
        `[${label}]: <${link.href}>${link.title ? ` ${JSON.stringify(link.title)}` : ""}`,
    )
    .join("\n");
  if (definitions)
    for (const part of parts) if (part.kind === "markdown") part.content += `\n${definitions}`;
  return parts;
}
