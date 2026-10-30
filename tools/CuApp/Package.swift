// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "CuApp",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(name: "CuApp", swiftSettings: [.swiftLanguageMode(.v5)]),
        .testTarget(name: "CuAppTests", dependencies: ["CuApp"],
                    swiftSettings: [.swiftLanguageMode(.v5)])
    ]
)
