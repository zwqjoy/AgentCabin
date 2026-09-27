import Foundation

struct CoordinateAXPressResult: Equatable {
	let performed: Bool
	let reason: String?
	let depth: Int
	let ownerPid: Int32?
}

func attemptCoordinateAXPress<Element>(
	button: String,
	clickCount: Int,
	targetPid: Int32,
	hitTest: () -> Element?,
	ownerPid: (Element) -> Int32?,
	role: (Element) -> String?,
	parent: (Element) -> Element?,
	supportsPress: (Element) -> Bool,
	performPress: (Element) -> Bool,
	maximumAncestorDepth: Int = 5
) -> CoordinateAXPressResult {
	guard button == "left", clickCount == 1 else {
		return CoordinateAXPressResult(performed: false, reason: "unsupported_button", depth: 0, ownerPid: nil)
	}
	guard let hit = hitTest() else {
		return CoordinateAXPressResult(performed: false, reason: "no_ax_element", depth: 0, ownerPid: nil)
	}

	var candidate: Element? = hit
	var depth = 0
	var actionFailed = false
	while let element = candidate, depth <= maximumAncestorDepth {
		guard let actualPid = ownerPid(element) else {
			return CoordinateAXPressResult(performed: false, reason: "pid_unavailable", depth: depth, ownerPid: nil)
		}
		guard actualPid == targetPid else {
			return CoordinateAXPressResult(performed: false, reason: "pid_mismatch", depth: depth, ownerPid: actualPid)
		}

		let elementRole = role(element) ?? ""
		if elementRole == "AXApplication" || elementRole == "AXWindow" { break }
		if supportsPress(element) {
			if performPress(element) {
				return CoordinateAXPressResult(performed: true, reason: nil, depth: depth, ownerPid: actualPid)
			}
			actionFailed = true
		}
		candidate = parent(element)
		depth += 1
	}

	return CoordinateAXPressResult(performed: false, reason: actionFailed ? "ax_action_failed" : "no_press_action", depth: depth, ownerPid: nil)
}
