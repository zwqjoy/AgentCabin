# Work Harness Boundary Audit & Simplification

Audit date: 2026-10-06. Baseline: latest local `dev` after pull, `ac9a2605`.

Pi owns the agent loop; AgentCabin Work owns authority, durability, side effects, recovery, and completion truth.

## Audit and decisions

| Capability | Before owner | After owner | Action | Reason |
| --- | --- | --- | --- | --- |
| Conversation / reasoning / model loop | Pi | Pi | Keep | Work launches Pi session actors; it does not run an agent loop. |
| Todo | Pi plus required Work plan updates | Pi | Simplify | Native Todo is the only agent-owned progress list. Work no longer registers or discovers a second progress API. |
| Goal | Pi prompt plus `work_set_goal` text; Host GoalSpec | Pi objective; Host acceptance contract | Internal-only | The removed tool only changed `WorkTaskState.goal`, not GoalSpec. Task artifact requirements create Host acceptance criteria. |
| Plan | Pi Todo plus Work plan tools | Pi; historical/internal Host representation | Internal-only | Remove model registration, catalog entries, schemas, mutation helpers and dual-maintenance prompt. Preserve persisted plan and recovery step references. |
| Checkpoint | Pi session plus model-authored Work summary | Pi session; Host Ledger and historical checkpoint | Internal-only | Ledger persists ToolProposed/Started/Result and outputs automatically. A model-authored summary is not a safe substitute for execution facts. |
| MCP discovery | Pi native MCP / tool_search | Pi | Keep | Deferred native tools stay native. Host projects authenticated localhost endpoints, owns real transport and credentials. |
| Skill loading | Pi with Host-selected eligible sources | Pi; Host trust/eligibility boundary | Keep | Host source selection is authority, not a second skill loader. |
| Policy / workspace boundary | Host | Host | Keep | Path resolution, restricted executor and capability policy remain authoritative. |
| Approval / PendingInteraction / Inbox | Host | Host | Keep | Durable decisions survive process interruption. |
| Tool execution / side effects | Host ToolPipeline | Host ToolPipeline | Keep | Normalize intent → policy → trust/security → approval → execute → validate → ToolResult → Ledger/Artifact. |
| Runtime Ledger | Host | Host | Keep | Recovery truth cannot be inferred from Pi narrative or session completion. |
| Artifact / completion | Host | Host | Keep | Pending interactions, unresolved tool outcomes, required artifacts, validation and acceptance gate completion. |
| Recovery | Host plus Pi session resume | Host effects; Pi cognition | Keep | Safe reads can be resumed; verified local outputs can be reused; unknown started external mutations require a human decision. |
| Scheduler / automation | Host | Host | Keep | Task/run identity, schedule triggers and unattended policy remain durable. |
| Subagent legacy | Historical Ledger facts / labels / projection | Historical replay only | Keep compatibility | No production delegate/agent implementation or catalog registration. Old facts remain readable without re-exposing tools. |
| Runtime abstraction | Pi implementation plus test adapter; shared provider enum | Pi production launch; test contract | Simplify | Remove unused UnsupportedRuntime/RuntimeUnavailable errors and multi-runtime claims. Keep trait because launch-order tests use a fake implementation. |
| Capability catalog / bridge client | Claimed runtime-neutral Pi/DSH metadata and transport | Pi capabilities and authenticated Host transport | Simplify | These files lower coupling and are tested; obsolete DSH extensibility claims do not justify deleting transport. |
| `WORK_COMPAT_TOOL_NAMES` | Pi read/write/edit/bash wrappers | Same confined aliases | Keep | These guard native names through Work authority; they are not historical DSH shims. |

A: Pi-native ownership is conversation, reasoning, model/context loop, Todo, skill loading and MCP discovery.

B: Host ownership is workspace authority, policy, approval, execution, side-effect classification, Ledger, pending interactions, artifact truth, recovery, scheduling and durable WorkRun lifecycle.

C: Duplicate Goal/Plan/Todo/Checkpoint model APIs are removed from the normal agent surface. Durable schemas and internal authenticated Host update/read endpoints remain for existing state, tests and recovery. No second Todo cache or automatic semantic summary is introduced.

D: Historical subagent facts/labels and provider identities are retained for persisted replay or Code sharing. The runtime launch trait remains for test isolation and for enforcing prepare → bridge registration → actor spawn. Work production lookup only accepts Pi. `runtime_bridge` is a transport/module name, not a runtime selector.

## Reviewed production dependencies

- `work/runtime/{mod,pi}.rs`, `work/session.rs` and `commands/session_dispatch.rs`: only Pi production lookup; actor/session identity and bridge registration precede launch. The fake adapter in dispatch tests verifies this order.
- `pi_core_extension.mjs` and `runtime_bridge/{work_tool_catalog,bridge_client}.mjs`: four default-active state tools previously required dual maintenance. Their JS fallback duplicated the Rust task-state writer and has been removed.
- `work/task_state.rs`, `tasks.rs`, `context/assembler.rs`, `projection.rs`, `progress.rs`: persisted snapshots, structured plan events, old current-step context and progress projections remain readable. They do not create new plans now that the model APIs are absent. Pending approval and Task-bound acceptance state remain live Host data.
- `work/lifecycle.rs`, `ledger.rs`, `artifacts.rs`, `goal.rs`: actual correctness dependencies. Completion checks pending interactions, unfinished ToolProposed facts and artifact acceptance; GoalSpec is generated for explicit task artifact requirements. Ordinary tasks without requirements skip GoalSpec repair.
- `work/pipeline/{mod,types,tests}.rs`, `policy.rs`, `executor.rs`, `internal_bridge.rs`: consequential effects retain Host classification, execution and authoritative results.
- `pi_mcp_adapter.mjs`: thin native extension initializer using createMcpExtension/createToolSearchExtension. Safe projection strips real endpoint/command/env/OAuth information and uses deferred tools. No old adapter runtime is restored.
- Subagent search: active catalog and registration contain no work_delegate/work_agent_* tools. `progress::resolve_subagents` reads historical Ledger facts only. Legacy role projections and guardian polling labels remain replay compatibility.
- RuntimeProviderKind is also used by Code capability resolution and serialized legacy profiles. Removing variants would expand scope and damage shared/historical contracts.

## Architecture after refactor

```text
Pi Runtime
  conversation · reasoning · context/model loop · Todo · skills
  native MCP + deferred tool_search
        |
        v
Work Capabilities (Pi registration + safe Host projection)
        |
        v
Host Authority (ToolPipeline)
  normalize → policy → trust → approval → execute → validate
        |
        v
Durable Work
  ToolResult/Ledger · Inbox · Artifact/acceptance · recovery · scheduler
```

## Removed / simplified / retained

Removed: four model-facing state tools, their schemas, JS task-state mutation queue, local file writer and test fallback, obsolete state-tool tests and their duplicate mock writer, and two unused runtime error variants.

Simplified: system guidance uses Pi Todo alone; catalog and bridge documentation describe Pi-to-Host ownership; the stale catalog assertion expecting the retired `mcp` proxy now asserts its absence.

Internal-only: GoalSpec acceptance, persisted WorkTaskState goal/plan/checkpoint and pending approval. No persisted fields or historical events are deleted or migrated. No semantic automatic checkpoint is needed: actual execution facts are already recorded automatically, while Pi session persistence owns conversational resume.

Retained for compatibility or coupling: runtime launch trait/test double, shared provider enum, authenticated bridge client, native-name confined aliases, legacy plan snapshots/parser/projection, subagent Ledger facts and labels. Historical plans can still display on old runs; modern Work cannot produce them via a model tool.

## Remaining debt

- Historical structured-task events and plan projections remain visible when replaying old runs. A future schema migration could separate legacy display from modern Todo, but removing old state here would damage replay.
- Internal task-state update endpoints still accept legacy plan/checkpoint fields. They are authenticated and not exposed as Pi tools; any later retirement needs a separate migration audit.
- The launch trait retains provider-shaped types because shared Code resolution and fake launch tests depend on them. There is no second production Work implementation.

## Validation

| Executed check | Result |
| --- | --- |
| `npm run verify` | PASS: ESLint, Prettier, Svelte typecheck, i18n, pet check, frontend unit tests, Vite build, Rust fmt and clippy. 120 test files / 2030 frontend tests. Existing Svelte/i18n warnings remain. |
| `node --test src-tauri/src/work/*.test.mjs src-tauri/src/work/runtime_bridge/*.test.mjs` | PASS: 60 tests, including native MCP projection, confined tool aliases, retired-tool registration/discovery/activation and Pi-only progress guidance. |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib` | PARTIAL: 1533 passed, 4 failed, 4 ignored before adding the 3 lifecycle tests. |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1` | PARTIAL: final source, 1536 passed, 4 failed, 4 ignored. All 3 new lifecycle tests pass. |
| Independent baseline export at `ac9a2605`, same bundled runtime/extension dependencies, serial Rust suite | Same 4 failures: 1533 passed, 4 failed, 4 ignored. Confirms these failures predate this change. |
| `node scripts/smoke-work-native-mcp.mjs` | PASS: real bundled Pi, 58 deferred tools, native search activation, active-tool preservation, authenticated loopback calls, resume, closed-server and malformed-response handling. |
| `npm run test:pi:rpc` | PASS: boot, prompt, steer, follow_up, abort, state/model/thinking, tool events, resume, fork and compaction. |
| `git diff --check` | PASS. |

The final Rust run includes 24 pipeline, 8 lifecycle, 6 projection, 7 Ledger, 17 artifact, 4 runtime, 35 Work E2E and 7 workbench/golden tests, all passing. New tests cover simple completion without GoalSpec/Plan/Checkpoint, unresolved approval blocking completion, and disk-store reopening after interrupted reads / started external mutation. Existing golden tests verify local output reuse after restart and multi-artifact delivery acceptance.

The four baseline failures are:

- `agent::capability_resolver::tests::expert_and_expert_team_skills_excluded_from_general_skills_unless_selected`
- `work::connector_package_manager::tests::projects_cli_package_and_resolves_fixed_operation`
- `work::resources::tests::connector_package_skill_overrides_same_named_profile_skill`
- `work::resources::tests::loads_work_resources_into_pi_agent_settings`

These expert selection / connector skill fixture failures are left for separate scope. No real remote mutation, live-model desktop smoke, or packaged desktop acceptance is claimed. The MCP and RPC smoke tests use deterministic local fixtures with the real bundled Pi runtime.
