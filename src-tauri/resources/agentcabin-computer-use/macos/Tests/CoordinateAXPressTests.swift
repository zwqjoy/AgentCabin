import XCTest
@testable import CoordinateAXPress

final class CoordinateAXPressTests: XCTestCase {
	private final class Element {
		let pid: Int32?
		let role: String
		let pressable: Bool
		var parent: Element?

		init(pid: Int32?, role: String = "AXButton", pressable: Bool = false, parent: Element? = nil) {
			self.pid = pid
			self.role = role
			self.pressable = pressable
			self.parent = parent
		}
	}

	private func attempt(_ hit: Element?, button: String = "left", clickCount: Int = 1, targetPid: Int32 = 42, calls: inout [Element]) -> CoordinateAXPressResult {
		attemptCoordinateAXAction(
			button: button,
			clickCount: clickCount,
			targetPid: targetPid,
			processIdentityMatches: { true },
			hitTest: { hit },
			ownerPid: { $0.pid },
			role: { $0.role },
			parent: { $0.parent },
			supportsAction: { $0.pressable && $1 == "AXPress" },
			performAction: { element, _ in calls.append(element); return true }
		)
	}

	func testCoordinateHitElementPressesWithoutPhysicalFallback() {
		let button = Element(pid: 42, pressable: true)
		var calls: [Element] = []
		let result = attempt(button, calls: &calls)
		XCTAssertTrue(result.performed)
		XCTAssertEqual(result.depth, 0)
		XCTAssertEqual(calls.count, 1)
	}

	func testPressesSafeAncestorWhenHitElementIsNotActionable() {
		let parent = Element(pid: 42, role: "AXGroup", pressable: true)
		let child = Element(pid: 42, role: "AXStaticText", parent: parent)
		var calls: [Element] = []
		let result = attempt(child, calls: &calls)
		XCTAssertTrue(result.performed)
		XCTAssertEqual(result.depth, 1)
		XCTAssertTrue(calls.first === parent)
	}

	func testRejectsPidMismatchWithoutPressingOtherApp() {
		let hit = Element(pid: 99, pressable: true)
		var calls: [Element] = []
		let result = attempt(hit, calls: &calls)
		XCTAssertEqual(result.reason, "pid_mismatch")
		XCTAssertTrue(calls.isEmpty)
	}

	func testRejectsAncestorOwnedByAnotherProcess() {
		let parent = Element(pid: 99, role: "AXGroup", pressable: true)
		let child = Element(pid: 42, role: "AXStaticText", parent: parent)
		var calls: [Element] = []
		let result = attempt(child, calls: &calls)
		XCTAssertEqual(result.reason, "pid_mismatch")
		XCTAssertTrue(calls.isEmpty)
	}

	func testFallsBackWhenHitElementHasNoPressAction() {
		let hit = Element(pid: 42)
		var calls: [Element] = []
		let result = attempt(hit, calls: &calls)
		XCTAssertEqual(result.reason, "no_press_action")
		XCTAssertTrue(calls.isEmpty)
	}

	func testFallsBackWhenAXPressFails() {
		let hit = Element(pid: 42, pressable: true)
		let result = attemptCoordinateAXAction(
			button: "left",
			clickCount: 1,
			targetPid: 42,
			processIdentityMatches: { true },
			hitTest: { hit },
			ownerPid: { $0.pid },
			role: { $0.role },
			parent: { $0.parent },
			supportsAction: { $0.pressable && $1 == "AXPress" },
			performAction: { _, _ in false }
		)
		XCTAssertEqual(result.reason, "ax_action_failed")
	}

	func testFailsClosedWhenOwnerPidCannotBeRead() {
		let hit = Element(pid: nil, pressable: true)
		var calls: [Element] = []
		let result = attempt(hit, calls: &calls)
		XCTAssertEqual(result.reason, "pid_unavailable")
		XCTAssertTrue(calls.isEmpty)
	}

	func testFallsBackWhenHitTestFindsNoElement() {
		var calls: [Element] = []
		let result = attempt(nil, calls: &calls)
		XCTAssertEqual(result.reason, "no_ax_element")
		XCTAssertTrue(calls.isEmpty)
	}

	func testUnsupportedButtonAndMultipleClicksKeepPhysicalFallback() {
		let hit = Element(pid: 42, pressable: true)
		var calls: [Element] = []
		XCTAssertEqual(attempt(hit, button: "middle", calls: &calls).reason, "unsupported_button")
		XCTAssertEqual(attempt(hit, clickCount: 2, calls: &calls).reason, "unsupported_button")
		XCTAssertTrue(calls.isEmpty)
	}

	func testRightClickUsesAXShowMenu() {
		let hit = Element(pid: 42, pressable: false)
		var actionCalls: [String] = []
		let result = attemptCoordinateAXAction(
			button: "right",
			clickCount: 1,
			targetPid: 42,
			processIdentityMatches: { true },
			hitTest: { hit },
			ownerPid: { $0.pid },
			role: { $0.role },
			parent: { $0.parent },
			supportsAction: { _, action in action == "AXShowMenu" },
			performAction: { _, action in actionCalls.append(action); return true }
		)
		XCTAssertTrue(result.performed)
		XCTAssertEqual(result.actionName, "AXShowMenu")
		XCTAssertEqual(actionCalls, ["AXShowMenu"])
	}

	func testProcessIdentityChangeFailsClosedBeforeAXAction() {
		let hit = Element(pid: 42, pressable: true)
		var calls: [Element] = []
		let result = attemptCoordinateAXAction(
			button: "left",
			clickCount: 1,
			targetPid: 42,
			processIdentityMatches: { false },
			hitTest: { hit },
			ownerPid: { $0.pid },
			role: { $0.role },
			parent: { $0.parent },
			supportsAction: { $0.pressable && $1 == "AXPress" },
			performAction: { element, _ in calls.append(element); return true }
		)
		XCTAssertEqual(result.reason, "process_identity_changed")
		XCTAssertTrue(calls.isEmpty)
	}

	func testProcessIdentityIncludesProcessStartTime() {
		let first = ComputerUseProcessIdentity(pid: 42, startSeconds: 10, startMicroseconds: 20)
		let sameProcess = ComputerUseProcessIdentity(pid: 42, startSeconds: 10, startMicroseconds: 20)
		let reusedPid = ComputerUseProcessIdentity(pid: 42, startSeconds: 11, startMicroseconds: 0)
		XCTAssertEqual(first, sameProcess)
		XCTAssertNotEqual(first, reusedPid)
	}

	func testStopsBeforeWindowAndApplicationActions() {
		let root = Element(pid: 42, role: "AXWindow", pressable: true)
		let hit = Element(pid: 42, parent: root)
		var calls: [Element] = []
		let result = attempt(hit, calls: &calls)
		XCTAssertEqual(result.reason, "no_press_action")
		XCTAssertTrue(calls.isEmpty)
	}
}
