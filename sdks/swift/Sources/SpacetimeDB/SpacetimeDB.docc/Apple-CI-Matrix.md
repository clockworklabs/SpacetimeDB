# Apple CI Matrix (macOS, iOS Simulator)

## Goals

- Keep package quality visible to Apple app teams.
- Verify host and simulator compatibility in every PR.
- Keep platform posture explicit: visionOS is not targeted yet.

## Recommended matrix

- `macOS`:
  - `swift test --package-path sdks/swift`
  - lockfile validation
  - demo builds
  - benchmark smoke
  - DocC build smoke
- `iOS Simulator`:
  - cross-compile SDK target using iOS simulator SDK
- `visionOS`:
  - intentionally unsupported in this package right now
  - CI fails if `.visionOS(...)` is added without a coordinated posture update

## iOS simulator build command

```bash
IOS_SDK_PATH="$(xcrun --sdk iphonesimulator --show-sdk-path)"
swift build \
  --package-path sdks/swift \
  --target SpacetimeDB \
  --triple arm64-apple-ios17.0-simulator \
  --sdk "$IOS_SDK_PATH"
```

## visionOS posture guard

```bash
if rg -q '\.visionOS\(' sdks/swift/Package.swift; then
  echo "visionOS currently unsupported; update CI/docs posture before enabling."
  exit 1
fi
```

## DocC smoke command

```bash
tools/swift-docc-smoke.sh
```
