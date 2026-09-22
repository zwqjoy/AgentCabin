/** Session-level file state cache for Explorer — survives component remount. */
const fileStateCache = new Map<string, string>();

/** Get cached selected file path for a project cwd. */
export function getCachedFile(cwd: string): string | undefined {
  return fileStateCache.get(cwd);
}

/** Cache the selected file path for a project cwd. */
export function setCachedFile(cwd: string, filePath: string): void {
  if (cwd && filePath) fileStateCache.set(cwd, filePath);
}

/** Clear cached file for a project cwd (e.g. after restore failure). */
export function clearCachedFile(cwd: string): void {
  fileStateCache.delete(cwd);
}
