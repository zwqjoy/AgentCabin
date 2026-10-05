#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(import.meta.url), "../..");
const suite = process.argv[2] || "all";
const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";
const node = process.execPath;

const packagedRuntimeCandidates = [
  "dist-electron/mac-arm64/AgentCabin.app/Contents/Resources/runtimes",
  "dist-electron/mac/AgentCabin.app/Contents/Resources/runtimes",
  "dist-electron/mac-universal/AgentCabin.app/Contents/Resources/runtimes",
  "dist-electron/win-unpacked/resources/runtimes",
  "dist-electron/linux-unpacked/resources/runtimes",
];
const runtimeRoot =
  process.env.AGENTCABIN_RUNTIME_ROOT ||
  ["runtime-build", ...packagedRuntimeCandidates]
    .map((path) => resolve(root, path))
    .find((path) => existsSync(resolve(path, "runtime-manifest.json")));

const cases = {
  capabilities: [
    [
      "Agent Plugin Skill and MCP lifecycle",
      cargo,
      [
        "test",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--lib",
        "acceptance_plugin_skill_and_mcp_share_trust_and_binding_lifecycle",
        "--",
        "--nocapture",
      ],
    ],
    [
      "Enabled Skill projection into isolated Code and Work runtimes",
      cargo,
      [
        "test",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--lib",
        "test_unified_global_capability_shared_by_default",
        "--",
        "--nocapture",
      ],
    ],
    [
      "Disabled and native-profile Skill isolation",
      cargo,
      [
        "test",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--lib",
        "test_negative_discovery_sentinel_isolation",
        "--",
        "--nocapture",
      ],
    ],
  ],
  code: [
    [
      "Code capability and session skill projection",
      npm,
      [
        "exec",
        "vitest",
        "run",
        "src/lib/utils/chat-skills.test.ts",
        "src/lib/utils/__tests__/first-message-skills.test.ts",
        "src/lib/utils/__tests__/capability_projection.test.ts",
        "src/lib/utils/__tests__/new-session-start.test.ts",
      ],
    ],
    [
      "Agent Plugin Code MCP route tests",
      node,
      ["--test", "src-tauri/src/work/pi_code_agent_plugin_mcp.test.mjs"],
    ],
  ],
  work: [
    [
      "Work approval resumes the pending run",
      cargo,
      [
        "test",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--lib",
        "test_7_resolving_inbox_actually_resumes_runtime",
        "--",
        "--nocapture",
      ],
    ],
    [
      "Work denial blocks tool execution",
      cargo,
      [
        "test",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--lib",
        "test_29_reject_tool_leaves_work_run_running_and_denies_execution",
        "--",
        "--nocapture",
      ],
    ],
    [
      "Work failure recovery creates a single recovery Inbox item",
      cargo,
      [
        "test",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--lib",
        "test_23c_unresolved_external_call_requires_one_recovery_inbox_item",
        "--",
        "--nocapture",
      ],
    ],
    [
      "Work approval grant lifecycle survives restart",
      cargo,
      [
        "test",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--lib",
        "test_26_vertical_unattended_ask_inbox_approve_grant_consumed_completion",
        "--",
        "--nocapture",
      ],
    ],
    [
      "Native Pi MCP discovery, Host approval, read, mutation, and resume",
      cargo,
      [
        "test",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--lib",
        "native_pi_search_calls_authenticated_host_bridge_and_inbox",
        "--",
        "--ignored",
        "--nocapture",
      ],
    ],
  ],
  runtime: [
    ["Bundled runtime closure", npm, ["run", "verify:runtimes"]],
    [
      "Packaged Pi RPC, extension lifecycle, native MCP, and process cleanup",
      npm,
      ["run", "smoke:packaged-runtimes"],
    ],
  ],
};

const requested = suite === "all" ? ["capabilities", "code", "work", "runtime"] : [suite];
if (!requested.every((name) => cases[name])) {
  console.error(
    `Unknown acceptance suite '${suite}'. Choose all, capabilities, code, work, or runtime.`,
  );
  process.exit(2);
}

if (
  requested.includes("runtime") &&
  (!runtimeRoot || !existsSync(resolve(runtimeRoot, "runtime-manifest.json")))
) {
  console.error(
    "FAIL Runtime closure: no prepared runtime found. Set AGENTCABIN_RUNTIME_ROOT or prepare runtime-build before running acceptance.",
  );
  process.exit(1);
}

console.log("AgentCabin Capability Acceptance\n");
let failures = 0;
for (const name of requested) {
  for (const [label, command, args] of cases[name]) {
    console.log(`RUN  ${label}`);
    const result = spawnSync(command, args, {
      cwd: root,
      env: {
        ...process.env,
        CI: "1",
        ...(name === "runtime" && runtimeRoot
          ? { AGENTCABIN_RUNTIME_ROOT: runtimeRoot, AGENTCABIN_RUNTIME_OUTPUT: runtimeRoot }
          : {}),
      },
      stdio: "inherit",
      shell: process.platform === "win32",
    });
    if (result.status === 0) {
      console.log(`PASS ${label}\n`);
    } else {
      failures++;
      console.error(
        `FAIL ${label}\nExpected: command exits successfully\nActual: exit ${result.status ?? result.signal}\n`,
      );
    }
  }
}
if (failures) {
  console.error(`Acceptance failed: ${failures} check(s) failed.`);
  process.exit(1);
}
console.log("Acceptance passed.");
