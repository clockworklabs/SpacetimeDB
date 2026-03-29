---
title: Kotlin Reference
slug: /clients/kotlin
---

The SpacetimeDB client SDK for Kotlin Multiplatform, targeting Android, JVM (Desktop), and iOS/Native.

Two templates are available:
- `basic-kt` — JVM-only console app (simplest starting point)
- `compose-kt` — Compose Multiplatform app targeting Android and Desktop

Before diving into the reference, you may want to review:

- [Generating Client Bindings](./00200-codegen.md) - How to generate Kotlin bindings from your module
- [Connecting to SpacetimeDB](./00300-connection.md) - Establishing and managing connections
- [SDK API Reference](./00400-sdk-api.md) - Core concepts that apply across all SDKs

| Name                                                              | Description                                                                                                                            |
| ----------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| [Project setup](#project-setup)                                   | Configure your Kotlin project to use the SpacetimeDB Kotlin SDK.                                                                       |
| [Generate module bindings](#generate-module-bindings)             | Generated types and how the Gradle plugin automates codegen.                                                                           |
| [`DbConnection` type](#type-dbconnection)                         | A connection to a remote database.                                                                                                     |
| [`EventContext` type](#type-eventcontext)                         | Context available in row and reducer callbacks.                                                                                        |
| [Access the client cache](#access-the-client-cache)               | Query subscribed rows and register row callbacks.                                                                                      |
| [Observe and invoke reducers](#observe-and-invoke-reducers)       | Call reducers and register callbacks for reducer events.                                                                               |
| [Subscribe to queries](#subscribe-to-queries)                     | Subscribe to table data using the type-safe query builder.                                                                             |
| [Identify a client](#identify-a-client)                           | Types for identifying users and client connections.                                                                                    |
| [Type mappings](#type-mappings)                                   | How SpacetimeDB types map to Kotlin types.                                                                                             |

## Project setup

### Using `spacetime dev` (recommended)

The fastest way to get started:

```bash
# JVM-only console app
spacetime dev --template basic-kt

# Compose Multiplatform (Android + Desktop)
spacetime dev --template compose-kt
```

Both templates come with the Gradle plugin pre-configured.

### Manual setup

Add the SpacetimeDB Gradle plugin to your `build.gradle.kts`:

```kotlin
plugins {
    id("com.clockworklabs.spacetimedb")
}

spacetimedb {
    modulePath.set(file("spacetimedb"))
}

dependencies {
    implementation("com.clockworklabs:spacetimedb-sdk")
}
```

In `settings.gradle.kts`, add the plugin repository:

```kotlin
pluginManagement {
    repositories {
        gradlePluginPortal()
        mavenCentral()
    }
}
```

The SDK requires JDK 21+ and uses [Ktor](https://ktor.io/) for WebSocket transport. Add a Ktor engine dependency for your platform:

```kotlin
// JVM / Android
implementation("io.ktor:ktor-client-okhttp:3.4.1")

// iOS / Native
implementation("io.ktor:ktor-client-darwin:3.4.1")
```

```kotlin
// All platforms need the WebSockets plugin
implementation("io.ktor:ktor-client-websockets:3.4.1")
```

## Generate module bindings

The SpacetimeDB Gradle plugin automatically generates Kotlin bindings when you compile. Bindings are generated into `build/generated/spacetimedb/bindings/` and wired into the Kotlin compilation automatically.

Generated files include:

| File | Description |
| ---- | ----------- |
| `Types.kt` | All user-defined types (`data class`, `sealed interface`, `enum class`) |
| `{Table}TableHandle.kt` | Table handle with field name constants, cache accessors, and callbacks |
| `{Reducer}Reducer.kt` | Reducer args `data class` and name constant |
| `RemoteTables.kt` | Aggregates all table accessors |
| `RemoteReducers.kt` | Reducer call stubs with one-shot callbacks |
| `RemoteProcedures.kt` | Procedure call methods and callback registration |
| `Module.kt` | Module descriptor, `QueryBuilder`, and `subscribeToAllTables` extension |

You can also generate bindings manually:

```bash
spacetime generate --lang kotlin --out-dir src/main/kotlin/module_bindings --module-path spacetimedb
```

## Type `DbConnection`

A `DbConnection` represents a live WebSocket connection to a SpacetimeDB database. Create one using the builder:

```kotlin
val httpClient = HttpClient(OkHttp) { install(WebSockets) }

val conn = DbConnection.Builder()
    .withHttpClient(httpClient)
    .withUri("ws://localhost:3000")
    .withDatabaseName("my-database")
    .withModuleBindings()
    .onConnect { conn, identity, token ->
        // Connected — register callbacks, subscribe, call reducers
    }
    .onDisconnect { conn, error ->
        // Disconnected — error is null for clean disconnects
    }
    .onConnectError { conn, error ->
        // Connection failed
    }
    .build()
```

### Builder methods

| Method | Description |
| ------ | ----------- |
| `withHttpClient(client)` | Ktor `HttpClient` with WebSockets installed |
| `withUri(uri)` | WebSocket URL (e.g. `ws://localhost:3000`) |
| `withDatabaseName(name)` | Database name or address |
| `withToken(token)` | Auth token (nullable, for reconnecting with saved identity) |
| `withModuleBindings()` | Generated extension that registers the module descriptor |
| `onConnect(cb)` | Called after successful connection with `(DbConnectionView, Identity, String)` |
| `onDisconnect(cb)` | Called on disconnect with `(DbConnectionView, Throwable?)` |
| `onConnectError(cb)` | Called on connection failure with `(DbConnectionView, Throwable)` |
| `build()` | Suspending — connects and returns the `DbConnection` |

### Using `use` for automatic cleanup

The SDK provides a `use` extension that keeps the connection alive and disconnects when the block completes:

```kotlin
conn.use {
    delay(Duration.INFINITE) // Keep alive until cancelled
}
```

### Accessing generated modules

Inside callbacks, the connection exposes generated accessors:

```kotlin
conn.db.person       // Table handle for the "person" table
conn.reducers.add()  // Call the "add" reducer
```

These are generated extension properties — `db`, `reducers`, and `procedures`.

## Type `EventContext`

Callbacks receive an `EventContext` that provides access to the database and metadata about the event:

```kotlin
conn.db.person.onInsert { ctx, person ->
    // ctx.db, ctx.reducers, ctx.procedures are available
    // ctx is an EventContext
}
```

Reducer callbacks receive an `EventContext.Reducer<A>` with additional fields:

```kotlin
conn.reducers.onAdd { ctx, name ->
    ctx.status         // Status (Committed, Failed)
    ctx.callerIdentity // Identity of the caller
}
```

## Access the client cache

Each table handle provides methods to read cached rows and register callbacks.

### Read rows

```kotlin
conn.db.person.count()              // Number of cached rows
conn.db.person.all()                // List<Person> of all cached rows
conn.db.person.iter()               // Sequence<Person> for lazy iteration
```

### Row callbacks

```kotlin
// Called when a row is inserted
conn.db.person.onInsert { ctx, person ->
    println("Inserted: ${person.name}")
}

// Called when a row is deleted
conn.db.person.onDelete { ctx, person ->
    println("Deleted: ${person.name}")
}

// Called when a row is updated (tables with primary keys only)
conn.db.person.onUpdate { ctx, oldPerson, newPerson ->
    println("Updated: ${oldPerson.name} -> ${newPerson.name}")
}

// Called before a row is deleted (for pre-delete logic)
conn.db.person.onBeforeDelete { ctx, person ->
    println("About to delete: ${person.name}")
}
```

Remove callbacks by passing the same function reference:

```kotlin
val cb: (EventContext, Person) -> Unit = { _, p -> println(p.name) }
conn.db.person.onInsert(cb)
conn.db.person.removeOnInsert(cb)
```

### Index lookups

For tables with unique indexes:

```kotlin
conn.db.person.id.find(42u)         // Person? — lookup by unique index
```

For tables with BTree indexes:

```kotlin
conn.db.person.nameIdx.filter("Alice")  // Set<Person> — filter by index
```

## Observe and invoke reducers

### Call a reducer

```kotlin
conn.reducers.add("Alice")
```

### Call with a one-shot callback

```kotlin
conn.reducers.add("Alice") { ctx ->
    println("Add completed: status=${ctx.status}")
}
```

The one-shot callback fires only for this specific call.

### Observe all calls to a reducer

```kotlin
conn.reducers.onAdd { ctx, name ->
    println("Someone called add($name), status=${ctx.status}")
}
```

## Subscribe to queries

### Subscribe to all tables

```kotlin
conn.subscriptionBuilder()
    .onError { _, error -> println("Subscription error: $error") }
    .subscribeToAllTables()
```

### Type-safe query builder

Use the generated `QueryBuilder` for type-safe subscriptions:

```kotlin
conn.subscriptionBuilder()
    .addQuery { qb -> qb.person().where { cols -> cols.name.eq("Alice") } }
    .onApplied { println("Subscription applied") }
    .subscribe()
```

The query builder supports:

| Method | Description |
| ------ | ----------- |
| `where { cols -> expr }` | Filter rows by column predicates |
| `leftSemijoin(other) { l, r -> expr }` | Keep left rows that match right |
| `rightSemijoin(other) { l, r -> expr }` | Keep right rows that match left |

Column predicates: `eq`, `neq`, `lt`, `lte`, `gt`, `gte`, combined with `and` / `or`.

## Identify a client

### `Identity`

A unique identifier for a user, consistent across connections. Represented as a 32-byte value.

```kotlin
val hex = identity.toHexString()
```

### `ConnectionId`

Identifies a specific connection (a user can have multiple).

## Type mappings

| SpacetimeDB Type | Kotlin Type |
| ---------------- | ----------- |
| `bool` | `Boolean` |
| `u8` | `UByte` |
| `u16` | `UShort` |
| `u32` | `UInt` |
| `u64` | `ULong` |
| `u128` | `UInt128` |
| `u256` | `UInt256` |
| `i8` | `Byte` |
| `i16` | `Short` |
| `i32` | `Int` |
| `i64` | `Long` |
| `i128` | `Int128` |
| `i256` | `Int256` |
| `f32` | `Float` |
| `f64` | `Double` |
| `String` | `String` |
| `Vec<u8>` / `bytes` | `ByteArray` |
| `Vec<T>` / `Array<T>` | `List<T>` |
| `Option<T>` | `T?` |
| `Identity` | `Identity` |
| `ConnectionId` | `ConnectionId` |
| `Timestamp` | `Timestamp` |
| `TimeDuration` | `TimeDuration` |
| `ScheduleAt` | `ScheduleAt` |
| `Uuid` | `SpacetimeUuid` |
| `Result<Ok, Err>` | `SpacetimeResult<Ok, Err>` |
| Product types | `data class` |
| Sum types (all unit) | `enum class` |
| Sum types (mixed) | `sealed interface` |
