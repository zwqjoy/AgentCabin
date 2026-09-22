/**
 * `$lib/platform` — the single entry point for desktop-host capabilities.
 *
 * Pages and components import from here and must not import `@tauri-apps/*`
 * or read `window.agentcabinDesktop` directly.
 *
 *   import { platform } from "$lib/platform";
 *   await platform.dialog.open({ directory: true });
 *   await platform.shell.openExternal(url);
 *   platform.window.focus();
 */
import "./bridge"; // installs the Window.agentcabinDesktop ambient type

import * as app from "./app";
import * as browser from "./browser";
import * as dialog from "./dialog";
import * as path from "./path";
import * as shell from "./shell";
import * as window_ from "./window";

export const platform = {
  app,
  browser,
  dialog,
  path,
  shell,
  window: window_,
} as const;

// Re-exported so named imports also work: `import { open } from "$lib/platform/dialog"`.
export * from "./dialog-types";
