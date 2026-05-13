# SpacetimeDB Kotlin SDK

## Overview

The Kotlin Multiplatform (KMP) client SDK for [SpacetimeDB](https://spacetimedb.com). Targets **JVM** and **iOS** (arm64, simulator-arm64, x64), enabling native SpacetimeDB clients from Kotlin, Java, and Swift (via KMP interop).

## Features

- BSATN binary protocol (`v2.bsatn.spacetimedb`)
- Subscriptions with SQL and typed query builder support
- One-off queries (suspend and callback variants)
- Reducer invocation with result callbacks
- Procedure invocation (non-transactional server-side functions)
- Token-based authentication with short-lived token exchange
- Automatic reconnection with exponential backoff
- Keep-alive pings (30s interval)
- Gzip and Brotli message decompression
- Client-side row cache with ref-counted overlapping subscriptions
- Typed code generation via `spacetime generate --lang kotlin`
- Configurable package/namespace for generated code

## Quick Start

### With generated client bindings

```kotlin
val conn = DbConnection.builder()
    .withUri("ws://localhost:3000")
    .withModuleName("my_module")
    .onConnect { conn, identity, token ->
        println("Connected as $identity")

        // Subscribe with typed query builder
        conn.subscriptionBuilder()
            .addQuery { users }
            .addQuery { roles.where { cols -> cols.name.eq("admin") } }
            .onApplied { println("Subscribed") }
            .subscribe()

        // Typed row callbacks
        conn.db.users.onInsert { ctx, row ->
            println("${row.name} inserted")
        }
        conn.db.users.onDelete { ctx, row ->
            println("${row.name} deleted")
        }
        conn.db.users.onUpdate { ctx, oldRow, newRow ->
            println("${oldRow.name} -> ${newRow.name}")
        }

        // Typed reducer calls
        conn.reducers.provisionRole(ProvisionRoleArgs(name = "admin"))
    }
    .onDisconnect { _, error ->
        println("Disconnected: ${error?.message ?: "clean"}")
    }
    .build()
```

### Without code generation (raw SDK)

```kotlin
val conn = DbConnection.builder()
    .withUri("ws://localhost:3000")
    .withModuleName("my_module")
    .build()

conn.subscriptionBuilder()
    .subscribe("SELECT * FROM users")

conn.table("users").onInsert { rowBytes ->
    // Raw BSATN bytes — use generated code for typed access
}
```

## Installation

Add the dependency to your `build.gradle.kts`:

```kotlin
repositories {
    mavenCentral()
    mavenLocal() // if building from source
}

kotlin {
    sourceSets {
        commonMain.dependencies {
            implementation("com.clockworklabs:spacetimedb-sdk:0.1.0")
        }
    }
}
```

## Code Generation

Generate typed client bindings from your SpacetimeDB module:

```bash
spacetime generate --lang kotlin --out-dir src/commonMain/kotlin --namespace my.package
```

This produces:
- **Types/** — Data classes for each type with BSATN `read`/`write` companions
- **Tables/** — Typed table handles with `onInsert`/`onDelete`/`onUpdate` callbacks
- **Reducers/** — Reducer args classes + `RemoteReducers` methods
- **Procedures/** — Procedure args classes + `RemoteProcedures` methods
- **RemoteModule.kt** — `RemoteTables`, `RemoteReducers`, `RemoteProcedures`, `From`, extensions

### Generated API

```kotlin
conn.db.users.subscribe()                              // subscribe via table handle
conn.subscriptionBuilder()
    .addQuery { users }                                 // typed query builder
    .addQuery { users.where { cols -> cols.age.gt(18) } }
    .subscribe()

conn.reducers.addPlayer(AddPlayerArgs(name = "Alice"))  // typed reducer call
conn.db.users.onInsert { ctx, row -> ... }              // typed row callback
conn.db.users.find { it.name == "Alice" }               // typed query
conn.unsubscribeAll()                                   // unsubscribe everything
```

## Features

### Connection

- `withToken(token)` — authenticate with an auth token
- `withCompression(mode)` — Gzip or Brotli compression
- `withReconnectPolicy(policy)` — automatic reconnection with backoff
- `withConfirmedReads(enabled)` — wait for durable confirmation
- `withLightMode(enabled)` — reduced network data

### Subscriptions

- `subscriptionBuilder()` — fluent builder with `onApplied`, `onError`, `onEnded`
- `subscribe(vararg queries)` — subscribe to SQL queries
- `unsubscribe()` / `unsubscribeThen(onEnded)` — end a subscription
- `unsubscribeAll()` — end all subscriptions on the connection

### Row Callbacks

- `onInsert { ctx, row -> ... }` — called on row insertion
- `onDelete { ctx, row -> ... }` — called on row deletion
- `onUpdate { ctx, oldRow, newRow -> ... }` — called on row update (tables with primary key)
- Callbacks fire for both initial subscription data and subsequent transaction updates

### Reducers & Procedures

- `conn.reducers.myReducer(MyReducerArgs(...))` — typed reducer call
- `conn.callReducer(name, args, callback?)` — raw reducer call
- `conn.procedures.myProcedure(MyProcedureArgs(...)) { result -> ... }` — typed procedure call
- `conn.callProcedure(name, args, callback?)` — raw procedure call

## Documentation

For the SpacetimeDB platform documentation, see [spacetimedb.com/docs](https://spacetimedb.com/docs).

## Internal Developer Documentation

See [`DEVELOP.md`](./DEVELOP.md).
