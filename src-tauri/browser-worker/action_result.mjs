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
  const before = { revision: beforeSnapshot.revision, documentId: beforeSnapshot.documentId, url: beforeSnapshot.url };
  try {
    const performed = await perform(beforeSnapshot);
    const observation = await generatePageSnapshot(page, { sinceRevision: beforeSnapshot.revision });
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
        ...(performed?.resolvedBy ? { resolvedBy: performed.resolvedBy } : performed?.source ? { resolvedBy: performed.source } : {}),
        ...(performed?.selected !== undefined ? { selected: performed.selected, selectedValue: performed.selectedValue, selectedLabel: performed.selectedLabel } : {}),
      },
      after: { revision: observation.revision, documentId: observation.documentId, url: observation.url },
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
      error: { code: actionErrorCode(error), message: error instanceof Error ? error.message : String(error) },
      recovery: { recommended: "snapshot" },
    };
  }
}
