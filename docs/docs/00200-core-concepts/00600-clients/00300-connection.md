---
title: Connecting to SpacetimeDB
slug: /clients/connection
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

After [generating client bindings](./00200-codegen.md) for your module, you can establish a connection to your SpacetimeDB [database](../00100-databases.md) from your client application. The `DbConnection` type provides a persistent WebSocket connection that enables real-time communication with the server.

## Prerequisites

Before connecting, ensure you have:

1. [Generated client bindings](./00200-codegen.md) for your module
2. A published database running on SpacetimeDB (local or on [MainCloud](../../00250-how-to/00100-deploy/00100-maincloud.md))
3. The database's URI and name or identity

## Basic Connection

Create a connection using the `DbConnection` builder pattern:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection } from './module_bindings';

const conn = DbConnection.builder()
    .withUri("https://maincloud.spacetimedb.com")
    .withDatabaseName("my_database")
    .withAutomaticReconnect()
    .build();
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

var conn = DbConnection.Builder()
    .WithUri("https://maincloud.spacetimedb.com")
    .WithDatabaseName("my_database")
    .WithAutomaticReconnect()
    .Build();
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use module_bindings::DbConnection;

let conn = DbConnection::builder()
    .with_uri("https://maincloud.spacetimedb.com")
    .with_database_name("my_database")
    .build();
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
#include "ModuleBindings/DbConnection.h"

UDbConnection* Conn = UDbConnection::Builder()
    ->WithUri(TEXT("https://maincloud.spacetimedb.com"))
    ->WithDatabaseName(TEXT("my_database"))
    ->Build();
```

</TabItem>
</Tabs>

Replace `"https://maincloud.spacetimedb.com"` with your SpacetimeDB host URI, and `"my_database"` with your database's name or identity.

### Connecting to MainCloud

To connect to a database hosted on MainCloud:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const conn = DbConnection.builder()
    .withUri("https://maincloud.spacetimedb.com")
    .withDatabaseName("my_database")
    .withAutomaticReconnect()
    .build();
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
var conn = DbConnection.Builder()
    .WithUri("https://maincloud.spacetimedb.com")
    .WithDatabaseName("my_database")
    .WithAutomaticReconnect()
    .Build();
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let conn = DbConnection::builder()
    .with_uri("https://maincloud.spacetimedb.com")
    .with_database_name("my_database")
    .build();
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
UDbConnection* Conn = UDbConnection::Builder()
    ->WithUri(TEXT("https://maincloud.spacetimedb.com"))
    ->WithDatabaseName(TEXT("my_database"))
    ->Build();
```

</TabItem>
</Tabs>

## Authentication with Tokens

To authenticate with a token (for example, from [SpacetimeAuth](../00500-authentication/00100-spacetimeauth/index.md)), provide it when building the connection:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const conn = DbConnection.builder()
    .withUri("https://maincloud.spacetimedb.com")
    .withDatabaseName("my_database")
    .withToken("your_auth_token_here")
    .withAutomaticReconnect()
    .build();
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
var conn = DbConnection.Builder()
    .WithUri("https://maincloud.spacetimedb.com")
    .WithDatabaseName("my_database")
    .WithAutomaticReconnect()
    .WithToken("your_auth_token_here")
    .Build();
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let conn = DbConnection::builder()
    .with_uri("https://maincloud.spacetimedb.com")
    .with_database_name("my_database")
    .with_token("your_auth_token_here")
    .build();
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
UDbConnection* Conn = UDbConnection::Builder()
    ->WithUri(TEXT("https://maincloud.spacetimedb.com"))
    ->WithDatabaseName(TEXT("my_database"))
    ->WithToken(TEXT("your_auth_token_here"))
    ->Build();
```

</TabItem>
</Tabs>

The token is sent to the server during connection and validates your identity. See the [SpacetimeAuth documentation](../00500-authentication/00100-spacetimeauth/index.md) for details on obtaining and managing tokens.

## Advancing the Connection

:::danger[Critical: C#, Unity, and Unreal Users]

In C# (including Unity), you **must** manually advance the connection to process incoming messages. In Unreal Engine, you must either manually advance the connection or enable automatic ticking. If the connection is not advanced, it will not process messages.

Call `FrameTick()` in your game loop or update method:

<Tabs groupId="client-language" queryString>
<TabItem value="csharp" label="C#">

```csharp
// In Unity, call this in your Update() method
void Update()
{
    conn.FrameTick();
}

// Or in a console application, call this in your main loop
while (running)
{
    conn.FrameTick();
    // Your application logic...
}
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
// Option 1: call FrameTick() from your Actor's Tick() method
void AMyActor::Tick(float DeltaTime)
{
    Super::Tick(DeltaTime);

    if (Conn)
    {
        Conn->FrameTick();
    }
}

// Option 2: enable automatic ticking once after building the connection
Conn = Builder->Build();
Conn->SetAutoTicking(true);
```

</TabItem>
</Tabs>

Failure to advance the connection means your client will not receive any updates from the server, including subscription data, reducer callbacks, or connection events.

:::

In Rust and TypeScript, the connection processes messages automatically via the browser's event loop or Node.js's event loop. No manual polling is required.

## Connection Lifecycle

### Connection Callbacks

Register callbacks to observe connection state changes:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const HOST = "https://maincloud.spacetimedb.com";
const DB_NAME = "my_database";
const TOKEN_KEY = `${HOST}/${DB_NAME}/auth_token`;

const conn = DbConnection.builder()
    .withUri(HOST)
    .withDatabaseName(DB_NAME)
    .withToken(localStorage.getItem(TOKEN_KEY) ?? undefined)
    .withAutomaticReconnect()
    .onConnect((conn, identity, token) => {
        console.log(`Connected! Identity: ${identity.toHexString()}`);
        // Save token for reconnection — keyed per server/database
        localStorage.setItem(TOKEN_KEY, token);
    })
    .onAutomaticReconnect((_conn, identity, token) => {
        console.log(`Reconnected! Identity: ${identity.toHexString()}`);
        localStorage.setItem(TOKEN_KEY, token);
    })
    .onConnectError((_ctx, error, attempt, delayMs) => {
        console.error('Connection failed:', error);
        if (attempt !== undefined) console.log(`Retry ${attempt} in ${delayMs} ms`);
    })
    .onDisconnect((_ctx, error, attempt, delayMs) => {
        if (attempt !== undefined) {
            console.log(`Connection lost; retry ${attempt} in ${delayMs} ms`, error);
        } else {
            console.log('Connection ended', error);
        }
    })
    .build();
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
var conn = DbConnection.Builder()
    .WithUri("https://maincloud.spacetimedb.com")
    .WithDatabaseName("my_database")
    .WithAutomaticReconnect()
    .OnConnect((conn, identity, token) =>
    {
        Console.WriteLine($"Connected! Identity: {identity}");
        // Save token for reconnection
    })
    .OnAutomaticReconnect((conn, identity, token) =>
    {
        Console.WriteLine($"Reconnected! Identity: {identity}");
        // Save the retained or refreshed token
    })
    .OnConnectError((error, next) =>
    {
        Console.WriteLine($"Connection failed: {error}");
        if (next is { } retry)
        {
            Console.WriteLine($"Retry {retry.Attempt} in {retry.Delay.TotalMilliseconds} ms");
        }
    })
    .OnDisconnect((conn, error, next) =>
    {
        if (next is { } retry)
        {
            Console.WriteLine($"Connection lost; retry {retry.Attempt} in {retry.Delay.TotalMilliseconds} ms: {error}");
        }
        else
        {
            Console.WriteLine($"Connection ended: {error}");
        }
    })
    .Build();
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let conn = DbConnection::builder()
    .with_uri("https://maincloud.spacetimedb.com")
    .with_database_name("my_database")
    .on_connect(|_ctx, _identity, token| {
        println!("Connected! Saving token...");
        // Save token for reconnection
    })
    .on_connect_error(|_ctx, error| {
        eprintln!("Connection failed: {}", error);
    })
    .on_disconnect(|_ctx, error| {
        if let Some(err) = error {
            eprintln!("Disconnected with error: {}", err);
        } else {
            println!("Disconnected normally");
        }
    })
    .build()
    .expect("Failed to connect");
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
// Create delegates
FOnConnectDelegate ConnectDelegate;
BIND_DELEGATE_SAFE(ConnectDelegate, this, AMyActor, OnConnected);

FOnConnectErrorDelegate ErrorDelegate;
BIND_DELEGATE_SAFE(ErrorDelegate, this, AMyActor, OnConnectError);

FOnDisconnectDelegate DisconnectDelegate;
BIND_DELEGATE_SAFE(DisconnectDelegate, this, AMyActor, OnDisconnected);

// Build connection with callbacks
UDbConnection* Conn = UDbConnection::Builder()
    ->WithUri(TEXT("https://maincloud.spacetimedb.com"))
    ->WithDatabaseName(TEXT("my_database"))
    ->OnConnect(ConnectDelegate)
    ->OnConnectError(ErrorDelegate)
    ->OnDisconnect(DisconnectDelegate)
    ->Build();

// Callback functions (must be UFUNCTION)
UFUNCTION()
void OnConnected(UDbConnection* Connection, FSpacetimeDBIdentity Identity, const FString& Token)
{
    UE_LOG(LogTemp, Log, TEXT("Connected! Identity: %s"), *Identity.ToHexString());
    // Save token for reconnection
}

UFUNCTION()
void OnConnectError(const FString& Error)
{
    UE_LOG(LogTemp, Error, TEXT("Connection failed: %s"), *Error);
}

UFUNCTION()
void OnDisconnected(UDbConnection* Connection, const FString& Error)
{
    UE_LOG(LogTemp, Warning, TEXT("Disconnected from SpacetimeDB: %s"), *Error);
}
```

</TabItem>
</Tabs>

### Disconnecting

Explicitly close the connection when you're done:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
conn.disconnect();
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
conn.Disconnect();
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
conn.disconnect();
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
Conn->Disconnect();
```

</TabItem>
</Tabs>

### Reconnection Behavior

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

Enable `.withAutomaticReconnect()` to recover after an established connection drops. The SDK retains the connection, cached rows, subscription handles, and callbacks, then restores subscriptions automatically. While `isReconnecting` is `true`, cache reads return the last known data.

Register subscriptions and row callbacks in `onConnect`, which runs only once, or once after `build()`. Successful recovery invokes `onAutomaticReconnect(conn, identity, token)` before subscription replay; wait for subscription `onApplied` callbacks when you need refreshed data. Save authentication tokens in both connection callbacks.

`onDisconnect` and `onConnectError` report the upcoming retry attempt and delay, or `undefined` when no retry is scheduled. Initial connection failures are not retried, and `disconnect()` stops recovery. For expiring credentials, combine `.withToken(initialToken)` with `.withTokenProvider(() => auth.getAccessToken())` to refresh tokens when needed before reconnect attempts.

React, Solid, and Svelte providers enable automatic reconnect through their shared connection manager. Vue and Angular require `.withAutomaticReconnect()` on the provider's builder. Without automatic reconnect, create a new connection after a connection loss. See the [TypeScript reference](./00700-typescript-reference.md#method-withautomaticreconnect) for backoff options, pending-call behavior, and server requirements.

</TabItem>
<TabItem value="csharp" label="C#">

Enable `.WithAutomaticReconnect()` to recover after an established connection drops. The SDK retains the connection, cached rows, subscription handles, and callbacks, then restores subscriptions in one batch. While `IsReconnecting` is true, cache reads return the last known data. **Keep calling `FrameTick()` during outages, even while `IsActive` is false.** Unity's `SpacetimeDBNetworkManager` handles this automatically.

Register subscriptions and row callbacks in `OnConnect`, which runs only once, or once after `Build()`. Successful recovery invokes `OnAutomaticReconnect(conn, identity, token)` before subscription replay; wait for subscription `OnApplied` callbacks when you need refreshed data. Save authentication tokens in both connection callbacks.

The `OnDisconnect((conn, error, next) => ...)` and `OnConnectError((error, next) => ...)` overloads report the upcoming retry's `Attempt` and `Delay`, or `null` when no retry is scheduled. Initial connection failures are not retried, and `Disconnect()` stops recovery. For expiring credentials, combine `.WithToken(initialToken)` with `.WithTokenProvider(() => RefreshTokenAsync())` to refresh tokens when needed before reconnect attempts.

Without automatic reconnect, create a new connection after a connection loss. See the [C# reference](./00600-csharp-reference.md#method-withautomaticreconnect) for backoff options, token-provider requirements, and pending-call behavior.

</TabItem>
<TabItem value="rust" label="Rust">

Automatic reconnect support is coming soon. For now, implement reconnection in your application: create a new connection and restore subscriptions after a connection loss.

</TabItem>
<TabItem value="unreal" label="Unreal">

Enable `->WithAutomaticReconnect()` to recover after an established connection drops. The connection, cached rows, subscription handles, and callbacks survive. While `IsReconnecting()` is true, reads return the last known rows. Keep automatic ticking enabled, or continue calling `FrameTick()` during outages even when `IsActive()` is false.

`OnConnect` runs only for the initial connection. `OnAutomaticReconnect` takes the same delegate type and runs after recovery, before subscription replay. Register subscriptions and row callbacks once. Subscriptions replay in one atomic batch, and their `OnApplied` callbacks run again after reconciliation. Unchanged rows do not trigger row callbacks.

`OnDisconnect` and `OnConnectError` keep their signatures. To see the retry schedule, register `OnDisconnectWithReconnect` or `OnConnectErrorWithReconnect`, which also receive `bWillReconnect`, `NextReconnectAttempt`, and `NextReconnectDelaySeconds`. When no retry is scheduled, these are `false`, `0`, and `0`. Initial connection failures are not retried; `Disconnect()` cancels recovery.

Use `->WithAutomaticReconnectOptions(Options)` with `FAutomaticReconnectOptions` to configure backoff. `MinDelaySeconds` defaults to 1 and `MaxDelaySeconds` to 30. Delays grow exponentially with 50% jitter, clamped to these bounds. The SDK raises the minimum to 0.5 seconds if needed, then raises the maximum to the greater of one second and the minimum.

For expiring credentials, combine `->WithToken(InitialToken)` and `->WithTokenProvider(Provider)`. The `FTokenProviderDelegate` receives a `UReconnectTokenRequest*`; complete it with `Request->Complete(NewToken, FString())`, or pass an error as the second argument. The provider can finish asynchronously and must return a token for the original identity. Provider failures retry; rejection of a fresh token and identity changes are terminal.

Reducer calls made before `OnConnect` or during recovery are not sent: they return `0` and raise `OnUnhandledReducerError` with `CallError == ESpacetimeDBCallError::Disconnected`. Procedure calls made in those windows complete immediately with `CallError == ESpacetimeDBCallError::Disconnected`. Interrupted calls may have executed: their callback events carry `CallError == ESpacetimeDBCallError::UnknownResult`. Calls are never replayed. New subscriptions queue during recovery; cancelling a subscription removes it from replay.

Regenerate C++ bindings to get the reconnect builder methods. Automatic reconnect requires a server supporting session replacement and batch subscriptions. See the [Unreal reference](./00800-unreal-reference.md#method-withautomaticreconnect) for the builder API.

</TabItem>
</Tabs>

## Connection Identity

Every connection receives a unique [identity](../../00100-intro/00100-getting-started/00400-key-architecture.md#identity) from the server. Access it through the `on_connect` callback:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
.onConnect((conn, identity, token) => {
    console.log(`Identity: ${identity.toHexString()}, ConnectionId: ${conn.connectionId}`);
})
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
.OnConnect((conn, identity, token) =>
{
    var connectionId = conn.ConnectionId;
    Console.WriteLine($"Identity: {identity}, ConnectionId: {connectionId}");
})
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
.on_connect(|ctx, identity, token| {
    let connection_id = ctx.connection_id();
    println!("Identity: {:?}, ConnectionId: {:?}", identity, connection_id);
})
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
UFUNCTION()
void OnConnected(UDbConnection* Connection, FSpacetimeDBIdentity Identity, const FString& Token)
{
    FSpacetimeDBConnectionId ConnectionId = Connection->GetConnectionId();
    UE_LOG(LogTemp, Log, TEXT("Identity: %s, ConnectionId: %s"),
        *Identity.ToHexString(), *ConnectionId.ToHexString());
}
```

</TabItem>
</Tabs>

The [identity](../../00100-intro/00100-getting-started/00400-key-architecture.md#identity) persists across connections and represents the user, while the [connection ID](../../00100-intro/00100-getting-started/00400-key-architecture.md#connectionid) is unique to each connection session.

## Connection Metrics

While connected, the SDK pings the server once a second to measure the connection's round-trip time and to estimate the server's clock. Read a snapshot of the results from the connection:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const stats = conn.networkStats;
if (stats) {
  console.log(`RTT ${stats.rtt.toFixed(1)} ms, jitter ${stats.jitter.toFixed(1)} ms`);
}
```

`networkStats` is `undefined` while the stats are absent. Durations are milliseconds, as a `number`.

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
var stats = conn.stats.Network;
if (stats != null)
{
    Console.WriteLine($"RTT {stats.Rtt.TotalMilliseconds:F1} ms, jitter {stats.Jitter.TotalMilliseconds:F1} ms");
}
```

`stats.Network` is `null` while the stats are absent. Durations are `TimeSpan`s.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
if let Some(stats) = conn.network_stats() {
    println!("RTT {:?}, jitter {:?}", stats.rtt, stats.jitter);
}
```

`network_stats()` returns `None` while the stats are absent. Durations are `Duration`s.

</TabItem>
<TabItem value="unreal" label="Unreal">

Connection metrics for the Unreal C++ SDK are coming soon.

</TabItem>
</Tabs>

The stats are absent until the first ping is answered, and again after the connection closes. Once present, every field is set. Each read returns a fresh snapshot, so read the stats again whenever you want current values.

| TypeScript         | C#                 | Rust                 | Meaning |
| ------------------ | ------------------ | -------------------- | ------- |
| `rtt`              | `Rtt`              | `rtt`                | Smoothed round-trip time, using the same smoothing as TCP (RFC 6298). |
| `rttLatest`        | `RttLatest`        | `rtt_latest`         | The most recent round-trip time sample. |
| `rttMin`           | `RttMin`           | `rtt_min`            | The lowest round-trip time over the last 10 samples. |
| `jitter`           | `Jitter`           | `jitter`             | How much the round-trip time varies (RFC 6298's RTTVAR). |
| `serverNow`        | `ServerNow`        | `server_now`         | The estimated current time on the server. |
| `serverTimeLatest` | `ServerTimeLatest` | `server_time_latest` | When the server received the most recent ping. It doesn't advance between pings. |
| `pongWait`         | `PongWait`         | `pong_wait`          | How long the oldest unanswered ping has been waiting, or zero if every ping has been answered. |

Round-trip times exclude the time the server spent handling each ping, so they measure the network, not server load.

`pongWait` is the field to watch for a stalled connection. On a healthy connection it stays below `rtt`. The other fields only change when a ping is answered, so if the network fails silently they keep showing their last good values, while `pongWait` keeps rising.

### Server Time

`serverNow` is on the same clock as a reducer's `ctx.timestamp`, so you can compare it with timestamps written by your module, for example to show how long ago a row was inserted, or to interpolate game state. It adjusts gradually as new measurements arrive and never goes backwards.

The SDK doesn't fall back to the local clock before the first ping is answered. If your app wants a fallback, choose one explicitly:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const now = conn.networkStats?.serverNow ?? Timestamp.now();
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
Timestamp now = conn.stats.Network?.ServerNow ?? (Timestamp)DateTimeOffset.UtcNow;
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let now = conn
    .network_stats()
    .map_or_else(Timestamp::now, |stats| stats.server_now);
```

</TabItem>
<TabItem value="unreal" label="Unreal">

Connection metrics for the Unreal C++ SDK are coming soon.

</TabItem>
</Tabs>

### Ping Interval

Change how often the SDK pings on the connection builder. The default is one second, and the first ping is sent as soon as the connection is established. An interval of zero disables periodic pinging. The stats then stay absent unless you call `ping()` yourself.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
DbConnection.builder()
  .withPingInterval(5000) // milliseconds; 0 disables
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
DbConnection.Builder()
    .WithPingInterval(TimeSpan.FromSeconds(5)) // TimeSpan.Zero disables
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
DbConnection::builder()
    .with_ping_interval(Duration::from_secs(5)) // Duration::ZERO disables
```

</TabItem>
<TabItem value="unreal" label="Unreal">

Connection metrics for the Unreal C++ SDK are coming soon.

</TabItem>
</Tabs>

### One-off Pings

To measure the connection on demand, for a debug overlay or a "test my connection" button, call `ping()`. It sends a ping immediately and completes with a `PingResult` for that ping alone. Its sample also feeds the connection's stats, and it works with periodic pinging disabled.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const result = await conn.ping();
console.log(`RTT ${result.rtt.toFixed(1)} ms`);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
var result = await conn.Ping();
Console.WriteLine($"RTT {result.Rtt.TotalMilliseconds:F1} ms");
```

You can call `Ping()` from any thread. The task completes without waiting for `FrameTick()`, except in WebGL players; see [`Ping()`](./00600-csharp-reference.md#method-ping) for which thread it completes on.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let result = conn.ping().await?;
println!("RTT {:?}", result.rtt);
```

</TabItem>
<TabItem value="unreal" label="Unreal">

Connection metrics for the Unreal C++ SDK are coming soon.

</TabItem>
</Tabs>

A `PingResult` has the ping's `rtt`, plus the measurements behind it: the total `roundTrip`, the time the server held the ping (`serverHold`), when the server received it (`serverReceiveTime`), and local monotonic clock readings for when it was sent and answered (`sentAt` and `receivedAt`). `ping()` fails if the connection isn't established, or if it closes before the ping is answered.

### Detecting a Dead Connection

A network that fails silently leaves the WebSocket open with nothing arriving: no close and no error. The browser's `WebSocket` gives the SDK no way to notice, so there the SDK uses its pings instead. Once the oldest unanswered ping is older than the ping timeout, and at least one more ping has been sent since, the connection is treated as lost. It reconnects if automatic reconnect is enabled, and disconnects with an error otherwise.

The ping timeout defaults to 10 seconds, or a few round-trip timeouts if that is longer, on a slow link. The check runs once per ping interval, so detection can take up to one interval longer than the timeout. Disabling periodic pinging disables the check.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

The TypeScript SDK applies the ping timeout in every environment, including Node.js. Change it with `.withPingTimeout(ms)`.

</TabItem>
<TabItem value="csharp" label="C#">

The ping timeout applies to Unity WebGL players only. Change it with `.WithPingTimeout(TimeSpan)`, which every build accepts and all but WebGL players ignore. Other builds don't detect a silently failed network, so watch `PongWait` if your app needs to notice one.

</TabItem>
<TabItem value="rust" label="Rust">

The ping timeout applies under the `browser` feature only. Change it with `.with_ping_timeout(Duration)`, which only exists under that feature. Native builds instead send a WebSocket ping frame after 30 seconds with nothing received, and close the connection if another 30 seconds pass without a reply.

</TabItem>
<TabItem value="unreal" label="Unreal">

Connection metrics for the Unreal C++ SDK are coming soon.

</TabItem>
</Tabs>

## Next Steps

Now that you have a connection established, you can:

- [Use the SDK API](./00400-sdk-api.md) to interact with tables, invoke reducers, and subscribe to data
- Register callbacks for observing database changes
- Call reducers and procedures on the server

For language-specific details, see:

- [Rust SDK Reference](./00500-rust-reference.md)
- [C# SDK Reference](./00600-csharp-reference.md)
- [TypeScript SDK Reference](./00700-typescript-reference.md)
- [Unreal SDK Reference](./00800-unreal-reference.md)
