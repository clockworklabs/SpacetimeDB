// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "SimpleModuleClient",
    platforms: [
        .macOS(.v15),
        .iOS(.v17)
    ],
    products: [
        .executable(
            name: "SimpleModuleClient",
            targets: ["SimpleModuleClient"]
        )
    ],
    dependencies: [
        .package(name: "SpacetimeDB", path: "../../../sdks/swift")
    ],
    targets: [
        .executableTarget(
            name: "SimpleModuleClient",
            dependencies: [
                .product(name: "SpacetimeDB", package: "SpacetimeDB")
            ]
        )
    ]
)
