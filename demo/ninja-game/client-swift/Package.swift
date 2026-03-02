// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "NinjaGameClient",
    platforms: [
        .macOS(.v15),
        .iOS(.v17)
    ],
    products: [
        .executable(
            name: "NinjaGameClient",
            targets: ["NinjaGameClient"]
        )
    ],
    dependencies: [
        .package(name: "SpacetimeDB", path: "../../../sdks/swift")
    ],
    targets: [
        .executableTarget(
            name: "NinjaGameClient",
            dependencies: [
                .product(name: "SpacetimeDB", package: "SpacetimeDB")
            ],
            resources: [
                .process("Resources")
            ]
        )
    ]
)
