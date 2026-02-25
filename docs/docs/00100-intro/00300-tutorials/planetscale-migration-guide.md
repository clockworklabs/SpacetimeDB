# Migrating from PlanetScale to SpacetimeDB

This guide covers how to migrate a backend built on PlanetScale (serverless MySQL) to SpacetimeDB. It addresses the architectural differences, how to rewrite server-side logic as reducers, how clients query and subscribe to data, and common patterns that arise during migration.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Core Concepts Mapping](#2-core-concepts-mapping)
3. [Schema Migration](#3-schema-migration)
4. [Migrating Server Logic to Reducers](#4-migrating-server-logic-to-reducers)
5. [Authentication and Identity](#5-authentication-and-identity)
6. [Querying Data from the Client](#6-querying-data-from-the-client)
7. [Real-Time Subscriptions](#7-real-time-subscriptions)
8. [Scheduled Jobs](#8-scheduled-jobs)
9. [Indexes and Constraints](#9-indexes-and-constraints)
10. [Data Types Reference](#10-data-types-reference)
11. [Deployment](#11-deployment)
12. [Common Migration Patterns](#12-common-migration-patterns)
13. [Migration Checklist](#13-migration-checklist)

---

## 1. Architecture Overview

### PlanetScale Architecture

In a PlanetScale stack, your application typically looks like this:

```
Client (browser / mobile / game)
    │
    ▼
App Server (Node.js / Go / etc.)
    │  ── REST or GraphQL API
    ▼
PlanetScale (serverless MySQL)
```

- **App server** owns all business logic and is the only entity that talks to the database.
- **Clients** never touch the database directly; they call API endpoints.
- **Real-time** requires a separate layer (WebSockets server, Pusher, Ably, etc.).
- **Auth** is managed by a separate service (Auth0, Clerk, NextAuth, etc.) and passed as JWTs to the app server.

### SpacetimeDB Architecture

SpacetimeDB collapses the app server and database into a single runtime:

```
Client (browser / mobile / game)
    │
    ▼  WebSocket
SpacetimeDB
    ├── Module (WASM) — your business logic as reducers
    └── Tables — in-memory, persisted to disk
```

- **No separate app server.** Your logic lives inside a SpacetimeDB module compiled to WebAssembly.
- **Clients connect directly** to SpacetimeDB over WebSocket.
- **Mutations** are performed by calling reducers (named, transactional functions in the module).
- **Reads** are handled by client-side subscriptions: the client registers SQL queries and receives the matching rows, plus live updates when those rows change.
- **Auth** is built in via `Identity` — a cryptographically verified identifier tied to each client.

### Key Mindset Shift

| PlanetScale model | SpacetimeDB model |
|---|---|
| Client → HTTP → App Server → SQL | Client → WebSocket → Reducer |
| App server polls or pushes changes | Client subscribes; changes pushed automatically |
| You manage connection pools | SpacetimeDB manages everything |
| External auth tokens (JWT) | Built-in `Identity` per client |
| Cron jobs via external scheduler | Scheduled reducers inside the module |

---

## 2. Core Concepts Mapping

| PlanetScale / MySQL concept | SpacetimeDB equivalent |
|---|---|
| Database table | SpacetimeDB table (defined in module) |
| `INSERT` statement | `insert()` call inside a reducer |
| `UPDATE` / `DELETE` statement | `update()` / `delete()` inside a reducer |
| `SELECT` query (server-side) | Iterator over table handle inside a reducer |
| `SELECT` query (client-side) | Subscription + client-side cache |
| Stored procedure | Reducer (client-callable) |
| Trigger | Lifecycle reducer (`client_connected`, etc.) |
| Cron job | Scheduled reducer |
| User auth token (JWT) | `Identity` (built-in, verified) |
| Connection pool | Managed by SpacetimeDB |
| Migration file (schema change) | Module republish (schema evolves with module) |
| Branch (PlanetScale feature) | Not applicable; use staging vs. production modules |

---

## 3. Schema Migration

### Defining Tables

In PlanetScale you write SQL DDL:

```sql
CREATE TABLE player (
  id         BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
  username   VARCHAR(64) NOT NULL UNIQUE,
  score      INT NOT NULL DEFAULT 0,
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE item (
  id        BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
  player_id BIGINT UNSIGNED NOT NULL,
  name      VARCHAR(128) NOT NULL,
  FOREIGN KEY (player_id) REFERENCES player(id)
);
```

In SpacetimeDB you define tables as structs in your module. The module language (Rust, TypeScript, C#) determines the syntax, but the concept is the same.

#### Rust

```rust
use spacetimedb::{table, Identity, Timestamp};

#[table(name = player, public)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[unique]
    pub username: String,
    pub score: i32,
    pub created_at: Timestamp,
}

#[table(name = item, public)]
pub struct Item {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub player_id: u64,   // references Player.id by convention
    pub name: String,
}
```

#### TypeScript

```typescript
import { table, primaryKey, autoInc, unique, TableContext } from "@spacetimedb/sdk";

export interface Player {
  id: bigint;        // #[auto_inc] via codegen
  username: string;
  score: number;
  createdAt: bigint; // Timestamp as microseconds
}

// Tables are declared in the module (not the client).
// The client receives generated bindings from `spacetimedb generate`.
```

#### C#

```csharp
using SpacetimeDB;

[SpacetimeDB.Table(Name = "player", Public = true)]
public partial class Player
{
    [SpacetimeDB.PrimaryKey, SpacetimeDB.AutoInc]
    public ulong Id;
    [SpacetimeDB.Unique]
    public string Username = "";
    public int Score;
    public SpacetimeDB.Timestamp CreatedAt;
}

[SpacetimeDB.Table(Name = "item", Public = true)]
public partial class Item
{
    [SpacetimeDB.PrimaryKey, SpacetimeDB.AutoInc]
    public ulong Id;
    public ulong PlayerId;
    public string Name = "";
}
```

### Table Visibility

- **`public`** — all connected clients can subscribe to this table.
- **Private (default)** — only the module itself can read/write; clients cannot subscribe.

Use private tables for internal bookkeeping (session state, rate-limit counters, etc.) that clients should never see.

### Foreign Keys

SpacetimeDB does not enforce foreign key constraints at the database level. Referential integrity must be enforced in your reducers. This is equivalent to PlanetScale's approach when you use `SET foreign_key_checks = 0` during bulk imports, except that in SpacetimeDB it is always the reducer's responsibility.

```rust
// Enforce referential integrity manually in a reducer
#[reducer]
pub fn add_item(ctx: &ReducerContext, player_id: u64, name: String) -> Result<(), String> {
    if ctx.db.player().id().find(player_id).is_none() {
        return Err(format!("Player {} not found", player_id));
    }
    ctx.db.item().insert(Item { id: 0, player_id, name });
    Ok(())
}
```

---

## 4. Migrating Server Logic to Reducers

### What is a Reducer?

A **reducer** is a named function defined inside a SpacetimeDB module that:

- Runs **atomically** — either all table changes commit or none do (full transaction semantics).
- Receives a `ReducerContext` giving access to the database, the caller's `Identity`, and the current `Timestamp`.
- Can **read and write** any table in the module.
- **Cannot** perform external I/O (no HTTP calls, no filesystem access, no random number generation outside of approved APIs).
- Returns `Result<(), impl Display>` — an `Err` return rolls back all changes and reports the error to the caller.

### ReducerContext

Every reducer receives `ctx: &ReducerContext` as its first argument:

```rust
pub struct ReducerContext {
    pub db: DbContext,          // Table access: ctx.db.player().insert(...)
    pub caller_identity: Identity,    // Who called this reducer
    pub caller_connection_id: Option<ConnectionId>, // Which connection (None for scheduled)
    pub timestamp: Timestamp,         // When this reducer was called
}
```

### Mapping HTTP Endpoints to Reducers

#### PlanetScale pattern: REST endpoint + SQL

```typescript
// Express route — app server
app.post("/players", async (req, res) => {
  const { username } = req.body;
  const userId = req.user.id; // from JWT middleware
  const [result] = await db.execute(
    "INSERT INTO player (username, created_at) VALUES (?, NOW())",
    [username]
  );
  res.json({ id: result.insertId });
});

app.post("/items", async (req, res) => {
  const { playerId, name } = req.body;
  await db.execute(
    "INSERT INTO item (player_id, name) VALUES (?, ?)",
    [playerId, name]
  );
  res.sendStatus(201);
});

app.put("/players/:id/score", async (req, res) => {
  const { delta } = req.body;
  await db.execute(
    "UPDATE player SET score = score + ? WHERE id = ?",
    [delta, req.params.id]
  );
  res.sendStatus(204);
});
```

#### SpacetimeDB equivalent: Reducers

```rust
use spacetimedb::{reducer, ReducerContext, Timestamp};

#[reducer]
pub fn create_player(ctx: &ReducerContext, username: String) -> Result<(), String> {
    // ctx.caller_identity replaces the JWT-decoded user id
    ctx.db.player().insert(Player {
        id: 0,  // auto_inc fills this in
        username,
        score: 0,
        created_at: ctx.timestamp,
    });
    Ok(())
}

#[reducer]
pub fn add_item(ctx: &ReducerContext, player_id: u64, name: String) -> Result<(), String> {
    if ctx.db.player().id().find(player_id).is_none() {
        return Err(format!("Player {} not found", player_id));
    }
    ctx.db.item().insert(Item { id: 0, player_id, name });
    Ok(())
}

#[reducer]
pub fn add_score(ctx: &ReducerContext, player_id: u64, delta: i32) -> Result<(), String> {
    let mut player = ctx.db.player().id().find(player_id)
        .ok_or_else(|| format!("Player {} not found", player_id))?;
    player.score += delta;
    ctx.db.player().id().update(player);
    Ok(())
}
```

### Lifecycle Reducers

SpacetimeDB provides three special reducers that run automatically:

| Reducer name | When it runs | `caller_identity` |
|---|---|---|
| `init` | Once, when the module is first published | Module's own identity |
| `client_connected` | When a client connects | The connecting client's `Identity` |
| `client_disconnected` | When a client disconnects | The disconnecting client's `Identity` |

These replace patterns you previously handled with JWT middleware or session management:

```rust
#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    // Seed initial data, configure defaults
    log::info!("Module initialized at {}", ctx.timestamp);
}

#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    // Track online players, log connections
    ctx.db.online_player().insert(OnlinePlayer {
        identity: ctx.caller_identity,
        connected_at: ctx.timestamp,
    });
}

#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    ctx.db.online_player()
        .identity()
        .delete(ctx.caller_identity);
}
```

### Authorization Inside Reducers

In PlanetScale you would check JWTs in middleware. In SpacetimeDB, use `ctx.caller_identity`:

```rust
// PlanetScale: JWT middleware sets req.user.id, checked before the handler runs

// SpacetimeDB: check inside the reducer
#[reducer]
pub fn delete_player(ctx: &ReducerContext, player_id: u64) -> Result<(), String> {
    let player = ctx.db.player().id().find(player_id)
        .ok_or("Player not found")?;

    // Only allow the player themselves (or an admin) to delete
    if player.owner_identity != ctx.caller_identity {
        return Err("Not authorized".into());
    }

    ctx.db.player().id().delete(player_id);
    Ok(())
}
```

Store the `Identity` of the creator in rows that need ownership checks:

```rust
#[table(name = player, public)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner_identity: Identity,  // set on creation
    pub username: String,
    pub score: i32,
    pub created_at: Timestamp,
}
```

### Complex Queries Inside Reducers

PlanetScale allows arbitrary SQL inside stored procedures. In SpacetimeDB reducers you use the table handle API:

```rust
// PlanetScale stored procedure
// SELECT p.*, SUM(i.value) as total
// FROM player p
// JOIN item i ON i.player_id = p.id
// WHERE p.score > 100
// GROUP BY p.id

// SpacetimeDB equivalent (Rust)
#[reducer]
pub fn award_bonus_to_top_players(ctx: &ReducerContext) {
    for player in ctx.db.player().iter().filter(|p| p.score > 100) {
        let item_count = ctx.db.item().player_id().filter(player.id).count();
        if item_count > 0 {
            // award bonus logic
        }
    }
}
```

> **Note:** Reducers iterate tables in memory. SpacetimeDB keeps all table data in memory for fast access, so full table scans are performant for moderate data sizes. For large datasets, always use indexed columns to filter first.

---

## 5. Authentication and Identity

### PlanetScale Auth Flow

```
Client → POST /auth/login → App Server
App Server verifies credentials → signs JWT
Client stores JWT → sends in Authorization header
App Server validates JWT on every request → looks up user in DB
```

### SpacetimeDB Auth Flow

```
Client generates a keypair (once, stored locally)
Client connects to SpacetimeDB → Identity is derived from public key
SpacetimeDB verifies identity cryptographically on every call
Reducer receives ctx.caller_identity — no token validation needed
```

The `Identity` is a 256-bit value unique to each client. It is consistent across reconnections as long as the client uses the same keypair.

### Connecting with Identity (TypeScript)

```typescript
import { DbConnection, Identity } from "@spacetimedb/sdk";
import { module_bindings } from "./module_bindings";

const conn = DbConnection.builder()
  .withUri("wss://maincloud.spacetimedb.com")
  .withModuleName("my-game")
  .withCredentials([token])   // optional: restore a previous identity
  .onConnect((conn, identity, token) => {
    console.log("Connected as", identity.toHexString());
    localStorage.setItem("spacetimedb_token", token);
  })
  .build();
```

### Per-Row Ownership Pattern

```rust
// Store owner on creation
#[reducer]
pub fn create_post(ctx: &ReducerContext, content: String) {
    ctx.db.post().insert(Post {
        id: 0,
        author: ctx.caller_identity,
        content,
        created_at: ctx.timestamp,
    });
}

// Enforce ownership on mutation
#[reducer]
pub fn edit_post(ctx: &ReducerContext, post_id: u64, new_content: String) -> Result<(), String> {
    let mut post = ctx.db.post().id().find(post_id)
        .ok_or("Post not found")?;
    if post.author != ctx.caller_identity {
        return Err("Not authorized".into());
    }
    post.content = new_content;
    ctx.db.post().id().update(post);
    Ok(())
}
```

---

## 6. Querying Data from the Client

### PlanetScale: Server-Side SELECT

```typescript
// App server executes SQL and returns JSON over HTTP
const [rows] = await db.execute(
  "SELECT * FROM player WHERE score > ? ORDER BY score DESC LIMIT 10",
  [minScore]
);
res.json(rows);
```

### SpacetimeDB: Client-Side Cache + Subscriptions

SpacetimeDB clients do **not** send SQL queries to the server for reads. Instead:

1. The client **subscribes** to one or more SQL queries.
2. SpacetimeDB sends the **initial matching rows** to the client.
3. As rows matching the subscription change, SpacetimeDB **pushes updates** automatically.
4. The client reads data from its **local in-memory cache**.

This means reads are always instant (local), and you never need to poll.

### Subscribing to Queries (TypeScript)

```typescript
import { DbConnection } from "@spacetimedb/sdk";
import { tables } from "./module_bindings";

// Subscribe using the type-safe Query Builder
conn.subscriptionBuilder()
  .onApplied(() => {
    // Initial data is now in the local cache
    const players = conn.db.player.iter();
    for (const player of players) {
      console.log(player.username, player.score);
    }
  })
  .subscribe([
    tables.player.where(p => p.score.gt(100)),
    tables.item.all(),
  ]);
```

### Query Builder API (TypeScript)

The generated `tables` object provides a type-safe builder for each table:

```typescript
import { tables } from "./module_bindings";

// All rows
tables.player.all()

// Equality filter
tables.player.where(p => p.username.eq("alice"))

// Numeric comparison
tables.player.where(p => p.score.gt(500))
tables.player.where(p => p.score.lte(1000))

// Boolean AND (multiple conditions on same row)
tables.player.where(p => p.score.gt(100).and(p.username.ne("bot")))

// Semijoin: players who have at least one item named "Sword"
tables.player.where(p =>
  p.id.inRelation(tables.item.where(i => i.name.eq("Sword")).select(i => i.playerId))
)
```

### Raw SQL Subscriptions (TypeScript)

You can also subscribe with raw SQL strings when the query builder is insufficient:

```typescript
conn.subscriptionBuilder()
  .subscribe("SELECT * FROM player WHERE score > 100");
```

> **Limitations:** Subscription SQL supports `SELECT * FROM table WHERE ...`. JOINs, aggregates (`COUNT`, `SUM`, etc.), `ORDER BY`, and `LIMIT` are not supported in subscription queries. For sorted or aggregated views, compute them server-side in a reducer and store results in a dedicated table.

### Reading from the Local Cache (TypeScript)

Once subscribed, read data synchronously from the local cache:

```typescript
// Iterate all cached rows
for (const player of conn.db.player.iter()) {
  console.log(player.username);
}

// Find by primary key
const player = conn.db.player.id.find(42n);

// Filter (client-side, over cached rows)
const topPlayers = [...conn.db.player.iter()].filter(p => p.score > 500);
```

### Subscribing to Queries (C#)

```csharp
conn.SubscriptionBuilder()
    .OnApplied(ctx => {
        foreach (var player in ctx.Db.Player.Iter())
            Debug.Log($"{player.Username}: {player.Score}");
    })
    .Subscribe(new[] {
        Tables.Player.Where(p => p.Score > 100),
        Tables.Item.All(),
    });
```

### Subscribing to Queries (Rust client)

```rust
conn.subscription_builder()
    .on_applied(|ctx| {
        for player in ctx.db.player().iter() {
            println!("{}: {}", player.username, player.score);
        }
    })
    .subscribe(vec![
        tables::player::filter(|p| p.score > 100),
        tables::item::all(),
    ]);
```

---

## 7. Real-Time Subscriptions

### PlanetScale: External Real-Time Layer

In PlanetScale stacks, real-time requires a separate service:

```typescript
// With Pusher / Ably / Supabase Realtime
const channel = pusher.subscribe("players");
channel.bind("score-updated", (data) => {
  updateUI(data);
});

// App server triggers the event after a DB write:
await pusher.trigger("players", "score-updated", { playerId, newScore });
```

### SpacetimeDB: Built-In Real-Time

Real-time is not a feature you add — it's how the system works. Subscribe once; receive all future changes automatically.

```typescript
// Register row change callbacks
conn.db.player.onInsert((ctx, player) => {
  console.log("New player:", player.username);
});

conn.db.player.onUpdate((ctx, oldPlayer, newPlayer) => {
  console.log(`${newPlayer.username} score: ${oldPlayer.score} → ${newPlayer.score}`);
});

conn.db.player.onDelete((ctx, player) => {
  console.log("Player left:", player.username);
});

// Subscribe to the query — callbacks fire for any future changes
conn.subscriptionBuilder()
  .subscribe([tables.player.all()]);
```

### Reducer Callbacks (TypeScript)

You can also react when a specific reducer completes:

```typescript
conn.reducers.onAddScore((ctx, playerId, delta) => {
  if (ctx.reducerEvent.status.tag === "committed") {
    console.log(`Score updated for player ${playerId} by ${delta}`);
  }
});
```

### Subscription Lifecycle

```typescript
const handle = conn.subscriptionBuilder()
  .onApplied(() => console.log("Initial data loaded"))
  .onError((ctx, error) => console.error("Subscription error:", error))
  .subscribe([tables.player.all()]);

// Later, unsubscribe
handle.unsubscribe();
```

---

## 8. Scheduled Jobs

### PlanetScale: External Cron

PlanetScale has no built-in scheduler. You use an external cron service (GitHub Actions, Railway cron, Vercel cron, AWS EventBridge) that calls an API endpoint, which then runs SQL.

```yaml
# GitHub Actions cron example
on:
  schedule:
    - cron: "0 * * * *"   # every hour
jobs:
  cleanup:
    runs-on: ubuntu-latest
    steps:
      - run: curl -X POST https://api.myapp.com/cron/cleanup
```

### SpacetimeDB: Scheduled Reducers

Scheduled reducers run inside the module on a repeating interval. They are stored in a special schedule table and require no external infrastructure.

#### Rust

```rust
use spacetimedb::{reducer, table, ReducerContext, ScheduleAt, Timestamp};
use std::time::Duration;

// 1. Declare a schedule table
#[table(name = cleanup_schedule, scheduled(run_cleanup))]
pub struct CleanupSchedule {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}

// 2. Seed the schedule in init
#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.cleanup_schedule().insert(CleanupSchedule {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_secs(3600).into()),
    });
}

// 3. The scheduled reducer
#[reducer]
pub fn run_cleanup(ctx: &ReducerContext, _arg: CleanupSchedule) -> Result<(), String> {
    let cutoff = ctx.timestamp - Duration::from_secs(86400 * 30).into();
    for stale in ctx.db.session().created_at().filter_range(..cutoff).collect::<Vec<_>>() {
        ctx.db.session().id().delete(stale.id);
    }
    log::info!("Cleanup complete at {}", ctx.timestamp);
    Ok(())
}
```

#### TypeScript module

```typescript
import { Reducer, ReducerContext, ScheduleAt, table, primaryKey, autoInc } from "@spacetimedb/sdk/module";

@table({ name: "cleanup_schedule", scheduled: "runCleanup" })
class CleanupSchedule {
  @primaryKey @autoInc scheduledId: bigint = 0n;
  scheduledAt!: ScheduleAt;
}

@Reducer.init
function init(ctx: ReducerContext) {
  ctx.db.cleanupSchedule.insert({
    scheduledId: 0n,
    scheduledAt: { tag: "Interval", value: BigInt(3_600_000_000) }, // 1 hour in microseconds
  });
}

@Reducer
function runCleanup(ctx: ReducerContext, _arg: CleanupSchedule) {
  // cleanup logic
}
```

---

## 9. Indexes and Constraints

### PlanetScale Indexes

```sql
-- Primary key (clustered index in InnoDB)
CREATE TABLE player (id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY, ...);

-- Secondary unique index
CREATE UNIQUE INDEX idx_player_username ON player (username);

-- Non-unique secondary index
CREATE INDEX idx_item_player ON item (player_id);

-- Composite index
CREATE INDEX idx_item_composite ON item (player_id, name);
```

### SpacetimeDB Indexes

SpacetimeDB supports:
- **`#[primary_key]`** — exactly one per table, unique, fast lookup.
- **`#[unique]`** — unique constraint + index on a single column.
- **`#[index(btree)]`** — non-unique BTree index for range queries and filtered iteration.
- **`#[auto_inc]`** — auto-incrementing integer (must be combined with `#[primary_key]` or `#[unique]`).

#### Rust

```rust
#[table(name = item, public)]
pub struct Item {
    #[primary_key]
    #[auto_inc]
    pub id: u64,

    #[index(btree)]          // fast filter by player_id
    pub player_id: u64,

    #[index(btree)]
    pub created_at: Timestamp,

    pub name: String,
}
```

Using an index inside a reducer:

```rust
// Without index: full scan — O(n)
ctx.db.item().iter().filter(|i| i.player_id == player_id)

// With #[index(btree)] on player_id — O(log n) + output size
ctx.db.item().player_id().filter(player_id)
```

#### Composite indexes (Rust)

```rust
#[table(name = item, public)]
#[index(btree, name = "by_player_and_name", columns = [player_id, name])]
pub struct Item {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub player_id: u64,
    pub name: String,
}
```

### Constraints

| PlanetScale (MySQL) | SpacetimeDB |
|---|---|
| `PRIMARY KEY` | `#[primary_key]` |
| `UNIQUE` | `#[unique]` |
| `NOT NULL` | All fields are non-null by default |
| `NULLABLE` | Use `Option<T>` |
| `FOREIGN KEY` | Not enforced; check in reducer |
| `CHECK` | Not supported; validate in reducer |
| `DEFAULT value` | Set default in reducer before insert |

---

## 10. Data Types Reference

| MySQL / PlanetScale type | SpacetimeDB Rust type | Notes |
|---|---|---|
| `TINYINT` / `BOOL` | `i8` / `bool` | |
| `SMALLINT` | `i16` | |
| `INT` | `i32` | |
| `BIGINT` | `i64` | |
| `BIGINT UNSIGNED` | `u64` | Common for IDs |
| `FLOAT` | `f32` | |
| `DOUBLE` | `f64` | |
| `VARCHAR(n)` / `TEXT` | `String` | No length limit |
| `BLOB` / `VARBINARY` | `Vec<u8>` / `Bytes` | |
| `DATETIME` / `TIMESTAMP` | `Timestamp` | Microseconds since Unix epoch |
| `JSON` | `String` (serialized) or custom struct | Use a struct for type safety |
| `ENUM('a','b','c')` | Rust `enum` | Use `#[sats]` derive |
| `NULL`-able column | `Option<T>` | |
| `UUID` | `[u8; 16]` or `String` | No native UUID type |
| User identity | `Identity` | Built-in 256-bit type |

### Nullable Columns

```sql
-- MySQL
ALTER TABLE player ADD COLUMN avatar_url VARCHAR(255) NULL;
```

```rust
// SpacetimeDB Rust
#[table(name = player, public)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub username: String,
    pub avatar_url: Option<String>,   // NULL equivalent
}
```

### Timestamps

```rust
use spacetimedb::Timestamp;

// Access in a reducer
let now: Timestamp = ctx.timestamp;

// Timestamp arithmetic (Duration addition)
use std::time::Duration;
let one_hour_later = ctx.timestamp + Duration::from_secs(3600).into();

// Store and compare
if player.created_at < ctx.timestamp - Duration::from_secs(86400).into() {
    // player joined more than 24 hours ago
}
```

---

## 11. Deployment

### PlanetScale Deployment

1. Create a database branch (`main`, `staging`).
2. Deploy schema changes via DDL migration on a branch.
3. Open a deploy request to merge schema changes to `main`.
4. Deploy your app server separately (Vercel, Railway, etc.).

### SpacetimeDB Deployment

SpacetimeDB modules are deployed with the `spacetime` CLI. There is no separate app server to deploy.

#### Install the CLI

```bash
curl -sSf https://install.spacetimedb.com | sh
```

#### Publish a Module

```bash
# From your module directory (Rust example)
spacetime publish --server maincloud.spacetimedb.com my-module-name

# Or targeting a self-hosted instance
spacetime publish --server http://localhost:3000 my-module-name
```

#### Update a Running Module

```bash
# Re-publish with the same name — performs a schema migration
spacetime publish --server maincloud.spacetimedb.com my-module-name
```

SpacetimeDB performs automatic schema migration when you re-publish. Additive changes (new tables, new columns with defaults) are applied without data loss. Destructive changes (removing columns, changing types) require manual migration steps.

#### Logs

```bash
spacetime logs --server maincloud.spacetimedb.com my-module-name --follow
```

#### Calling Reducers from the CLI

```bash
# Useful for seeding data or admin operations
spacetime call my-module-name create_player '["alice"]'
```

#### SQL Queries from the CLI

```bash
# Inspect table data directly
spacetime sql my-module-name "SELECT * FROM player ORDER BY score DESC LIMIT 10"
```

### Environment Parity

| PlanetScale concept | SpacetimeDB equivalent |
|---|---|
| Database branch (`main`) | Published module (production name) |
| Database branch (`staging`) | Published module (staging name, e.g. `my-module-staging`) |
| Deploy request | `spacetime publish` |
| Connection string | `wss://maincloud.spacetimedb.com` + module name |
| Credentials in `.env` | Client token stored in `localStorage` / keychain |

---

## 12. Common Migration Patterns

### Pattern 1: Leaderboard

**PlanetScale approach:** `SELECT * FROM player ORDER BY score DESC LIMIT 10` called from the server on every page load or on a timer.

**SpacetimeDB approach:** Subscribe to all players; sort client-side. For very large tables, maintain a `leaderboard` table updated by a reducer.

```rust
#[table(name = leaderboard_entry, public)]
pub struct LeaderboardEntry {
    #[primary_key]
    pub rank: u32,
    pub player_id: u64,
    pub username: String,
    pub score: i32,
}

#[reducer]
pub fn refresh_leaderboard(ctx: &ReducerContext) {
    // Clear old entries
    for entry in ctx.db.leaderboard_entry().iter().collect::<Vec<_>>() {
        ctx.db.leaderboard_entry().rank().delete(entry.rank);
    }
    // Compute top 10 (sort in-memory)
    let mut players: Vec<Player> = ctx.db.player().iter().collect();
    players.sort_by(|a, b| b.score.cmp(&a.score));
    for (rank, player) in players.into_iter().take(10).enumerate() {
        ctx.db.leaderboard_entry().insert(LeaderboardEntry {
            rank: rank as u32 + 1,
            player_id: player.id,
            username: player.username.clone(),
            score: player.score,
        });
    }
}
```

Clients subscribe to `leaderboard_entry` and receive updates whenever `refresh_leaderboard` runs.

### Pattern 2: Pagination

**PlanetScale approach:** `SELECT * FROM item LIMIT 20 OFFSET 40`

**SpacetimeDB approach:** Subscription queries do not support `LIMIT`/`OFFSET`. Options:

1. **Subscribe to all** — practical if the table is small to moderate in size.
2. **Cursor table** — a reducer writes a `paged_result` table scoped to the requesting identity; the client subscribes to rows matching their identity.
3. **Client-side pagination** — subscribe to all rows for the user, sort/paginate in the client UI layer.

```rust
// Cursor pattern: server-computed pages
#[table(name = item_page, public)]
pub struct ItemPage {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub requester: Identity,
    pub page: u32,
    pub item_id: u64,
    pub item_name: String,
}

#[reducer]
pub fn fetch_item_page(ctx: &ReducerContext, page: u32, page_size: u32) {
    // Delete previous results for this requester
    for row in ctx.db.item_page().requester().filter(ctx.caller_identity).collect::<Vec<_>>() {
        ctx.db.item_page().id().delete(row.id);
    }
    let offset = (page * page_size) as usize;
    let items: Vec<Item> = ctx.db.item()
        .player_id()
        .filter(ctx.caller_identity.into()) // if items are scoped to a player
        .skip(offset)
        .take(page_size as usize)
        .collect();
    for item in items {
        ctx.db.item_page().insert(ItemPage {
            id: 0,
            requester: ctx.caller_identity,
            page,
            item_id: item.id,
            item_name: item.name.clone(),
        });
    }
}
```

### Pattern 3: User Sessions / Presence

**PlanetScale approach:** Store sessions in a `sessions` table with TTLs; poll or use a heartbeat endpoint.

**SpacetimeDB approach:** Use `client_connected` / `client_disconnected` lifecycle reducers. The session table stays accurate automatically.

```rust
#[table(name = presence, public)]
pub struct Presence {
    #[primary_key]
    pub identity: Identity,
    pub username: String,
    pub connected_at: Timestamp,
}

#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    if let Some(player) = ctx.db.player().owner_identity().find(ctx.caller_identity) {
        ctx.db.presence().insert(Presence {
            identity: ctx.caller_identity,
            username: player.username,
            connected_at: ctx.timestamp,
        });
    }
}

#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    ctx.db.presence().identity().delete(ctx.caller_identity);
}
```

Clients subscribing to `presence` automatically see who is online without any polling.

### Pattern 4: Soft Deletes

**PlanetScale approach:** Add a `deleted_at` column; filter `WHERE deleted_at IS NULL` in every query.

**SpacetimeDB approach:** Because clients subscribe to full rows, soft deletes work the same way. Use an `Option<Timestamp>` for `deleted_at`. Alternatively, for truly deleted data, perform a hard delete — clients with a subscription will receive a delete event automatically.

```rust
#[table(name = post, public)]
pub struct Post {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub author: Identity,
    pub content: String,
    pub deleted_at: Option<Timestamp>,
}

#[reducer]
pub fn soft_delete_post(ctx: &ReducerContext, post_id: u64) -> Result<(), String> {
    let mut post = ctx.db.post().id().find(post_id).ok_or("Not found")?;
    if post.author != ctx.caller_identity {
        return Err("Not authorized".into());
    }
    post.deleted_at = Some(ctx.timestamp);
    ctx.db.post().id().update(post);
    Ok(())
}
```

### Pattern 5: Aggregates / Counters

**PlanetScale approach:** `SELECT COUNT(*), SUM(score) FROM player` in a server-side handler, cached externally (Redis).

**SpacetimeDB approach:** Maintain aggregate tables updated by reducers.

```rust
#[table(name = game_stats, public)]
pub struct GameStats {
    #[primary_key]
    pub id: u32,           // singleton row, always id = 1
    pub total_players: u64,
    pub total_score: i64,
}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.game_stats().insert(GameStats { id: 1, total_players: 0, total_score: 0 });
}

#[reducer]
pub fn create_player(ctx: &ReducerContext, username: String) {
    ctx.db.player().insert(Player { id: 0, username, score: 0, created_at: ctx.timestamp });
    if let Some(mut stats) = ctx.db.game_stats().id().find(1) {
        stats.total_players += 1;
        ctx.db.game_stats().id().update(stats);
    }
}
```

Clients subscribing to `game_stats` always have the live aggregate without any extra work.

### Pattern 6: Multi-Tenant Data

**PlanetScale approach:** Add a `tenant_id` column to every table; enforce in every query with `WHERE tenant_id = ?`.

**SpacetimeDB approach:** Use `Identity` as the tenant discriminator. Private tables are invisible to clients by default; for public tables, clients only subscribe to rows that include their identity.

```typescript
// TypeScript client: subscribe only to your own data
conn.subscriptionBuilder().subscribe([
  tables.item.where(i => i.ownerId.eq(conn.identity!)),
]);
```

```rust
// Rust module: private tables are only accessible inside reducers
#[table(name = tenant_config)]   // no `public` — clients can't see this
pub struct TenantConfig {
    #[primary_key]
    pub tenant: Identity,
    pub plan: String,
    pub max_items: u32,
}
```

---

## 13. Migration Checklist

Use this checklist to track your migration progress.

### Planning

- [ ] Map every PlanetScale table to a SpacetimeDB table definition.
- [ ] Identify which tables should be `public` (client-visible) vs. private.
- [ ] Map every HTTP endpoint / stored procedure to a reducer.
- [ ] Identify lifecycle events (`init`, connect, disconnect) that replace middleware.
- [ ] Identify cron jobs that become scheduled reducers.
- [ ] Plan how `owner_identity` or similar fields replace JWT-based auth.

### Schema

- [ ] Translate all DDL (`CREATE TABLE`) to SpacetimeDB table structs.
- [ ] Replace `AUTO_INCREMENT PRIMARY KEY` with `#[primary_key] #[auto_inc]`.
- [ ] Replace `UNIQUE` constraints with `#[unique]`.
- [ ] Replace secondary indexes with `#[index(btree)]`.
- [ ] Replace `NULL`-able columns with `Option<T>`.
- [ ] Replace `DATETIME`/`TIMESTAMP` columns with `Timestamp`.
- [ ] Remove `FOREIGN KEY` constraints; add manual checks to reducers.

### Server Logic

- [ ] Convert every INSERT/UPDATE/DELETE endpoint to a reducer.
- [ ] Add authorization checks using `ctx.caller_identity`.
- [ ] Replace JWT middleware with `ctx.caller_identity` comparisons.
- [ ] Implement `client_connected` / `client_disconnected` for session/presence.
- [ ] Replace cron jobs with scheduled reducers (schedule table + reducer).
- [ ] Remove all external HTTP calls from server logic (not permitted in reducers).
- [ ] Move any external integrations (email, webhooks) to a sidecar service that calls reducers via the SpacetimeDB HTTP API.

### Client

- [ ] Remove all HTTP API calls for data fetching.
- [ ] Replace API calls for mutations with `conn.reducers.reducerName(args)`.
- [ ] Set up `DbConnection` with the correct host and module name.
- [ ] Define subscriptions for all tables the client needs.
- [ ] Replace polling or webhook listeners with `onInsert` / `onUpdate` / `onDelete` callbacks.
- [ ] Store and restore the client `token` across sessions for consistent `Identity`.
- [ ] Replace server-sorted/paginated responses with client-side sorting over cached rows (or server-computed page tables).

### Testing and Verification

- [ ] Use `spacetime sql <module> "SELECT * FROM <table>"` to verify reducer effects.
- [ ] Use `spacetime logs <module> --follow` during development for reducer output.
- [ ] Call reducers directly with `spacetime call <module> <reducer> <args>` for integration tests.
- [ ] Verify that `client_disconnected` cleans up presence/session tables correctly.
- [ ] Test authorization: attempt to call reducers as the wrong identity and confirm they return errors.
- [ ] Load test subscriptions: verify clients receive updates within acceptable latency under concurrent writes.

### Deployment

- [ ] Build and publish the module: `spacetime publish --server <host> <name>`.
- [ ] Verify production module name matches client connection strings.
- [ ] Store per-environment module names in environment variables (not hardcoded).
- [ ] Set up log monitoring for reducer errors via `spacetime logs`.
- [ ] Remove the old app server once all traffic is migrated.
- [ ] Remove PlanetScale database branches once data migration is verified.

---

## Further Reading

- [SpacetimeDB Documentation](https://spacetimedb.com/docs)
- [Reducers Reference](https://spacetimedb.com/docs/modules/rust/reducers)
- [Tables Reference](https://spacetimedb.com/docs/modules/rust/tables)
- [TypeScript Client SDK Reference](https://spacetimedb.com/docs/sdks/typescript)
- [C# Client SDK Reference](https://spacetimedb.com/docs/sdks/csharp)
- [Rust Client SDK Reference](https://spacetimedb.com/docs/sdks/rust)
- [Subscriptions Reference](https://spacetimedb.com/docs/subscriptions)
- [SpacetimeDB CLI Reference](https://spacetimedb.com/docs/cli)
