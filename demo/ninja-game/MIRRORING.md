# Mirror Notes

This repository mirrors `demo/ninja-game` from the SpacetimeDB monorepo.

Upstream source:
- https://github.com/avias8/SpacetimeDB
- Path: `demo/ninja-game`

Client dependency:
- Swift SDK package: `https://github.com/avias8/spacetimedb-swift.git`
- Version: `from: 0.21.0`

Suggested sync workflow:
1. Copy latest `demo/ninja-game` from upstream.
2. Keep `client-swift/Package.swift` pointing to the mirrored Swift SDK URL.
3. Run `cd client-swift && swift build && swift test` (if tests exist).
