---
name: spacetimedb-csharp
description: Build C# modules and clients for SpacetimeDB. Covers server-side module development and client SDK integration.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
  tested_with: "SpacetimeDB 2.0, .NET 8 SDK"
---

# SpacetimeDB C# SDK

This skill provides guidance for building C# server-side modules and C# clients that connect to SpacetimeDB 2.0.

---

## HALLUCINATED APIs — DO NOT USE

**These APIs DO NOT EXIST. LLMs frequently hallucinate them.**

```csharp
// WRONG — these table access patterns do not exist
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
[SpacetimeDB.Index.BTree(Accessor = "idx", Columns = ["Col"])]  // Valid with modern C# collection expressions

// WRONG — old 1.0 patterns
[SpacetimeDB.Table(Name = "Player")]        // Use Accessor, not Name (2.0)
<PackageReference Include="SpacetimeDB.ServerSdk" />  // Use SpacetimeDB.Runtime
.WithModuleName("my-db")                    // Use .WithDatabaseName() (2.0)
ScheduleAt.Time(futureTime)                 // Use new ScheduleAt.Time(futureTime)

// WRONG — lifecycle hooks starting with "On"
[SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
public static void OnClientConnected(ReducerContext ctx) { }  // STDB0010 error!

// WRONG — non-deterministic code in reducers
var random = new Random();          // Use ctx.Rng
var guid = Guid.NewGuid();          // Not allowed
var now = DateTime.Now;             // Use ctx.Timestamp

// WRONG — collection parameters
int[] itemIds = { 1, 2, 3 };
_conn.Reducers.ProcessItems(itemIds);  // Generated code expects List<T>!
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
    [SpacetimeDB.Index.BTree]
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
var player = ctx.Db.Player.Id.Find(playerId);           // Unique/PK: returns nullable
foreach (var p in ctx.Db.Player.OwnerId.Filter(ctx.Sender)) { }  // BTree: returns IEnumerable

// CORRECT SUM TYPE — partial record with named tuple elements
[SpacetimeDB.Type]
public partial record Shape : TaggedEnum<(Circle Circle, Rectangle Rectangle)> { }

// CORRECT — collection parameters use List<T>
_conn.Reducers.ProcessItems(new List<int> { 1, 2, 3 });
```

---

## Common Mistakes Table

| Wrong | Right | Error |
|-------|-------|-------|
| Wrong .csproj name | `StdbModule.csproj` | Publish fails silently |
| .NET 9 SDK | .NET 8 SDK only | WASI compilation fails |
| Missing WASI workload | `dotnet workload install wasi-experimental` | Build fails |
| async/await in reducers | Synchronous only | Not supported |
| `table.Name.Update(...)` | `table.Id.Update(...)` | Update only via primary key (2.0) |
| Not calling `FrameTick()` | `conn.FrameTick()` in Update loop | No callbacks fire |
| Accessing `conn.Db` from background thread | Copy data in callback | Data races |

---

## Hard Requirements

1. **Tables and Module MUST be `partial`** — required for code generation
2. **Use `Accessor =` in table attributes** — `Name =` is only for SQL compatibility (2.0)
3. **Project file MUST be named `StdbModule.csproj`** — CLI requirement
4. **Requires .NET 8 SDK** — .NET 9 and newer not yet supported
5. **Install WASI workload** — `dotnet workload install wasi-experimental`
6. **Procedures are supported** — use `[SpacetimeDB.Procedure]` with `ProcedureContext` when needed
7. **Reducers must be deterministic** — no filesystem, network, timers, or `Random`
8. **Add `Public = true`** — if clients need to subscribe to a table
9. **Use `T?` for nullable fields** — not `Optional<T>`
10. **Pass `0` for auto-increment** — to trigger ID generation on insert
11. **Sum types must be `partial record`** — not struct or class
12. **Fully qualify Index attribute** — `[SpacetimeDB.Index.BTree]` to avoid System.Index ambiguity
13. **Update only via primary key** — use delete+insert for non-PK changes (2.0)
14. **Use `SpacetimeDB.Runtime` package** — not `ServerSdk` (2.0)
15. **Use `List<T>` for collection parameters** — not arrays
16. **`Identity` is in `SpacetimeDB` namespace** — not `SpacetimeDB.Types`

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

### SpacetimeDB Column Types

```csharp
Identity                     // User identity (SpacetimeDB namespace, not SpacetimeDB.Types)
Timestamp                    // Timestamp (use ctx.Timestamp server-side, never DateTime.Now)
ScheduleAt                   // For scheduled tables
T?                           // Nullable (e.g., string?)
List<T>                      // Collections (use List, not arrays)
```

Standard C# primitives (`bool`, `byte`..`ulong`, `float`, `double`, `string`) are all supported.

### Insert with Auto-Increment

```csharp
var player = ctx.Db.Player.Insert(new Player
{
    Id = 0,  // Pass 0 to trigger auto-increment
    OwnerId = ctx.Sender,
    Name = name,
    CreatedAt = ctx.Timestamp
});
ulong newId = player.Id;  // Insert returns the row with generated ID
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

    // CRITICAL: no "On" prefix!
    [SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        Log.Info($"Client connected: {ctx.Sender}");
        if (ctx.Db.User.Identity.Find(ctx.Sender) is User user)
        {
            ctx.Db.User.Identity.Update(user with { Online = true });
        }
        else
        {
            ctx.Db.User.Insert(new User { Identity = ctx.Sender, Online = true });
        }
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
    public static void ClientDisconnected(ReducerContext ctx)
    {
        if (ctx.Db.User.Identity.Find(ctx.Sender) is User user)
        {
            ctx.Db.User.Identity.Update(user with { Online = false });
        }
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

// Find by unique index — returns nullable
if (ctx.Db.Player.Username.Find("alice") is Player player) { }

// Filter by B-tree index — returns iterator
foreach (var task in ctx.Db.Task.OwnerId.Filter(ctx.Sender)) { }

// Full table scan — avoid for large tables
foreach (var task in ctx.Db.Task.Iter()) { }
var count = ctx.Db.Task.Count;
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

### Logging

```csharp
Log.Debug("Debug message");
Log.Info("Information");
Log.Warn("Warning");
Log.Error("Error occurred");
Log.Exception("Critical failure");  // Logs at error level
```

### ReducerContext API

```csharp
ctx.Sender          // Identity of the caller
ctx.Timestamp       // Current timestamp
ctx.Db              // Database access
ctx.Identity        // Module's own identity
ctx.ConnectionId    // Connection ID (nullable)
ctx.SenderAuth      // Authorization context (JWT claims, internal call detection)
ctx.Rng             // Deterministic random number generator
```

### Error Handling

Throwing an exception in a reducer rolls back the entire transaction:

```csharp
[SpacetimeDB.Reducer]
public static void TransferCredits(ReducerContext ctx, Identity toUser, uint amount)
{
    if (ctx.Db.User.Identity.Find(ctx.Sender) is not User sender)
        throw new Exception("Sender not found");

    if (sender.Credits < amount)
        throw new Exception("Insufficient credits");

    ctx.Db.User.Identity.Update(sender with { Credits = sender.Credits - amount });

    if (ctx.Db.User.Identity.Find(toUser) is User receiver)
        ctx.Db.User.Identity.Update(receiver with { Credits = receiver.Credits + amount });
}
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

### Installation

```bash
dotnet add package SpacetimeDB.ClientSDK
```

### Generate Module Bindings

```bash
spacetime generate --lang csharp --out-dir module_bindings --module-path PATH_TO_MODULE
```

This creates `SpacetimeDBClient.g.cs`, `Tables/*.g.cs`, `Reducers/*.g.cs`, and `Types/*.g.cs`.

### Connection Setup

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-database")
    .WithToken(savedToken)
    .OnConnect(OnConnected)
    .OnConnectError(err => Console.Error.WriteLine($"Failed: {err}"))
    .OnDisconnect((conn, err) => { if (err != null) Console.Error.WriteLine(err); })
    .Build();

void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    // Save authToken to persistent storage for reconnection
    Console.WriteLine($"Connected: {identity}");
    conn.SubscriptionBuilder()
        .OnApplied(OnSubscriptionApplied)
        .SubscribeToAllTables();
}
```

### Critical: FrameTick

**The SDK does NOT automatically process messages.** You must call `FrameTick()` regularly.

```csharp
// Console application
while (running) { conn.FrameTick(); Thread.Sleep(16); }

// Unity: call conn?.FrameTick() in Update()
```

**Warning**: Do NOT call `FrameTick()` from a background thread. It modifies `conn.Db` and can cause data races.

### Subscribing to Tables

```csharp
// SQL queries
conn.SubscriptionBuilder()
    .OnApplied(OnSubscriptionApplied)
    .OnError((ctx, err) => Console.Error.WriteLine($"Subscription failed: {err}"))
    .Subscribe(new[] {
        "SELECT * FROM player",
        "SELECT * FROM message WHERE sender = :sender"
    });

// Subscribe to all tables (development only)
conn.SubscriptionBuilder()
    .OnApplied(OnSubscriptionApplied)
    .SubscribeToAllTables();

// Subscription handle for later unsubscribe
SubscriptionHandle handle = conn.SubscriptionBuilder()
    .OnApplied(ctx => Console.WriteLine("Applied"))
    .Subscribe(new[] { "SELECT * FROM player" });

handle.UnsubscribeThen(ctx => Console.WriteLine("Unsubscribed"));
```

**Warning**: `SubscribeToAllTables()` cannot be mixed with `Subscribe()` on the same connection.

### Accessing the Client Cache

```csharp
// Iterate all rows
foreach (var player in ctx.Db.Player.Iter()) { Console.WriteLine(player.Name); }

// Count rows
int playerCount = ctx.Db.Player.Count;

// Find by unique/primary key — returns nullable
Player? player = ctx.Db.Player.Identity.Find(someIdentity);
if (player != null) { Console.WriteLine(player.Name); }

// Filter by BTree index — returns IEnumerable
foreach (var p in ctx.Db.Player.Level.Filter(1)) { }
```

### Row Event Callbacks

```csharp
ctx.Db.Player.OnInsert += (EventContext ctx, Player player) => {
    Console.WriteLine($"Player joined: {player.Name}");
};

ctx.Db.Player.OnDelete += (EventContext ctx, Player player) => {
    Console.WriteLine($"Player left: {player.Name}");
};

ctx.Db.Player.OnUpdate += (EventContext ctx, Player oldRow, Player newRow) => {
    Console.WriteLine($"Player {oldRow.Name} renamed to {newRow.Name}");
};

// Checking event source
ctx.Db.Player.OnInsert += (EventContext ctx, Player player) => {
    switch (ctx.Event)
    {
        case Event<Reducer>.SubscribeApplied:
            break;  // Initial subscription data
        case Event<Reducer>.Reducer(var reducerEvent):
            Console.WriteLine($"Reducer: {reducerEvent.Reducer}");
            break;
    }
};
```

### Calling Reducers

```csharp
ctx.Reducers.SendMessage("Hello, world!");
ctx.Reducers.CreatePlayer("NewPlayer");

// Reducer completion callbacks
conn.Reducers.OnSendMessage += (ReducerEventContext ctx, string text) => {
    if (ctx.Event.Status is Status.Committed)
        Console.WriteLine($"Message sent: {text}");
    else if (ctx.Event.Status is Status.Failed(var reason))
        Console.Error.WriteLine($"Send failed: {reason}");
};

// Unhandled reducer errors
conn.OnUnhandledReducerError += (ReducerEventContext ctx, Exception ex) => {
    Console.Error.WriteLine($"Reducer error: {ex.Message}");
};
```

### Identity and Authentication

```csharp
// In OnConnect callback — save token for reconnection
void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    // Save authToken to persistent storage (file, config, PlayerPrefs, etc.)
    SaveToken(authToken);
}

// Reconnect with saved token
string savedToken = LoadToken();
DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-database")
    .WithToken(savedToken)
    .OnConnect(OnConnected)
    .Build();

// Pass null or omit WithToken for anonymous connection
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
