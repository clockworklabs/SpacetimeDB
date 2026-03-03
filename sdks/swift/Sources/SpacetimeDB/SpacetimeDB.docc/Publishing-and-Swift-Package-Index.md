# Publishing DocC and Submitting to Swift Package Index

## Repository shape requirement

Swift Package Manager dependencies and Swift Package Index expect a package at repository root.

This monorepo places the Swift package under `sdks/swift`, so public distribution should use a dedicated Swift package repository (for example `spacetimedb-swift`) that mirrors this directory at root.

## 1. Prepare the package repo

- Mirror `sdks/swift` to a standalone repository root.
- Keep `Package.swift`, `Package.resolved`, `Sources`, and `Tests` at that root.
- Keep `.spi.yml` at that root (see file in this SDK directory).

## 2. Tag releases

- Use semantic versioning tags (`v0.1.0`, `v0.2.0`, ...).
- Ensure the tag includes updated DocC content and changelog notes.

## 3. Build DocC locally

From package root:

```bash
tools/swift-docc-smoke.sh
```

## 4. Validate package and docs

```bash
swift test
swift package resolve --force-resolved-versions
```

## 5. Submit to Swift Package Index

1. Open [https://swiftpackageindex.com/add-a-package](https://swiftpackageindex.com/add-a-package).
2. Submit the public package repository URL.
3. Verify docs are generated for target `SpacetimeDB`.
4. Add package keywords, README metadata, and compatibility notes.

## 6. Release checklist

- CI green across macOS + iOS simulator builds
- Benchmark smoke pass
- DocC builds without errors
- Tag pushed and visible on SPI
