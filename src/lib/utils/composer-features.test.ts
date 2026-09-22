import { describe, expect, it } from "vitest";
import {
  composerFeatureDescription,
  composerSlashEnabled,
  getComposerClearAction,
  getComposerFeatureActions,
  isComposerFeatureActionBlocked,
  isPiGoalChipActive,
} from "./composer-features";

describe("composer feature actions", () => {
  it("keeps the slash affordance while a provider catalog is still being discovered", () => {
    expect(composerSlashEnabled("grok", false, false, false)).toBe(true);
    expect(composerSlashEnabled("grok", false, false, true)).toBe(true);
    expect(composerSlashEnabled("codex", true, true, false)).toBe(true);
    expect(composerSlashEnabled("claude", true, true, false)).toBe(true);
    expect(composerSlashEnabled("minimal", false, false, false)).toBe(false);
  });

  it("only exposes capabilities that are actually available", () => {
    expect(getComposerFeatureActions({ goalAvailable: true, planAvailable: false })).toEqual([
      "goal",
    ]);
    expect(getComposerFeatureActions({ goalAvailable: false, planAvailable: true })).toEqual([
      "plan",
    ]);
    expect(getComposerFeatureActions({ goalAvailable: false, planAvailable: false })).toEqual([]);
  });

  it("allows a cold Pi session to bootstrap while capabilities are pending", () => {
    expect(
      isComposerFeatureActionBlocked({
        disabled: false,
        discoveryPending: true,
        operationPending: false,
        allowDiscoveryPendingAction: true,
      }),
    ).toBe(false);
    expect(
      isComposerFeatureActionBlocked({
        disabled: false,
        discoveryPending: true,
        operationPending: false,
      }),
    ).toBe(true);
  });

  it("changes descriptions for active and discovery-pending states", () => {
    expect(composerFeatureDescription("plan", false, false)).toBe("开启计划模式");
    expect(composerFeatureDescription("plan", true, false)).toBe("关闭计划模式");
    expect(composerFeatureDescription("goal", false, true)).toBe("正在检测目标能力…");
  });

  it("keeps a native Goal chip for all meaningful persisted phases", () => {
    expect(isPiGoalChipActive("active")).toBe(true);
    expect(isPiGoalChipActive("paused")).toBe(true);
    expect(isPiGoalChipActive("complete")).toBe(true);
    expect(isPiGoalChipActive("inactive")).toBe(false);
    expect(isPiGoalChipActive("unavailable")).toBe(false);
  });

  it("routes clear chips through each agent's native or compatibility path", () => {
    expect(getComposerClearAction("pi", "plan")).toBe("pi-plan-exit");
    expect(getComposerClearAction("claude", "plan")).toBe("plan-mode-off");
    expect(getComposerClearAction("grok", "goal")).toBe("local-goal-clear");
    expect(getComposerClearAction("codex", "goal")).toBe("codex-goal-clear");
    expect(getComposerClearAction("pi", "goal")).toBe("pi-goal-clear");
  });
});
