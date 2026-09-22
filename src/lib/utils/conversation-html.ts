import { Lexer } from "marked";

export interface ConversationPart {
  kind: "markdown" | "html";
  content: string;
  pending?: boolean;
}

/** Only top-level, explicitly labelled fences are executable previews. */
export function conversationParts(text: string, streaming = false): ConversationPart[] {
  const parts: ConversationPart[] = [];
  const tokens = Lexer.lex(text);
  for (const token of tokens) {
    const opening = /^ {0,3}(`{3,}|~{3,})([^\n]*)\n/.exec(token.raw);
    if (
      token.type === "code" &&
      opening &&
      /^(html|html-preview)$/i.test(token.lang?.trim() ?? "")
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
