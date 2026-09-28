import Foundation

struct AXElementIdentitySnapshot {
	let pid: Int32
	let processIdentity: ComputerUseProcessIdentity
	let windowId: UInt32
	let role: String
	let subrole: String
	let identifier: String
	let semanticFingerprint: String
	let stableSemanticFingerprint: String
	let actionableTraits: Set<String>
	let parentSemanticFingerprint: String
	let rect: CGRect

	func matchesCachedElement(_ current: AXElementIdentitySnapshot) -> Bool {
		contextMatches(current)
			&& role == current.role
			&& subrole == current.subrole
			&& identifier == current.identifier
			&& semanticFingerprint == current.semanticFingerprint
			&& actionableTraits == current.actionableTraits
			&& parentSemanticFingerprint == current.parentSemanticFingerprint
	}

	func canAdvanceWithinBatch(to current: AXElementIdentitySnapshot, afterAction action: String) -> Bool {
		guard action == "setText", stableSemanticFingerprint == current.stableSemanticFingerprint else { return false }
		return contextMatches(current)
			&& role == current.role
			&& subrole == current.subrole
			&& identifier == current.identifier
			&& actionableTraits == current.actionableTraits
			&& parentSemanticFingerprint == current.parentSemanticFingerprint
	}

	func matchesRefindCandidate(_ candidate: AXElementIdentitySnapshot) -> Bool {
		guard contextMatches(candidate), role == candidate.role, subrole == candidate.subrole else { return false }
		if !identifier.isEmpty {
			return candidate.identifier == identifier
		}
		guard !semanticFingerprint.isEmpty, !parentSemanticFingerprint.isEmpty else { return false }
		return semanticFingerprint == candidate.semanticFingerprint
			&& actionableTraits == candidate.actionableTraits
			&& parentSemanticFingerprint == candidate.parentSemanticFingerprint
	}

	private func contextMatches(_ other: AXElementIdentitySnapshot) -> Bool {
		pid == other.pid && processIdentity == other.processIdentity && windowId == other.windowId
	}
}

enum AXElementIdentityValidation: Equatable {
	case valid
	case staleProcess
	case staleRef
}

func validateAXElementIdentity(
	_ snapshot: AXElementIdentitySnapshot,
	targetPid: Int32,
	processIdentity: ComputerUseProcessIdentity,
	windowId: UInt32
) -> AXElementIdentityValidation {
	guard snapshot.pid == targetPid else { return .staleRef }
	guard snapshot.processIdentity == processIdentity else { return .staleProcess }
	guard snapshot.windowId == windowId else { return .staleRef }
	return .valid
}

func uniqueAXRefindCandidateIndex(
	snapshot: AXElementIdentitySnapshot,
	candidates: [AXElementIdentitySnapshot]
) -> Int? {
	let matches = candidates.indices.filter { snapshot.matchesRefindCandidate(candidates[$0]) }
	guard matches.count == 1 else { return nil }
	return matches[0]
}
