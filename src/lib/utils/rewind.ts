/** Rewind file-revert utilities. */

// ── Types ──

export interface RewindCandidate {
  cliUuid: string;
  content: string; // user message text
  ts: string; // ISO timestamp
  timelineIndex: number; // position in timeline
}

export interface RewindDryRunResult {
  canRewind: boolean;
  filesChanged?: string[];
  error?: string;
}

export interface RewindMarker {
  id: string;
  ts: string;
  targetContent: string; // truncated target message text
  filesReverted: string[];
}

// ── Parsing helpers ──

/** Extract the business payload from a control response envelope. */
export function unwrapControlPayload(resp: unknown): Record<string, unknown> | null {
  if (!resp || typeof resp !== "object") return null;
  const r = resp as Record<string, unknown>;
  // Standard path: resp.response is the business payload
  if (r.response && typeof r.response === "object") {
    return r.response as Record<string, unknown>;
  }
  // Fallback: treat top-level as business payload (CLI behavior change safety net)
  return r;
}

/** Check if the error/response indicates a missing checkpoint (can fallback to earlier UUID). */
export function isCheckpointNotFound(input: unknown): boolean {
  const pattern = /checkpoint|snapshot/i;
  // String form (Tauri IPC errors are often strings)
  if (typeof input === "string") {
    return pattern.test(input);
  }
  // Error object
  if (input instanceof Error) {
    return pattern.test(input.message);
  }
  // Object form
  if (input && typeof input === "object") {
    const r = input as Record<string, unknown>;
    // Unwrapped business payload: canRewind === false
    if (r.canRewind === false) return true;
    // Wrapped control response: check resp.response.canRewind
    const payload = unwrapControlPayload(r);
    if (payload && payload.canRewind === false) return true;
    // error field
    const errMsg = (r.error ?? payload?.error) as string | undefined;
    if (typeof errMsg === "string" && pattern.test(errMsg)) return true;
  }
  return false;
}

/** Parse dryRun response (strict: canRewind must be explicitly true or file list present). */
export function parseDryRunResult(raw: unknown): RewindDryRunResult {
  return parseRewindResponse(raw, true);
}

/** Parse execute response (lenient: subtype !== error and no error field → success). */
export function parseExecuteResult(raw: unknown): RewindDryRunResult {
  return parseRewindResponse(raw, false);
}

/** Internal parser. strict=true requires canRewind===true (dryRun),
 *  strict=false only fails on canRewind===false or error field (execute). */
function parseRewindResponse(raw: unknown, strict: boolean): RewindDryRunResult {
  // Check for error subtype first (backend may resolve without throwing)
  if (raw && typeof raw === "object" && (raw as Record<string, unknown>).subtype === "error") {
    return {
      canRewind: false,
      error: String((raw as Record<string, unknown>).error ?? "Unknown error"),
    };
  }
  const payload = unwrapControlPayload(raw);
  if (!payload) return { canRewind: false, error: "Invalid response" };
  // error field → always not rewindable
  const errMsg = typeof payload.error === "string" ? payload.error : undefined;
  if (errMsg) return { canRewind: false, error: errMsg };
  // Support both camelCase filesChanged and snake_case files_changed (Rust backend compat)
  const fc = Array.isArray(payload.filesChanged)
    ? (payload.filesChanged as string[])
    : Array.isArray(payload.files_changed)
      ? (payload.files_changed as string[])
      : undefined;
  if (strict) {
    // dryRun: canRewind === true → rewindable
    // Compat: no canRewind field but file list present (including empty array) and no error → also rewindable
    // Assumption: CLI returns canRewind=false or error when not rewindable, never just an empty file list
    const canRewind =
      payload.canRewind === true || (payload.canRewind === undefined && fc !== undefined);
    return { canRewind, filesChanged: fc };
  }
  // execute: only explicit false = failure, everything else (true / undefined / missing) = success
  return {
    canRewind: payload.canRewind !== false,
    filesChanged: fc,
  };
}

/** Check if a dryRun error indicates "CLI doesn't support dry_run" (safe to skip preview),
 *  vs hard failure (session dead/timeout — should NOT retry).
 *  Requires "dry_run" / "dry run" to appear, or the specific "unsupported control subtype" phrase.
 *  Standalone "unsupported" is intentionally excluded to avoid false positives. */
export function isDryRunUnsupported(err: unknown): boolean {
  const msg = err instanceof Error ? err.message : typeof err === "string" ? err : "";
  return /unsupported control subtype|unknown.*subtype|unknown.*command|dry.?run/i.test(msg);
}

/** Check if an execute error indicates the CLI doesn't support the `files` parameter (can degrade to full rewind).
 *  Compatible with string / Error / { error: string } / { response: { error: string } }. */
export function isFilesParamUnsupported(err: unknown): boolean {
  const pattern =
    /unknown.*(field|argument|option).*files|unsupported.*files|unexpected.*files|files.*not.*supported|invalid.*option.*files/i;
  if (typeof err === "string") return pattern.test(err);
  if (err instanceof Error) return pattern.test(err.message);
  if (err && typeof err === "object") {
    const r = err as Record<string, unknown>;
    const errMsg = (r.error ??
      (unwrapControlPayload(r) as Record<string, unknown> | null)?.error) as string | undefined;
    if (typeof errMsg === "string") return pattern.test(errMsg);
  }
  return false;
}
