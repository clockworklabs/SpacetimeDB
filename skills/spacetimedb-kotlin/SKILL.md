---
name: spacetimedb-kotlin
description: Build Kotlin Multiplatform clients for SpacetimeDB. Covers KMP SDK integration for Android, JVM Desktop, and iOS/Native.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.1"
  tested_with: "SpacetimeDB 2.1, JDK 21+, Kotlin 2.1"
---

# SpacetimeDB Kotlin SDK

Build real-time Kotlin Multiplatform clients that connect directly to SpacetimeDB modules. The SDK provides type-safe database access, automatic synchronization, and reactive updates for Android, JVM Desktop, and iOS/Native apps.

The server module is written in Rust (or C#/TypeScript). Kotlin is a **client-only** SDK — there is no `crates/bindings-kotlin` for server-side modules.

---

## HALLUCINATED APIs — DO NOT USE

**These APIs DO NOT EXIST. LLMs frequently hallucinate them.**

```kotlin
// WRONG — these builder methods do not exist
DbConnection.Builder().withHost("localhost")       // Use withUri("ws://localhost:3000")
DbConnection.Builder().withDatabase("my-db")       // Use withDatabaseName("my-db")
DbConnection.Builder().withModule(Module)           // Use withModuleBindings() (generated extension)
DbConnection.Builder().connect()                    // Use build() (suspending)

// WRONG — blocking build
val conn = DbConnection.Builder().build()           // build() is suspend — must be in coroutine

// WRONG — table access patterns
conn.db.Person                                      // Wrong casing — use generated accessor name
conn.tables.person                                  // No .tables — use conn.db.person
conn.db.person.get(id)                              // No .get() — use index: conn.db.person.id.find(id)
conn.db.person.findById(id)                         // No .findById() — use conn.db.person.id.find(id)
conn.db.person.query("SELECT ...")                  // No SQL on client — use subscriptions + query builder

// WRONG — callback signatures
conn.db.person.onInsert { person -> }               // Missing EventContext: { ctx, person -> }
conn.db.person.onUpdate { old, new -> }             // Missing EventContext: { ctx, old, new -> }
conn.db.person.onInsert(::handleInsert)             // OK — function references work if signature matches (EventContext, Person) -> Unit

// WRONG — subscription patterns
conn.subscribe("SELECT * FROM person")              // No direct subscribe — use subscriptionBuilder()
conn.subscriptionBuilder().subscribe("SELECT ...")   // Works, but prefer typed query builder for compile-time safety

// WRONG — reducer call patterns
conn.call("add", "Alice")                           // No generic call — use conn.reducers.add("Alice")
conn.reducers.add("Alice").await()                  // Reducers don't return futures — use one-shot callback

// WRONG — non-existent types
import spacetimedb.Identity                         // Wrong package — use com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import spacetimedb.DbConnection                     // Wrong — use com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.DbConnection

// WRONG — generating bindings to src/
// The Gradle plugin generates to build/generated/spacetimedb/bindings/, NOT src/main/kotlin/module_bindings/
```

### CORRECT PATTERNS

```kotlin
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.DbConnection
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.use
import io.ktor.client.HttpClient
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.plugins.websocket.WebSockets
import module_bindings.*

suspend fun main() {
    val httpClient = HttpClient(OkHttp) { install(WebSockets) }

    DbConnection.Builder()
        .withHttpClient(httpClient)
        .withUri("ws://localhost:3000")
        .withDatabaseName(SpacetimeConfig.DATABASE_NAME)
        .withModuleBindings()                        // Generated extension — registers module descriptor
        .onConnect { conn, identity, token ->
            conn.db.person.onInsert { ctx, person ->
                println("Inserted: ${person.name}")
            }

            conn.subscriptionBuilder()
                .subscribeToAllTables()

            conn.reducers.add("Alice") { ctx ->
                println("status=${ctx.status}")
            }
        }
        .onDisconnect { _, error ->
            println("Disconnected: ${error?.message ?: "clean"}")
        }
        .build()
        .use { delay(Duration.INFINITE) }
}
```

---

## Common Mistakes Table

| Wrong | Right | Why |
|-------|-------|-----|
| `DbConnection.Builder().build()` outside coroutine | Wrap in `runBlocking` or `launch` | `build()` is `suspend` |
| Forgetting `install(WebSockets)` on HttpClient | `HttpClient(OkHttp) { install(WebSockets) }` | SDK needs WebSocket support |
| Using `withModuleDescriptor(Module)` | Use `withModuleBindings()` | Generated extension handles registration |
| Callbacks without EventContext | `{ ctx, row -> }` not `{ row -> }` | All callbacks receive EventContext first |
| `onUpdate` on table without primary key | Only available on `RemotePersistentTableWithPrimaryKey` | Need `#[primary_key]` on server table |
| Calling `conn.db` from wrong thread | SDK is coroutine-safe via atomic state | Use from any coroutine scope |
| Generating bindings to `src/` | Gradle plugin generates to `build/generated/spacetimedb/bindings/` | Bindings are build artifacts, not source |
| Using `includeBuild` without local SDK checkout | Required until SDK is published on Maven Central | Templates have placeholder comments |

---

## Hard Requirements

1. **JDK 21+** — required by the SDK and Gradle plugin
2. **Ktor HttpClient with WebSockets** — must `install(WebSockets)` on the client
3. **`build()` is suspending** — must be called from a coroutine
4. **`withModuleBindings()`** — generated extension, call on builder to register module
5. **`SpacetimeConfig.DATABASE_NAME`** — generated constant, use for database name
6. **Callbacks always receive `EventContext` as first param** — `{ ctx, row -> }`
7. **`onUpdate` requires primary key** — only on `RemotePersistentTableWithPrimaryKey`
8. **Gradle plugin auto-generates bindings** — no manual `spacetime generate` needed when using the plugin
9. **Server module is Rust** — templates use Rust server modules, not Kotlin

---

## Client SDK API

### DbConnection.Builder

```kotlin
val conn = DbConnection.Builder()
    .withHttpClient(httpClient)              // Required: Ktor HttpClient
    .withUri("ws://localhost:3000")           // Required: WebSocket URL
    .withDatabaseName("my-database")         // Required: database name
    .withToken(savedToken)                   // Optional: auth token for reconnection
    .withModuleBindings()                    // Required: generated extension
    .onConnect { conn, identity, token -> }  // Connected callback
    .onDisconnect { conn, error -> }         // Disconnected callback
    .onConnectError { conn, error -> }       // Connection failed callback
    .build()                                 // Suspending — returns DbConnection
```

### Connection Lifecycle

```kotlin
// Keep alive with automatic cleanup
conn.use {
    delay(Duration.INFINITE)
}

// Manual disconnect
conn.disconnect()
```

### Table Access (Client Cache)

```kotlin
// Read cached rows
conn.db.person.count()                       // Int
conn.db.person.all()                         // List<Person>
conn.db.person.iter()                        // Sequence<Person>

// Index lookups (generated per-table)
conn.db.person.id.find(42u)                  // Person? — unique index
conn.db.person.nameIdx.filter("Alice")       // Set<Person> — BTree index
```

### Row Callbacks

```kotlin
conn.db.person.onInsert { ctx, person -> }
conn.db.person.onDelete { ctx, person -> }
conn.db.person.onUpdate { ctx, oldPerson, newPerson -> }    // PK tables only
conn.db.person.onBeforeDelete { ctx, person -> }

// Remove callback
val cb: (EventContext, Person) -> Unit = { ctx, p -> println(p) }
conn.db.person.onInsert(cb)
conn.db.person.removeOnInsert(cb)
```

### Reducers

```kotlin
// Call a reducer
conn.reducers.add("Alice")

// Call with one-shot callback
conn.reducers.add("Alice") { ctx ->
    println("status=${ctx.status}")
}

// Observe all calls to a reducer
conn.reducers.onAdd { ctx, name ->
    println("add($name) status=${ctx.status}")
}
```

### Subscriptions

```kotlin
// Subscribe to all tables
conn.subscriptionBuilder()
    .onError { _, error -> println(error) }
    .subscribeToAllTables()

// Type-safe query builder
conn.subscriptionBuilder()
    .addQuery { qb -> qb.person().where { cols -> cols.name.eq("Alice") } }
    .onApplied { println("Applied") }
    .subscribe()

// Query builder operations
qb.person()
    .where { cols -> cols.name.eq("Alice").and(cols.id.gt(0u)) }

qb.person()
    .leftSemijoin(qb.team()) { person, team ->
        person.teamId.eq(team.id)
    }
```

### Identity

```kotlin
identity.toHexString()                       // Hex string representation
```

---

## Type Mappings

| SpacetimeDB | Kotlin |
|-------------|--------|
| `bool` | `Boolean` |
| `u8`/`u16`/`u32`/`u64` | `UByte`/`UShort`/`UInt`/`ULong` |
| `i8`/`i16`/`i32`/`i64` | `Byte`/`Short`/`Int`/`Long` |
| `u128`/`u256` | `UInt128`/`UInt256` |
| `i128`/`i256` | `Int128`/`Int256` |
| `f32`/`f64` | `Float`/`Double` |
| `String` | `String` |
| `Vec<u8>` | `ByteArray` |
| `Vec<T>` | `List<T>` |
| `Option<T>` | `T?` |
| `Identity` | `Identity` |
| `ConnectionId` | `ConnectionId` |
| `Timestamp` | `Timestamp` |
| `TimeDuration` | `TimeDuration` |
| `ScheduleAt` | `ScheduleAt` |
| `Uuid` | `SpacetimeUuid` |
| Product types | `data class` |
| Sum types (all unit) | `enum class` |
| Sum types (mixed) | `sealed interface` |

---

## Project Structure

### basic-kt (JVM-only)

```
my-app/
├── spacetimedb/                 # Rust server module
│   ├── Cargo.toml
│   └── src/lib.rs
├── src/main/kotlin/
│   └── Main.kt                 # JVM client
├── build/generated/spacetimedb/
│   └── bindings/                # Auto-generated (by Gradle plugin)
├── build.gradle.kts
├── settings.gradle.kts
└── spacetime.json
```

### compose-kt (KMP: Android + Desktop)

```
my-app/
├── spacetimedb/                 # Rust server module
├── androidApp/                  # Android entry point (MainActivity)
├── desktopApp/                  # Desktop entry point (main.kt)
├── sharedClient/                # Shared KMP module (UI + SpacetimeDB client)
│   └── src/
│       ├── commonMain/kotlin/app/
│       │   ├── AppViewModel.kt
│       │   ├── ChatRepository.kt
│       │   └── composable/      # Compose UI screens
│       ├── androidMain/         # Android-specific (TokenStore)
│       └── jvmMain/             # Desktop-specific (TokenStore)
└── spacetime.json
```

---

## Commands

```bash
# Create project from template
spacetime init --template basic-kt --project-path ./my-app --non-interactive my-app

# Build and run (interactive — requires terminal)
spacetime dev

# Generate bindings manually (not needed with Gradle plugin)
spacetime generate --lang kotlin --out-dir src/main/kotlin/module_bindings --module-path spacetimedb

# Build Kotlin client
./gradlew compileKotlin

# Run Kotlin client
./gradlew run
```
