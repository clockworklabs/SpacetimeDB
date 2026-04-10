# Kotlin SDK — Developer Guide

Internal documentation for contributors working on the SpacetimeDB Kotlin SDK.

## Project Structure

```
src/
  commonMain/     Shared Kotlin code (all targets)
    com/clockworklabs/spacetimedb/
      SpacetimeDBClient.kt    DbConnection, DbConnectionBuilder
      Identity.kt             Identity, ConnectionId, Address, Timestamp
      ClientCache.kt          Client-side row cache (TableCache, ByteArrayWrapper)
      TableHandle.kt          Per-table callback registration
      SubscriptionHandle.kt   Subscription lifecycle
      SubscriptionBuilder.kt  Fluent subscription API
      ReconnectPolicy.kt      Exponential backoff configuration
      Compression.kt          expect declarations for decompression
      bsatn/
        BsatnReader.kt        Binary deserialization
        BsatnWriter.kt        Binary serialization
        BsatnRowList.kt       Row list decoding
      protocol/
        ServerMessage.kt      Server → Client message decoding
        ClientMessage.kt      Client → Server message encoding
        ProtocolTypes.kt      QuerySetId, QueryRows, TableUpdateRows, etc.
      websocket/
        WebSocketTransport.kt WebSocket lifecycle, ping/pong, reconnection
  jvmMain/        JVM-specific (Gzip via java.util.zip, Brotli via org.brotli)
  iosMain/        iOS-specific (Gzip via platform.zlib)
  commonTest/     Shared tests
  jvmTest/        JVM-only tests (compression round-trips)
```

## Architecture

### Connection Lifecycle

```
DbConnectionBuilder.build()
  → DbConnection constructor
    → WebSocketTransport.connect()
      → connectSession() opens WebSocket
        → processSendQueue()   (coroutine: outbound messages)
        → processIncoming()    (coroutine: inbound frames)
        → runKeepAlive()       (coroutine: 30s idle ping/pong)
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

### Client Cache

`ClientCache` maintains a map of `TableCache` instances, one per table. Each
`TableCache` stores rows keyed by content (`ByteArrayWrapper`) with reference
counting. This allows overlapping subscriptions to share rows without duplicates.

Transaction updates produce `TableOperation` events (Insert, Delete, Update,
EventInsert) which drive the `TableHandle` callback system.

### Threading Model

- `WebSocketTransport` runs on a `CoroutineScope(SupervisorJob() + Dispatchers.Default)`.
- All `handleMessage` processing is serialized behind a `Mutex` to prevent
  concurrent cache mutation.
- `atomicfu` atomics are used for transport-level flags (`idle`, `wantPong`,
  `intentionalDisconnect`) that are read/written across coroutines.

### Platform-Specific Code

Uses Kotlin `expect`/`actual` for decompression:

| Platform | Gzip | Brotli |
|----------|------|--------|
| JVM | `java.util.zip.GZIPInputStream` | `org.brotli.dec.BrotliInputStream` |
| iOS | `platform.zlib` (wbits=31) | Not supported (SDK defaults to Gzip) |

## Building

```bash
# Run all JVM tests
./gradlew jvmTest

# Compile JVM
./gradlew compileKotlinJvm

# Compile iOS (verifies expect/actual)
./gradlew compileKotlinIosArm64

# All targets
./gradlew build
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

## Design Decisions

1. **Manual ping/pong** instead of Ktor's `pingIntervalMillis` — OkHttp engine
   doesn't support Ktor's built-in ping, so we implement idle detection
   ourselves (matching the Rust SDK's 30s pattern).

2. **ByteArray row storage** — Rows are stored as raw BSATN bytes rather than
   deserialized objects. This keeps the core SDK schema-agnostic; code
   generation (future) will layer typed access on top.

3. **Compression negotiation** — The SDK advertises `compression=Gzip` in the
   connection URI. Brotli is supported on JVM but not iOS; Gzip provides
   universal coverage.

4. **No Brotli on iOS** — Apple's Compression framework supports Brotli
   (`COMPRESSION_BROTLI`) but it's not directly available via Kotlin/Native's
   `platform.compression` interop. Since the SDK requests Gzip, this is a
   non-issue in practice.
