/**
 * Minimal ExtensionAPI shim for host-side programmatic loading of Pi extensions.
 * Provides defineTool and type definitions required by @injaneity/pi-computer-use.
 */
export function defineTool(tool) {
  return tool;
}

export default {
  defineTool,
};
