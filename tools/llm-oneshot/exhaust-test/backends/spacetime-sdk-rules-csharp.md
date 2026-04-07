# SpacetimeDB C# SDK Reference

## Imports

```csharp
using SpacetimeDB;
```

## Tables

`[SpacetimeDB.Table(...)]` on a `public partial struct` — `Accessor` must be snake_case:

```csharp
[SpacetimeDB.Table(Accessor = "entity", Public = true)]
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

Options: `Accessor = "snake_case"` (required), `Public = true`, `Scheduled = nameof(ReducerFn)`, `Event = true`

`ctx.Db` accessors use the `Accessor` name (lowercase, snake_case).

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

```csharp
[PrimaryKey]          // primary key
[AutoInc]             // auto-increment (use 0 as placeholder on insert)
[Unique]              // unique constraint
[SpacetimeDB.Index.BTree]  // btree index (enables FilterBy on this column)
```

## Indexes

Prefer `[SpacetimeDB.Index.BTree]` inline for single-column. Multi-column uses struct-level:

```csharp
// Inline (preferred):
[SpacetimeDB.Index.BTree]
public ulong AuthorId;
// Access: ctx.Db.post.FilterByAuthorId(authorId)

// Multi-column (struct-level):
[SpacetimeDB.Table(Accessor = "post", Public = true)]
[SpacetimeDB.Index.BTree(Accessor = "by_cat_sev", Columns = new[] { "Category", "Severity" })]
public partial struct Post { ... }
```

## Reducers

```csharp
[SpacetimeDB.Reducer]
public static void CreateEntity(ReducerContext ctx, string name, int age)
{
    ctx.Db.entity.Insert(new Entity { Owner = ctx.Sender, Name = name, Age = age, Active = true });
}

// No arguments:
[SpacetimeDB.Reducer]
public static void DoReset(ReducerContext ctx) { ... }
```

## DB Operations

```csharp
ctx.Db.entity.Insert(new Entity { Name = "Sample" });          // Insert
ctx.Db.entity.FindById(entityId);                              // Find by PK → Entity? (nullable)
ctx.Db.entity.FindByIdentity(ctx.Sender);                      // Find by unique column → Entity?
ctx.Db.item.FilterByAuthorId(authorId);                        // Filter by index → IEnumerable<Item>
ctx.Db.entity.Iter();                                          // All rows → IEnumerable<Entity>
ctx.Db.entity.UpdateById(new Entity { ..existing, Name = newName }); // Update by PK
ctx.Db.entity.DeleteById(entityId);                            // Delete by PK
```

Note: Filter/Iter return enumerables. Use `.ToList()` if you need to sort or mutate.

The generated Find/Filter/Update/Delete methods follow the pattern `VerbByColumnName` where `ColumnName` is the PascalCase field name.

## Lifecycle Hooks

```csharp
[SpacetimeDB.Reducer(Kind = ReducerKind.Init)]
public static void OnInit(ReducerContext ctx) { ... }

[SpacetimeDB.Reducer(Kind = ReducerKind.ClientConnected)]
public static void OnConnect(ReducerContext ctx) { ... }

[SpacetimeDB.Reducer(Kind = ReducerKind.ClientDisconnected)]
public static void OnDisconnect(ReducerContext ctx) { ... }
```

## Authentication & Timestamps

```csharp
// Auth: ctx.Sender is the caller's Identity
if (row.Owner != ctx.Sender)
    throw new Exception("unauthorized");

// Server timestamps
ctx.Db.item.Insert(new Item { CreatedAt = ctx.Timestamp, .. });

// Timestamp arithmetic
var expiry = ctx.Timestamp + TimeDuration.FromMicroseconds(delayMicros);

// Client: Timestamp → milliseconds since epoch
timestamp.MicrosecondsSinceEpoch / 1000
```

## Scheduled Tables

```csharp
[SpacetimeDB.Table(
    Accessor = "tick_timer",
    Scheduled = nameof(Tick),
    ScheduledAt = nameof(scheduled_at),
    Public = true
)]
public partial struct TickTimer
{
    [PrimaryKey]
    [AutoInc]
    public ulong scheduled_id;
    public ScheduleAt scheduled_at;
}

[SpacetimeDB.Reducer]
public static void Tick(ReducerContext ctx, TickTimer timer)
{
    // timer row is auto-deleted after this reducer runs
}

// One-time: fires once at a specific time
var at = new ScheduleAt.Time(ctx.Timestamp + TimeDuration.FromMicroseconds(delayMicros));
// Repeating: fires on an interval
var at = new ScheduleAt.Interval(TimeDuration.FromMicroseconds(60_000_000));

ctx.Db.tick_timer.Insert(new TickTimer { scheduled_id = 0, scheduled_at = at });
```

## Custom Types

```csharp
[SpacetimeDB.Type]
public enum Status { Online, Away, Offline }

[SpacetimeDB.Type]
public partial struct Point { public float X; public float Y; }

// Tagged enum (discriminated union):
[SpacetimeDB.Type]
public partial record MyUnion : SpacetimeDB.TaggedEnum<(string Text, int Number)>;
```

## Complete Example

```csharp
using SpacetimeDB;

[SpacetimeDB.Table(Accessor = "entity", Public = true)]
public partial struct Entity
{
    [PrimaryKey]
    public Identity Identity;
    public string Name;
    public bool Active;
}

[SpacetimeDB.Table(Accessor = "record", Public = true)]
public partial struct Record
{
    [PrimaryKey]
    [AutoInc]
    public ulong Id;
    public Identity Owner;
    public uint Value;
    public Timestamp CreatedAt;
}

public static partial class Module
{
    [SpacetimeDB.Reducer(Kind = ReducerKind.ClientConnected)]
    public static void OnConnect(ReducerContext ctx)
    {
        var existing = ctx.Db.entity.FindByIdentity(ctx.Sender);
        if (existing is not null)
            ctx.Db.entity.UpdateByIdentity(existing.Value with { Active = true });
    }

    [SpacetimeDB.Reducer(Kind = ReducerKind.ClientDisconnected)]
    public static void OnDisconnect(ReducerContext ctx)
    {
        var existing = ctx.Db.entity.FindByIdentity(ctx.Sender);
        if (existing is not null)
            ctx.Db.entity.UpdateByIdentity(existing.Value with { Active = false });
    }

    [SpacetimeDB.Reducer]
    public static void CreateEntity(ReducerContext ctx, string name)
    {
        if (ctx.Db.entity.FindByIdentity(ctx.Sender) is not null)
            throw new Exception("already exists");
        ctx.Db.entity.Insert(new Entity { Identity = ctx.Sender, Name = name, Active = true });
    }

    [SpacetimeDB.Reducer]
    public static void AddRecord(ReducerContext ctx, uint value)
    {
        if (ctx.Db.entity.FindByIdentity(ctx.Sender) is null)
            throw new Exception("not found");
        ctx.Db.record.Insert(new Record {
            Id = 0,
            Owner = ctx.Sender,
            Value = value,
            CreatedAt = ctx.Timestamp,
        });
    }
}
```
