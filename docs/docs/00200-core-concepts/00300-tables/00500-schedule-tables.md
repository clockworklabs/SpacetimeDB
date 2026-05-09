---
title: Schedule Tables
slug: /tables/schedule-tables
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";


Tables can trigger [reducers](../00200-functions/00300-reducers/00300-reducers.md) or [procedures](../00200-functions/00400-procedures.md) at specific times by including a special scheduling column. This allows you to schedule future actions like sending reminders, expiring items, or running periodic maintenance tasks.

:::tip Scheduling Procedures
Procedures use the same scheduling pattern as reducers. Simply reference the procedure name in the `scheduled` attribute. This is particularly useful when you need scheduled tasks that make HTTP requests or perform other side effects. See [Scheduling Procedures](../00200-functions/00300-reducers/00300-reducers.md#scheduling-procedures) for an example.
:::

## Defining a Schedule Table

:::note Why "scheduled" in the code?
The table attribute uses `scheduled` (with a "d") because it refers to the **scheduled reducer** - the function that will be scheduled for execution. The table itself is a "schedule table" that stores schedules, while the reducer it triggers is a "scheduled reducer".
:::

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const reminder = table(
  { name: 'reminder', scheduled: (): any => send_reminder },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
    message: t.string(),
  }
);

export const send_reminder = spacetimedb.reducer({ arg: reminder.rowType }, (_ctx, { arg }) => {
  // Invoked automatically by the scheduler
  // arg.message, arg.scheduled_at, arg.scheduled_id
});
```

</TabItem>
<TabItem value="csharp" label="C#">

:::tip C# schedule column
In `[SpacetimeDB.Table(..., ScheduledAt = "...")]`, the value must exactly match the name of a field on that table whose type is `ScheduleAt` (for example, `"ScheduledAt"` or `"scheduled_at"`).
:::

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "Reminder", Scheduled = "SendReminder", ScheduledAt = "ScheduledAt")]
    public partial struct Reminder
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        public uint UserId;
        public string Message;
        public ScheduleAt ScheduledAt;
    }

    [SpacetimeDB.Reducer]
    public static void SendReminder(ReducerContext ctx, Reminder reminder)
    {
        // Process the scheduled reminder
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{reducer, table, ReducerContext, ScheduleAt, Table};
use std::time::Duration;

#[table(accessor = reminder_schedule, scheduled(send_reminder))]
pub struct Reminder {
    #[primary_key]
    #[auto_inc]
    id: u64,
    user_id: u32,
    message: String,
    scheduled_at: ScheduleAt,
}

#[reducer]
fn send_reminder(ctx: &ReducerContext, reminder: Reminder) -> Result<(), String> {
    // Process the scheduled reminder
    Ok(())
}

#[reducer(init)]
fn init(ctx: &ReducerContext) {
    ctx.db.reminder_schedule().insert(Reminder {
        id: 0,
        user_id: 0,
        message: "Game tick".to_string(),
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(50).into()),
    });
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
struct Reminder {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
    std::string message;
};
SPACETIMEDB_STRUCT(Reminder, scheduled_id, scheduled_at, message)
SPACETIMEDB_TABLE(Reminder, reminder, Public)
FIELD_PrimaryKeyAutoInc(reminder, scheduled_id)
SPACETIMEDB_SCHEDULE(reminder, 1, send_reminder)  // Column 1 is scheduled_at

// Reducer invoked automatically by the scheduler
SPACETIMEDB_REDUCER(send_reminder, ReducerContext ctx, Reminder arg)
{
    // Invoked automatically by the scheduler
    // arg.message, arg.scheduled_at, arg.scheduled_id
    LOG_INFO("Scheduled reminder: " + arg.message);
    return Ok();
}
```

</TabItem>
</Tabs>

## Inserting Schedules

To schedule an action, insert a row into the schedule table with a `scheduled_at` value. You can schedule actions to run:

- **At intervals** - Execute repeatedly at fixed time intervals (e.g., every 5 seconds)
- **At specific times** - Execute once at an absolute timestamp

### Scheduling at Intervals

Use intervals for periodic tasks like game ticks, heartbeats, or recurring maintenance:

:::important TypeScript: ScheduleAt import
`ScheduleAt` is imported from `'spacetimedb'`, **not** from `'spacetimedb/server'`. Use: `import { ScheduleAt } from 'spacetimedb';`
:::

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { ScheduleAt } from 'spacetimedb';
import { schema } from 'spacetimedb/server';
const spacetimedb = schema({ reminder }); // reminder table defined above
export default spacetimedb;

export const schedule_periodic_tasks = spacetimedb.reducer((ctx) => {
  // Schedule to run every 5 seconds (5,000,000 microseconds)
  ctx.db.reminder.insert({
    scheduled_id: 0n,
    scheduled_at: ScheduleAt.interval(5_000_000n),
    message: "Check for updates",
  });

  // Schedule to run every 100 milliseconds
  ctx.db.reminder.insert({
    scheduled_id: 0n,
    scheduled_at: ScheduleAt.interval(100_000n), // 100ms in microseconds
    message: "Game tick",
  });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public partial class Module
{
    [SpacetimeDB.Reducer]
    public static void SchedulePeriodicTasks(ReducerContext ctx)
    {
        // Schedule to run every 5 seconds
        ctx.Db.Reminder.Insert(new Reminder
        {
            Message = "Check for updates",
            ScheduledAt = new ScheduleAt.Interval(TimeSpan.FromSeconds(5))
        });

        // Schedule to run every 100 milliseconds
        ctx.Db.Reminder.Insert(new Reminder
        {
            Message = "Game tick",
            ScheduledAt = new ScheduleAt.Interval(TimeSpan.FromMilliseconds(100))
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{ScheduleAt, ReducerContext, Table};
use std::time::Duration;

#[spacetimedb::reducer]
fn schedule_periodic_tasks(ctx: &ReducerContext) {
    // Schedule to run every 5 seconds
    ctx.db.reminder().insert(Reminder {
        id: 0,
        message: "Check for updates".to_string(),
        scheduled_at: ScheduleAt::Interval(Duration::from_secs(5).into()),
    });

    // Schedule to run every 100 milliseconds
    ctx.db.reminder().insert(Reminder {
        id: 0,
        message: "Game tick".to_string(),
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(100).into()),
    });
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// Schedule to run every 5 seconds
ctx.db[reminder].insert(Reminder{
    0,
    ScheduleAt::interval(TimeDuration::from_seconds(5)),
    "Check for updates"
});

// Schedule to run every 100 milliseconds
ctx.db[reminder].insert(Reminder{
    0,
    ScheduleAt::interval(TimeDuration::from_millis(100)),
    "Game tick"
});
```

</TabItem>
</Tabs>

### Scheduling at Specific Times

Use specific times for one-shot actions like sending a reminder at a particular moment or expiring content:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { ScheduleAt } from 'spacetimedb';
import { schema } from 'spacetimedb/server';
const spacetimedb = schema({ reminder }); // reminder table defined above
export default spacetimedb;

export const schedule_timed_tasks = spacetimedb.reducer((ctx) => {
  // Schedule for 10 seconds from now
  const tenSecondsFromNow = ctx.timestamp.microsSinceUnixEpoch + 10_000_000n;
  ctx.db.reminder.insert({
    scheduled_id: 0n,
    scheduled_at: ScheduleAt.time(tenSecondsFromNow),
    message: "Your auction has ended",
  });

  // Schedule for a specific Unix timestamp (microseconds since epoch)
  const targetTime = 1735689600_000_000n; // Jan 1, 2025 00:00:00 UTC
  ctx.db.reminder.insert({
    scheduled_id: 0n,
    scheduled_at: ScheduleAt.time(targetTime),
    message: "Happy New Year!",
  });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void ScheduleTimedTasks(ReducerContext ctx)
    {
        // Schedule for 10 seconds from now
        ctx.Db.Reminder.Insert(new Reminder
        {
            Message = "Your auction has ended",
            ScheduledAt = new ScheduleAt.Time(DateTimeOffset.UtcNow.AddSeconds(10))
        });

        // Schedule for a specific time
        var targetTime = new DateTimeOffset(2025, 1, 1, 0, 0, 0, TimeSpan.Zero);
        ctx.Db.Reminder.Insert(new Reminder
        {
            Message = "Happy New Year!",
            ScheduledAt = new ScheduleAt.Time(targetTime)
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{ScheduleAt, ReducerContext, Table};
use std::time::Duration;

#[spacetimedb::reducer]
fn schedule_timed_tasks(ctx: &ReducerContext) {
    // Schedule for 10 seconds from now
    let ten_seconds_from_now = ctx.timestamp + Duration::from_secs(10);
    ctx.db.reminder().insert(Reminder {
        id: 0,
        message: "Your auction has ended".to_string(),
        scheduled_at: ScheduleAt::Time(ten_seconds_from_now),
    });

    // Schedule for immediate execution (current timestamp)
    ctx.db.reminder().insert(Reminder {
        id: 0,
        message: "Process now".to_string(),
        scheduled_at: ScheduleAt::Time(ctx.timestamp.clone()),
    });
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// Schedule for 10 seconds from now
Timestamp tenSecondsFromNow = ctx.timestamp + TimeDuration::from_seconds(10);
ctx.db[reminder].insert(Reminder{
    0,
    ScheduleAt::time(tenSecondsFromNow),
    "Your auction has ended"
});

// Schedule for immediate execution (current timestamp)
ctx.db[reminder].insert(Reminder{
    0,
    ScheduleAt::time(ctx.timestamp),
    "Process now"
});
```

</TabItem>
</Tabs>

## How It Works

1. **Insert a row** with a `ScheduleAt` value
2. **SpacetimeDB monitors** the schedule table
3. **When the time arrives**, the specified reducer/procedure is automatically called with the row as a parameter
4. **The row is typically deleted** or updated by the reducer after processing

## Use Cases

- **Reminders and notifications** - Schedule messages to be sent at specific times
- **Expiring content** - Automatically remove or archive old data
- **Delayed actions** - Queue up actions to execute after a delay
- **Periodic tasks** - Schedule repeating maintenance or cleanup operations
- **Game mechanics** - Timer-based gameplay events (building completion, energy regeneration, etc.)

## Next Steps

- Learn about [Reducers](../00200-functions/00300-reducers/00300-reducers.md) to handle scheduled actions
- Explore [Procedures](../00200-functions/00400-procedures.md) for scheduled execution patterns
