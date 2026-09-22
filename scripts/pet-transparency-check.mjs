import { readFileSync } from "node:fs";

const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const petWindow = config.app?.windows?.find((window) => window.label === "pet");
const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
const windowSource = readFileSync("src-tauri/src/pet/window.rs", "utf8");

if (!petWindow?.transparent) {
  throw new Error('The Tauri pet window must keep "transparent": true.');
}

if (!cargo.includes('"macos-private-api"')) {
  throw new Error("The Tauri macOS transparent WebView feature is missing.");
}

if (!/set_background_color\(Some\(Color\(0,\s*0,\s*0,\s*0\)\)\)/.test(windowSource)) {
  throw new Error("The pet WebView background must be explicitly set to RGBA(0, 0, 0, 0).");
}

console.log("Pet transparency contract passed.");
