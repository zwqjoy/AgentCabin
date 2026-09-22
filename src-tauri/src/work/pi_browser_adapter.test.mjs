import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

const TYPEBOX_STUB = `
export const Type = {
  Array: () => ({}), Integer: () => ({}), Object: () => ({}), Optional: (v) => v,
  String: () => ({}),
};
`;

async function loadAdapter(tempRoot) {
  const extensionDir = path.join(tempRoot, "extensions");
  const typeboxDir = path.join(tempRoot, "node_modules", "typebox");
  fs.mkdirSync(extensionDir, { recursive: true });
  fs.mkdirSync(typeboxDir, { recursive: true });
  fs.writeFileSync(path.join(tempRoot, "package.json"), '{"type":"module"}\n');
  fs.writeFileSync(path.join(tempRoot, "node_modules", "typebox", "package.json"), '{"type":"module","exports":"./index.js"}\n');
  fs.writeFileSync(path.join(typeboxDir, "index.js"), TYPEBOX_STUB);
  fs.copyFileSync(new URL("./pi_browser_adapter.mjs", import.meta.url), path.join(extensionDir, "adapter.mjs"));
  return import(`${pathToFileURL(path.join(extensionDir, "adapter.mjs"))}?test=${Date.now()}`);
}

function packageSearchTool(resultsOrError) {
  return {
    execute: async () => {
      if (resultsOrError instanceof Error) throw resultsOrError;
      const results = resultsOrError.map((item) => `${item.title}\n   ${item.url}`).join("\n\n");
      return {
        content: [{ type: "text", text: results }],
        details: { results: resultsOrError },
      };
    },
  };
}

function packageFetchTool(fetcher) {
  return {
    execute: async (_callId, params) => fetcher(String(params.url)),
  };
}

function fetchedPage(url, title, text, mimeType = "text/html") {
  return {
    content: [{ type: "text", text }],
    details: { urls: [url], title, mimeType, urlCount: 1, successful: 1 },
  };
}

test("Browser URL guard rejects loopback, private, metadata and credential URLs", async () => {
  const module = await loadAdapter(fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-ssrf-")));
  for (const value of [
    "http://127.0.0.1:8080/",
    "http://10.0.0.4/",
    "http://172.16.0.1/",
    "http://192.168.1.1/",
    "http://169.254.169.254/",
    "http://[::1]/",
    "http://[fe80::1]/",
    "http://[fd00::1]/",
    "http://metadata.google.internal/",
    "http://user:pass@example.com/",
    "ftp://example.com/file",
  ]) {
    await assert.rejects(module.assertPublicUrl(value), /拒绝|仅允许|无效/);
  }
  assert.equal(module.isPrivateAddress("192.168.1.2"), true);
  assert.equal(module.isPrivateAddress("10.0.0.1"), true);
  assert.equal(module.isPrivateAddress("172.20.0.1"), true);
  assert.equal(module.isPrivateAddress("224.0.0.1"), true);
  assert.equal(module.isPrivateAddress("169.254.169.254"), true);
  assert.equal(module.isPrivateAddress("::ffff:7f00:1"), true);
  assert.equal(module.isPrivateAddress("fea0::1"), true);
  assert.equal(module.isPrivateAddress("8.8.8.8"), false);
  assert.equal(module.isPrivateAddress("93.184.216.34"), false);
});

test("Browser delegates public DNS to the explicit Work proxy", async () => {
  const module = await loadAdapter(fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-proxy-")));
  const previous = {
    enabled: process.env.AGENTCABIN_WORK_PROXY_ENABLED,
    url: process.env.AGENTCABIN_WORK_PROXY_URL,
  };
  let lookups = 0;
  const syntheticLookup = async () => {
    lookups += 1;
    return [{ address: "198.18.0.116", family: 4 }];
  };
  delete process.env.AGENTCABIN_WORK_PROXY_ENABLED;
  delete process.env.AGENTCABIN_WORK_PROXY_URL;
  try {
    await assert.rejects(
      module.assertPublicUrl("https://doc.rust-lang.org/book/", syntheticLookup),
      /DNS|内网/,
    );
    assert.equal(lookups, 1);
    process.env.AGENTCABIN_WORK_PROXY_ENABLED = "1";
    process.env.AGENTCABIN_WORK_PROXY_URL = "http://127.0.0.1:7897";
    lookups = 0;
    await assert.doesNotReject(
      module.assertPublicUrl("https://doc.rust-lang.org/book/", syntheticLookup),
    );
    assert.equal(lookups, 1);
    await assert.rejects(module.assertPublicUrl("http://127.0.0.1/", syntheticLookup), /拒绝/);
  } finally {
    if (previous.enabled === undefined) delete process.env.AGENTCABIN_WORK_PROXY_ENABLED; else process.env.AGENTCABIN_WORK_PROXY_ENABLED = previous.enabled;
    if (previous.url === undefined) delete process.env.AGENTCABIN_WORK_PROXY_URL; else process.env.AGENTCABIN_WORK_PROXY_URL = previous.url;
  }
});

test("网络访问第一阶段拒绝 Cookie、视频、本地文件和 GitHub 专用入口", async () => {
  const module = await loadAdapter(fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-mvp-policy-")));
  const publicLookup = async () => [{ address: "93.184.216.34", family: 4 }];
  for (const value of [
    "https://github.com/openai/agents",
    "https://www.youtube.com/watch?v=demo",
    "https://example.com/video.mp4",
    "file:///Users/cengwenqi/secret.txt",
  ]) {
    await assert.rejects(module.assertFirstStageUrl(value, publicLookup), /第一阶段|仅允许|拒绝/);
  }
});

test("网络访问 exposes the underlying network code for failures", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-network-error-"));
  const previous = {
    enabled: process.env.AGENTCABIN_WORK_BROWSER_ENABLED,
    key: process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY,
    dir: process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR,
  };
  process.env.AGENTCABIN_WORK_BROWSER_ENABLED = "1";
  process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = "test-key";
  process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = temp;
  try {
    const module = await loadAdapter(temp);
    const tools = new Map();
    const searchTool = packageSearchTool((() => {
      const error = new TypeError("fetch failed");
      error.cause = { code: "ECONNRESET" };
      return error;
    })());
    module.default({ registerTool(tool) { tools.set(tool.name, tool); } }, { piWebAccessSearch: searchTool });
    const failed = await tools.get("web_search").execute("network-1", { query: "test" });
    assert.equal(failed.details.ok, false);
    assert.match(failed.content[0].text, /ECONNRESET/);
  } finally {
    if (previous.enabled === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED; else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous.enabled;
    if (previous.key === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY; else process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = previous.key;
    if (previous.dir === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR; else process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = previous.dir;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Native Web Search performs direct Tavily search and populates results", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-native-search-"));
  const previous = {
    enabled: process.env.AGENTCABIN_WORK_BROWSER_ENABLED,
    provider: process.env.AGENTCABIN_WORK_BROWSER_PROVIDER,
    key: process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY,
    dir: process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR,
  };
  process.env.AGENTCABIN_WORK_BROWSER_ENABLED = "1";
  process.env.AGENTCABIN_WORK_BROWSER_PROVIDER = "tavily";
  process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = "tvly-test";
  process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = temp;

  const mockFetch = async (url, options) => {
    assert.equal(url, "https://api.tavily.com/search");
    const body = JSON.parse(options.body);
    assert.equal(body.api_key, "tvly-test");
    assert.equal(body.query, "rust async");
    assert.equal(body.max_results, 3);
    return {
      ok: true,
      status: 200,
      json: async () => ({
        results: [
          { title: "Async Rust", url: "https://rust-lang.org/async", content: "Asynchronous programming in Rust." },
          { title: "Tokio", url: "https://tokio.rs", content: "A runtime for writing reliable network apps." },
        ],
      }),
    };
  };

  try {
    const module = await loadAdapter(temp);
    const tools = new Map();
    module.default(
      { registerTool(tool) { tools.set(tool.name, tool); } },
      { fetch: mockFetch },
    );
    const searchRes = await tools.get("web_search").execute("call-search", { query: "rust async", max_results: 3 });
    assert.equal(searchRes.details.ok, true);
    assert.equal(searchRes.details.results.length, 2);
    assert.match(searchRes.details.results[0].source_id, /_source_1$/);
    assert.equal(searchRes.details.results[0].title, "Async Rust");
    assert.equal(searchRes.details.results[0].url, "https://rust-lang.org/async");

    const ledger = JSON.parse(fs.readFileSync(path.join(temp, "browser-ledger.json"), "utf8"));
    assert.equal(ledger.searches.length, 1);
    assert.equal(ledger.searches[0].query, "rust async");
    assert.equal(ledger.searches[0].provider, "agentcabin/tavily");
  } finally {
    if (previous.enabled === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED; else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous.enabled;
    if (previous.provider === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_PROVIDER; else process.env.AGENTCABIN_WORK_BROWSER_PROVIDER = previous.provider;
    if (previous.key === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY; else process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = previous.key;
    if (previous.dir === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR; else process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = previous.dir;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Work Native Web uses the authenticated bridge for search and fetch", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-bridge-"));
  const previous = {
    enabled: process.env.AGENTCABIN_WORK_BROWSER_ENABLED,
    key: process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY,
    dir: process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR,
    port: process.env.AGENTCABIN_WORK_BRIDGE_PORT,
    token: process.env.AGENTCABIN_WORK_BRIDGE_TOKEN,
  };
  process.env.AGENTCABIN_WORK_BROWSER_ENABLED = "1";
  delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY;
  process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = temp;
  process.env.AGENTCABIN_WORK_BRIDGE_PORT = "54321";
  process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = "bridge-test-token";

  const calls = [];
  const bridgeFetch = async (url, options) => {
    calls.push({ url, options });
    assert.equal(options.headers.Authorization, "Bearer bridge-test-token");
    const body = JSON.parse(options.body);
    if (url.endsWith("/internal/work/web/search")) {
      assert.deepEqual(body, { query: "bridge query", maxResults: 2, toolUseId: "bridge-search" });
      return {
        ok: true,
        status: 200,
        json: async () => ({
          ok: true,
          results: [{ title: "Bridge result", url: "https://example.com/article", snippet: "bridge" }],
        }),
      };
    }
    assert.equal(url, "http://127.0.0.1:54321/internal/work/web/fetch");
    assert.deepEqual(body, {
      url: "https://example.com/article",
      toolUseId: "bridge-open",
    });
    return {
      ok: true,
      status: 200,
      json: async () => ({
        ok: true,
        finalUrl: "https://example.com/article",
        title: "Bridge article",
        contentType: "text/html",
        text: "Readable bridge content",
      }),
    };
  };

  try {
    const module = await loadAdapter(temp);
    const tools = new Map();
    module.default(
      { registerTool(tool) { tools.set(tool.name, tool); } },
      {
        fetch: bridgeFetch,
        lookup: async () => [{ address: "93.184.216.34", family: 4 }],
      },
    );

    const search = await tools.get("web_search").execute("bridge-search", {
      query: "bridge query",
      max_results: 2,
    });
    assert.equal(search.details.ok, true);
    assert.equal(search.details.results[0].url, "https://example.com/article");

    const opened = await tools.get("web_open").execute("bridge-open", {
      source_id: search.details.results[0].source_id,
    });
    assert.equal(opened.details.ok, true);
    assert.equal(opened.details.url, "https://example.com/article");
    assert.deepEqual(
      calls.map(({ url }) => url),
      [
        "http://127.0.0.1:54321/internal/work/web/search",
        "http://127.0.0.1:54321/internal/work/web/fetch",
      ],
    );
  } finally {
    if (previous.enabled === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED; else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous.enabled;
    if (previous.key === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY; else process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = previous.key;
    if (previous.dir === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR; else process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = previous.dir;
    if (previous.port === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_PORT; else process.env.AGENTCABIN_WORK_BRIDGE_PORT = previous.port;
    if (previous.token === undefined) delete process.env.AGENTCABIN_WORK_BRIDGE_TOKEN; else process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = previous.token;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Native Web Fetch extracts title, readable text, strips scripts, and rejects binary", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-native-fetch-"));
  const previous = {
    enabled: process.env.AGENTCABIN_WORK_BROWSER_ENABLED,
    key: process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY,
    dir: process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR,
  };
  process.env.AGENTCABIN_WORK_BROWSER_ENABLED = "1";
  process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = "tvly-test";
  process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = temp;

  const htmlContent = `
    <!DOCTYPE html>
    <html>
      <head><title>My Article &amp; Insights</title><script>console.log(123);</script><style>h1{color:blue;}</style></head>
      <body>
        <h1>Main Title</h1>
        <p>This is the first paragraph with citable information.</p>
        <noscript>Hidden content</noscript>
        <div>Second section with details.</div>
      </body>
    </html>
  `;

  const mockFetch = async (url) => {
    if (url === "https://example.com/binary.pdf") {
      return {
        ok: true,
        status: 200,
        headers: new Headers({ "content-type": "application/pdf" }),
        text: async () => "%PDF-1.4 binary content",
      };
    }
    return {
      ok: true,
      status: 200,
      headers: new Headers({ "content-type": "text/html; charset=utf-8" }),
      text: async () => htmlContent,
    };
  };

  try {
    const module = await loadAdapter(temp);
    const tools = new Map();
    module.default(
      { registerTool(tool) { tools.set(tool.name, tool); } },
      {
        fetch: mockFetch,
        lookup: async () => [{ address: "93.184.216.34", family: 4 }],
      },
    );

    const openRes = await tools.get("web_open").execute("open-1", { url: "https://example.com/article" });
    assert.equal(openRes.details.ok, true);
    assert.equal(openRes.details.title, "My Article & Insights");
    assert.match(openRes.details.preview, /Main Title/);
    assert.match(openRes.details.preview, /This is the first paragraph with citable information\./);
    assert.ok(!openRes.details.preview.includes("console.log"));
    assert.ok(!openRes.details.preview.includes("color:blue"));

    // Reject PDF/binary content
    const binaryRes = await tools.get("web_open").execute("open-pdf", { url: "https://example.com/binary.pdf" });
    assert.equal(binaryRes.details.ok, false);
    assert.match(binaryRes.content[0].text, /仅支持/);
  } finally {
    if (previous.enabled === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED; else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous.enabled;
    if (previous.key === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY; else process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = previous.key;
    if (previous.dir === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR; else process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = previous.dir;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Native Web Fetch re-validates SSRF on HTTP redirect destination and rejects redirect to private host", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-redirect-ssrf-"));
  const previous = {
    enabled: process.env.AGENTCABIN_WORK_BROWSER_ENABLED,
    key: process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY,
    dir: process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR,
  };
  process.env.AGENTCABIN_WORK_BROWSER_ENABLED = "1";
  process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = "tvly-test";
  process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = temp;

  const mockFetch = async (url) => {
    if (url === "https://public.example.com/redirect-to-localhost") {
      return {
        ok: false,
        status: 302,
        headers: new Headers({ "location": "http://127.0.0.1:8080/admin" }),
      };
    }
    if (url === "https://public.example.com/redirect-to-metadata") {
      return {
        ok: false,
        status: 302,
        headers: new Headers({ "location": "http://metadata.google.internal/computeMetadata/v1/" }),
      };
    }
    if (url === "https://public.example.com/redirect-loop") {
      return {
        ok: false,
        status: 302,
        headers: new Headers({ "location": "https://public.example.com/redirect-loop" }),
      };
    }
    return {
      ok: true,
      status: 200,
      headers: new Headers({ "content-type": "text/html" }),
      text: async () => "<h1>Valid</h1>",
    };
  };

  try {
    const module = await loadAdapter(temp);
    const tools = new Map();
    module.default(
      { registerTool(tool) { tools.set(tool.name, tool); } },
      {
        fetch: mockFetch,
        lookup: async (host) => {
          if (host === "public.example.com") return [{ address: "93.184.216.34", family: 4 }];
          if (host === "127.0.0.1" || host === "localhost") return [{ address: "127.0.0.1", family: 4 }];
          return [{ address: "10.0.0.1", family: 4 }];
        },
      },
    );

    const redirectLocal = await tools.get("web_open").execute("o-local", { url: "https://public.example.com/redirect-to-localhost" });
    assert.equal(redirectLocal.details.ok, false);
    assert.match(redirectLocal.content[0].text, /拒绝|内网|本机/);

    const redirectMeta = await tools.get("web_open").execute("o-meta", { url: "https://public.example.com/redirect-to-metadata" });
    assert.equal(redirectMeta.details.ok, false);
    assert.match(redirectMeta.content[0].text, /拒绝|metadata/);

    const redirectLoop = await tools.get("web_open").execute("o-loop", { url: "https://public.example.com/redirect-loop" });
    assert.equal(redirectLoop.details.ok, false);
    assert.match(redirectLoop.content[0].text, /死循环|重定向/);
  } finally {
    if (previous.enabled === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED; else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous.enabled;
    if (previous.key === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY; else process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = previous.key;
    if (previous.dir === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR; else process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = previous.dir;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Browser preserves a damaged source ledger instead of silently overwriting it", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-damaged-ledger-"));
  const previous = {
    enabled: process.env.AGENTCABIN_WORK_BROWSER_ENABLED,
    key: process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY,
    dir: process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR,
  };
  process.env.AGENTCABIN_WORK_BROWSER_ENABLED = "1";
  process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = "test-key";
  process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = temp;
  fs.writeFileSync(path.join(temp, "browser-ledger.json"), "{damaged", "utf8");
  try {
    const module = await loadAdapter(temp);
    const tools = new Map();
    module.default(
      { registerTool(tool) { tools.set(tool.name, tool); } },
      { piWebAccessSearch: packageSearchTool([]) },
    );
    const failed = await tools.get("web_search").execute("damaged-1", { query: "test" });
    assert.equal(failed.details.ok, false);
    assert.match(failed.content[0].text, /来源账本损坏/);
    assert.equal(fs.readFileSync(path.join(temp, "browser-ledger.json"), "utf8"), "{damaged");
  } finally {
    if (previous.enabled === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED; else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous.enabled;
    if (previous.key === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY; else process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = previous.key;
    if (previous.dir === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR; else process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = previous.dir;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Browser tools require the explicit Work Browser permission gate", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-permission-"));
  const previous = {
    enabled: process.env.AGENTCABIN_WORK_BROWSER_ENABLED,
    key: process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY,
    dir: process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR,
  };
  delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED;
  delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY;
  process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = temp;
  try {
    const module = await loadAdapter(temp);
    const tools = new Map();
    module.default({ registerTool(tool) { tools.set(tool.name, tool); } });
    const denied = await tools.get("web_search").execute("permission-1", { query: "should be denied" });
    assert.equal(denied.details.ok, false);
    assert.match(denied.content[0].text, /未启用|未配置/);
  } finally {
    if (previous.enabled === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED; else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous.enabled;
    if (previous.key === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY; else process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = previous.key;
    if (previous.dir === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR; else process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = previous.dir;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Native Web Search performs keyless DuckDuckGo search without API key", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-ddg-search-"));
  const previous = {
    enabled: process.env.AGENTCABIN_WORK_BROWSER_ENABLED,
    provider: process.env.AGENTCABIN_WORK_BROWSER_PROVIDER,
    key: process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY,
    dir: process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR,
  };
  process.env.AGENTCABIN_WORK_BROWSER_ENABLED = "1";
  process.env.AGENTCABIN_WORK_BROWSER_PROVIDER = "duckduckgo";
  delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY;
  process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = temp;

  const mockHtml = `
    <!DOCTYPE html><html><body>
      <div class="result results_links results_links_deep web-result ">
        <h2 class="result__title"><a class="result__a" href="https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage">Example Page</a></h2>
        <a class="result__snippet">This is an example snippet.</a>
      </div>
    </body></html>
  `;

  const mockFetch = async (url, options) => {
    assert.match(url, /duckduckgo\.com/);
    return {
      ok: true,
      status: 200,
      text: async () => mockHtml,
    };
  };

  try {
    const module = await loadAdapter(temp);
    const tools = new Map();
    module.default(
      { registerTool(tool) { tools.set(tool.name, tool); } },
      { fetch: mockFetch },
    );
    const searchRes = await tools.get("web_search").execute("call-ddg", { query: "example search", max_results: 3 });
    assert.equal(searchRes.details.ok, true);
    assert.equal(searchRes.details.results.length, 1);
    assert.equal(searchRes.details.results[0].title, "Example Page");
    assert.equal(searchRes.details.results[0].url, "https://example.com/page");
    assert.equal(searchRes.details.results[0].snippet, "This is an example snippet.");

    const ledger = JSON.parse(fs.readFileSync(path.join(temp, "browser-ledger.json"), "utf8"));
    assert.equal(ledger.searches.length, 1);
    assert.equal(ledger.searches[0].provider, "agentcabin/duckduckgo");
  } finally {
    if (previous.enabled === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED; else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous.enabled;
    if (previous.provider === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_PROVIDER; else process.env.AGENTCABIN_WORK_BROWSER_PROVIDER = previous.provider;
    if (previous.key === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY; else process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = previous.key;
    if (previous.dir === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR; else process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = previous.dir;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("网络访问 adapter completes search, URL fetch, extract and cite with a private ledger", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-ledger-"));
  const previous = {
    enabled: process.env.AGENTCABIN_WORK_BROWSER_ENABLED,
    key: process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY,
    dir: process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR,
  };
  process.env.AGENTCABIN_WORK_BROWSER_ENABLED = "1";
  process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = "test-key";
  process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = temp;
  try {
    const module = await loadAdapter(temp);
    const tools = new Map();
    module.default(
      { registerTool(tool) { tools.set(tool.name, tool); } },
      {
        piWebAccessSearch: packageSearchTool([
          { title: "AgentCabin", url: "https://example.com/work", snippet: "Work browser progress and citations." },
        ]),
        piWebAccessFetch: packageFetchTool(async (url) => fetchedPage(
          url,
          "Work page",
          "Browser progress is recorded.\nIt enables memory safety without\nneeding a garbage collector.\nCitations keep the source URL.",
        )),
        lookup: async () => [{ address: "93.184.216.34", family: 4 }],
      },
    );
    const search = await tools.get("web_search").execute("s1", { query: "Work browser" });
    assert.equal(search.details.ok, true);
    const sourceId = search.details.results[0].source_id;
    assert.match(search.content[0].text, new RegExp(sourceId));
    assert.match(search.content[0].text, /web_open/);
    const opened = await tools.get("web_open").execute("o1", { source_id: sourceId });
    assert.equal(opened.details.ok, true);
    assert.match(opened.content[0].text, new RegExp(opened.details.page_id));
    assert.match(opened.content[0].text, /web_extract/);
    const invalidExtract = await tools.get("web_extract").execute("e-invalid", {
      page_id: "search-0",
      query: "citations",
    });
    assert.equal(invalidExtract.details.ok, false);
    assert.match(invalidExtract.content[0].text, new RegExp(opened.details.page_id));
    const extracted = await tools.get("web_extract").execute("e1", { page_id: opened.details.page_id, query: "citations" });
    assert.equal(extracted.details.ok, true);
    assert.match(extracted.content[0].text, /Citations/);
    assert.ok(
      extracted.details.passages.some((passage) =>
        passage.text.includes("memory safety without needing a garbage collector"),
      ),
      "HTML source line breaks must not fragment a sentence into separate passages",
    );
    const cited = await tools.get("web_cite").execute("c1", { page_id: opened.details.page_id, passage_ids: [extracted.details.passages[0].passage_id] });
    assert.equal(cited.details.ok, true);
    assert.match(cited.content[0].text, /example\.com/);
    const rawIdCited = await tools.get("web_cite").execute("c-raw", {
      page_id: opened.details.page_id,
      passage_ids: [extracted.details.passages[1].passage_id.replace(/^passage_/, "")],
    });
    assert.equal(rawIdCited.details.ok, true);
    const arrayCited = await tools.get("web_cite").execute("c-array", [
      extracted.details.passages[2].passage_id.replace(/^passage_/, ""),
    ]);
    assert.equal(arrayCited.details.ok, true);
    const ledger = JSON.parse(fs.readFileSync(path.join(temp, "browser-ledger.json"), "utf8"));
    assert.equal(ledger.searches.length, 1);
    assert.equal(ledger.pages.length, 1);
    assert.equal(ledger.passages.length, 3);
    assert.equal(ledger.citations.length, 3);
    assert.equal(fs.existsSync(path.join(temp, "pages", `${opened.details.page_id}.json`)), true);
  } finally {
    if (previous.enabled === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED; else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous.enabled;
    if (previous.key === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY; else process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = previous.key;
    if (previous.dir === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR; else process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = previous.dir;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("Browser ledger preserves every page when opens complete concurrently", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-concurrent-"));
  const previous = {
    enabled: process.env.AGENTCABIN_WORK_BROWSER_ENABLED,
    key: process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY,
    dir: process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR,
  };
  process.env.AGENTCABIN_WORK_BROWSER_ENABLED = "1";
  process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = "test-key";
  process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = temp;
  let releaseFirst;
  const firstBlocked = new Promise((resolve) => {
    releaseFirst = resolve;
  });
  let firstReached;
  const firstReady = new Promise((resolve) => {
    firstReached = resolve;
  });
  const fetchImpl = async (url) => {
    if (String(url).endsWith("/one")) {
      firstReached();
      await firstBlocked;
    }
    return fetchedPage(url, url, `A complete page for ${url}.`);
  };
  try {
    const module = await loadAdapter(temp);
    const tools = new Map();
    module.default(
      { registerTool(tool) { tools.set(tool.name, tool); } },
      {
        piWebAccessFetch: packageFetchTool(fetchImpl),
        lookup: async () => [{ address: "93.184.216.34", family: 4 }],
      },
    );
    const first = tools.get("web_open").execute("open-1", {
      url: "https://example.com/one",
    });
    await firstReady;
    const second = await tools.get("web_open").execute("open-2", {
      url: "https://example.com/two",
    });
    releaseFirst();
    const firstResult = await first;
    assert.equal(firstResult.details.ok, true);
    assert.equal(second.details.ok, true);
    const ledger = JSON.parse(fs.readFileSync(path.join(temp, "browser-ledger.json"), "utf8"));
    assert.deepEqual(
      new Set(ledger.pages.map((page) => page.page_id)),
      new Set([firstResult.details.page_id, second.details.page_id]),
    );
  } finally {
    releaseFirst?.();
    if (previous.enabled === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED; else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous.enabled;
    if (previous.key === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY; else process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = previous.key;
    if (previous.dir === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR; else process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = previous.dir;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("网络访问 cite groups a bare passage-id array across multiple pages", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-multi-cite-"));
  const previous = {
    enabled: process.env.AGENTCABIN_WORK_BROWSER_ENABLED,
    key: process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY,
    dir: process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR,
  };
  process.env.AGENTCABIN_WORK_BROWSER_ENABLED = "1";
  process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = "test-key";
  process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = temp;
  const searchResults = [
    { title: "First", url: "https://example.com/first", snippet: "First source has a citable sentence." },
    { title: "Second", url: "https://example.com/second", snippet: "Second source has a citable sentence." },
  ];
  try {
    const module = await loadAdapter(temp);
    const tools = new Map();
    module.default(
      { registerTool(tool) { tools.set(tool.name, tool); } },
      {
        piWebAccessSearch: packageSearchTool(searchResults),
        piWebAccessFetch: packageFetchTool(async (url) => {
          const label = String(url).endsWith("first") ? "First" : "Second";
          return fetchedPage(url, label, `${label} source has a citable sentence.`);
        }),
        lookup: async () => [{ address: "93.184.216.34", family: 4 }],
      },
    );
    const search = await tools.get("web_search").execute("multi-search", { query: "citable" });
    const first = await tools.get("web_open").execute("multi-open-1", { source_id: search.details.results[0].source_id });
    const second = await tools.get("web_open").execute("multi-open-2", { source_id: search.details.results[1].source_id });
    const firstExtract = await tools.get("web_extract").execute("multi-extract-1", { page_id: first.details.page_id });
    const secondExtract = await tools.get("web_extract").execute("multi-extract-2", { page_id: second.details.page_id });
    const cited = await tools.get("web_cite").execute("multi-cite", [
      firstExtract.details.passages[0].passage_id.slice("passage_".length),
      secondExtract.details.passages[0].passage_id.slice("passage_".length),
    ]);
    assert.equal(cited.details.ok, true);
    assert.equal(cited.details.citations.length, 2);
    assert.match(cited.content[0].text, /example\.com\/first/);
    assert.match(cited.content[0].text, /example\.com\/second/);
  } finally {
    if (previous.enabled === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED; else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous.enabled;
    if (previous.key === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY; else process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY = previous.key;
    if (previous.dir === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR; else process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = previous.dir;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test("parallel subagents keep independent Web source ledgers", async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agentcabin-browser-subagents-"));
  const previous = {
    enabled: process.env.AGENTCABIN_WORK_BROWSER_ENABLED,
    dir: process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR,
    child: process.env.PI_SUBAGENT_CHILD,
    childId: process.env.PI_SUBAGENT_RUN_ID,
  };
  process.env.AGENTCABIN_WORK_BROWSER_ENABLED = "1";
  process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = temp;
  process.env.PI_SUBAGENT_CHILD = "1";
  try {
    const module = await loadAdapter(temp);
    for (const childId of ["researcher-a", "researcher-b", "researcher-c"]) {
      process.env.PI_SUBAGENT_RUN_ID = childId;
      const tools = new Map();
      module.default(
        { registerTool(tool) { tools.set(tool.name, tool); } },
        {
          piWebAccessSearch: packageSearchTool([
            { title: childId, url: `https://example.com/${childId}`, snippet: "evidence" },
          ]),
        },
      );
      const searched = await tools.get("web_search").execute(`search-${childId}`, {
        query: childId,
      });
      assert.equal(searched.details.ok, true);
    }

    for (const childId of ["researcher-a", "researcher-b", "researcher-c"]) {
      const ledgerPath = path.join(temp, "subagents", childId, "browser-ledger.json");
      const ledger = JSON.parse(fs.readFileSync(ledgerPath, "utf8"));
      assert.equal(ledger.searches.length, 1);
      assert.equal(ledger.searches[0].query, childId);
    }
    assert.equal(fs.existsSync(path.join(temp, "browser-ledger.json")), false);
  } finally {
    if (previous.enabled === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_ENABLED; else process.env.AGENTCABIN_WORK_BROWSER_ENABLED = previous.enabled;
    if (previous.dir === undefined) delete process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR; else process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR = previous.dir;
    if (previous.child === undefined) delete process.env.PI_SUBAGENT_CHILD; else process.env.PI_SUBAGENT_CHILD = previous.child;
    if (previous.childId === undefined) delete process.env.PI_SUBAGENT_RUN_ID; else process.env.PI_SUBAGENT_RUN_ID = previous.childId;
    fs.rmSync(temp, { recursive: true, force: true });
  }
});
