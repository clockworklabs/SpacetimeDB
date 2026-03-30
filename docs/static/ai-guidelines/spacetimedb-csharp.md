# SpacetimeDB C# Server Module Guidelines

## Imports

```csharp
using SpacetimeDB;
```

Additional imports when needed:
```csharp
using System.Linq;                // For LINQ queries (OrderBy, Take, etc.)
using System.Collections.Generic; // For HashSet, List, etc.
```

## Module Structure

All code goes inside a static partial class:
```csharp
using SpacetimeDB;

public static partial class Module
{
    // Tables, types, and reducers here
}
```

## Table Definitions

Basic table:
```csharp
[Table(Accessor = "User")]
public partial struct User
{
    [PrimaryKey]
    public int Id;
    public string Name;
    public int Age;
    public bool Active;
}
```

Public table with auto-increment:
```csharp
[Table(Accessor = "Message", Public = true)]
public partial struct Message
{
    [PrimaryKey]
    [AutoInc]
    public ulong Id;
    public Identity Owner;
    public string Text;
}
```

Event table (append-only):
```csharp
[Table(Accessor = "DamageEvent", Public = true, Event = true)]
public partial struct DamageEvent
{
    public ulong EntityId;
    public uint Damage;
    public string Source;
}
```

Scheduled table:
```csharp
[Table(Accessor = "TickTimer", Scheduled = nameof(Tick), ScheduledAt = nameof(TickTimer.ScheduledAt))]
public partial struct TickTimer
{
    [PrimaryKey, AutoInc]
    public ulong ScheduledId;
    public ScheduleAt ScheduledAt;
}
```

## Column Types

| C# Type | Usage |
|---------|-------|
| `int`, `uint` | 32-bit integers |
| `long`, `ulong` | 64-bit integers |
| `float`, `double` | Floating point |
| `bool` | Boolean |
| `string` | Text |
| `Identity` | User identity |
| `Timestamp` | Timestamp |
| `ScheduleAt` | Schedule metadata |
| `T?` (nullable) | Optional field |

Timestamp arithmetic — use `TimeDuration`:
```csharp
ctx.Timestamp + new TimeDuration { Microseconds = 60_000_000 }  // add 60 seconds
ctx.Timestamp.MicrosecondsSinceUnixEpoch  // raw long value
```

## Constraints and Indexes

```csharp
[PrimaryKey] public int Id;
[PrimaryKey, AutoInc] public ulong Id;
[Unique] public string Email;
```

Single-column index:
```csharp
[SpacetimeDB.Index.BTree]
public Identity Owner;
```

Named single-column index:
```csharp
[SpacetimeDB.Index.BTree(Accessor = "by_name", Columns = [nameof(Name)])]
```

Multi-column index (on table attribute — MUST include Accessor):
```csharp
[Table(Accessor = "Log")]
[SpacetimeDB.Index.BTree(Accessor = "by_user_day", Columns = new[] { nameof(UserId), nameof(Day) })]
public partial struct Log
{
    [PrimaryKey] public int Id;
    public int UserId;
    public int Day;
}
```

## Product Types (Structs)

```csharp
[Type]
public partial struct Position
{
    public int X;
    public int Y;
}

[Table(Accessor = "Entity")]
public partial struct Entity
{
    [PrimaryKey] public int Id;
    public Position Pos;
}
```

## Sum Types (Tagged Unions)

```csharp
[Type]
public partial struct Circle { public int Radius; }

[Type]
public partial struct Rectangle { public int Width; public int Height; }

[Type]
public partial record Shape : TaggedEnum<(Circle Circle, Rectangle Rectangle)> {}

// Construction:
new Shape.Circle(new Circle { Radius = 10 })
new Shape.Rectangle(new Rectangle { Width = 5, Height = 3 })
```

## Reducers

Basic reducer:
```csharp
[Reducer]
public static void InsertUser(ReducerContext ctx, int id, string name, int age, bool active)
{
    ctx.Db.User.Insert(new User { Id = id, Name = name, Age = age, Active = active });
}
```

Init reducer (runs once on startup):
```csharp
[Reducer(ReducerKind.Init)]
public static void Init(ReducerContext ctx)
{
    ctx.Db.Config.Insert(new Config { Id = 0, Setting = "default" });
}
```

Scheduled reducer:
```csharp
[Reducer]
public static void Tick(ReducerContext ctx, TickTimer timer)
{
    // Runs when timer fires
}
```

## Database Operations

### Insert
```csharp
ctx.Db.User.Insert(new User { Id = 1, Name = "Alice", Age = 30, Active = true });

// Auto-inc: use 0 as placeholder
var row = ctx.Db.Message.Insert(new Message { Id = 0, Text = "Hello" });
// row.Id now has the assigned value
```

### Find (by primary key or unique index)
```csharp
var user = ctx.Db.User.Id.Find(userId);
if (user is not null) { /* use user */ }
if (user is User u) { /* use u */ }

// With expect pattern:
var msg = ctx.Db.Message.Id.Find(id) ?? throw new Exception("not found");
```

### Filter (by btree index — returns iterator)
```csharp
foreach (var post in ctx.Db.Post.AuthorId.Filter(authorId))
{
    ctx.Db.Post.Id.Delete(post.Id);
}
```

### Iterate all rows
```csharp
foreach (var row in ctx.Db.User.Iter())
{
    // process row
}
```

### Update
```csharp
ctx.Db.User.Id.Update(new User { Id = id, Name = newName, Age = newAge, Active = true });

// Using `with` expression:
ctx.Db.RateLimit.Identity.Update(existing with { LastCallUs = now });
```

### Delete
```csharp
ctx.Db.User.Id.Delete(userId);
ctx.Db.OnlinePlayer.Identity.Delete(ctx.Sender);
```

## Authentication

`ctx.Sender` is the authenticated caller's `Identity`:

```csharp
[Reducer]
public static void SendMessage(ReducerContext ctx, string text)
{
    ctx.Db.Message.Insert(new Message
    {
        Id = 0,
        Owner = ctx.Sender,
        Text = text,
    });
}

[Reducer]
public static void DeleteMessage(ReducerContext ctx, ulong id)
{
    var msg = ctx.Db.Message.Id.Find(id) ?? throw new Exception("not found");
    if (msg.Owner != ctx.Sender)
    {
        throw new Exception("unauthorized");
    }
    ctx.Db.Message.Id.Delete(id);
}
```

Identity as primary key:
```csharp
[Table(Accessor = "Player")]
public partial struct Player
{
    [PrimaryKey]
    public Identity Identity;
    public string Name;
}

// Check registration
if (ctx.Db.Player.Identity.Find(ctx.Sender) is not null)
{
    throw new Exception("already registered");
}
```

## Lifecycle Hooks

```csharp
[Reducer(ReducerKind.ClientConnected)]
public static void ClientConnected(ReducerContext ctx)
{
    ctx.Db.OnlinePlayer.Insert(new OnlinePlayer
    {
        Identity = ctx.Sender,
        ConnectedAt = ctx.Timestamp,
    });
}

[Reducer(ReducerKind.ClientDisconnected)]
public static void ClientDisconnected(ReducerContext ctx)
{
    ctx.Db.OnlinePlayer.Identity.Delete(ctx.Sender);
}
```

## Views

Anonymous view (same result for all clients):
```csharp
[SpacetimeDB.View(Accessor = "ActiveAnnouncements", Public = true)]
public static List<Announcement> ActiveAnnouncements(AnonymousViewContext ctx)
{
    return ctx.Db.Announcement.Active.Filter(true).ToList();
}
```

Per-user view:
```csharp
// Return types: T? for single, List<T> for multiple. Never IEnumerable<T>.
[SpacetimeDB.View(Accessor = "MyProfile", Public = true)]
public static Profile? MyProfile(ViewContext ctx)
{
    return ctx.Db.Profile.Identity.Find(ctx.Sender) as Profile?;
}
```

## Scheduled Tables

```csharp
[Table(Accessor = "Reminder", Scheduled = nameof(SendReminder), ScheduledAt = nameof(Reminder.ScheduledAt))]
public partial struct Reminder
{
    [PrimaryKey, AutoInc]
    public ulong ScheduledId;
    public ScheduleAt ScheduledAt;
    public string Message;
}

[Reducer]
public static void SendReminder(ReducerContext ctx, Reminder row)
{
    // row.Message available
}

// Schedule recurring interval
var interval = new TimeDuration { Microseconds = 50_000 };
ctx.Db.TickTimer.Insert(new TickTimer
{
    ScheduledAt = new ScheduleAt.Interval(interval),
});

// Schedule at specific time
var delay = new TimeDuration { Microseconds = 60_000_000 };
ctx.Db.Reminder.Insert(new Reminder
{
    ScheduledAt = new ScheduleAt.Time(ctx.Timestamp + delay),
    Message = "Hello!",
});

// Cancel a scheduled job
ctx.Db.Reminder.ScheduledId.Delete(scheduledId);
```

## Optional Fields

```csharp
[Table(Accessor = "Player")]
public partial struct Player
{
    [PrimaryKey, AutoInc]
    public ulong Id;
    public string Name;
    public string? Nickname;
    public uint? HighScore;
}
```

## LINQ Queries

```csharp
using System.Linq;

// Sort and limit
var topPlayers = ctx.Db.Player.Iter()
    .OrderByDescending(p => p.Score)
    .Take((int)limit)
    .ToList();

// Distinct values
var categories = new HashSet<string>();
foreach (var o in ctx.Db.Order.Iter())
{
    categories.Add(o.Category);
}

// Count
var total = (ulong)ctx.Db.User.Iter().Count();
```

## Helper Functions

```csharp
static int Add(int a, int b) => a + b;

[Reducer]
public static void ComputeSum(ReducerContext ctx, int id, int a, int b)
{
    ctx.Db.Result.Insert(new Result { Id = id, Sum = Add(a, b) });
}
```

## Complete Module Example

```csharp
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "User", Public = true)]
    public partial struct User
    {
        [PrimaryKey]
        public Identity Identity;
        public string Name;
        public bool Online;
    }

    [Table(Accessor = "Message", Public = true)]
    [SpacetimeDB.Index.BTree(Accessor = "by_sender", Columns = [nameof(Message.Sender)])]
    public partial struct Message
    {
        [PrimaryKey, AutoInc]
        public ulong Id;
        public Identity Sender;
        public string Text;
        public Timestamp SentAt;
    }

    [Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        if (ctx.Db.User.Identity.Find(ctx.Sender) is User u)
        {
            ctx.Db.User.Identity.Update(u with { Online = true });
        }
    }

    [Reducer(ReducerKind.ClientDisconnected)]
    public static void ClientDisconnected(ReducerContext ctx)
    {
        if (ctx.Db.User.Identity.Find(ctx.Sender) is User u)
        {
            ctx.Db.User.Identity.Update(u with { Online = false });
        }
    }

    [Reducer]
    public static void Register(ReducerContext ctx, string name)
    {
        if (ctx.Db.User.Identity.Find(ctx.Sender) is not null)
        {
            throw new Exception("already registered");
        }
        ctx.Db.User.Insert(new User { Identity = ctx.Sender, Name = name, Online = true });
    }

    [Reducer]
    public static void SendMessage(ReducerContext ctx, string text)
    {
        if (ctx.Db.User.Identity.Find(ctx.Sender) is null)
        {
            throw new Exception("not registered");
        }
        ctx.Db.Message.Insert(new Message
        {
            Id = 0,
            Sender = ctx.Sender,
            Text = text,
            SentAt = ctx.Timestamp,
        });
    }
}
```
