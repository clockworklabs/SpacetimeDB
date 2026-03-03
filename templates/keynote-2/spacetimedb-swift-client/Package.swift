// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "spacetimedb-swift-client",
    platforms: [
        .macOS(.v15)
    ],
    dependencies: [
        .package(name: "SpacetimeDBPkg", path: "../../../sdks/swift")
    ],
    targets: [
        .executableTarget(
            name: "SpacetimeDBSwiftTransferSim",
            dependencies: [
                .product(name: "SpacetimeDB", package: "SpacetimeDBPkg")
            ]
        )
    ]
)
