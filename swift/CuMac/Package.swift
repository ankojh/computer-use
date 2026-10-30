// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "CuMac",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "CuMac", type: .static, targets: ["CuMac"]),
    ],
    targets: [
        .systemLibrary(name: "CCu", path: "include"),
        .target(
            name: "CuMac",
            dependencies: ["CCu"],
            path: "Sources/CuMac",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .testTarget(
            name: "CuMacTests",
            dependencies: ["CuMac", "CCu"],
            path: "Tests/CuMacTests",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
    ]
)
