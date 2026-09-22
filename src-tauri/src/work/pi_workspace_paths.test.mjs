import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  isWorkFullAccess,
  loadWorkAccessRoots,
  normalizeWorkPath,
} from "./pi_workspace_paths.mjs";

test("authorized external roots are readable but not writable when marked read-only", () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-paths-"));
  const workspaceRoot = path.join(temp, "workspace");
  const externalRoot = path.join(temp, "external");
  const manifestPath = path.join(workspaceRoot, "manifest.json");
  fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
  fs.mkdirSync(externalRoot, { recursive: true });
  fs.writeFileSync(path.join(externalRoot, "notes.md"), "authorized\n", "utf8");
  fs.writeFileSync(
    manifestPath,
    JSON.stringify({ accessRoots: [{ path: externalRoot, writable: false }] }),
    "utf8",
  );

  try {
    const roots = loadWorkAccessRoots({ manifestPath });
    const file = normalizeWorkPath(path.join(externalRoot, "notes.md"), {
      workspaceRoot,
      accessRoots: roots,
    });
    assert.equal(file.scope, "external");
    assert.equal(file.absolute, fs.realpathSync(path.join(externalRoot, "notes.md")));
    assert.throws(
      () =>
        normalizeWorkPath(path.join(externalRoot, "new.md"), {
          workspaceRoot,
          accessRoots: roots,
          writable: true,
        }),
      /read-only/,
    );
    assert.throws(
      () =>
        normalizeWorkPath(path.join(temp, "unapproved", "secret.md"), {
          workspaceRoot,
          accessRoots: roots,
        }),
      /not been granted/,
    );
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("FullAccess resolves an unregistered absolute host path", () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-full-access-"));
  const workspaceRoot = path.join(temp, "workspace");
  const permissionPath = path.join(temp, "permission.json");
  const outsideRoot = path.join(temp, "outside");
  fs.mkdirSync(path.join(workspaceRoot, "input"), { recursive: true });
  fs.mkdirSync(outsideRoot, { recursive: true });
  fs.writeFileSync(
    permissionPath,
    JSON.stringify({ policy: { executionMode: "full_access" } }),
    "utf8",
  );

  const previousPermissionPath = process.env.AGENTCABIN_WORK_PERMISSION_PATH;
  process.env.AGENTCABIN_WORK_PERMISSION_PATH = permissionPath;
  try {
    assert.equal(isWorkFullAccess(), true);
    const target = normalizeWorkPath(path.join(outsideRoot, "new.txt"), {
      workspaceRoot,
      allowUnrestricted: isWorkFullAccess(),
      writable: true,
    });
    assert.equal(target.scope, "full_access");
    assert.equal(target.absolute, path.join(fs.realpathSync(outsideRoot), "new.txt"));

    const inWorkspaceOutput = normalizeWorkPath("output/report.md", {
      workspaceRoot,
      allowUnrestricted: isWorkFullAccess(),
      writable: true,
    });
    assert.equal(inWorkspaceOutput.scope, "workspace");
    assert.equal(inWorkspaceOutput.area, "output");
    assert.equal(inWorkspaceOutput.relative, "output/report.md");
  } finally {
    if (previousPermissionPath === undefined) {
      delete process.env.AGENTCABIN_WORK_PERMISSION_PATH;
    } else {
      process.env.AGENTCABIN_WORK_PERMISSION_PATH = previousPermissionPath;
    }
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("LocalFolder workspace allows reading and writing project files without area prefixes and routes reserved areas to managed state", () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-local-folder-"));
  const userProjectRoot = path.join(temp, "MyProject");
  const managedStateDir = path.join(temp, "agentcabin_state", "workspaces", "ws-123");
  fs.mkdirSync(path.join(userProjectRoot, "src"), { recursive: true });
  fs.mkdirSync(path.join(managedStateDir, "input"), { recursive: true });
  fs.mkdirSync(path.join(managedStateDir, "scratch"), { recursive: true });
  fs.mkdirSync(path.join(managedStateDir, "output"), { recursive: true });
  fs.mkdirSync(path.join(managedStateDir, "context"), { recursive: true });
  fs.writeFileSync(path.join(userProjectRoot, "src", "index.ts"), "export const x = 1;\n", "utf8");
  fs.writeFileSync(path.join(managedStateDir, "input", "report.pdf"), "binary-pdf-content", "utf8");
  fs.writeFileSync(path.join(managedStateDir, "context", "decisions.md"), "# Decisions\n", "utf8");

  process.env.AGENTCABIN_MANAGED_STATE_DIR = managedStateDir;
  try {
    // 1. Relative project file is writable in local folder project (area: project)
    const projectFile = normalizeWorkPath("src/index.ts", {
      workspaceRoot: userProjectRoot,
      writable: true,
    });
    assert.equal(projectFile.scope, "workspace");
    assert.equal(projectFile.area, "project");
    assert.equal(projectFile.absolute, fs.realpathSync(path.join(userProjectRoot, "src", "index.ts")));

    // 2. New file in project root is writable (area: project)
    const newFile = normalizeWorkPath("README.md", {
      workspaceRoot: userProjectRoot,
      writable: true,
    });
    assert.equal(newFile.scope, "workspace");
    assert.equal(newFile.area, "project");
    assert.equal(newFile.absolute, path.join(fs.realpathSync(userProjectRoot), "README.md"));

    // 3. Relative input/ path routes to managed state (scope: managed_state, area: input)
    const inputFile = normalizeWorkPath("input/report.pdf", {
      workspaceRoot: userProjectRoot,
      writable: false,
    });
    assert.equal(inputFile.scope, "managed_state");
    assert.equal(inputFile.area, "input");
    assert.equal(inputFile.relative, "input/report.pdf");
    assert.equal(inputFile.absolute, fs.realpathSync(path.join(managedStateDir, "input", "report.pdf")));

    // 4. Relative scratch/ path routes to managed state and is writable
    const scratchRel = normalizeWorkPath("scratch/temp.txt", {
      workspaceRoot: userProjectRoot,
      writable: true,
    });
    assert.equal(scratchRel.scope, "managed_state");
    assert.equal(scratchRel.area, "scratch");
    assert.equal(scratchRel.relative, "scratch/temp.txt");
    assert.equal(scratchRel.absolute, path.join(fs.realpathSync(managedStateDir), "scratch", "temp.txt"));

    // 5. Relative output/ path routes to managed state and is writable
    const outputRel = normalizeWorkPath("output/report.pptx", {
      workspaceRoot: userProjectRoot,
      writable: true,
    });
    assert.equal(outputRel.scope, "managed_state");
    assert.equal(outputRel.area, "output");
    assert.equal(outputRel.relative, "output/report.pptx");
    assert.equal(outputRel.absolute, path.join(fs.realpathSync(managedStateDir), "output", "report.pptx"));

    // 6. Relative context/ path routes to managed state
    const contextRel = normalizeWorkPath("context/decisions.md", {
      workspaceRoot: userProjectRoot,
      writable: false,
    });
    assert.equal(contextRel.scope, "managed_state");
    assert.equal(contextRel.area, "context");
    assert.equal(contextRel.relative, "context/decisions.md");
    assert.equal(contextRel.absolute, fs.realpathSync(path.join(managedStateDir, "context", "decisions.md")));

    // 7. Explicit options.managedStateRoot overrides env
    const explicitManaged = normalizeWorkPath("output/summary.csv", {
      workspaceRoot: userProjectRoot,
      managedStateRoot: managedStateDir,
      writable: true,
    });
    assert.equal(explicitManaged.scope, "managed_state");
    assert.equal(explicitManaged.area, "output");
  } finally {
    delete process.env.AGENTCABIN_MANAGED_STATE_DIR;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("LocalFolder direct artifact storage routes output to the selected project root", () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-work-direct-output-"));
  const userProjectRoot = path.join(temp, "MyProject");
  const managedStateDir = path.join(temp, "agentcabin_state", "workspaces", "ws-123");
  fs.mkdirSync(userProjectRoot, { recursive: true });
  fs.mkdirSync(path.join(managedStateDir, "scratch"), { recursive: true });
  fs.writeFileSync(
    path.join(managedStateDir, "manifest.json"),
    JSON.stringify({
      rootKind: "local_folder",
      primaryWorkRoot: userProjectRoot,
      artifactStorageMode: "primary_work_root",
    }),
    "utf8",
  );

  process.env.AGENTCABIN_MANAGED_STATE_DIR = managedStateDir;
  try {
    const output = normalizeWorkPath("output/report.md", {
      workspaceRoot: userProjectRoot,
      writable: true,
    });
    assert.equal(output.scope, "primary_output");
    assert.equal(output.area, "output");
    assert.equal(output.relative, "output/report.md");
    assert.equal(output.absolute, path.join(fs.realpathSync(userProjectRoot), "output", "report.md"));

    const scratch = normalizeWorkPath("scratch/checkpoint.md", {
      workspaceRoot: userProjectRoot,
      writable: true,
    });
    assert.equal(scratch.scope, "managed_state");
    assert.equal(scratch.absolute, path.join(fs.realpathSync(managedStateDir), "scratch", "checkpoint.md"));
  } finally {
    delete process.env.AGENTCABIN_MANAGED_STATE_DIR;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});
