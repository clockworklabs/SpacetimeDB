---
title: Connecting to SpacetimeDB
slug: /sdks/connection
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


After [generating client bindings](/sdks/codegen) for your module, you can establish a connection to your SpacetimeDB [database](/databases) from your client application. The `DbConnection` type provides a persistent WebSocket connection that enables real-time communication with the server.

## Prerequisites

Before connecting, ensure you have:

1. [Generated client bindings](/sdks/codegen) for your module
2. A published database running on SpacetimeDB (local or on [MainCloud](/how-to/deploy/maincloud))
3. The database's URI and name or identity

## Basic Connection

Create a connection using the `DbConnection` builder pattern:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection } from './module_bindings';

const conn = new DbConnection.builder()
    .withUri("http://localhost:3000")
    .withModuleName("my_database");
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

var conn = DbConnection.Builder()
    .WithUri(new Uri("http://localhost:3000"))
    .WithModuleName("my_database")
    .Build();
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use module_bindings::DbConnection;

let conn = DbConnection::builder()
    .with_uri("http://localhost:3000")
    .with_module_name("my_database")
    .build();
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
#include "ModuleBindings/DbConnection.h"

UDbConnection* Conn = UDbConnection::Builder()
    ->WithUri(TEXT("http://localhost:3000"))
    ->WithModuleName(TEXT("my_database"))
    ->Build();
```

</TabItem>
</Tabs>

Replace `"http://localhost:3000"` with your SpacetimeDB host URI, and `"my_database"` with your database's name or identity.

### Connecting to MainCloud

To connect to a database hosted on MainCloud:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const conn = new DbConnection.builder()
    .withUri("https://maincloud.spacetimedb.com")
    .withModuleName("my_database");
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
var conn = DbConnection.Builder()
    .WithUri(new Uri("https://maincloud.spacetimedb.com"))
    .WithModuleName("my_database")
    .Build();
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let conn = DbConnection::builder()
    .with_uri("https://maincloud.spacetimedb.com")
    .with_module_name("my_database")
    .build();
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
UDbConnection* Conn = UDbConnection::Builder()
    ->WithUri(TEXT("https://maincloud.spacetimedb.com"))
    ->WithModuleName(TEXT("my_database"))
    ->Build();
```

</TabItem>
</Tabs>

## Authentication with Tokens

To authenticate with a token (for example, from [SpacetimeAuth](/spacetimeauth)), provide it when building the connection:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const conn = new DbConnection.builder()
    .withUri("https://maincloud.spacetimedb.com")
    .withModuleName("my_database")
    .withToken("your_auth_token_here");
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
var conn = DbConnection.Builder()
    .WithUri(new Uri("https://maincloud.spacetimedb.com"))
    .WithModuleName("my_database")
    .WithToken("your_auth_token_here")
    .Build();
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let conn = DbConnection::builder()
    .with_uri("https://maincloud.spacetimedb.com")
    .with_module_name("my_database")
    .with_token("your_auth_token_here")
    .build();
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
UDbConnection* Conn = UDbConnection::Builder()
    ->WithUri(TEXT("https://maincloud.spacetimedb.com"))
    ->WithModuleName(TEXT("my_database"))
    ->WithToken(TEXT("your_auth_token_here"))
    ->Build();
```

</TabItem>
</Tabs>

The token is sent to the server during connection and validates your identity. See the [SpacetimeAuth documentation](/spacetimeauth) for details on obtaining and managing tokens.

## Advancing the Connection

:::danger[Critical: C#, Unity, and Unreal Users]

In C# (including Unity) and Unreal Engine, you **must** manually advance the connection to process incoming messages. The connection does not process messages automatically!

Call `DbConnection.FrameTick()` in your game loop or update method:

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
// In your Actor's Tick() method
void AMyActor::Tick(float DeltaTime)
{
    Super::Tick(DeltaTime);
    
    if (Conn)
    {
        Conn->FrameTick();
    }
}
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
const conn = DbConnection.builder()
    .withUri("http://localhost:3000")
    .withModuleName("my_database")
    .onConnect((conn, identity, token) => {
        console.log(`Connected! Identity: ${identity.toHexString()}`);
        // Save token for reconnection
        localStorage.setItem('auth_token', token);
    })
    .onConnectError((_ctx, error) => {
        console.error(`Connection failed:`, error);
    })
    .onDisconnect(() => {
        console.log('Disconnected from SpacetimeDB');
    });
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
var conn = DbConnection.Builder()
    .WithUri(new Uri("http://localhost:3000"))
    .WithModuleName("my_database")
    .OnConnect((conn, identity, token) =>
    {
        Console.WriteLine($"Connected! Identity: {identity}");
        // Save token for reconnection
    })
    .OnConnectError((error) =>
    {
        Console.WriteLine($"Connection failed: {error}");
    })
    .OnDisconnect((conn, error) =>
    {
        if (error != null)
        {
            Console.WriteLine($"Disconnected with error: {error}");
        }
        else
        {
            Console.WriteLine("Disconnected normally");
        }
    })
    .Build();
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let conn = DbConnection::builder()
    .with_uri("http://localhost:3000")
    .with_module_name("my_database")
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
ConnectDelegate.BindDynamic(this, &AMyActor::OnConnected);

FOnConnectErrorDelegate ErrorDelegate;
ErrorDelegate.BindDynamic(this, &AMyActor::OnConnectError);

FOnDisconnectDelegate DisconnectDelegate;
DisconnectDelegate.BindDynamic(this, &AMyActor::OnDisconnected);

// Build connection with callbacks
UDbConnection* Conn = UDbConnection::Builder()
    ->WithUri(TEXT("http://localhost:3000"))
    ->WithModuleName(TEXT("my_database"))
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
void OnDisconnected()
{
    UE_LOG(LogTemp, Warning, TEXT("Disconnected from SpacetimeDB"));
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

:::note[Current Limitation]

Automatic reconnection behavior is inconsistently implemented across SDKs. If your connection is interrupted, you may need to create a new `DbConnection` to re-establish connectivity.

We recommend implementing reconnection logic in your application if reliable connectivity is critical.

:::

## Connection Identity

Every connection receives a unique [identity](/intro/key-architecture#identity) from the server. Access it through the `on_connect` callback:

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

The [identity](/intro/key-architecture#identity) persists across connections and represents the user, while the [connection ID](/intro/key-architecture#connectionid) is unique to each connection session.

## Next Steps

Now that you have a connection established, you can:

- [Use the SDK API](/sdks/api) to interact with tables, invoke reducers, and subscribe to data
- Register callbacks for observing database changes
- Call reducers and procedures on the server

For language-specific details, see:
- [Rust SDK Reference](/sdks/rust)
- [C# SDK Reference](/sdks/c-sharp)
- [TypeScript SDK Reference](/sdks/typescript)
- [Unreal SDK Reference](/sdks/unreal)
