#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { chmodSync, mkdirSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

if (process.platform !== "darwin") process.exit(0);

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceDir = resolve(root, "src-tauri/resources/agentcabin-computer-use/macos");
const appBundle = resolve(sourceDir, "agentcabin-computer-use.app");
const output = resolve(appBundle, "Contents/MacOS/bridge");
const arch = process.arch === "x64" ? "x86_64" : "arm64";
const target = `${arch}-apple-macosx14.0`;
const cache = resolve(root, ".agentcabin-build-cache/agentcabin-computer-use-swift-modules", arch);
const BUNDLE_ID = "com.agentcabin.computer-use";

const swiftSources = [
  resolve(sourceDir, "agent_cursor.swift"),
  resolve(sourceDir, "agent_cursor_motion.swift"),
  resolve(sourceDir, "bridge.swift"),
  resolve(appBundle, "Contents/Info.plist"),
];

// Check if output is already up to date to prevent repeated recompilation and TCC invalidation
let isUpToDate = false;
const force = process.argv.includes("--force");
try {
  const outputStat = statSync(output);
  const newestSourceMtime = Math.max(...swiftSources.map((f) => statSync(f).mtimeMs));
  if (!force && outputStat.mtimeMs > newestSourceMtime) {
    isUpToDate = true;
  }
} catch {
  isUpToDate = false;
}

if (!isUpToDate) {
  mkdirSync(cache, { recursive: true });
  mkdirSync(dirname(output), { recursive: true });

  const args = [
    "swiftc",
    "-target",
    target,
    "-module-cache-path",
    cache,
    "-O",
    "-framework",
    "ApplicationServices",
    "-framework",
    "AppKit",
    "-framework",
    "ScreenCaptureKit",
    "-framework",
    "Foundation",
    "-framework",
    "SwiftUI",
    resolve(sourceDir, "agent_cursor.swift"),
    resolve(sourceDir, "agent_cursor_motion.swift"),
    resolve(sourceDir, "bridge.swift"),
    "-o",
    output,
  ];
  const compiled = spawnSync("xcrun", args, { stdio: "inherit" });
  if (compiled.error) throw compiled.error;
  if (compiled.status !== 0) process.exit(compiled.status ?? 1);
  chmodSync(output, 0o755);

  // Sign with an explicit designated requirement tied to the bundle identifier.
  // Without -r, ad-hoc signing defaults to pinning the exact CDHash, which
  // invalidates macOS TCC permissions on every rebuild.
  const signed = spawnSync(
    "codesign",
    [
      "--force",
      "--deep",
      "--timestamp=none",
      "--sign",
      "-",
      "-i",
      BUNDLE_ID,
      "-r",
      `=designated => identifier "${BUNDLE_ID}"`,
      appBundle,
    ],
    { stdio: "inherit" },
  );
  if (signed.error) throw signed.error;
  if (signed.status !== 0) process.exit(signed.status ?? 1);
  console.log(`[agentcabin-computer-use] macOS app bundle compiled & signed: ${appBundle}`);
} else {
  console.log(`[agentcabin-computer-use] macOS app bundle is up to date: ${appBundle}`);
}

// Compile Apple Vision OCR helper executable
const ocrScript = resolve(root, "scripts/macos_ocr.swift");
const ocrOutput = resolve(sourceDir, "ocr-helper");
const ocrCacheDir = resolve(root, ".agentcabin-build-cache/macos_ocr");
const ocrCacheOutput = resolve(ocrCacheDir, "ocr-helper");
const OCR_BUNDLE_ID = "com.agentcabin.ocr-helper";

let isOcrUpToDate = false;
try {
  const ocrStat = statSync(ocrOutput);
  const scriptStat = statSync(ocrScript);
  if (!force && ocrStat.mtimeMs > scriptStat.mtimeMs) {
    isOcrUpToDate = true;
  }
} catch {
  isOcrUpToDate = false;
}

if (!isOcrUpToDate) {
  mkdirSync(cache, { recursive: true });
  mkdirSync(dirname(ocrOutput), { recursive: true });
  mkdirSync(ocrCacheDir, { recursive: true });

  const ocrArgs = [
    "swiftc",
    "-target",
    target,
    "-module-cache-path",
    cache,
    "-O",
    "-framework",
    "Foundation",
    "-framework",
    "CoreGraphics",
    "-framework",
    "ImageIO",
    "-framework",
    "Vision",
    ocrScript,
    "-o",
    ocrOutput,
  ];
  const compiled = spawnSync("xcrun", ocrArgs, { stdio: "inherit" });
  if (compiled.error) throw compiled.error;
  if (compiled.status !== 0) process.exit(compiled.status ?? 1);
  chmodSync(ocrOutput, 0o755);

  const signed = spawnSync(
    "codesign",
    [
      "--force",
      "--timestamp=none",
      "--sign",
      "-",
      "-i",
      OCR_BUNDLE_ID,
      ocrOutput,
    ],
    { stdio: "inherit" },
  );
  if (signed.error) throw signed.error;
  if (signed.status !== 0) process.exit(signed.status ?? 1);

  // Copy to build cache as well
  try {
    const { copyFileSync } = await import("node:fs");
    copyFileSync(ocrOutput, ocrCacheOutput);
    chmodSync(ocrCacheOutput, 0o755);
  } catch {
    // Cache copy optional
  }

  console.log(`[agentcabin-computer-use] macOS OCR helper compiled & signed: ${ocrOutput}`);
} else {
  console.log(`[agentcabin-computer-use] macOS OCR helper is up to date: ${ocrOutput}`);
}
