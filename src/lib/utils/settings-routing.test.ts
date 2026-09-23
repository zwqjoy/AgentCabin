import { describe, expect, it } from "vitest";
import { resolveSettingsRoute, CANONICAL_SETTINGS_TABS } from "./settings-routing";

describe("Settings V2 routing & legacy tab compatibility", () => {
  it("resolves null/undefined to general", () => {
    expect(resolveSettingsRoute(null)).toEqual({ tab: "general" });
    expect(resolveSettingsRoute(undefined)).toEqual({ tab: "general" });
    expect(resolveSettingsRoute("")).toEqual({ tab: "general" });
  });

  it("resolves canonical V2 tabs correctly", () => {
    for (const tab of CANONICAL_SETTINGS_TABS) {
      const res = resolveSettingsRoute(tab);
      expect(res.tab).toBe(tab);
    }
  });

  it("maps native runtime engine tabs to runtimes with corresponding subtab", () => {
    expect(resolveSettingsRoute("native-codex")).toEqual({
      tab: "runtimes",
      runtimeSubTab: "codex",
    });
    expect(resolveSettingsRoute("codex")).toEqual({ tab: "runtimes", runtimeSubTab: "codex" });

    expect(resolveSettingsRoute("native-claude")).toEqual({
      tab: "runtimes",
      runtimeSubTab: "claude",
    });
    expect(resolveSettingsRoute("claude")).toEqual({ tab: "runtimes", runtimeSubTab: "claude" });
    expect(resolveSettingsRoute("agent-auth")).toEqual({
      tab: "runtimes",
      runtimeSubTab: "claude",
    });
    expect(resolveSettingsRoute("cli-config")).toEqual({
      tab: "runtimes",
      runtimeSubTab: "claude",
    });

    expect(resolveSettingsRoute("native-grok")).toEqual({ tab: "runtimes", runtimeSubTab: "grok" });
    expect(resolveSettingsRoute("grok")).toEqual({ tab: "runtimes", runtimeSubTab: "grok" });

    expect(resolveSettingsRoute("native-dsh")).toEqual({ tab: "runtimes", runtimeSubTab: "dsh" });
    expect(resolveSettingsRoute("dsh")).toEqual({ tab: "runtimes", runtimeSubTab: "dsh" });

    expect(resolveSettingsRoute("pi")).toEqual({ tab: "runtimes", runtimeSubTab: "pi" });
    expect(resolveSettingsRoute("pi-common")).toEqual({ tab: "runtimes", runtimeSubTab: "pi" });
    expect(resolveSettingsRoute("runtimes")).toEqual({ tab: "runtimes", runtimeSubTab: "pi" });
    expect(resolveSettingsRoute("runtime")).toEqual({ tab: "runtimes", runtimeSubTab: "pi" });
  });

  it("maps code carrier mode tabs correctly", () => {
    expect(resolveSettingsRoute("code")).toEqual({ tab: "code" });
    expect(resolveSettingsRoute("pi-code")).toEqual({ tab: "code" });
    expect(resolveSettingsRoute("worktrees")).toEqual({ tab: "code" });
  });

  it("maps legacy work and overview tabs correctly", () => {
    expect(resolveSettingsRoute("pi-work")).toEqual({ tab: "work" });
    expect(resolveSettingsRoute("work")).toEqual({ tab: "work" });

    expect(resolveSettingsRoute("runtime-overview")).toEqual({ tab: "doctor" });
    expect(resolveSettingsRoute("doctor")).toEqual({ tab: "doctor" });
    expect(resolveSettingsRoute("overview")).toEqual({ tab: "doctor" });

    expect(resolveSettingsRoute("connection")).toEqual({ tab: "models" });
    expect(resolveSettingsRoute("providers")).toEqual({ tab: "models" });
    expect(resolveSettingsRoute("models")).toEqual({ tab: "models" });
  });

  it("maps absorbed tabs (shortcuts, debug, hosts)", () => {
    expect(resolveSettingsRoute("debug")).toEqual({ tab: "general" });
    expect(resolveSettingsRoute("hosts")).toEqual({ tab: "general" });
    expect(resolveSettingsRoute("ssh")).toEqual({ tab: "general" });
    expect(resolveSettingsRoute("shortcuts")).toEqual({ tab: "keybindings" });
  });

  it("maps tool and capability tabs", () => {
    expect(resolveSettingsRoute("plugins")).toEqual({ tab: "capability-center" });
    expect(resolveSettingsRoute("capability")).toEqual({ tab: "capability-center" });
    expect(resolveSettingsRoute("web")).toEqual({ tab: "web-access" });
    expect(resolveSettingsRoute("remote")).toEqual({ tab: "remote-access" });
    expect(resolveSettingsRoute("web-server")).toEqual({ tab: "remote-access" });
    expect(resolveSettingsRoute("remote-browser")).toEqual({ tab: "remote-access" });
    expect(resolveSettingsRoute("browser")).toEqual({ tab: "browser-use" });
    expect(resolveSettingsRoute("desktop")).toEqual({ tab: "desktop-use" });
    expect(resolveSettingsRoute("computer")).toEqual({ tab: "desktop-use" });
    expect(resolveSettingsRoute("hooks")).toEqual({ tab: "general" });
    expect(resolveSettingsRoute("mcp")).toEqual({ tab: "capability-center", section: "mcp" });
  });
});
