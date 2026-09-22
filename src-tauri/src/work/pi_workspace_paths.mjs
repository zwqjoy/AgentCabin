import fs from "node:fs";
import path from "node:path";

export const WORKSPACE_AREAS = new Set(["input", "scratch", "output", "context"]);
const READABLE_AREAS = new Set(WORKSPACE_AREAS);
const WRITABLE_AREAS = new Set(["scratch", "output"]);

export function isInside(root, candidate) {
  return candidate === root || candidate.startsWith(`${root}${path.sep}`);
}

export function realTarget(candidate) {
  let current = candidate;
  const suffix = [];
  while (!fs.existsSync(current)) {
    const parent = path.dirname(current);
    if (parent === current) throw new Error("Cannot resolve Workspace path");
    suffix.unshift(path.basename(current));
    current = parent;
  }
  return suffix.reduce((value, part) => path.join(value, part), fs.realpathSync(current));
}

function parseRoots(value) {
  if (Array.isArray(value)) return value;
  if (typeof value !== "string" || !value.trim()) return [];
  try {
    const parsed = JSON.parse(value);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function readPermissionModeFromFile(permissionPath) {
  if (!permissionPath) return "";
  try {
    const value = JSON.parse(fs.readFileSync(permissionPath, "utf8"));
    return String(
      value?.policy?.executionMode ??
        value?.policy?.execution_mode ??
        value?.permission_mode ??
        value?.permissionMode ??
        "",
    ).trim();
  } catch {
    return "";
  }
}

/** Read the live Work permission selection without relying on process env refreshes. */
export function workExecutionMode() {
  const fromFile = readPermissionModeFromFile(
    String(process.env.AGENTCABIN_WORK_PERMISSION_PATH || "").trim(),
  );
  if (fromFile) return fromFile;

  try {
    const inline = JSON.parse(String(process.env.AGENTCABIN_WORK_POLICY || ""));
    const mode = String(inline?.executionMode ?? inline?.execution_mode ?? "").trim();
    if (mode) return mode;
  } catch {
    // Fall through to the explicit mode environment variable.
  }

  return String(process.env.AGENTCABIN_WORK_EXECUTION_MODE || "").trim();
}

export function isWorkFullAccess() {
  return ["full_access", "fullAccess", "bypass", "bypassPermissions"].includes(
    workExecutionMode(),
  );
}

function normalizeRoot(root) {
  const rawPath = String(root?.path || "").trim();
  if (!rawPath || !path.isAbsolute(rawPath)) return null;
  try {
    const resolved = fs.realpathSync(rawPath);
    if (!fs.statSync(resolved).isDirectory()) return null;
    return { path: resolved, writable: Boolean(root?.writable) };
  } catch {
    return null;
  }
}

/** Read the latest persisted Work access roots so a running session sees UI changes. */
export function loadWorkAccessRoots({ manifestPath, fallback } = {}) {
  let roots = [];
  let manifestLoaded = false;
  if (manifestPath) {
    try {
      const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
      roots = manifest?.accessRoots ?? manifest?.access_roots ?? [];
      manifestLoaded = true;
    } catch {
      roots = [];
    }
  }
  if (!manifestLoaded || !Array.isArray(roots)) {
    roots = parseRoots(fallback);
  }
  const unique = new Map();
  for (const root of roots) {
    const normalized = normalizeRoot(root);
    if (normalized) unique.set(normalized.path, normalized);
  }
  return [...unique.values()];
}

/** Read the persisted output placement for the current Workspace. */
export function readArtifactStorageMode(managedStateRoot) {
  const root = String(managedStateRoot || "").trim();
  if (!root) return "managed";
  try {
    const manifest = JSON.parse(fs.readFileSync(path.join(root, "manifest.json"), "utf8"));
    return String(
      manifest?.artifactStorageMode ?? manifest?.artifact_storage_mode ?? "managed",
    ).trim() || "managed";
  } catch {
    return "managed";
  }
}

function resolveWorkspaceRoot(workspaceRoot) {
  const root = fs.realpathSync(workspaceRoot);
  if (!fs.statSync(root).isDirectory()) throw new Error("AgentCabin Work Workspace is not a directory");
  return root;
}

function normalizeTrustedReadRoots(roots) {
  return roots
    .map((candidate) => String(candidate || "").trim())
    .filter(Boolean)
    .map((candidate) => {
      try {
        const resolved = fs.realpathSync(candidate);
        return fs.statSync(resolved).isDirectory() ? resolved : null;
      } catch {
        return null;
      }
    })
    .filter(Boolean);
}

/** Resolve a Workspace-relative path, an authorized external path, or a trusted read-only path. */
export function normalizeWorkPath(
  rawPath,
  {
    workspaceRoot,
    accessRoots = [],
    trustedReadRoots = [],
    writable = false,
    allowUnrestricted = false,
    managedStateRoot,
    artifactStorageMode,
  } = {},
) {
  const root = resolveWorkspaceRoot(workspaceRoot);
  const raw = String(rawPath || "").trim().replace(/\\+/g, "/");
  if (!raw || raw.includes("://")) throw new Error("Workspace path must be relative or an authorized absolute path");

  const managedStateDir = String(
    managedStateRoot || process.env.AGENTCABIN_MANAGED_STATE_DIR || "",
  ).trim();
  let managedCanonical = "";
  if (managedStateDir) {
    try {
      managedCanonical = fs.realpathSync(managedStateDir);
    } catch {
      managedCanonical = path.resolve(managedStateDir);
    }
  }
  const isLocalFolderProject = Boolean(managedCanonical) && managedCanonical !== root;
  const storageMode = String(
    artifactStorageMode || readArtifactStorageMode(managedCanonical),
  ).trim();
  const directOutput = isLocalFolderProject && storageMode === "primary_work_root";

  let absolute;
  if (path.isAbsolute(raw) || /^[A-Za-z]:\//.test(raw)) {
    absolute = path.resolve(raw);
  } else {
    const normalized = path.posix.normalize(raw).replace(/^\.\//, "");
    if (!normalized || normalized === "." || normalized === ".." || normalized.startsWith("../")) {
      throw new Error("Workspace path cannot escape the Workspace");
    }

    if (isLocalFolderProject) {
      const firstSegment = normalized.split("/")[0];
      if (WORKSPACE_AREAS.has(firstSegment)) {
        // input/, scratch/, and context/ stay managed. An explicit direct-save
        // Workspace maps only output/ to the selected local project root.
        absolute = directOutput && firstSegment === "output"
          ? path.resolve(root, normalized)
          : path.resolve(managedCanonical, normalized);
      } else {
        if (allowUnrestricted) {
          absolute = path.resolve(root, raw);
        } else {
          const inWorkspace = path.resolve(root, normalized);
          if (fs.existsSync(inWorkspace) || accessRoots.length === 0) {
            absolute = inWorkspace;
          } else {
            const matchingRoot = accessRoots.find((ar) => fs.existsSync(path.resolve(ar.path, normalized)));
            absolute = matchingRoot ? path.resolve(matchingRoot.path, normalized) : inWorkspace;
          }
        }
      }
    } else {
      if (allowUnrestricted) {
        absolute = path.resolve(root, raw);
      } else {
        const inWorkspace = path.resolve(root, normalized);
        if (fs.existsSync(inWorkspace) || accessRoots.length === 0) {
          absolute = inWorkspace;
        } else {
          const matchingRoot = accessRoots.find((ar) => fs.existsSync(path.resolve(ar.path, normalized)));
          if (matchingRoot) {
            absolute = path.resolve(matchingRoot.path, normalized);
          } else {
            absolute = inWorkspace;
          }
        }
      }
    }
  }

  const resolved = realTarget(absolute);

  if (directOutput && isInside(root, resolved)) {
    const relative = path.relative(root, resolved).split(path.sep).join("/");
    if (relative === "output" || relative.startsWith("output/")) {
      return {
        scope: "primary_output",
        root,
        absolute: resolved,
        relative,
        area: "output",
      };
    }
  }

  if (isLocalFolderProject && managedCanonical && isInside(managedCanonical, resolved)) {
    const relative = path.relative(managedCanonical, resolved).split(path.sep).join("/");
    const area = relative ? relative.split("/")[0] : "managed_state";
    if (WORKSPACE_AREAS.has(area)) {
      if (writable && !WRITABLE_AREAS.has(area) && !allowUnrestricted) {
        throw new Error("Only scratch and output are writable by Work tools");
      }
      if (!writable && !READABLE_AREAS.has(area) && !allowUnrestricted) {
        throw new Error("Workspace area is not readable");
      }
      return {
        scope: "managed_state",
        root: managedCanonical,
        absolute: resolved,
        relative,
        area,
      };
    }
    return {
      scope: "managed_state",
      root: managedCanonical,
      absolute: resolved,
      relative,
      area: "managed_state",
    };
  }

  if (isInside(root, resolved)) {
    const relative = path.relative(root, resolved);
    if (!relative) {
      return {
        scope: "workspace",
        root,
        absolute: resolved,
        relative: "",
        area: "workspace",
      };
    }

    if (isLocalFolderProject) {
      return {
        scope: "workspace",
        root,
        absolute: resolved,
        relative: relative.split(path.sep).join("/"),
        area: "project",
      };
    }

    const area = relative.split(path.sep)[0];
    if (!WORKSPACE_AREAS.has(area)) {
      if (allowUnrestricted) {
        return {
          scope: "workspace",
          root,
          absolute: resolved,
          relative: relative.split(path.sep).join("/"),
          area: "workspace",
        };
      }
      throw new Error("Workspace path must start with input, scratch, output, or context");
    }
    if (writable && !WRITABLE_AREAS.has(area) && !allowUnrestricted) {
      throw new Error("Only scratch and output are writable by Work tools");
    }
    if (!writable && !READABLE_AREAS.has(area) && !allowUnrestricted) {
      throw new Error("Workspace area is not readable");
    }
    return {
      scope: "workspace",
      root,
      absolute: resolved,
      relative: relative.split(path.sep).join("/"),
      area,
    };
  }

  if (allowUnrestricted) {
    return {
      scope: "full_access",
      root: path.parse(resolved).root,
      absolute: resolved,
      relative: resolved,
      area: "full_access",
    };
  }

  if (!writable) {
    const trustedRoot = normalizeTrustedReadRoots(trustedReadRoots).find((candidate) =>
      isInside(candidate, resolved),
    );
    if (trustedRoot) {
      return {
        scope: "trusted",
        root: trustedRoot,
        absolute: resolved,
        relative: resolved,
        area: "trusted",
      };
    }
  }

  const accessRoot = accessRoots.find((candidate) => isInside(candidate.path, resolved));
  if (!accessRoot) {
    throw new Error("Path is outside the Workspace and has not been granted to Work");
  }
  if (writable && !accessRoot.writable) {
    throw new Error("This external directory is read-only");
  }

  return {
    scope: "external",
    root: accessRoot.path,
    absolute: resolved,
    relative: resolved,
    area: "external",
  };
}
