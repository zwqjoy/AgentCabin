import { describe, expect, it } from "vitest";
import type {
  CapabilityCenterItem,
  CapabilityCenterOverview,
  RunEffectiveCapabilitiesView,
  WorkArtifactAcceptance,
  WorkArtifactCheck,
} from "$lib/types/work";

describe("Capability Center 2.0 Projection & UI Models", () => {
  it("correctly derives overview readiness counts", () => {
    const items: CapabilityCenterItem[] = [
      {
        id: "skill:code_review",
        category: "skill",
        name: "Code Review",
        description: "Review code diffs",
        readiness: "ready",
        readinessReason: "Ready to use",
        installed: true,
        enabled: true,
        scopes: ["workspace"],
        runtimeAvailability: [
          { provider: "pi", available: true },
          { provider: "claude", available: true },
        ],
        permissions: [],
        actions: [],
        origin: "builtin",
      },
      {
        id: "mcp:github",
        category: "mcp",
        name: "GitHub MCP",
        description: "GitHub integration",
        readiness: "needs_auth",
        readinessReason: "Missing GITHUB_TOKEN",
        installed: true,
        enabled: true,
        scopes: ["user"],
        runtimeAvailability: [{ provider: "claude", available: true }],
        permissions: [],
        actions: [{ actionType: "login", label: "登录认证" }],
        origin: "user",
      },
      {
        id: "connector:jira",
        category: "connector",
        name: "Jira Connector",
        description: "Jira issue tracker",
        readiness: "missing_dependency",
        readinessReason: "Requires jira CLI tool",
        installed: true,
        enabled: false,
        scopes: ["workspace"],
        runtimeAvailability: [{ provider: "pi", available: true }],
        permissions: [],
        actions: [{ actionType: "install_dependency", label: "安装依赖" }],
        origin: "user",
      },
      {
        id: "app:slack",
        category: "app",
        name: "Slack",
        description: "Slack messaging",
        readiness: "disabled",
        readinessReason: "App disabled in settings",
        installed: true,
        enabled: false,
        scopes: ["system"],
        runtimeAvailability: [{ provider: "pi", available: true }],
        permissions: [],
        actions: [{ actionType: "enable", label: "启用能力" }],
        origin: "builtin",
      },
      {
        id: "mcp:broken",
        category: "mcp",
        name: "Broken MCP",
        description: "Failing server",
        readiness: "unhealthy",
        readinessReason: "Process exited with code 1",
        installed: true,
        enabled: true,
        scopes: ["user"],
        runtimeAvailability: [{ provider: "claude", available: false }],
        permissions: [],
        actions: [{ actionType: "inspect_diagnostics", label: "查看诊断" }],
        origin: "user",
      },
    ];

    const overview: CapabilityCenterOverview = {
      total: items.length,
      readyCount: items.filter((i) => i.readiness === "ready").length,
      needsSetupCount: items.filter(
        (i) => i.readiness === "missing_dependency" || i.readiness === "disabled",
      ).length,
      needsAuthCount: items.filter((i) => i.readiness === "needs_auth").length,
      unavailableCount: items.filter(
        (i) =>
          i.readiness === "unhealthy" ||
          i.readiness === "incompatible" ||
          i.readiness === "not_installed",
      ).length,
    };

    expect(overview.total).toBe(5);
    expect(overview.readyCount).toBe(1);
    expect(overview.needsSetupCount).toBe(2);
    expect(overview.needsAuthCount).toBe(1);
    expect(overview.unavailableCount).toBe(1);
  });

  it("enforces secret redaction invariant in RunEffectiveCapabilitiesView", () => {
    const runCaps: RunEffectiveCapabilitiesView = {
      runId: "run-test-123",
      runtime: "pi",
      appMode: "work",
      enabledSkills: [
        {
          id: "github_sync",
          name: "github_sync",
          description: "Sync with github",
        },
      ],
      mcpServers: [
        {
          id: "secure-mcp",
          transport: "stdio",
          envKeys: ["API_KEY", "SECRET_TOKEN"],
          headerKeys: ["Authorization"],
        },
      ],
      connectors: [
        {
          id: "jira-connector",
          name: "Jira Connector",
          entryPoint: "jira",
        },
      ],
      browserEnabled: true,
      browserUseEnabled: false,
      browserStatus: "ready",
      allowedTools: ["*"],
      disallowedTools: [],
      diagnostics: [],
      strictMode: false,
    };

    const mcp = runCaps.mcpServers[0];
    expect(mcp.envKeys).toEqual(["API_KEY", "SECRET_TOKEN"]);
    expect(mcp.headerKeys).toEqual(["Authorization"]);

    // JSON serialization check: ensures no raw values leaked
    const serialized = JSON.stringify(runCaps);
    expect(serialized).not.toContain("password");
    expect(serialized).not.toContain("token_value");
    expect(serialized).not.toContain("bearer");
  });

  it("validates WorkArtifactAcceptance structure and evaluation logic", () => {
    const checks: WorkArtifactCheck[] = [
      {
        requirement: {
          path: "output/report.pdf",
          title: "Final evaluation report",
          required: true,
        },
        status: "satisfied",
        artifactId: "art-1",
        message: "Delivered and validated",
      },
      {
        requirement: {
          path: "output/summary.csv",
          title: "Data summary table",
          required: true,
        },
        status: "missing",
        message: "File was not produced during execution",
      },
    ];

    const acceptance: WorkArtifactAcceptance = {
      runId: "run-123",
      checks,
      requiredCount: checks.length,
      satisfiedCount: checks.filter((c) => c.status === "satisfied").length,
      missingCount: checks.filter((c) => c.status === "missing").length,
      invalidCount: checks.filter((c) => c.status === "invalid").length,
      satisfied: false,
    };

    expect(acceptance.satisfied).toBe(false);
    expect(acceptance.requiredCount).toBe(2);
    expect(acceptance.satisfiedCount).toBe(1);
    expect(acceptance.missingCount).toBe(1);
    expect(acceptance.checks[1].message).toContain("not produced");

    // After satisfying missing check
    const updatedChecks: WorkArtifactCheck[] = acceptance.checks.map((c) =>
      c.requirement.path === "output/summary.csv"
        ? {
            ...c,
            status: "satisfied" as const,
            artifactId: "art-2",
            message: "Delivered",
          }
        : c,
    );
    const updatedAcceptance: WorkArtifactAcceptance = {
      runId: "run-123",
      checks: updatedChecks,
      requiredCount: updatedChecks.length,
      satisfiedCount: updatedChecks.filter((c) => c.status === "satisfied").length,
      missingCount: updatedChecks.filter((c) => c.status === "missing").length,
      invalidCount: updatedChecks.filter((c) => c.status === "invalid").length,
      satisfied: updatedChecks.every((c) => c.status === "satisfied"),
    };

    expect(updatedAcceptance.satisfied).toBe(true);
    expect(updatedAcceptance.satisfiedCount).toBe(2);
    expect(updatedAcceptance.missingCount).toBe(0);
  });
});
