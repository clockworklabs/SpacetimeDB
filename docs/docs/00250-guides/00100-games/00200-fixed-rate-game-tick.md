---
title: Run a fixed-rate game tick
slug: /guides/games/fixed-rate-game-tick
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
You need a module you can publish. If you don't have one yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.
:::

In this guide, you'll run your game's server logic at a fixed rate: a reducer that SpacetimeDB calls 20 times per second, which moves every entity in the world. You'll use a [schedule table](../../00200-core-concepts/00300-tables/00500-schedule-tables.md) to call the reducer and the `init` [lifecycle reducer](../../00200-core-concepts/00200-functions/00300-reducers/00500-lifecycle.md) to start it.

## How it works

- A `tick_timer` schedule table holds one row that tells SpacetimeDB to call the `tick` reducer every 50 milliseconds.
- The `init` reducer inserts that row when the database is created.
- The `tick` reducer measures how much time has passed since the last tick, then moves each entity by its velocity.

## Define the tables

Add three tables:

- `tick_timer` is the schedule table. Its `scheduled_at` column says when to call `tick`.
- `tick_state` is a private table with a single row that stores when the last tick ran.
- `entity` holds the things your tick moves. It's public, so clients can subscribe to it.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { ScheduleAt } from 'spacetimedb';
import { schema, table, t, type InferSchema, type ReducerCtx } from 'spacetimedb/server';

// 50 ms between ticks: 20 ticks per second.
const TICK_INTERVAL_MICROS = 50_000n;

const tickTimer = table(
  { name: 'tick_timer' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

const tickState = table(
  { name: 'tick_state' },
  {
    id: t.u8().primaryKey(),
    lastTick: t.timestamp(),
  }
);

const entity = table(
  { name: 'entity', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    x: t.f32(),
    y: t.f32(),
    velocityX: t.f32(),
    velocityY: t.f32(),
  }
);

const spacetimedb = schema({ tickTimer, tickState, entity });
export default spacetimedb;

type Ctx = ReducerCtx<InferSchema<typeof spacetimedb>>;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    // 50 ms between ticks: 20 ticks per second.
    static readonly TimeSpan TickInterval = TimeSpan.FromMilliseconds(50);

    [SpacetimeDB.Table(Accessor = "TickTimer", Scheduled = "Tick", ScheduledAt = "ScheduledAt")]
    public partial struct TickTimer
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
    }

    [SpacetimeDB.Table(Accessor = "TickState")]
    public partial struct TickState
    {
        [SpacetimeDB.PrimaryKey]
        public byte Id;
        public Timestamp LastTick;
    }

    [SpacetimeDB.Table(Accessor = "Entity", Public = true)]
    public partial struct Entity
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        public float X;
        public float Y;
        public float VelocityX;
        public float VelocityY;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{reducer, table, ReducerContext, ScheduleAt, Table, Timestamp};
use std::time::Duration;

// 50 ms between ticks: 20 ticks per second.
const TICK_INTERVAL: Duration = Duration::from_millis(50);

#[table(accessor = tick_timer, scheduled(tick))]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[table(accessor = tick_state)]
pub struct TickState {
    #[primary_key]
    id: u8,
    last_tick: Timestamp,
}

#[table(accessor = entity, public)]
pub struct Entity {
    #[primary_key]
    #[auto_inc]
    id: u64,
    x: f32,
    y: f32,
    velocity_x: f32,
    velocity_y: f32,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

// 50 ms between ticks: 20 ticks per second.
const TimeDuration TICK_INTERVAL = TimeDuration::from_millis(50);

struct TickTimer {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
};
SPACETIMEDB_STRUCT(TickTimer, scheduled_id, scheduled_at)
SPACETIMEDB_TABLE(TickTimer, tick_timer, Private)
FIELD_PrimaryKeyAutoInc(tick_timer, scheduled_id)
SPACETIMEDB_SCHEDULE(tick_timer, 1, tick)  // Column 1 is scheduled_at

struct TickState {
    uint8_t id;
    Timestamp last_tick;
};
SPACETIMEDB_STRUCT(TickState, id, last_tick)
SPACETIMEDB_TABLE(TickState, tick_state, Private)
FIELD_PrimaryKey(tick_state, id)

struct Entity {
    uint64_t id;
    float x;
    float y;
    float velocity_x;
    float velocity_y;
};
SPACETIMEDB_STRUCT(Entity, id, x, y, velocity_x, velocity_y)
SPACETIMEDB_TABLE(Entity, entity, Public)
FIELD_PrimaryKeyAutoInc(entity, id)
```

</TabItem>
</Tabs>

## Start the tick

Insert the timer row and the tick state from the `init` reducer. SpacetimeDB then calls `tick` every 50 milliseconds, until the row is deleted.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const init = spacetimedb.init(ctx => {
  ctx.db.tickState.insert({ id: 0, lastTick: ctx.timestamp });
  ctx.db.tickTimer.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(TICK_INTERVAL_MICROS),
  });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        ctx.Db.TickState.Insert(new TickState { Id = 0, LastTick = ctx.Timestamp });
        ctx.Db.TickTimer.Insert(new TickTimer
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Interval(TickInterval),
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.tick_state().insert(TickState {
        id: 0,
        last_tick: ctx.timestamp,
    });
    ctx.db.tick_timer().insert(TickTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(TICK_INTERVAL.into()),
    });
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_INIT(init, ReducerContext ctx) {
    ctx.db[tick_state].insert(TickState{0, ctx.timestamp});
    ctx.db[tick_timer].insert(TickTimer{0, ScheduleAt(TICK_INTERVAL)});
    return Ok();
}
```

</TabItem>
</Tabs>

:::note
`init` only runs when the database is created. Publishing new code to an existing database doesn't run it again, so the tick won't start there. To start it, either republish with `--delete-data`, which erases all data, or insert the timer row from another reducer. Before inserting, check that `tick_timer` is empty: with two timer rows, `tick` would run twice as often.
:::

## Write the tick reducer

The `tick` reducer does two things:

1. **It measures the time since the last tick**, rather than assuming exactly 50 milliseconds. This is standard practice for game loops: the time between two ticks varies slightly, and moving by the measured time keeps every entity at a steady speed.
2. **It moves each entity** by its velocity multiplied by that time. This is where your game logic goes.

Clients can't call `tick` themselves: scheduled reducers are private, so only the scheduler, the database owner and team collaborators can run them.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const tick = spacetimedb.reducer(
  { onSchedule: tickTimer },
  { arg: tickTimer.rowType },
  ctx => {
    const state = ctx.db.tickState.id.find(0);
    if (!state) {
      throw new Error('tick state missing');
    }
    // Seconds since the last tick.
    const dt = Number(ctx.timestamp.since(state.lastTick).micros) / 1_000_000;
    ctx.db.tickState.id.update({ ...state, lastTick: ctx.timestamp });

    for (const e of ctx.db.entity.iter()) {
      if (e.velocityX === 0 && e.velocityY === 0) {
        continue;
      }
      ctx.db.entity.id.update({
        ...e,
        x: e.x + e.velocityX * dt,
        y: e.y + e.velocityY * dt,
      });
    }
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void Tick(ReducerContext ctx, TickTimer timer)
    {
        var state = ctx.Db.TickState.Id.Find(0) ?? throw new Exception("Tick state missing");
        // Seconds since the last tick.
        var dt = ctx.Timestamp.TimeDurationSince(state.LastTick).Microseconds / 1_000_000f;
        state.LastTick = ctx.Timestamp;
        ctx.Db.TickState.Id.Update(state);

        foreach (var entity in ctx.Db.Entity.Iter().ToList())
        {
            if (entity.VelocityX == 0 && entity.VelocityY == 0)
            {
                continue;
            }
            var moved = entity;
            moved.X += entity.VelocityX * dt;
            moved.Y += entity.VelocityY * dt;
            ctx.Db.Entity.Id.Update(moved);
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn tick(ctx: &ReducerContext, _timer: TickTimer) -> Result<(), String> {
    let mut state = ctx.db.tick_state().id().find(0).ok_or("tick state missing")?;
    // Seconds since the last tick.
    let dt = ctx
        .timestamp
        .duration_since(state.last_tick)
        .unwrap_or_default()
        .as_secs_f32();
    state.last_tick = ctx.timestamp;
    ctx.db.tick_state().id().update(state);

    for mut entity in ctx.db.entity().iter() {
        if entity.velocity_x == 0.0 && entity.velocity_y == 0.0 {
            continue;
        }
        entity.x += entity.velocity_x * dt;
        entity.y += entity.velocity_y * dt;
        ctx.db.entity().id().update(entity);
    }
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_REDUCER(tick, ReducerContext ctx, TickTimer timer) {
    auto state = ctx.db[tick_state_id].find(uint8_t(0));
    if (!state) {
        return Err("tick state missing");
    }
    // Seconds since the last tick.
    float dt = (ctx.timestamp.micros_since_epoch() - state->last_tick.micros_since_epoch()) / 1'000'000.0f;
    TickState updated_state = *state;
    updated_state.last_tick = ctx.timestamp;
    ctx.db[tick_state_id].update(updated_state);

    for (auto e : ctx.db[entity]) {
        if (e.velocity_x == 0.0f && e.velocity_y == 0.0f) {
            continue;
        }
        e.x += e.velocity_x * dt;
        e.y += e.velocity_y * dt;
        ctx.db[entity_id].update(e);
    }
    return Ok();
}
```

</TabItem>
</Tabs>

## Stop the tick

The tick runs until its timer row is deleted. To stop it, for example when a match ends, delete the row from your game logic:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
function stopTick(ctx: Ctx) {
  for (const timer of ctx.db.tickTimer.iter()) {
    ctx.db.tickTimer.scheduledId.delete(timer.scheduledId);
  }
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    static void StopTick(ReducerContext ctx)
    {
        foreach (var timer in ctx.Db.TickTimer.Iter().ToList())
        {
            ctx.Db.TickTimer.ScheduledId.Delete(timer.ScheduledId);
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
fn stop_tick(ctx: &ReducerContext) {
    for timer in ctx.db.tick_timer().iter() {
        ctx.db.tick_timer().delete(timer);
    }
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
void stop_tick(ReducerContext& ctx) {
    for (auto timer : ctx.db[tick_timer]) {
        ctx.db[tick_timer_scheduled_id].delete_by_key(timer.scheduled_id);
    }
}
```

</TabItem>
</Tabs>

To start it again, insert a new timer row as in `init`, after checking that none exists.

## Keep the tick fast

- **Finish well within the interval.** A tick that takes longer than 50 milliseconds delays everything else in the database, and the ticks it overlaps are skipped. Keep expensive work, like pathfinding or AI decisions, out of the tick, or spread it across several ticks.
- **Only write what changed.** Every update to a public table is sent to the clients subscribed to it, so a tick that rewrites every row sends every row to every client 20 times per second. Skipping entities that aren't moving, as the reducer above does, avoids that. To also limit which entities each client receives, see [Only sync what's near the player](./00500-sync-nearby-entities.md).

## What's next?

You now have server logic that runs 20 times per second:

- A schedule table calls the `tick` reducer at a fixed interval, starting from `init`.
- The reducer moves entities by the measured time since the last tick, so they move at a steady speed.
- Deleting the timer row stops the tick.

To learn more about the features used here, see [Schedule Tables](../../00200-core-concepts/00300-tables/00500-schedule-tables.md) and [Lifecycle Reducers](../../00200-core-concepts/00200-functions/00300-reducers/00500-lifecycle.md).

Next, you can [build a matchmaking queue](./00300-matchmaking-queue.md).
