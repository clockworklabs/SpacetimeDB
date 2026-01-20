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
</Tabs>

## Reducers

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema } from 'spacetimedb/server';

const spacetimedb = schema(player);

// Basic reducer
spacetimedb.reducer('create_player', { username: t.string() }, (ctx, { username }) => {
  ctx.db.player.insert({ id: 0n, username, score: 0 });
});

// With error handling
spacetimedb.reducer('update_score', { id: t.u64(), points: t.i32() }, (ctx, { id, points }) => {
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
</Tabs>

## Lifecycle Reducers

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
spacetimedb.init(ctx => { /* ... */ });

spacetimedb.clientConnected(ctx => { /* ... */ });

spacetimedb.clientDisconnected(ctx => { /* ... */ });
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
</Tabs>

## Schedule Tables

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const reminder = table(
  { name: 'reminder', scheduled: 'send_reminder' },
  {
    id: t.u64().primaryKey().autoInc(),
    message: t.string(),
    scheduled_at: t.scheduleAt(),
  }
);

spacetimedb.reducer('send_reminder', { arg: reminder.rowType }, (ctx, { arg }) => {
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
</Tabs>

## Procedures

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
spacetimedb.procedure(
  'fetch_data',
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
// C# procedure support coming soon
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
</Tabs>

## Views

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Return single row
spacetimedb.view('my_player', {}, t.option(player.rowType), ctx => {
  return ctx.db.player.identity.find(ctx.sender);
});

// Return multiple rows
spacetimedb.view('top_players', {}, t.array(player.rowType), ctx => {
  return ctx.db.player.iter().filter(p => p.score > 1000);
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

// Return multiple rows
[SpacetimeDB.View(Public = true)]
public static IEnumerable<Player> TopPlayers(ViewContext ctx)
{
    return ctx.Db.Player.Iter().Where(p => p.Score > 1000);
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{view, ViewContext};

// Return single row
#[view(name = my_player, public)]
fn my_player(ctx: &ViewContext) -> Option<Player> {
    ctx.db.player().identity().find(ctx.sender)
}

// Return multiple rows
#[view(name = top_players, public)]
fn top_players(ctx: &ViewContext) -> Vec<Player> {
    ctx.db.player().iter()
        .filter(|p| p.score > 1000)
        .collect()
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
ctx.sender              // Identity of caller
ctx.connection_id       // Option<ConnectionId>
ctx.timestamp           // Timestamp
ctx.identity()          // Module's identity
ctx.rng()               // Random number generator
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
</Tabs>
