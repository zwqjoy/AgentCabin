import test from "node:test";
import assert from "node:assert/strict";

import {
  FAILURE_CATEGORIES,
  ALL_FAILURE_CATEGORIES,
  classifyFailure,
  validateEvalCase,
} from "./eval_types.mjs";
import { MACOS_EVAL_CASES } from "./eval_cases_macos.mjs";
import {
  ComputerUseEvalRunner,
  RealComputerUseEvalSession,
  createRealSessionFactory,
} from "./eval_runner.mjs";
import { ComputerUseV2Session } from "../computer_use_v2_runtime.mjs";
import {
  createEvalFixtureServer,
  createLiveBrowserInvoker,
  parseBrowserBridgeResponse,
} from "./run_live.mjs";

test("Milestone C: Failure taxonomy covers all 13 canonical failure categories", () => {
  assert.equal(ALL_FAILURE_CATEGORIES.length, 13);
  assert.ok(ALL_FAILURE_CATEGORIES.includes("root_not_found"));
  assert.ok(ALL_FAILURE_CATEGORIES.includes("stale_state"));
  assert.ok(ALL_FAILURE_CATEGORIES.includes("wrong_grounding"));
  assert.ok(ALL_FAILURE_CATEGORIES.includes("element_not_found"));
  assert.ok(ALL_FAILURE_CATEGORIES.includes("action_rejected"));
  assert.ok(ALL_FAILURE_CATEGORIES.includes("timeout_waiting"));
  assert.ok(ALL_FAILURE_CATEGORIES.includes("postcondition_failed"));
  assert.ok(ALL_FAILURE_CATEGORIES.includes("unexpected_modal"));
  assert.ok(ALL_FAILURE_CATEGORIES.includes("coordinate_out_of_bounds"));
  assert.ok(ALL_FAILURE_CATEGORIES.includes("daemon_crash"));
  assert.ok(ALL_FAILURE_CATEGORIES.includes("permission_denied"));
  assert.ok(ALL_FAILURE_CATEGORIES.includes("unsupported_action"));
  assert.ok(ALL_FAILURE_CATEGORIES.includes("flaky_success"));
});

test("Milestone C: Failure classification attributes errors accurately to taxonomies", () => {
  assert.equal(
    classifyFailure(new Error("Accessibility permission is missing.")),
    FAILURE_CATEGORIES.PERMISSION_DENIED
  );
  assert.equal(
    classifyFailure(new Error("Unknown root '@r99'. Call find_roots again.")),
    FAILURE_CATEGORIES.ROOT_NOT_FOUND
  );
  assert.equal(
    classifyFailure(new Error("State is stale for desktop-root:@r1: expected epoch 0, current epoch 1.")),
    FAILURE_CATEGORIES.STALE_STATE
  );
  assert.equal(
    classifyFailure(new Error("Coordinate action target stateId 's1' does not match active state 's2'. Cross-state coordinate reuse is prohibited.")),
    FAILURE_CATEGORIES.COORDINATE_OUT_OF_BOUNDS
  );
  assert.equal(
    classifyFailure(new Error("Native daemon pipe closed unexpectedly.")),
    FAILURE_CATEGORIES.DAEMON_CRASH
  );
  assert.equal(
    classifyFailure(new Error("Action 'swipe' is not supported by the current native backend.")),
    FAILURE_CATEGORIES.UNSUPPORTED_ACTION
  );
  assert.equal(
    classifyFailure(new Error("Element '@e99' is not owned by state s1.")),
    FAILURE_CATEGORIES.ELEMENT_NOT_FOUND
  );
  assert.equal(
    classifyFailure(new Error("Unexpected modal dialog interrupted action.")),
    FAILURE_CATEGORIES.UNEXPECTED_MODAL
  );
  assert.equal(
    classifyFailure(new Error("Wrong grounding: clicked target element was mismatched.")),
    FAILURE_CATEGORIES.WRONG_GROUNDING
  );
  assert.equal(
    classifyFailure(new Error("Action timed out."), { verification: { timedOut: true } }),
    FAILURE_CATEGORIES.TIMEOUT_WAITING
  );
  assert.equal(
    classifyFailure(new Error("Verification failed."), { outcome: "didnt" }),
    FAILURE_CATEGORIES.POSTCONDITION_FAILED
  );
  assert.equal(
    classifyFailure(new Error("Native action rejected by window manager.")),
    FAILURE_CATEGORIES.ACTION_REJECTED
  );
  assert.equal(
    classifyFailure(new Error("Detected flaky success on unstable ref.")),
    FAILURE_CATEGORIES.FLAKY_SUCCESS
  );
});

test("Milestone C: All 25 curated macOS eval cases satisfy schema requirements", () => {
  assert.equal(MACOS_EVAL_CASES.length, 25);

  const categories = new Set(MACOS_EVAL_CASES.map((c) => c.category));
  assert.ok(categories.has("calculator"));
  assert.ok(categories.has("textedit"));
  assert.ok(categories.has("finder"));
  assert.ok(categories.has("chrome"));
  assert.ok(categories.has("cross_app"));

  for (const caseDef of MACOS_EVAL_CASES) {
    assert.doesNotThrow(() => validateEvalCase(caseDef));
  }
});

test("Milestone C: ComputerUseEvalRunner runs cases and generates comprehensive metrics report", async () => {
  const runner = new ComputerUseEvalRunner();
  const report = await runner.runAll();

  assert.equal(report.totalCases, 25);
  assert.equal(report.passed, 25);
  assert.equal(report.failed, 0);
  assert.equal(report.successRate, 100);
  assert.ok(report.avgToolCalls > 0);
  assert.ok(report.avgLatencyMs >= 0);
  assert.ok(Array.isArray(report.results));
  assert.equal(report.results.length, 25);
});

test("Milestone C: EvalRunner captures failure attribution and computes failure distribution", async () => {
  const faultyCases = [
    {
      id: "faulty_01",
      title: "Permission Failure Case",
      category: "calculator",
      targetApp: "Calculator",
      prompt: "Test permission denial",
      initialState: {},
      groundTruthOutcome: { expect: { text: "42" } },
    },
    {
      id: "faulty_02",
      title: "Stale State Case",
      category: "calculator",
      targetApp: "Calculator",
      prompt: "Test stale state rejection",
      initialState: {},
      groundTruthOutcome: { expect: { text: "42" } },
    },
  ];

  const runner = new ComputerUseEvalRunner({
    cases: faultyCases,
    sessionFactory: (c) => ({
      state: () => ({ elements: [] }),
      observeUi: async () => {
        if (c.id === "faulty_01") {
          throw new Error("Accessibility permission denied by macOS TCC.");
        }
        throw new Error("State is stale for desktop:current. Observe again.");
      },
      actUi: async () => ({ details: { ok: true } }),
    }),
  });

  const report = await runner.runAll();
  assert.equal(report.totalCases, 2);
  assert.equal(report.passed, 0);
  assert.equal(report.failed, 2);
  assert.equal(report.successRate, 0);
  assert.equal(report.failureDistribution[FAILURE_CATEGORIES.PERMISSION_DENIED], 1);
  assert.equal(report.failureDistribution[FAILURE_CATEGORIES.STALE_STATE], 1);
});

test("Milestone C: EvalRunner instruments wrongClicks on out-of-bounds, nonexistent, or rejected clicks", async () => {
  const caseWithMistakes = {
    id: "mistake_case",
    title: "Wrong Click Case",
    category: "calculator",
    targetApp: "Calculator",
    prompt: "Click nonexistent element and out-of-bounds coord",
    initialState: {
      elements: [
        { ref: "@e1", role: "button", title: "Valid Button", rect: { x: 50, y: 50, width: 100, height: 40 } },
      ],
    },
    actions: [
      { action: "click", ref: "@e999_nonexistent" },
      { action: "click", x: 9999, y: 9999 }, // Out of bounds of all elements
      { action: "click", ref: "@e1" }, // Valid
    ],
    groundTruthOutcome: { expect: { text: "Valid Button" } },
  };

  const runner = new ComputerUseEvalRunner({
    cases: [caseWithMistakes],
  });

  const res = await runner.runCase(caseWithMistakes);
  assert.ok(res.wrongClicks >= 2, `Expected wrongClicks >= 2, got ${res.wrongClicks}`);

  const report = await runner.runAll();
  assert.ok(report.wrongClickRate > 0, `Expected wrongClickRate > 0, got ${report.wrongClickRate}`);
});

test("Milestone C: RealComputerUseEvalSession bridges ComputerUseV2Session with dynamic element resolution", async () => {
  // Simulated backend providing realistic Calculator window
  let calcValue = "0";
  const mockBackend = async (_callId, tool, action, params) => {
    if (tool === "desktop_open_app") {
      return {
        success: true,
        stdout: JSON.stringify({
          pid: 501,
          window_id: 10,
          root_ref: "win-calc",
          app_name: "Calculator",
          title: "Calculator",
        }),
      };
    }
    if (tool === "desktop_observe") {
      return {
        success: true,
        stdout: JSON.stringify({
          pid: 501,
          window_id: 10,
          root_ref: "win-calc",
          app_name: "Calculator",
          title: "Calculator",
          elements: [
            { ref: "btn1", role: "button", title: "1", rect: { x: 10, y: 10, width: 30, height: 30 } },
            { ref: "btnPlus", role: "button", title: "+", rect: { x: 50, y: 10, width: 30, height: 30 } },
            { ref: "btn2", role: "button", title: "2", rect: { x: 90, y: 10, width: 30, height: 30 } },
            { ref: "btnEq", role: "button", title: "=", rect: { x: 130, y: 10, width: 30, height: 30 } },
            { ref: "display", role: "text", title: "Display", value: calcValue, rect: { x: 10, y: 50, width: 150, height: 30 } },
          ],
        }),
      };
    }
    if (tool === "desktop_act_batch" || tool === "desktop_act") {
      calcValue = "3";
      return {
        success: true,
        stdout: JSON.stringify({
          verification: { outcome: "worked" },
          observation: {
            pid: 501,
            window_id: 10,
            root_ref: "win-calc",
            app_name: "Calculator",
            title: "Calculator",
            elements: [
              { ref: "btn1", role: "button", title: "1", rect: { x: 10, y: 10, width: 30, height: 30 } },
              { ref: "btnPlus", role: "button", title: "+", rect: { x: 50, y: 10, width: 30, height: 30 } },
              { ref: "btn2", role: "button", title: "2", rect: { x: 90, y: 10, width: 30, height: 30 } },
              { ref: "btnEq", role: "button", title: "=", rect: { x: 130, y: 10, width: 30, height: 30 } },
              { ref: "display", role: "text", title: "Display", value: "3", rect: { x: 10, y: 50, width: 150, height: 30 } },
            ],
          },
          execution: { outcome: "worked", stoppedAt: undefined },
        }),
      };
    }
    return { success: true, stdout: "{}" };
  };

  const v2Session = new ComputerUseV2Session(mockBackend);
  const evalCase = {
    id: "real_calc_test",
    title: "1 + 2 = 3",
    category: "calculator",
    targetApp: "Calculator",
    prompt: "Calculate 1 + 2 = 3",
    initialState: {
      elements: [
        { ref: "@e1", title: "1" },
        { ref: "@e2", title: "+" },
        { ref: "@e3", title: "2" },
        { ref: "@e4", title: "=" },
      ],
    },
    actions: [
      { action: "click", ref: "@e1" },
      { action: "click", ref: "@e2" },
      { action: "click", ref: "@e3" },
      { action: "click", ref: "@e4" },
    ],
    groundTruthOutcome: { expect: { text: "3" } },
  };

  const realRunner = new ComputerUseEvalRunner({
    cases: [evalCase],
    sessionFactory: createRealSessionFactory(v2Session),
  });

  const result = await realRunner.runCase(evalCase);
  assert.equal(result.passed, true);
  assert.equal(result.wrongClicks, 0);
});

test("Regression 8: Real live eval session does not use mock session factory", async () => {
  let v2ActCalled = false;
  const v2Session = new ComputerUseV2Session(async (_id, tool) => {
    if (tool === "desktop_open_app" || tool === "desktop_list_apps") {
      return {
        success: true,
        stdout: JSON.stringify({
          windows: [{ pid: 100, window_id: 1, app_name: "Calculator", root_ref: "r1" }],
        }),
      };
    }
    if (tool === "desktop_observe") {
      return {
        success: true,
        stdout: JSON.stringify({
          pid: 100,
          window_id: 1,
          root_ref: "r1",
          elements: [{ ref: "btn1", role: "button", title: "1" }],
        }),
      };
    }
    if (tool === "desktop_act_batch") {
      v2ActCalled = true;
      return {
        success: true,
        stdout: JSON.stringify({
          verification: { outcome: "worked" },
          observation: {
            pid: 100,
            window_id: 1,
            root_ref: "r1",
            elements: [{ ref: "disp", role: "text", title: "1", value: "1" }],
          },
          execution: { outcome: "worked" },
        }),
      };
    }
    return { success: true, stdout: "{}" };
  });

  const evalCase = {
    id: "real_session_test",
    title: "Verify real pipeline dispatch",
    category: "calculator",
    targetApp: "Calculator",
    prompt: "Click 1",
    actions: [{ action: "click", targetTitle: "1" }],
    groundTruthOutcome: { expect: { text: "1" } },
  };

  const runner = new ComputerUseEvalRunner({
    cases: [evalCase],
    sessionFactory: createRealSessionFactory(v2Session),
  });

  const res = await runner.runCase(evalCase);
  assert.equal(v2ActCalled, true, "RealComputerUseEvalSession must invoke real ComputerUseV2Session");
  assert.equal(res.passed, true);
  // Real session must NOT inject mock element @e999
  const state = v2Session.state(v2Session.latestStates.values().next().value.stateId);
  assert.equal(state.elements.some((e) => e.ref === "@e999"), false, "Must not inject synthetic mock elements");
});

test("Regression 9: real cross-app case changes second app state", async () => {
  let textEditReceivedText = null;

  const v2Session = new ComputerUseV2Session(async (_id, tool, _action, params) => {
    if (tool === "desktop_open_app" || tool === "desktop_list_apps") {
      return {
        success: true,
        stdout: JSON.stringify({
          windows: [
            { pid: 101, window_id: 1, app_name: "Calculator", root_ref: "calc-win" },
            { pid: 102, window_id: 2, app_name: "TextEdit", root_ref: "textedit-win" },
          ],
        }),
      };
    }
    if (tool === "desktop_observe") {
      const isCalc = params.root_ref === "calc-win";
      return {
        success: true,
        stdout: JSON.stringify({
          pid: isCalc ? 101 : 102,
          window_id: isCalc ? 1 : 2,
          root_ref: params.root_ref,
          elements: isCalc
            ? [{ ref: "disp", role: "text", title: "81", value: "81" }]
            : [{ ref: "doc", role: "textarea", title: "Document", value: textEditReceivedText || "" }],
        }),
      };
    }
    if (tool === "desktop_act_batch") {
      for (const act of params.actions || []) {
        const text = act.text || act.params?.text;
        if (act.action === "typeText" || act.action === "setText" || act.intent === "type_text" || act.params?.native_action === "typeText" || text) {
          textEditReceivedText = text;
        }
      }
      return {
        success: true,
        stdout: JSON.stringify({
          verification: { outcome: "worked" },
          observation: {
            pid: 102,
            window_id: 2,
            root_ref: "textedit-win",
            elements: [{ ref: "doc", role: "textarea", title: "Document", value: textEditReceivedText }],
          },
          execution: { outcome: "worked" },
        }),
      };
    }
    return { success: true, stdout: "{}" };
  });

  // Step 1: Discover roots and read from Calculator
  const roots = (await v2Session.findRoots("roots", {})).details.roots;
  const calcRoot = roots.find((r) => r.appName === "Calculator");
  const textEditRoot = roots.find((r) => r.appName === "TextEdit");

  const calcObs = await v2Session.observeUi("obs-calc", { root: calcRoot.ref });
  const calcText = calcObs.details.elements.find((e) => e.value === "81")?.value;
  assert.equal(calcText, "81", "Calculator output must be read as 81");

  // Step 2: Write into TextEdit
  const teObs = await v2Session.observeUi("obs-te", { root: textEditRoot.ref });
  const teAct = await v2Session.actUi("act-te", {
    stateId: teObs.details.stateId,
    actions: [{ action: "typeText", ref: teObs.details.elements[0].ref, text: calcText }],
    expect: { text: "81" },
  });

  assert.equal(teAct.details.ok, true);
  assert.equal(textEditReceivedText, "81", "Second app (TextEdit) must receive the data calculated in app 1");
  const teFinal = v2Session.state(teAct.details.stateId);
  assert.ok(teFinal.elements.some((e) => e.value === "81"), "TextEdit successor state must retain 81");
});

test("Regression 10: wrong-click metric does not classify all postcondition failures as wrong clicks", async () => {
  const v2Session = new ComputerUseV2Session(async (_id, tool) => {
    if (tool === "desktop_open_app" || tool === "desktop_list_apps") {
      return {
        success: true,
        stdout: JSON.stringify({
          windows: [{ pid: 200, window_id: 1, app_name: "Calculator", root_ref: "r1" }],
        }),
      };
    }
    if (tool === "desktop_observe") {
      return {
        success: true,
        stdout: JSON.stringify({
          pid: 200,
          window_id: 1,
          root_ref: "r1",
          elements: [{ ref: "btn1", role: "button", title: "1", rect: { x: 10, y: 10, width: 30, height: 30 } }],
        }),
      };
    }
    if (tool === "desktop_act_batch") {
      // Click succeeded on valid button, but display did not change to expected text
      return {
        success: true,
        stdout: JSON.stringify({
          verification: { outcome: "didnt", status: "failed", reason: "display showed 0 instead of 42" },
          observation: {
            pid: 200,
            window_id: 1,
            root_ref: "r1",
            elements: [{ ref: "disp", role: "text", title: "0", value: "0" }],
          },
          execution: { outcome: "worked" },
        }),
      };
    }
    return { success: true, stdout: "{}" };
  });

  const evalCase = {
    id: "postcondition_only_failure",
    title: "Valid click but postcondition failed",
    category: "calculator",
    targetApp: "Calculator",
    prompt: "Click 1 expecting 42",
    actions: [{ action: "click", targetTitle: "1" }],
    groundTruthOutcome: { expect: { text: "42", timeoutMs: 100 } },
  };

  const runner = new ComputerUseEvalRunner({
    cases: [evalCase],
    sessionFactory: createRealSessionFactory(v2Session),
  });

  const res = await runner.runCase(evalCase);
  assert.equal(res.passed, false);
  // Postcondition failure with valid click must NOT be counted as wrong click!
  assert.equal(res.wrongClicks, 0, "Postcondition failure on valid element must have wrongClicks = 0");
  assert.equal(res.failureCategory, FAILURE_CATEGORIES.POSTCONDITION_FAILED, "Must be classified as postcondition_failed");
});

test("Regression 11: live eval serves fixtures from an ephemeral loopback HTTP origin", async () => {
  const fixtureServer = await createEvalFixtureServer();
  try {
    assert.match(fixtureServer.origin, /^http:\/\/127\.0\.0\.1:\d+$/);
    const response = await fetch(fixtureServer.urlFor("fixtures/computer-use/form.html"));
    assert.equal(response.status, 200);
    assert.match(await response.text(), /Submit Form/);
    assert.throws(
      () => fixtureServer.urlFor("fixtures/computer-use/../secret.html"),
      /Unsupported live eval fixture path/,
    );
  } finally {
    await fixtureServer.close();
  }
});

test("Regression 12: host live eval parses Browser Runtime stdout and forwards one fixture origin", async () => {
  const origin = "http://127.0.0.1:43127";
  let request;
  const invoker = createLiveBrowserInvoker({
    hostOnly: true,
    bridgePort: 49152,
    bridgeToken: "test-token",
    evalFixtureOrigin: origin,
    fetchImpl: async (_url, init) => {
      request = JSON.parse(init.body);
      return {
        ok: true,
        status: 200,
        async json() {
          return {
            success: true,
            stdout: JSON.stringify({ ok: true, url: `${origin}/fixtures/computer-use/form.html` }),
          };
        },
      };
    },
  });

  const result = await invoker("eval-nav", "browser_navigate", {
    url: `${origin}/fixtures/computer-use/form.html`,
  });
  assert.deepEqual(result, { ok: true, url: `${origin}/fixtures/computer-use/form.html` });
  assert.equal(request.evalFixtureOrigin, origin);
  assert.deepEqual(request.params, { url: `${origin}/fixtures/computer-use/form.html` });
  assert.deepEqual(
    parseBrowserBridgeResponse({ success: true, stdout: JSON.stringify({ ok: true, value: 1 }) }),
    { ok: true, value: 1 },
  );
});
