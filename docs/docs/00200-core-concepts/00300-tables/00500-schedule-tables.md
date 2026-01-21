---
title: Schedule Tables
slug: /tables/schedule-tables
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Tables can trigger [reducers](/functions/reducers) or [procedures](/functions/procedures) at specific times by including a special scheduling column. This allows you to schedule future actions like sending reminders, expiring items, or running periodic maintenance tasks.

## Defining a Schedule Table

:::note Why "scheduled" in the code?
The table attribute uses `scheduled` (with a "d") because it refers to the **scheduled reducer** - the function that will be scheduled for execution. The table itself is a "schedule table" that stores schedules, while the reducer it triggers is a "scheduled reducer".
:::

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const reminder = table(
  { name: 'reminder', scheduled: 'send_reminder' },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
    message: t.string(),
  }
);

spacetimedb.reducer('send_reminder', { arg: reminder.rowType }, (_ctx, { arg }) => {
  // Invoked automatically by the scheduler
  // arg.message, arg.scheduled_at, arg.scheduled_id
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Scheduled = "SendReminder", ScheduledAt = "ScheduleAt")]
public partial struct Reminder
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    public uint UserId;
    public string Message;
    public ScheduleAt ScheduleAt;
}

[SpacetimeDB.Reducer()]
public static void SendReminder(ReducerContext ctx, Reminder reminder)
{
    // Process the scheduled reminder
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = reminder_schedule, scheduled(send_reminder))]
pub struct Reminder {
    #[primary_key]
    #[auto_inc]
    id: u64,
    user_id: u32,
    message: String,
    scheduled_at: ScheduleAt,
}

#[spacetimedb::reducer]
fn send_reminder(ctx: &ReducerContext, reminder: Reminder) -> Result<(), String> {
    // Process the scheduled reminder
    Ok(())
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

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { ScheduleAt } from 'spacetimedb';

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
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Schedule to run every 5 seconds
ctx.Db.Reminder.Insert(new Reminder
{
    Message = "Check for updates",
    ScheduleAt = new ScheduleAt.Interval(TimeSpan.FromSeconds(5))
});

// Schedule to run every 100 milliseconds
ctx.Db.Reminder.Insert(new Reminder
{
    Message = "Game tick",
    ScheduleAt = new ScheduleAt.Interval(TimeSpan.FromMilliseconds(100))
});
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{ScheduleAt, Duration};

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
```

</TabItem>
</Tabs>

### Scheduling at Specific Times

Use specific times for one-shot actions like sending a reminder at a particular moment or expiring content:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { ScheduleAt } from 'spacetimedb';

// Schedule for 10 seconds from now
const tenSecondsFromNow = ctx.timestamp.microseconds + 10_000_000n;
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
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Schedule for 10 seconds from now
ctx.Db.Reminder.Insert(new Reminder
{
    Message = "Your auction has ended",
    ScheduleAt = new ScheduleAt.Time(DateTimeOffset.UtcNow.AddSeconds(10))
});

// Schedule for a specific time
var targetTime = new DateTimeOffset(2025, 1, 1, 0, 0, 0, TimeSpan.Zero);
ctx.Db.Reminder.Insert(new Reminder
{
    Message = "Happy New Year!",
    ScheduleAt = new ScheduleAt.Time(targetTime)
});
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{ScheduleAt, Duration};

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
```

</TabItem>
</Tabs>

## How It Works

1. **Insert a row** with a `ScheduleAt` value
2. **SpacetimeDB monitors** the schedule table
3. **When the time arrives**, the specified reducer/procedure is automatically called with the row as a parameter
4. **The row is typically deleted** or updated by the reducer after processing

## Security Considerations

:::warning Scheduled Reducers Are Callable by Clients
Scheduled reducers are normal reducers that can also be invoked by external clients. If a scheduled reducer should only execute via the scheduler, add authentication checks.
:::

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
spacetimedb.reducer('send_reminder', { arg: Reminder.rowType }, (ctx, { arg }) => {
  if (!ctx.senderAuth.isInternal) {
    throw new SenderError('This reducer can only be called by the scheduler');
  }
  // Process the scheduled reminder
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Reducer()]
public static void SendReminder(ReducerContext ctx, Reminder reminder)
{
    if (!ctx.SenderAuth.IsInternal)
    {
        throw new Exception("This reducer can only be called by the scheduler");
    }
    // Process the scheduled reminder
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::reducer]
fn send_reminder(ctx: &ReducerContext, reminder: Reminder) -> Result<(), String> {
    if !ctx.sender_auth().is_internal() {
        return Err("This reducer can only be called by the scheduler".to_string());
    }
    // Process the scheduled reminder
    Ok(())
}
```

</TabItem>
</Tabs>

## Use Cases

- **Reminders and notifications** - Schedule messages to be sent at specific times
- **Expiring content** - Automatically remove or archive old data
- **Delayed actions** - Queue up actions to execute after a delay
- **Periodic tasks** - Schedule repeating maintenance or cleanup operations
- **Game mechanics** - Timer-based gameplay events (building completion, energy regeneration, etc.)

## Next Steps

- Learn about [Reducers](/functions/reducers) to handle scheduled actions
- Explore [Procedures](/functions/procedures) for scheduled execution patterns
