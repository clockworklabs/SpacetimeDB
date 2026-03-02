# SpacetimeDB Swift SDK

Native Swift package for SpacetimeDB client connectivity, BSATN encoding/decoding, and local replica cache integration.

## Location

This SDK now lives at:

`sdks/swift`

## Build And Test

From repo root:

```bash
swift test --package-path sdks/swift
```

## Use In A Local Swift Package

```swift
dependencies: [
    .package(name: "SpacetimeDB", path: "../../../sdks/swift")
]
```

Then add the product dependency:

```swift
.product(name: "SpacetimeDB", package: "SpacetimeDB")
```

## Quick Validation Matrix

From repo root:

```bash
swift test --package-path sdks/swift
swift build --package-path demo/simple-module/client-swift
swift build --package-path demo/ninja-game/client-swift
```

## Notes

- `tools/swift-procedure-e2e.sh` runs a procedure callback E2E integration scenario.
- CI workflow for this package and Swift demos: `.github/workflows/swift-sdk.yml`.
