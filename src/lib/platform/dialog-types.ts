/**
 * Platform abstraction: the only place that knows the app runs in
 * Tauri, Electron, or a plain browser.
 *
 * Pages and components must import from `$lib/platform` and never import
 * `@tauri-apps/*` (or read `window.agentcabinDesktop`) directly. Signatures
 * mirror the Tauri plugin APIs so existing call sites keep working.
 */

// Dialog option shapes mirror @tauri-apps/plugin-dialog v2.

export interface DialogFilter {
  name: string;
  /** Extensions without the leading dot, e.g. ["md", "json"]. */
  extensions: string[];
}

export interface OpenDialogOptions {
  title?: string;
  filters?: DialogFilter[];
  defaultPath?: string;
  multiple?: boolean;
  directory?: boolean;
}

export interface SaveDialogOptions {
  title?: string;
  filters?: DialogFilter[];
  defaultPath?: string;
}

export type ConfirmKind = "info" | "warning" | "error";

export interface ConfirmOptions {
  title?: string;
  kind?: ConfirmKind;
}
