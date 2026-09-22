/**
 * AgentCabin Computer Use V3 Eval Harness Types & Taxonomies.
 *
 * Defines the 13-category failure taxonomy, ComputerUseEvalCase schema,
 * and metric evaluation contracts.
 */

export const FAILURE_CATEGORIES = Object.freeze({
  ROOT_NOT_FOUND: "root_not_found",
  STALE_STATE: "stale_state",
  WRONG_GROUNDING: "wrong_grounding",
  ELEMENT_NOT_FOUND: "element_not_found",
  ACTION_REJECTED: "action_rejected",
  TIMEOUT_WAITING: "timeout_waiting",
  POSTCONDITION_FAILED: "postcondition_failed",
  UNEXPECTED_MODAL: "unexpected_modal",
  COORDINATE_OUT_OF_BOUNDS: "coordinate_out_of_bounds",
  DAEMON_CRASH: "daemon_crash",
  PERMISSION_DENIED: "permission_denied",
  UNSUPPORTED_ACTION: "unsupported_action",
  FLAKY_SUCCESS: "flaky_success",
});

export const ALL_FAILURE_CATEGORIES = Object.freeze(Object.values(FAILURE_CATEGORIES));

/**
 * Classifies an execution failure into one of the 13 canonical failure categories.
 */
export function classifyFailure(error, executionDetails = {}) {
  const msg = (error instanceof Error ? error.message : String(error || "")).toLowerCase();
  const outcome = String(executionDetails.outcome || "").toLowerCase();
  const verification = executionDetails.verification || {};

  if (
    msg.includes("permission") ||
    msg.includes("accessibility") ||
    msg.includes("screen recording") ||
    msg.includes("eperm") ||
    msg.includes("tcc") ||
    msg.includes("error -54") ||
    msg.includes("lsopenurlswithcompletionhandler") ||
    msg.includes("mach port") ||
    msg.includes("operation not permitted") ||
    msg.includes("not permitted")
  ) {
    return FAILURE_CATEGORIES.PERMISSION_DENIED;
  }
  if (
    msg.includes("root") &&
    (msg.includes("not found") ||
      msg.includes("unknown root") ||
      msg.includes("no matching controllable roots") ||
      msg.includes("not owned by a running app") ||
      msg.includes("not available through accessibility"))
  ) {
    return FAILURE_CATEGORIES.ROOT_NOT_FOUND;
  }
  if (msg.includes("stale") || msg.includes("epoch") || msg.includes("observe again") || msg.includes("unavailable")) {
    return FAILURE_CATEGORIES.STALE_STATE;
  }
  if (msg.includes("cross-state") || msg.includes("coordinate reuse") || msg.includes("out of bounds") || msg.includes("outside")) {
    return FAILURE_CATEGORIES.COORDINATE_OUT_OF_BOUNDS;
  }
  if (msg.includes("crash") || msg.includes("daemon exited") || msg.includes("econrefused") || msg.includes("pipe closed")) {
    return FAILURE_CATEGORIES.DAEMON_CRASH;
  }
  if (msg.includes("not supported") || msg.includes("unsupported")) {
    return FAILURE_CATEGORIES.UNSUPPORTED_ACTION;
  }
  if (msg.includes("not owned by state") || msg.includes("element not found") || msg.includes("not found in state")) {
    return FAILURE_CATEGORIES.ELEMENT_NOT_FOUND;
  }
  if (msg.includes("modal") || msg.includes("dialog interrupted") || msg.includes("alert blocked")) {
    return FAILURE_CATEGORIES.UNEXPECTED_MODAL;
  }
  if (msg.includes("wrong grounding") || msg.includes("mismatched target") || msg.includes("clicked wrong element")) {
    return FAILURE_CATEGORIES.WRONG_GROUNDING;
  }
  if (msg.includes("postcondition failed") || msg.includes("expect failed")) {
    return FAILURE_CATEGORIES.POSTCONDITION_FAILED;
  }
  if (verification.timedOut || msg.includes("timed out") || msg.includes("timeout")) {
    return FAILURE_CATEGORIES.TIMEOUT_WAITING;
  }
  if (verification.status === "failed" || outcome === "didnt") {
    return FAILURE_CATEGORIES.POSTCONDITION_FAILED;
  }
  if (msg.includes("rejected") || msg.includes("action failed") || msg.includes("invalid action")) {
    return FAILURE_CATEGORIES.ACTION_REJECTED;
  }
  if (msg.includes("flaky") || msg.includes("unstable ref")) {
    return FAILURE_CATEGORIES.FLAKY_SUCCESS;
  }

  // Fallback attribution
  return FAILURE_CATEGORIES.ACTION_REJECTED;
}

/**
 * Validates the schema of a ComputerUseEvalCase.
 */
export function validateEvalCase(caseDef) {
  if (!caseDef || typeof caseDef !== "object") {
    throw new TypeError("EvalCase must be an object.");
  }
  if (!caseDef.id || typeof caseDef.id !== "string") {
    throw new TypeError("EvalCase.id must be a non-empty string.");
  }
  if (!caseDef.title || typeof caseDef.title !== "string") {
    throw new TypeError(`EvalCase '${caseDef.id}' must have a title.`);
  }
  if (!caseDef.category || typeof caseDef.category !== "string") {
    throw new TypeError(`EvalCase '${caseDef.id}' must have a category.`);
  }
  if (!caseDef.prompt || typeof caseDef.prompt !== "string") {
    throw new TypeError(`EvalCase '${caseDef.id}' must have a prompt.`);
  }
  if (!caseDef.targetApp || typeof caseDef.targetApp !== "string") {
    throw new TypeError(`EvalCase '${caseDef.id}' must have a targetApp.`);
  }
  if (!caseDef.groundTruthOutcome || typeof caseDef.groundTruthOutcome !== "object") {
    throw new TypeError(`EvalCase '${caseDef.id}' must specify groundTruthOutcome.`);
  }
  return true;
}
