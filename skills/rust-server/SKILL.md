---
name: rust-server
description: SpacetimeDB Rust server module SDK reference. Use when writing tables, reducers, or module logic in Rust.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
  role: server
  language: rust
  cursor_globs: "**/*.rs"
  cursor_always_apply: true
---

# SpacetimeDB Rust SDK Reference

## Imports

```rust
use spacetimedb::{
    reducer, table, Identity, ReducerContext, SpacetimeType, Table,
    ConnectionId, ScheduleAt, TimeDuration, Timestamp, Uuid,
};
```

**`Table` is required** — without it, `ctx.db.*.insert()`, `.iter()`, `.find()` etc. won't compile (`no method named 'insert' found`).

## Tables

`#[spacetimedb::table(...)]` on a `pub struct` — `accessor` must be snake_case:

```rust
#[spacetimedb::table(accessor = entity, public)]
pub struct Entity {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner: Identity,
    pub name: String,
    pub active: bool,
}
```

Options: `accessor = snake_case` (required), `public`, `scheduled(reducer_fn)`, `index(...)`

`ctx.db` accessors use the `accessor` name (snake_case).

## Column Types

| Rust type | Notes |
|-----------|-------|
| `u8` / `u16` / `u32` / `u64` / `u128` | unsigned integers |
| `i8` / `i16` / `i32` / `i64` / `i128` | signed integers |
| `f32` / `f64` | floats |
| `bool` | boolean |
| `String` | text |
| `Vec<T>` | list/array |
| `Identity` | user identity |
| `ConnectionId` | connection handle |
| `Timestamp` | server timestamp (microseconds since epoch) |
| `TimeDuration` | duration in microseconds |
| `Uuid` | UUID |
| `Option<T>` | nullable column |

## Column Attributes

```rust
#[primary_key]          // primary key
#[auto_inc]             // auto-increment (use 0 as placeholder on insert)
#[unique]               // unique constraint
#[index(btree)]         // btree index (enables .filter() on this column)
```

## Indexes

Prefer `#[index(btree)]` inline for single-column. Multi-column uses table-level:

```rust
// Inline (preferred for single-column):
#[index(btree)]
pub author_id: u64,
// Access: ctx.db.post().author_id().filter(author_id)

// Multi-column (table-level):
#[spacetimedb::table(accessor = membership, public,
    index(accessor = by_group_user, btree(columns = [group_id, user_id]))
)]
pub struct Membership { pub group_id: u64, pub user_id: Identity, ... }
// Access: ctx.db.membership().by_group_user().filter((group_id, &user_id))
```

When you frequently look up rows by multiple columns, prefer a multi-column index over filtering by one column and looping over the results.

## Reducers

```rust
#[spacetimedb::reducer]
pub fn create_entity(ctx: &ReducerContext, name: String) {
    ctx.db.entity().insert(Entity { id: 0, owner: ctx.sender(), name, active: true });
}

// Reducers can return Result<(), String> or Result<(), E> where E: Display
#[spacetimedb::reducer]
pub fn validate_entity(ctx: &ReducerContext, name: String) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    ctx.db.entity().try_insert(Entity { id: 0, owner: ctx.sender(), name, active: true })?;
    Ok(())
}
```

Note: `insert()` panics on constraint violations. Use `try_insert()` with `?` when returning `Result`.

## DB Operations

```rust
ctx.db.entity().insert(Entity { id: 0, name: "Sample".into() });  // Insert (0 for autoInc)
ctx.db.entity().id().find(entity_id);                              // Find by PK → Option<Entity>
ctx.db.entity().identity().find(ctx.sender());                     // Find by unique column → Option<Entity>
ctx.db.item().author_id().filter(author_id);                       // Filter by index → iterator
ctx.db.entity().iter();                                            // All rows → iterator
ctx.db.entity().count();                                           // Count rows
ctx.db.entity().id().update(Entity { ..existing, name: new_name }); // Update (spread + override)
ctx.db.entity().id().delete(entity_id);                            // Delete by PK
ctx.db.entity().name().delete("Alice");                            // Delete by indexed column
```

Note: `iter()` and `filter()` return iterators. Collect to Vec if you need `.sort()`, `.filter()`, `.map()`.

Range queries on btree indexes: `filter(18..=65)`, `filter(18..)`, `filter(..18)`.

## Lifecycle Hooks

```rust
#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) { ... }

#[spacetimedb::reducer(client_connected)]
pub fn on_connect(ctx: &ReducerContext) { ... }

#[spacetimedb::reducer(client_disconnected)]
pub fn on_disconnect(ctx: &ReducerContext) { ... }
```

## Views

```rust
// Anonymous view (same result for all clients):
use spacetimedb::{view, AnonymousViewContext};

#[view(accessor = active_users, public)]
fn active_users(ctx: &AnonymousViewContext) -> Vec<Entity> {
    ctx.db.entity().iter().filter(|e| e.active).collect()
}

// Per-user view (result varies by sender):
use spacetimedb::{view, ViewContext};

#[view(accessor = my_profile, public)]
fn my_profile(ctx: &ViewContext) -> Option<Entity> {
    ctx.db.entity().identity().find(ctx.sender())
}
```

## Authentication & Timestamps

```rust
// Auth: ctx.sender() is the caller's Identity
if row.owner != ctx.sender() {
    panic!("unauthorized");
    // or: return Err(anyhow::anyhow!("unauthorized"));
}

// Server timestamps
ctx.db.item().insert(Item { id: 0, owner: ctx.sender(), created_at: ctx.timestamp, .. });

// Timestamp arithmetic
let expiry = ctx.timestamp + TimeDuration::from_micros(delay_micros);

// Client: Timestamp → milliseconds since epoch
timestamp.to_micros_since_unix_epoch() / 1000
```

## Scheduled Tables

```rust
#[spacetimedb::table(accessor = tick_timer, scheduled(tick), public)]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
}

#[spacetimedb::reducer]
pub fn tick(ctx: &ReducerContext, timer: TickTimer) {
    // timer row is auto-deleted after this reducer runs
}

// One-time: fires once at a specific time
let at = ScheduleAt::Time(ctx.timestamp + std::time::Duration::from_secs(10));
// Repeating: fires on an interval
let at = ScheduleAt::Interval(std::time::Duration::from_secs(5).into());

ctx.db.tick_timer().insert(TickTimer { scheduled_id: 0, scheduled_at: at });
```

## Logging

```rust
log::info!("Player connected: {:?}", ctx.sender());
log::warn!("Low health: {}", hp);
log::error!("Failed to find entity");
```

## Custom Types

```rust
#[derive(SpacetimeType)]
pub enum Status { Online, Away, Offline }

#[derive(SpacetimeType)]
pub struct Point { x: f32, y: f32 }
```

## Complete Example

```rust
// src/lib.rs
use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, Timestamp};

#[spacetimedb::table(accessor = entity, public)]
pub struct Entity {
    #[primary_key]
    pub identity: Identity,
    pub name: String,
    pub active: bool,
}

#[spacetimedb::table(accessor = record, public)]
pub struct Record {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner: Identity,
    pub value: u32,
    pub created_at: Timestamp,
}

#[spacetimedb::reducer(client_connected)]
pub fn on_connect(ctx: &ReducerContext) {
    if let Some(existing) = ctx.db.entity().identity().find(ctx.sender()) {
        ctx.db.entity().identity().update(Entity { active: true, ..existing });
    }
}

#[spacetimedb::reducer(client_disconnected)]
pub fn on_disconnect(ctx: &ReducerContext) {
    if let Some(existing) = ctx.db.entity().identity().find(ctx.sender()) {
        ctx.db.entity().identity().update(Entity { active: false, ..existing });
    }
}

#[spacetimedb::reducer]
pub fn create_entity(ctx: &ReducerContext, name: String) {
    if ctx.db.entity().identity().find(ctx.sender()).is_some() {
        panic!("already exists");
    }
    ctx.db.entity().insert(Entity { identity: ctx.sender(), name, active: true });
}

#[spacetimedb::reducer]
pub fn add_record(ctx: &ReducerContext, value: u32) {
    if ctx.db.entity().identity().find(ctx.sender()).is_none() {
        panic!("not found");
    }
    ctx.db.record().insert(Record {
        id: 0,
        owner: ctx.sender(),
        value,
        created_at: ctx.timestamp,
    });
}
```
