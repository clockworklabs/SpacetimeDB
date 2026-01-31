---
name: spacetimedb-rust
description: Develop SpacetimeDB server modules in Rust. Use when writing reducers, tables, or module logic.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "1.0"
---

# SpacetimeDB Rust Module Development

SpacetimeDB modules are WebAssembly applications that run inside the database. They define tables to store data and reducers to modify data. Clients connect directly to the database and execute application logic inside it.

## Project Setup

### Cargo.toml Requirements

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

The `crate-type = ["cdylib"]` is required for WebAssembly compilation.

### Essential Imports

```rust
use spacetimedb::{ReducerContext, Table};
```

Additional imports as needed:
```rust
use spacetimedb::{Identity, Timestamp, ConnectionId, ScheduleAt};
use spacetimedb::sats::{i256, u256};  // For 256-bit integers
```

## Table Definitions

Tables store data in SpacetimeDB. Define tables using the `#[spacetimedb::table]` macro on a struct.

### Basic Table

```rust
#[spacetimedb::table(name = player, public)]
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
| `name = identifier` | Required. The table name used in `ctx.db.{name}()` |
| `public` | Makes table visible to clients via subscriptions |
| `scheduled(reducer_name)` | Creates a schedule table that triggers the named reducer |
| `index(name = idx, btree(columns = [a, b]))` | Creates a multi-column index |

### Column Attributes

| Attribute | Description |
|-----------|-------------|
| `#[primary_key]` | Unique identifier for the row (one per table max) |
| `#[unique]` | Enforces uniqueness, enables `find()` method |
| `#[auto_inc]` | Auto-generates unique integer values when inserting 0 |
| `#[index(btree)]` | Creates a B-tree index for efficient lookups |
| `#[default(value)]` | Default value for migrations (must be const-evaluable) |

### Supported Column Types

**Primitives**: `u8`, `u16`, `u32`, `u64`, `u128`, `u256`, `i8`, `i16`, `i32`, `i64`, `i128`, `i256`, `f32`, `f64`, `bool`, `String`

**SpacetimeDB Types**: `Identity`, `ConnectionId`, `Timestamp`, `Uuid`, `ScheduleAt`

**Collections**: `Vec<T>`, `Option<T>`, `Result<T, E>` where inner types are also supported

**Custom Types**: Any struct/enum with `#[derive(SpacetimeType)]`

## Reducers

Reducers are transactional functions that modify database state. They run inside the database and are the only way to mutate tables.

### Basic Reducer

```rust
#[spacetimedb::reducer]
pub fn create_player(ctx: &ReducerContext, name: String) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }

    ctx.db.player().insert(Player {
        id: 0,  // auto_inc assigns the value
        name,
        score: 0,
    });

    Ok(())
}
```

### Reducer Rules

1. First parameter must be `&ReducerContext`
2. Additional parameters must implement `SpacetimeType`
3. Return `()`, `Result<(), String>`, or `Result<(), E>` where `E: Display`
4. All changes roll back on panic or `Err` return
5. Reducers run in isolation from concurrent reducers
6. Cannot make network requests or access filesystem
7. Must import `Table` trait for table operations: `use spacetimedb::Table;`

## ReducerContext

The `ReducerContext` provides access to the database and caller information.

### Properties

```rust
#[spacetimedb::reducer]
pub fn example(ctx: &ReducerContext) {
    // Database access
    let _table = ctx.db.player();

    // Caller identity (always present)
    let caller: Identity = ctx.sender;

    // Connection ID (None for scheduled/system reducers)
    let conn: Option<ConnectionId> = ctx.connection_id;

    // Invocation timestamp
    let when: Timestamp = ctx.timestamp;

    // Module's own identity
    let module_id: Identity = ctx.identity();

    // Random number generation (deterministic)
    let random_val: u32 = ctx.random();

    // UUID generation
    let uuid = ctx.new_uuid_v4().unwrap();  // Random UUID
    let uuid = ctx.new_uuid_v7().unwrap();  // Timestamp-based UUID

    // Check if caller is internal (scheduled reducer)
    if ctx.sender_auth().is_internal() {
        // Called by scheduler, not external client
    }
}
```

## Table Operations

### Insert

```rust
// Insert returns the row with auto_inc values populated
let player = ctx.db.player().insert(Player {
    id: 0,  // auto_inc fills this
    name: "Alice".to_string(),
    score: 100,
});
log::info!("Created player with id: {}", player.id);
```

### Find by Unique/Primary Key

```rust
// find() returns Option<RowType>
if let Some(player) = ctx.db.player().id().find(123) {
    log::info!("Found: {}", player.name);
}
```

### Filter by Indexed Column

```rust
// filter() returns an iterator
for player in ctx.db.player().name().filter("Alice") {
    log::info!("Player {}: score {}", player.id, player.score);
}

// Range queries (Rust range syntax)
for player in ctx.db.player().score().filter(50..=100) {
    log::info!("{} has score {}", player.name, player.score);
}
```

### Update

Updates require a unique column. Find the row, modify it, then call `update()`:

```rust
if let Some(mut player) = ctx.db.player().id().find(123) {
    player.score += 10;
    ctx.db.player().id().update(player);
}
```

### Delete

```rust
// Delete by unique key
ctx.db.player().id().delete(&123);

// Delete by indexed column (returns count)
let deleted = ctx.db.player().name().delete("Alice");
log::info!("Deleted {} rows", deleted);

// Delete by range
ctx.db.player().score().delete(..50);  // Delete all with score < 50
```

### Iterate All Rows

```rust
for player in ctx.db.player().iter() {
    log::info!("{}: {}", player.name, player.score);
}

// Count rows
let total = ctx.db.player().count();
```

## Indexes

### Single-Column Index

```rust
#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    id: u64,
    #[index(btree)]
    level: u32,
    name: String,
}
```

### Multi-Column Index

```rust
#[spacetimedb::table(
    name = score,
    public,
    index(name = by_player_level, btree(columns = [player_id, level]))
)]
pub struct Score {
    player_id: u32,
    level: u32,
    points: i64,
}
```

### Querying Multi-Column Indexes

```rust
// Prefix match (first column only)
for score in ctx.db.score().by_player_level().filter(&123u32) {
    log::info!("Level {}: {} points", score.level, score.points);
}

// Full match
for score in ctx.db.score().by_player_level().filter((123u32, 5u32)) {
    log::info!("Points: {}", score.points);
}

// Prefix with range on second column
for score in ctx.db.score().by_player_level().filter((123u32, 1u32..=10u32)) {
    log::info!("Level {}: {} points", score.level, score.points);
}
```

## Identity and Authentication

### Storing User Identity

```rust
#[spacetimedb::table(name = user_profile, public)]
pub struct UserProfile {
    #[primary_key]
    identity: Identity,
    display_name: String,
    created_at: Timestamp,
}

#[spacetimedb::reducer]
pub fn create_profile(ctx: &ReducerContext, display_name: String) -> Result<(), String> {
    // Check if profile already exists
    if ctx.db.user_profile().identity().find(ctx.sender).is_some() {
        return Err("Profile already exists".to_string());
    }

    ctx.db.user_profile().insert(UserProfile {
        identity: ctx.sender,
        display_name,
        created_at: ctx.timestamp,
    });

    Ok(())
}
```

### Verifying Caller Identity

```rust
#[spacetimedb::reducer]
pub fn update_my_profile(ctx: &ReducerContext, new_name: String) -> Result<(), String> {
    // Only allow users to update their own profile
    if let Some(mut profile) = ctx.db.user_profile().identity().find(ctx.sender) {
        profile.display_name = new_name;
        ctx.db.user_profile().identity().update(profile);
        Ok(())
    } else {
        Err("Profile not found".to_string())
    }
}
```

## Lifecycle Reducers

### Init Reducer

Runs once when the module is first published or database is cleared:

```rust
#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Database initializing...");

    // Set up default data
    if ctx.db.config().count() == 0 {
        ctx.db.config().insert(Config {
            key: "version".to_string(),
            value: "1.0.0".to_string(),
        });
    }

    Ok(())
}
```

### Client Connected

Runs when a client establishes a connection:

```rust
#[spacetimedb::reducer(client_connected)]
pub fn on_connect(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Client connected: {}", ctx.sender);

    // connection_id is guaranteed to be Some
    let conn_id = ctx.connection_id.unwrap();

    // Create or update user session
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User { online: true, ..user });
    } else {
        ctx.db.user().insert(User {
            identity: ctx.sender,
            online: true,
            name: None,
        });
    }

    Ok(())
}
```

### Client Disconnected

Runs when a client connection terminates:

```rust
#[spacetimedb::reducer(client_disconnected)]
pub fn on_disconnect(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Client disconnected: {}", ctx.sender);

    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User { online: false, ..user });
    }

    Ok(())
}
```

## Scheduled Reducers

Schedule reducers to run at specific times or intervals.

### Define a Schedule Table

```rust
use spacetimedb::ScheduleAt;
use std::time::Duration;

#[spacetimedb::table(name = game_tick_schedule, scheduled(game_tick))]
pub struct GameTickSchedule {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[spacetimedb::reducer]
fn game_tick(ctx: &ReducerContext, schedule: GameTickSchedule) {
    // Verify this is an internal call (from scheduler)
    if !ctx.sender_auth().is_internal() {
        log::warn!("External call to scheduled reducer rejected");
        return;
    }

    // Game logic here
    log::info!("Game tick at {:?}", ctx.timestamp);
}
```

### Scheduling at Intervals

```rust
#[spacetimedb::reducer]
fn start_game_loop(ctx: &ReducerContext) {
    // Schedule game tick every 100ms
    ctx.db.game_tick_schedule().insert(GameTickSchedule {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(100).into()),
    });
}
```

### Scheduling at Specific Times

```rust
#[spacetimedb::reducer]
fn schedule_reminder(ctx: &ReducerContext, delay_secs: u64) {
    let run_at = ctx.timestamp + Duration::from_secs(delay_secs);

    ctx.db.reminder_schedule().insert(ReminderSchedule {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(run_at),
        message: "Time's up!".to_string(),
    });
}
```

## Error Handling

### Sender Errors (Expected)

Return errors for invalid client input:

```rust
#[spacetimedb::reducer]
pub fn transfer_credits(
    ctx: &ReducerContext,
    to_user: Identity,
    amount: u32,
) -> Result<(), String> {
    let sender = ctx.db.user().identity().find(ctx.sender)
        .ok_or("Sender not found")?;

    if sender.credits < amount {
        return Err("Insufficient credits".to_string());
    }

    // Perform transfer...
    Ok(())
}
```

### Programmer Errors (Bugs)

Use panic for unexpected states that indicate bugs:

```rust
#[spacetimedb::reducer]
pub fn process_data(ctx: &ReducerContext, data: Vec<u8>) {
    // This should never happen - indicates a bug
    assert!(!data.is_empty(), "Unexpected empty data");

    // Use expect for operations that should always succeed
    let parsed = parse_data(&data).expect("Failed to parse data");
}
```

## Custom Types

Define custom types using `#[derive(SpacetimeType)]`:

```rust
use spacetimedb::SpacetimeType;

#[derive(SpacetimeType)]
pub enum PlayerStatus {
    Active,
    Idle,
    Away,
}

#[derive(SpacetimeType)]
pub struct Position {
    x: f32,
    y: f32,
    z: f32,
}

#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    id: u64,
    status: PlayerStatus,
    position: Position,
}
```

## Multiple Tables from Same Type

Apply multiple `#[spacetimedb::table]` attributes to create separate tables with the same schema:

```rust
#[spacetimedb::table(name = online_player, public)]
#[spacetimedb::table(name = offline_player)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    name: String,
}

#[spacetimedb::reducer]
fn player_logout(ctx: &ReducerContext) {
    if let Some(player) = ctx.db.online_player().identity().find(ctx.sender) {
        ctx.db.offline_player().insert(player.clone());
        ctx.db.online_player().identity().delete(&ctx.sender);
    }
}
```

## Logging

Use the `log` crate for debug output. View logs with `spacetime logs <database>`:

```rust
log::trace!("Detailed trace info");
log::debug!("Debug information");
log::info!("General information");
log::warn!("Warning message");
log::error!("Error occurred");
```

Never use `println!`, `eprintln!`, or `dbg!` in modules.

## Common Patterns

### Player Session Management

```rust
#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    name: Option<String>,
    online: bool,
    last_seen: Timestamp,
}

#[spacetimedb::reducer(client_connected)]
pub fn on_connect(ctx: &ReducerContext) {
    match ctx.db.player().identity().find(ctx.sender) {
        Some(player) => {
            ctx.db.player().identity().update(Player {
                online: true,
                last_seen: ctx.timestamp,
                ..player
            });
        }
        None => {
            ctx.db.player().insert(Player {
                identity: ctx.sender,
                name: None,
                online: true,
                last_seen: ctx.timestamp,
            });
        }
    }
}

#[spacetimedb::reducer(client_disconnected)]
pub fn on_disconnect(ctx: &ReducerContext) {
    if let Some(player) = ctx.db.player().identity().find(ctx.sender) {
        ctx.db.player().identity().update(Player {
            online: false,
            last_seen: ctx.timestamp,
            ..player
        });
    }
}
```

### Sequential ID Generation (Gap-Free)

Auto-increment may have gaps after crashes. For strictly sequential IDs:

```rust
#[spacetimedb::table(name = counter)]
pub struct Counter {
    #[primary_key]
    name: String,
    value: u64,
}

#[spacetimedb::reducer]
fn create_invoice(ctx: &ReducerContext, amount: u64) -> Result<(), String> {
    let mut counter = ctx.db.counter().name().find(&"invoice".to_string())
        .unwrap_or(Counter { name: "invoice".to_string(), value: 0 });

    counter.value += 1;
    ctx.db.counter().name().update(counter.clone());

    ctx.db.invoice().insert(Invoice {
        invoice_number: counter.value,
        amount,
    });

    Ok(())
}
```

### Owner-Only Reducers

```rust
#[spacetimedb::table(name = admin)]
pub struct Admin {
    #[primary_key]
    identity: Identity,
}

#[spacetimedb::reducer]
fn admin_action(ctx: &ReducerContext) -> Result<(), String> {
    if ctx.db.admin().identity().find(ctx.sender).is_none() {
        return Err("Not authorized".to_string());
    }

    // Admin-only logic here
    Ok(())
}
```

## Build and Deploy

```bash
# Build the module
spacetime build

# Deploy to local instance
spacetime publish my_database

# Deploy with database clear (DESTROYS DATA)
spacetime publish my_database --delete-data

# View logs
spacetime logs my_database

# Call a reducer
spacetime call my_database create_player "Alice"

# Run SQL query
spacetime sql my_database "SELECT * FROM player"
```

## Important Constraints

1. **No Global State**: Static/global variables are undefined behavior across reducer calls
2. **No Side Effects**: Reducers cannot make network requests or file I/O
3. **Deterministic Execution**: Use `ctx.random()` and `ctx.new_uuid_*()` for randomness
4. **Transactional**: All reducer changes roll back on failure
5. **Isolated**: Reducers don't see concurrent changes until commit
