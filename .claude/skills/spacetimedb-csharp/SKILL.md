---
name: spacetimedb-csharp
description: Build C# and Unity clients for SpacetimeDB. Use when integrating SpacetimeDB with Unity games or .NET applications.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "1.0"
---

# SpacetimeDB C# / Unity SDK

This skill provides comprehensive guidance for building C# and Unity clients that connect to SpacetimeDB modules.

## Overview

The SpacetimeDB C# SDK enables .NET applications and Unity games to:
- Connect to SpacetimeDB databases over WebSocket
- Subscribe to real-time table updates
- Invoke reducers (server-side functions)
- Maintain a local cache of subscribed data
- Handle authentication via Identity tokens

**Critical Requirement**: The C# SDK requires manual connection advancement. You must call `FrameTick()` regularly to process messages.

## Installation

### .NET Console/Library Applications

Add the NuGet package:

```bash
dotnet add package SpacetimeDB.ClientSDK
```

### Unity Applications

Add via Unity Package Manager using the git URL:

```
https://github.com/clockworklabs/com.clockworklabs.spacetimedbsdk.git
```

Steps:
1. Open Window > Package Manager
2. Click the + button in top-left
3. Select "Add package from git URL"
4. Paste the URL above and click Add

## Generate Module Bindings

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

## Connection Setup

### Basic Connection Pattern

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

### Connection Builder Methods

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

## Critical: Advancing the Connection

**The SDK does NOT automatically process messages.** You must call `FrameTick()` regularly.

### Console Application Loop

```csharp
while (true)
{
    conn.FrameTick();
    Thread.Sleep(16); // ~60 FPS
}
```

### Unity MonoBehaviour Pattern

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

## Subscribing to Tables

### Using SQL Queries

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

### Using Typed Query Builder

```csharp
conn.SubscriptionBuilder()
    .OnApplied(OnSubscriptionApplied)
    .OnError((ctx, err) => Console.Error.WriteLine(err))
    .AddQuery(qb => qb.From.Player().Build())
    .AddQuery(qb => qb.From.Message().Where(c => c.Sender.Eq(identity)).Build())
    .Subscribe();
```

### Subscribe to All Tables (Development Only)

```csharp
conn.SubscriptionBuilder()
    .OnApplied(OnSubscriptionApplied)
    .SubscribeToAllTables();
```

**Warning**: `SubscribeToAllTables()` cannot be mixed with `Subscribe()` on the same connection.

### Subscription Handle

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

## Accessing the Client Cache

Subscribed data is stored in `conn.Db` (or `ctx.Db` in callbacks).

### Iterating All Rows

```csharp
foreach (var player in ctx.Db.Player.Iter())
{
    Console.WriteLine($"Player: {player.Name}");
}
```

### Count Rows

```csharp
int playerCount = ctx.Db.Player.Count;
```

### Find by Unique/Primary Key

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

### Filter by BTree Index

For columns with `[Index.BTree]` on the server:

```csharp
// Filter returns IEnumerable
IEnumerable<Player> levelOnePlayers = ctx.Db.Player.Level.Filter(1);

int count = levelOnePlayers.Count();
```

### Remote Query (Ad-hoc SQL)

```csharp
var result = ctx.Db.Player.RemoteQuery("WHERE level > 10");
Player[] highLevelPlayers = result.Result;
```

## Row Event Callbacks

Register callbacks to react to table changes:

### OnInsert

```csharp
ctx.Db.Player.OnInsert += (EventContext ctx, Player player) => {
    Console.WriteLine($"Player joined: {player.Name}");
};
```

### OnDelete

```csharp
ctx.Db.Player.OnDelete += (EventContext ctx, Player player) => {
    Console.WriteLine($"Player left: {player.Name}");
};
```

### OnUpdate

Fires when a row with a primary key is replaced:

```csharp
ctx.Db.Player.OnUpdate += (EventContext ctx, Player oldRow, Player newRow) => {
    Console.WriteLine($"Player {oldRow.Name} renamed to {newRow.Name}");
};
```

### Checking Event Source

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

## Calling Reducers

Reducers are server-side functions that modify the database.

### Invoke a Reducer

```csharp
// Reducers are methods on ctx.Reducers or conn.Reducers
ctx.Reducers.SendMessage("Hello, world!");
ctx.Reducers.CreatePlayer("NewPlayer");
ctx.Reducers.UpdateScore(playerId, 100);
```

### Reducer Callbacks

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

### Unhandled Reducer Errors

Catch reducer errors without specific handlers:

```csharp
conn.OnUnhandledReducerError += (ReducerEventContext ctx, Exception ex) => {
    Console.Error.WriteLine($"Reducer error: {ex.Message}");
};
```

### Reducer Event Properties

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

## Identity and Authentication

### Getting Current Identity

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

### Reconnecting with Token

```csharp
string savedToken = PlayerPrefs.GetString("SpacetimeToken", null);

DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithModuleName("my-database")
    .WithToken(savedToken)  // Reconnect as same identity
    .OnConnect(OnConnected)
    .Build();
```

### Anonymous Connection

Pass `null` to `WithToken` or omit it entirely for a new anonymous identity.

## BSATN Serialization

SpacetimeDB uses BSATN (Binary SpacetimeDB Algebraic Type Notation) for serialization. The SDK handles this automatically for generated types.

### Supported Types

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

### Custom Types

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

### TaggedEnum (Sum Types)

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

### Result Type

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

## Unity Integration

### Project Setup

1. Add the SpacetimeDB package via Package Manager
2. Generate bindings and add to your Unity project
3. Create a manager MonoBehaviour

### SpacetimeManager Pattern

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

### Unity-Specific Considerations

1. **Main Thread Only**: All SpacetimeDB callbacks run on the main thread (during `FrameTick()`)

2. **Scene Loading**: Use `DontDestroyOnLoad` for the connection manager

3. **Reconnection**: Handle disconnects gracefully for mobile/poor connectivity

4. **PlayerPrefs**: Use for token persistence (or use a more secure method for production)

### Spawning GameObjects from Table Data

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

## Thread Safety

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

## Error Handling

### Connection Errors

```csharp
.OnConnectError((err) => {
    // Network errors, invalid module name, etc.
    Debug.LogError($"Connect error: {err}");
})
```

### Subscription Errors

```csharp
.OnError((ctx, err) => {
    // Invalid SQL, schema changes, etc.
    Debug.LogError($"Subscription error: {err}");
})
```

### Reducer Errors

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

## Complete Example

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

## Common Patterns

### Optimistic Updates

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

### Local Player Detection

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

### Waiting for Specific Data

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

## References

- [C# SDK Reference](https://spacetimedb.com/docs/sdks/c-sharp)
- [Unity Tutorial](https://spacetimedb.com/docs/unity/part-1)
- [SpacetimeDB SQL Reference](https://spacetimedb.com/docs/sql)
- [GitHub: Unity Demo (Blackholio)](https://github.com/clockworklabs/SpacetimeDB/tree/master/demo/Blackholio)
