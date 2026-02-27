---
name: spacetimedb-rust
description: Develop SpacetimeDB server modules in Rust. Use when writing reducers, tables, or module logic.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
---

# SpacetimeDB Rust Module Development

SpacetimeDB modules are WebAssembly applications that run inside the database. They define tables to store data and reducers to modify data. Clients connect directly to the database and execute application logic inside it.

> **Tested with:** SpacetimeDB 2.0, `spacetimedb` crate 1.1.x

---

## HALLUCINATED APIs — DO NOT USE

**These APIs DO NOT EXIST. LLMs frequently hallucinate them.**

```rust
// WRONG — these macros/attributes don't exist
#[spacetimedb::table]           // Use #[table] after importing
#[spacetimedb::reducer]         // Use #[reducer] after importing
#[derive(Table)]                // Tables use #[table] attribute, not derive
#[derive(Reducer)]              // Reducers use #[reducer] attribute

// WRONG — SpacetimeType on tables
#[derive(SpacetimeType)]        // DO NOT use on #[table] structs!
#[table(accessor = my_table)]
pub struct MyTable { ... }

// WRONG — mutable context
pub fn my_reducer(ctx: &mut ReducerContext, ...) { }  // Should be &ReducerContext

// WRONG — table access without parentheses
ctx.db.player                   // Should be ctx.db.player()
ctx.db.player.find(id)          // Should be ctx.db.player().id().find(&id)

// WRONG — old 1.0 patterns
#[table(name = my_table)]       // Use accessor, not name (2.0)
ctx.sender                      // Use ctx.sender() — method, not field (2.0)
.with_module_name("db")         // Use .with_database_name() (2.0)
ctx.db.user().name().update(..) // Update only via primary key (2.0)
```

### CORRECT PATTERNS:

```rust
use spacetimedb::{table, reducer, Table, ReducerContext, Identity, Timestamp};
use spacetimedb::SpacetimeType;  // Only for custom types, NOT tables

// CORRECT TABLE — accessor, not name; no SpacetimeType derive!
#[table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    pub id: u64,
    pub name: String,
}

// CORRECT REDUCER — immutable context, sender() is a method
#[reducer]
pub fn create_player(ctx: &ReducerContext, name: String) {
    ctx.db.player().insert(Player { id: 0, name });
}

// CORRECT TABLE ACCESS — methods with parentheses, sender() method
let player = ctx.db.player().id().find(&player_id);
let caller = ctx.sender();
```

### DO NOT:
- **Derive `SpacetimeType` on `#[table]` structs** — the macro handles this
- **Use mutable context** — `&ReducerContext`, not `&mut ReducerContext`
- **Forget `Table` trait import** — required for table operations
- **Use field access for tables** — `ctx.db.player()` not `ctx.db.player`
- **Use `ctx.sender`** — it's `ctx.sender()` (method) in 2.0
- **Use `name =` in table/index attributes** — use `accessor =` in 2.0

---

## Common Mistakes Table

| Wrong | Right | Error |
|-------|-------|-------|
| `#[derive(SpacetimeType)]` on `#[table]` | Remove it — macro handles this | Conflicting derive macros |
| `ctx.db.player` (field access) | `ctx.db.player()` (method) | "no field `player` on type" |
| `ctx.db.player().find(id)` | `ctx.db.player().id().find(&id)` | Must access via index |
| `&mut ReducerContext` | `&ReducerContext` | Wrong context type |
| Missing `use spacetimedb::Table;` | Add import | "no method named `insert`" |
| `#[table(accessor = "my_table")]` | `#[table(accessor = my_table)]` | String literals not allowed |
| `#[table(name = my_table)]` | `#[table(accessor = my_table)]` | 2.0 uses `accessor` |
| `ctx.sender` | `ctx.sender()` | 2.0: method, not field |
| Missing `public` on table | Add `public` flag | Clients can't subscribe |
| Network/filesystem in reducer | Use procedures instead | Sandbox violation |
| Panic for expected errors | Return `Result<(), String>` | WASM instance destroyed |
| `.name().update(row)` | `.id().update(row)` | Update only via primary key (2.0) |

---

## Hard Requirements

1. **DO NOT derive `SpacetimeType` on `#[table]` structs** — the macro handles this
2. **Import `Table` trait** — required for all table operations
3. **Use `&ReducerContext`** — not `&mut ReducerContext`
4. **Tables are methods** — `ctx.db.table()` not `ctx.db.table`
5. **Use `ctx.sender()`** — method call, not field access (2.0)
6. **Use `accessor =`** — not `name =` in table/index attributes (2.0)
7. **Reducers must be deterministic** — no filesystem, network, timers, or external RNG
8. **Use `ctx.rng`** — not `rand` crate for random numbers
9. **Add `public` flag** — if clients need to subscribe to a table
10. **Update only via primary key** — use delete+insert for non-PK changes (2.0)

---

## Project Setup

```toml
[package]
name = "my-module"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
spacetimedb = "1.0"
log = "0.4"
```

### Essential Imports

```rust
use spacetimedb::{ReducerContext, Table};
use spacetimedb::{Identity, Timestamp, ConnectionId, ScheduleAt};
```

## Table Definitions

```rust
#[spacetimedb::table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
    score: u32,
}
```

### Table Attributes

| Attribute | Description |
|-----------|-------------|
| `accessor = identifier` | Required. The API name used in `ctx.db.{accessor}()` |
| `public` | Makes table visible to clients via subscriptions |
| `scheduled(reducer_name)` | Creates a schedule table that triggers the named reducer |
| `index(accessor = idx, btree(columns = [a, b]))` | Multi-column index |

### Column Attributes

| Attribute | Description |
|-----------|-------------|
| `#[primary_key]` | Unique identifier for the row (one per table max) |
| `#[unique]` | Enforces uniqueness, enables `find()` method |
| `#[auto_inc]` | Auto-generates unique integer values when inserting 0 |
| `#[index(btree)]` | Creates a B-tree index for efficient lookups |

### Supported Column Types

**Primitives**: `u8`-`u256`, `i8`-`i256`, `f32`, `f64`, `bool`, `String`

**SpacetimeDB Types**: `Identity`, `ConnectionId`, `Timestamp`, `Uuid`, `ScheduleAt`

**Collections**: `Vec<T>`, `Option<T>`, `Result<T, E>`

**Custom Types**: Any struct/enum with `#[derive(SpacetimeType)]`

---

## Reducers

```rust
#[spacetimedb::reducer]
pub fn create_player(ctx: &ReducerContext, name: String) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    ctx.db.player().insert(Player { id: 0, name, score: 0 });
    Ok(())
}
```

### Reducer Rules

1. First parameter must be `&ReducerContext`
2. Return `()`, `Result<(), String>`, or `Result<(), E>` where `E: Display`
3. All changes roll back on panic or `Err` return
4. Must import `Table` trait: `use spacetimedb::Table;`

### ReducerContext

```rust
ctx.db              // Database access
ctx.sender()        // Identity of the caller (method, not field!)
ctx.connection_id   // Connection ID (None for scheduled/system reducers)
ctx.timestamp       // Invocation timestamp
ctx.identity()      // Module's own identity
ctx.rng             // Deterministic RNG
```

---

## Table Operations

```rust
// Insert — returns the row with auto_inc values populated
let player = ctx.db.player().insert(Player { id: 0, name: "Alice".into(), score: 100 });
log::info!("Created player with id: {}", player.id);

// Find by unique/primary key — returns Option
if let Some(player) = ctx.db.player().id().find(123) { }

// Filter by indexed column — returns iterator
for player in ctx.db.player().name().filter("Alice") { }

// Update via primary key (2.0: only primary key has update)
if let Some(player) = ctx.db.player().id().find(123) {
    ctx.db.player().id().update(Player { score: player.score + 10, ..player });
}

// Delete
ctx.db.player().id().delete(&123);

// Iterate all rows
for player in ctx.db.player().iter() { }
let total = ctx.db.player().count();
```

---

## Indexes

```rust
// Single-column index
#[spacetimedb::table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    id: u64,
    #[index(btree)]
    level: u32,
    name: String,
}

// Multi-column index
#[spacetimedb::table(
    accessor = score, public,
    index(accessor = by_player_level, btree(columns = [player_id, level]))
)]
pub struct Score {
    player_id: u32,
    level: u32,
    points: i64,
}
```

---

## Event Tables (2.0)

Reducer callbacks are removed in 2.0. Use event tables + `on_insert` instead.

```rust
#[table(accessor = damage_event, public, event)]
pub struct DamageEvent {
    pub target: Identity,
    pub amount: u32,
}

#[reducer]
fn deal_damage(ctx: &ReducerContext, target: Identity, amount: u32) {
    ctx.db.damage_event().insert(DamageEvent { target, amount });
}
```

Client subscribes and uses `on_insert`:
```rust
conn.db.damage_event().on_insert(|ctx, event| {
    play_damage_animation(event.target, event.amount);
});
```

Event tables must be subscribed explicitly — they are excluded from `subscribe_to_all_tables()`.

---

## Lifecycle Reducers

```rust
#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Database initializing...");
    Ok(())
}

#[spacetimedb::reducer(client_connected)]
pub fn on_connect(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Client connected: {}", ctx.sender());
    Ok(())
}

#[spacetimedb::reducer(client_disconnected)]
pub fn on_disconnect(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Client disconnected: {}", ctx.sender());
    Ok(())
}
```

---

## Scheduled Reducers

```rust
use spacetimedb::ScheduleAt;
use std::time::Duration;

#[spacetimedb::table(accessor = game_tick_schedule, scheduled(game_tick))]
pub struct GameTickSchedule {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[spacetimedb::reducer]
fn game_tick(ctx: &ReducerContext, schedule: GameTickSchedule) {
    if !ctx.sender_auth().is_internal() { return; }
    log::info!("Game tick at {:?}", ctx.timestamp);
}

// Schedule at interval
ctx.db.game_tick_schedule().insert(GameTickSchedule {
    scheduled_id: 0,
    scheduled_at: ScheduleAt::Interval(Duration::from_millis(100).into()),
});

// Schedule at specific time
let run_at = ctx.timestamp + Duration::from_secs(delay_secs);
ctx.db.reminder_schedule().insert(ReminderSchedule {
    scheduled_id: 0,
    scheduled_at: ScheduleAt::Time(run_at),
});
```

---

## Procedures (Beta)

```rust
use spacetimedb::{procedure, ProcedureContext};

#[procedure]
fn save_external_data(ctx: &mut ProcedureContext, url: String) -> Result<(), String> {
    let data = fetch_from_url(&url)?;
    ctx.try_with_tx(|tx| {
        tx.db.external_data().insert(ExternalData { id: 0, content: data });
        Ok(())
    })?;
    Ok(())
}
```

| Reducers | Procedures |
|----------|------------|
| `&ReducerContext` (immutable) | `&mut ProcedureContext` (mutable) |
| Direct `ctx.db` access | Must use `ctx.with_tx()` |
| No HTTP/network | HTTP allowed |
| No return values | Can return data |

---

## Custom Types

```rust
use spacetimedb::SpacetimeType;

#[derive(SpacetimeType)]
pub enum PlayerStatus { Active, Idle, Away }

#[derive(SpacetimeType)]
pub struct Position { x: f32, y: f32, z: f32 }

// Use in table (DO NOT derive SpacetimeType on the table!)
#[spacetimedb::table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    id: u64,
    status: PlayerStatus,
    position: Position,
}
```

---

## Commands

```bash
spacetime build
spacetime publish my_database --module-path .
spacetime publish my_database --clear-database --module-path .
spacetime logs my_database
spacetime call my_database create_player "Alice"
spacetime sql my_database "SELECT * FROM player"
spacetime generate --lang rust --out-dir <client>/src/module_bindings --module-path <backend-dir>
```

## Important Constraints

1. **No Global State**: Static/global variables are undefined behavior across reducer calls
2. **No Side Effects**: Reducers cannot make network requests or file I/O
3. **Deterministic Execution**: Use `ctx.rng` and `ctx.new_uuid_*()` for randomness
4. **Transactional**: All reducer changes roll back on failure
5. **Isolated**: Reducers don't see concurrent changes until commit
