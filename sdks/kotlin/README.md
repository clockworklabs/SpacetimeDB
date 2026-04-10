# SpacetimeDB Kotlin SDK

## Overview

The Kotlin Multiplatform (KMP) client SDK for [SpacetimeDB](https://spacetimedb.com). Targets **JVM** and **iOS** (arm64, simulator-arm64, x64), enabling native SpacetimeDB clients from Kotlin, Java, and Swift (via KMP interop).

## Features

- BSATN binary protocol (`v2.bsatn.spacetimedb`)
- Subscriptions with SQL query support
- One-off queries (suspend and callback variants)
- Reducer invocation with result callbacks
- Automatic reconnection with exponential backoff
- Ping/pong keep-alive (30s idle timeout)
- Gzip and Brotli message decompression
- Client-side row cache with ref-counted rows

## Quick Start

```kotlin
val conn = DbConnection.builder()
    .withUri("ws://localhost:3000")
    .withModuleName("my_module")
    .onConnect { conn, identity, token ->
        println("Connected as $identity")

        // Subscribe to table changes
        conn.subscriptionBuilder()
            .onApplied { println("Subscription active") }
            .subscribe("SELECT * FROM users")

        // Observe a table
        conn.table("users").onInsert { row ->
            println("New user row: ${row.size} bytes")
        }
    }
    .onDisconnect { _, error ->
        println("Disconnected: ${error?.message ?: "clean"}")
    }
    .build()
```

## Installation

Add to your `build.gradle.kts`:

```kotlin
kotlin {
    sourceSets {
        commonMain.dependencies {
            implementation("com.clockworklabs:spacetimedb-sdk:0.1.0")
        }
    }
}
```

## Performance

The SDK has been benchmarked against the reference Rust benchmark client using the keynote-2 fund transfer workload (10 connections, 16384 max in-flight reducers, 100k accounts, Zipf distribution):

| Client | Avg TPS (Apple Silicon) |
|--------|------------------------|
| Rust (v1 protocol) | ~72,000 |
| Kotlin (v2 protocol) | ~88,000 |

See `Keynote2BenchmarkTest.kt` for the full benchmark and methodology.

## Roadmap

### Phase 1 — Code Generation & Typed Access

The highest-impact gap. Currently rows are raw BSATN bytes. Code generation will produce:

- **Typed table classes** — Generated data classes per table with proper field types, `equals`/`hashCode`, and BSATN serialization. Access rows as `Player(id=42, name="Alice")` instead of raw `ByteArray`.
- **Typed reducer calls** — Generated functions like `conn.reducers.addPlayer("Alice")` instead of manually encoding BSATN args.
- **Typed event callbacks** — `table.onInsert { player: Player -> ... }` instead of raw byte callbacks.

This requires adding a Kotlin backend to the `spacetime generate` CLI command (alongside the existing C#, TypeScript, and Rust backends in `crates/codegen/`).

### Phase 2 — Event System & Procedure Support

- **Rich event context** — Add `ReducerEventContext` and `SubscriptionEventContext` to callbacks, providing metadata about who triggered the event and transaction details (matching C#/Rust SDKs).
- **Procedure calls** — Support for `CallProcedure` (non-transactional server-side functions), completing API parity with the C# and Rust SDKs.

### Phase 3 — Observability

- **Structured logging** — Pluggable logging interface with configurable log levels (Debug/Info/Warn/Error). Default implementation for SLF4J on JVM and OSLog on iOS.
- **Connection metrics** — Request latency tracking, message counts, and byte throughput. Expose via Micrometer on JVM for Prometheus/Grafana integration.

### Phase 4 — Framework Integrations

- **Jetpack Compose** — `rememberSpacetimeDB()` composable, `collectAsState()` extensions for table subscriptions, lifecycle-aware connection management.
- **Kotlin Flow** — `Flow<List<T>>` adapters for table subscriptions, enabling reactive pipelines with `map`/`filter`/`combine`.
- **SwiftUI** — `@Observable` wrappers via KMP interop for native iOS integration.

### Phase 5 — Platform Expansion

- **Android target** — Dedicated Android source set with lifecycle integration, ProGuard rules, and a sample app.
- **Maven Central publish** — Automated release pipeline with version management.
- **Light mode & confirmed reads** — Advanced connection options to reduce bandwidth or increase consistency guarantees.

### Feature Parity Status

| Feature | Kotlin | C# | TypeScript | Rust |
|---------|--------|-----|-----------|------|
| BSATN Protocol | done | done | done | done |
| WebSocket Transport | done | done | done | done |
| Reconnection | done | done | done | done |
| Compression | done | done | done | done |
| Keep-Alive | done | done | done | done |
| Row Cache | done | done | done | done |
| Subscriptions | done | done | done | done |
| One-off Queries | done | done | done | done |
| Reducer Calls | done | done | done | done |
| Procedure Calls | planned | done | done | done |
| Code Generation | planned | done | done | done |
| Event System | partial | done | done | done |
| Logging | planned | done | done | done |
| Metrics | planned | done | done | done |
| Framework Integration | planned | done (Unity) | done (React/Vue) | — |

## Documentation

For the SpacetimeDB platform documentation, see [spacetimedb.com/docs](https://spacetimedb.com/docs).

## Internal Developer Documentation

See [`DEVELOP.md`](./DEVELOP.md).
