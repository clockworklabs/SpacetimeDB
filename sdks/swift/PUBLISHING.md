# Swift SDK Publishing Guide

This guide prepares the Swift SDK for public Apple ecosystem consumption.

## Scope

- DocC documentation and tutorials
- Swift Package Index submission
- Apple platform CI confidence (macOS + iOS simulator; visionOS not targeted yet)
- mirror repository release process for public SPM consumption

## Important Packaging Constraint

The monorepo root is not a Swift package root. To publish as an SPM dependency and submit to Swift Package Index, use a dedicated package repository with the Swift SDK directory contents at repository root.

Detailed distribution runbook:

- `sdks/swift/DISTRIBUTION.md`

Mirror/release automation:

- `tools/swift-package-mirror.sh`

Operational checklist for SPI submission + badge verification:

- `sdks/swift/SPI_SUBMISSION_CHECKLIST.md`

## DocC

DocC bundle location:

- `Sources/SpacetimeDB/SpacetimeDB.docc`

Build docs locally:

```bash
tools/swift-docc-smoke.sh
```

## Swift Package Index Submission

1. Ensure public repository URL is accessible.
2. Push semantic version tag in the mirror repo (`vX.Y.Z`).
3. Submit URL at [https://swiftpackageindex.com/add-a-package](https://swiftpackageindex.com/add-a-package).
4. Verify docs generation and badge endpoints:
   - Swift versions badge
   - Supported platforms badge

## CI Matrix

The Swift SDK workflow should include:

- macOS quality run: tests, lockfile validation, docs smoke, demos, benchmark smoke
- iOS simulator compile run for `SpacetimeDB` target
- explicit guard that visionOS is not targeted yet (`.visionOS(...)` absent in `Package.swift`)

Current workflow file:

- `.github/workflows/swift-sdk.yml`

## Release Checklist

- `swift test --package-path sdks/swift`
- `swift package --package-path sdks/swift resolve --force-resolved-versions`
- `tools/swift-benchmark-smoke.sh`
- `tools/swift-benchmark-baseline.sh <machine-profile-baseline-name>`
- `tools/swift-docc-smoke.sh`
- CI matrix green on PR and default branch
- `tools/swift-package-mirror.sh sync --mirror <mirror-repo-path>`
- `tools/swift-package-mirror.sh release --mirror <mirror-repo-path> --version <X.Y.Z> --push`
- Submit mirror repo URL to Swift Package Index
- Verify SPI package page + docs + badge URLs
