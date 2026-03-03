// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "spacetimedb-swift-client",
    platforms: [
        .macOS(.v15)
    ],
    dependencies: [
        .package(name: "SpacetimeDBPkg", path: "../../../sdks/swift"),
        .package(url: "https://github.com/apple/swift-atomics.git", from: "1.2.0")
    ],
    targets: [
        .executableTarget(
            name: "SpacetimeDBSwiftTransferSim",
            dependencies: [
                .product(name: "SpacetimeDB", package: "SpacetimeDBPkg"),
                .product(name: "Atomics", package: "swift-atomics")
            ]
        )
    ]
)
