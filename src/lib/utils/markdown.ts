import { Marked, type Token } from "marked";
import { escapeHtml } from "$lib/utils/ansi";
import { perfMark } from "$lib/utils/perf";
import { hljs } from "$lib/utils/hljs-init";
import DOMPurify from "dompurify";

const marked = new Marked();

export function isFilePath(str: string): boolean {
  if (!str) return false;
  const trimmed = str.trim();
  if (!trimmed || trimmed.includes("\n") || trimmed.length > 260) return false;
  if (/^(https?|mailto|tel|data|javascript):/i.test(trimmed)) return false;
  const clean = trimmed.startsWith("file://") ? trimmed.slice(7) : trimmed;
  if (!clean || /[*?"<>|]/.test(clean) || clean.startsWith("-")) return false;
  const hasExt = /\.[a-zA-Z0-9_-]*[a-zA-Z_-][a-zA-Z0-9_-]*$/.test(clean);
  if (hasExt) {
    if (clean.includes(" ") && !clean.includes("/") && !clean.includes("\\")) {
      if (clean.split(" ").length > 3) return false;
    }
    return true;
  }
  return false;
}

marked.use({
  gfm: true,
  breaks: false,
  extensions: [
    {
      name: "filePath",
      level: "inline",
      start(src: string) {
        const match = src.match(
          /(?:^|[\s(（`])((?:[a-zA-Z0-9_\u4e00-\u9fa5.\-~]+[\\/])+[a-zA-Z0-9_\u4e00-\u9fa5.\-~]+\.[a-zA-Z0-9_-]*[a-zA-Z_-][a-zA-Z0-9_-]*)/,
        );
        if (!match || match.index === undefined) return -1;
        const offset = match[0].length - match[1].length;
        return match.index + offset;
      },
      tokenizer(src: string) {
        const rule =
          /^((?:[a-zA-Z0-9_\u4e00-\u9fa5.\-~]+[\\/])+[a-zA-Z0-9_\u4e00-\u9fa5.\-~]+\.[a-zA-Z0-9_-]*[a-zA-Z_-][a-zA-Z0-9_-]*)/;
        const match = rule.exec(src);
        if (match) {
          return {
            type: "filePath",
            raw: match[0],
            path: match[0],
          };
        }
      },
      renderer(token: any) {
        const p = escapeHtml(token.path || "");
        return `<code class="file-path-link" data-file-path="${p}" role="button" tabindex="0" title="点击打开文件: ${p}"><span class="file-icon" aria-hidden="true">📄</span>${p}</code>`;
      },
    },
  ],
  renderer: {
    // marked v15: table(token) receives a Token with header[] and rows[][]
    table(token: {
      header: Array<{ tokens: Token[]; align: string | null; header: boolean }>;
      rows: Array<Array<{ tokens: Token[]; align: string | null; header: boolean }>>;
    }) {
      // Build header cells
      let headerCells = "";
      for (const cell of token.header) {
        const content = this.parser.parseInline(cell.tokens);
        const tag = cell.align ? `<th align="${cell.align}">` : "<th>";
        headerCells += `${tag}${content}</th>\n`;
      }
      const headerRow = `<tr>\n${headerCells}</tr>\n`;

      // Build body rows
      let body = "";
      for (const row of token.rows) {
        let rowCells = "";
        for (const cell of row) {
          const content = this.parser.parseInline(cell.tokens);
          const tag = cell.align ? `<td align="${cell.align}">` : "<td>";
          rowCells += `${tag}${content}</td>\n`;
        }
        body += `<tr>\n${rowCells}</tr>\n`;
      }
      if (body) body = `<tbody>${body}</tbody>`;

      return `<div class="table-wrapper"><table><thead>${headerRow}</thead>${body}</table></div>`;
    },
    code({ text, lang }: { text: string; lang?: string }) {
      const language = lang || "";
      let highlighted: string;

      if (language && hljs.getLanguage(language)) {
        try {
          highlighted = hljs.highlight(text, { language }).value;
        } catch {
          highlighted = escapeHtml(text);
        }
      } else {
        // Skip highlightAuto() — it tries all ~190 languages synchronously
        // and can freeze the UI for seconds on large code blocks
        highlighted = escapeHtml(text);
      }

      const displayLang = language || "text";

      return `<div class="code-block not-prose"><div class="code-block-header"><span class="code-block-lang">${escapeHtml(displayLang)}</span><button class="code-block-copy" data-code-copy>Copy</button></div><pre><code class="hljs language-${escapeHtml(language)}">${highlighted}</code></pre></div>`;
    },
    codespan(token: { text: string }) {
      if (isFilePath(token.text)) {
        const p = escapeHtml(token.text.trim());
        return `<code class="file-path-link" data-file-path="${p}" role="button" tabindex="0" title="点击打开文件: ${p}"><span class="file-icon" aria-hidden="true">📄</span>${p}</code>`;
      }
      return `<code>${escapeHtml(token.text)}</code>`;
    },
    link(token: { href: string; text: string; title?: string | null; tokens: Token[] }) {
      const content = this.parser.parseInline(token.tokens);
      const href = token.href || "";
      const titleAttr = token.title ? ` title="${escapeHtml(token.title)}"` : "";
      if (isFilePath(href) || isFilePath(token.text)) {
        const p = escapeHtml((isFilePath(href) ? href : token.text).trim());
        return `<a href="${escapeHtml(href)}" class="file-path-link" data-file-path="${p}" role="button" tabindex="0"${titleAttr || ` title="点击打开文件: ${p}"`}><span class="file-icon" aria-hidden="true">📄</span>${content}</a>`;
      }
      const target =
        href.startsWith("http://") || href.startsWith("https://")
          ? ' target="_blank" rel="noopener noreferrer"'
          : "";
      return `<a href="${escapeHtml(href)}"${titleAttr}${target}>${content}</a>`;
    },
  },
});

export function renderMarkdown(text: string): string {
  return perfMark(
    "md-render",
    () => {
      const raw = marked.parse(text);
      if (typeof raw !== "string") return "";
      return DOMPurify.sanitize(raw, {
        ADD_ATTR: [
          "class",
          "target",
          "rel",
          "data-code-copy",
          "data-file-path",
          "role",
          "tabindex",
          "title",
          "aria-hidden",
        ],
        ADD_TAGS: ["span"],
      });
    },
    { chars: text.length, codeFenceCount: text.match(/```/g)?.length ?? 0 },
  );
}
