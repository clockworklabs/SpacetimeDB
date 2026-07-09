---
name: csharp-server
description: SpacetimeDB C# server module SDK reference. Use when writing tables, reducers, or module logic in C#.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
  role: server
  language: csharp
  cursor_globs: "**/*.cs"
  cursor_always_apply: true
---

# SpacetimeDB C# SDK Reference

## Module Structure

All tables, types, and reducers go inside one `public static partial class Module`; the only import is `using SpacetimeDB;`:

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "ScoreRecord", Public = true)]
    public partial struct ScoreRecord
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public Identity Owner;
        public uint Value;
    }

    [SpacetimeDB.Reducer]
    public static void AddRecord(ReducerContext ctx, uint value)
    {
        ctx.Db.ScoreRecord.Insert(new ScoreRecord { Id = 0, Owner = ctx.Sender, Value = value });
    }
}
```

## Tables

`[SpacetimeDB.Table(...)]` on a `public partial struct` inside the `Module` class. `Accessor` should be PascalCase:

```csharp
[SpacetimeDB.Table(Accessor = "Entity", Public = true)]
public partial struct Entity
{
    [PrimaryKey]
    [AutoInc]
    public ulong Id;
    public Identity Owner;
    public string Name;
    public bool Active;
}
```

Options: `Accessor = "PascalCase"` (recommended), `Public = true`, `Scheduled = nameof(ReducerFn)`, `ScheduledAt = nameof(field)`, `Event = true`

`ctx.Db` accessors use the `Accessor` name: `ctx.Db.Entity`, `ctx.Db.ScoreRecord`.

## Column Types

| C# type | Notes |
|---------|-------|
| `byte` / `ushort` / `uint` / `ulong` | unsigned integers |
| `U128` / `U256` | large unsigned integers (SpacetimeDB types) |
| `sbyte` / `short` / `int` / `long` | signed integers |
| `I128` / `I256` | large signed integers (SpacetimeDB types) |
| `float` / `double` | floats |
| `bool` | boolean |
| `string` | text |
| `List<T>` | list/array |
| `Identity` | user identity |
| `ConnectionId` | connection handle |
| `Timestamp` | server timestamp (microseconds since epoch) |
| `TimeDuration` | duration in microseconds |
| `Uuid` | UUID |

## Column Attributes

The complete set of column attributes:

```csharp
[PrimaryKey]          // primary key
[AutoInc]             // auto-increment (use 0 as placeholder on insert)
[Unique]              // unique constraint
[SpacetimeDB.Index.BTree]  // btree index (enables .Filter() on this column)
```

## Indexes

Write the index attribute fully qualified: `[SpacetimeDB.Index.BTree]`. Prefer inline for single-column; multi-column uses struct-level:

```csharp
// Inline (preferred for single-column):
[SpacetimeDB.Index.BTree]
public ulong AuthorId;
// Access: ctx.Db.Post.AuthorId.Filter(authorId)

// Multi-column (struct-level):
[SpacetimeDB.Table(Accessor = "Membership")]
[SpacetimeDB.Index.BTree(Accessor = "ByGroupUser", Columns = new[] { nameof(GroupId), nameof(UserId) })]
public partial struct Membership { public ulong GroupId; public Identity UserId; ... }
```

When you frequently look up rows by multiple columns, prefer a multi-column index over filtering by one column and looping over the results.

## Reducers

```csharp
[SpacetimeDB.Reducer]
public static void CreateEntity(ReducerContext ctx, string name, int age)
{
    ctx.Db.Entity.Insert(new Entity { Owner = ctx.Sender, Name = name, Age = age, Active = true });
}

// No arguments:
[SpacetimeDB.Reducer]
public static void DoReset(ReducerContext ctx) { ... }
```

## DB Operations

```csharp
ctx.Db.Entity.Insert(new Entity { Name = "Sample" });             // Insert
ctx.Db.Entity.Id.Find(entityId);                                  // Find by PK → Entity? (nullable)
ctx.Db.Entity.Identity.Find(ctx.Sender);                          // Find by unique column → Entity?
ctx.Db.Item.AuthorId.Filter(authorId);                            // Filter by index → IEnumerable<Item>
ctx.Db.Entity.Iter();                                             // All rows → IEnumerable<Entity>
ctx.Db.Entity.Count;                                              // Count rows
ctx.Db.Entity.Id.Update(existing with { Name = newName });        // Update by PK
ctx.Db.Entity.Id.Delete(entityId);                                // Delete by PK
```

Note: Filter/Iter return enumerables. Use `.ToList()` if you need to sort or mutate.

The pattern is `ctx.Db.{Accessor}.{ColumnName}.{Method}(value)` for all indexed column operations.

## Lifecycle Hooks

```csharp
[SpacetimeDB.Reducer(ReducerKind.Init)]
public static void OnInit(ReducerContext ctx) { ... }

[SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
public static void OnConnect(ReducerContext ctx) { ... }

[SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
public static void OnDisconnect(ReducerContext ctx) { ... }
```

## Views

```csharp
// Anonymous view (same result for all clients):
[SpacetimeDB.View(Accessor = "ActiveUsers", Public = true)]
public static List<Entity> ActiveUsers(AnonymousViewContext ctx)
{
    return ctx.Db.Entity.Iter().Where(e => e.Active).ToList();
}

// Per-user view:
[SpacetimeDB.View(Accessor = "MyProfile", Public = true)]
public static Entity? MyProfile(ViewContext ctx)
{
    return ctx.Db.Entity.Identity.Find(ctx.Sender) as Entity?;
}
```

## Reducer Context API

`ReducerContext` is the single source of sender identity, deterministic time, and deterministic randomness inside a reducer. Always go through `ctx` for these. Standard library clocks and random sources are not available in modules.

```csharp
// Auth: ctx.Sender is the caller's Identity
if (row.Owner != ctx.Sender)
    throw new Exception("unauthorized");

// Server timestamp (deterministic per reducer call)
ctx.Db.Item.Insert(new Item { CreatedAt = ctx.Timestamp, .. });

// Timestamp arithmetic
var expiry = ctx.Timestamp + new TimeDuration(delayMicros);

// Deterministic RNG
int roll = ctx.Rng.Next(1, 7);          // [1, 7): inclusive 1, exclusive 7
double f = ctx.Rng.NextDouble();        // [0.0, 1.0)

// Client: Timestamp → milliseconds since epoch
timestamp.MicrosecondsSinceUnixEpoch / 1000
```

## Scheduled Tables

Declare the scheduled table and its reducer in the same `Module` class so `nameof(...)` resolves:

```csharp
[SpacetimeDB.Table(
    Accessor = "TickTimer",
    Scheduled = nameof(Tick),
    ScheduledAt = nameof(ScheduledAt),
    Public = true
)]
public partial struct TickTimer
{
    [PrimaryKey]
    [AutoInc]
    public ulong ScheduledId;
    public ScheduleAt ScheduledAt;
}

[SpacetimeDB.Reducer]
public static void Tick(ReducerContext ctx, TickTimer timer)
{
    // timer row is auto-deleted after this reducer runs
}

// One-time: fires once at a specific time
var at = new ScheduleAt.Time(ctx.Timestamp + new TimeDuration(10_000_000));
// Repeating: fires on an interval
var at = new ScheduleAt.Interval(TimeSpan.FromSeconds(5));

ctx.Db.TickTimer.Insert(new TickTimer { ScheduledId = 0, ScheduledAt = at });
```

## Custom Types

```csharp
[SpacetimeDB.Type]
public enum Status { Online, Away, Offline }

[SpacetimeDB.Type]
public partial struct Point { public float X; public float Y; }
```

Tagged enums (discriminated unions): a `partial record` with empty body and no constructor parameters. Payloads are `[Type] partial struct`s:

```csharp
[SpacetimeDB.Type]
public partial struct Circle { public int Radius; }

[SpacetimeDB.Type]
public partial struct Rectangle { public int Width; public int Height; }

[SpacetimeDB.Type]
public partial record Shape : SpacetimeDB.TaggedEnum<(Circle Circle, Rectangle Rectangle)> { }

// Construct variants via the generated nested constructors:
var a = new Shape.Circle(new Circle { Radius = 10 });
var b = new Shape.Rectangle(new Rectangle { Width = 4, Height = 6 });
```

## Optional Fields

```csharp
[SpacetimeDB.Table(Accessor = "Player")]
public partial struct Player
{
    [PrimaryKey, AutoInc]
    public ulong Id;
    public string Name;
    public string? Nickname;
    public uint? HighScore;
}
```
