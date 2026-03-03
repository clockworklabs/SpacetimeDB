# High Level Goals

## Program Context

We are delivering first-class Swift support for SpacetimeDB inside the monorepo, then upstreaming it as an open-source PR.

Work is happening on branch `swift-integration` and includes:

- Swift runtime SDK (`sdks/swift`)
- Swift code generation in Rust/CLI (`crates/codegen`, `crates/cli`)
- End-to-end Swift demos (`demo/simple-module/client-swift`, `demo/ninja-game/client-swift`)
- CI, tooling, docs, and distribution hardening for Apple app teams

## Updated Product Direction

The goal is a pure Swift, lightweight, high-performance realtime client SDK for Apple development.

Important scope clarification:

- The SDK is rendering-agnostic and does not depend on Metal.
- Demo rendering is currently SwiftUI/Canvas-based.
- The key value is low-latency, reactive state replication into native Swift app/game loops.

## End-State Goals

1. Ship a production-credible Swift SDK in the monorepo and upstream PR.
2. Keep runtime path pure Swift with no third-party runtime dependency burden.
3. Provide typed Swift bindings from SpacetimeDB schema via official CLI generation.
4. Prove end-to-end realtime behavior with native Swift demo clients.
5. Add packaging, documentation, benchmarks, and CI guardrails so Apple teams can trust releases.

## Workstreams

### 1) Swift Runtime SDK

Goal: stable native runtime with robust transport, cache, and API ergonomics.

Target outcomes:

- BSATN encoding/decoding and v2 protocol support
- websocket transport with reconnect and keepalive handling
- local replica cache with table delta callbacks
- reducer/procedure/query APIs (callback + async/await)
- token persistence and observability hooks

### 2) Swift Code Generation

Goal: avoid hand-written schema glue; generate strongly-typed Swift APIs.

Target outcomes:

- `spacetime generate --lang swift` support in official CLI
- generated rows/tables/reducers/procedures/module registration
- binding-drift checks in CI for demo artifacts

### 3) Demo Validation Apps

Goal: prove practical integration and realtime behavior in native Swift clients.

Target outcomes:

- `simple-module` demo validates connect/add/delete/replica updates
- `ninja-game` demo validates higher-frequency multiplayer state updates and gameplay loops
- both demos build and stay in sync with generated bindings

### 4) Apple Ecosystem Hardening

Goal: make the package consumable and trustworthy for app teams.

Target outcomes:

- DocC docs/tutorials and publishing guides
- Apple CI matrix (macOS + iOS simulator; explicit current visionOS posture)
- package benchmark suite and baseline capture tooling
- mirror/release automation for package-root SPI/SPM distribution

## Current Status Snapshot

Broadly complete:

- Native Swift runtime package with tests
- Swift codegen backend and CLI integration
- Two Swift demo clients in-repo
- CI coverage for tests/builds/drift/E2E/bench smoke/docs smoke
- Observability, keepalive/state surface, connectivity-aware reconnect
- Keychain token utility
- Benchmark suite + baseline tooling
- DocC + SPI config + distribution runbooks

Remaining high-priority external step:

- Submit Swift mirror package repository to Swift Package Index and verify docs/platform badges

## PR Success Criteria

1. Branch demonstrates complete Swift SDK path (runtime + codegen + demos + CI).
2. Validation matrix is green and reproducible.
3. Documentation reflects actual behavior and support posture.
4. PR narrative (`changes.md`) is comprehensive and accurate.
5. Scope claims match implementation (native Swift realtime SDK, not a Metal-specific SDK).

## Principles

- Runtime-first correctness over UI-specific coupling.
- Pure Swift integration path for Apple developers.
- Strong typing via generation, not manual schema translation.
- CI-enforced reproducibility and drift prevention.
- Clear support posture and release process documentation.

## Near-Term Next Goals

1. Execute manual SPI submission for mirror repo and verify badge endpoints.
2. Finalize any remaining TODO audit items and reflect them in backlog docs.
3. Keep Swift parity improvements scoped and test-backed (builder ergonomics, optional helper APIs).
4. Revisit visionOS support when trigger criteria in `sdks/swift/VISIONOS_ROADMAP.md` are met.

## Non-Goals (Current Phase)

- Building a Metal-specific rendering SDK layer.
- Coupling SDK internals to any single game/UI framework.
- Expanding platform support claims beyond documented CI-backed posture.
