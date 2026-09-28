import Foundation

struct ComputerUseProcessIdentity: Equatable {
	let pid: Int32
	let startSeconds: UInt64
	let startMicroseconds: UInt64
}

struct CoordinateAXPressResult: Equatable {
	let performed: Bool
	let reason: String?
	let depth: Int
	let ownerPid: Int32?
	let actionName: String?
}

func coordinateAXFailureCode(reason: String?, delivery: String) -> String? {
	if reason == "process_identity_changed" { return "stale_process" }
	if reason == "pid_mismatch" || (reason == "pid_unavailable" && delivery == "hid") {
		return "occluded_target"
	}
	return nil
}

func attemptCoordinateAXAction<Element>(
	button: String,
	clickCount: Int,
	targetPid: Int32,
	processIdentityMatches: () -> Bool = { true },
	hitTest: () -> Element?,
	ownerPid: (Element) -> Int32?,
	role: (Element) -> String?,
	parent: (Element) -> Element?,
	supportsAction: (Element, String) -> Bool,
	performAction: (Element, String) -> Bool,
	maximumAncestorDepth: Int = 5
) -> CoordinateAXPressResult {
	let actionName: String
	switch button {
	case "left": actionName = "AXPress"
	case "right": actionName = "AXShowMenu"
	default:
		return CoordinateAXPressResult(performed: false, reason: "unsupported_button", depth: 0, ownerPid: nil, actionName: nil)
	}
	guard clickCount == 1 else {
		return CoordinateAXPressResult(performed: false, reason: "unsupported_button", depth: 0, ownerPid: nil, actionName: actionName)
	}
	guard processIdentityMatches() else {
		return CoordinateAXPressResult(performed: false, reason: "process_identity_changed", depth: 0, ownerPid: nil, actionName: actionName)
	}
	guard let hit = hitTest() else {
		return CoordinateAXPressResult(performed: false, reason: "no_ax_element", depth: 0, ownerPid: nil, actionName: actionName)
	}

	var candidate: Element? = hit
	var depth = 0
	var actionFailed = false
	while let element = candidate, depth <= maximumAncestorDepth {
		guard let actualPid = ownerPid(element) else {
			return CoordinateAXPressResult(performed: false, reason: "pid_unavailable", depth: depth, ownerPid: nil, actionName: actionName)
		}
		guard actualPid == targetPid else {
			return CoordinateAXPressResult(performed: false, reason: "pid_mismatch", depth: depth, ownerPid: actualPid, actionName: actionName)
		}

		let elementRole = role(element) ?? ""
		if elementRole == "AXApplication" || elementRole == "AXWindow" { break }
		if supportsAction(element, actionName) {
			guard processIdentityMatches() else {
				return CoordinateAXPressResult(performed: false, reason: "process_identity_changed", depth: depth, ownerPid: nil, actionName: actionName)
			}
			if performAction(element, actionName) {
				return CoordinateAXPressResult(performed: true, reason: nil, depth: depth, ownerPid: actualPid, actionName: actionName)
			}
			actionFailed = true
		}
		candidate = parent(element)
		depth += 1
	}

	return CoordinateAXPressResult(performed: false, reason: actionFailed ? "ax_action_failed" : "no_press_action", depth: depth, ownerPid: nil, actionName: actionName)
}
