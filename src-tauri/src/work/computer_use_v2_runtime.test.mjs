import assert from "node:assert/strict";
import test from "node:test";

import http from "node:http";
import { ComputerUseV2Session, RefStabilizer } from "./computer_use_v2_runtime.mjs";
import { CdpComputerUseBackend } from "./cdp_computer_use_backend.mjs";
import { probeElectronCdp } from "./desktop_computer_use_backend.mjs";
import {
  BACKEND_CDP,
  BACKEND_MACOS_AX,
  ROOT_KIND_BROWSER_PAGE,
  ROOT_KIND_DESKTOP_WINDOW,
  ROOT_KIND_ELECTRON_PAGE,
  StateStore,
} from "./computer_use_v3_models.mjs";

function envelope(parsed) {
  return { success: true, status: "success", stdout: JSON.stringify(parsed) };
}

function observation(observationId, value = "0") {
  return {
    content: [{ type: "text", text: "Calculator" }],
    structuredContent: {
      pid: 42,
      window_id: 7,
      observation_id: observationId,
      backend: "agentcabin-computer-use",
      elements: [
        { ref: "e1", role: "button", title: "7" },
        { ref: "e2", role: "text", title: "display", value },
      ],
    },
  };
}

test("native ref spelling resolves only within the supplied state", async () => {
  const session = new ComputerUseV2Session(async () => envelope(observation("obs")));
  const observed = await session.observeUi("observe", {});
  const stateId = observed.details.stateId;
  const state = session.state(stateId);
  assert.deepEqual(session.legacyAction(state, { action: "click", ref: "e1" }),
    session.legacyAction(state, { action: "click", ref: "@e1" }));
  assert.equal(session.inspectUi("inspect", { stateId, ref: "e1" }).details.element.ref, "@e1");
  assert.equal(session.readText("read", { stateId, ref: "e2" }).details.ok, true);
  assert.equal(session.expandUi("expand", { stateId, ref: "e1" }).details.ok, true);
  assert.equal(session.conditionMatches(state, { ref: "e2", value: "0" }), true);
  assert.throws(() => session.legacyAction(state, { action: "click", ref: "e999" }), /not owned/);
  const other = session.saveState({ pid: 42, elements: [{ ref: "e999" }] }, {}, 0);
  assert.throws(() => session.legacyAction(other, { action: "click", ref: "e1" }), /not owned/);
});

test("semantic refs survive regenerated native refs across observations", async () => {
  let observationCount = 0;
  const session = new ComputerUseV2Session(async () => {
    observationCount += 1;
    return envelope({
      structuredContent: {
        pid: 42,
        window_id: 7,
        observation_id: `obs-${observationCount}`,
        elements: observationCount === 1
          ? [{ ref: "ax:old-button", role: "button", title: "Save" }]
          : [{ ref: "ax:new-button", role: "button", title: "Save" }],
      },
    });
  });
  const first = await session.observeUi("observe-1", {});
  const second = await session.observeUi("observe-2", {});
  assert.equal(first.details.elements[0].ref, "@e1");
  assert.equal(second.details.elements[0].ref, first.details.elements[0].ref);
  assert.equal(second.details.elements[0].nativeRef, "ax:new-button");
});

test("compound desktop operations use distinct durable IDs and stable IDs on retry", async () => {
  const ledger = new Map();
  let polls = 0;
  const session = new ComputerUseV2Session(async (id, tool) => {
    if (ledger.has(id)) throw new Error(`already has a durable result: ${id}`);
    ledger.set(id, tool);
    if (tool === "desktop_open_app") return envelope({ structuredContent: { pid: 42 } });
    if (tool === "desktop_list_apps") return envelope({ structuredContent: { windows: [
      { pid: 42, window_id: 7, owner: "Calculator", title: "Calculator" },
    ] } });
    if (tool === "desktop_observe") {
      const value = id.includes(":poll:") && ++polls >= 2 ? "15" : "0";
      return envelope(observation(id, value));
    }
    throw new Error(`unexpected tool ${tool}`);
  });
  const launched = await session.launchApp("launch-parent", { name: "Calculator" });
  assert.ok(launched.details.stateId, JSON.stringify(launched));
  const roots = await session.findRoots("find", {});
  const observed = await session.observeUi("select-parent", { root: roots.details.roots[0].ref });
  assert.ok(observed.details.stateId, JSON.stringify(observed));
  const waited = await session.waitFor("wait-parent", {
    stateId: observed.details.stateId, ref: "@e2", value: "15", timeoutMs: 2000,
  });
  assert.equal(waited.details.found, true, JSON.stringify(waited));
  assert.equal(polls, 2);
  const count = ledger.size;
  const replay = await session.launchApp("launch-parent", { name: "Calculator" });
  assert.equal(replay.details.ok, false);
  assert.equal(ledger.size, count, "retry must reuse its original durable ID");
});

test("Computer Use V2 creates immutable states and rejects a stale mutation before dispatch", async () => {
  let actions = 0;
  const session = new ComputerUseV2Session(async (_id, tool) => {
    if (tool === "desktop_observe") return envelope(observation("obs-1"));
    if (tool === "desktop_act_batch") {
      actions += 1;
      return envelope({
        content: [{ type: "text", text: "clicked" }],
        structuredContent: {
          verification: { effect: "confirmed", outcome: "worked" },
          execution: { steps: [{ outcome: "worked" }] },
          observation: observation("obs-2", "7").structuredContent,
        },
      });
    }
    throw new Error(`unexpected tool ${tool}`);
  });

  const observed = await session.observeUi("observe", {}, undefined);
  const stateId = observed.details.stateId;
  const acted = await session.actUi("act", {
    stateId,
    actions: [{ action: "press", ref: "@e1" }],
    expect: { ref: "@e2", value: "7" },
  });
  assert.equal(acted.details.execution.outcome, "worked");
  assert.equal(acted.details.execution.verification.status, "verified");
  assert.notEqual(acted.details.stateId, stateId);

  const stale = await session.actUi("stale", {
    stateId,
    actions: [{ action: "press", ref: "@e1" }],
  });
  assert.equal(stale.details.ok, false);
  assert.match(stale.content[0].text, /State is stale/);
  assert.equal(actions, 1);
});

test("Computer Use V2 cached search and inspect do not call the live backend", async () => {
  let calls = 0;
  const session = new ComputerUseV2Session(async () => {
    calls += 1;
    return envelope(observation("obs-1"));
  });
  const observed = await session.observeUi("observe", {}, undefined);
  assert.equal(calls, 1);

  const searched = session.searchUi("search", { stateId: observed.details.stateId, text: "7" });
  const inspected = session.inspectUi("inspect", { stateId: observed.details.stateId, ref: "@e1" });
  assert.equal(searched.details.matches.length, 1);
  assert.equal(inspected.details.element.legacyRef, "e1");
  assert.equal(calls, 1);
});

test("Computer Use V2 reports failed semantic postconditions honestly", async () => {
  const session = new ComputerUseV2Session(async (_id, tool) => {
    if (tool === "desktop_observe") return envelope(observation("obs-1", "0"));
    if (tool === "desktop_act_batch") {
      return envelope({
        content: [{ type: "text", text: "clicked" }],
        structuredContent: {
          verification: { effect: "confirmed", outcome: "worked" },
          execution: { steps: [{ outcome: "worked" }] },
          observation: observation("obs-2", "0").structuredContent,
        },
      });
    }
    throw new Error(`unexpected tool ${tool}`);
  });
  const observed = await session.observeUi("observe", {}, undefined);
  const acted = await session.actUi("act", {
    stateId: observed.details.stateId,
    actions: [{ action: "press", ref: "@e1" }],
    expect: { text: "Saved", timeoutMs: 100 },
  });
  assert.equal(acted.details.execution.outcome, "didnt");
  assert.equal(acted.details.execution.verification.status, "failed");
  assert.equal(acted.details.execution.verification.timedOut, true);
});

test("expect waits for delayed results and returns the final screenshot and native refs", async () => {
  let polls = 0;
  const session = new ComputerUseV2Session(async (id, tool) => {
    if (tool === "desktop_act_batch") return envelope({ structuredContent: {
      verification: { outcome: "worked" }, observation: observation("after-click").structuredContent,
    }, content: [{ type: "image", data: "old", mimeType: "image/png" }] });
    const parsed = observation(id, id.includes(":poll:") && ++polls >= 2 ? "15" : "0");
    parsed.structuredContent.elements[1].ref = `e${100 + polls}`;
    parsed.content = [{ type: "image", data: `image-${polls}`, mimeType: "image/png" }];
    return envelope(parsed);
  });
  const initial = await session.observeUi("initial", {});
  const acted = await session.actUi("click", {
    stateId: initial.details.stateId, actions: [{ action: "press", ref: "@e1" }],
    expect: { text: "display", value: "15", timeoutMs: 1500 },
  });
  assert.equal(acted.details.execution.verification.status, "verified", JSON.stringify(acted));
  assert.equal(acted.details.execution.verification.polls, 2);
  assert.equal(acted.content.find((block) => block.type === "image").data, "image-2");
  assert.equal(acted.details.elements[1].nativeRef, "e102");
});

test("observing two roots routes actions to their own window without relaunching", async () => {
  const calls = [];
  const session = new ComputerUseV2Session(async (_id, tool, _action, params) => {
    calls.push({ tool, params });
    if (tool === "desktop_list_apps") return envelope({ structuredContent: { windows: [
      { pid: 42, window_id: 7, root_ref: "w7" },
      { pid: 42, window_id: 8, root_ref: "w8" },
    ] } });
    const parsed = observation("look");
    Object.assign(parsed.structuredContent, { window_id: params.window_id, root_ref: params.root_ref });
    if (tool === "desktop_act_batch") return envelope({ structuredContent: {
      observation: parsed.structuredContent, verification: { outcome: "worked" },
    } });
    return envelope(parsed);
  });
  const roots = (await session.findRoots("find", {})).details.roots;
  const a = await session.observeUi("a", { root: roots[0].ref });
  const b = await session.observeUi("b", { root: roots[1].ref });
  assert.notEqual(a.details.resourceKey, b.details.resourceKey);
  const acted = await session.actUi("act-a", { stateId: a.details.stateId, actions: [{ action: "press", ref: "@e1" }] });
  assert.equal(acted.details.ok, true);
  assert.equal(calls.at(-1).params.window_id, 7);
  assert.equal(calls.at(-1).params.root_ref, "w7");
  assert.equal(calls.some((call) => call.tool === "desktop_open_app"), false);
});

test("two writes from same epoch: exactly one succeeds and second is rejected", async () => {
  let backendCalls = 0;
  const session = new ComputerUseV2Session(async (_id, tool) => {
    if (tool === "desktop_observe") return envelope(observation("obs-1"));
    if (tool === "desktop_act_batch") {
      backendCalls++;
      return envelope({
        structuredContent: {
          verification: { outcome: "worked" },
          observation: observation("obs-2", "1").structuredContent,
        },
      });
    }
    throw new Error(`unexpected tool ${tool}`);
  });

  const observed = await session.observeUi("observe", {});
  const stateId = observed.details.stateId;

  // Run two concurrent act_ui calls using the exact same stateId & epoch
  const [res1, res2] = await Promise.all([
    session.actUi("act-1", { stateId, actions: [{ action: "press", ref: "@e1" }] }),
    session.actUi("act-2", { stateId, actions: [{ action: "press", ref: "@e1" }] }),
  ]);

  const successes = [res1, res2].filter((r) => r.details.ok);
  const failures = [res1, res2].filter((r) => !r.details.ok);

  assert.equal(successes.length, 1);
  assert.equal(failures.length, 1);
  assert.match(failures[0].content[0].text, /State is stale/);
  assert.equal(backendCalls, 1);
});

test("state eviction fails clearly when accessing expired state", async () => {
  const session = new ComputerUseV2Session(async () => envelope(observation("obs")), { stateLimit: 2 });
  const s1 = await session.observeUi("obs-1", {});
  const s2 = await session.observeUi("obs-2", {});
  const s3 = await session.observeUi("obs-3", {});

  // s1 should have been evicted because stateLimit is 2
  assert.throws(() => session.state(s1.details.stateId), /unavailable/);
  assert.ok(session.state(s2.details.stateId));
  assert.ok(session.state(s3.details.stateId));
});

test("stable refs: removed elements disappear and new elements receive distinct new refs", async () => {
  let step = 1;
  const session = new ComputerUseV2Session(async () => {
    if (step === 1) {
      return envelope({
        structuredContent: {
          pid: 10,
          elements: [
            { ref: "native-1", role: "button", title: "OK" },
            { ref: "native-2", role: "button", title: "Cancel" },
          ],
        },
      });
    }
    return envelope({
      structuredContent: {
        pid: 10,
        elements: [
          { ref: "native-2-new", role: "button", title: "Cancel" },
          { ref: "native-3-new", role: "button", title: "Help" },
        ],
      },
    });
  });

  const first = await session.observeUi("obs-1", {});
  assert.equal(first.details.elements[0].ref, "@e1");
  assert.equal(first.details.elements[1].ref, "@e2");

  step = 2;
  const second = await session.observeUi("obs-2", {});
  // Cancel kept its identity and ref @e2
  assert.equal(second.details.elements[0].ref, "@e2");
  assert.equal(second.details.elements[0].title, "Cancel");
  // Help is brand new, receives @e3
  assert.equal(second.details.elements[1].ref, "@e3");
  assert.equal(second.details.elements[1].title, "Help");

  // Semantic diff
  const changes = second.details.changes;
  const removed = changes.find((c) => c.type === "removed");
  const added = changes.find((c) => c.type === "added");
  assert.equal(removed.ref, "@e1");
  assert.equal(added.ref, "@e3");
});

test("stable refs: ambiguous identities do not forcefully reuse old refs", async () => {
  let step = 1;
  const session = new ComputerUseV2Session(async () => {
    if (step === 1) {
      return envelope({
        structuredContent: {
          pid: 10,
          elements: [
            { ref: "btn-1", role: "button", title: "Option" },
          ],
        },
      });
    }
    // Step 2 has two identical buttons with identical roles, titles, and no distinctive coords
    return envelope({
      structuredContent: {
        pid: 10,
        elements: [
          { ref: "btn-a", role: "button", title: "Option" },
          { ref: "btn-b", role: "button", title: "Option" },
        ],
      },
    });
  });

  const first = await session.observeUi("obs-1", {});
  assert.equal(first.details.elements[0].ref, "@e1");

  step = 2;
  const second = await session.observeUi("obs-2", {});
  // Because two candidates in the next state matched identically with equal score, it is ambiguous
  // Ambiguous elements must not force-reuse @e1
  assert.notEqual(second.details.elements[0].ref, "@e1");
  assert.notEqual(second.details.elements[1].ref, "@e1");
});

test("multi-action batch stops on failed step and reports stoppedAt", async () => {
  const session = new ComputerUseV2Session(async (_id, tool) => {
    if (tool === "desktop_observe") return envelope(observation("obs-1"));
    if (tool === "desktop_act_batch") {
      return envelope({
        structuredContent: {
          verification: { effect: "unverifiable", outcome: "didnt" },
          execution: {
            outcome: "didnt",
            stoppedAt: 1,
            steps: [
              { outcome: "worked" },
              { outcome: "didnt", error: { code: "not_clickable", message: "Element obscured" } },
            ],
          },
          observation: observation("obs-after-failure").structuredContent,
        },
      });
    }
    throw new Error(`unexpected tool ${tool}`);
  });

  const initial = await session.observeUi("initial", {});
  const res = await session.actUi("batch-fail", {
    stateId: initial.details.stateId,
    actions: [
      { action: "press", ref: "@e1" },
      { action: "press", ref: "@e2" },
    ],
  });

  assert.equal(res.details.execution.outcome, "didnt");
  assert.equal(res.details.execution.stoppedAt, 1);
  assert.equal(res.details.execution.steps.length, 2);
  assert.ok(res.details.stateId);
});

test("expect reports preexisting when condition was already satisfied in baseState", async () => {
  const session = new ComputerUseV2Session(async (_id, tool) => {
    if (tool === "desktop_observe") return envelope(observation("obs-1", "Done"));
    if (tool === "desktop_act_batch") {
      return envelope({
        structuredContent: {
          verification: { outcome: "worked" },
          observation: observation("obs-2", "Done").structuredContent,
        },
      });
    }
    throw new Error(`unexpected tool ${tool}`);
  });

  const initial = await session.observeUi("initial", {});
  const res = await session.actUi("act", {
    stateId: initial.details.stateId,
    actions: [{ action: "press", ref: "@e1" }],
    expect: { ref: "@e2", value: "Done" },
  });

  assert.equal(res.details.execution.verification.status, "preexisting");
  assert.equal(res.details.execution.outcome, "worked");
});

test("concurrency: different resources may overlap while same resource serializes", async () => {
  const order = [];
  const session = new ComputerUseV2Session(async (_id, tool, _action, params) => {
    if (tool === "desktop_observe") {
      const isRootA = params.root_ref === "wA";
      order.push(`start-${isRootA ? "A" : "B"}`);
      await new Promise((r) => setTimeout(r, isRootA ? 50 : 10));
      order.push(`end-${isRootA ? "A" : "B"}`);
      const obs = observation("obs");
      obs.structuredContent.root_ref = params.root_ref;
      obs.structuredContent.window_id = isRootA ? 1 : 2;
      return envelope(obs);
    }
    if (tool === "desktop_list_apps") {
      return envelope({
        structuredContent: {
          windows: [
            { pid: 1, window_id: 1, root_ref: "wA" },
            { pid: 1, window_id: 2, root_ref: "wB" },
          ],
        },
      });
    }
    throw new Error(`unexpected tool ${tool}`);
  });

  const roots = (await session.findRoots("roots", {})).details.roots;
  // Concurrently observe root A and root B
  await Promise.all([
    session.observeUi("obs-A", { root: roots[0].ref }),
    session.observeUi("obs-B", { root: roots[1].ref }),
  ]);

  // Root B takes 10ms, Root A takes 50ms. Because they are on different resource keys,
  // B finished before A despite A starting first or at the same time!
  assert.ok(order.includes("end-B"));
  assert.ok(order.includes("end-A"));
  const bEndIndex = order.indexOf("end-B");
  const aEndIndex = order.indexOf("end-A");
  assert.ok(bEndIndex < aEndIndex, "different resources should not block each other");
});

test("Milestone B: find_roots coexists desktop, browser, and electron roots in unified UI forest", async () => {
  const session = new ComputerUseV2Session(
    async (_id, tool) => {
      if (tool === "desktop_list_apps") {
        return envelope({
          structuredContent: {
            windows: [
              { pid: 101, window_id: 1, root_ref: "win-notes", owner: "Notes", title: "Meeting Notes" },
            ],
          },
        });
      }
      throw new Error(`unexpected tool ${tool}`);
    },
    {
      cdpBackend: new CdpComputerUseBackend(async (_id, tool) => {
        if (tool === "browser_tabs") {
          return {
            tabs: [
              { index: 0, title: "AgentCabin Docs", url: "https://agentcabin.dev", active: true },
              { index: 1, title: "Visual Studio Code - Workspace", url: "file:///Users/dev/workspace", active: false },
            ],
          };
        }
        throw new Error(`unexpected browser tool ${tool}`);
      }),
    }
  );

  const res = await session.findRoots("find-roots-1", {});
  assert.equal(res.details.ok, true);
  const roots = res.details.roots;
  assert.equal(roots.length, 3);

  // Desktop window
  assert.equal(roots[0].ref, "@r1");
  assert.equal(roots[0].kind, ROOT_KIND_DESKTOP_WINDOW);
  assert.equal(roots[0].backend, BACKEND_MACOS_AX);
  assert.equal(roots[0].appName, "Notes");
  assert.equal(roots[0].pid, 101);

  // Browser page
  assert.equal(roots[1].ref, "@r2");
  assert.equal(roots[1].kind, ROOT_KIND_BROWSER_PAGE);
  assert.equal(roots[1].backend, BACKEND_CDP);
  assert.equal(roots[1].appName, "Chromium");
  assert.equal(roots[1].url, "https://agentcabin.dev");

  // Electron page
  assert.equal(roots[2].ref, "@r3");
  assert.equal(roots[2].kind, ROOT_KIND_ELECTRON_PAGE);
  assert.equal(roots[2].backend, BACKEND_CDP);
  assert.equal(roots[2].appName, "Electron");
  assert.equal(roots[2].url, "file:///Users/dev/workspace");

  // Agent visible text format
  assert.ok(res.content[0].text.includes("@r1 Notes — Meeting Notes pid=101"));
  assert.ok(res.content[0].text.includes("@r2 Chromium — AgentCabin Docs (https://agentcabin.dev)"));
  assert.ok(res.content[0].text.includes("@r3 Electron — Visual Studio Code - Workspace (file:///Users/dev/workspace)"));
});

test("Milestone B: CDP observe produces immutable state with stabilized @e refs and semantic diff", async () => {
  let snapshotCount = 0;
  const session = new ComputerUseV2Session(
    async () => envelope({}),
    {
      cdpBackend: new CdpComputerUseBackend(async (_id, tool) => {
        if (tool === "browser_tabs") {
          return { tabs: [{ index: 0, title: "Web App", url: "http://localhost:3000" }] };
        }
        if (tool === "browser_snapshot") {
          snapshotCount += 1;
          return {
            title: "Web App",
            url: "http://localhost:3000",
            refs: snapshotCount === 1 ? {
              "dom-search": { role: "textbox", name: "Search", value: "" },
              "dom-btn": { role: "button", name: "Submit", value: "Ready" },
            } : {
              "dom-search": { role: "textbox", name: "Search", value: "agentcabin" },
              "dom-btn": { role: "button", name: "Submit", value: "Loading" },
              "dom-result": { role: "article", name: "Result 1", value: "Found" },
            },
            screenshot: { base64: "data:image/png;base64,mockpng", mimeType: "image/png" },
          };
        }
        throw new Error(`unexpected browser tool ${tool}`);
      }),
    }
  );

  const roots = (await session.findRoots("roots", {})).details.roots;
  const root = roots[0];
  assert.equal(root.ref, "@r1");

  const obs1 = await session.observeUi("obs-1", { root: "@r1" });
  assert.equal(obs1.details.backend, BACKEND_CDP);
  assert.equal(obs1.details.resourceKey, "cdp-root:@r1");
  assert.equal(obs1.details.elements.length, 2);
  assert.equal(obs1.details.elements[0].ref, "@e1");
  assert.equal(obs1.details.elements[1].ref, "@e2");

  const obs2 = await session.observeUi("obs-2", { root: "@r1" });
  assert.equal(obs2.details.baseStateId, obs1.details.stateId);
  assert.equal(obs2.details.elements[0].ref, "@e1");
  assert.equal(obs2.details.elements[1].ref, "@e2");
  assert.equal(obs2.details.elements[2].ref, "@e3");

  // Semantic diff
  const changes = obs2.details.changes;
  assert.equal(changes.length, 3); // 2 updated (value changed), 1 added
  assert.ok(changes.some((c) => c.type === "added" && c.ref === "@e3"));
  assert.ok(changes.some((c) => c.type === "updated" && c.ref === "@e1"));
  assert.ok(changes.some((c) => c.type === "updated" && c.ref === "@e2"));
});

test("Milestone B: CDP act_ui executes semantic action on @e ref and checks postcondition expect", async () => {
  const browserCalls = [];
  let formSubmitted = false;

  const session = new ComputerUseV2Session(
    async () => envelope({}),
    {
      cdpBackend: new CdpComputerUseBackend(async (id, tool, params) => {
        browserCalls.push({ id, tool, params });
        if (tool === "browser_tabs") {
          return { tabs: [{ index: 0, title: "Form", url: "http://localhost:3000" }] };
        }
        if (tool === "browser_type") {
          return { ok: true };
        }
        if (tool === "browser_click") {
          formSubmitted = true;
          return { ok: true };
        }
        if (tool === "browser_snapshot") {
          return {
            title: "Form",
            url: "http://localhost:3000",
            refs: {
              "dom-input": { role: "textbox", name: "Username", value: formSubmitted ? "alice" : "" },
              "dom-submit": { role: "button", name: "Submit", value: formSubmitted ? "Success" : "Pending" },
            },
          };
        }
        throw new Error(`unexpected browser tool ${tool}`);
      }),
    }
  );

  const roots = (await session.findRoots("roots", {})).details.roots;
  const initial = await session.observeUi("obs", { root: roots[0].ref });

  const actRes = await session.actUi("act-1", {
    stateId: initial.details.stateId,
    actions: [
      { action: "setText", ref: "@e1", text: "alice" },
      { action: "press", ref: "@e2" },
    ],
    expect: { ref: "@e2", value: "Success" },
  });

  assert.equal(actRes.details.execution.outcome, "worked");
  assert.equal(actRes.details.execution.verification.status, "verified");

  // Verify internal translations used DOM refs, not public @e refs
  const typeCall = browserCalls.find((c) => c.tool === "browser_type");
  assert.equal(typeCall.params.ref, "dom-input");
  assert.equal(typeCall.params.text, "alice");

  const clickCall = browserCalls.find((c) => c.tool === "browser_click");
  assert.equal(clickCall.params.ref, "dom-submit");
});

test("Milestone B: CDP act_ui multi-action batch stops on step failure and reports stoppedAt", async () => {
  const session = new ComputerUseV2Session(
    async () => envelope({}),
    {
      cdpBackend: new CdpComputerUseBackend(async (_id, tool) => {
        if (tool === "browser_tabs") {
          return { tabs: [{ index: 0, title: "Test", url: "http://localhost:3000" }] };
        }
        if (tool === "browser_type") {
          return { ok: true };
        }
        if (tool === "browser_click") {
          throw new Error("Element is covered by a modal dialog");
        }
        if (tool === "browser_snapshot") {
          return {
            title: "Test",
            url: "http://localhost:3000",
            refs: {
              "dom-input": { role: "textbox", name: "Input", value: "" },
              "dom-btn": { role: "button", name: "Btn", value: "" },
            },
          };
        }
        throw new Error(`unexpected browser tool ${tool}`);
      }),
    }
  );

  const roots = (await session.findRoots("roots", {})).details.roots;
  const initial = await session.observeUi("obs", { root: roots[0].ref });

  const actRes = await session.actUi("act-batch", {
    stateId: initial.details.stateId,
    actions: [
      { action: "setText", ref: "@e1", text: "test" },
      { action: "press", ref: "@e2" },
      { action: "setText", ref: "@e1", text: "should-not-run" },
    ],
  });

  assert.equal(actRes.details.execution.outcome, "didnt");
  assert.equal(actRes.details.execution.stoppedAt, 1);
  assert.equal(actRes.details.execution.steps.length, 2);
  assert.equal(actRes.details.execution.steps[0].outcome, "worked");
  assert.equal(actRes.details.execution.steps[1].outcome, "didnt");
});

test("Milestone B: multiplexing desktop and CDP roots maintains independent epochs and isolation", async () => {
  const session = new ComputerUseV2Session(
    async (_id, tool, _action, params) => {
      if (tool === "desktop_list_apps") {
        return envelope({
          structuredContent: {
            windows: [{ pid: 50, window_id: 1, root_ref: "w-desktop", owner: "Calculator" }],
          },
        });
      }
      if (tool === "desktop_observe") {
        return envelope(observation("obs-desktop", "100"));
      }
      if (tool === "desktop_act_batch") {
        return envelope({
          structuredContent: {
            verification: { outcome: "worked" },
            observation: observation("obs-desktop-after", "200").structuredContent,
          },
        });
      }
      throw new Error(`unexpected desktop tool ${tool}`);
    },
    {
      cdpBackend: new CdpComputerUseBackend(async (_id, tool) => {
        if (tool === "browser_tabs") {
          return { tabs: [{ index: 0, title: "Browser Tab", url: "https://agentcabin.dev" }] };
        }
        if (tool === "browser_snapshot") {
          return {
            title: "Browser Tab",
            url: "https://agentcabin.dev",
            refs: { "btn-search": { role: "button", name: "Search", value: "Go" } },
          };
        }
        if (tool === "browser_click") {
          return { ok: true };
        }
        throw new Error(`unexpected browser tool ${tool}`);
      }),
    }
  );

  const roots = (await session.findRoots("roots", {})).details.roots;
  assert.equal(roots.length, 2);
  const desktopRoot = roots[0];
  const browserRoot = roots[1];

  // Observe both roots
  const stateDesktop0 = await session.observeUi("obs-d", { root: desktopRoot.ref });
  const stateBrowser0 = await session.observeUi("obs-b", { root: browserRoot.ref });

  assert.equal(stateDesktop0.details.resourceKey, "desktop-root:@r1");
  assert.equal(stateBrowser0.details.resourceKey, "cdp-root:@r2");
  assert.equal(stateDesktop0.details.epoch, 0);
  assert.equal(stateBrowser0.details.epoch, 0);

  // Act on desktop root -> advances desktop epoch to 1
  const actDesktop = await session.actUi("act-d", {
    stateId: stateDesktop0.details.stateId,
    actions: [{ action: "press", ref: "@e1" }],
  });
  assert.equal(actDesktop.details.epoch, 1);

  // Now act on browser root using its epoch 0 state -> SUCCEEDS because browser root has independent epoch!
  const actBrowser = await session.actUi("act-b", {
    stateId: stateBrowser0.details.stateId,
    actions: [{ action: "press", ref: "@e1" }],
  });
  assert.equal(actBrowser.details.epoch, 1);

  // Attempting to act on desktop root with old state (epoch 0) returns fail-closed stale error
  const staleDesktop = await session.actUi("act-d-stale", {
    stateId: stateDesktop0.details.stateId,
    actions: [{ action: "press", ref: "@e1" }],
  });
  assert.equal(staleDesktop.details.ok, false);
  assert.match(staleDesktop.content[0].text, /State is stale/);
});

test("Milestone B Hardening: StateStore domain repository manages state persistence and bounded eviction", () => {
  const store = new StateStore(3);
  const s1 = { stateId: "id-1", resourceKey: "res-A", epoch: 0 };
  const s2 = { stateId: "id-2", resourceKey: "res-A", epoch: 1 };
  const s3 = { stateId: "id-3", resourceKey: "res-B", epoch: 0 };
  const s4 = { stateId: "id-4", resourceKey: "res-B", epoch: 1 };

  store.saveState(s1);
  store.saveState(s2);
  store.saveState(s3);
  assert.equal(store.size, 3);
  assert.equal(store.getState("id-1"), s1);
  assert.equal(store.getLatestState("res-A"), s2);

  // Exceed limit -> s1 should be evicted
  store.saveState(s4);
  assert.equal(store.size, 3);
  assert.equal(store.hasState("id-1"), false);
  assert.throws(() => store.getState("id-1"), /unavailable/);
  assert.equal(store.getState("id-4"), s4);
  assert.equal(store.getLatestState("res-B"), s4);
});

test("Milestone B Hardening: probeElectronCdp connects to live debugging endpoint and falls back to null when closed", async () => {
  // Test 1: Closed port / network error -> returns null (fallback to AX)
  const fallback = await probeElectronCdp(12345, 65432, 100, async () => {
    throw new Error("connect ECONNREFUSED 127.0.0.1:65432");
  });
  assert.equal(fallback, null, "unreachable port must return null for AX fallback");

  // Test 2: Active Electron CDP port -> parses metadata and returns endpoint
  const detected = await probeElectronCdp(9999, 9222, 500, async (url) => {
    assert.equal(url, "http://127.0.0.1:9222/json/version");
    return {
      ok: true,
      json: async () => ({
        Browser: "Chrome/120.0.0.0",
        webSocketDebuggerUrl: "ws://127.0.0.1:9222/devtools/browser/xyz",
      }),
    };
  });
  assert.ok(detected, "active CDP port must be detected");
  assert.equal(detected.port, 9222);
  assert.equal(detected.browser, "Chrome/120.0.0.0");
  assert.equal(detected.webSocketDebuggerUrl, "ws://127.0.0.1:9222/devtools/browser/xyz");
});

test("Milestone A Regression: RefStabilizer enforces strict one-to-one matching without shared @e refs", () => {
  const stabilizer = new RefStabilizer();
  const baseState = {
    elements: [
      { ref: "@e1", role: "button", title: "Save", rect: { x: 100, y: 100, width: 60, height: 25 } },
    ],
  };

  // Two successor elements: one is a strong match, the other also matches role & title (score >= 45)
  const nextElements = [
    { role: "button", title: "Save", rect: { x: 100, y: 100, width: 60, height: 25 } },
    { role: "button", title: "Save", rect: { x: 400, y: 500, width: 80, height: 30 } },
  ];

  const stabilized = stabilizer.stabilize(nextElements, baseState);
  assert.equal(stabilized.length, 2);
  assert.equal(stabilized[0].ref, "@e1", "top-scoring match must retain @e1");
  assert.notEqual(stabilized[1].ref, "@e1", "second element must NOT steal or share @e1");
  assert.equal(stabilized[1].ref, "@e2", "second element must receive fresh @e2");

  // Verify all refs in stabilized list are unique
  const refs = stabilized.map((el) => el.ref);
  assert.equal(new Set(refs).size, refs.length, "every element must have a unique public ref");
});

test("Milestone B Fix: launchApp correctly handles UiRoot[] array returned from cdpBackend.listRoots", async () => {
  const mockCdpBackend = {
    invokeBrowser: async () => ({ ok: true }),
    listRoots: async () => [
      {
        backend: BACKEND_CDP,
        kind: ROOT_KIND_BROWSER_PAGE,
        title: "Test Tab",
        appName: "Chromium",
        browserTargetId: "target-123",
        url: "https://example.com",
      },
    ],
    observe: async () => ({
      elements: [{ ref: "cdp-1", role: "link", title: "Example" }],
    }),
  };

  const session = new ComputerUseV2Session(async () => envelope(observation("obs")), {
    cdpBackend: mockCdpBackend,
  });

  const launchRes = await session.launchApp("call-1", { name: "browser", url: "https://example.com" });
  assert.equal(launchRes.details?.ok, true);
  assert.ok(launchRes.details?.stateId);
  const elements = launchRes.details?.elements || [];
  assert.ok(elements.length > 0);
  assert.equal(elements[0].title, "Example");
});

test("Regression 1: Unified find_roots invokes DesktopComputerUseBackend classification", async () => {
  let desktopBackendInvoked = false;
  const mockDesktopBackend = {
    listRoots: async () => {
      desktopBackendInvoked = true;
      return [
        {
          backend: BACKEND_MACOS_AX,
          kind: ROOT_KIND_DESKTOP_WINDOW,
          appName: "TextEdit",
          title: "Untitled",
          pid: 1234,
          windowId: 10,
          nativeRootRef: "win-10",
        },
      ];
    },
  };

  const session = new ComputerUseV2Session(async () => envelope(observation("obs")), {
    desktopBackend: mockDesktopBackend,
  });

  const rootsRes = await session.findRoots("call-roots", {});
  assert.equal(desktopBackendInvoked, true, "find_roots must delegate root enumeration to desktopBackend.listRoots");
  assert.equal(rootsRes.details.roots.length, 1);
  assert.equal(rootsRes.details.roots[0].appName, "TextEdit");
  assert.equal(rootsRes.details.roots[0].ref, "@r1");
});

test("Regression 2: Electron PID probe reaches production find_roots path", async () => {
  const session = new ComputerUseV2Session(async (_id, tool) => {
    if (tool === "desktop_list_apps") {
      return envelope({
        structuredContent: {
          windows: [
            {
              pid: 9999,
              window_id: 200,
              appName: "Slack",
              title: "General",
              root_ref: "slack-root",
              debuggingPort: 9222, // Electron CDP hint
            },
          ],
        },
      });
    }
    return envelope(observation("obs"));
  });

  // Override probeElectronCdp response for test
  session.desktopBackend.options = { fetcher: async () => ({ ok: true, json: async () => ({ webSocketDebuggerUrl: "ws://127.0.0.1:9222/devtools/page/abc" }) }) };

  const rootsRes = await session.findRoots("call-electron-roots", {});
  const electronRoot = rootsRes.details.roots.find((r) => r.appName === "Slack");
  assert.ok(electronRoot, "Slack root should be found");
  assert.equal(electronRoot.kind, ROOT_KIND_ELECTRON_PAGE, "Electron root must be classified as ROOT_KIND_ELECTRON_PAGE");
  assert.equal(electronRoot.backend, BACKEND_CDP, "Electron root must use BACKEND_CDP");
});

test("Regression 3: stale browser targetId fails closed without falling back to index 0", async () => {
  const cdpBackend = new CdpComputerUseBackend(async (toolCallId, toolName, params) => {
    if (toolName === "browser_tabs" && params.action === "switch") {
      if (params.targetId === "closed-target-id") {
        throw new Error("stale_browser_target: Browser target 'closed-target-id' not found.");
      }
    }
    return { ok: true, refs: {}, title: "Page", url: "https://example.com" };
  });

  const staleRoot = {
    ref: "@r2",
    backend: BACKEND_CDP,
    browserTargetId: "closed-target-id",
    title: "Closed Tab",
  };

  await assert.rejects(
    async () => {
      await cdpBackend.observe(staleRoot, {});
    },
    (err) => {
      assert.ok(String(err).includes("stale_browser_target"), "Must fail closed with stale_browser_target");
      assert.ok(String(err).includes("Call find_roots again"), "Must prompt to call find_roots");
      return true;
    },
  );
});

test("Regression 4: reordered tabs preserve correct @r -> targetId mapping", async () => {
  let activeTargetId = null;
  const mockTabs = [
    { targetId: "target-B", title: "Tab B", index: 0 },
    { targetId: "target-A", title: "Tab A", index: 1 }, // Tab A moved to index 1
  ];

  const cdpBackend = new CdpComputerUseBackend(async (_id, toolName, params) => {
    if (toolName === "browser_tabs") {
      if (params.action === "list") return { ok: true, tabs: mockTabs };
      if (params.action === "switch") {
        activeTargetId = params.targetId;
        return { ok: true, targetId: params.targetId };
      }
    }
    return { ok: true, refs: {}, title: "Tab A", url: "https://example.com" };
  });

  const session = new ComputerUseV2Session(async () => envelope(observation("obs")), {
    cdpBackend,
  });

  const rootsRes = await session.findRoots("roots", {});
  const rootA = rootsRes.details.roots.find((r) => r.browserTargetId === "target-A");
  assert.ok(rootA);

  // Observing root A must target target-A regardless of index shift
  await session.observeUi("obs", { root: rootA.ref });
  assert.equal(activeTargetId, "target-A", "Must switch using targetId target-A, not stale index 0");
});

test("Regression 5: CDP switch failure propagates to observe_ui / act_ui requiring find_roots", async () => {
  const cdpBackend = new CdpComputerUseBackend(async (_id, toolName, params) => {
    if (toolName === "browser_tabs" && params.action === "switch") {
      throw new Error("Target closed");
    }
    return { ok: true };
  });

  const session = new ComputerUseV2Session(async () => envelope(observation("obs")), {
    cdpBackend,
  });

  session.registerRoot({
    ref: "@r1",
    backend: BACKEND_CDP,
    kind: ROOT_KIND_BROWSER_PAGE,
    browserTargetId: "defunct-target",
    title: "Defunct",
  });

  const obsRes = await session.observeUi("obs", { root: "@r1" });
  assert.equal(obsRes.details.ok, false);
  assert.ok(obsRes.content[0].text.includes("stale_browser_target"));
  assert.ok(obsRes.content[0].text.includes("Call find_roots again"));
});

test("Regression 6: external Electron roots route CDP observe and act through Browser Worker", async () => {
  const browserCalls = [];
  const cdpBackend = new CdpComputerUseBackend(async (toolCallId, toolName, params) => {
    browserCalls.push({ toolCallId, toolName, params });
    if (toolName === "browser_cdp_observe") {
      return {
        ok: true,
        backend: BACKEND_CDP,
        title: "Electron App",
        url: "file:///app/index.html",
        elements: [{ ref: "dom:0", role: "button", title: "Run", nativeRef: "dom:0" }],
        images: [],
      };
    }
    if (toolName === "browser_cdp_act") {
      return {
        ok: true,
        outcome: "worked",
        execution: { outcome: "worked", steps: [{ outcome: "worked" }] },
        observation: {
          backend: BACKEND_CDP,
          title: "Electron App",
          url: "file:///app/index.html",
          elements: [{ ref: "dom:0", role: "button", title: "Run", nativeRef: "dom:0" }],
          images: [],
        },
      };
    }
    throw new Error(`unexpected browser tool ${toolName}`);
  });
  const root = {
    ref: "@r1",
    resourceKey: "electron:target",
    backend: BACKEND_CDP,
    kind: ROOT_KIND_ELECTRON_PAGE,
    cdpEndpoint: "ws://127.0.0.1:9222/devtools/browser/test",
    cdpTargetId: "electron-target",
    cdpPageUrl: "file:///app/index.html",
    browserTargetId: "electron-target",
    url: "file:///app/index.html",
  };
  const observed = await cdpBackend.observe(root);
  assert.equal(observed.elements[0].nativeRef, "dom:0");
  const acted = await cdpBackend.act(root, {
    elements: [{ ref: "@e1", nativeRef: "dom:0" }],
  }, { actions: [{ action: "click", ref: "@e1" }] });
  assert.equal(acted.outcome, "worked");
  assert.deepEqual(browserCalls.map((call) => call.toolName), ["browser_cdp_observe", "browser_cdp_act"]);
  assert.equal(browserCalls[0].params.cdpTargetId, "electron-target");
  assert.equal(browserCalls[1].params.actions[0].ref, "dom:0");
});

test("Regression 7: external CDP failures remain stale instead of selecting another Electron page", async () => {
  const cdpBackend = new CdpComputerUseBackend(async (_toolCallId, toolName) => {
    assert.equal(toolName, "browser_cdp_observe");
    throw new Error("stale_browser_target: Electron target 'closed-target' was not found. Call find_roots again.");
  });
  await assert.rejects(
    () => cdpBackend.observe({
      resourceKey: "electron:closed-target",
      cdpEndpoint: "ws://127.0.0.1:9222/devtools/browser/test",
      cdpTargetId: "closed-target",
      browserTargetId: "closed-target",
      url: "file:///app/index.html",
    }),
    /stale_browser_target: Electron target 'closed-target'/,
  );
});
