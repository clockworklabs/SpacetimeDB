---
name: spacetimedb-csharp
description: Build C# modules and Unity clients for SpacetimeDB. Covers server-side module development and client SDK integration.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "1.1"
  tested_with: "SpacetimeDB runtime 1.11.x, .NET 8 SDK"
---

# SpacetimeDB C# SDK

This skill provides comprehensive guidance for building C# server-side modules and Unity/C# clients that connect to SpacetimeDB.

---

## HALLUCINATED APIs — DO NOT USE

**These APIs DO NOT EXIST. LLMs frequently hallucinate them.**

```csharp
// WRONG — these do not exist
[SpacetimeDB.Procedure]             // C# does NOT support procedures yet!
ctx.db.tableName                    // Wrong casing, should be PascalCase
ctx.Db.tableName.Get(id)            // Use Find, not Get
ctx.Db.TableName.FindById(id)       // Use index accessor: ctx.Db.TableName.Id.Find(id)
ctx.Db.table.field_name.Find(x)     // Wrong! Use PascalCase: ctx.Db.Table.FieldName.Find(x)
Optional<string> field;             // Use C# nullable: string? field

// WRONG — missing partial keyword
public struct MyTable { }           // Must be "partial struct"
public class Module { }             // Must be "static partial class"

// WRONG — non-partial types
[SpacetimeDB.Table(Name = "player")]
public struct Player { }            // WRONG — missing partial!

// WRONG — sum type syntax (VERY COMMON MISTAKE)
public partial struct Shape : TaggedEnum<(Circle, Rectangle)> { }     // WRONG: struct, missing names
public partial record Shape : TaggedEnum<(Circle, Rectangle)> { }     // WRONG: missing variant names
public partial class Shape : TaggedEnum<(Circle Circle, Rectangle Rectangle)> { }  // WRONG: class

// WRONG — Index attribute without full qualification
[Index.BTree(Name = "idx", Columns = new[] { "Col" })]    // Ambiguous with System.Index!
[Index.BTree(Name = "idx", Columns = ["Col"])]            // Collection expressions don't work in attributes!
```

### CORRECT PATTERNS

```csharp
// CORRECT IMPORTS
using SpacetimeDB;

// CORRECT TABLE — must be partial struct
[SpacetimeDB.Table(Name = "player", Public = true)]
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

### DO NOT

- **Forget `partial` keyword** — required on all tables and Module class
- **Use lowercase table access** — `ctx.Db.Player` not `ctx.Db.player`
- **Try to use procedures** — C# does not support procedures yet
- **Use `Optional<T>`** — use C# nullable syntax `T?` instead
- **Use struct for sum types** — must be `partial record`

---

## Common Mistakes Table

### Server-side errors

| Wrong | Right | Error |
|-------|-------|-------|
| Missing `partial` keyword | `public partial struct Table` | Generated code won't compile |
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

### Client-side errors

| Wrong | Right | Error |
|-------|-------|-------|
| Wrong namespace | `using SpacetimeDB.ClientApi;` | Types not found |
| Not calling `FrameTick()` | `conn.FrameTick()` in Update loop | No callbacks fire |
| Accessing `conn.Db` from background thread | Copy data in callback, process elsewhere | Data races |

---

## Hard Requirements

1. **Tables and Module MUST be `partial`** — required for code generation
2. **Use PascalCase for table access** — `ctx.Db.TableName`, not `ctx.Db.tableName`
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

---

## Server-Side Module Development

### Table Definition (CRITICAL)

**Tables MUST use `partial struct` or `partial class` for code generation.**

```csharp
using SpacetimeDB;

// WRONG — missing partial!
[SpacetimeDB.Table(Name = "player")]
public struct Player { }  // Will not generate properly!

// RIGHT — with partial keyword
[SpacetimeDB.Table(Name = "player", Public = true)]
public partial struct Player
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public Identity OwnerId;
    public string Name;
    public Timestamp CreatedAt;
}

// With indexes
[SpacetimeDB.Table(Name = "task", Public = true)]
public partial struct Task
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    [SpacetimeDB.Index.BTree]
    public Identity OwnerId;

    public string Title;
    public bool Completed;
}

// Multi-column index
[SpacetimeDB.Table(Name = "score", Public = true)]
[SpacetimeDB.Index.BTree(Name = "by_player_game", Columns = new[] { "PlayerId", "GameId" })]
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

### Column Types

```csharp
byte, sbyte, short, ushort   // 8/16-bit integers
int, uint, long, ulong       // 32/64-bit integers
float, double                // Floats
bool                         // Boolean
string                       // Text
Identity                     // User identity
Timestamp                    // Timestamp
ScheduleAt                   // For scheduled tables
T?                           // Nullable (e.g., string?)
List<T>                      // Arrays
```

### Insert with Auto-increment

```csharp
// Insert returns the row with generated ID
var player = ctx.Db.Player.Insert(new Player
{
    Id = 0,  // Pass 0 to trigger auto-increment
    OwnerId = ctx.Sender,
    Name = name,
    CreatedAt = ctx.Timestamp
});
ulong newId = player.Id;  // Get actual generated ID
```

### Module and Reducers

**The Module class MUST be `public static partial class`.**

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void CreateTask(ReducerContext ctx, string title)
    {
        // Validate
        if (string.IsNullOrEmpty(title))
        {
            throw new Exception("Title cannot be empty");  // Rolls back transaction
        }

        // Insert
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
        var task = ctx.Db.Task.Id.Find(taskId);
        if (task is null)
        {
            throw new Exception("Task not found");
        }

        if (task.Value.OwnerId != ctx.Sender)
        {
            throw new Exception("Not authorized");
        }

        ctx.Db.Task.Id.Update(task.Value with { Completed = true });
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
        // Called once when module is first published
        Log.Info("Module initialized");
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
    public static void OnConnect(ReducerContext ctx)
    {
        // ctx.Sender is the connecting client
        Log.Info($"Client connected: {ctx.Sender}");
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
    public static void OnDisconnect(ReducerContext ctx)
    {
        // Clean up client state
        Log.Info($"Client disconnected: {ctx.Sender}");
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

### Database Access

#### Naming Convention

- **Tables**: Use PascalCase singular names in the `Name` attribute
  - `[Table(Name = "User")]` → `ctx.Db.User`
  - `[Table(Name = "PlayerStats")]` → `ctx.Db.PlayerStats`
- **Indexes**: PascalCase, match field name
  - Field `OwnerId` with `[Index.BTree]` → `ctx.Db.User.OwnerId`

#### Primary Key Operations

```csharp
// Find by primary key — returns nullable
if (ctx.Db.Task.Id.Find(taskId) is Task task)
{
    // Use task
}

// Update by primary key
ctx.Db.Task.Id.Update(updatedTask);

// Delete by primary key
ctx.Db.Task.Id.Delete(taskId);
```

#### Index Operations

```csharp
// Find by unique index — returns nullable
if (ctx.Db.Player.Username.Find("alice") is Player player)
{
    // Found player
}

// Filter by B-tree index — returns iterator
foreach (var task in ctx.Db.Task.OwnerId.Filter(ctx.Sender))
{
    // Process each task
}
```

#### Iterate All Rows

```csharp
// Full table scan
foreach (var task in ctx.Db.Task.Iter())
{
    // Process each task
}
```

### Custom Types

**Use `[SpacetimeDB.Type]` for custom structs/enums. Must be `partial`.**

```csharp
using SpacetimeDB;

[SpacetimeDB.Type]
public partial struct Position
{
    public int X;
    public int Y;
}

[SpacetimeDB.Type]
public partial struct PlayerStats
{
    public int Health;
    public int Mana;
    public Position Location;
}

// Use in table
[SpacetimeDB.Table(Name = "player", Public = true)]
public partial struct Player
{
    [SpacetimeDB.PrimaryKey]
    public Identity Id;

    public string Name;
    public PlayerStats Stats;
}
```

### Sum Types / Tagged Enums (CRITICAL)

**Sum types MUST use `partial record` and inherit from `TaggedEnum<T>`.**

```csharp
using SpacetimeDB;

// Step 1: Define variant types as partial structs with [Type]
[SpacetimeDB.Type]
public partial struct Circle { public int Radius; }

[SpacetimeDB.Type]
public partial struct Rectangle { public int Width; public int Height; }

// Step 2: Define sum type as partial RECORD (not struct!) inheriting TaggedEnum
// The tuple MUST include both the type AND a name for each variant
[SpacetimeDB.Type]
public partial record Shape : TaggedEnum<(Circle Circle, Rectangle Rectangle)> { }

// Step 3: Use in a table
[SpacetimeDB.Table(Name = "drawings", Public = true)]
public partial struct Drawing
{
    [SpacetimeDB.PrimaryKey]
    public int Id;
    public Shape ShapeA;
    public Shape ShapeB;
}
```

#### Creating Sum Type Values

```csharp
// Create variant instances using the generated nested types
var circle = new Shape.Circle(new Circle { Radius = 10 });
var rect = new Shape.Rectangle(new Rectangle { Width = 4, Height = 6 });

// Insert into table
ctx.Db.Drawing.Insert(new Drawing { Id = 1, ShapeA = circle, ShapeB = rect });
```

#### COMMON SUM TYPE MISTAKES

| Wrong | Right | Why |
|-------|-------|-----|
| `partial struct Shape : TaggedEnum<...>` | `partial record Shape : TaggedEnum<...>` | Must be `record`, not `struct` |
| `TaggedEnum<(Circle, Rectangle)>` | `TaggedEnum<(Circle Circle, Rectangle Rectangle)>` | Tuple must have names |
| `new Shape { ... }` | `new Shape.Circle(new Circle { ... })` | Use nested variant constructor |

### Scheduled Tables

```csharp
using SpacetimeDB;

[SpacetimeDB.Table(Name = "reminder", Scheduled = nameof(Module.SendReminder))]
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
    // Scheduled reducer receives the full row
    [SpacetimeDB.Reducer]
    public static void SendReminder(ReducerContext ctx, Reminder reminder)
    {
        Log.Info($"Reminder: {reminder.Message}");
        // Row is automatically deleted after reducer completes
    }

    [SpacetimeDB.Reducer]
    public static void CreateReminder(ReducerContext ctx, string message, ulong delaySecs)
    {
        var futureTime = ctx.Timestamp + TimeSpan.FromSeconds(delaySecs);
        ctx.Db.Reminder.Insert(new Reminder
        {
            Id = 0,
            Message = message,
            ScheduledAt = ScheduleAt.Time(futureTime)
        });
    }

    [SpacetimeDB.Reducer]
    public static void CancelReminder(ReducerContext ctx, ulong reminderId)
    {
        ctx.Db.Reminder.Id.Delete(reminderId);
    }
}
```

### Logging

```csharp
using SpacetimeDB;

Log.Debug("Debug message");
Log.Info("Information");
Log.Warn("Warning");
Log.Error("Error occurred");
Log.Panic("Critical failure");  // Terminates execution
```

### Data Visibility

**`Public = true` exposes ALL rows to ALL clients.**

| Scenario | Pattern |
|----------|---------|
| Everyone sees all rows | `[Table(Name = "x", Public = true)]` |
| Server-only data | `[Table(Name = "x")]` (private by default) |

### Project Setup

#### Required .csproj (MUST be named `StdbModule.csproj`)

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
    <PackageReference Include="SpacetimeDB.ServerSdk" Version="1.*" />
  </ItemGroup>
</Project>
```

#### Prerequisites

```bash
# Install .NET 8 SDK (required, not .NET 9)
# Download from https://dotnet.microsoft.com/download/dotnet/8.0

# Install WASI workload
dotnet workload install wasi-experimental
```

### Commands

```bash
# Start local server
spacetime start

# Publish module
spacetime publish <module-name> --project-path <backend-dir>

# Clear database and republish
spacetime publish <module-name> --clear-database -y --project-path <backend-dir>

# Generate bindings
spacetime generate --lang csharp --out-dir <client>/SpacetimeDB --project-path <backend-dir>

# View logs
spacetime logs <module-name>
```

---

## Client-Side SDK

### Overview

The SpacetimeDB C# SDK enables .NET applications and Unity games to:
- Connect to SpacetimeDB databases over WebSocket
- Subscribe to real-time table updates
- Invoke reducers (server-side functions)
- Maintain a local cache of subscribed data
- Handle authentication via Identity tokens

**Critical Requirement**: The C# SDK requires manual connection advancement. You must call `FrameTick()` regularly to process messages.

### Installation

#### .NET Console/Library Applications

Add the NuGet package:

```bash
dotnet add package SpacetimeDB.ClientSDK
```

#### Unity Applications

Add via Unity Package Manager using the git URL:

```
https://github.com/clockworklabs/com.clockworklabs.spacetimedbsdk.git
```

Steps:
1. Open Window > Package Manager
2. Click the + button in top-left
3. Select "Add package from git URL"
4. Paste the URL above and click Add

### Generate Module Bindings

Before using the SDK, generate type-safe bindings from your module:

```bash
mkdir -p module_bindings
spacetime generate --lang cs --out-dir module_bindings --project-path PATH_TO_MODULE
```

This creates:
- `SpacetimeDBClient.g.cs` - Main client types (DbConnection, contexts, builders)
- `Tables/*.g.cs` - Table handle classes with typed access
- `Reducers/*.g.cs` - Reducer invocation methods
- `Types/*.g.cs` - Row types and custom types from the module

### Connection Setup

#### Basic Connection Pattern

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;

DbConnection? conn = null;

conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")           // SpacetimeDB server URL
    .WithModuleName("my-database")              // Database name or Identity
    .OnConnect(OnConnected)                     // Connection success callback
    .OnConnectError((err) => {                  // Connection failure callback
        Console.Error.WriteLine($"Connection failed: {err}");
    })
    .OnDisconnect((conn, err) => {              // Disconnection callback
        if (err != null) {
            Console.Error.WriteLine($"Disconnected with error: {err}");
        }
    })
    .Build();

void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    Console.WriteLine($"Connected with Identity: {identity}");
    // Save authToken for reconnection
    // Set up subscriptions here
}
```

#### Connection Builder Methods

| Method | Description |
|--------|-------------|
| `WithUri(string uri)` | SpacetimeDB server URI (required) |
| `WithModuleName(string name)` | Database name or Identity (required) |
| `WithToken(string token)` | Auth token for reconnection |
| `WithConfirmedReads(bool)` | Wait for durable writes before returning |
| `OnConnect(callback)` | Called on successful connection |
| `OnConnectError(callback)` | Called if connection fails |
| `OnDisconnect(callback)` | Called when disconnected |
| `Build()` | Create and open the connection |

### Critical: Advancing the Connection

**The SDK does NOT automatically process messages.** You must call `FrameTick()` regularly.

#### Console Application Loop

```csharp
while (true)
{
    conn.FrameTick();
    Thread.Sleep(16); // ~60 FPS
}
```

#### Unity MonoBehaviour Pattern

```csharp
public class SpacetimeManager : MonoBehaviour
{
    private DbConnection conn;

    void Update()
    {
        conn?.FrameTick();
    }
}
```

**Warning**: Do NOT call `FrameTick()` from a background thread. It modifies `conn.Db` and can cause data races with main thread access.

### Subscribing to Tables

#### Using SQL Queries

```csharp
void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    conn.SubscriptionBuilder()
        .OnApplied(OnSubscriptionApplied)
        .OnError((ctx, err) => {
            Console.Error.WriteLine($"Subscription failed: {err}");
        })
        .Subscribe(new[] {
            "SELECT * FROM player",
            "SELECT * FROM message WHERE sender = :sender"
        });
}

void OnSubscriptionApplied(SubscriptionEventContext ctx)
{
    Console.WriteLine("Subscription ready - data available");
    // Access ctx.Db to read subscribed rows
}
```

#### Using Typed Query Builder

```csharp
conn.SubscriptionBuilder()
    .OnApplied(OnSubscriptionApplied)
    .OnError((ctx, err) => Console.Error.WriteLine(err))
    .AddQuery(qb => qb.From.Player().Build())
    .AddQuery(qb => qb.From.Message().Where(c => c.Sender.Eq(identity)).Build())
    .Subscribe();
```

#### Subscribe to All Tables (Development Only)

```csharp
conn.SubscriptionBuilder()
    .OnApplied(OnSubscriptionApplied)
    .SubscribeToAllTables();
```

**Warning**: `SubscribeToAllTables()` cannot be mixed with `Subscribe()` on the same connection.

#### Subscription Handle

```csharp
SubscriptionHandle handle = conn.SubscriptionBuilder()
    .OnApplied(ctx => Console.WriteLine("Applied"))
    .Subscribe(new[] { "SELECT * FROM player" });

// Later: unsubscribe
handle.UnsubscribeThen(ctx => {
    Console.WriteLine("Unsubscribed");
});

// Check status
bool isActive = handle.IsActive;
bool isEnded = handle.IsEnded;
```

### Accessing the Client Cache

Subscribed data is stored in `conn.Db` (or `ctx.Db` in callbacks).

#### Iterating All Rows

```csharp
foreach (var player in ctx.Db.Player.Iter())
{
    Console.WriteLine($"Player: {player.Name}");
}
```

#### Count Rows

```csharp
int playerCount = ctx.Db.Player.Count;
```

#### Find by Unique/Primary Key

For columns marked `[Unique]` or `[PrimaryKey]` on the server:

```csharp
// Find by unique column
Player? player = ctx.Db.Player.Identity.Find(someIdentity);

// Returns null if not found
if (player != null)
{
    Console.WriteLine($"Found: {player.Name}");
}
```

#### Filter by BTree Index

For columns with `[Index.BTree]` on the server:

```csharp
// Filter returns IEnumerable
IEnumerable<Player> levelOnePlayers = ctx.Db.Player.Level.Filter(1);

int count = levelOnePlayers.Count();
```

#### Remote Query (Ad-hoc SQL)

```csharp
var result = ctx.Db.Player.RemoteQuery("WHERE level > 10");
Player[] highLevelPlayers = result.Result;
```

### Row Event Callbacks

Register callbacks to react to table changes:

#### OnInsert

```csharp
ctx.Db.Player.OnInsert += (EventContext ctx, Player player) => {
    Console.WriteLine($"Player joined: {player.Name}");
};
```

#### OnDelete

```csharp
ctx.Db.Player.OnDelete += (EventContext ctx, Player player) => {
    Console.WriteLine($"Player left: {player.Name}");
};
```

#### OnUpdate

Fires when a row with a primary key is replaced:

```csharp
ctx.Db.Player.OnUpdate += (EventContext ctx, Player oldRow, Player newRow) => {
    Console.WriteLine($"Player {oldRow.Name} renamed to {newRow.Name}");
};
```

#### Checking Event Source

```csharp
ctx.Db.Player.OnInsert += (EventContext ctx, Player player) => {
    switch (ctx.Event)
    {
        case Event<Reducer>.SubscribeApplied:
            // Initial subscription data
            break;
        case Event<Reducer>.Reducer(var reducerEvent):
            // Change from a reducer
            Console.WriteLine($"Reducer: {reducerEvent.Reducer}");
            break;
    }
};
```

### Calling Reducers

Reducers are server-side functions that modify the database.

#### Invoke a Reducer

```csharp
// Reducers are methods on ctx.Reducers or conn.Reducers
ctx.Reducers.SendMessage("Hello, world!");
ctx.Reducers.CreatePlayer("NewPlayer");
ctx.Reducers.UpdateScore(playerId, 100);
```

#### Reducer Callbacks

React when a reducer completes (success or failure):

```csharp
conn.Reducers.OnSendMessage += (ReducerEventContext ctx, string text) => {
    if (ctx.Event.Status is Status.Committed)
    {
        Console.WriteLine($"Message sent: {text}");
    }
    else if (ctx.Event.Status is Status.Failed(var reason))
    {
        Console.Error.WriteLine($"Send failed: {reason}");
    }
};
```

#### Unhandled Reducer Errors

Catch reducer errors without specific handlers:

```csharp
conn.OnUnhandledReducerError += (ReducerEventContext ctx, Exception ex) => {
    Console.Error.WriteLine($"Reducer error: {ex.Message}");
};
```

#### Reducer Event Properties

```csharp
conn.Reducers.OnSendMessage += (ReducerEventContext ctx, string text) => {
    ReducerEvent<Reducer> evt = ctx.Event;

    Timestamp when = evt.Timestamp;
    Status status = evt.Status;
    Identity caller = evt.CallerIdentity;
    ConnectionId? callerId = evt.CallerConnectionId;
    U128? energy = evt.EnergyConsumed;
};
```

### Identity and Authentication

#### Getting Current Identity

```csharp
// In OnConnect callback
void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    // identity - your unique identifier
    // authToken - save this for reconnection
    PlayerPrefs.SetString("SpacetimeToken", authToken);
}

// From any context
Identity? myIdentity = ctx.Identity;
ConnectionId myConnectionId = ctx.ConnectionId;
```

#### Reconnecting with Token

```csharp
string savedToken = PlayerPrefs.GetString("SpacetimeToken", null);

DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithModuleName("my-database")
    .WithToken(savedToken)  // Reconnect as same identity
    .OnConnect(OnConnected)
    .Build();
```

#### Anonymous Connection

Pass `null` to `WithToken` or omit it entirely for a new anonymous identity.

### BSATN Serialization

SpacetimeDB uses BSATN (Binary SpacetimeDB Algebraic Type Notation) for serialization. The SDK handles this automatically for generated types.

#### Supported Types

| C# Type | SpacetimeDB Type |
|---------|------------------|
| `bool` | Bool |
| `byte`, `sbyte` | U8, I8 |
| `ushort`, `short` | U16, I16 |
| `uint`, `int` | U32, I32 |
| `ulong`, `long` | U64, I64 |
| `U128`, `I128` | U128, I128 |
| `U256`, `I256` | U256, I256 |
| `float`, `double` | F32, F64 |
| `string` | String |
| `List<T>` | Array |
| `T?` (nullable) | Option |
| `Identity` | Identity |
| `ConnectionId` | ConnectionId |
| `Timestamp` | Timestamp |
| `Uuid` | Uuid |

#### Custom Types

Types marked with `[SpacetimeDB.Type]` on the server are generated as C# types:

```csharp
// Server-side (Rust or C#)
[SpacetimeDB.Type]
public partial struct Vector3
{
    public float X;
    public float Y;
    public float Z;
}

// Client-side (auto-generated)
public partial struct Vector3 : IEquatable<Vector3>
{
    public float X;
    public float Y;
    public float Z;
    // BSATN serialization methods included
}
```

#### TaggedEnum (Sum Types) on Client

```csharp
// Server
[SpacetimeDB.Type]
public partial record GameEvent : TaggedEnum<(
    string PlayerJoined,
    string PlayerLeft,
    (string player, int score) ScoreUpdate
)>;

// Client usage
switch (gameEvent)
{
    case GameEvent.PlayerJoined(var name):
        Console.WriteLine($"{name} joined");
        break;
    case GameEvent.ScoreUpdate((var player, var score)):
        Console.WriteLine($"{player} scored {score}");
        break;
}
```

#### Result Type

```csharp
// Result<T, E> for success/error handling
Result<Player, string> result = ...;

if (result is Result<Player, string>.Ok(var player))
{
    Console.WriteLine($"Success: {player.Name}");
}
else if (result is Result<Player, string>.Err(var error))
{
    Console.Error.WriteLine($"Error: {error}");
}
```

### Unity Integration

#### Project Setup

1. Add the SpacetimeDB package via Package Manager
2. Generate bindings and add to your Unity project
3. Create a manager MonoBehaviour

#### SpacetimeManager Pattern

```csharp
using UnityEngine;
using SpacetimeDB;
using SpacetimeDB.Types;

public class SpacetimeManager : MonoBehaviour
{
    public static SpacetimeManager Instance { get; private set; }

    [SerializeField] private string serverUri = "http://localhost:3000";
    [SerializeField] private string moduleName = "my-game";

    private DbConnection conn;
    public DbConnection Connection => conn;

    void Awake()
    {
        if (Instance != null)
        {
            Destroy(gameObject);
            return;
        }
        Instance = this;
        DontDestroyOnLoad(gameObject);
    }

    void Start()
    {
        Connect();
    }

    void Update()
    {
        // CRITICAL: Must call every frame
        conn?.FrameTick();
    }

    void OnDestroy()
    {
        conn?.Disconnect();
    }

    public void Connect()
    {
        string token = PlayerPrefs.GetString("SpacetimeToken", null);

        conn = DbConnection.Builder()
            .WithUri(serverUri)
            .WithModuleName(moduleName)
            .WithToken(token)
            .OnConnect(OnConnected)
            .OnConnectError(OnConnectError)
            .OnDisconnect(OnDisconnect)
            .Build();
    }

    private void OnConnected(DbConnection conn, Identity identity, string authToken)
    {
        Debug.Log($"Connected as {identity}");
        PlayerPrefs.SetString("SpacetimeToken", authToken);

        conn.SubscriptionBuilder()
            .OnApplied(OnSubscriptionApplied)
            .OnError((ctx, err) => Debug.LogError($"Subscription error: {err}"))
            .SubscribeToAllTables();
    }

    private void OnConnectError(Exception err)
    {
        Debug.LogError($"Connection failed: {err}");
    }

    private void OnDisconnect(DbConnection conn, Exception err)
    {
        if (err != null)
        {
            Debug.LogError($"Disconnected: {err}");
        }
    }

    private void OnSubscriptionApplied(SubscriptionEventContext ctx)
    {
        Debug.Log("Subscription ready");
        // Initialize game state from ctx.Db
    }
}
```

#### Unity-Specific Considerations

1. **Main Thread Only**: All SpacetimeDB callbacks run on the main thread (during `FrameTick()`)

2. **Scene Loading**: Use `DontDestroyOnLoad` for the connection manager

3. **Reconnection**: Handle disconnects gracefully for mobile/poor connectivity

4. **PlayerPrefs**: Use for token persistence (or use a more secure method for production)

#### Spawning GameObjects from Table Data

```csharp
public class PlayerSpawner : MonoBehaviour
{
    [SerializeField] private GameObject playerPrefab;
    private Dictionary<Identity, GameObject> playerObjects = new();

    void Start()
    {
        var conn = SpacetimeManager.Instance.Connection;

        conn.Db.Player.OnInsert += OnPlayerInsert;
        conn.Db.Player.OnDelete += OnPlayerDelete;
        conn.Db.Player.OnUpdate += OnPlayerUpdate;

        // Spawn existing players
        foreach (var player in conn.Db.Player.Iter())
        {
            SpawnPlayer(player);
        }
    }

    void OnPlayerInsert(EventContext ctx, Player player)
    {
        // Skip if this is initial subscription data we already handled
        if (ctx.Event is Event<Reducer>.SubscribeApplied) return;

        SpawnPlayer(player);
    }

    void OnPlayerDelete(EventContext ctx, Player player)
    {
        if (playerObjects.TryGetValue(player.Identity, out var go))
        {
            Destroy(go);
            playerObjects.Remove(player.Identity);
        }
    }

    void OnPlayerUpdate(EventContext ctx, Player oldPlayer, Player newPlayer)
    {
        if (playerObjects.TryGetValue(newPlayer.Identity, out var go))
        {
            // Update position, etc.
            go.transform.position = new Vector3(newPlayer.X, newPlayer.Y, newPlayer.Z);
        }
    }

    void SpawnPlayer(Player player)
    {
        var go = Instantiate(playerPrefab);
        go.transform.position = new Vector3(player.X, player.Y, player.Z);
        playerObjects[player.Identity] = go;
    }
}
```

### Thread Safety

The C# SDK is NOT thread-safe. Follow these rules:

1. **Call `FrameTick()` from ONE thread only** (main thread recommended)

2. **All callbacks run during `FrameTick()`** on the calling thread

3. **Do NOT access `conn.Db` from other threads** while `FrameTick()` may be running

4. **Background work**: Copy data out of callbacks, process on background threads

```csharp
// Safe pattern for background processing
conn.Db.Player.OnInsert += (ctx, player) => {
    // Copy the data
    var playerData = new PlayerDTO {
        Id = player.Id,
        Name = player.Name
    };

    // Process on background thread
    Task.Run(() => ProcessPlayerAsync(playerData));
};
```

### Error Handling

#### Connection Errors

```csharp
.OnConnectError((err) => {
    // Network errors, invalid module name, etc.
    Debug.LogError($"Connect error: {err}");
})
```

#### Subscription Errors

```csharp
.OnError((ctx, err) => {
    // Invalid SQL, schema changes, etc.
    Debug.LogError($"Subscription error: {err}");
})
```

#### Reducer Errors

```csharp
conn.Reducers.OnMyReducer += (ctx, args) => {
    if (ctx.Event.Status is Status.Failed(var reason))
    {
        Debug.LogError($"Reducer failed: {reason}");
    }
};

// Catch-all for unhandled reducer errors
conn.OnUnhandledReducerError += (ctx, ex) => {
    Debug.LogError($"Unhandled: {ex}");
};
```

### Complete Console Example

```csharp
using System;
using SpacetimeDB;
using SpacetimeDB.Types;

class Program
{
    static DbConnection? conn;
    static bool running = true;

    static void Main()
    {
        conn = DbConnection.Builder()
            .WithUri("http://localhost:3000")
            .WithModuleName("chat")
            .OnConnect(OnConnect)
            .OnConnectError(err => Console.Error.WriteLine($"Failed: {err}"))
            .OnDisconnect((c, err) => running = false)
            .Build();

        while (running)
        {
            conn.FrameTick();
            Thread.Sleep(16);
        }
    }

    static void OnConnect(DbConnection conn, Identity id, string token)
    {
        Console.WriteLine($"Connected as {id}");

        // Set up callbacks
        conn.Db.Message.OnInsert += (ctx, msg) => {
            Console.WriteLine($"[{msg.Sender}]: {msg.Text}");
        };

        conn.Reducers.OnSendMessage += (ctx, text) => {
            if (ctx.Event.Status is Status.Failed(var reason))
            {
                Console.Error.WriteLine($"Send failed: {reason}");
            }
        };

        // Subscribe
        conn.SubscriptionBuilder()
            .OnApplied(ctx => {
                Console.WriteLine("Ready! Type messages:");
                StartInputLoop(ctx);
            })
            .Subscribe(new[] { "SELECT * FROM message" });
    }

    static void StartInputLoop(SubscriptionEventContext ctx)
    {
        Task.Run(() => {
            while (running)
            {
                var input = Console.ReadLine();
                if (!string.IsNullOrEmpty(input))
                {
                    ctx.Reducers.SendMessage(input);
                }
            }
        });
    }
}
```

### Common Patterns

#### Optimistic Updates

```csharp
// Show immediate feedback, correct on server response
void SendMessage(string text)
{
    // Optimistic: show immediately
    AddMessageToUI(myIdentity, text, isPending: true);

    // Send to server
    conn.Reducers.SendMessage(text);
}

conn.Reducers.OnSendMessage += (ctx, text) => {
    if (ctx.Event.CallerIdentity == conn.Identity)
    {
        if (ctx.Event.Status is Status.Committed)
        {
            // Confirm the pending message
            ConfirmPendingMessage(text);
        }
        else
        {
            // Remove failed message
            RemovePendingMessage(text);
        }
    }
};
```

#### Local Player Detection

```csharp
conn.Db.Player.OnInsert += (ctx, player) => {
    bool isLocalPlayer = player.Identity == ctx.Identity;

    if (isLocalPlayer)
    {
        // This is our player
        SetupLocalPlayerController(player);
    }
    else
    {
        // Remote player
        SpawnRemotePlayer(player);
    }
};
```

#### Waiting for Specific Data

```csharp
async Task<Player> WaitForPlayerAsync(Identity playerId)
{
    var tcs = new TaskCompletionSource<Player>();

    void Handler(EventContext ctx, Player player)
    {
        if (player.Identity == playerId)
        {
            tcs.TrySetResult(player);
            conn.Db.Player.OnInsert -= Handler;
        }
    }

    // Check if already exists
    var existing = conn.Db.Player.Identity.Find(playerId);
    if (existing != null) return existing;

    conn.Db.Player.OnInsert += Handler;
    return await tcs.Task;
}
```

---

## Troubleshooting

### Connection Issues

- **"Connection refused"**: Check server is running at the specified URI
- **"Module not found"**: Verify module name matches published database
- **Timeout**: Check firewall/network, ensure `FrameTick()` is being called

### No Callbacks Firing

- **Check `FrameTick()`**: Must be called regularly
- **Check subscription**: Ensure `OnApplied` fired successfully
- **Check callback registration**: Register before subscribing

### Data Not Appearing

- **Check SQL syntax**: Invalid queries fail silently
- **Check table visibility**: Tables must be `Public = true` in the module
- **Check subscription scope**: Only subscribed rows are cached

### Unity-Specific

- **NullReferenceException in Update**: Guard with `conn?.FrameTick()`
- **Missing types**: Regenerate bindings after module changes
- **Assembly errors**: Ensure SpacetimeDB assemblies are in correct folder

### Build Issues

- **WASI compilation fails**: Ensure .NET 8 SDK (not 9+), install WASI workload
- **Publish fails silently**: Ensure project is named `StdbModule.csproj`
- **Generated code errors**: Ensure all tables/types have `partial` keyword

---

## References

- [C# SDK Reference](https://spacetimedb.com/docs/sdks/c-sharp)
- [Unity Tutorial](https://spacetimedb.com/docs/unity/part-1)
- [SpacetimeDB SQL Reference](https://spacetimedb.com/docs/sql)
- [GitHub: Unity Demo (Blackholio)](https://github.com/clockworklabs/SpacetimeDB/tree/master/demo/Blackholio)
