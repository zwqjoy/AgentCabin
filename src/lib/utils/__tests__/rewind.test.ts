import { describe, it, expect } from "vitest";
import {
  unwrapControlPayload,
  isCheckpointNotFound,
  parseDryRunResult,
  parseExecuteResult,
  isDryRunUnsupported,
  isFilesParamUnsupported,
} from "../rewind";

describe("unwrapControlPayload", () => {
  it("extracts nested response", () => {
    const raw = {
      subtype: "success",
      request_id: "x",
      response: { canRewind: true, filesChanged: ["a.ts"] },
    };
    expect(unwrapControlPayload(raw)).toEqual({ canRewind: true, filesChanged: ["a.ts"] });
  });

  it("falls back to top-level if no response field", () => {
    const raw = { canRewind: false, error: "no snapshot" };
    expect(unwrapControlPayload(raw)).toEqual(raw);
  });

  it("returns null for non-object", () => {
    expect(unwrapControlPayload(null)).toBeNull();
    expect(unwrapControlPayload("string")).toBeNull();
  });
});

describe("isCheckpointNotFound", () => {
  it("detects canRewind: false in unwrapped payload", () => {
    expect(isCheckpointNotFound({ canRewind: false })).toBe(true);
  });

  it("detects canRewind: false in control response envelope", () => {
    expect(isCheckpointNotFound({ subtype: "success", response: { canRewind: false } })).toBe(true);
  });

  it("detects checkpoint keyword in string error", () => {
    expect(isCheckpointNotFound("No checkpoint found")).toBe(true);
    expect(isCheckpointNotFound("Timeout waiting for control response")).toBe(false);
  });

  it("detects checkpoint keyword in Error object", () => {
    expect(isCheckpointNotFound(new Error("snapshot not found"))).toBe(true);
    expect(isCheckpointNotFound(new Error("Actor dead"))).toBe(false);
  });

  it("detects checkpoint keyword in error field", () => {
    expect(isCheckpointNotFound({ error: "no snapshot for this message" })).toBe(true);
  });

  it("returns false for unrelated errors", () => {
    expect(isCheckpointNotFound({ subtype: "error", error: "permission denied" })).toBe(false);
    expect(isCheckpointNotFound("session not found")).toBe(false);
  });

  it("returns false for canRewind: true", () => {
    expect(isCheckpointNotFound({ canRewind: true })).toBe(false);
  });
});

describe("parseDryRunResult (strict mode)", () => {
  it("parses successful dryRun with filesChanged", () => {
    const raw = {
      subtype: "success",
      request_id: "x",
      response: { canRewind: true, filesChanged: ["a.ts", "b.ts"] },
    };
    const result = parseDryRunResult(raw);
    expect(result.canRewind).toBe(true);
    expect(result.filesChanged).toEqual(["a.ts", "b.ts"]);
  });

  it("parses error subtype", () => {
    const raw = { subtype: "error", error: "No checkpoint found" };
    expect(parseDryRunResult(raw).canRewind).toBe(false);
    expect(parseDryRunResult(raw).error).toBe("No checkpoint found");
  });

  it("parses canRewind: false", () => {
    const raw = { subtype: "success", response: { canRewind: false } };
    expect(parseDryRunResult(raw).canRewind).toBe(false);
  });

  it("treats error field as canRewind: false even without explicit canRewind", () => {
    const raw = { subtype: "success", response: { error: "some error" } };
    expect(parseDryRunResult(raw).canRewind).toBe(false);
    expect(parseDryRunResult(raw).error).toBe("some error");
  });

  it("requires canRewind or file list in strict mode (empty response = false)", () => {
    const raw = { subtype: "success", response: {} };
    expect(parseDryRunResult(raw).canRewind).toBe(false);
  });

  it("treats presence of file list without canRewind as success (CLI compat)", () => {
    const raw = { subtype: "success", response: { filesChanged: ["a.ts"] } };
    expect(parseDryRunResult(raw).canRewind).toBe(true);
  });

  it("treats empty file list without canRewind as success (no-change rewind)", () => {
    const raw = { subtype: "success", response: { filesChanged: [] } };
    expect(parseDryRunResult(raw).canRewind).toBe(true);
    expect(parseDryRunResult(raw).filesChanged).toEqual([]);
  });

  it("handles null/undefined", () => {
    expect(parseDryRunResult(null).canRewind).toBe(false);
    expect(parseDryRunResult(undefined).canRewind).toBe(false);
  });

  it("supports snake_case files_changed from Rust backend", () => {
    const raw = { subtype: "success", response: { canRewind: true, files_changed: ["x.rs"] } };
    expect(parseDryRunResult(raw).filesChanged).toEqual(["x.rs"]);
  });

  it("prefers camelCase filesChanged over snake_case", () => {
    const raw = {
      subtype: "success",
      response: { canRewind: true, filesChanged: ["a.ts"], files_changed: ["b.rs"] },
    };
    expect(parseDryRunResult(raw).filesChanged).toEqual(["a.ts"]);
  });
});

describe("parseExecuteResult (lenient mode)", () => {
  it("treats absent canRewind as success", () => {
    const raw = { subtype: "success", response: {} };
    expect(parseExecuteResult(raw).canRewind).toBe(true);
  });

  it("treats canRewind: true as success", () => {
    const raw = { subtype: "success", response: { canRewind: true } };
    expect(parseExecuteResult(raw).canRewind).toBe(true);
  });

  it("treats canRewind: false as failure", () => {
    const raw = { subtype: "success", response: { canRewind: false } };
    expect(parseExecuteResult(raw).canRewind).toBe(false);
  });

  it("treats error subtype as failure", () => {
    const raw = { subtype: "error", error: "session dead" };
    expect(parseExecuteResult(raw).canRewind).toBe(false);
  });

  it("treats error field as failure", () => {
    const raw = { subtype: "success", response: { error: "oops" } };
    expect(parseExecuteResult(raw).canRewind).toBe(false);
  });
});

describe("isDryRunUnsupported", () => {
  it("detects unsupported control subtype errors", () => {
    expect(isDryRunUnsupported("unsupported control subtype")).toBe(true);
    expect(isDryRunUnsupported("unsupported control subtype: dry_run")).toBe(true);
    expect(isDryRunUnsupported(new Error("unknown command: dry_run"))).toBe(true);
  });

  it("detects dry_run keyword in error messages", () => {
    expect(isDryRunUnsupported("dry_run is not supported")).toBe(true);
    expect(isDryRunUnsupported("dry run not available")).toBe(true);
  });

  it("returns false for standalone 'unsupported' without dry_run context", () => {
    expect(isDryRunUnsupported("unsupported model")).toBe(false);
    expect(isDryRunUnsupported("unsupported API version")).toBe(false);
  });

  it("returns false for hard failures", () => {
    expect(isDryRunUnsupported("Actor dead")).toBe(false);
    expect(isDryRunUnsupported("Timeout waiting for control response")).toBe(false);
    expect(isDryRunUnsupported(new Error("session not found"))).toBe(false);
  });
});

describe("isFilesParamUnsupported", () => {
  it("detects string errors with files keyword", () => {
    expect(isFilesParamUnsupported("unknown field: files")).toBe(true);
    expect(isFilesParamUnsupported("unsupported parameter: files")).toBe(true);
    expect(isFilesParamUnsupported("files not supported in this version")).toBe(true);
    expect(isFilesParamUnsupported("unknown argument: files")).toBe(true);
    expect(isFilesParamUnsupported("invalid option: files")).toBe(true);
  });

  it("detects Error objects", () => {
    expect(isFilesParamUnsupported(new Error("unexpected field: files"))).toBe(true);
  });

  it("detects object error body (Tauri IPC)", () => {
    expect(isFilesParamUnsupported({ error: "unknown field: files" })).toBe(true);
  });

  it("detects nested response.error (control response envelope)", () => {
    expect(
      isFilesParamUnsupported({
        subtype: "error",
        response: { error: "unsupported files param" },
      }),
    ).toBe(true);
  });

  it("returns false for unrelated errors", () => {
    expect(isFilesParamUnsupported("Actor dead")).toBe(false);
    expect(isFilesParamUnsupported("session not found")).toBe(false);
    expect(isFilesParamUnsupported("No checkpoint found")).toBe(false);
    expect(isFilesParamUnsupported({ error: "permission denied" })).toBe(false);
  });
});

describe("resolve-path dryRunSkipped integration", () => {
  it("parseDryRunResult + isDryRunUnsupported detects non-exception dry_run unsupported", () => {
    // Old CLI returns subtype:"error" (resolve doesn't throw) for unsupported dry_run
    const raw = { subtype: "error", error: "unsupported control subtype: dry_run" };
    const result = parseDryRunResult(raw);
    expect(result.canRewind).toBe(false);
    expect(result.error).toBe("unsupported control subtype: dry_run");
    // selectCheckpoint logic: !canRewind && error && isDryRunUnsupported(error) → dryRunSkipped
    expect(isDryRunUnsupported(result.error!)).toBe(true);
  });

  it("does NOT dryRunSkip for hard errors in resolve path", () => {
    const raw = { subtype: "error", error: "session not found" };
    const result = parseDryRunResult(raw);
    expect(result.canRewind).toBe(false);
    expect(isDryRunUnsupported(result.error!)).toBe(false);
  });
});
