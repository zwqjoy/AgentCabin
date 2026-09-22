/**
 * Minimal SPA-aware static file server for the built SvelteKit renderer.
 *
 * The production build (adapter-static, `fallback: "index.html"`) is served
 * over http://127.0.0.1:<random port> instead of file:// so that absolute
 * asset paths and the WebSocket transport keep working unchanged.
 */
import http from "node:http";
import { createReadStream, existsSync, statSync } from "node:fs";
import path from "node:path";

const MIME_TYPES: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".gif": "image/gif",
  ".webp": "image/webp",
  ".ico": "image/x-icon",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
  ".ttf": "font/ttf",
  ".otf": "font/otf",
  ".wasm": "application/wasm",
  ".map": "application/json; charset=utf-8",
  ".txt": "text/plain; charset=utf-8",
};

function contentType(filePath: string): string {
  return MIME_TYPES[path.extname(filePath).toLowerCase()] ?? "application/octet-stream";
}

export function startStaticServer(rootDir: string): Promise<{
  url: string;
  close: () => void;
}> {
  const server = http.createServer((req, res) => {
    if (req.method !== "GET" && req.method !== "HEAD") {
      res.writeHead(405).end();
      return;
    }

    const urlPath = decodeURIComponent(new URL(req.url ?? "/", "http://localhost").pathname);
    const safePath = path.normalize(urlPath).replace(/^(\.\.[/\\])+/, "");
    let filePath = path.join(rootDir, safePath);

    // Stay inside rootDir.
    if (!filePath.startsWith(path.resolve(rootDir))) {
      res.writeHead(403).end();
      return;
    }

    // Unknown path or directory -> SPA fallback (SvelteKit adapter-static).
    if (!existsSync(filePath) || statSync(filePath).isDirectory()) {
      filePath = path.join(rootDir, "index.html");
    }

    if (!existsSync(filePath)) {
      res.writeHead(404).end("Renderer build not found. Run `npm run build` first.");
      return;
    }

    res.writeHead(200, { "Content-Type": contentType(filePath) });
    if (req.method === "HEAD") {
      res.end();
      return;
    }
    createReadStream(filePath).pipe(res);
  });

  return new Promise((resolve, reject) => {
    server.once("error", reject);
    // Port 0 = OS-assigned random port, loopback only.
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        reject(new Error("Failed to bind static server"));
        return;
      }
      resolve({
        url: `http://127.0.0.1:${address.port}`,
        close: () => server.close(),
      });
    });
  });
}
