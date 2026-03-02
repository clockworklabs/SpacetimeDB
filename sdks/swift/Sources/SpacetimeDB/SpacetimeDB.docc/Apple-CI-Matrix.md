# Apple CI Matrix (macOS, iOS Simulator, visionOS-if-targeted)

## Goals

- Keep package quality visible to Apple app teams.
- Verify host and simulator compatibility in every PR.
- Automatically include visionOS validation if the package targets it.

## Recommended matrix

- `macOS`:
  - `swift test --package-path sdks/swift`
  - lockfile validation
  - demo builds
  - benchmark smoke
  - DocC build smoke
- `iOS Simulator`:
  - cross-compile SDK target using iOS simulator SDK
- `visionOS Simulator`:
  - run only when `.visionOS(...)` is present in `Package.swift`

## iOS simulator build command

```bash
IOS_SDK_PATH="$(xcrun --sdk iphonesimulator --show-sdk-path)"
swift build \
  --package-path sdks/swift \
  --target SpacetimeDB \
  --triple arm64-apple-ios17.0-simulator \
  --sdk "$IOS_SDK_PATH"
```

## Conditional visionOS command

```bash
if rg -q '\.visionOS\(' sdks/swift/Package.swift; then
  XR_SDK_PATH="$(xcrun --sdk xrsimulator --show-sdk-path)"
  swift build \
    --package-path sdks/swift \
    --target SpacetimeDB \
    --triple arm64-apple-xros1.0-simulator \
    --sdk "$XR_SDK_PATH"
fi
```

## DocC smoke command

```bash
cd sdks/swift
xcodebuild docbuild \
  -scheme SpacetimeDB-Package \
  -destination 'generic/platform=macOS' \
  -derivedDataPath .build/docc \
  -skipPackagePluginValidation
```
