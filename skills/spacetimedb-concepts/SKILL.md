---
name: spacetimedb-concepts
description: Understand SpacetimeDB architecture and core concepts. Use when learning SpacetimeDB or making architectural decisions.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "1.1"
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
5. **`ctx.sender` is the authenticated principal** — never trust identity passed as arguments. Always use `ctx.sender` for authorization.

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
# Start local SpacetimeDB
spacetime start

# Publish module
spacetime publish <db-name> --project-path <module-path>

# Clear and republish
spacetime publish <db-name> --clear-database -y --project-path <module-path>

# Generate client bindings
spacetime generate --lang <lang> --out-dir <out> --project-path <module-path>

# View logs
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

SpacetimeDB powers BitCraft Online, an MMORPG where the entire game backend (chat, items, resources, terrain, player positions) runs as a single SpacetimeDB module.

## The Five Zen Principles

SpacetimeDB is built on five core principles that guide both development and usage:

1. **Everything is a Table**: Your entire application state lives in tables. No separate cache layer, no Redis, no in-memory state to synchronize. The database IS your state.

2. **Everything is Persistent**: SpacetimeDB persists everything by default, including full history. Persistence only increases latency, never decreases throughput. Modern SSDs can write 15+ GB/s.

3. **Everything is Real-Time**: Clients are replicas of server state. Subscribe to data and it flows automatically. No polling, no fetching.

4. **Everything is Transactional**: Every reducer runs atomically. Either all changes succeed or all roll back. No partial updates, no corrupted state.

5. **Everything is Programmable**: Modules are real code (Rust, C#, TypeScript) running inside the database. Full Turing-complete power for any logic.

## Tables

Tables store all data in SpacetimeDB. They use the relational model and support SQL queries for subscriptions.

### Defining Tables

Tables are defined using language-specific attributes:

**Rust:**
```rust
#[spacetimedb::table(name = player, public)]
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
[SpacetimeDB.Table(Name = "Player", Public = true)]
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
                velocity_y
```

Benefits:
- Reduced bandwidth (clients subscribing to positions do not receive settings updates)
- Cache efficiency (similar update frequencies in contiguous memory)
- Schema evolution (add columns without affecting other tables)
- Semantic clarity (each table has single responsibility)

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
- `ctx.sender`: The Identity of the caller (use this for authorization, never trust args)
- `ctx.connection_id`: The connection ID of the caller
- `ctx.timestamp`: The current timestamp

## Subscriptions

Subscriptions replicate database rows to clients in real-time. When you subscribe to a query, SpacetimeDB sends matching rows immediately and pushes updates whenever those rows change.

### How Subscriptions Work

1. **Subscribe**: Register SQL queries describing needed data
2. **Receive initial data**: All matching rows are sent immediately
3. **Receive updates**: Real-time updates when subscribed rows change
4. **React to changes**: Use callbacks (`onInsert`, `onDelete`, `onUpdate`) to handle changes

### Client-Side Usage

**TypeScript:**
```typescript
const conn = DbConnection.builder()
  .withUri('wss://maincloud.spacetimedb.com')
  .withModuleName('my_module')
  .onConnect((ctx) => {
    ctx.subscriptionBuilder()
      .onApplied(() => console.log('Subscription ready!'))
      .subscribe(['SELECT * FROM user', 'SELECT * FROM message']);
  })
  .build();

// React to changes
conn.db.user.onInsert((ctx, user) => console.log(`New user: ${user.name}`));
conn.db.user.onDelete((ctx, user) => console.log(`User left: ${user.name}`));
conn.db.user.onUpdate((ctx, old, new_) => console.log(`${old.name} -> ${new_.name}`));
```

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
- **Procedures**: (Beta) Functions that can have side effects (HTTP requests)

### Module Languages

Server-side modules can be written in:
- Rust
- C#
- TypeScript (beta)

### Module Lifecycle

1. **Write**: Define tables and reducers in your chosen language
2. **Compile**: Build to WebAssembly using the SpacetimeDB CLI
3. **Publish**: Upload to a SpacetimeDB host with `spacetime publish`
4. **Hot-swap**: Republish to update code without disconnecting clients

## Identity

Identity is SpacetimeDB's authentication system based on OpenID Connect (OIDC).

### Identity Concepts

- **Identity**: A long-lived, globally unique identifier for a user. Derived from OIDC issuer and subject claims.
- **ConnectionId**: Identifies a specific client connection. A user may have multiple connections.

### Identity in Reducers

```rust
#[spacetimedb::reducer]
pub fn do_something(ctx: &ReducerContext) {
    let caller_identity = ctx.sender;  // Who is calling this reducer?
    // Use identity for authorization checks
    // NEVER trust identity passed as a reducer argument
}
```

### Authentication Providers

SpacetimeDB works with any OIDC provider:
- **SpacetimeAuth**: Built-in managed provider (simple, production-ready)
- **Third-party**: Auth0, Clerk, Keycloak, Google, GitHub, etc.

## SATS (SpacetimeDB Algebraic Type System)

SATS is the type system and serialization format used throughout SpacetimeDB.

### Core Types

| Category | Types |
|----------|-------|
| Primitives | `Bool`, `U8`-`U256`, `I8`-`I256`, `F32`, `F64`, `String` |
| Composite | `ProductType` (structs), `SumType` (enums/tagged unions) |
| Collections | `Array`, `Map` |
| Special | `Identity`, `ConnectionId`, `ScheduleAt` |

### Serialization Formats

- **BSATN**: Binary format for module-host communication and row storage
- **SATS-JSON**: JSON format for HTTP API and WebSocket text protocol

### Type Compatibility

Types must implement `SpacetimeType` to be used in tables and reducers. This is automatic for primitive types and structs using the appropriate attributes.

## Client-Server Data Flow

### Write Path (Client to Database)

1. Client calls reducer (e.g., `ctx.reducers.createUser("Alice")`)
2. Request sent over WebSocket to SpacetimeDB host
3. Host validates identity and executes reducer in transaction
4. On success, changes are committed; on error, all changes roll back
5. Subscribed clients receive updates for affected rows

### Read Path (Database to Client)

1. Client subscribes with SQL queries (e.g., `SELECT * FROM user`)
2. Server evaluates query and sends matching rows
3. Client maintains local cache of subscribed data
4. When subscribed data changes, server pushes delta updates
5. Client cache is automatically updated; callbacks fire

### Data Flow Diagram

```
┌─────────────────────────────────────────────────────────┐
│                        CLIENT                           │
│  ┌─────────────┐     ┌─────────────────────────────┐   │
│  │  Reducers   │────>│     Local Cache (Read)      │   │
│  │  (Write)    │     │  - Tables from subscriptions│   │
│  └─────────────┘     │  - Automatically synced     │   │
│         │            └─────────────────────────────┘   │
└─────────│──────────────────────────│───────────────────┘
          │ WebSocket                │ Updates pushed
          v                          │
┌─────────────────────────────────────────────────────────┐
│                     SpacetimeDB                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │                    Module                        │   │
│  │  - Reducers (transactional logic)               │   │
│  │  - Tables (in-memory + persisted)               │   │
│  │  - Subscriptions (real-time queries)            │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

## When to Use SpacetimeDB

### Ideal Use Cases

- **Real-time games**: MMOs, multiplayer games, turn-based games
- **Collaborative applications**: Document editing, whiteboards, design tools
- **Chat and messaging**: Real-time communication with presence
- **Live dashboards**: Streaming analytics and monitoring
- **IoT applications**: Sensor data with real-time updates

### Key Decision Factors

Choose SpacetimeDB when you need:
- Sub-10ms latency for reads and writes
- Automatic real-time synchronization
- Transactional guarantees for all operations
- Simplified architecture (no separate cache, queue, or server)

### Less Suitable For

- **Batch analytics**: SpacetimeDB is optimized for OLTP, not OLAP
- **Large blob storage**: Better suited for structured relational data
- **Stateless APIs**: Traditional REST APIs do not need real-time sync

## Comparison to Traditional Architectures

### Traditional Stack

```
Client
   │
   v
Load Balancer
   │
   v
Web/Game Servers (stateless or stateful)
   │
   ├──> Cache (Redis)
   │
   v
Database (PostgreSQL, MySQL)
   │
   v
Message Queue (for real-time)
```

**Pain points:**
- Multiple systems to deploy and manage
- Cache invalidation complexity
- State synchronization between servers
- Manual real-time implementation
- Horizontal scaling complexity

### SpacetimeDB Stack

```
Client
   │
   v
SpacetimeDB Host
   │
   v
Module (your logic + tables)
```

**Benefits:**
- Single deployment target
- No cache layer needed (in-memory by design)
- Automatic real-time synchronization
- Built-in horizontal scaling (future)
- Transactional guarantees everywhere

### Smart Contract Comparison

SpacetimeDB modules are conceptually similar to smart contracts:
- Application logic runs inside the data layer
- Transactions are atomic and verified
- State changes are deterministic

Key differences:
- SpacetimeDB is orders of magnitude faster (no consensus overhead)
- Full relational database capabilities
- No blockchain or cryptocurrency involved
- Designed for real-time, not eventual consistency

## Common Patterns

**Authentication check in reducer:**
```rust
#[spacetimedb::reducer]
fn admin_action(ctx: &ReducerContext) -> Result<(), String> {
    let admin = ctx.db.admin().identity().find(&ctx.sender)
        .ok_or("Not an admin")?;
    // ... perform admin action
    Ok(())
}
```

**Moving between tables (state machine):**
```rust
#[spacetimedb::reducer]
fn login(ctx: &ReducerContext) -> Result<(), String> {
    let player = ctx.db.logged_out_player().identity().find(&ctx.sender)
        .ok_or("Not found")?;
    ctx.db.player().insert(player.clone());
    ctx.db.logged_out_player().identity().delete(&ctx.sender);
    Ok(())
}
```

**Scheduled reducer:**
```rust
#[spacetimedb::table(name = reminder, scheduled(send_reminder))]
pub struct Reminder {
    #[primary_key]
    #[auto_inc]
    id: u64,
    scheduled_at: ScheduleAt,
    message: String,
}

#[spacetimedb::reducer]
fn send_reminder(ctx: &ReducerContext, reminder: Reminder) {
    // This runs at the scheduled time
    log::info!("Reminder: {}", reminder.message);
}
```

---

## Editing Behavior

When modifying SpacetimeDB code:

- Make the smallest change necessary
- Do NOT touch unrelated files, configs, or dependencies
- Do NOT invent new SpacetimeDB APIs — use only what exists in docs or this repo
