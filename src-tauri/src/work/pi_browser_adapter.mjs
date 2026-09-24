// AgentCabin Native Web Capability Tool Adapter.
//
// Exposes AgentCabin-owned web_* tools to Pi / Work mode:
// - web_search: search public web via AgentCabin Native Web Search (Tavily backend)
// - web_open: safe URL fetch with multi-hop SSRF validation and HTML readable text snapshot
// - web_extract: extract query-relevant passages from a snapshot
// - web_cite: create stable citation records in the run ledger
//
// This adapter no longer depends on or imports pi-web-access.
import crypto from "node:crypto";
import dns from "node:dns/promises";
import fs from "node:fs";
import path from "node:path";
import { Type } from "typebox";

const MAX_PREVIEW_CHARS = 50_000;
const MAX_BODY_BYTES = 2 * 1024 * 1024;
const MAX_REDIRECT_HOPS = 5;
const LEDGER_VERSION = 1;

let ledgerMutationQueue = Promise.resolve();

function result(text, details = {}) {
  return { content: [{ type: "text", text }], details };
}

function fail(message, details = {}) {
  return result(message, { ok: false, ...details });
}

function browserEnabled() {
  return process.env.AGENTCABIN_WEB_ENABLED === "1" ||
    process.env.AGENTCABIN_WORK_BROWSER_ENABLED === "1";
}

function browserProvider() {
  return String(
    process.env.AGENTCABIN_WEB_PROVIDER ||
    process.env.AGENTCABIN_WORK_BROWSER_PROVIDER ||
    "duckduckgo",
  ).trim().toLowerCase();
}

function endpointUrl() {
  return String(
    process.env.AGENTCABIN_WEB_ENDPOINT_URL ||
    process.env.AGENTCABIN_WORK_BROWSER_ENDPOINT_URL ||
    "",
  ).trim();
}

function apiKey() {
  return String(
    process.env.AGENTCABIN_WEB_API_KEY ||
    process.env.AGENTCABIN_WORK_BROWSER_API_KEY ||
    process.env.AGENTCABIN_WEB_TAVILY_API_KEY ||
    process.env.AGENTCABIN_WORK_BROWSER_TAVILY_API_KEY ||
    process.env.TAVILY_API_KEY ||
    "",
  ).trim();
}

function workBridgeConfig() {
  const port = Number(process.env.AGENTCABIN_WORK_BRIDGE_PORT || 0);
  const token = String(process.env.AGENTCABIN_WORK_BRIDGE_TOKEN || "");
  if (Number.isInteger(port) && port > 0 && token) {
    return { baseUrl: `http://127.0.0.1:${port}`, token };
  }
  return null;
}

function workProxyEnabled() {
  return (process.env.AGENTCABIN_WEB_PROXY_ENABLED === "1" || process.env.AGENTCABIN_WORK_PROXY_ENABLED === "1") &&
    /^https?:\/\//i.test(String(process.env.AGENTCABIN_WEB_PROXY_URL || process.env.AGENTCABIN_WORK_PROXY_URL || "").trim());
}

function describeNetworkError(label, error) {
  const message = error instanceof Error ? error.message : String(error);
  const code = error?.cause?.code || error?.code;
  const suffix = code && !message.includes(String(code)) ? `（${code}）` : "";
  if (/fetch failed/i.test(message)) {
    const hint = workProxyEnabled()
      ? "；系统代理已注入，请检查代理是否运行"
      : "；未检测到系统代理";
    return `${label}失败：${message}${suffix}${hint}`;
  }
  return `${label}失败：${message}${suffix}`;
}

function runDir() {
  const value = String(
    process.env.AGENTCABIN_WEB_RUN_DIR ||
    process.env.AGENTCABIN_WORK_BROWSER_RUN_DIR ||
    "",
  ).trim();
  if (!value) throw new Error("Web Access run ledger 未配置");
  return path.resolve(value);
}

function ensureRunDir() {
  const root = runDir();
  fs.mkdirSync(path.join(root, "pages"), { recursive: true });
  try { fs.chmodSync(root, 0o700); } catch { /* platform without chmod */ }
  try { fs.chmodSync(path.join(root, "pages"), 0o700); } catch { /* platform without chmod */ }
  return root;
}

function ledgerPath() {
  return path.join(ensureRunDir(), "browser-ledger.json");
}

function emptyLedger() {
  return { version: LEDGER_VERSION, searches: [], pages: [], passages: [], citations: [] };
}

function readLedger() {
  const target = ledgerPath();
  if (!fs.existsSync(target)) return emptyLedger();
  let value;
  try {
    value = JSON.parse(fs.readFileSync(target, "utf8"));
  } catch {
    throw new Error("Work 网络访问来源账本损坏；已停止写入以保留原始记录");
  }
  return {
    version: LEDGER_VERSION,
    searches: Array.isArray(value?.searches) ? value.searches : [],
    pages: Array.isArray(value?.pages) ? value.pages : [],
    passages: Array.isArray(value?.passages) ? value.passages : [],
    citations: Array.isArray(value?.citations) ? value.citations : [],
  };
}

function atomicWritePrivateJson(target, value) {
  const temporary = `${target}.${process.pid}.${crypto.randomUUID()}.tmp`;
  fs.writeFileSync(temporary, `${JSON.stringify(value, null, 2)}\n`, "utf8");
  try { fs.chmodSync(temporary, 0o600); } catch { /* platform without chmod */ }
  try {
    fs.renameSync(temporary, target);
  } catch (error) {
    try { fs.rmSync(temporary, { force: true }); } catch { /* preserve original error */ }
    throw error;
  }
}

function writeLedger(ledger) {
  atomicWritePrivateJson(path.join(ensureRunDir(), "browser-ledger.json"), ledger);
}

async function mutateLedger(mutator) {
  const operation = ledgerMutationQueue.then(() => {
    const ledger = readLedger();
    const value = mutator(ledger);
    writeLedger(ledger);
    return value;
  });
  ledgerMutationQueue = operation.catch(() => undefined);
  return operation;
}

function id(prefix) {
  return `${prefix}_${crypto.randomUUID()}`;
}

function isPrivateIpv4(host) {
  const parts = host.split(".").map(Number);
  if (parts.length !== 4 || parts.some((part) => !Number.isInteger(part) || part < 0 || part > 255)) {
    return false;
  }
  const [a, b] = parts;
  return a === 0 || a === 10 || a === 127 || (a === 169 && b === 254) ||
    (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168) ||
    (a === 100 && b >= 64 && b <= 127) || (a === 198 && (b === 18 || b === 19)) ||
    a >= 224;
}

function isProxySyntheticIpv4(host) {
  const parts = String(host || "").split(".").map(Number);
  return parts.length === 4 && parts.every((part) => Number.isInteger(part) && part >= 0 && part <= 255) &&
    parts[0] === 198 && (parts[1] === 18 || parts[1] === 19);
}

function isPrivateIpv6(host) {
  const normalized = host.toLowerCase().replace(/^\[|\]$/g, "");
  const firstHextet = Number.parseInt(normalized.split(":", 1)[0] || "0", 16);
  const mapped = normalized.startsWith("::ffff:") ? normalized.slice(7) : "";
  let mappedPrivate = false;
  if (mapped) {
    if (mapped.includes(".")) {
      mappedPrivate = isPrivateIpv4(mapped);
    } else {
      const groups = mapped.split(":");
      if (groups.length === 2 && groups.every((group) => /^[0-9a-f]{1,4}$/.test(group))) {
        const high = Number.parseInt(groups[0], 16);
        const low = Number.parseInt(groups[1], 16);
        mappedPrivate = isPrivateIpv4(
          `${high >> 8}.${high & 255}.${low >> 8}.${low & 255}`,
        );
      }
    }
  }
  return normalized === "::" || normalized === "::1" ||
    (Number.isFinite(firstHextet) && (firstHextet & 0xffc0) === 0xfe80) ||
    normalized.startsWith("fc") || normalized.startsWith("fd") || normalized.startsWith("ff") ||
    normalized.startsWith("2001:db8") || mappedPrivate;
}

export function isPrivateAddress(address) {
  const value = String(address || "").trim();
  return isPrivateIpv4(value) || isPrivateIpv6(value);
}

function getAllowedHosts() {
  const raw = String(
    process.env.AGENTCABIN_WEB_ALLOWED_HOSTS ||
    process.env.AGENTCABIN_WORK_BROWSER_ALLOWED_HOSTS ||
    process.env.AGENTCABIN_BROWSER_ALLOWED_HOSTS ||
    ""
  );
  return raw
    .split(/[,\n;]/)
    .map((s) => s.trim().toLowerCase())
    .filter(Boolean);
}

function matchesIpv4Cidr(ip, cidr) {
  const [netStr, prefixStr] = cidr.split("/");
  const prefix = parseInt(prefixStr, 10);
  if (isNaN(prefix) || prefix < 0 || prefix > 32) return false;
  const netParts = netStr.split(".").map(Number);
  const ipParts = ip.split(".").map(Number);
  if (netParts.length !== 4 || ipParts.length !== 4) return false;
  if (netParts.some((n) => isNaN(n) || n < 0 || n > 255)) return false;
  if (ipParts.some((n) => isNaN(n) || n < 0 || n > 255)) return false;
  const netInt = ((netParts[0] << 24) | (netParts[1] << 16) | (netParts[2] << 8) | netParts[3]) >>> 0;
  const ipInt = ((ipParts[0] << 24) | (ipParts[1] << 16) | (ipParts[2] << 8) | ipParts[3]) >>> 0;
  const mask = prefix === 0 ? 0 : ((0xffffffff << (32 - prefix)) >>> 0);
  return (ipInt & mask) === (netInt & mask);
}

function isHostOrIpAllowed(hostname, addresses = []) {
  const allowed = getAllowedHosts();
  if (allowed.length === 0) return false;
  const lowerHost = hostname.toLowerCase();

  for (const pattern of allowed) {
    if (pattern === lowerHost) return true;
    if (pattern.startsWith("*.")) {
      const suffix = pattern.slice(2);
      if (lowerHost === suffix || lowerHost.endsWith("." + suffix)) return true;
    } else if (pattern.startsWith(".")) {
      const suffix = pattern.slice(1);
      if (lowerHost === suffix || lowerHost.endsWith("." + suffix)) return true;
    }
    if (addresses.includes(pattern)) return true;
    if (pattern.includes("/")) {
      for (const addr of [lowerHost, ...addresses]) {
        if (matchesIpv4Cidr(addr, pattern)) return true;
      }
    }
  }
  return false;
}

function rejectUnsafeHostname(hostname) {
  const host = String(hostname || "").toLowerCase().replace(/^\[|\]$/g, "");
  if (!host || host === "localhost" || host.endsWith(".localhost") || host.endsWith(".local") ||
      host.endsWith(".internal") || host === "metadata.google.internal" || isPrivateAddress(host)) {
    throw new Error("网络访问 URL 指向本机、内网或云 metadata 地址，已拒绝");
  }
}

/** Validate URL syntax and resolve DNS before any outbound request. */
export async function assertPublicUrl(rawUrl, lookup = dns.lookup) {
  let parsed;
  try { parsed = new URL(String(rawUrl || "").trim()); } catch { throw new Error("网络访问 URL 无效"); }
  if (!/^https?:$/.test(parsed.protocol) || parsed.username || parsed.password) {
    throw new Error("网络访问仅允许不带凭据的 http/https 公网 URL");
  }
  const host = String(parsed.hostname || "").toLowerCase().replace(/^\[|\]$/g, "");
  if (!host || host === "metadata.google.internal" || host === "169.254.169.254") {
    throw new Error("网络访问 URL 指向云 metadata 地址，已拒绝");
  }

  const hostAllowed = isHostOrIpAllowed(host, []);

  let records = [];
  try {
    records = await lookup(parsed.hostname, { all: true, verbatim: true });
  } catch (error) {
    if (!hostAllowed) {
      if (error?.message?.includes("已拒绝")) throw error;
      throw new Error("网络访问 URL 无法完成安全 DNS 校验");
    }
  }

  const addresses = Array.isArray(records) ? records.map((r) => String(r.address || "")) : [];
  if (addresses.includes("169.254.169.254")) {
    throw new Error("网络访问 URL 指向云 metadata 地址，已拒绝");
  }

  if (hostAllowed || isHostOrIpAllowed(host, addresses)) {
    return parsed;
  }

  rejectUnsafeHostname(parsed.hostname);
  if (!isPrivateAddress(parsed.hostname)) {
    if (!Array.isArray(records) || records.length === 0) {
      throw new Error("网络访问 URL 无法完成安全 DNS 校验");
    }
    if (records.some((record) => {
      const address = String(record.address || "");
      return isPrivateAddress(address) && !(workProxyEnabled() && isProxySyntheticIpv4(address));
    })) {
      throw new Error("网络访问 URL 的 DNS 解析包含内网地址，已拒绝");
    }
  }
  return parsed;
}

function decodeEntities(value) {
  return value
    .replace(/&(?:amp|lt|gt|quot|apos|nbsp);/gi, (entity) => ({
      "&amp;": "&", "&lt;": "<", "&gt;": ">", "&quot;": '"', "&apos;": "'", "&nbsp;": " ",
    }[entity.toLowerCase()] || entity))
    .replace(/&#(\d{1,7});/g, (_match, num) => {
      const code = Number.parseInt(num, 10);
      return Number.isFinite(code) && code <= 0x10ffff ? String.fromCodePoint(code) : _match;
    })
    .replace(/&#x([0-9a-f]{1,6});/gi, (_match, hex) => {
      const code = Number.parseInt(hex, 16);
      return Number.isFinite(code) && code <= 0x10ffff ? String.fromCodePoint(code) : _match;
    });
}

export function extractTitle(html, fallbackHost = "") {
  const match = String(html || "").match(/<title\b[^>]*>([\s\S]*?)<\/title>/i);
  if (match && match[1]) {
    const cleaned = decodeEntities(match[1]).replace(/[\r\n\t]+/g, " ").replace(/\s+/g, " ").trim();
    if (cleaned) return cleaned;
  }
  return fallbackHost;
}

/** Convert a bounded HTML response into readable text without executing it. */
export function extractText(body, contentType = "") {
  if (/json|xml|text\//i.test(contentType) && !/html/i.test(contentType)) {
    return String(body).replace(/\s+/g, " ").trim();
  }
  return decodeEntities(String(body)
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, " ")
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, " ")
    .replace(/<noscript\b[^>]*>[\s\S]*?<\/noscript>/gi, " ")
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<\/p\s*>|<\/div\s*>|<\/li\s*>|<\/h[1-6]\s*>|<\/tr\s*>|<\/article\s*>|<\/section\s*>/gi, "\n")
    .replace(/<[^>]+>/g, " ")
    .replace(/\r/g, "")
    .replace(/[ \t]+/g, " ")
    .replace(/\n\s*\n+/g, "\n")
    .trim());
}

function isBlockedFirstStageHost(hostname) {
  const host = String(hostname || "").toLowerCase().replace(/^www\./, "");
  return host === "github.com" || host.endsWith(".github.com") ||
    host === "youtube.com" || host.endsWith(".youtube.com") ||
    host === "youtu.be" || host.endsWith(".youtu.be");
}

/** First-stage policy: public HTML/text/JSON URLs only. */
export async function assertFirstStageUrl(rawUrl, lookup = dns.lookup) {
  const parsed = await assertPublicUrl(rawUrl, lookup);
  if (isBlockedFirstStageHost(parsed.hostname)) {
    throw new Error("网络访问第一阶段暂不支持 GitHub 克隆或视频 URL");
  }
  if (/\.(?:mp4|mov|webm|avi|mpeg|mpg|wmv|flv|3gp|pdf|zip)(?:$|[?#])/i.test(parsed.pathname)) {
    throw new Error("网络访问第一阶段仅支持 HTML、纯文本或 JSON URL");
  }
  return parsed;
}

function assertFetchResultIsText(contentType) {
  const normalized = String(contentType || "").toLowerCase();
  if (/application\/pdf|image\/|audio\/|video\/|application\/octet-stream|application\/zip/i.test(normalized)) {
    throw new Error("网络访问第一阶段仅支持 HTML、纯文本或 JSON 页面");
  }
  return normalized || "text/html";
}

function requireId(value, label) {
  const idValue = String(value || "").trim();
  if (!idValue) throw new Error(`${label} 不能为空`);
  return idValue;
}

function scorePassage(text, query) {
  const tokens = String(query || "").toLocaleLowerCase().split(/[^\p{L}\p{N}]+/u).filter(Boolean);
  const lower = text.toLocaleLowerCase();
  return tokens.reduce((score, token) => score + (lower.includes(token) ? 1 : 0), 0);
}

const SearchSchema = Type.Object({
  query: Type.String({ minLength: 1, description: "Search query." }),
  max_results: Type.Optional(Type.Integer({ minimum: 1, maximum: 10 })),
});
const OpenSchema = Type.Object({
  source_id: Type.Optional(Type.String({ description: "Exact source_id returned in web_search content." })),
  url: Type.Optional(Type.String()),
});
const ExtractSchema = Type.Object({
  page_id: Type.String({ description: "Exact page_id returned in web_open content." }),
  query: Type.Optional(Type.String()),
  max_passages: Type.Optional(Type.Integer({ minimum: 1, maximum: 20 })),
});
const CiteSchema = Type.Object({
  page_id: Type.Optional(Type.String({ description: "Exact page_id returned in web_open content." })),
  passage_ids: Type.Optional(Type.Array(Type.String({ description: "Exact passage_id returned in web_extract content." }), { minItems: 1, maxItems: 20 })),
});

function passageMatches(value, passage) {
  const requested = String(value || "").trim();
  const actual = String(passage?.passage_id || "");
  return requested === actual ||
    (actual.startsWith("passage_") && requested === actual.slice("passage_".length));
}

function normalizeCitationRequests(params, ledger) {
  const rawPageId = !Array.isArray(params) ? String(params?.page_id || "").trim() : "";
  const rawPassageIds = Array.isArray(params)
    ? params
    : (Array.isArray(params?.passage_ids) ? params.passage_ids : []);
  const matching = rawPassageIds
    .map((value) => ledger.passages.find((passage) => passageMatches(value, passage)))
    .filter(Boolean);
  const unique = [...new Map(matching.map((passage) => [passage.passage_id, passage])).values()];
  if (rawPageId) {
    const passages = rawPassageIds.length
      ? unique.filter((passage) => passage.page_id === rawPageId)
      : ledger.passages.filter((passage) => passage.page_id === rawPageId);
    return [{ pageId: rawPageId, passages }];
  }
  const grouped = new Map();
  for (const passage of unique) {
    const group = grouped.get(passage.page_id) || [];
    group.push(passage);
    grouped.set(passage.page_id, group);
  }
  return [...grouped.entries()].map(([pageId, passages]) => ({ pageId, passages }));
}

function parseDuckDuckGoHtml(html, maxResults) {
  const results = [];
  const chunks = html.split('class="result results_links');
  for (let i = 1; i < chunks.length; i++) {
    if (results.length >= maxResults) break;
    const chunk = chunks[i];
    const tagMatch = chunk.match(/class="result__(?:a|url)"[^>]*href="([^"]+)"/i) || chunk.match(/href="([^"]+)"/i);
    if (!tagMatch) continue;
    let url = tagMatch[1];
    if (url.includes("uddg=")) {
      try {
        const match = url.match(/uddg=([^&]+)/);
        if (match) url = decodeURIComponent(match[1]);
      } catch {}
    }
    if (!/^https?:\/\//i.test(url)) continue;

    let title = "Untitled";
    const titleMatch = chunk.match(/class="result__title"[^>]*>([\s\S]*?)<\/a>/i) || chunk.match(/class="result__a"[^>]*>([\s\S]*?)<\/a>/i);
    if (titleMatch) {
      title = titleMatch[1].replace(/<[^>]+>/g, "").replace(/&amp;/g, "&").replace(/&quot;/g, '"').replace(/&#39;/g, "'").trim();
    }

    let snippet = "";
    const snippetMatch = chunk.match(/class="result__snippet"[^>]*>([\s\S]*?)<\/(?:a|div)>/i);
    if (snippetMatch) {
      snippet = snippetMatch[1].replace(/<[^>]+>/g, "").replace(/&amp;/g, "&").replace(/&quot;/g, '"').replace(/&#39;/g, "'").trim();
    }

    results.push({ title: title || url, url, snippet: snippet.slice(0, 2000) });
  }
  return results;
}

async function performNativeSearch(query, maxResults, signal, fetchImpl = globalThis.fetch) {
  const provider = browserProvider();
  const key = apiKey();

  if (provider === "duckduckgo" || provider === "ddg" || provider === "anysearch") {
    const response = await fetchImpl("https://html.duckduckgo.com/html/", {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
        "User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
      },
      body: `q=${encodeURIComponent(query)}`,
      signal,
    });
    if (!response.ok) {
      const errorText = await response.text().catch(() => "");
      throw new Error(`DuckDuckGo 搜索返回 HTTP ${response.status}: ${errorText}`);
    }
    const html = await response.text();
    return parseDuckDuckGoHtml(html, maxResults);
  }

  if (provider === "tavily") {
    if (!key) throw new Error("网络访问未配置 Tavily API key");
    const response = await fetchImpl("https://api.tavily.com/search", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        api_key: key,
        query,
        max_results: maxResults,
        include_answer: false,
      }),
      signal,
    });

    if (!response.ok) {
      const errorText = await response.text().catch(() => "");
      throw new Error(`Tavily 搜索返回 HTTP ${response.status}: ${errorText}`);
    }

    const data = await response.json();
    if (data?.error) throw new Error(data.error);

    const rawResults = Array.isArray(data?.results) ? data.results : [];
    return rawResults.map((item) => ({
      title: String(item?.title || item?.url || "Untitled"),
      url: String(item?.url || "").trim(),
      snippet: String(item?.content || item?.snippet || "").slice(0, 2_000),
    })).filter((item) => /^https?:\/\//i.test(item.url)).slice(0, maxResults);
  }

  if (provider === "exa") {
    if (!key) throw new Error("网络访问未配置 Exa API key");
    const response = await fetchImpl("https://api.exa.ai/search", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "x-api-key": key,
      },
      body: JSON.stringify({
        query,
        numResults: maxResults,
        useAutoprompt: true,
      }),
      signal,
    });

    if (!response.ok) {
      const errorText = await response.text().catch(() => "");
      throw new Error(`Exa 搜索返回 HTTP ${response.status}: ${errorText}`);
    }

    const data = await response.json();
    if (data?.error) throw new Error(data.error);

    const rawResults = Array.isArray(data?.results) ? data.results : [];
    return rawResults.map((item) => ({
      title: String(item?.title || item?.url || "Untitled"),
      url: String(item?.url || "").trim(),
      snippet: String(item?.highlights?.[0] || item?.text || "").slice(0, 2_000),
    })).filter((item) => /^https?:\/\//i.test(item.url)).slice(0, maxResults);
  }

  if (provider === "searxng" || provider === "searx") {
    const endpoint = endpointUrl();
    if (!endpoint) throw new Error("网络访问未配置 SearXNG 实例地址 (URL)");
    const base = endpoint.replace(/\/+$/, "");
    const headers = {};
    if (key) headers["Authorization"] = `Bearer ${key}`;
    const response = await fetchImpl(`${base}/search?format=json&q=${encodeURIComponent(query)}`, {
      headers,
      signal,
    });

    if (!response.ok) {
      const errorText = await response.text().catch(() => "");
      throw new Error(`SearXNG 搜索返回 HTTP ${response.status}: ${errorText}`);
    }

    const data = await response.json();
    const rawResults = Array.isArray(data?.results) ? data.results : [];
    return rawResults.map((item) => ({
      title: String(item?.title || item?.url || "Untitled"),
      url: String(item?.url || "").trim(),
      snippet: String(item?.content || "").slice(0, 2_000),
    })).filter((item) => /^https?:\/\//i.test(item.url)).slice(0, maxResults);
  }

  throw new Error(`不支持的网络访问 provider: ${provider}`);
}

async function performNativeFetch(initialUrl, signal, lookup = dns.lookup, fetchImpl = globalThis.fetch) {
  let currentParsed = await assertFirstStageUrl(initialUrl, lookup);
  let currentUrl = currentParsed.toString();
  const visited = new Set([currentUrl]);
  let hops = 0;

  while (hops <= MAX_REDIRECT_HOPS) {
    const response = await fetchImpl(currentUrl, {
      method: "GET",
      redirect: "manual",
      headers: {
        "User-Agent": "AgentCabin/1.0.0 (Native Web)",
        "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,text/plain,application/json,*/*;q=0.8",
      },
      signal,
    });

    if ([301, 302, 303, 307, 308].includes(response.status)) {
      hops += 1;
      if (hops > MAX_REDIRECT_HOPS) {
        throw new Error("网络访问重定向次数过多（已超过限制）");
      }
      const location = response.headers.get("location");
      if (!location) throw new Error("重定向缺少 Location 响应头");

      const nextParsed = new URL(location, currentUrl);
      if (visited.has(nextParsed.toString())) {
        throw new Error("检测到重定向死循环");
      }
      visited.add(nextParsed.toString());

      // Re-run SSRF validation for the redirect target
      currentParsed = await assertFirstStageUrl(nextParsed.toString(), lookup);
      currentUrl = currentParsed.toString();
      continue;
    }

    if (!response.ok) {
      throw new Error(`网页抓取失败，HTTP 返回 ${response.status}`);
    }

    const contentType = assertFetchResultIsText(response.headers.get("content-type"));
    const rawBody = await response.text();

    if (rawBody.length > MAX_BODY_BYTES) {
      throw new Error(`网页大小超出安全限制（${rawBody.length} 字符，最大支持 ${MAX_BODY_BYTES}）`);
    }

    const title = extractTitle(rawBody, currentParsed.hostname);
    const text = extractText(rawBody, contentType);

    if (!text) {
      throw new Error("未返回可读取的页面内容");
    }

    return {
      finalUrl: currentUrl,
      title,
      contentType,
      text,
    };
  }

  throw new Error("网络访问重定向次数过多（已超过限制）");
}

export default function agentCabinWorkBrowserExtension(pi, dependencies = {}) {
  const lookup = dependencies.lookup || dns.lookup;
  const injectedSearch = dependencies.piWebAccessSearch || dependencies.webSearch;
  const injectedFetch = dependencies.piWebAccessFetch || dependencies.webFetch;
  const customFetch = dependencies.fetch || globalThis.fetch;

  pi.registerTool({
    name: "web_search",
    label: "web_search",
    description: "Search the public web through the AgentCabin Native Web Capability.",
    parameters: SearchSchema,
    async execute(_toolCallId, params, signal) {
      try {
        const query = String(params?.query || "").trim();
        if (!query) return fail("网络访问 search query 不能为空");
        const maxResults = Math.min(Math.max(Number(params?.max_results || process.env.AGENTCABIN_WEB_MAX_RESULTS || process.env.AGENTCABIN_WORK_BROWSER_MAX_RESULTS || 5), 1), 10);
        const provider = browserProvider();
        const key = apiKey();
        const bridge = workBridgeConfig();

        if (!browserEnabled()) {
          return fail("网络访问未启用");
        }
        if (!bridge && !injectedSearch) {
          if ((provider === "tavily" || provider === "exa") && !key) {
            return fail(`网络访问未配置 ${provider === "tavily" ? "Tavily" : "Exa"} API key`);
          }
          if ((provider === "searxng" || provider === "searx") && !endpointUrl()) {
            return fail("网络访问未配置 SearXNG 实例地址 (URL)");
          }
        }

        let rawResults;
        if (injectedSearch) {
          const output = await injectedSearch.execute(_toolCallId, {
            query,
            numResults: maxResults,
            provider,
            workflow: "none",
          }, signal);
          if (output?.details?.error) return fail(`搜索失败：${output.details.error}`);
          if (Array.isArray(output?.details?.results)) {
            rawResults = output.details.results;
          } else {
            const lines = (output?.content || []).filter((i) => i.type === "text").map((i) => i.text).join("\n").split("\n").filter(Boolean);
            rawResults = lines.filter((l) => /^https?:\/\//i.test(l.trim())).map((url) => ({ title: url, url, snippet: "" }));
          }
        } else {
          if (bridge) {
            const bridgeRes = await customFetch(`${bridge.baseUrl}/internal/work/web/search`, {
              method: "POST",
              headers: {
                "Content-Type": "application/json",
                "Authorization": `Bearer ${bridge.token}`,
              },
              body: JSON.stringify({ query, maxResults, toolUseId: _toolCallId }),
              signal,
            });
            const bridgeData = await bridgeRes.json().catch(() => ({}));
            if (!bridgeRes.ok || !bridgeData.ok) {
              throw new Error(bridgeData.error || `Bridge search request failed (${bridgeRes.status})`);
            }
            rawResults = bridgeData.results || [];
          } else {
            rawResults = await performNativeSearch(query, maxResults, signal, customFetch);
          }
        }

        const searchId = id("search");
        const results = rawResults.map((item, index) => ({
          source_id: `${searchId}_source_${index + 1}`,
          title: String(item?.title || item?.url || "Untitled"),
          url: String(item?.url || "").trim(),
          snippet: String(item?.snippet || item?.content || "").slice(0, 2_000),
        })).filter((item) => /^https?:\/\//i.test(item.url)).slice(0, maxResults);

        await mutateLedger((ledger) => {
          ledger.searches.push({ search_id: searchId, query, provider: `agentcabin/${provider}`, created_at: new Date().toISOString(), results });
        });

        const modelVisibleResults = results.map((item) => [
          `source_id: ${item.source_id}`,
          `title: ${item.title}`,
          `url: ${item.url}`,
          `snippet: ${item.snippet.slice(0, 1_000)}`,
        ].join("\n"));

        return result([
          `找到 ${results.length} 条结果。搜索摘要仅供选择来源，不能直接作为引用。`,
          ...modelVisibleResults,
          `下一步：使用上面的精确 source_id 调用 web_open。`,
        ].join("\n\n"), { ok: true, search_id: searchId, provider: `agentcabin/${provider}`, query, results });
      } catch (error) {
        return fail(describeNetworkError("AgentCabin 搜索", error));
      }
    },
  });

  pi.registerTool({
    name: "web_open",
    label: "web_open",
    description: "Fetch a public HTML, text, or JSON URL and save its bounded private snapshot to the current run.",
    parameters: OpenSchema,
    async execute(_toolCallId, params, signal) {
      try {
        if (!browserEnabled()) {
          return fail("网络访问未启用");
        }
        const ledger = readLedger();
        const sourceId = params?.source_id ? requireId(params.source_id, "source_id") : null;
        const source = sourceId
          ? ledger.searches.flatMap((search) => search.results || []).find((item) => item.source_id === sourceId)
          : null;
        const requestedUrl = params?.url || source?.url;
        if (!requestedUrl) return fail("请提供 source_id 或 url");
        const publicUrl = await assertFirstStageUrl(requestedUrl, lookup);

        let fetched;
        if (injectedFetch) {
          const output = await injectedFetch.execute(_toolCallId, {
            url: publicUrl.toString(),
            mode: "readable",
          }, signal);
          if (output?.details?.error) return fail(`URL 抓取失败：${output.details.error}`);
          const text = (output?.content || []).filter((i) => i.type === "text").map((i) => i.text).join("\n").trim();
          if (!text) return fail("未返回可读取的页面内容");
          const contentType = assertFetchResultIsText(output?.details?.mimeType || "text/html");
          fetched = {
            finalUrl: String(output?.details?.urls?.[0] || publicUrl.toString()),
            title: String(output?.details?.title || publicUrl.hostname),
            contentType,
            text,
          };
        } else {
          const bridge = workBridgeConfig();
          if (bridge) {
            const bridgeRes = await customFetch(`${bridge.baseUrl}/internal/work/web/fetch`, {
              method: "POST",
              headers: {
                "Content-Type": "application/json",
                "Authorization": `Bearer ${bridge.token}`,
              },
              body: JSON.stringify({ url: publicUrl.toString(), toolUseId: _toolCallId }),
              signal,
            });
            const bridgeData = await bridgeRes.json().catch(() => ({}));
            if (!bridgeRes.ok || !bridgeData.ok) {
              throw new Error(bridgeData.error || `Bridge fetch request failed (${bridgeRes.status})`);
            }
            fetched = {
              finalUrl: bridgeData.finalUrl,
              title: bridgeData.title,
              contentType: bridgeData.contentType,
              text: bridgeData.text,
            };
          } else {
            fetched = await performNativeFetch(publicUrl.toString(), signal, lookup, customFetch);
          }
        }

        const pageId = id("page");
        const previewText = fetched.text.slice(0, MAX_PREVIEW_CHARS);
        const record = {
          page_id: pageId,
          source_id: source?.source_id || null,
          url: fetched.finalUrl,
          title: fetched.title,
          content_type: fetched.contentType,
          created_at: new Date().toISOString(),
          text_chars: previewText.length,
        };

        const pagePath = path.join(ensureRunDir(), "pages", `${pageId}.json`);
        atomicWritePrivateJson(pagePath, { ...record, text: previewText });
        try {
          await mutateLedger((currentLedger) => {
            currentLedger.pages.push(record);
          });
        } catch (error) {
          try { fs.rmSync(pagePath, { force: true }); } catch { /* preserve ledger error */ }
          throw error;
        }

        return result([
          `已打开：${fetched.title}`,
          `page_id: ${pageId}`,
          `source_id: ${record.source_id || "(direct-url)"}`,
          `url: ${fetched.finalUrl}`,
          `snapshot_chars: ${previewText.length}`,
          `下一步：使用上面的精确 page_id 调用 web_extract。`,
        ].join("\n"), {
          ok: true,
          page_id: pageId,
          source_id: record.source_id,
          url: fetched.finalUrl,
          title: fetched.title,
          content_type: fetched.contentType,
          preview: previewText,
          truncated: fetched.text.length > MAX_PREVIEW_CHARS,
        });
      } catch (error) {
        return fail(describeNetworkError("打开网页", error));
      }
    },
  });

  pi.registerTool({
    name: "web_extract",
    label: "web_extract",
    description: "Extract relevant passages from a previously opened page snapshot.",
    parameters: ExtractSchema,
    async execute(_toolCallId, params) {
      try {
        const pageId = requireId(params?.page_id, "page_id");
        const ledger = readLedger();
        const page = ledger.pages.find((item) => item.page_id === pageId);
        if (!page) {
          const availablePages = ledger.pages.slice(-10).map((item) => ({
            page_id: item.page_id,
            title: item.title,
            url: item.url,
          }));
          const availableText = availablePages.length
            ? `\n当前可用 page_id:\n${availablePages.map((item) => `- ${item.page_id} | ${item.title}`).join("\n")}`
            : "";
          return fail(`Page snapshot 不存在；必须使用 web_open 返回的精确 page_id。${availableText}`, { available_pages: availablePages });
        }
        const stored = JSON.parse(fs.readFileSync(path.join(ensureRunDir(), "pages", `${pageId}.json`), "utf8"));
        const query = String(params?.query || "").trim();
        const maxPassages = Math.min(Math.max(Number(params?.max_passages || 5), 1), 20);

        const normalizedText = String(stored.text || "").replace(/\s+/g, " ").trim();
        const sentences = normalizedText
          .split(/(?<=[.!?。！？])\s+/)
          .map((text) => text.trim())
          .filter((text) => text.length >= 20);
        const passages = sentences.map((text) => {
          const exactOffset = String(stored.text || "").indexOf(text);
          const prefix = text.split(/\s+/).slice(0, 6).join(" ");
          const prefixOffset = String(stored.text || "").indexOf(prefix);
          return {
            text,
            score: query ? scorePassage(text, query) : 1,
            offset: exactOffset >= 0 ? exactOffset : Math.max(prefixOffset, 0),
          };
        }).sort((a, b) => b.score - a.score || a.offset - b.offset).slice(0, maxPassages).map((item) => ({
          passage_id: id("passage"),
          page_id: pageId,
          text: item.text.slice(0, 4_000),
          score: item.score,
          offset: item.offset,
        }));

        await mutateLedger((currentLedger) => {
          currentLedger.passages.push(...passages);
        });

        const visiblePassages = passages.map((item) => `[${item.passage_id}] ${item.text}`).join("\n\n");
        const nextStep = passages.length
          ? `下一步：使用 page_id ${pageId} 和上面的精确 passage_id 调用 web_cite。`
          : `未摘取到可引用段落，请调整 query 后重试 web_extract。`;
        return result([visiblePassages, nextStep].filter(Boolean).join("\n\n"), { ok: true, page_id: pageId, query: query || null, passages });
      } catch (error) {
        return fail(error instanceof Error ? error.message : String(error));
      }
    },
  });

  pi.registerTool({
    name: "web_cite",
    label: "web_cite",
    description: "Create a citation record from passages in the current 网络访问 ledger.",
    parameters: CiteSchema,
    async execute(_toolCallId, params) {
      try {
        const ledger = readLedger();
        const requests = normalizeCitationRequests(params, ledger);
        const citations = [];
        for (const request of requests) {
          const page = ledger.pages.find((item) => item.page_id === request.pageId);
          if (!page || !request.passages.length) continue;
          const citationId = id("citation");
          const markdown = `[${page.title || page.url}](${page.url})`;
          citations.push({ citation_id: citationId, page_id: request.pageId, passage_ids: request.passages.map((item) => item.passage_id), url: page.url, title: page.title, markdown, created_at: new Date().toISOString() });
        }
        if (!citations.length) return fail("没有找到可引用的页面摘录");
        await mutateLedger((currentLedger) => {
          currentLedger.citations.push(...citations);
        });
        const passages = citations.flatMap((citation) =>
          ledger.passages.filter((item) => citation.passage_ids.includes(item.passage_id)),
        );
        const markdown = citations.map((citation) => citation.markdown).join("\n");
        return result(markdown, {
          ok: true,
          markdown,
          citation_id: citations.length === 1 ? citations[0].citation_id : undefined,
          citation: citations.length === 1 ? citations[0] : undefined,
          citations,
          passages,
        });
      } catch (error) {
        return fail(error instanceof Error ? error.message : String(error));
      }
    },
  });
}
