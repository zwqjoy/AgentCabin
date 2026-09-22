/**
 * Navigation decisions for the embedded browser view.
 *
 * Agent-initiated navigation is validated in Rust
 * (`work::browser_operator::security::validate_browser_url`); this module only
 * covers in-page navigation the user triggers by clicking a link, so policy
 * lives in two places for two different entry points. Loopback and cloud
 * metadata hosts are hard-blocked here and can never be allow-listed.
 */

export type NavigationDecision =
  | { action: "allow" }
  | { action: "ask"; host: string }
  | {
      action: "block";
      reason: "unsupported_scheme" | "unparseable_url" | "local_or_metadata_host";
    };

export interface NavigationPolicyInput {
  fromUrl: string;
  toUrl: string;
  allowedHosts: string[];
}

const LOCAL_HOST_PATTERNS = [
  /^localhost$/i,
  /^127\./,
  /^0\.0\.0\.0$/,
  /^\[?::1\]?$/,
  /^10\./,
  /^192\.168\./,
  /^172\.(1[6-9]|2\d|3[01])\./,
  /^169\.254\./,
  /^metadata\.google\.internal$/i,
  /\.local$/i,
];

function isLocalOrMetadataHost(host: string): boolean {
  return LOCAL_HOST_PATTERNS.some((pattern) => pattern.test(host));
}

function originsMatch(a: string, b: string): boolean {
  try {
    return new URL(a).origin === new URL(b).origin;
  } catch {
    return false;
  }
}

export function classifyNavigation(input: NavigationPolicyInput): NavigationDecision {
  const target = String(input.toUrl ?? "").trim();

  if (target === "about:blank") return { action: "allow" };

  let parsed: URL;
  try {
    parsed = new URL(target);
  } catch {
    return { action: "block", reason: "unparseable_url" };
  }

  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    return { action: "block", reason: "unsupported_scheme" };
  }

  const host = parsed.hostname.toLowerCase();

  if (isLocalOrMetadataHost(host)) {
    return { action: "block", reason: "local_or_metadata_host" };
  }

  if (originsMatch(input.fromUrl, target)) return { action: "allow" };

  const allowed = (input.allowedHosts ?? []).map((entry) => entry.trim().toLowerCase());
  if (allowed.includes(host)) return { action: "allow" };

  return { action: "ask", host };
}
