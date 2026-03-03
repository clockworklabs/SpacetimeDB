# SpacetimeDB Swift SDK

Native Swift SDK for connecting to SpacetimeDB over `v2.bsatn.spacetimedb`, decoding realtime updates, and maintaining a typed local cache.

## Contents

- [Requirements](#requirements)
- [Package Layout](#package-layout)
- [Add The SDK To A Swift Package](#add-the-sdk-to-a-swift-package)
- [Quick Start](#quick-start)
- [Core API](#core-api)
- [Auth Token Persistence (Keychain)](#auth-token-persistence-keychain)
- [Network Awareness And Reconnect Behavior](#network-awareness-and-reconnect-behavior)
- [Distribution Hardening](#distribution-hardening)
- [DocC and Swift Package Index](#docc-and-swift-package-index)
- [Apple CI Matrix](#apple-ci-matrix)
- [Logging](#logging)
- [Benchmarks](#benchmarks)
- [Validation Matrix](#validation-matrix)
- [Examples](#examples)

## Requirements

- Swift tools `6.2`
- Apple platforms:
  - macOS `15+`
  - iOS `17+`
  - visionOS: not supported yet

## Package Layout

```text
sdks/swift
├── Package.swift
├── Benchmarks/SpacetimeDBBenchmarks
├── Sources/SpacetimeDB
│   ├── Auth/KeychainTokenStore.swift
│   ├── BSATN/
│   ├── Cache/
│   ├── Network/
│   ├── Log.swift
│   ├── RuntimeTypes.swift
│   └── SpacetimeDB.swift
└── Tests/SpacetimeDBTests
```

## Add The SDK To A Swift Package

From a local checkout:

```swift
dependencies: [
    .package(name: "SpacetimeDB", path: "../../../sdks/swift"),
],
targets: [
    .executableTarget(
        name: "MyClient",
        dependencies: [
            .product(name: "SpacetimeDB", package: "SpacetimeDB"),
        ]
    ),
]
```

Then import in code:

```swift
import SpacetimeDB
```

## Quick Start

```swift
import Foundation
import SpacetimeDB

@MainActor
final class AppModel: SpacetimeClientDelegate {
    private var client: SpacetimeClient?

    func start() {
        SpacetimeModule.registerTables()

        let client = SpacetimeClient(
            serverUrl: URL(string: "http://127.0.0.1:3000")!,
            moduleName: "my-module"
        )
        client.delegate = self
        client.connect()
        self.client = client
    }

    func stop() {
        client?.disconnect()
        client = nil
    }

    // MARK: SpacetimeClientDelegate
    func onConnect() {}
    func onDisconnect(error: Error?) {}
    func onIdentityReceived(identity: [UInt8], token: String) {}
    func onTransactionUpdate(message: Data?) {}
    func onReducerError(reducer: String, message: String, isInternal: Bool) {}
}
```

## Core API

### Connect/Disconnect

```swift
let client = SpacetimeClient(serverUrl: url, moduleName: "my-module")
client.delegate = delegate
client.connect(token: optionalBearerToken)
client.disconnect()
```

### Reducers

```swift
client.send("add_person", argsData)
```

### Procedures

Typed callback:

```swift
client.sendProcedure("hello", argsData, responseType: String.self) { result in
    // Result<String, Error>
}
```

Raw callback:

```swift
client.sendProcedure("hello", argsData) { result in
    // Result<Data, Error>
}
```

`async/await`:

```swift
let rawData = try await client.sendProcedure("hello", argsData)
let value = try await client.sendProcedure("hello", argsData, responseType: String.self)
let timed = try await client.sendProcedure("hello", argsData, timeout: .seconds(5))
let typedTimed = try await client.sendProcedure("hello", argsData, responseType: String.self, timeout: .seconds(5))
```

### One-Off Queries

Callback:

```swift
client.oneOffQuery("SELECT * FROM person") { result in
    // Result<QueryRows, Error>
}
```

`async/await`:

```swift
let rows = try await client.oneOffQuery("SELECT * FROM person")
let timedRows = try await client.oneOffQuery("SELECT * FROM person", timeout: .seconds(3))
```

Cancellation: cancel the task calling an async procedure/query API, and it throws `CancellationError` while removing the pending callback state.

### Subscriptions

```swift
let handle = client.subscribe(
    queries: ["SELECT * FROM person"],
    onApplied: { /* initial snapshot applied */ },
    onError: { message in /* subscription failed */ }
)

handle.unsubscribe()
```

### Client Cache

Register tables once, then read typed rows from generated caches:

```swift
SpacetimeModule.registerTables()
let people = PersonTable.cache.rows
```

The SDK applies transaction updates into `SpacetimeClient.clientCache` and table caches are updated on the main actor.

## Auth Token Persistence (Keychain)

`KeychainTokenStore` provides opt-in token persistence per module:

```swift
let tokenStore = KeychainTokenStore(service: "com.example.myapp.spacetimedb")

// Load on app start.
let savedToken = tokenStore.load(forModule: "my-module")
client.connect(token: savedToken)

// Save when identity/token arrives.
func onIdentityReceived(identity: [UInt8], token: String) {
    tokenStore.save(token: token, forModule: "my-module")
}
```

## Network Awareness And Reconnect Behavior

`SpacetimeClient` supports reconnect backoff through `ReconnectPolicy`:

```swift
let policy = ReconnectPolicy(
    maxRetries: nil,
    initialDelaySeconds: 1.0,
    maxDelaySeconds: 30.0,
    multiplier: 2.0,
    jitterRatio: 0.2
)
```

Network path changes are monitored internally. When the device is offline, reconnect attempts are deferred; when connectivity returns, reconnect is retriggered automatically.

Compression mode can be set at client construction:

```swift
let client = SpacetimeClient(
    serverUrl: url,
    moduleName: "my-module",
    reconnectPolicy: policy,
    compressionMode: .gzip // .none | .gzip | .brotli
)
```

## Distribution Hardening

The Swift package is validated in CI for reproducibility and packaging health:

- `swift test --package-path sdks/swift`
- `swift package --package-path sdks/swift resolve --force-resolved-versions`
- demo package builds
- benchmark smoke run

Dependency versions are pinned in `sdks/swift/Package.resolved` to avoid accidental drift.

For public SPM/SPI distribution from this monorepo, use the mirror runbook and automation:

- `sdks/swift/DISTRIBUTION.md`
- `tools/swift-package-mirror.sh`

## DocC and Swift Package Index

DocC bundle and tutorials live in:

- `sdks/swift/Sources/SpacetimeDB/SpacetimeDB.docc`

DocC build command:

```bash
tools/swift-docc-smoke.sh
```

Swift Package Index builder config is in:

- `sdks/swift/.spi.yml`

Detailed publishing runbook:

- `sdks/swift/PUBLISHING.md`
- `sdks/swift/DISTRIBUTION.md`
- `sdks/swift/SPI_SUBMISSION_CHECKLIST.md`

Swift Package Index link and badge templates (replace `<owner>/<repo>` with mirror coordinates):

```text
Package: https://swiftpackageindex.com/<owner>/<repo>
Swift versions badge: https://img.shields.io/endpoint?url=https://swiftpackageindex.com/api/packages/<owner>/<repo>/badge?type=swift-versions
Platforms badge: https://img.shields.io/endpoint?url=https://swiftpackageindex.com/api/packages/<owner>/<repo>/badge?type=platforms
```

## Apple CI Matrix

Swift CI runs as a platform matrix in `.github/workflows/swift-sdk.yml`:

- `macOS`: tests, lockfile validation, demos, benchmark smoke, DocC build
- `iOS simulator`: cross-build of `SpacetimeDB` target
- `visionOS`: intentionally not targeted yet; CI asserts `.visionOS(...)` is absent in `Package.swift`

## Logging

The SDK uses `os.Logger` categories:

- `Client`
- `Cache`
- `Network`

Logs are visible in Console.app and device logs using subsystem `com.clockworklabs.SpacetimeDB`.

## Benchmarks

Benchmark target: `SpacetimeDBBenchmarks`

Includes suites for:

- BSATN encode/decode
- protocol message encode/decode
- reducer/procedure request+response round-trip encode/decode
- cache insert/delete throughput

Run from repo root:

```bash
swift package --package-path sdks/swift benchmark --target SpacetimeDBBenchmarks
```

List available benchmarks:

```bash
swift package --package-path sdks/swift benchmark list
```

Run fast smoke benchmarks (used by CI):

```bash
tools/swift-benchmark-smoke.sh
```

Capture a named reproducible baseline (raw + summary + machine metadata):

```bash
tools/swift-benchmark-baseline.sh macos-arm64-14.4-swift6.2
```

Compare two captured baselines:

```bash
cd sdks/swift
swift package benchmark baseline compare <baseline-a> <baseline-b> --target SpacetimeDBBenchmarks --no-progress
```

## Validation Matrix

From repo root:

```bash
swift test --package-path sdks/swift
swift build --package-path sdks/swift
swift build --package-path demo/simple-module/client-swift
swift build --package-path demo/ninja-game/client-swift
swift package --package-path sdks/swift resolve --force-resolved-versions
swift package --package-path sdks/swift benchmark list
swift package --package-path sdks/swift benchmark --target SpacetimeDBBenchmarks
tools/swift-benchmark-smoke.sh
tools/swift-docc-smoke.sh
```

## Examples

- `demo/simple-module/client-swift`
- `demo/ninja-game/client-swift`
