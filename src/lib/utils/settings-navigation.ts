/**
 * A settings tab is only active while the settings view is being rendered.
 * The settings component stays mounted when switching to plugin views, so the
 * last tab value must not leak into the plugin navigation highlight.
 */
export function isSettingsTabActive(activeView: string, activeTab: string, tab: string): boolean {
  return activeView === "settings" && activeTab === tab;
}

/**
 * Settings pages keep the shared panel typography and spacing. The capability
 * center uses the available width and lets its content component own spacing.
 */
export function shouldUseSettingsMainPanel(activeView: string): boolean {
  return activeView === "settings";
}

/**
 * The capability center uses the same panel styling as other settings pages,
 * but its catalog needs the available content width.
 */
export function shouldUseWideSettingsMainPanel(activeView: string, activeTab: string): boolean {
  return (
    activeView === "settings" &&
    ["capability-center", "remote-access", "web-access", "browser-use", "desktop-use"].includes(
      activeTab,
    )
  );
}
