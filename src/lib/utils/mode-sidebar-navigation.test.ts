import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const sidebarSources = [
  ["Native / Pi Code", "src/routes/+layout.svelte"],
  ["Pi Work", "src/lib/components/work/WorkSidebar.svelte"],
] as const;

describe("mode sidebar navigation", () => {
  it("does not expose Capability Center in any of the three modes", () => {
    for (const [mode, relativePath] of sidebarSources) {
      const source = readFileSync(resolve(process.cwd(), relativePath), "utf8");
      expect(source, `${mode} sidebar`).not.toMatch(/Capability Center|能力中心/);
    }
  });

  it("exposes a mode-specific new conversation action", () => {
    const nativeAndPiCodeSidebar = readFileSync(
      resolve(process.cwd(), "src/routes/+layout.svelte"),
      "utf8",
    );
    const piWorkSidebar = readFileSync(
      resolve(process.cwd(), "src/lib/components/work/WorkSidebar.svelte"),
      "utf8",
    );

    expect(nativeAndPiCodeSidebar).toMatch(/onclick=\{newChat\}/);
    expect(piWorkSidebar).toMatch(/newConversationForWorkspace|newStandaloneConversation/);
  });
});
