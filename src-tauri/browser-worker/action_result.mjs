import { generatePageSnapshot } from "./snapshot.mjs";

function actionErrorCode(error) {
  if (error?.code) return error.code;
  const message = error instanceof Error ? error.message : String(error);
  if (/timeout/i.test(message)) return "timeout";
  if (/stale ref/i.test(message)) return "stale_ref";
  if (/selector.*invalid|invalid.*selector/i.test(message)) return "invalid_selector";
  if (/not visible/i.test(message)) return "target_not_visible";
  if (/disabled/i.test(message)) return "target_disabled";
  if (/not found|no .*matches/i.test(message)) return "target_not_found";
  if (/unsupported_action/i.test(message)) return "unsupported_action";
  return "action_failed";
}

export async function structuredAction(page, action, params = {}, perform) {
  const beforeSnapshot = await generatePageSnapshot(page);
  const ref = String(params.ref || "").replace(/^@/, "");
  const targetInfo = beforeSnapshot.refs?.[ref] || {};
  const target = ref
    ? { ref, ...(targetInfo.role ? { role: targetInfo.role } : {}), ...((params.target_label || targetInfo.name) ? { name: params.target_label || targetInfo.name } : {}) }
    : params.locator || (params.selector ? { selector: params.selector } : params.x !== undefined && params.y !== undefined ? { x: params.x, y: params.y } : undefined);
  const before = { revision: beforeSnapshot.revision, documentId: beforeSnapshot.documentId, workerInstanceId: beforeSnapshot.workerInstanceId, targetId: beforeSnapshot.targetId, url: beforeSnapshot.url };
  let documentChanged = false;
  try {
    const requested = params.targetIdentity;
    if (requested && (typeof requested !== "object" || !requested.workerInstanceId || !requested.documentId)) {
      throw Object.assign(new Error("The supplied browser target identity is incomplete; capture a fresh snapshot."), { code: "stale_browser_instance" });
    }
    if (requested?.workerInstanceId && requested.workerInstanceId !== beforeSnapshot.workerInstanceId) {
      throw Object.assign(new Error("The Browser Worker instance changed; capture a fresh snapshot."), { code: "stale_browser_instance" });
    }
    if (requested?.targetId && requested.targetId !== beforeSnapshot.targetId) {
      throw Object.assign(new Error("The browser tab changed; capture a fresh snapshot."), { code: "stale_browser_instance" });
    }
    documentChanged = Boolean(requested?.documentId && requested.documentId !== beforeSnapshot.documentId);
    const targetsElement = params.ref || params.locator || params.selector || params.x !== undefined || params.y !== undefined;
    if (documentChanged && targetsElement && !params.locator) {
      throw Object.assign(new Error("The referenced document changed and no semantic recovery locator was supplied."), { code: "stale_ref" });
    }
    const actionParams = documentChanged ? { ...params, ref: undefined, selector: undefined } : params;
    const performed = await perform(beforeSnapshot, actionParams);
    const observation = await generatePageSnapshot(page, { sinceRevision: beforeSnapshot.revision });
    if (performed?.timedOut) {
      return {
        ok: false,
        action,
        ...(target ? { target } : {}),
        before,
        execution: { performed: false },
        after: { revision: observation.revision, documentId: observation.documentId, workerInstanceId: observation.workerInstanceId, targetId: observation.targetId, url: observation.url },
        observation,
        timedOut: true,
        checks: performed.checks || [],
        failures: performed.failures || [],
        error: { code: "timeout", message: "The expected browser page condition was not met before the timeout." },
        recovery: { recommended: "snapshot" },
      };
    }
    return {
      ok: true,
      action,
      ...(target ? {
        target: performed?.target
          ? { ref: performed.target.ref, ...(performed.target.role ? { role: performed.target.role } : {}), ...(performed.target.name ? { name: performed.target.name } : {}) }
          : target,
      } : {}),
      before,
      execution: {
        performed: true,
        ...(performed?.resolvedBy || performed?.source ? { resolvedBy: documentChanged && (performed?.resolvedBy || performed?.source) === "locator" ? "semantic_recovery" : (performed?.resolvedBy || performed?.source) } : {}),
        ...(performed?.selected !== undefined ? { selected: performed.selected, selectedValue: performed.selectedValue, selectedLabel: performed.selectedLabel } : {}),
      },
      after: { revision: observation.revision, documentId: observation.documentId, workerInstanceId: observation.workerInstanceId, targetId: observation.targetId, url: observation.url },
      change: { type: observation.snapshotType },
      observation,
      ...(performed?.checks ? { checks: performed.checks } : {}),
      ...(typeof performed?.verified === "boolean" ? { verified: performed.verified } : {}),
      ...(typeof performed?.timedOut === "boolean" ? { timedOut: performed.timedOut } : {}),
      ...(performed?.failures ? { failures: performed.failures } : {}),
    };
  } catch (error) {
    return {
      ok: false,
      action,
      ...(target ? { target } : {}),
      before,
      execution: { performed: false },
      error: { code: documentChanged && actionErrorCode(error) === "target_not_found" ? "stale_ref" : actionErrorCode(error), message: error instanceof Error ? error.message : String(error) },
      recovery: { recommended: "snapshot" },
    };
  }
}
