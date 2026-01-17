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

## How It Works

1. **Insert a row** with a `schedule_at` time
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
