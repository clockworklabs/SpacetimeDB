---
name: spacetimedb-concepts
description: Understand SpacetimeDB architecture and core concepts. Use when learning SpacetimeDB or making architectural decisions.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
---

# SpacetimeDB Core Concepts

SpacetimeDB is a relational database that is also a server. It lets you upload application logic directly into the database via WebAssembly modules, eliminating the traditional web/game server layer entirely.

---

## Critical Rules (Read First)

These five rules prevent the most common SpacetimeDB mistakes:

1. **Reducers are transactional** — they do not return data to callers. Use subscriptions to read data.
2. **Reducers must be deterministic** — no filesystem, network, timers, or random. All state must come from tables.
3. **Read data via tables/subscriptions** — not reducer return values. Clients get data through subscribed queries.
4. **Auto-increment IDs are not sequential** — gaps are normal, do not use for ordering. Use timestamps or explicit sequence columns.
5. **`ctx.sender()` is the authenticated principal** — never trust identity passed as arguments. Always use `ctx.sender()` for authorization.

---

## Feature Implementation Checklist

When implementing a feature that spans backend and client:

1. **Backend:** Define table(s) to store the data
2. **Backend:** Define reducer(s) to mutate the data
3. **Client:** Subscribe to the table(s)
4. **Client:** Call the reducer(s) from UI — **do not skip this step**
5. **Client:** Render the data from the table(s)

**Common mistake:** Building backend tables/reducers but forgetting to wire up the client to call them.

---

## Debugging Checklist

When things are not working:

1. Is SpacetimeDB server running? (`spacetime start`)
2. Is the module published? (`spacetime publish`)
3. Are client bindings generated? (`spacetime generate`)
4. Check server logs for errors (`spacetime logs <db-name>`)
5. **Is the reducer actually being called from the client?**

---

## CLI Commands

```bash
spacetime start
spacetime publish <db-name> --module-path <module-path>
spacetime publish <db-name> --clear-database -y --module-path <module-path>
spacetime generate --lang <lang> --out-dir <out> --module-path <module-path>
spacetime logs <db-name>
```

---

## What SpacetimeDB Is

SpacetimeDB combines a database and application server into a single deployable unit. Clients connect directly to the database and execute application logic inside it. The system is optimized for real-time applications requiring maximum speed and minimum latency.

Key characteristics:

- **In-memory execution**: All application state lives in memory for sub-millisecond access
- **Persistent storage**: Data is automatically persisted to a write-ahead log (WAL) for durability
- **Real-time synchronization**: Changes are automatically pushed to subscribed clients
- **Single deployment**: No separate servers, containers, or infrastructure to manage

## The Five Zen Principles

1. **Everything is a Table**: Your entire application state lives in tables. No separate cache layer, no Redis, no in-memory state to synchronize.
2. **Everything is Persistent**: SpacetimeDB persists everything by default, including full history.
3. **Everything is Real-Time**: Clients are replicas of server state. Subscribe to data and it flows automatically.
4. **Everything is Transactional**: Every reducer runs atomically. Either all changes succeed or all roll back.
5. **Everything is Programmable**: Modules are real code (Rust, C#, TypeScript) running inside the database.

## Tables

Tables store all data in SpacetimeDB. They use the relational model and support SQL queries for subscriptions.

### Defining Tables

Tables are defined using language-specific attributes. In 2.0, use `accessor` (not `name`) for the API name:

**Rust:**
```rust
#[spacetimedb::table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    id: u32,
    #[index(btree)]
    name: String,
    #[unique]
    email: String,
}
```

**C#:**
```csharp
[SpacetimeDB.Table(Accessor = "Player", Public = true)]
public partial struct Player
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public uint Id;
    [SpacetimeDB.Index.BTree]
    public string Name;
    [SpacetimeDB.Unique]
    public string Email;
}
```

**TypeScript:**
```typescript
const players = table(
  { name: 'players', public: true },
  {
    id: t.u32().primaryKey().autoInc(),
    name: t.string().index('btree'),
    email: t.string().unique(),
  }
);
```

### Table Visibility

- **Private tables** (default): Only accessible by reducers and the database owner
- **Public tables**: Exposed for client read access through subscriptions. Writes still require reducers.

### Table Design Principles

Organize data by access pattern, not by entity:

**Decomposed approach (recommended):**
```
Player          PlayerState         PlayerStats
id         <--  player_id           player_id
name            position_x          total_kills
                position_y          total_deaths
                velocity_x          play_time
```

Benefits: reduced bandwidth, cache efficiency, schema evolution, semantic clarity.

## Reducers

Reducers are transactional functions that modify database state. They are the ONLY way to mutate tables in SpacetimeDB.

### Key Properties

- **Transactional**: Run in isolated database transactions
- **Atomic**: Either all changes succeed or all roll back
- **Isolated**: Cannot interact with the outside world (no network, no filesystem)
- **Callable**: Clients invoke reducers as remote procedure calls

### Critical Reducer Rules

1. **No global state**: Relying on static variables is undefined behavior
2. **No side effects**: Reducers cannot make network requests or access files
3. **Store state in tables**: All persistent state must be in tables
4. **No return data**: Reducers do not return data to callers — use subscriptions
5. **Must be deterministic**: No random, no timers, no external I/O

### Defining Reducers

**Rust:**
```rust
#[spacetimedb::reducer]
pub fn create_user(ctx: &ReducerContext, name: String, email: String) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    ctx.db.user().insert(User { id: 0, name, email });
    Ok(())
}
```

**C#:**
```csharp
[SpacetimeDB.Reducer]
public static void CreateUser(ReducerContext ctx, string name, string email)
{
    if (string.IsNullOrEmpty(name))
        throw new ArgumentException("Name cannot be empty");
    ctx.Db.User.Insert(new User { Id = 0, Name = name, Email = email });
}
```

### ReducerContext

Every reducer receives a `ReducerContext` providing:
- `ctx.db`: Access to all tables (read and write)
- `ctx.sender()`: The Identity of the caller (Rust: method; C#/TS: property/field)
- `ctx.connection_id`: The connection ID of the caller
- `ctx.timestamp`: The current timestamp

## Event Tables (2.0)

Reducer callbacks are removed in 2.0. Use **event tables** to broadcast reducer-specific data to clients.

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

Clients subscribe to event tables and use `on_insert` callbacks. Event tables are excluded from `subscribe_to_all_tables()` and must be subscribed explicitly.

## Subscriptions

Subscriptions replicate database rows to clients in real-time.

### How Subscriptions Work

1. **Subscribe**: Register SQL queries describing needed data
2. **Receive initial data**: All matching rows are sent immediately
3. **Receive updates**: Real-time updates when subscribed rows change
4. **React to changes**: Use callbacks (`onInsert`, `onDelete`, `onUpdate`)

### Subscription Best Practices

1. **Group subscriptions by lifetime**: Keep always-needed data separate from temporary subscriptions
2. **Subscribe before unsubscribing**: When updating subscriptions, subscribe to new data first
3. **Avoid overlapping queries**: Distinct queries returning overlapping data cause redundant processing
4. **Use indexes**: Queries on indexed columns are efficient; full table scans are expensive

## Modules

Modules are WebAssembly bundles containing application logic that runs inside the database.

### Module Components

- **Tables**: Define the data schema
- **Reducers**: Define callable functions that modify state
- **Views**: Define read-only computed queries
- **Event Tables**: Broadcast reducer-specific data to clients (2.0)
- **Procedures**: (Beta) Functions that can have side effects (HTTP requests)

### Module Languages

Server-side modules can be written in: Rust, C#, TypeScript (beta)

### Module Lifecycle

1. **Write**: Define tables and reducers in your chosen language
2. **Compile**: Build to WebAssembly using the SpacetimeDB CLI
3. **Publish**: Upload to a SpacetimeDB host with `spacetime publish`
4. **Hot-swap**: Republish to update code without disconnecting clients

## Identity

Identity is SpacetimeDB's authentication system based on OpenID Connect (OIDC).

- **Identity**: A long-lived, globally unique identifier for a user.
- **ConnectionId**: Identifies a specific client connection.

```rust
#[spacetimedb::reducer]
pub fn do_something(ctx: &ReducerContext) {
    let caller_identity = ctx.sender();  // Who is calling?
    // NEVER trust identity passed as a reducer argument
}
```

### Authentication Providers

SpacetimeDB works with any OIDC provider: SpacetimeAuth (built-in), Auth0, Clerk, Keycloak, Google, GitHub, etc.

## When to Use SpacetimeDB

### Ideal Use Cases

- **Real-time games**: MMOs, multiplayer games, turn-based games
- **Collaborative applications**: Document editing, whiteboards, design tools
- **Chat and messaging**: Real-time communication with presence
- **Live dashboards**: Streaming analytics and monitoring

### Key Decision Factors

Choose SpacetimeDB when you need:
- Sub-10ms latency for reads and writes
- Automatic real-time synchronization
- Transactional guarantees for all operations
- Simplified architecture (no separate cache, queue, or server)

### Less Suitable For

- **Batch analytics**: Optimized for OLTP, not OLAP
- **Large blob storage**: Better suited for structured relational data
- **Stateless APIs**: Traditional REST APIs do not need real-time sync

## Common Patterns

**Authentication check in reducer:**
```rust
#[spacetimedb::reducer]
fn admin_action(ctx: &ReducerContext) -> Result<(), String> {
    let admin = ctx.db.admin().identity().find(&ctx.sender())
        .ok_or("Not an admin")?;
    Ok(())
}
```

**Scheduled reducer:**
```rust
#[spacetimedb::table(accessor = reminder, scheduled(send_reminder))]
pub struct Reminder {
    #[primary_key]
    #[auto_inc]
    id: u64,
    scheduled_at: ScheduleAt,
    message: String,
}

#[spacetimedb::reducer]
fn send_reminder(ctx: &ReducerContext, reminder: Reminder) {
    log::info!("Reminder: {}", reminder.message);
}
```

---

## Editing Behavior

When modifying SpacetimeDB code:

- Make the smallest change necessary
- Do NOT touch unrelated files, configs, or dependencies
- Do NOT invent new SpacetimeDB APIs — use only what exists in docs or this repo
