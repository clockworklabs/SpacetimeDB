# visionOS Support Roadmap (Deferred)

## Status

Deferred for now. Revisit when there is a concrete adopter or roadmap priority requiring native visionOS positioning.

## Goal

Add first-class visionOS support to the SpacetimeDB Swift SDK in a staged way, starting with package/CI compatibility and then moving to broader product confidence.

## Why This Is Worth Doing

- Increases Apple ecosystem trust for app teams evaluating SDK portability.
- SDK architecture is runtime/network heavy (not UI-heavy), so baseline support is relatively low risk.
- Creates a clearer path for Vision Pro-native apps if/when demand appears.

## Why We Are Deferring Full Investment

- Current value can be captured with iOS/macOS support and compatibility paths.
- Deeper investment is best justified by explicit customer demand or internal product targets.

## Trigger Criteria To Start

Start this roadmap when one or more are true:

- A customer or internal app team requests native visionOS support.
- Mirror repo + SPI distribution is stable and release operations are routine.
- CI capacity is available for another simulator target without destabilizing baseline throughput.

## Proposed Execution Plan

1. Phase 0: Compile/Posture Enablement
- Add `.visionOS(...)` to `Package.swift` with a pinned minimum version.
- Replace current CI guard with actual visionOS simulator compile validation.
- Update README/DocC/PUBLISHING support matrix and release checklist.

2. Phase 1: Runtime Confidence
- Run full Swift SDK tests in visionOS-compatible lanes where feasible.
- Validate networking, reconnect, keepalive, keychain usage, and cancellation/timeout paths.
- Add targeted regression tests for any platform-specific behavior differences.

3. Phase 2: Productization
- Add a minimal visionOS sample/integration reference app if needed.
- Confirm SPI platform badges and documentation correctly reflect support.
- Promote support level from experimental to supported once release stability criteria are met.

## Exit Criteria (Definition of Done)

- `Package.swift` declares visionOS support.
- CI validates visionOS simulator compile for SDK target (and tests if feasible).
- Docs and publishing guides state explicit visionOS support posture.
- Release checklist includes visionOS validation.
- No known visionOS-specific regressions remain open for GA support level.

## Initial Time Box (Suggested)

- Phase 0: 0.5-1.0 engineer day
- Phase 1: 1-2 engineer days
- Phase 2: optional, based on adoption and product goals

## Owner

Swift SDK maintainers (`swift-integration` branch owners)
