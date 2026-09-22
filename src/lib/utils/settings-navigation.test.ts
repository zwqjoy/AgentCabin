import { describe, expect, it } from "vitest";
import {
  isSettingsTabActive,
  shouldUseSettingsMainPanel,
  shouldUseWideSettingsMainPanel,
} from "./settings-navigation";

describe("settings navigation active state", () => {
  it("does not keep a settings tab highlighted while viewing a plugin section", () => {
    expect(isSettingsTabActive("plugins", "remote", "remote")).toBe(false);
    expect(isSettingsTabActive("plugins", "remote", "general")).toBe(false);
  });

  it("highlights the selected settings tab only inside the settings view", () => {
    expect(isSettingsTabActive("settings", "remote", "remote")).toBe(true);
    expect(isSettingsTabActive("settings", "remote", "general")).toBe(false);
    expect(isSettingsTabActive("settings", "pet", "pet")).toBe(true);
    expect(isSettingsTabActive("settings", "general", "pet")).toBe(false);
  });

  it("keeps the shared settings panel style for the capability center", () => {
    expect(shouldUseSettingsMainPanel("settings")).toBe(true);
    expect(shouldUseSettingsMainPanel("plugins")).toBe(false);
    expect(shouldUseWideSettingsMainPanel("settings", "capability-center")).toBe(true);
    expect(shouldUseWideSettingsMainPanel("settings", "web-access")).toBe(true);
    expect(shouldUseWideSettingsMainPanel("settings", "browser-use")).toBe(true);
    expect(shouldUseWideSettingsMainPanel("settings", "desktop-use")).toBe(true);
    expect(shouldUseWideSettingsMainPanel("settings", "general")).toBe(false);
    expect(shouldUseWideSettingsMainPanel("plugins", "capability-center")).toBe(false);
  });
});
