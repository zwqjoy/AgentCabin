import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { lstatSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const extensionsDir = join(root, "src-tauri/runtime/extensions");
const npmCacheDir = join(root, ".agentcabin-build-cache/npm");
const npmCommand = process.platform === "win32" ? "npm.cmd" : "npm";

execFileSync(
  npmCommand,
  ["ci", "--omit=dev", "--ignore-scripts", "--no-audit", "--no-fund", "--legacy-peer-deps"],
  {
    cwd: extensionsDir,
    env: { ...process.env, npm_config_cache: npmCacheDir },
    stdio: "inherit",
  },
);

const removedNativeDirectories = [];
const nonDarwinPlatform = /(?:^|-)(?:win32|linux|android|freebsd|openbsd|sunos|aix)(?:-|$)/;
const nonArmDarwinPlatform = /(?:^|-)darwin-(?:x64|universal)(?:-|$)/;

function containsNativeBinary(path) {
  return readdirSync(path, { withFileTypes: true }).some((entry) => {
    if (entry.isFile()) return entry.name.endsWith(".node");
    return entry.isDirectory() && containsNativeBinary(join(path, entry.name));
  });
}

function pruneNonArmNativeDirectories(path) {
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const child = join(path, entry.name);
    const platformSpecific =
      nonDarwinPlatform.test(entry.name.toLowerCase()) ||
      nonArmDarwinPlatform.test(entry.name.toLowerCase());
    if (platformSpecific && containsNativeBinary(child)) {
      removedNativeDirectories.push(relative(extensionsDir, child));
      rmSync(child, { recursive: true, force: true });
    } else {
      pruneNonArmNativeDirectories(child);
    }
  }
}

pruneNonArmNativeDirectories(join(extensionsDir, "node_modules"));

function hashTree(path, output = {}) {
  for (const name of readdirSync(path).sort()) {
    const child = join(path, name);
    const stat = lstatSync(child);
    if (stat.isDirectory()) {
      hashTree(child, output);
    } else if (stat.isFile()) {
      output[relative(extensionsDir, child)] = createHash("sha256")
        .update(readFileSync(child))
        .digest("hex");
    }
  }
  return output;
}

const manifest = JSON.parse(readFileSync(join(extensionsDir, "package.json"), "utf8"));
const files = hashTree(join(extensionsDir, "node_modules"));
const integrity = {
  schemaVersion: 1,
  extensions: manifest.dependencies,
  target: "aarch64-apple-darwin",
  removedNativeDirectories: removedNativeDirectories.sort(),
  files,
};
writeFileSync(join(extensionsDir, "integrity.json"), `${JSON.stringify(integrity, null, 2)}\n`);
console.log(
  `Prepared ${Object.keys(manifest.dependencies).length} pinned Pi extensions (${Object.keys(files).length} files, ${removedNativeDirectories.length} non-arm64 native directories removed).`,
);
