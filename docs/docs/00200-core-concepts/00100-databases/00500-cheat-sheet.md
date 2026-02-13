---
title: Cheat Sheet
slug: /databases/cheat-sheet
---


import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

Quick reference for SpacetimeDB module syntax across Rust, C#, and TypeScript.

## Project Setup

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```bash
spacetime init --lang typescript --project-path my-project my-project
cd my-project
spacetime login
spacetime publish <DATABASE_NAME>
```

</TabItem>
<TabItem value="csharp" label="C#">

```bash
spacetime init --lang csharp --project-path my-project my-project
cd my-project
spacetime login
spacetime publish <DATABASE_NAME>
```

</TabItem>
<TabItem value="rust" label="Rust">

```bash
spacetime init --lang rust --project-path my-project my-project
cd my-project
spacetime login
spacetime publish <DATABASE_NAME>
```

</TabItem>
<TabItem value="cpp" label="C++">

```bash
spacetime init --lang cpp --project-path my-project my-project
cd my-project
spacetime login
spacetime publish <DATABASE_NAME>
```

</TabItem>
</Tabs>

## Tables

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { table, t, schema } from 'spacetimedb/server';

// Basic table
const player = table(
  { name: 'player', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    username: t.string().unique(),
    score: t.i32().index('btree'),
  }
);

// Multi-column index
const score = table(
  {
    name: 'score',
    indexes: [{
      name: 'idx',
      algorithm: 'btree',
      columns: ['player_id', 'level'],
    }],
  },
  {
    player_id: t.u64(),
    level: t.u32(),
  }
);

// Custom types
const status = t.enum('Status', ['Active', 'Inactive']);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

// Basic table
[SpacetimeDB.Table(Public = true)]
public partial struct Player
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    [SpacetimeDB.Unique]
    public string Username;
    
    [SpacetimeDB.Index.BTree]
    public int Score;
}

// Multi-column index
[SpacetimeDB.Table]
[SpacetimeDB.Index.BTree(Name = "idx", Columns = ["PlayerId", "Level"])]
public partial struct Score
{
    public ulong PlayerId;
    public uint Level;
}

// Custom types
[SpacetimeDB.Type]
public enum Status
{
    Active,
    Inactive,
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{table, SpacetimeType};

// Basic table
#[table(name = player, public)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[unique]
    username: String,
    #[index(btree)]
    score: i32,
}

// Multi-column index
#[table(name = score, index(name = idx, btree(columns = [player_id, level])))]
pub struct Score {
    player_id: u64,
    level: u32,
}

// Custom types
#[derive(SpacetimeType)]
pub enum Status {
    Active,
    Inactive,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

// Basic table
struct Player {
  uint64_t id;
  std::string username;
  int32_t score;
};
SPACETIMEDB_STRUCT(Player, id, username, score);
SPACETIMEDB_TABLE(Player, player, Public);
FIELD_PrimaryKeyAutoInc(player, id);
FIELD_Unique(player, username);
FIELD_Index(player, score);

// Multi-column index
struct Score {
  uint64_t player_id;
  uint32_t level;
};
SPACETIMEDB_STRUCT(Score, player_id, level);
SPACETIMEDB_TABLE(Score, score, Private);
// Named multi-column btree index on (player_id, level)
FIELD_NamedMultiColumnIndex(score, idx, player_id, level);

// Custom types (enums)
// Note: 'Status' conflicts with a built-in SDK type; use a distinct name
SPACETIMEDB_ENUM(PlayerStatus, Active, Inactive);
```

</TabItem>
</Tabs>

## Reducers

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema } from 'spacetimedb/server';

const spacetimedb = schema({ player });
export default spacetimedb;

// Basic reducer
export const createPlayer = spacetimedb.reducer({ username: t.string() }, (ctx, { username }) => {
  ctx.db.player.insert({ id: 0n, username, score: 0 });
});

// With error handling
export const updateScore = spacetimedb.reducer({ id: t.u64(), points: t.i32() }, (ctx, { id, points }) => {
  const player = ctx.db.player.id.find(id);
  if (!player) throw new Error('Player not found');
  player.score += points;
  ctx.db.player.id.update(player);
});

// Query examples
const player = ctx.db.player.id.find(123n);           // Find by primary key
const players = ctx.db.player.username.filter('Alice'); // Filter by index
const all = ctx.db.player.iter();                      // Iterate all
ctx.db.player.id.delete(123n);                         // Delete by primary key
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

// Basic reducer
[SpacetimeDB.Reducer]
public static void CreatePlayer(ReducerContext ctx, string username)
{
    ctx.Db.Player.Insert(new Player { Id = 0, Username = username, Score = 0 });
}

// With error handling
[SpacetimeDB.Reducer]
public static void UpdateScore(ReducerContext ctx, ulong id, int points)
{
    var player = ctx.Db.Player.Id.Find(id) 
        ?? throw new Exception("Player not found");
    player.Score += points;
    ctx.Db.Player.Id.Update(player);
}

// Query examples
var player = ctx.Db.Player.Id.Find(123);           // Find by primary key
var players = ctx.Db.Player.Username.Filter("Alice"); // Filter by index
var all = ctx.Db.Player.Iter();                    // Iterate all
ctx.Db.Player.Id.Delete(123);                      // Delete by primary key
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{reducer, ReducerContext};

// Basic reducer
#[reducer]
pub fn create_player(ctx: &ReducerContext, username: String) {
    ctx.db.player().insert(Player { id: 0, username, score: 0 });
}

// With error handling
#[reducer]
pub fn update_score(ctx: &ReducerContext, id: u64, points: i32) -> Result<(), String> {
    let mut player = ctx.db.player().id().find(id)
        .ok_or("Player not found")?;
    player.score += points;
    ctx.db.player().id().update(player);
    Ok(())
}

// Query examples
let player = ctx.db.player().id().find(123);           // Find by primary key
let players = ctx.db.player().username().filter("Alice"); // Filter by index
let all = ctx.db.player().iter();                      // Iterate all
ctx.db.player().id().delete(123);                      // Delete by primary key
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

// Basic reducer
SPACETIMEDB_REDUCER(create_player, ReducerContext ctx, std::string username) {
  ctx.db[player].insert(Player{0, username, 0});
  return Ok();
}

// With error handling
SPACETIMEDB_REDUCER(update_score, ReducerContext ctx, uint64_t id, int32_t points) {
  auto player_opt = ctx.db[player_id].find(id);
  if (!player_opt) {
    return Err("Player not found");
  }
  Player updated = *player_opt;
  updated.score += points;
  ctx.db[player_id].update(updated);
  return Ok();
}

// Query examples
auto player = ctx.db[player_id].find((uint64_t)123);            // Find by primary key
auto player_by_name = ctx.db[player_username].find(std::string("Alice")); // Filter by unique index
for (const auto& p : ctx.db[player]) { /* iterate all */ }      // Iterate all
ctx.db[player_id].delete_by_key((uint64_t)123);                 // Delete by primary key
```

</TabItem>
</Tabs>

## Lifecycle Reducers

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const init = spacetimedb.init(ctx => { /* ... */ });

export const onConnect = spacetimedb.clientConnected(ctx => { /* ... */ });

export const onDisconnect = spacetimedb.clientDisconnected(ctx => { /* ... */ });
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Reducer(ReducerKind.Init)]
public static void Init(ReducerContext ctx) { /* ... */ }

[SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
public static void OnConnect(ReducerContext ctx) { /* ... */ }

[SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
public static void OnDisconnect(ReducerContext ctx) { /* ... */ }
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer(init)]
pub fn init(ctx: &ReducerContext) { /* ... */ }

#[reducer(client_connected)]
pub fn on_connect(ctx: &ReducerContext) { /* ... */ }

#[reducer(client_disconnected)]
pub fn on_disconnect(ctx: &ReducerContext) { /* ... */ }
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
using namespace SpacetimeDB;

SPACETIMEDB_INIT(init, ReducerContext ctx) { /* ... */ }

SPACETIMEDB_CLIENT_CONNECTED(on_connect, ReducerContext ctx) { /* ... */ }

SPACETIMEDB_CLIENT_DISCONNECTED(on_disconnect, ReducerContext ctx) { /* ... */ }
```

</TabItem>
</Tabs>

## Schedule Tables

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const reminder = table(
  { name: 'reminder', scheduled: (): any => sendReminder },
  {
    id: t.u64().primaryKey().autoInc(),
    message: t.string(),
    scheduled_at: t.scheduleAt(),
  }
);

export const sendReminder = spacetimedb.reducer({ arg: reminder.rowType }, (ctx, { arg }) => {
  console.log(`Reminder: ${arg.message}`);
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Scheduled = "SendReminder", ScheduledAt = "ScheduledAt")]
public partial struct Reminder
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    public string Message;
    public ScheduleAt ScheduledAt;
}

[SpacetimeDB.Reducer]
public static void SendReminder(ReducerContext ctx, Reminder reminder)
{
    Log.Info($"Reminder: {reminder.Message}");
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[table(name = reminder, scheduled(send_reminder))]
pub struct Reminder {
    #[primary_key]
    #[auto_inc]
    id: u64,
    message: String,
    scheduled_at: ScheduleAt,
}

#[reducer]
fn send_reminder(ctx: &ReducerContext, reminder: Reminder) {
    log::info!("Reminder: {}", reminder.message);
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
struct Reminder {
    uint64_t id;
    std::string message;
    ScheduleAt scheduled_at;
};
SPACETIMEDB_STRUCT(Reminder, id, message, scheduled_at)
SPACETIMEDB_TABLE(Reminder, reminder, Private)
FIELD_PrimaryKeyAutoInc(reminder, id)

SPACETIMEDB_SCHEDULE(reminder, 2, send_reminder)

SPACETIMEDB_REDUCER(send_reminder, ReducerContext ctx, Reminder reminder) {
    LOG_INFO("Reminder: " + reminder.message);
    return Ok();
}
```

</TabItem>
</Tabs>

## Procedures

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const fetchData = spacetimedb.procedure(
  { url: t.string() },
  t.string(),
  (ctx, { url }) => {
    const response = ctx.http.fetch(url);
    const data = response.text();
    
    ctx.withTx(ctx => {
      ctx.db.cache.insert({ data });
    });
    
    return data;
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Add #pragma warning disable STDB_UNSTABLE at file top

[SpacetimeDB.Procedure]
public static string FetchData(ProcedureContext ctx, string url)
{
    var result = ctx.Http.Get(url);
    if (result is Result<HttpResponse, HttpError>.OkR(var response))
    {
        var data = response.Body.ToStringUtf8Lossy();
        ctx.WithTx(txCtx =>
        {
            txCtx.Db.Cache.Insert(new Cache { Data = data });
            return 0;
        });
        return data;
    }
    return "";
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// In Cargo.toml: spacetimedb = { version = "1.*", features = ["unstable"] }

#[spacetimedb::procedure]
fn fetch_data(ctx: &mut ProcedureContext, url: String) -> String {
    match ctx.http.get(&url) {
        Ok(response) => {
            let body = response.body().to_string_lossy();
            ctx.with_tx(|ctx| {
                ctx.db.cache().insert(Cache { data: body.clone() });
            });
            body
        }
        Err(error) => {
            log::error!("HTTP request failed: {error:?}");
            String::new()
        }
    }
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// SPACETIMEDB_UNSTABLE_FEATURES is necessary to access Http + Transactions in C++ Procedures
#define SPACETIMEDB_UNSTABLE_FEATURES
#include <spacetimedb.h>
using namespace SpacetimeDB;

// Cache table for fetched data
struct Cache {
    std::string data;
};
SPACETIMEDB_STRUCT(Cache, data)
SPACETIMEDB_TABLE(Cache, cache, Private)

SPACETIMEDB_PROCEDURE(std::string, fetch_data, ProcedureContext ctx, std::string url) {
    // Fetch from HTTP (outside transaction)
    auto response = ctx.http.get(url);
    if (!response.is_ok()) {
        LOG_ERROR("HTTP request failed");
        return std::string("");
    }
    
    std::string body = response.value().body.to_string_utf8_lossy();
    
    // Insert into cache with transaction
    ctx.with_tx([&body](TxContext& tx) {
        tx.db[cache].insert(Cache{body});
    });
    
    return body;
}
```

</TabItem>
</Tabs>

## Views

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Return single row
export const myPlayer = spacetimedb.view({ name: 'my_player' }, {}, t.option(player.rowType), ctx => {
  return ctx.db.player.identity.find(ctx.sender);
});

// Return potentially multiple rows
export const topPlayers = spacetimedb.view({ name: 'top_players' }, {}, t.array(player.rowType), ctx => {
  return ctx.db.player.score.filter(1000);
});

// Perform a generic filter using the query builder.
// Equivalent to `SELECT * FROM player WHERE score < 1000`.
export const bottomPlayers = spacetimedb.view({ name: 'bottom_players' }, {}, t.array(player.rowType), ctx => {
  return ctx.from.player.where(p => p.score.lt(1000))
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

// Return single row
[SpacetimeDB.View(Public = true)]
public static Player? MyPlayer(ViewContext ctx)
{
    return ctx.Db.Player.Identity.Find(ctx.Sender);
}

// Return potentially multiple rows
[SpacetimeDB.View(Public = true)]
public static IEnumerable<Player> TopPlayers(ViewContext ctx)
{
    return ctx.Db.Player.Score.Filter(1000);
}

// Perform a generic filter using the query builder.
// Equivalent to `SELECT * FROM player WHERE score < 1000`.
[SpacetimeDB.View(Public = true)]
public static IQuery<Player> BottomPlayers(ViewContext ctx)
{
    return ctx.From.Player.Where(p => p.Score.Lt(1000));
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{view, Query, ViewContext};

// Return single row
#[view(name = my_player, public)]
fn my_player(ctx: &ViewContext) -> Option<Player> {
    ctx.db.player().identity().find(ctx.sender())
}

// Return potentially multiple rows
#[view(name = top_players, public)]
fn top_players(ctx: &ViewContext) -> Vec<Player> {
    ctx.db.player().score().filter(1000).collect()
}

// Perform a generic filter using the query builder.
// Equivalent to `SELECT * FROM player WHERE score < 1000`.
#[view(name = bottom_players, public)]
fn bottom_players(ctx: &ViewContext) -> impl Query<Player> {
    ctx.from.player().r#where(|p| p.score.lt(1000))
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
using namespace SpacetimeDB;

// Return single row using unique indexed field
SPACETIMEDB_VIEW(std::optional<Player>, my_player, Public, ViewContext ctx) {
    return ctx.db[player_identity].find(ctx.sender);
}

// Return multiple rows using indexed field
SPACETIMEDB_VIEW(std::vector<Player>, top_players, Public, ViewContext ctx) {
    return ctx.db[player_score].filter(range_from(int32_t(1000))).collect();
}
```

</TabItem>
</Tabs>

## Context Properties

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
ctx.db                  // Database access
ctx.sender              // Identity of caller
ctx.connectionId        // ConnectionId | undefined
ctx.timestamp           // Timestamp
ctx.identity            // Module's identity
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
ctx.Db                  // Database access
ctx.Sender              // Identity of caller
ctx.ConnectionId        // ConnectionId?
ctx.Timestamp           // Timestamp
ctx.Identity            // Module's identity
ctx.Rng                 // Random number generator
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
ctx.db                  // Database access
ctx.sender()            // Identity of caller
ctx.connection_id()     // Option<ConnectionId>
ctx.timestamp           // Timestamp
ctx.identity()          // Module's identity
ctx.rng()               // Random number generator
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
ctx.db                  // Database access (Table accessor)
ctx.sender              // Identity of caller (Identity type)
ctx.connection_id       // std::optional<ConnectionId>
ctx.timestamp           // Timestamp of current transaction (Timestamp type)
ctx.identity()          // Module's own identity (Identity type)
ctx.rng()               // Random number generator (for seeded randomness)
```

</TabItem>
</Tabs>

## Logging

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
console.error(`Error: ${msg}`);
console.warn(`Warning: ${msg}`);
console.log(`Info: ${msg}`);
console.debug(`Debug: ${msg}`);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
Log.Error($"Error: {msg}");
Log.Warn($"Warning: {msg}");
Log.Info($"Info: {msg}");
Log.Debug($"Debug: {msg}");
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
log::error!("Error: {}", msg);
log::warn!("Warning: {}", msg);
log::info!("Info: {}", msg);
log::debug!("Debug: {}", msg);
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
LOG_ERROR("Error: " + msg);
LOG_WARN("Warning: " + msg);
LOG_INFO("Info: " + msg);
LOG_DEBUG("Debug: " + msg);
```

</TabItem>
</Tabs>

## Common CLI Commands

```bash
# Development
spacetime start                          # Start local server
spacetime dev                            # Interactive development mode
spacetime login                          # Authenticate

# Module management
spacetime build                          # Build module
spacetime publish <NAME>                 # Publish module
spacetime publish --delete-data <NAME>   # Reset database
spacetime delete <NAME>                  # Delete database

# Database operations
spacetime logs <NAME>                    # View logs
spacetime logs --follow <NAME>           # Stream logs
spacetime sql <NAME> "SELECT * FROM t"   # Run SQL query
spacetime describe <NAME>                # Show schema
spacetime call <NAME> reducer arg1 arg2  # Call reducer

# Code generation
spacetime generate --lang rust <NAME>    # Generate Rust client
spacetime generate --lang csharp <NAME>  # Generate C# client
spacetime generate --lang ts <NAME>      # Generate TypeScript client
```

## Common Types

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Type builders
t.bool(), t.string(), t.f32(), t.f64()
t.i8(), t.i16(), t.i32(), t.i64(), t.i128()
t.u8(), t.u16(), t.u32(), t.u64(), t.u128()

// Collections
t.option(T), t.array(T)

// SpacetimeDB types
t.identity(), t.connectionId(), t.timestamp(), t.timeDuration(), t.scheduleAt()

// Structured types
t.object('Name', { field: t.type() })
t.enum('Name', ['Variant1', 'Variant2'])
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Primitives
bool, string, float, double
sbyte, short, int, long, SpacetimeDB.I128
byte, ushort, uint, ulong, SpacetimeDB.U128

// Collections
T?, List<T>

// SpacetimeDB types
Identity, ConnectionId, Timestamp, TimeDuration, ScheduleAt
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Primitives
bool, String, f32, f64
i8, i16, i32, i64, i128
u8, u16, u32, u64, u128

// Collections
Option<T>, Vec<T>

// SpacetimeDB types
Identity, ConnectionId, Timestamp, Duration, ScheduleAt
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// Primitives
bool, std::string, float, double
int8_t, int16_t, int32_t, int64_t
uint8_t, uint16_t, uint32_t, uint64_t

// Large integers (SpacetimeDB types)
SpacetimeDB::i128, SpacetimeDB::u128
SpacetimeDB::i256, SpacetimeDB::u256

// Collections
std::optional<T>, std::vector<T>

// SpacetimeDB types
Identity, ConnectionId, Timestamp, TimeDuration, ScheduleAt
```

</TabItem>
</Tabs>
