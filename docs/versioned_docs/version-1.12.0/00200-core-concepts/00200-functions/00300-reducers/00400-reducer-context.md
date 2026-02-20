---
title: Reducer Context
slug: /functions/reducers/reducer-context
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Every reducer receives a special context parameter as its first argument. This context provides read-write access to the database, information about the caller, and additional utilities like random number generation.

The reducer context is required for accessing tables, executing database operations, and retrieving metadata about the current reducer invocation.

## Accessing the Database

The primary purpose of the reducer context is to provide access to the module's database tables.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema, table, t } from 'spacetimedb/server';

const user = table(
  { name: 'user', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
  }
);

const spacetimedb = schema(user);

spacetimedb.reducer('create_user', { name: t.string() }, (ctx, { name }) => {
  ctx.db.user.insert({ id: 0n, name });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table]
    public partial struct User
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong id;
        public string name;
    }

    [SpacetimeDB.Reducer]
    public static void CreateUser(ReducerContext ctx, string name)
    {
        ctx.Db.User.Insert(new User { id = 0, name = name });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{table, reducer, ReducerContext};

#[table(name = user)]
pub struct User {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
}

#[reducer]
fn create_user(ctx: &ReducerContext, name: String) {
    ctx.db.user().insert(User { id: 0, name });
}
```

</TabItem>
</Tabs>

## Caller Information

The context provides information about who invoked the reducer and when.

### Sender Identity

Every reducer invocation has an associated caller identity.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema, table, t, type Identity } from 'spacetimedb/server';

const player = table(
  { name: 'player', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    score: t.u32(),
  }
);

const spacetimedb = schema(player);

spacetimedb.reducer('update_score', { newScore: t.u32() }, (ctx, { newScore }) => {
  // Get the caller's identity
  const caller = ctx.sender;
  
  // Find and update their player record
  const existingPlayer = ctx.db.player.identity.find(caller);
  if (existingPlayer) {
    ctx.db.player.identity.update({
      ...existingPlayer,
      score: newScore,
    });
  }
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table]
    public partial struct Player
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;
        public string Name;
        public uint Score;
    }

    [SpacetimeDB.Reducer]
    public static void UpdateScore(ReducerContext ctx, uint newScore)
    {
        // Get the caller's identity
        Identity caller = ctx.Sender;
        
        // Find and update their player record
        if (ctx.Db.Player.Identity.Find(caller) is Player player)
        {
            player.Score = newScore;
            ctx.Db.Player.Identity.Update(player);
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{table, reducer, ReducerContext, Identity};

#[table(name = player)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    name: String,
    score: u32,
}

#[reducer]
fn update_score(ctx: &ReducerContext, new_score: u32) {
    // Get the caller's identity
    let caller = ctx.sender;
    
    // Find and update their player record
    if let Some(mut player) = ctx.db.player().identity().find(caller) {
        player.score = new_score;
        ctx.db.player().identity().update(player);
    }
}
```

</TabItem>
</Tabs>

### Connection ID

The connection ID identifies the specific client connection that invoked the reducer. This is useful for tracking sessions or implementing per-connection state.

:::note
The connection ID may be `None`/`null`/`undefined` for reducers invoked by the system (such as scheduled reducers or lifecycle reducers) or when called via the CLI without specifying a connection.
:::

### Timestamp

The timestamp indicates when the reducer was invoked. This value is consistent throughout the reducer execution and is useful for timestamping events or implementing time-based logic.

## Random Number Generation

The context provides access to a random number generator that is deterministic and reproducible. This ensures that reducer execution is consistent across all nodes in a distributed system.

:::warning
Never use external random number generators (like `Math.random()` in TypeScript or `Random` in C# without using the context). These are non-deterministic and will cause different nodes to produce different results, breaking consensus.
:::

## Module Identity

The context provides access to the module's own identity, which is useful for distinguishing between user-initiated and system-initiated reducer calls.

This is particularly important for [scheduled reducers](/functions/reducers) that should only be invoked by the system, not by external clients.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema, table, t, SenderError } from 'spacetimedb/server';

const scheduledTask = table(
  { name: 'scheduled_task', scheduled: 'send_reminder' },
  {
    taskId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    message: t.string(),
  }
);

const spacetimedb = schema(scheduledTask);

spacetimedb.reducer('send_reminder', { arg: scheduledTask.rowType }, (ctx, { arg }) => {
  // Only allow the scheduler (module identity) to call this
  if (ctx.sender != ctx.identity) {
    throw new SenderError('This reducer can only be called by the scheduler');
  }
  
  console.log(`Reminder: ${arg.message}`);
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Name = "ScheduledTask", Scheduled = nameof(SendReminder))]
    public partial struct ScheduledTask
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong taskId;
        public ScheduleAt scheduledAt;
        public string message;
    }

    [SpacetimeDB.Reducer]
    public static void SendReminder(ReducerContext ctx, ScheduledTask task)
    {
        // Only allow the scheduler (module identity) to call this
        if (ctx.Sender != ctx.Identity)
        {
            throw new Exception("This reducer can only be called by the scheduler");
        }
        
        Log.Info($"Reminder: {task.message}");
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{table, reducer, ReducerContext, ScheduleAt};

#[table(name = scheduled_task, scheduled(send_reminder))]
pub struct ScheduledTask {
    #[primary_key]
    #[auto_inc]
    task_id: u64,
    scheduled_at: ScheduleAt,
    message: String,
}

#[reducer]
fn send_reminder(ctx: &ReducerContext, task: ScheduledTask) {
    // Only allow the scheduler (module identity) to call this
    if ctx.sender != ctx.identity() {
        panic!("This reducer can only be called by the scheduler");
    }
    
    spacetimedb::log::info!("Reminder: {}", task.message);
}
```

</TabItem>
</Tabs>

## Context Properties Reference

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

| Property       | Type                       | Description                                     |
| -------------- | -------------------------- | ----------------------------------------------- |
| `db`           | `DbView`                   | Access to the module's database tables          |
| `sender`       | `Identity`                 | Identity of the caller                          |
| `senderAuth`   | `AuthCtx`                  | Authorization context for the caller (includes JWT claims and internal call detection) |
| `connectionId` | `ConnectionId \| undefined`| Connection ID of the caller, if available       |
| `timestamp`    | `Timestamp`                | Time when the reducer was invoked               |

:::note
TypeScript uses `Math.random()` for random number generation, which is automatically seeded deterministically by SpacetimeDB.
:::
</TabItem>
<TabItem value="csharp" label="C#">

| Property       | Type                  | Description                                     |
| -------------- | --------------------- | ----------------------------------------------- |
| `Db`           | `DbView`              | Access to the module's database tables          |
| `Sender`       | `Identity`            | Identity of the caller                          |
| `SenderAuth`   | `AuthCtx`             | Authorization context for the caller (includes JWT claims and internal call detection) |
| `ConnectionId` | `ConnectionId?`       | Connection ID of the caller, if available       |
| `Timestamp`    | `Timestamp`           | Time when the reducer was invoked               |
| `Rng`          | `Random`              | Random number generator                         |
| `Identity`     | `Identity`            | The module's identity                           |
</TabItem>
<TabItem value="rust" label="Rust">

| Property        | Type                  | Description                                     |
| --------------- | --------------------- | ----------------------------------------------- |
| `db`            | `Local`               | Access to the module's database tables          |
| `sender`        | `Identity`            | Identity of the caller                          |
| `connection_id` | `Option<ConnectionId>`| Connection ID of the caller, if available       |
| `timestamp`     | `Timestamp`           | Time when the reducer was invoked               |

**Methods:**

- `identity() -> Identity` - Get the module's identity
- `rng() -> &StdbRng` - Get the random number generator
- `random<T>() -> T` - Generate a single random value
- `sender_auth() -> &AuthCtx` - Get authorization context for the caller (includes JWT claims and internal call detection)
</TabItem>
</Tabs>
