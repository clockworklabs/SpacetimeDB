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

## Documentation

For the SpacetimeDB platform documentation, see [spacetimedb.com/docs](https://spacetimedb.com/docs).

## Internal Developer Documentation

See [`DEVELOP.md`](./DEVELOP.md).
