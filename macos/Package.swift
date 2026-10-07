// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "DYAppMacOS",
    platforms: [
        .macOS(.v11),
    ],
    products: [
        .executable(name: "DYApp", targets: ["DYApp"]),
    ],
    dependencies: [],
    targets: [
        .executableTarget(
            name: "DYApp",
            dependencies: [],
            path: "src"
        ),
        .testTarget(
            name: "DYAppTests",
            dependencies: ["DYApp"],
            path: "Tests"
        ),
    ]
)
