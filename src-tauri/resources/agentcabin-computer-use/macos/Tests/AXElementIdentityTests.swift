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
		let before = evidence(role: "AXTextField", semantic: "textfield ", traits: ["setValue", "focus"], parent: "AXGroup form")
		let after = evidence(role: "AXTextField", semantic: "textfield hello", traits: ["setValue", "focus"], parent: "AXGroup form")
		XCTAssertFalse(before.matchesCachedElement(after))
		XCTAssertTrue(before.canAdvanceWithinBatch(to: after))
	}

	func testBatchIdentityCannotAdvanceAcrossWindowOrTopologyChange() {
		let before = evidence(role: "AXTextField", semantic: "textfield ", traits: ["setValue", "focus"], parent: "AXGroup form")
		let movedWindow = evidence(windowId: 8, role: "AXTextField", semantic: "textfield hello", traits: ["setValue", "focus"], parent: "AXGroup form")
		let changedParent = evidence(role: "AXTextField", semantic: "textfield hello", traits: ["setValue", "focus"], parent: "AXGroup other")
		XCTAssertFalse(before.canAdvanceWithinBatch(to: movedWindow))
		XCTAssertFalse(before.canAdvanceWithinBatch(to: changedParent))
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
