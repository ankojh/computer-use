// swift-tools-version: 6.0
import PackageDescription

// Test target app for input/capture integration tests (HANDOFF.md §16).
let package = Package(
    name: "CuTarget",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "CuTarget",
            path: "Sources/CuTarget",
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
    ]
)
