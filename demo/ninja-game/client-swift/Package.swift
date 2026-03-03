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
        .package(url: "https://github.com/avias8/spacetimedb-swift.git", from: "0.21.0")
    ],
    targets: [
        .executableTarget(
            name: "NinjaGameClient",
            dependencies: [
                .product(name: "SpacetimeDB", package: "spacetimedb-swift")
            ],
            resources: [
                .process("Resources")
            ]
        )
    ]
)
