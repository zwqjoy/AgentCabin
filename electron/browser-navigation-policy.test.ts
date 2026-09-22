import { describe, expect, it } from "vitest";
import { classifyNavigation } from "./browser-navigation-policy";

const ctx = { fromUrl: "https://app.example.com/dashboard", allowedHosts: ["cdn.example.com"] };

describe("classifyNavigation", () => {
  it("allows same-origin navigation", () => {
    expect(classifyNavigation({ ...ctx, toUrl: "https://app.example.com/settings" })).toEqual({
      action: "allow",
    });
  });

  it("allows an explicitly allow-listed host", () => {
    expect(classifyNavigation({ ...ctx, toUrl: "https://cdn.example.com/a.js" })).toEqual({
      action: "allow",
    });
  });

  it("asks before an unknown host", () => {
    expect(classifyNavigation({ ...ctx, toUrl: "https://docs.other.com/x" })).toEqual({
      action: "ask",
      host: "docs.other.com",
    });
  });

  it("hard-blocks loopback even when allow-listed", () => {
    expect(
      classifyNavigation({
        fromUrl: "https://app.example.com/",
        toUrl: "http://127.0.0.1:8080/admin",
        allowedHosts: ["127.0.0.1"],
      }),
    ).toEqual({ action: "block", reason: "local_or_metadata_host" });
  });

  it("blocks non-http(s) schemes", () => {
    expect(classifyNavigation({ ...ctx, toUrl: "file:///etc/passwd" })).toEqual({
      action: "block",
      reason: "unsupported_scheme",
    });
  });

  it("allows about:blank", () => {
    expect(classifyNavigation({ ...ctx, toUrl: "about:blank" })).toEqual({ action: "allow" });
  });

  it("blocks an unparseable target", () => {
    expect(classifyNavigation({ ...ctx, toUrl: "http://" })).toEqual({
      action: "block",
      reason: "unparseable_url",
    });
  });
});
