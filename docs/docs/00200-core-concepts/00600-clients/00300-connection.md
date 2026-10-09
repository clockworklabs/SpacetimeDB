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
2. A published database running on SpacetimeDB (local or on [MainCloud](../../00300-resources/00100-how-to/00100-deploy/00100-maincloud.md))
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

TypeScript processes messages through the browser or Node.js event loop. Rust applications must advance their connection using `frame_tick()`, `run_threaded()`, `run_async()`, or the browser target's `run_background_task()`.

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
    .on_connect_error(|_ctx, error, _next| {
        eprintln!("Connection failed: {}", error);
    })
    .on_disconnect(|_ctx, error, _next| {
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

Enable `.with_automatic_reconnect()` to recover after an established connection drops. The same connection, table handles, callbacks, and subscription handles survive. While `is_reconnecting()` is true, cache reads return the last known rows. **Keep advancing the connection during outages**, using `frame_tick()`, `run_threaded()`, or `run_async()`; do not stop ticking when `is_active()` becomes false.

`on_connect` fires only for the initial connection. Register `on_automatic_reconnect` for successful automatic reconnects. Both receive the connection, identity, and token. Subscriptions replay together in one atomic batch, and their `on_applied` callbacks fire again; unchanged rows do not fire row callbacks.

`on_disconnect(|ctx, error, next| ...)` and `on_connect_error(|ctx, error, next| ...)` receive `Option<NextReconnect>`, with the next attempt number and `Duration`, or `None` when no retry is scheduled. Initial connection failures are not retried. `disconnect()` cancels recovery.

Use `.with_automatic_reconnect_options(AutomaticReconnectOptions { min_delay, max_delay })` to configure backoff. Defaults are one second and 30 seconds, with exponential growth and 50% jitter clamped to the bounds. The minimum delay is at least 500 ms; the maximum is at least one second and the minimum delay.

For expiring credentials, combine `.with_token(Some(initial_token))` and `.with_token_provider(|| async { /* return Result<String> */ })`. The provider refreshes near expiry or after rejection and must return a token for the same identity. Provider failures retry; rejection of a fresh token and identity changes are terminal.

Calls made while reconnecting fail with a disconnected error. Calls interrupted by a drop may have executed: reducer/procedure callback errors expose `is_unknown_result()`. Calls are never replayed. Subscription creation is queued during recovery, and cancellation removes a set from replay.

Regenerate Rust bindings to expose `is_reconnecting()`. Lifecycle error callbacks gain the third argument; subscription `on_applied` closures must implement `FnMut`. `on_connect` remains `FnOnce`. Automatic reconnect requires a server supporting session replacement and batch subscriptions.

</TabItem>
<TabItem value="unreal" label="Unreal">

Automatic reconnect support for the Unreal C++ SDK is coming soon. For now, implement reconnection in your application: create a new connection and restore subscriptions after a connection loss.

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
