---
name: spacetimedb-csharp
description: Build C# modules and Unity clients for SpacetimeDB. Covers server-side module development and client SDK integration.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
  tested_with: "SpacetimeDB 2.0, .NET 8 SDK"
---

# SpacetimeDB C# SDK

This skill provides guidance for building C# server-side modules and Unity/C# clients that connect to SpacetimeDB 2.0.

---

## HALLUCINATED APIs — DO NOT USE

**These APIs DO NOT EXIST. LLMs frequently hallucinate them.**

```csharp
// WRONG — these do not exist
[SpacetimeDB.Procedure]             // C# does NOT support procedures yet!
ctx.db.tableName                    // Wrong casing — use ctx.Db
ctx.Db.tableName                    // Wrong casing — accessor must match exactly
ctx.Db.TableName.Get(id)            // Use Find, not Get
ctx.Db.TableName.FindById(id)       // Use index accessor: ctx.Db.TableName.Id.Find(id)
ctx.Db.table.field_name.Find(x)     // Wrong! Use PascalCase: ctx.Db.Table.FieldName.Find(x)
Optional<string> field;             // Use C# nullable: string? field

// WRONG — missing partial keyword
public struct MyTable { }           // Must be "partial struct"
public class Module { }             // Must be "static partial class"

// WRONG — non-partial types
[SpacetimeDB.Table(Accessor = "Player")]
public struct Player { }            // WRONG — missing partial!

// WRONG — sum type syntax (VERY COMMON MISTAKE)
public partial struct Shape : TaggedEnum<(Circle, Rectangle)> { }     // WRONG: struct, missing names
public partial record Shape : TaggedEnum<(Circle, Rectangle)> { }     // WRONG: missing variant names
public partial class Shape : TaggedEnum<(Circle Circle, Rectangle Rectangle)> { }  // WRONG: class

// WRONG — Index attribute without full qualification
[Index.BTree(Accessor = "idx", Columns = new[] { "Col" })]    // Ambiguous with System.Index!
[SpacetimeDB.Index.BTree(Accessor = "idx", Columns = ["Col"])]  // Collection expressions don't work!

// WRONG — old 1.0 patterns
[SpacetimeDB.Table(Name = "Player")]        // Use Accessor, not Name (2.0)
<PackageReference Include="SpacetimeDB.ServerSdk" />  // Use SpacetimeDB.Runtime
.WithModuleName("my-db")                    // Use .WithDatabaseName() (2.0)
ScheduleAt.Time(futureTime)                 // Use new ScheduleAt.Time(futureTime)

// WRONG — lifecycle hooks starting with "On"
[SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
public static void OnClientConnected(ReducerContext ctx) { }  // STDB0010 error!
```

### CORRECT PATTERNS

```csharp
using SpacetimeDB;

// CORRECT TABLE — must be partial struct, use Accessor
[SpacetimeDB.Table(Accessor = "Player", Public = true)]
public partial struct Player
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    public Identity OwnerId;
    public string Name;
}

// CORRECT MODULE — must be static partial class
public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void CreatePlayer(ReducerContext ctx, string name)
    {
        ctx.Db.Player.Insert(new Player { Id = 0, OwnerId = ctx.Sender, Name = name });
    }
}

// CORRECT DATABASE ACCESS — PascalCase, index-based lookups
var player = ctx.Db.Player.Id.Find(playerId);
var player = ctx.Db.Player.OwnerId.Find(ctx.Sender);

// CORRECT SUM TYPE — partial record with named tuple elements
[SpacetimeDB.Type]
public partial record Shape : TaggedEnum<(Circle Circle, Rectangle Rectangle)> { }
```

---

## Common Mistakes Table

### Server-side errors

| Wrong | Right | Error |
|-------|-------|-------|
| Missing `partial` keyword | `public partial struct Table` | Generated code won't compile |
| `[Table(Name = "x")]` | `[Table(Accessor = "x")]` | 2.0 uses Accessor, not Name |
| `ctx.Db.player` (lowercase) | `ctx.Db.Player` (PascalCase) | Property not found |
| `Optional<string>` | `string?` | Type not found |
| `ctx.Db.Table.Get(id)` | `ctx.Db.Table.Id.Find(id)` | Method not found |
| Wrong .csproj name | `StdbModule.csproj` | Publish fails silently |
| .NET 9 SDK | .NET 8 SDK only | WASI compilation fails |
| Missing WASI workload | `dotnet workload install wasi-experimental` | Build fails |
| `[Procedure]` attribute | Reducers only | Procedures not supported in C# |
| Missing `Public = true` | Add to `[Table]` attribute | Clients can't subscribe |
| Using `Random` | Avoid non-deterministic code | Sandbox violation |
| async/await in reducers | Synchronous only | Not supported |
| `[Index.BTree(...)]` | `[SpacetimeDB.Index.BTree(...)]` | Ambiguous with System.Index |
| `Columns = ["A", "B"]` | `Columns = new[] { "A", "B" }` | Collection expressions invalid in attributes |
| `partial struct : TaggedEnum` | `partial record : TaggedEnum` | Sum types must be record |
| `TaggedEnum<(A, B)>` | `TaggedEnum<(A A, B B)>` | Tuple must include variant names |
| `SpacetimeDB.ServerSdk` | `SpacetimeDB.Runtime` | Wrong package name |
| `OnClientConnected` hook name | `ClientConnected` | STDB0010 error |
| `table.Name.Update(...)` | `table.Id.Update(...)` | Update only via primary key (2.0) |

### Client-side errors

| Wrong | Right | Error |
|-------|-------|-------|
| `.WithModuleName()` | `.WithDatabaseName()` | 2.0 renamed method |
| `ScheduleAt.Time(x)` | `new ScheduleAt.Time(x)` | Must use constructor |
| Not calling `FrameTick()` | `conn.FrameTick()` in Update loop | No callbacks fire |
| Accessing `conn.Db` from background thread | Copy data in callback | Data races |

---

## Hard Requirements

1. **Tables and Module MUST be `partial`** — required for code generation
2. **Use `Accessor =` in table attributes** — `Name =` is only for SQL compatibility (2.0)
3. **Project file MUST be named `StdbModule.csproj`** — CLI requirement
4. **Requires .NET 8 SDK** — .NET 9 and newer not yet supported
5. **Install WASI workload** — `dotnet workload install wasi-experimental`
6. **C# does NOT support procedures** — use reducers only
7. **Reducers must be deterministic** — no filesystem, network, timers, or `Random`
8. **Add `Public = true`** — if clients need to subscribe to a table
9. **Use `T?` for nullable fields** — not `Optional<T>`
10. **Pass `0` for auto-increment** — to trigger ID generation on insert
11. **Sum types must be `partial record`** — not struct or class
12. **Fully qualify Index attribute** — `[SpacetimeDB.Index.BTree]` to avoid System.Index ambiguity
13. **Update only via primary key** — use delete+insert for non-PK changes (2.0)
14. **Use `SpacetimeDB.Runtime` package** — not `ServerSdk` (2.0)

---

## Server-Side Module Development

### Table Definition

```csharp
using SpacetimeDB;

[SpacetimeDB.Table(Accessor = "Player", Public = true)]
public partial struct Player
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    [SpacetimeDB.Index.BTree]
    public Identity OwnerId;

    public string Name;
    public Timestamp CreatedAt;
}

// Multi-column index (use fully-qualified attribute!)
[SpacetimeDB.Table(Accessor = "Score", Public = true)]
[SpacetimeDB.Index.BTree(Accessor = "by_player_game", Columns = new[] { "PlayerId", "GameId" })]
public partial struct Score
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    public Identity PlayerId;
    public string GameId;
    public int Points;
}
```

### Field Attributes

```csharp
[SpacetimeDB.PrimaryKey]     // Exactly one per table (required)
[SpacetimeDB.AutoInc]        // Auto-increment (integer fields only)
[SpacetimeDB.Unique]         // Unique constraint
[SpacetimeDB.Index.BTree]    // Single-column B-tree index
[SpacetimeDB.Default(value)] // Default value for new columns
```

### Module and Reducers

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void CreateTask(ReducerContext ctx, string title)
    {
        if (string.IsNullOrEmpty(title))
            throw new Exception("Title cannot be empty");

        ctx.Db.Task.Insert(new Task
        {
            Id = 0,
            OwnerId = ctx.Sender,
            Title = title,
            Completed = false
        });
    }

    [SpacetimeDB.Reducer]
    public static void CompleteTask(ReducerContext ctx, ulong taskId)
    {
        if (ctx.Db.Task.Id.Find(taskId) is not Task task)
            throw new Exception("Task not found");
        if (task.OwnerId != ctx.Sender)
            throw new Exception("Not authorized");

        ctx.Db.Task.Id.Update(task with { Completed = true });
    }

    [SpacetimeDB.Reducer]
    public static void DeleteTask(ReducerContext ctx, ulong taskId)
    {
        ctx.Db.Task.Id.Delete(taskId);
    }
}
```

### Lifecycle Reducers

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        Log.Info("Module initialized");
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        Log.Info($"Client connected: {ctx.Sender}");
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
    public static void ClientDisconnected(ReducerContext ctx)
    {
        Log.Info($"Client disconnected: {ctx.Sender}");
    }
}
```

### Event Tables (2.0)

Reducer callbacks are removed in 2.0. Use event tables + `OnInsert` instead.

```csharp
[SpacetimeDB.Table(Accessor = "DamageEvent", Public = true, Event = true)]
public partial struct DamageEvent
{
    public Identity Target;
    public uint Amount;
}

[SpacetimeDB.Reducer]
public static void DealDamage(ReducerContext ctx, Identity target, uint amount)
{
    ctx.Db.DamageEvent.Insert(new DamageEvent { Target = target, Amount = amount });
}
```

Client subscribes and uses `OnInsert`:
```csharp
conn.Db.DamageEvent.OnInsert += (ctx, evt) => {
    PlayDamageAnimation(evt.Target, evt.Amount);
};
```

Event tables must be subscribed explicitly — they are excluded from `SubscribeToAllTables()`.

### Database Access

```csharp
// Find by primary key — returns nullable, use pattern matching
if (ctx.Db.Task.Id.Find(taskId) is Task task) { /* use task */ }

// Update by primary key (2.0: only primary key has .Update)
ctx.Db.Task.Id.Update(task with { Title = newTitle });

// Delete by primary key
ctx.Db.Task.Id.Delete(taskId);

// Filter by B-tree index — returns iterator
foreach (var task in ctx.Db.Task.OwnerId.Filter(ctx.Sender)) { }

// Full table scan
foreach (var task in ctx.Db.Task.Iter()) { }
```

### Custom Types and Sum Types

```csharp
[SpacetimeDB.Type]
public partial struct Position { public int X; public int Y; }

// Sum types MUST be partial record with named tuple
[SpacetimeDB.Type]
public partial struct Circle { public int Radius; }
[SpacetimeDB.Type]
public partial struct Rectangle { public int Width; public int Height; }
[SpacetimeDB.Type]
public partial record Shape : TaggedEnum<(Circle Circle, Rectangle Rectangle)> { }

// Creating sum type values
var circle = new Shape.Circle(new Circle { Radius = 10 });
```

### Scheduled Tables

```csharp
[SpacetimeDB.Table(Accessor = "Reminder", Scheduled = nameof(Module.SendReminder))]
public partial struct Reminder
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    public string Message;
    public ScheduleAt ScheduledAt;
}

public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void SendReminder(ReducerContext ctx, Reminder reminder)
    {
        Log.Info($"Reminder: {reminder.Message}");
    }

    [SpacetimeDB.Reducer]
    public static void CreateReminder(ReducerContext ctx, string message, ulong delaySecs)
    {
        ctx.Db.Reminder.Insert(new Reminder
        {
            Id = 0,
            Message = message,
            ScheduledAt = new ScheduleAt.Time(ctx.Timestamp + TimeSpan.FromSeconds(delaySecs))
        });
    }
}
```

### ReducerContext API

```csharp
ctx.Sender          // Identity of the caller
ctx.Timestamp       // Current timestamp
ctx.Db              // Database access
ctx.Identity        // Module's own identity
ctx.ConnectionId    // Connection ID (nullable)
```

---

## Project Setup

### Required .csproj (MUST be named `StdbModule.csproj`)

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <RuntimeIdentifier>wasi-wasm</RuntimeIdentifier>
    <OutputType>Exe</OutputType>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="SpacetimeDB.Runtime" Version="1.*" />
  </ItemGroup>
</Project>
```

### Prerequisites

```bash
# Install .NET 8 SDK (required, not .NET 9)
# Install WASI workload
dotnet workload install wasi-experimental
```

---

## Client SDK

### Connection Setup

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;

conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-database")
    .WithToken(savedToken)
    .OnConnect(OnConnected)
    .OnConnectError(err => Console.Error.WriteLine($"Failed: {err}"))
    .OnDisconnect((conn, err) => { if (err != null) Console.Error.WriteLine(err); })
    .Build();

void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    Console.WriteLine($"Connected: {identity}");
    conn.SubscriptionBuilder()
        .OnApplied(OnSubscriptionApplied)
        .SubscribeToAllTables();
}
```

### Critical: FrameTick

```csharp
while (running) { conn.FrameTick(); Thread.Sleep(16); }
// Unity: call conn?.FrameTick() in Update()
```

### Row Callbacks and Reducers

```csharp
conn.Db.Player.OnInsert += (EventContext ctx, Player p) => { };
conn.Db.Player.OnUpdate += (EventContext ctx, Player old, Player new_) => { };
conn.Db.Player.OnDelete += (EventContext ctx, Player p) => { };

conn.Reducers.CreatePlayer("Alice");

conn.Reducers.OnCreatePlayer += (ctx) => {
    if (ctx.Event.Status is Status.Committed) { /* success */ }
    else if (ctx.Event.Status is Status.Failed failed) { /* error */ }
};
```

---

## Commands

```bash
spacetime start
spacetime publish <module-name> --module-path <backend-dir>
spacetime publish <module-name> --clear-database -y --module-path <backend-dir>
spacetime generate --lang csharp --out-dir <client>/SpacetimeDB --module-path <backend-dir>
spacetime logs <module-name>
```
