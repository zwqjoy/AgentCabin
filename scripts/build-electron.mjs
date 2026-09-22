/**
 * Bundle the Electron main and preload scripts with esbuild.
 *
 * Output: electron/dist/main.cjs + electron/dist/preload.cjs (CommonJS,
 * required because sandboxed preloads cannot be ESM).
 */
import { build } from "esbuild";
import { mkdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url)) + "/..";

await mkdir(path.join(root, "electron/dist"), { recursive: true });

const common = {
  bundle: true,
  platform: "node",
  target: "node22",
  format: "cjs",
  external: ["electron"],
  sourcemap: true,
  logLevel: "info",
};

await Promise.all([
  build({
    ...common,
    entryPoints: [path.join(root, "electron/main.ts")],
    outfile: path.join(root, "electron/dist/main.cjs"),
  }),
  build({
    ...common,
    entryPoints: [path.join(root, "electron/preload.ts")],
    outfile: path.join(root, "electron/dist/preload.cjs"),
  }),
]);

console.log("electron build done -> electron/dist/");
