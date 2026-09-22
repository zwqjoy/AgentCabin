/**
 * Injects strict CSP meta tag into raw HTML for secure iframe rendering.
 */
export function injectCsp(rawHtml: string): string {
  const cspMeta = `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline' data:; img-src data: blob:; font-src data:; connect-src 'none'; form-action 'none'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; worker-src 'none'; manifest-src 'none';">`;

  if (!rawHtml) return cspMeta;

  // Check if <head> exists
  const headMatch = /<head[^>]*>/i.exec(rawHtml);
  if (headMatch) {
    const idx = headMatch.index + headMatch[0].length;
    return rawHtml.slice(0, idx) + "\n  " + cspMeta + rawHtml.slice(idx);
  }

  // Check if <html> exists
  const htmlMatch = /<html[^>]*>/i.exec(rawHtml);
  if (htmlMatch) {
    const idx = htmlMatch.index + htmlMatch[0].length;
    return rawHtml.slice(0, idx) + "\n<head>\n  " + cspMeta + "\n</head>" + rawHtml.slice(idx);
  }

  // Otherwise prepend head with CSP
  return `<head>\n  ${cspMeta}\n</head>\n` + rawHtml;
}
