// swift-tools-version: 6.2
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "SpacetimeDB",
    platforms: [
        .macOS(.v15),
        .iOS(.v17)
    ],
    products: [
        // Products define the executables and libraries a package produces, making them visible to other packages.
        .library(
            name: "SpacetimeDB",
            targets: ["SpacetimeDB"]
        ),
    ],
    targets: [
        .target(
            name: "SpacetimeDB"
        ),
        .testTarget(
            name: "SpacetimeDBTests",
            dependencies: ["SpacetimeDB"]
        ),
    ]
)
