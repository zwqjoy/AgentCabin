# Grok ACP fixtures

## `grok_build_1_0_initialize.json`

This fixture is a **source-derived Grok Build 1.0.0 initialize-response baseline**.

Provenance:

- Upstream repository: `xai-org/grok-build`
- Upstream commit: `8a14c91d88875a831a38b3a066b1683116bcb31c`
- Version proof: `crates/codegen/xai-grok-version/Cargo.toml` declares `version = "1.0.0"`
- Response construction: `crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs`, `MvpAgent::initialize`

The stable response shape is copied from the 1.0.0 source. Runtime-dependent values are
normalized so the fixture is deterministic: auth methods, cwd, process/agent IDs, hostname,
model catalog, MCP server list, available commands, and resolved settings values. It remains
checked in as a source-derived comparison point alongside the live wire-derived fixture below.

Do not infer AgentCabin host filesystem or terminal support from this fixture. Those are
client capabilities sent in AgentCabin's initialize request and must remain disabled until
the corresponding host-side handlers are implemented and tested.

## `grok_build_1_0_initialize.wire.json`

This fixture is a **normalized live Grok Build 1.0.0 initialize response** captured on
2026-08-09 UTC from `grok 1.0.0 (3cd0d0cbcebe)`. The captured `response.result` output
has SHA-256
`9a98ce23222d6e2530dab52fbdc8fb58ed33b45cca8f0662846af3aa6cc3cd3a`.

The live response confirmed explicit `promptCapabilities.image=false` and `audio=false`,
the `x.ai/hooks` capability shape, seven available commands including `goal`,
`cancelRewind=true`, `voiceMode=true`, and reasoning-effort metadata in the model state.
Only runtime identity fields (`currentWorkingDirectory`, agent IDs, and hostname) were
replaced with fixture values. The observed model and command catalog are retained as wire
shape evidence and should not be treated as an invariant across all Grok configurations.

The raw trace is intentionally not checked in because it contains the capturing machine's
working directory, hostname, and process identifiers. A fresh run can regenerate it and
produce a new normalized fixture after review.

## Capturing a live initialize exchange

Use the repository harness from an environment that has the target Grok binary installed:

```sh
npm run grok:capture-initialize -- \
  --binary /path/to/grok \
  --expected-agent-version 1.0.0 \
  --response-out /tmp/grok_build_1_0_initialize.wire.json \
  --trace-out /tmp/grok_build_1_0_initialize.trace.json
```

The harness sends the same initialize request as the Grok actor, waits for JSON-RPC id `1`,
then terminates the child without sending `session/new`, `session/load`, or a prompt. The
response output is the JSON-RPC `result` only; the trace output also retains the request,
raw stdout lines, stderr, and the full response envelope. Existing files require `--force`
to overwrite.

Review a live response before checking it in: model catalogs, paths, host identifiers, and
other runtime-dependent values may need normalization. Keep the trace outside the repository
unless its provenance and contents are safe to publish. A live trace is evidence for the
wire shape; it does not by itself enable AgentCabin host filesystem or terminal support.
