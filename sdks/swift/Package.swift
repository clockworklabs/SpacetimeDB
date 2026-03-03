// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "SpacetimeDB",
    platforms: [
        .macOS(.v15),
        .iOS(.v18),
        .visionOS(.v2),
        .watchOS(.v11)
    ],
    products: [
        .library(
            name: "SpacetimeDB",
            targets: ["SpacetimeDB"]
        ),
    ],
    dependencies: [
        .package(
            url: "https://github.com/ordo-one/package-benchmark",
            from: "1.30.0",
            traits: []
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
        .executableTarget(
            name: "SpacetimeDBBenchmarks",
            dependencies: [
                "SpacetimeDB",
                .product(name: "Benchmark", package: "package-benchmark"),
            ],
            path: "Benchmarks/SpacetimeDBBenchmarks",
            plugins: [
                .plugin(name: "BenchmarkPlugin", package: "package-benchmark"),
            ]
        ),
        .executableTarget(
            name: "GeneratedBindingsBenchmarks",
            dependencies: [
                "SpacetimeDB",
                .product(name: "Benchmark", package: "package-benchmark"),
            ],
            path: "Benchmarks/GeneratedBindingsBenchmarks",
            plugins: [
                .plugin(name: "BenchmarkPlugin", package: "package-benchmark"),
            ]
        ),
    ]
)
