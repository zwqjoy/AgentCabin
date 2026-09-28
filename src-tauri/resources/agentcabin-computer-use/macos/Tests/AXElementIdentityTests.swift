import XCTest
@testable import CoordinateAXPress

final class AXElementIdentityTests: XCTestCase {
	private let process = ComputerUseProcessIdentity(pid: 42, startSeconds: 100, startMicroseconds: 5)

	private func evidence(
		pid: Int32 = 42,
		processIdentity: ComputerUseProcessIdentity? = nil,
		windowId: UInt32 = 7,
		role: String = "AXButton",
		subrole: String = "",
		identifier: String = "",
		semantic: String = "button delete",
		stableSemantic: String? = nil,
		traits: Set<String> = ["AXPress"],
		parent: String = "AXRow file a",
		x: CGFloat = 10
	) -> AXElementIdentitySnapshot {
		AXElementIdentitySnapshot(
			pid: pid,
			processIdentity: processIdentity ?? process,
			windowId: windowId,
			role: role,
			subrole: subrole,
			identifier: identifier,
			semanticFingerprint: semantic,
			stableSemanticFingerprint: stableSemantic ?? semantic,
			actionableTraits: traits,
			parentSemanticFingerprint: parent,
			rect: CGRect(x: x, y: 20, width: 30, height: 20)
		)
	}

	func testReadableCachedPointerWithChangedSemanticIdentityIsRejected() {
		let observed = evidence()
		let stillReadableButChanged = evidence(semantic: "button archive")
		XCTAssertFalse(observed.matchesCachedElement(stillReadableButChanged))
	}

	func testSuccessfulBatchMutationCanAdvanceValueWithoutChangingControlIdentity() {
		let before = evidence(role: "AXTextField", semantic: "textfield ", stableSemantic: "textfield email", traits: ["setValue", "focus"], parent: "AXGroup form")
		let after = evidence(role: "AXTextField", semantic: "textfield hello", stableSemantic: "textfield email", traits: ["setValue", "focus"], parent: "AXGroup form")
		XCTAssertFalse(before.matchesCachedElement(after))
		XCTAssertTrue(before.canAdvanceWithinBatch(to: after, afterAction: "setText"))
	}

	func testTypeTextThenKeypressCanReuseSameBatchRefAfterValueChanges() {
		let before = evidence(role: "AXTextField", semantic: "textfield ", stableSemantic: "textfield email", traits: ["setValue", "focus"], parent: "AXGroup form")
		let afterTypeText = evidence(role: "AXTextField", semantic: "textfield hello", stableSemantic: "textfield email", traits: ["setValue", "focus"], parent: "AXGroup form")
		XCTAssertTrue(before.canAdvanceWithinBatch(to: afterTypeText, afterAction: "typeText"))
		XCTAssertTrue(afterTypeText.matchesCachedElement(afterTypeText), "The next same-ref batch step validates against the advanced evidence.")
	}

	func testClickCannotAdvanceAButtonFromDeleteToUndo() {
		let before = evidence(semantic: "button delete", stableSemantic: "button delete")
		let after = evidence(semantic: "button undo", stableSemantic: "button undo")
		XCTAssertFalse(before.matchesCachedElement(after))
		XCTAssertFalse(before.canAdvanceWithinBatch(to: after, afterAction: "click"))

		let fieldBefore = evidence(role: "AXTextField", semantic: "textfield old value", stableSemantic: "textfield email", traits: ["setValue", "focus"], parent: "AXGroup form")
		let fieldAfter = evidence(role: "AXTextField", semantic: "textfield new value", stableSemantic: "textfield email", traits: ["setValue", "focus"], parent: "AXGroup form")
		XCTAssertFalse(fieldBefore.canAdvanceWithinBatch(to: fieldAfter, afterAction: "click"))
	}

	func testSetTextCannotAdvanceWhenStableControlSemanticsChange() {
		let before = evidence(role: "AXTextField", semantic: "textfield name", stableSemantic: "textfield name", traits: ["setValue", "focus"], parent: "AXGroup form")
		let after = evidence(role: "AXTextField", semantic: "textfield changed", stableSemantic: "textfield changed", traits: ["setValue", "focus"], parent: "AXGroup form")
		XCTAssertFalse(before.canAdvanceWithinBatch(to: after, afterAction: "setText"))
	}

	func testBatchIdentityCannotAdvanceAcrossWindowOrTopologyChange() {
		let before = evidence(role: "AXTextField", semantic: "textfield ", stableSemantic: "textfield email", traits: ["setValue", "focus"], parent: "AXGroup form")
		let movedWindow = evidence(windowId: 8, role: "AXTextField", semantic: "textfield hello", stableSemantic: "textfield email", traits: ["setValue", "focus"], parent: "AXGroup form")
		let changedParent = evidence(role: "AXTextField", semantic: "textfield hello", stableSemantic: "textfield email", traits: ["setValue", "focus"], parent: "AXGroup other")
		XCTAssertFalse(before.canAdvanceWithinBatch(to: movedWindow, afterAction: "setText"))
		XCTAssertFalse(before.canAdvanceWithinBatch(to: changedParent, afterAction: "setText"))
	}

	func testUniqueStrongIdentifierAllowsSafeRefind() {
		let observed = evidence(identifier: "delete-file-a")
		let candidates = [
			evidence(identifier: "delete-file-b", parent: "AXRow file b", x: 40),
			evidence(identifier: "delete-file-a", parent: "AXRow file a", x: 80),
		]
		XCTAssertEqual(uniqueAXRefindCandidateIndex(snapshot: observed, candidates: candidates), 1)
	}

	func testAmbiguousDuplicateControlsFailClosedRegardlessOfDistance() {
		let observed = evidence(parent: "")
		let candidates = [
			evidence(parent: "", x: 11),
			evidence(parent: "", x: 500),
		]
		XCTAssertNil(uniqueAXRefindCandidateIndex(snapshot: observed, candidates: candidates))
	}

	func testChangedWindowCannotRefindAcrossWindows() {
		let observed = evidence(windowId: 7, identifier: "save")
		let candidate = evidence(windowId: 8, identifier: "save")
		XCTAssertEqual(validateAXElementIdentity(observed, targetPid: 42, processIdentity: process, windowId: 8), .staleRef)
		XCTAssertNil(uniqueAXRefindCandidateIndex(snapshot: observed, candidates: [candidate]))
	}

	func testPidReuseWithDifferentStartTimeIsStaleProcess() {
		let observed = evidence(identifier: "save")
		let restarted = ComputerUseProcessIdentity(pid: 42, startSeconds: 101, startMicroseconds: 0)
		XCTAssertEqual(validateAXElementIdentity(observed, targetPid: 42, processIdentity: restarted, windowId: 7), .staleProcess)
	}

	func testCachedElementOwnedByAnotherPidDoesNotMatch() {
		let observed = evidence()
		let foreign = evidence(pid: 99)
		XCTAssertFalse(observed.matchesCachedElement(foreign))
		XCTAssertFalse(observed.matchesRefindCandidate(foreign))
	}
}
