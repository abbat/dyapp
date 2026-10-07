// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "DYApp",
    platforms: [
        .iOS(.v14),
        .macOS(.v11),
    ],
    products: [
        .library(name: "DYApp", targets: ["DYApp"]),
    ],
    dependencies: [
        // Rust FFI will be linked via build settings
    ],
    targets: [
        .target(
            name: "DYApp",
            dependencies: [],
            path: "DYApp",
            sources: [
                "App.swift",
                "ContentView.swift",
                "RustBridge.swift",
            ],
            swiftSettings: [
                .unsafeFlags(["-suppress-warnings"], .when(configuration: .debug)),
            ]
        ),
        .testTarget(
            name: "DYAppTests",
            dependencies: ["DYApp"],
            path: "Tests",
            sources: [
                "AppTests.swift",
                "RustBridgeTests.swift",
            ]
        ),
    ]
)
