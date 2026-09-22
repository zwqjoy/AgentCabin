export type RuntimeSubTab = "codex" | "claude" | "grok" | "dsh" | "pi";
export type CodeSubTab = "carrier" | "worktrees";

export type SettingsTab =
  | "general"
  | "appearance"
  | "keybindings"
  | "usage"
  | "pet"
  | "capability-center"
  | "code"
  | "work"
  | "runtimes"
  | "models"
  | "doctor"
  | "web-access"
  | "browser-use"
  | "desktop-use";

export interface ResolvedSettingsRoute {
  tab: SettingsTab;
  codeSubTab?: CodeSubTab | string;
  runtimeSubTab?: RuntimeSubTab;
  section?: string;
}

export const CANONICAL_SETTINGS_TABS: readonly SettingsTab[] = [
  "general",
  "appearance",
  "keybindings",
  "usage",
  "pet",
  "capability-center",
  "code",
  "work",
  "runtimes",
  "models",
  "doctor",
  "web-access",
  "browser-use",
  "desktop-use",
] as const;

/**
 * Resolves any incoming tab parameter (including legacy AC tabs) into
 * the canonical Settings V2 tab and optional subtab.
 */
export function resolveSettingsRoute(
  rawTab: string | null | undefined,
  category: string | null = null,
): ResolvedSettingsRoute {
  if (!rawTab) return { tab: "general" };

  const tab = rawTab.trim().toLowerCase();

  // Native Runtime Providers mapping
  if (tab === "native-codex" || tab === "codex") {
    return { tab: "runtimes", runtimeSubTab: "codex" };
  }
  if (tab === "native-claude" || tab === "claude" || tab === "cli-config" || tab === "agent-auth") {
    return { tab: "runtimes", runtimeSubTab: "claude" };
  }
  if (tab === "native-grok" || tab === "grok") {
    return { tab: "runtimes", runtimeSubTab: "grok" };
  }
  if (tab === "native-dsh" || tab === "dsh") {
    return { tab: "runtimes", runtimeSubTab: "dsh" };
  }
  if (tab === "pi-common" || tab === "pi" || tab === "pi-agent") {
    return { tab: "runtimes", runtimeSubTab: "pi" };
  }
  if (
    tab === "runtimes" ||
    tab === "runtime" ||
    tab === "runtime-providers" ||
    tab === "native-agents"
  ) {
    return { tab: "runtimes", runtimeSubTab: "dsh" };
  }

  // Code carrier mode mapping
  if (tab === "code" || tab === "pi-code" || tab === "worktrees") {
    return { tab: "code" };
  }

  // Work carrier mode mapping
  if (tab === "pi-work" || tab === "work") {
    return { tab: "work" };
  }

  // Models & Providers mapping
  if (tab === "connection" || tab === "models" || tab === "providers" || tab === "global-models") {
    return { tab: "models" };
  }

  // Doctor & CLI Health mapping
  if (
    tab === "runtime-overview" ||
    tab === "doctor" ||
    tab === "overview" ||
    tab === "cli-doctor"
  ) {
    return { tab: "doctor" };
  }

  // Tools & Capability Center mapping
  if (tab === "web-access" || tab === "web" || category === "web") {
    return { tab: "web-access" };
  }
  if (tab === "browser-use" || tab === "browser" || category === "browser") {
    return { tab: "browser-use" };
  }
  if (tab === "desktop-use" || tab === "desktop" || tab === "computer") {
    return { tab: "desktop-use" };
  }
  if (tab === "mcp") {
    return { tab: "capability-center", section: "mcp" };
  }
  if (
    tab === "capability-center" ||
    tab === "capability" ||
    tab === "capabilities" ||
    tab === "plugins"
  ) {
    return { tab: "capability-center" };
  }

  // Personal mapping
  if (tab === "appearance" || tab === "theme") {
    return { tab: "appearance" };
  }
  if (tab === "keybindings" || tab === "shortcuts") {
    return { tab: "keybindings" };
  }
  if (tab === "usage" || tab === "billing") {
    return { tab: "usage" };
  }
  if (tab === "pet") {
    return { tab: "pet" };
  }
  if (tab === "general" || tab === "hosts" || tab === "ssh" || tab === "debug") {
    return { tab: "general" };
  }

  // If already a canonical tab
  if (CANONICAL_SETTINGS_TABS.includes(tab as SettingsTab)) {
    return { tab: tab as SettingsTab };
  }

  return { tab: "general" };
}
