# Kotlin SDK — Developer Guide

Internal documentation for contributors working on the SpacetimeDB Kotlin SDK.

## Project Structure

```
src/
  commonMain/     Shared Kotlin code (all targets)
    com/clockworklabs/spacetimedb/
      SpacetimeDBClient.kt    DbConnection, DbConnectionBuilder, ProcedureResult
      Identity.kt             Identity, ConnectionId, Address, Timestamp, TimeDuration
      ClientCache.kt          Client-side row cache (TableCache, ByteArrayWrapper)
      TableHandle.kt          Untyped per-table callback registration
      Table.kt                Typed Table/TableWithPrimaryKey/EventTable interfaces
      SubscriptionHandle.kt   Subscription lifecycle (PENDING/ACTIVE/ENDED/CANCELLED)
      SubscriptionBuilder.kt  Fluent subscription API with pending query accumulation
      Event.kt                Event sealed class, ReducerEvent, Credentials
      DbContext.kt            Interface for connection context
      EventContext.kt         Event context with event metadata
      ReducerEventContext.kt  Reducer-specific event context
      ProcedureEventContext.kt Procedure event context
      SubscriptionEventContext.kt Subscription event context
      ErrorContext.kt         Error context for disconnect/error callbacks
      ReconnectPolicy.kt      Exponential backoff configuration
      Compression.kt          expect declarations for decompression
      ScheduleAt.kt           ScheduleAt type (Interval/Time)
      Uuid.kt                 UUID type with BSATN read/write + toByteArray/fromByteArray
      bsatn/
        BsatnReader.kt        Binary deserialization (little-endian, SATS-compatible)
        BsatnWriter.kt        Binary serialization (little-endian, SATS-compatible)
        BsatnRowList.kt       Row list decoding (FixedSize/RowOffsets)
      protocol/
        ServerMessage.kt      Server → Client message decoding
        ClientMessage.kt      Client → Server message encoding
        ProtocolTypes.kt      QuerySetId, QueryRows, TableUpdateRows, etc.
      query/
        QueryBuilder.kt       QueryTable, Col, BoolExpr, Cols, FromQuery, QueryProvider, QueryFrom
      websocket/
        WebSocketTransport.kt WebSocket lifecycle, token exchange, keep-alive, reconnection
  jvmMain/        JVM-specific (Gzip via java.util.zip, Brotli via org.brotli)
  iosMain/        iOS-specific (Gzip via platform.zlib)
  commonTest/     Shared tests
  jvmTest/        JVM-only tests (compression round-trips, live integration)
```

## Architecture

### Connection Lifecycle

```
DbConnectionBuilder.build()
  → DbConnection constructor
    → WebSocketTransport.connect()
      → exchangeToken()          POST /v1/identity/websocket-token (if token provided)
      → connectSession() opens WebSocket
        → processSendQueue()   (coroutine: outbound messages)
        → processIncoming()    (coroutine: inbound frames)
        → runKeepAlive()       (coroutine: 30s ping heartbeats)
```

On unexpected disconnect with a `ReconnectPolicy`, the transport enters a
`RECONNECTING` state and calls `attemptReconnect()` which retries with
exponential backoff up to `maxRetries` times.

### Wire Protocol

Uses the `v2.bsatn.spacetimedb` WebSocket subprotocol. All messages are BSATN
(Binary SpacetimeDB Algebraic Type Notation) — a tag-length-value encoding
defined in `crates/client-api-messages/src/websocket/v2.rs`.

**Server messages** are preceded by a compression byte:
- `0x00` — uncompressed
- `0x01` — Brotli
- `0x02` — Gzip

The SDK requests Gzip compression via the `compression=Gzip` query parameter.

**Key protocol details:**
- All integers are little-endian
- Sum types use u8 tag: `Option<T>` uses tag `0` = Some, tag `1` = None
- UUID is encoded as two little-endian i64 values (MSB first, LSB second)
- Connection URL includes `connection_id`, `compression`, optional `token`, `confirmed`, `light` params

### Client Cache

`ClientCache` maintains a map of `TableCache` instances, one per table. Each
`TableCache` stores rows keyed by content (`ByteArrayWrapper`) with reference
counting. This allows overlapping subscriptions to share rows without duplicates.

Transaction updates produce `TableOperation` events (Insert, Delete, Update,
EventInsert) which drive the `TableHandle` callback system.

**New: Initial subscription data (SubscribeApplied) now fires `onInsert` callbacks**
for each row, matching Rust/TypeScript SDK behavior. Previously initial rows
were inserted silently.

### Query Builder

The SDK includes a type-safe query builder DSL matching the Rust SDK pattern:

```kotlin
conn.subscriptionBuilder()
    .addQuery { users }                                    // SELECT * FROM "users"
    .addQuery { users.where { cols -> cols.age.gt(18) } }  // SELECT * FROM "users" WHERE ...
    .subscribe()
```

Each generated table has a `{Table}Cols` class with typed `Col<V>` column accessors
supporting `.eq()`, `.ne()`, `.gt()`, `.lt()`, `.gte()`, `.lte()`, plus `.and()` / `.or()`
combinators on `BoolExpr`.

### Event System

The SDK provides typed event contexts matching Rust SDK patterns:

| Context | Event | When |
|---------|-------|------|
| `EventContext<R>` | `Event<R>` | Row callbacks (insert/delete/update) |
| `ReducerEventContext` | `ReducerEvent` | Reducer completion callbacks |
| `ProcedureEventContext` | — | Procedure completion callbacks |
| `SubscriptionEventContext` | — | Subscription applied/ended |
| `ErrorContext` | `Throwable?` | Connection errors |

### Threading Model

- `WebSocketTransport` runs on a `CoroutineScope(SupervisorJob() + Dispatchers.Default)`.
- All `handleMessage` processing is serialized behind a `Mutex` to prevent
  concurrent cache mutation.
- `atomicfu` atomics are used for transport-level flags (`idle`, `wantPong`,
  `intentionalDisconnect`) that are read/written across coroutines.
- User callbacks are wrapped in try-catch to prevent exceptions from crashing
  the message processing loop.

### Platform-Specific Code

Uses Kotlin `expect`/`actual` for decompression:

| Platform | Gzip | Brotli |
|----------|------|--------|
| JVM | `java.util.zip.GZIPInputStream` | `org.brotli.dec.BrotliInputStream` |
| iOS | `platform.zlib` (wbits=31) | Not supported (SDK defaults to Gzip) |

## Code Generation

The Kotlin codegen backend lives at `crates/codegen/src/kotlin.rs`. It implements
the `Lang` trait and generates:

- **Type files:** `data class` for products, `sealed class` for sums, `enum class` for plain enums
  — each with BSATN `read`/`write` companion methods
- **Table files:** Typed handle classes implementing `TableWithPrimaryKey<TRow>` or `EventTable<TRow>`
  with `iter()`, `find()`, `onInsert`/`onDelete`/`onUpdate` callbacks
- **Reducer files:** Args data class + `internal fun {name}Reducer(conn, args)`
- **Procedure files:** Args data class + `internal fun {name}Procedure(conn, args, callback)`
- **Global files:** `RemoteTables`, `RemoteReducers`, `RemoteProcedures`, `From` (typed query builder)

Generate bindings:

```bash
spacetime generate --lang kotlin --out-dir src/commonMain/kotlin --namespace my.package
```

## Building & Testing

```bash
# Build
./gradlew compileKotlinJvm

# Run unit tests
./gradlew jvmTest

# Run live integration tests (requires running server + published module)
SPACETIMEDB_TEST=1 SPACETIMEDB_URI=ws://127.0.0.1:3000 SPACETIMEDB_MODULE=kotlin-sdk-test \
    ./gradlew jvmTest --rerun-tasks

# Publish to local Maven
./gradlew publishToMavenLocal
```

## Test Suite

| File | Coverage |
|------|----------|
| `BsatnTest.kt` | Reader/Writer round-trips for all primitive types |
| `ProtocolTest.kt` | ServerMessage and ClientMessage encode/decode |
| `ClientCacheTest.kt` | Cache operations, ref counting, transaction updates |
| `OneOffQueryTest.kt` | OneOffQueryResult decode (Ok and Err variants) |
| `CompressionTest.kt` | Gzip round-trip, empty/large payloads (JVM only) |
| `ReconnectPolicyTest.kt` | Backoff calculation, parameter validation |
| `EdgeCaseTest.kt` | Option encoding, subscription lifecycle, callback safety, URI normalization |
| `LiveIntegrationTest.kt` | Connect, subscribe, reducer call, one-off query (requires server) |
| `LiveEdgeCaseTest.kt` | Multi-subscription, reconnect, invalid queries (requires server) |
| `PerformanceBenchmarkTest.kt` | Keynote-2 workload benchmark (JVM only) |

## Design Decisions

1. **Option encoding** — SATS encodes `Option<T>` with tag `0` = Some, tag `1` = None.
   This differs from what a naive u8 sum type implementation might assume (0 = None).

2. **UUID byte order** — MSB first (two little-endian i64 values). This matches the
   `AlgebraicType::uuid()` wire format used by the SpacetimeDB server.

3. **Ktor built-in ping interval** — The SDK configures Ktor's `WebSockets` plugin
   with `pingInterval = 30.seconds` to send periodic WebSocket pings, keeping the
   connection alive. No custom ping/pong coroutine is needed. The Rust SDK's
   custom keep-alive logic is not replicated since Ktor handles this at the
   transport layer.

4. **Token exchange** — The SDK POSTs to `/v1/identity/websocket-token` to exchange
   the long-lived auth token for a short-lived WebSocket token, matching the TypeScript
   SDK authentication flow.

5. **ByteArray row storage** — Rows are stored as raw BSATN bytes in the cache.
   Typed access is provided via generated code that wraps the raw bytes with
   type-specific `read()`/`write()` companions.

6. **Compression negotiation** — The SDK advertises `compression=Gzip` in the
   connection URI. Brotli is supported on JVM but not iOS; Gzip provides
   universal coverage.

7. **Callback safety** — All user callbacks are wrapped in try-catch to prevent
   exceptions from crashing the message processing coroutine. This matches the
   Rust SDK's approach of deferring callback execution.

8. **Subscription lifecycle** — Supports `PENDING`, `ACTIVE`, `ENDED`, and `CANCELLED`
   states. Unsubscribing before a subscription is applied (`PENDING -> CANCELLED`)
   prevents it from being registered when the server responds.
