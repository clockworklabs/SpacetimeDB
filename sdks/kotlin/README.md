# SpacetimeDB Kotlin SDK

Kotlin Multiplatform client SDK for [SpacetimeDB](https://spacetimedb.com). Connects to a SpacetimeDB module over WebSocket, synchronizes table state into an in-memory client cache, and provides typed access to tables, reducers, and procedures via generated bindings.

## Supported Platforms

| Platform | Minimum Version |
|----------|----------------|
| JVM      | 21             |
| Android  | API 26         |
| iOS      | arm64 / x64 / simulator-arm64 |

The SDK uses [Ktor](https://ktor.io/) for WebSocket transport. You must provide an `HttpClient` with a platform-appropriate engine (e.g. OkHttp for JVM/Android, Darwin for iOS) and the WebSockets plugin installed.

## Installation

### Gradle Plugin (recommended)

Apply the plugin to your module's `build.gradle.kts`:

```kotlin
plugins {
    id("com.clockworklabs.spacetimedb")
}

spacetimedb {
    // Path to spacetimedb-cli binary (defaults to "spacetimedb-cli" on PATH)
    cli.set(file("/path/to/spacetimedb-cli"))
    // Path to your SpacetimeDB module directory (defaults to "spacetimedb/")
    modulePath.set(file("spacetimedb/"))
}
```

The plugin registers a `generateSpacetimeBindings` task that runs `spacetimedb-cli generate --lang kotlin` and wires the output into Kotlin compilation automatically.

### Manual Setup

Add the SDK dependency and generate bindings with the CLI:

```kotlin
// build.gradle.kts
dependencies {
    implementation("com.clockworklabs:spacetimedb-kotlin-sdk:0.1.0")
}
```

```bash
spacetimedb-cli generate \
    --lang kotlin \
    --out-dir src/main/kotlin/module_bindings/ \
    --module-path path/to/your/spacetimedb/module
```

## Quick Start

```kotlin
import io.ktor.client.HttpClient
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.plugins.websocket.WebSockets
import module_bindings.*

suspend fun main() {
    val httpClient = HttpClient(OkHttp) { install(WebSockets) }

    val conn = DbConnection.Builder()
        .withHttpClient(httpClient)
        .withUri("ws://localhost:3000")
        .withDatabaseName("my_module")
        .withModuleBindings()
        .onConnect { conn, identity, token ->
            println("Connected as $identity")

            // Subscribe to tables
            conn.subscriptionBuilder()
                .addQuery { qb -> qb.person() }
                .subscribe()
        }
        .onDisconnect { _, reason ->
            println("Disconnected: $reason")
        }
        .build()

    // Register table callbacks
    conn.db.person.onInsert { ctx, person ->
        println("New person: ${person.name}")
    }

    // Call reducers
    conn.reducers.add("Alice")

    // Register reducer callbacks
    conn.reducers.onAdd { ctx, name ->
        println("add reducer called with: $name (status: ${ctx.status})")
    }
}
```

## Generated Bindings

Running codegen produces the following files:

| File | Contents |
|------|----------|
| `Types.kt` | Data classes for all user-defined types |
| `*TableHandle.kt` | Table handle with callbacks, queries, and column metadata |
| `*Reducer.kt` | Reducer args data class and name constant |
| `RemoteTables.kt` | Aggregates all table accessors |
| `RemoteReducers.kt` | Reducer call stubs and per-reducer callbacks |
| `RemoteProcedures.kt` | Procedure call stubs |
| `Module.kt` | Module metadata, extension properties (`conn.db`, `conn.reducers`, `conn.procedures`), query builder |

Extension properties are generated on both `DbConnection` and `EventContext`, so you can access `ctx.db.person` directly inside callbacks.

## Connection Lifecycle

### Builder Options

```kotlin
DbConnection.Builder()
    .withHttpClient(httpClient)              // Ktor HttpClient with WebSockets (required)
    .withUri("ws://localhost:3000")          // WebSocket URI (required)
    .withDatabaseName("my_module")           // Module name or address (required)
    .withModuleBindings()                    // Register generated module (required)
    .withToken(savedToken)                   // Auth token for identity reuse
    .withCompression(CompressionMode.GZIP)   // Enable GZIP compression
    .withLightMode(true)                     // Light mode (reduced server-side state)
    .withCallbackDispatcher(Dispatchers.Main)// Dispatch callbacks on a specific dispatcher
    .onConnect { conn, identity, token -> }  // Fires once on successful connection
    .onDisconnect { conn, reason -> }        // Fires on disconnect
    .onConnectError { conn, error -> }       // Fires if connection fails
    .build()                                 // Returns connected DbConnection
```

### States

A `DbConnection` transitions through these states:

```
DISCONNECTED → CONNECTING → CONNECTED → CLOSED
```

Once `CLOSED`, the connection cannot be reused. Create a new `DbConnection` to reconnect.

### Reconnection

The SDK does not reconnect automatically. Implement retry logic at the application level:

```kotlin
suspend fun connectWithRetry(httpClient: HttpClient, maxAttempts: Int = 5): DbConnection {
    repeat(maxAttempts) { attempt ->
        try {
            return DbConnection.Builder()
                .withHttpClient(httpClient)
                .withUri("ws://localhost:3000")
                .withDatabaseName("my_module")
                .withModuleBindings()
                .build()
        } catch (e: Exception) {
            if (attempt == maxAttempts - 1) throw e
            delay(1000L * (attempt + 1)) // linear backoff
        }
    }
    error("unreachable")
}
```

## Subscriptions

### SQL-string subscriptions

```kotlin
// Subscribe to all rows
conn.subscribe("SELECT person.* FROM person")

// Multiple queries
conn.subscribe(
    "SELECT person.* FROM person",
    "SELECT item.* FROM item",
)
```

### Type-safe query builder

```kotlin
conn.subscriptionBuilder()
    .addQuery { qb -> qb.person() }                                    // all rows
    .addQuery { qb -> qb.person().where { c -> c.name.eq("Alice") } }  // filtered
    .onApplied { ctx -> println("Subscription applied") }
    .onError { ctx, err -> println("Subscription error: $err") }
    .subscribe()
```

## Table Callbacks

```kotlin
// Fires for each inserted row
conn.db.person.onInsert { ctx, person -> }

// Fires for each deleted row (persistent tables only)
conn.db.person.onDelete { ctx, person -> }

// Fires before delete (useful for cleanup/animation triggers)
conn.db.person.onBeforeDelete { ctx, person -> }
```

Remove callbacks by passing the same function reference to the corresponding `removeOn*` method.

## Reading Table Data

```kotlin
// All cached rows
val people: List<Person> = conn.db.person.all()

// Row count
val count: Int = conn.db.person.count()

// Lazy iteration
conn.db.person.iter().forEach { person -> println(person.name) }
```

## One-Off Queries

Execute a query outside of subscriptions:

```kotlin
// Callback-based
conn.oneOffQuery("SELECT person.* FROM person") { result -> }

// Suspend (with optional timeout)
val result = conn.oneOffQuery("SELECT person.* FROM person", timeout = 5.seconds)

```

## Thread Safety

The SDK is safe to use from any thread/coroutine:

- **Client cache**: All row storage uses atomic references over persistent immutable collections (`kotlinx.collections.immutable`). No locks are needed — each reader gets a consistent snapshot via atomic reference reads.
- **Callback lists**: Stored as atomic `PersistentList` references. Adding/removing callbacks and iterating over them are lock-free operations.
- **Connection state**: Managed via atomic compare-and-swap, preventing double-connect or double-disconnect races.

### Callback Dispatcher

By default, callbacks execute on the WebSocket receive coroutine. To dispatch callbacks on a specific thread (e.g., the main/UI thread):

```kotlin
DbConnection.Builder()
    .withHttpClient(httpClient)
    .withCallbackDispatcher(Dispatchers.Main)
    // ...
    .build()
```

This applies to all table, reducer, subscription, and connection callbacks.

## Dependencies

| Library | Version | Purpose |
|---------|---------|---------|
| Ktor Client | 3.4.1 | WebSocket transport |
| kotlinx-coroutines | 1.10.2 | Async runtime |
| kotlinx-atomicfu | 0.31.0 | Lock-free atomics |
| kotlinx-collections-immutable | 0.4.0 | Persistent data structures |
