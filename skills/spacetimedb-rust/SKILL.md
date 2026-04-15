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

> **Tested with:** SpacetimeDB 2.0+ APIs

---

## HALLUCINATED APIs — DO NOT USE

**These APIs/patterns are incorrect. LLMs frequently hallucinate them.**

Both macro forms are valid in 2.0: `#[spacetimedb::table(...)]` / `#[table(...)]` and `#[spacetimedb::reducer]` / `#[reducer]`.

```rust
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

---

## Common Mistakes Table

| Wrong | Right | Error |
|-------|-------|-------|
| `#[table(accessor = "my_table")]` | `#[table(accessor = my_table)]` | String literals not allowed |
| Missing `public` on table | Add `public` flag | Clients can't subscribe |
| Network/filesystem in reducer | Use procedures instead | Sandbox violation |
| Panic for expected errors | Return `Result<(), String>` | WASM instance destroyed |

---

## Hard Requirements

1. **DO NOT derive `SpacetimeType` on `#[table]` structs** — the macro handles this
2. **Import `Table` trait** — required for all table operations
3. **Use `&ReducerContext`** — not `&mut ReducerContext`
4. **Tables are methods** — `ctx.db.table()` not `ctx.db.table`
5. **Use `ctx.sender()`** — method call, not field access (2.0)
6. **Use `accessor =` for API handles** — `name = "..."` is optional canonical naming in table/index attributes
7. **Reducers must be deterministic** — no filesystem, network, timers, or external RNG
8. **Use `ctx.rng()`** — not `rand` crate for random numbers
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
spacetimedb = { workspace = true }
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
| `scheduled(function_name)` | Creates a schedule table that triggers the named reducer or procedure |
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
ctx.connection_id() // Option<ConnectionId> (None for scheduled/system reducers)
ctx.timestamp       // Invocation timestamp
ctx.identity()      // Module's own identity
ctx.rng()            // Deterministic RNG (method, not field!)
```

---

## Table Operations

### Insert

```rust
// Insert returns the row with auto_inc values populated
let player = ctx.db.player().insert(Player { id: 0, name: "Alice".into(), score: 100 });
log::info!("Created player with id: {}", player.id);
```

### Find and Filter

```rust
// Find by unique/primary key — returns Option
if let Some(player) = ctx.db.player().id().find(&123) {
    log::info!("Found: {}", player.name);
}

// Optional clarity: typed literals can avoid inference ambiguity
if let Some(player) = ctx.db.player().id().find(&123u64) {
    log::info!("Found: {}", player.name);
}

// Filter by indexed column — returns iterator
for player in ctx.db.player().name().filter(&"Alice".to_string()) {
    log::info!("Player: {}", player.name);
}

// Full table scan
for player in ctx.db.player().iter() { }
let total = ctx.db.player().count();
```

### Update

```rust
// Update via primary key (2.0: only primary key has update)
if let Some(player) = ctx.db.player().id().find(&123) {
    ctx.db.player().id().update(Player { score: player.score + 10, ..player });
}

// For non-PK changes: delete + insert
if let Some(old) = ctx.db.player().id().find(&id) {
    ctx.db.player().id().delete(&id);
    ctx.db.player().insert(Player { name: new_name, ..old });
}
```

### Delete

```rust
// Delete by primary key
ctx.db.player().id().delete(&123);

// Delete by indexed column (collect first to avoid iterator invalidation)
let to_remove: Vec<u64> = ctx.db.player().name().filter(&"Alice".to_string())
    .map(|p| p.id)
    .collect();
for id in to_remove {
    ctx.db.player().id().delete(&id);
}
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

// Multi-column index querying: prefix match (first column only)
for s in ctx.db.score().by_player_level().filter(&(42,)) {
    log::info!("Player 42, any level: {} pts", s.points);
}

// Full match (both columns)
for s in ctx.db.score().by_player_level().filter(&(42, 5)) {
    log::info!("Player 42, level 5: {} pts", s.points);
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
    ctx.db.config().insert(Config {
        id: 0,
        max_players: 100,
        game_mode: "default".to_string(),
    });
    Ok(())
}

#[spacetimedb::reducer(client_connected)]
pub fn on_connect(ctx: &ReducerContext) -> Result<(), String> {
    let caller = ctx.sender();
    log::info!("Client connected: {}", caller);

    if let Some(user) = ctx.db.user().identity().find(&caller) {
        ctx.db.user().identity().update(User { online: true, ..user });
    } else {
        ctx.db.user().insert(User {
            identity: caller,
            name: format!("User-{}", &caller.to_hex()[..8]),
            online: true,
        });
    }
    Ok(())
}

#[spacetimedb::reducer(client_disconnected)]
pub fn on_disconnect(ctx: &ReducerContext) -> Result<(), String> {
    let caller = ctx.sender();
    if let Some(user) = ctx.db.user().identity().find(&caller) {
        ctx.db.user().identity().update(User { online: false, ..user });
    }
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

// Schedule at interval (e.g., in init reducer)
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

## Identity and Authentication

```rust
#[spacetimedb::table(accessor = user, public)]
pub struct User {
    #[primary_key]
    identity: Identity,
    name: String,
    online: bool,
}

#[spacetimedb::reducer]
pub fn set_name(ctx: &ReducerContext, new_name: String) -> Result<(), String> {
    let caller = ctx.sender();
    let user = ctx.db.user().identity().find(&caller)
        .ok_or("User not found — connect first")?;
    ctx.db.user().identity().update(User { name: new_name, ..user });
    Ok(())
}
```

### Owner-Only Reducer Pattern

```rust
fn require_owner(ctx: &ReducerContext, entity_owner: &Identity) -> Result<(), String> {
    if ctx.sender() != *entity_owner {
        Err("Not authorized: you don't own this entity".to_string())
    } else {
        Ok(())
    }
}

#[spacetimedb::reducer]
pub fn rename_character(ctx: &ReducerContext, char_id: u64, new_name: String) -> Result<(), String> {
    let character = ctx.db.character().id().find(&char_id)
        .ok_or("Character not found")?;
    require_owner(ctx, &character.owner)?;
    ctx.db.character().id().update(Character { name: new_name, ..character });
    Ok(())
}
```

---

## Error Handling

```rust
// Sender error — return Err (user sees message, transaction rolls back cleanly)
#[spacetimedb::reducer]
pub fn transfer(ctx: &ReducerContext, to: Identity, amount: u64) -> Result<(), String> {
    let sender = ctx.db.wallet().identity().find(&ctx.sender())
        .ok_or("Wallet not found")?;
    if sender.balance < amount {
        return Err("Insufficient balance".to_string());
    }
    // ... proceed with transfer
    Ok(())
}

// Programmer error — panic (destroys the WASM instance, expensive!)
// Only use for truly impossible states
#[spacetimedb::reducer]
pub fn process(ctx: &ReducerContext, id: u64) {
    let item = ctx.db.item().id().find(&id)
        .expect("BUG: item should exist at this point");
    // ...
}
```

Prefer `Result<(), String>` for all expected failure cases. Panics destroy and recreate the WASM instance.

---

## Procedures (Beta)

> Procedures are behind the `unstable` feature in `spacetimedb`.
> In `Cargo.toml`: `spacetimedb = { version = "...", features = ["unstable"] }`

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
3. **Deterministic Execution**: Use `ctx.rng()` and `ctx.new_uuid_*()` for randomness
4. **Transactional**: All reducer changes roll back on failure
5. **Isolated**: Reducers don't see concurrent changes until commit
