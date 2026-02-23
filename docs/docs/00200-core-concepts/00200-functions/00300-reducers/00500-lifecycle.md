---
title: Lifecycle Reducers
slug: /functions/reducers/lifecycle
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";


Special reducers handle system events during the database lifecycle.

## Init Reducer

Runs once when the module is first published or when the database is cleared.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const init = spacetimedb.init((ctx) => {
  console.log('Database initializing...');
  
  // Set up default data
  if (ctx.db.settings.count === 0) {
    ctx.db.settings.insert({
      key: 'welcome_message',
      value: 'Hello, SpacetimeDB!'
    });
  }
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Reducer(ReducerKind.Init)]
public static void Init(ReducerContext ctx)
{
    Log.Info("Database initializing...");
    
    // Set up default data
    if (ctx.Db.Settings.Count == 0)
    {
        ctx.Db.Settings.Insert(new Settings
        {
            Key = "welcome_message",
            Value = "Hello, SpacetimeDB!"
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer(init)]
pub fn init(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Database initializing...");
    
    // Set up default data
    if ctx.db.settings().count() == 0 {
        ctx.db.settings().try_insert(Settings {
            key: "welcome_message".to_string(),
            value: "Hello, SpacetimeDB!".to_string(),
        })?;
    }
    
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

struct Settings {
    std::string key;
    std::string value;
};
SPACETIMEDB_STRUCT(Settings, key, value);
SPACETIMEDB_TABLE(Settings, settings, Private);
FIELD_Unique(settings, key);

SPACETIMEDB_INIT(init, ReducerContext ctx) {
    LOG_INFO("Database initializing...");
    
    // Set up default data
    if (ctx.db[settings].count() == 0) {
        ctx.db[settings].insert(Settings{
            "welcome_message",
            "Hello, SpacetimeDB!"
        });
    }
    
    return Ok();
}
```

</TabItem>
</Tabs>

The `init` reducer:
- Cannot take arguments beyond `ReducerContext`
- Runs when publishing with `spacetime publish`
- Runs when clearing with `spacetime publish -c`
- Failure prevents publishing or clearing

## Client Connected

Runs when a client establishes a connection.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const onConnect = spacetimedb.clientConnected((ctx) => {
  console.log(`Client connected: ${ctx.sender}`);
  
  // ctx.connectionId is guaranteed to be defined
  const connId = ctx.connectionId!;
  
  // Initialize client session
  ctx.db.sessions.insert({
    connection_id: connId,
    identity: ctx.sender,
    connected_at: ctx.timestamp
  });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
public static void OnConnect(ReducerContext ctx)
{
    Log.Info($"Client connected: {ctx.Sender}");
    
    // ctx.ConnectionId is guaranteed to be non-null
    var connId = ctx.ConnectionId!.Value;
    
    // Initialize client session
    ctx.Db.Session.Insert(new Session
    {
        ConnectionId = connId,
        Identity = ctx.Sender,
        ConnectedAt = ctx.Timestamp
    });
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer(client_connected)]
pub fn on_connect(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Client connected: {}", ctx.sender());
    
    // ctx.connection_id() is guaranteed to be Some(...)
    let conn_id = ctx.connection_id().unwrap();
    
    // Initialize client session
    ctx.db.sessions().try_insert(Session {
        connection_id: conn_id,
        identity: ctx.sender(),
        connected_at: ctx.timestamp,
    })?;
    
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

struct Session {
    ConnectionId connection_id;
    Identity identity;
    Timestamp connected_at;
};
SPACETIMEDB_STRUCT(Session, connection_id, identity, connected_at);
SPACETIMEDB_TABLE(Session, sessions, Private);
FIELD_PrimaryKey(sessions, connection_id);

SPACETIMEDB_CLIENT_CONNECTED(on_connect, ReducerContext ctx) {
    LOG_INFO("Client connected: " + ctx.sender.to_string());
    
    // ctx.connection_id is guaranteed to be present
    auto conn_id = ctx.connection_id.value();
    
    // Initialize client session
    ctx.db[sessions].insert(Session{
        conn_id,
        ctx.sender,
        ctx.timestamp
    });
    
    return Ok();
}
```

</TabItem>
</Tabs>

The `client_connected` reducer:
- Cannot take arguments beyond `ReducerContext`
- `ctx.connection_id()` is guaranteed to be present
- Failure disconnects the client
- Runs for each distinct connection (WebSocket, HTTP call)

## Client Disconnected

Runs when a client connection terminates.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  console.log(`Client disconnected: ${ctx.sender}`);
  
  // ctx.connectionId is guaranteed to be defined
  const connId = ctx.connectionId!;
  
  // Clean up client session
  ctx.db.sessions.connection_id.delete(connId);
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
public static void OnDisconnect(ReducerContext ctx)
{
    Log.Info($"Client disconnected: {ctx.Sender}");
    
    // ctx.ConnectionId is guaranteed to be non-null
    var connId = ctx.ConnectionId!.Value;
    
    // Clean up client session
    ctx.Db.Session.ConnectionId.Delete(connId);
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer(client_disconnected)]
pub fn on_disconnect(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Client disconnected: {}", ctx.sender());
    
    // ctx.connection_id() is guaranteed to be Some(...)
    let conn_id = ctx.connection_id().unwrap();
    
    // Clean up client session
    ctx.db.sessions().connection_id().delete(&conn_id);
    
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

struct Session {
    ConnectionId connection_id;
    Identity identity;
    Timestamp connected_at;
};
SPACETIMEDB_STRUCT(Session, connection_id, identity, connected_at);
SPACETIMEDB_TABLE(Session, sessions, Private);
FIELD_PrimaryKey(sessions, connection_id);

SPACETIMEDB_CLIENT_DISCONNECTED(on_disconnect, ReducerContext ctx) {
    LOG_INFO("Client disconnected: " + ctx.sender.to_string());
    
    // ctx.connection_id is guaranteed to be present
    auto conn_id = ctx.connection_id.value();
    
    // Clean up client session
    ctx.db[sessions_connection_id].delete_by_key(conn_id);
    
    return Ok();
}
```

</TabItem>
</Tabs>

The `client_disconnected` reducer:
- Cannot take arguments beyond `ReducerContext`
- `ctx.connection_id()` is guaranteed to be present
- Failure is logged but doesn't prevent disconnection
- Runs when connection ends (close, timeout, error)

## Scheduled Reducers

Reducers can be triggered at specific times using schedule tables. See [Schedule Tables](../../00300-tables/00500-schedule-tables.md) for details on:

- Defining schedule tables
- Triggering reducers at specific timestamps
- Running reducers periodically
- Canceling scheduled executions

:::info Scheduled Reducer Context
Scheduled reducer calls originate from SpacetimeDB itself, not from a client. Therefore:
- `ctx.sender()` will be the module's own identity
- `ctx.connection_id()` will be `None`/`null`/`undefined`
:::
