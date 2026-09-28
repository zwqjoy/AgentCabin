// swift-tools-version: 5.9
import PackageDescription

let package = Package(
	name: "CoordinateAXPress",
	products: [.library(name: "CoordinateAXPress", targets: ["CoordinateAXPress"])],
	targets: [
		.target(
			name: "CoordinateAXPress",
			path: ".",
			exclude: ["Tests", "Package.swift", ".build", "agentcabin-computer-use.app", "bridge.swift", "agent_cursor.swift", "agent_cursor_motion.swift", "ocr-helper"],
			sources: ["CoordinateAXPress.swift", "AXElementIdentity.swift"]
		),
		.testTarget(name: "CoordinateAXPressTests", dependencies: ["CoordinateAXPress"], path: "Tests"),
	]
)
