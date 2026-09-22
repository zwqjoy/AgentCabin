import { getTransport } from "$lib/transport";

export interface DshPluginManifest {
  id: string;
  name: string;
  package?: string;
  path?: string;
  version?: string;
  description?: string;
  enabled: boolean;
  safeInWork: boolean;
  security: "safeInWork" | "codeOnly";
  config?: unknown;
}

export interface DshPluginRegistration {
  id: string;
  name: string;
  package?: string;
  path?: string;
  version?: string;
  description?: string;
  enabled?: boolean;
  security?: "safeInWork" | "codeOnly";
  config?: unknown;
}

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return getTransport().invoke<T>(cmd, args);
}

export function listDshPlugins(): Promise<DshPluginManifest[]> {
  return invoke<DshPluginManifest[]>("list_dsh_plugins");
}

export function toggleDshPlugin(pluginId: string, enabled: boolean): Promise<DshPluginManifest> {
  return invoke<DshPluginManifest>("toggle_dsh_plugin", { pluginId, enabled });
}

export function registerDshPlugin(registration: DshPluginRegistration): Promise<DshPluginManifest> {
  return invoke<DshPluginManifest>("register_dsh_plugin", { registration });
}

export function unregisterDshPlugin(pluginId: string): Promise<void> {
  return invoke<void>("unregister_dsh_plugin", { pluginId });
}

export function updateDshPlugin(pluginId: string): Promise<DshPluginManifest> {
  return invoke<DshPluginManifest>("update_dsh_plugin", { pluginId });
}
