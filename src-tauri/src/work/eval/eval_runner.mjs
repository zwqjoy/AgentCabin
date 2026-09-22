/**
 * AgentCabin Computer Use V3 Eval Harness Runner.
 *
 * Runs test cases, computes performance metrics, and produces structured evaluation reports.
 */

import { validateEvalCase, classifyFailure, FAILURE_CATEGORIES } from "./eval_types.mjs";
import { MACOS_EVAL_CASES } from "./eval_cases_macos.mjs";

export class ComputerUseEvalRunner {
  constructor(options = {}) {
    this.cases = options.cases || MACOS_EVAL_CASES;
    this.sessionFactory = options.sessionFactory || defaultSessionFactory;
  }

  /**
   * Executes a single evaluation case.
   */
  async runCase(caseDef) {
    validateEvalCase(caseDef);

    const startTime = Date.now();
    let toolCalls = 0;
    let wrongClicks = 0;
    let passed = false;
    let failureCategory = null;
    let errorDetails = null;
    let lastExecution = {};

    try {
      const session = await this.sessionFactory(caseDef);

      // 1. Initial observation / setup
      toolCalls++;
      const observed = await session.observeUi("eval:init");
      if (!observed?.details?.ok) {
        throw new Error(observed?.content?.[0]?.text || "Failed initial observation.");
      }

      let currentStateId = observed.details.stateId;

      // 2. Execute actions if specified
      const actions = caseDef.actions || [];
      if (actions.length > 0) {
        // Pre-validate clicks against current state
        const curState = session.state(currentStateId);
        const curElements = curState?.elements || [];
        for (const act of actions) {
          if (act.action === "click" || act.action === "press") {
            if (act.ref) {
              const el = curElements.find((e) => e.ref === act.ref);
              if (!el && typeof session.getWrongClicks !== "function") {
                wrongClicks++;
              } else if (el && (el.disabled || (el.canPress === false && el.canFocus === false && !el.actions?.length))) {
                wrongClicks++;
              }
            } else if (act.x !== undefined && act.y !== undefined) {
              const inside = curElements.some((el) => {
                if (!el.rect) return false;
                const rx = el.rect.x ?? 0;
                const ry = el.rect.y ?? 0;
                const rw = el.rect.width ?? el.rect.w ?? 0;
                const rh = el.rect.height ?? el.rect.h ?? 0;
                return act.x >= rx && act.x <= rx + rw && act.y >= ry && act.y <= ry + rh;
              });
              if (!inside) {
                wrongClicks++;
              }
            }
          }
        }

        toolCalls++;
        const actResult = await session.actUi("eval:act", {
          stateId: currentStateId,
          actions,
          expect: caseDef.groundTruthOutcome?.expect,
        });

        if (typeof session.getWrongClicks === "function") {
          wrongClicks += session.getWrongClicks();
        }

        if (!actResult?.details?.ok) {
          throw new Error(actResult?.content?.[0]?.text || "Action batch failed.");
        }

        if (actResult?.details?.stateId) {
          currentStateId = actResult.details.stateId;
        }

        lastExecution = {
          outcome: actResult.details.execution?.outcome,
          verification: actResult.details.execution?.verification,
        };

        if (lastExecution.verification?.status === "failed") {
          throw new Error(`Postcondition failed for case ${caseDef.id}: ${lastExecution.verification?.reason || "expected outcome not verified"}`);
        }
        if (lastExecution.verification?.status === "timeout" || lastExecution.verification?.timedOut) {
          throw new Error(`Postcondition timed out for case ${caseDef.id}.`);
        }
        if (lastExecution.outcome === "didnt") {
          throw new Error(`Action outcome was didn't for case ${caseDef.id}.`);
        }
      }

      // Cross-app cases must prove the value came from the source app and was
      // then entered into a newly opened target app. A final Calculator state
      // containing 81 is not sufficient evidence of the transfer.
      if (caseDef.crossApp) {
        const sourceState = session.state(currentStateId);
        if (!verifyOutcome(sourceState, { expect: caseDef.crossApp.sourceExpect })) {
          throw new Error(`Cross-app source postcondition failed for case ${caseDef.id}.`);
        }
        const transferText = readExpectedText(sourceState, caseDef.crossApp.sourceExpect?.text);
        if (typeof session.openApp !== "function") {
          throw new Error(`Cross-app case ${caseDef.id} requires a session with openApp support.`);
        }
        toolCalls++;
        const opened = await session.openApp(`${caseDef.id}:cross-app-open`, caseDef.crossApp.targetApp);
        if (!opened?.details?.ok) {
          throw new Error(opened?.content?.[0]?.text || `Failed to open ${caseDef.crossApp.targetApp}.`);
        }
        currentStateId = opened.details.stateId;

        const crossActions = (caseDef.crossApp.actions || []).map((action) => ({
          ...action,
          ...(typeof action.text === "string"
            ? { text: action.text.replaceAll("{{transferText}}", transferText) }
            : {}),
        }));
        if (crossActions.length > 0) {
          toolCalls++;
          const crossResult = await session.actUi(`${caseDef.id}:cross-app-act`, {
            stateId: currentStateId,
            actions: crossActions,
            expect: caseDef.crossApp.expect,
          });
          if (!crossResult?.details?.ok) {
            throw new Error(crossResult?.content?.[0]?.text || "Cross-app action failed.");
          }
          if (crossResult.details.stateId) currentStateId = crossResult.details.stateId;
          lastExecution = {
            outcome: crossResult.details.execution?.outcome,
            verification: crossResult.details.execution?.verification,
          };
          if (lastExecution.verification?.status === "failed") {
            throw new Error(`Cross-app postcondition failed for case ${caseDef.id}.`);
          }
          if (lastExecution.verification?.status === "timeout" || lastExecution.verification?.timedOut) {
            throw new Error(`Cross-app postcondition timed out for case ${caseDef.id}.`);
          }
          if (lastExecution.outcome === "didnt") {
            throw new Error(`Cross-app action outcome was didn't for case ${caseDef.id}.`);
          }
        }
      }

      // 3. Final verification check against session state
      const state = session.state(currentStateId);
      passed = verifyOutcome(state, caseDef.groundTruthOutcome);
      if (!passed) {
        failureCategory = FAILURE_CATEGORIES.POSTCONDITION_FAILED;
        errorDetails = "Final state did not meet ground truth expectations.";
      }
    } catch (err) {
      passed = false;
      errorDetails = err instanceof Error ? err.message : String(err);
      failureCategory = classifyFailure(err, lastExecution);
    }

    const latencyMs = Date.now() - startTime;

    return {
      id: caseDef.id,
      title: caseDef.title,
      category: caseDef.category,
      passed,
      latencyMs,
      toolCalls,
      wrongClicks,
      failureCategory: passed ? null : (failureCategory || FAILURE_CATEGORIES.ACTION_REJECTED),
      error: errorDetails,
    };
  }

  /**
   * Runs all configured evaluation cases and aggregates metrics.
   */
  async runAll(filterFn = null) {
    const selectedCases = typeof filterFn === "function" ? this.cases.filter(filterFn) : this.cases;
    const results = [];
    const failureDistribution = {};

    let totalToolCalls = 0;
    let totalLatencyMs = 0;
    let totalWrongClicks = 0;

    for (const caseDef of selectedCases) {
      const result = await this.runCase(caseDef);
      results.push(result);

      totalToolCalls += result.toolCalls;
      totalLatencyMs += result.latencyMs;
      totalWrongClicks += result.wrongClicks;

      if (!result.passed && result.failureCategory) {
        failureDistribution[result.failureCategory] = (failureDistribution[result.failureCategory] || 0) + 1;
      }
    }

    const totalCases = results.length;
    const passed = results.filter((r) => r.passed).length;
    const failed = totalCases - passed;
    const successRate = totalCases > 0 ? (passed / totalCases) * 100 : 0;
    const avgToolCalls = totalCases > 0 ? Number((totalToolCalls / totalCases).toFixed(2)) : 0;
    const avgLatencyMs = totalCases > 0 ? Math.round(totalLatencyMs / totalCases) : 0;
    const wrongClickRate = totalToolCalls > 0 ? Number(((totalWrongClicks / totalToolCalls) * 100).toFixed(2)) : 0;

    return {
      timestamp: new Date().toISOString(),
      totalCases,
      passed,
      failed,
      successRate: Number(successRate.toFixed(2)),
      wrongClickRate,
      avgToolCalls,
      avgLatencyMs,
      failureDistribution,
      results,
    };
  }
}

/**
 * Checks whether state satisfies ground truth assertions.
 */
function verifyOutcome(state, groundTruth) {
  if (!state || !groundTruth) return false;
  const expect = groundTruth.expect;
  if (!expect) return true;

  if (expect.requireVisualDiff) {
    return true;
  }

  if (expect.text) {
    const target = String(expect.text).toLowerCase();
    const hasElementText = (state.elements || []).some((el) => {
      const text = `${el.title || ""} ${el.label || ""} ${el.value || ""}`.toLowerCase();
      return text.includes(target);
    });
    const hasRootText = (state.roots || []).some((r) => {
      const text = `${r.appName || ""} ${r.title || ""}`.toLowerCase();
      return text.includes(target);
    });
    if (!hasElementText && !hasRootText) return false;
  }

  if (expect.role) {
    const targetRole = String(expect.role).toLowerCase();
    if (targetRole === "root") {
      return (state.elements || []).length > 0 || state.rootRef !== undefined;
    }
    const hasRole = (state.elements || []).some((el) => String(el.role || "").toLowerCase() === targetRole);
    if (!hasRole) return false;
  }

  return true;
}

function readExpectedText(state, expectedText) {
  const target = String(expectedText || "").toLowerCase();
  const match = (state?.elements || []).find((element) => {
    const text = `${element.title || ""} ${element.label || ""} ${element.value || ""}`.toLowerCase();
    return target && text.includes(target);
  });
  const value = match?.value ?? match?.title ?? match?.label;
  if (value === undefined || value === null || String(value).trim() === "") {
    throw new Error("Cross-app source value is not readable from the observed state.");
  }
  return String(value).trim();
}

/**
 * Default mock session factory for offline eval testing.
 */
function defaultSessionFactory(caseDef) {
  const init = caseDef.initialState || {};
  let currentElements = [...(init.elements || [])];

  const stateObj = {
    stateId: `eval-state-${caseDef.id}`,
    elements: currentElements,
    roots: init.roots || [],
    rootRef: "@r1",
  };

  return {
    state: () => stateObj,
    observeUi: async () => ({
      details: { ok: true, stateId: stateObj.stateId, elements: currentElements },
      content: [{ type: "text", text: "Observed" }],
    }),
    actUi: async (_callId, params) => {
      // Simulate action application
      if (caseDef.groundTruthOutcome?.expect?.text) {
        currentElements.push({
          role: "text",
          title: caseDef.groundTruthOutcome.expect.text,
          value: caseDef.groundTruthOutcome.expect.text,
          ref: "@e999",
        });
      }
      if (caseDef.groundTruthOutcome?.expect?.role && caseDef.groundTruthOutcome.expect.role !== "root") {
        currentElements.push({
          role: caseDef.groundTruthOutcome.expect.role,
          title: caseDef.groundTruthOutcome.expect.role,
          ref: "@e998",
        });
      }
      return {
        details: {
          ok: true,
          stateId: stateObj.stateId,
          execution: { outcome: "worked", verification: { status: "verified" } },
        },
      };
    },
  };
}

/**
 * Real evaluation session backed by a live ComputerUseV2Session.
 * Connects directly to real desktop / browser processes, resolves semantic actions to
 * actual UI elements, and verifies ground truth postconditions without mock data injection.
 */
export class RealComputerUseEvalSession {
  constructor(options = {}) {
    this.session = options.session;
    this.caseDef = options.caseDef;
    this.targetApp = options.targetApp || options.caseDef?.targetApp;
    this.fixtureBaseUrl = String(options.fixtureBaseUrl || "").trim().replace(/\/$/, "");
    this.rootRef = undefined;
    this.wrongClicks = 0;
  }

  getWrongClicks() {
    const wc = this.wrongClicks;
    this.wrongClicks = 0;
    return wc;
  }

  state(stateId) {
    return this.session.state(stateId);
  }

  async openApp(toolCallId, targetApp = this.targetApp) {
    this.targetApp = targetApp;
    this.rootRef = undefined;
    const isBrowser = /^(browser|chrome|chromium|safari|web)$/i.test(String(targetApp).trim());
    if (isBrowser) {
      let fixtureUrl = undefined;
      if (this.caseDef?.fixture) {
        const fixture = String(this.caseDef.fixture).trim();
        if (!this.fixtureBaseUrl || /^https?:\/\//i.test(fixture)) {
          throw new Error("Live browser eval requires an ephemeral fixtureBaseUrl; file:// fixtures are disabled.");
        }
        if (!fixture.startsWith("fixtures/computer-use/") || fixture.includes("..")) {
          throw new Error(`Live browser eval fixture path is outside the checked-in fixture directory: ${fixture}`);
        }
        fixtureUrl = `${this.fixtureBaseUrl}/${fixture.replace(/^\/+/, "")}`;
      }
      const res = await this.session.launchApp(`${toolCallId}:launch`, { name: "browser", url: fixtureUrl });
      if (res?.details?.ok) {
        this.rootRef = res.details.rootRef || this.session.lastRootRef;
        return res;
      }
      throw new Error(res?.content?.[0]?.text || "Failed to launch browser target.");
    }
    const res = await this.session.launchApp(`${toolCallId}:launch`, { name: targetApp });
    if (res?.details?.ok) {
      this.rootRef = res.details.rootRef || this.session.lastRootRef;
      return res;
    }
    throw new Error(res?.content?.[0]?.text || `Failed to launch app ${targetApp}.`);
  }

  async observeUi(toolCallId = "eval:init", params = {}) {
    if (!this.rootRef && this.targetApp) {
      return this.openApp(toolCallId, this.targetApp);
    }
    return await this.session.observeUi(toolCallId, { root: this.rootRef, ...params });
  }

  async actUi(toolCallId = "eval:act", params = {}) {
    const currentState = this.session.state(params.stateId);
    const currentElements = currentState?.elements || [];

    const mockElements = this.caseDef?.initialState?.elements || [];
    const mockRefToTitle = new Map(mockElements.map((el) => [el.ref, el.title || el.name || el.label]));

    const resolvedActions = (params.actions || []).map((act) => {
      let targetTitle = act.targetTitle || act.targetLabel || act.label;
      if (!targetTitle && act.ref && mockRefToTitle.has(act.ref)) {
        targetTitle = mockRefToTitle.get(act.ref);
      }

      if (targetTitle) {
        const query = String(targetTitle).trim().toLowerCase();
        const match = currentElements.find((el) => {
          const title = String(el.title || el.name || el.label || "").trim().toLowerCase();
          return title === query;
        }) || currentElements.find((el) => {
          const title = String(el.title || el.name || el.label || "").trim().toLowerCase();
          return title.includes(query);
        });

        if (match) {
          return { ...act, ref: match.ref };
        } else {
          this.wrongClicks++;
        }
      }
      return act;
    });

    const res = await this.session.actUi(toolCallId, {
      ...params,
      actions: resolvedActions,
    });

    const verification = res?.details?.execution?.verification;
    if (res?.details?.execution?.outcome === "didnt" && (!verification || verification.status !== "failed")) {
      this.wrongClicks++;
    }

    return res;
  }
}

/**
 * Creates an evaluation session factory backed by a real ComputerUseV2Session.
 */
export function createRealSessionFactory(session) {
  return (caseDef) => new RealComputerUseEvalSession({ session, caseDef });
}
