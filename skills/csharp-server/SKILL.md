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

Reducers are static methods in a `static partial class`; tables are `public partial struct`s. This reference keeps everything in one `public static partial class Module`, which needs only `using SpacetimeDB;`:

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

`[SpacetimeDB.Table(...)]` on a `public partial struct`. `Accessor` should be PascalCase:

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

Optional columns: nullable types (`string? Nickname`, `uint? HighScore`)

## Column Attributes

The complete set of column attributes:

```csharp
[PrimaryKey]          // primary key
[AutoInc]             // auto-increment (use 0 as placeholder on insert)
[Unique]              // unique constraint; indexes the column, enables .Find()
[SpacetimeDB.Index.BTree]  // btree index (enables .Filter() on this column)
[Default(true)]       // migration-safe default for a newly appended field
```

Defaults are for compatible schema upgrades. Preserve existing fields and reducers, append the defaulted field, and do not apply `[Default(...)]` to primary-key, unique, or auto-increment fields.

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

Prefer a multi-column index over filtering by one column and looping.

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
var row = ctx.Db.Entity.Insert(new Entity { Name = "Sample" });   // Insert; returns the row with AutoInc fields assigned
ctx.Db.Entity.Id.Find(entityId);                                  // Find by PK → Entity? (nullable)
ctx.Db.Entity.Identity.Find(ctx.Sender);                          // Find by unique column → Entity?
if (ctx.Db.Entity.Id.Find(entityId) is { } entity) { ... }        // unwrap Entity? before member access
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

Query-builder views use `ViewContext`, `ctx.From`, and return `IQuery<T>` directly. Use `Where` for predicates and `RightSemijoin` when the result should contain right-side rows that have a matching left-side row:

```csharp
ctx.From.Article().Where(article => article.Published.Eq(true));
ctx.From.Subscription().RightSemijoin(
    ctx.From.Account(),
    (subscription, account) => subscription.AccountId.Eq(account.Id)
);
```

Declare a procedural view primary key in its attribute: `[SpacetimeDB.View(Accessor = "CatalogEntry", Public = true, PrimaryKey = nameof(CatalogRow.Sku))]`.

Inclusive btree ranges use tuples, for example `ctx.Db.Shipment.DeliverBy.Filter((new Timestamp(1_000), new Timestamp(2_000)))`.

## Client Visibility Filters

```csharp
[ClientVisibilityFilter]
public static readonly Filter PrivateNoteFilter = new Filter.Sql(
    "SELECT * FROM OwnedRow WHERE Owner = :sender"
);
```

## Reducer Context API

`ReducerContext` (`ctx`) is the only source of sender identity, time, and randomness; stdlib clocks and RNG are unavailable in modules.

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

// Timestamp → milliseconds since epoch
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

Construct a connection ID from 16 bytes with `ConnectionId.From(bytes)`; the result is nullable and must be checked.

## Procedures and HTTP

Procedures are unstable APIs, so modules using them should include `#pragma warning disable STDB_UNSTABLE`. They receive `ProcedureContext` and may return `[SpacetimeDB.Type]` values:

```csharp
[SpacetimeDB.Type]
public partial struct ResultValue { public string Value; }

[SpacetimeDB.Procedure]
public static ResultValue Inspect(ProcedureContext ctx, string input) =>
    new() { Value = input };
```

Outbound HTTP is available through `ctx.Http`; handle its success/error result before using the response. Open short database transactions with `ctx.WithTx`. Perform network I/O before opening the transaction, and keep only database work inside its callback.

Scheduled procedures use the ordinary scheduled-table shape. Its `Scheduled` name refers to a `[SpacetimeDB.Procedure]` method taking `ProcedureContext` plus the scheduled row, and database access inside that procedure goes through `ctx.WithTx`.

Inbound HTTP uses handler attributes and one router. Handler database access also goes through `ctx.WithTx`:

```csharp
[SpacetimeDB.HttpHandler]
public static HttpResponse Health(HandlerContext ctx, HttpRequest request) => new(
    200, HttpVersion.Http11, new(), HttpBody.FromString("ok")
);

[SpacetimeDB.HttpRouter]
public static Router Routes() => SpacetimeDB.Router.New().Get("/health", Handlers.Health);
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
