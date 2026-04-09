# SpacetimeDB Rust Server Module Guidelines

## Imports

```rust
use spacetimedb::{reducer, table, ReducerContext, Table};
```

Additional imports when needed:
```rust
use spacetimedb::SpacetimeType;                    // For custom product/sum types
use spacetimedb::Identity;                         // For auth identity fields
use spacetimedb::Timestamp;                        // For timestamp fields
use spacetimedb::ScheduleAt;                       // For scheduled tables
use spacetimedb::{view, AnonymousViewContext};     // For anonymous views
use spacetimedb::{view, ViewContext};              // For per-user views
use std::time::Duration;                           // For schedule intervals
```

## Table Definitions

```rust
#[table(accessor = user, public)]
pub struct User {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
    pub active: bool,
}
```

Table attribute options:
- `accessor = name` — required, the API name used to access the table
- `public` — makes table visible to clients
- `event` — marks as event table (append-only, no primary key needed)
- `scheduled(reducer_name)` — links table to a scheduled reducer

## Primary Keys and Constraints

```rust
#[primary_key]
pub id: i32,

#[primary_key]
#[auto_inc]
pub id: u64,

#[unique]
pub email: String,
```

## Indexes

Single-column inline index:
```rust
#[index(btree)]
pub owner: Identity,
```

Named index in table attribute:
```rust
#[table(
    accessor = order,
    index(accessor = by_category, btree(columns = [category]))
)]
pub struct Order {
    #[primary_key]
    pub id: i32,
    pub category: String,
    pub amount: u64,
}
```

Multi-column index:
```rust
#[table(
    accessor = membership,
    index(accessor = by_user,  btree(columns = [user_id])),
    index(accessor = by_group, btree(columns = [group_id]))
)]
pub struct Membership {
    #[primary_key]
    pub id: i32,
    pub user_id: i32,
    pub group_id: i32,
}
```

## Column Types

| Rust Type | Usage |
|-----------|-------|
| `i32`, `i64` | Signed integers |
| `u32`, `u64` | Unsigned integers |
| `f32`, `f64` | Floating point |
| `bool` | Boolean |
| `String` | Text |
| `Identity` | User identity |
| `Timestamp` | Timestamp |
| `ScheduleAt` | Schedule metadata |
| `Option<T>` | Nullable field |

## Product Types (Structs)

```rust
#[derive(SpacetimeType, Clone, Debug)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[table(accessor = entity)]
pub struct Entity {
    #[primary_key]
    pub id: i32,
    pub pos: Position,
}
```

## Sum Types (Enums)

```rust
#[derive(SpacetimeType, Clone, Debug)]
pub struct Rect {
    pub width: i32,
    pub height: i32,
}

#[derive(SpacetimeType, Clone, Debug)]
pub enum Shape {
    Circle(i32),
    Rectangle(Rect),
}

#[table(accessor = drawing)]
pub struct Drawing {
    #[primary_key]
    pub id: i32,
    pub shape: Shape,
}
```

## Reducers

Basic reducer:
```rust
#[reducer]
pub fn insert_user(ctx: &ReducerContext, id: i32, name: String, age: i32, active: bool) {
    ctx.db.user().insert(User { id, name, age, active });
}
```

Reducer with Result return:
```rust
#[reducer]
pub fn create_item(ctx: &ReducerContext, name: String) -> Result<(), String> {
    ctx.db.item().insert(Item { id: 0, name });
    Ok(())
}
```

Reducer with no arguments:
```rust
#[reducer]
pub fn reset_all(ctx: &ReducerContext) -> Result<(), String> {
    Ok(())
}
```

## Database Operations

### Insert
```rust
ctx.db.user().insert(User { id: 1, name: "Alice".into(), age: 30, active: true });

// Auto-inc: use 0 as placeholder
ctx.db.message().insert(Message { id: 0, text: "Hello".to_string() });

// Insert returns the inserted row
let row = ctx.db.user().insert(User { id: 0, name: "Bob".into() });
```

### Find (by primary key or unique index)
```rust
if let Some(user) = ctx.db.user().id().find(user_id) {
    // use user
}

let user = ctx.db.user().id().find(user_id).expect("not found");
```

### Filter (by btree index — returns iterator)
```rust
for order in ctx.db.order().by_category().filter(&category) {
    total += order.amount;
}

// Collect to vec
let posts: Vec<Post> = ctx.db.post().by_author().filter(&author_id).collect();
```

### Iterate all rows
```rust
for row in ctx.db.user().iter() {
    // process row
}

let count = ctx.db.user().iter().count();
```

### Update (by primary key — pass full struct)
```rust
ctx.db.user().id().update(User { id, name: new_name, age: new_age, active: true });

// Or modify existing:
let mut user = ctx.db.user().id().find(id).expect("not found");
user.name = new_name;
ctx.db.user().id().update(user);
```

### Delete (by primary key or indexed column)
```rust
ctx.db.user().id().delete(user_id);
ctx.db.online_player().identity().delete(&ctx.sender());
```

## Authentication

`ctx.sender()` returns the authenticated caller's `Identity`:

```rust
#[table(accessor = message, public)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner: Identity,
    pub text: String,
}

#[reducer]
pub fn send_message(ctx: &ReducerContext, text: String) {
    ctx.db.message().insert(Message {
        id: 0,
        owner: ctx.sender(),
        text,
    });
}

#[reducer]
pub fn delete_message(ctx: &ReducerContext, id: u64) {
    let msg = ctx.db.message().id().find(id).expect("not found");
    if msg.owner != ctx.sender() {
        panic!("unauthorized");
    }
    ctx.db.message().id().delete(id);
}
```

Identity as primary key:
```rust
#[table(accessor = player)]
pub struct Player {
    #[primary_key]
    pub identity: Identity,
    pub name: String,
}

// Lookup by identity
if ctx.db.player().identity().find(ctx.sender()).is_some() {
    panic!("already registered");
}
```

## Lifecycle Hooks

```rust
#[reducer(init)]
pub fn init(ctx: &ReducerContext) -> Result<(), String> {
    ctx.db.config().insert(Config { id: 0, setting: "default".into() });
    Ok(())
}

#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    ctx.db.online_player().insert(OnlinePlayer {
        identity: ctx.sender(),
        connected_at: ctx.timestamp,
    });
}

#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    ctx.db.online_player().identity().delete(&ctx.sender());
}
```

## Views

Anonymous view (same result for all clients):
```rust
use spacetimedb::{view, AnonymousViewContext};

#[view(accessor = active_announcements, public)]
fn active_announcements(ctx: &AnonymousViewContext) -> Vec<Announcement> {
    ctx.db.announcement().active().filter(true).collect()
}
```

Per-user view (result varies by sender):
```rust
use spacetimedb::{view, ViewContext};

#[view(accessor = my_profile, public)]
fn my_profile(ctx: &ViewContext) -> Option<Profile> {
    ctx.db.profile().identity().find(ctx.sender())
}
```

## Scheduled Tables

```rust
#[table(accessor = tick_timer, scheduled(tick))]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[reducer]
pub fn tick(_ctx: &ReducerContext, _row: TickTimer) -> Result<(), String> {
    // Runs each time the timer fires
    Ok(())
}

// Schedule a recurring interval
#[reducer(init)]
pub fn init(ctx: &ReducerContext) -> Result<(), String> {
    let interval: ScheduleAt = Duration::from_millis(50).into();
    ctx.db.tick_timer().insert(TickTimer {
        scheduled_id: 0,
        scheduled_at: interval,
    });
    Ok(())
}

// Schedule at a specific time
let fire_at = ctx.timestamp + Duration::from_secs(60);
ctx.db.reminder().insert(Reminder {
    scheduled_id: 0,
    scheduled_at: ScheduleAt::Time(fire_at),
    message: "Hello!".to_string(),
});

// Cancel a scheduled job
ctx.db.reminder().scheduled_id().delete(&job_id);
```

## Optional Fields

```rust
#[table(accessor = player)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
    pub nickname: Option<String>,
    pub high_score: Option<u32>,
}
```

## Helper Functions

Non-reducer utility functions (no `#[reducer]` attribute):
```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[reducer]
pub fn compute_sum(ctx: &ReducerContext, id: i32, a: i32, b: i32) {
    ctx.db.result().insert(ResultRow { id, sum: add(a, b) });
}
```

## Complete Module Example

```rust
use spacetimedb::{reducer, table, Identity, ReducerContext, Table, Timestamp};

#[table(accessor = user, public)]
pub struct User {
    #[primary_key]
    pub identity: Identity,
    pub name: String,
    pub online: bool,
}

#[table(
    accessor = message,
    public,
    index(accessor = by_sender, btree(columns = [sender]))
)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub sender: Identity,
    pub text: String,
    pub sent_at: Timestamp,
}

#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    if let Some(mut user) = ctx.db.user().identity().find(ctx.sender()) {
        user.online = true;
        ctx.db.user().identity().update(user);
    }
}

#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    if let Some(mut user) = ctx.db.user().identity().find(ctx.sender()) {
        user.online = false;
        ctx.db.user().identity().update(user);
    }
}

#[reducer]
pub fn register(ctx: &ReducerContext, name: String) {
    if ctx.db.user().identity().find(ctx.sender()).is_some() {
        panic!("already registered");
    }
    ctx.db.user().insert(User {
        identity: ctx.sender(),
        name,
        online: true,
    });
}

#[reducer]
pub fn send_message(ctx: &ReducerContext, text: String) {
    if ctx.db.user().identity().find(ctx.sender()).is_none() {
        panic!("not registered");
    }
    ctx.db.message().insert(Message {
        id: 0,
        sender: ctx.sender(),
        text,
        sent_at: ctx.timestamp,
    });
}
```
