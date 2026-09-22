import type { WorkRuntimeClient } from "./types";
import { piWorkRuntimeClient } from "./pi";
import { dshWorkRuntimeClient } from "./dsh";
import { createReadOnlyWorkRuntimeClient } from "./read-only";

export * from "./types";
export * from "./pi";
export * from "./dsh";
export * from "./read-only";
export * from "./fake";

export class UnsupportedWorkRuntimeError extends Error {
  constructor(public readonly provider?: string) {
    super(`Work Runtime 不支持 '${provider}' 运行时适配器。当前没有可用的 Work Runtime adapter。`);
    this.name = "UnsupportedWorkRuntimeError";
  }
}

export function getWorkRuntimeClient(provider?: string): WorkRuntimeClient {
  const normalized = provider?.trim().toLowerCase();
  if (!normalized || normalized === "pi") {
    return piWorkRuntimeClient;
  }
  if (normalized === "dsh") {
    return dshWorkRuntimeClient;
  }
  throw new UnsupportedWorkRuntimeError(provider);
}

/**
 * Keep Work provider selection driven by the registered client adapters.
 * Settings and capability UI should not maintain a second Pi-only allowlist.
 */
export function isWorkRuntimeSupported(provider?: string): boolean {
  try {
    getWorkRuntimeClient(provider);
    return true;
  } catch {
    return false;
  }
}

export function getWorkRuntimeClientOrReadOnly(provider?: string): WorkRuntimeClient {
  try {
    return getWorkRuntimeClient(provider);
  } catch {
    return createReadOnlyWorkRuntimeClient(provider);
  }
}
