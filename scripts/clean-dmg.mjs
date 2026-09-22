#!/usr/bin/env node

/**
 * Clean leftover DMG artifacts before building.
 *
 * bundle_dmg.sh's `hdiutil convert` has no -ov (overwrite) flag,
 * so existing DMG files from previous builds cause it to fail.
 * This script removes:
 *   - Final DMG files in bundle/dmg/
 *   - Temporary rw.*.dmg files in bundle/macos/
 */

import { readdirSync, unlinkSync } from "node:fs";
import { join } from "node:path";

const TARGET_DIRS = [
  "src-tauri/target/aarch64-apple-darwin/release/bundle",
  "src-tauri/target/universal-apple-darwin/release/bundle",
  "src-tauri/target/x86_64-apple-darwin/release/bundle",
  "src-tauri/target/release/bundle",
];

const cleanups = TARGET_DIRS.flatMap((base) => [
  { dir: join(base, "dmg"), pattern: /^AgentCabin_.*\.dmg$/ },
  { dir: join(base, "macos"), pattern: /^rw\..*\.dmg$/ },
]);

let removed = 0;
for (const { dir, pattern } of cleanups) {
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    continue; // directory doesn't exist yet — nothing to clean
  }
  for (const name of entries) {
    if (pattern.test(name)) {
      const p = join(dir, name);
      unlinkSync(p);
      console.log(`  removed ${p}`);
      removed++;
    }
  }
}

if (removed) {
  console.log(`Cleaned ${removed} leftover DMG artifact(s)`);
} else {
  console.log("No leftover DMG artifacts to clean");
}
