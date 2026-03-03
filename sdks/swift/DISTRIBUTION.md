# Swift SDK Distribution Runbook

This runbook defines how to ship `sdks/swift` as a standalone public Swift package for SPM and Swift Package Index.

Execution checklist (submission + badge verification):

- `sdks/swift/SPI_SUBMISSION_CHECKLIST.md`

## Why Mirror Is Required

The monorepo root is not a Swift package root. Public SPM consumers and Swift Package Index expect `Package.swift` at repository root.

Use a dedicated mirror repository that contains the contents of `sdks/swift` at repository root (for example: `spacetimedb-swift`).

## One-Time Setup

1. Create the public mirror repository.
2. Clone it locally.
3. Add/keep `sdks/swift/.spi.yml` in mirror root.
4. Ensure `README.md` includes Swift Package Index links/badges once package is indexed.

## Sync And Release Automation

Automation script:

- `tools/swift-package-mirror.sh`

Sync only:

```bash
tools/swift-package-mirror.sh sync --mirror ../spacetimedb-swift
```

Create release commit + tag:

```bash
tools/swift-package-mirror.sh release --mirror ../spacetimedb-swift --version 0.1.0
```

Create release and push:

```bash
tools/swift-package-mirror.sh release --mirror ../spacetimedb-swift --version 0.1.0 --push
```

## Swift Package Index Submission

1. Confirm mirror repository is public and contains package at root.
2. Push release tag (`vX.Y.Z`) in the mirror repository.
3. Submit package URL at:
   - <https://swiftpackageindex.com/add-a-package>
4. Wait for indexing + documentation build to complete.

## Swift Package Index Badge/Link Templates

Replace `<owner>/<repo>` with the mirror repository coordinates.

Package page:

- `https://swiftpackageindex.com/<owner>/<repo>`

Swift versions badge:

- `https://img.shields.io/endpoint?url=https://swiftpackageindex.com/api/packages/<owner>/<repo>/badge?type=swift-versions`

Platforms badge:

- `https://img.shields.io/endpoint?url=https://swiftpackageindex.com/api/packages/<owner>/<repo>/badge?type=platforms`

## Verification Checklist

- Mirror repo root contains `Package.swift`, `Sources`, `Tests`, `.spi.yml`.
- Mirror release tag pushed (`vX.Y.Z`).
- Package appears on Swift Package Index.
- Swift Package Index docs build succeeds.
- README in mirror repo contains working SPI package link and badges.
