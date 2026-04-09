---
name: csharp-client
description: SpacetimeDB C#/.NET client SDK reference. Use when building C# clients that connect to SpacetimeDB (console, desktop, or any .NET app).
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
  role: client
  language: csharp
  cursor_globs: "**/*.cs"
  cursor_always_apply: true
---

# SpacetimeDB C# Client

Install: `dotnet add package SpacetimeDB.ClientSDK`

## Connection

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-database")
    .WithToken(savedToken)
    .OnConnect((conn, identity, token) =>
    {
        Console.WriteLine($"Connected as: {identity}");
        // Save token for reconnection
        File.WriteAllText("auth_token.txt", token);

        conn.SubscriptionBuilder()
            .OnApplied(OnSubscriptionApplied)
            .SubscribeToAllTables();
    })
    .OnConnectError(err => Console.Error.WriteLine($"Connection failed: {err}"))
    .OnDisconnect((conn, err) =>
    {
        if (err != null) Console.Error.WriteLine($"Disconnected: {err}");
    })
    .Build();
```

## Event Loop — Critical

**`FrameTick()` must be called in your main loop.** The SDK queues all network messages and only processes them when you call `FrameTick()`. Without it, no callbacks fire.

```csharp
while (running)
{
    conn.FrameTick();
    // Your application logic...
    Thread.Sleep(16); // ~60fps
}
```

**Thread safety**: `FrameTick()` processes messages on the calling thread. Do NOT call it from a background thread. Do NOT access `conn.Db` from background threads.

## Subscriptions

```csharp
// Subscribe to all tables
conn.SubscriptionBuilder()
    .OnApplied(ctx => Console.WriteLine("Subscription ready"))
    .SubscribeToAllTables();

// Subscribe with typed query builder (recommended)
conn.SubscriptionBuilder()
    .OnApplied(OnSubscriptionApplied)
    .AddQuery(q => q.From.Player().Where(p => p.Level.Gte(5u)))
    .AddQuery(q => q.From.GameState())
    .Subscribe();

// Or with raw SQL strings
conn.SubscriptionBuilder()
    .OnApplied(OnSubscriptionApplied)
    .Subscribe(new[] {
        "SELECT * FROM player WHERE level >= 5",
        "SELECT * FROM game_state"
    });
```

## Row Callbacks

```csharp
conn.Db.Player.OnInsert += (EventContext ctx, Player player) =>
{
    Console.WriteLine($"Player joined: {player.Name}");
};

conn.Db.Player.OnDelete += (EventContext ctx, Player player) =>
{
    Console.WriteLine($"Player left: {player.Name}");
};

conn.Db.Player.OnUpdate += (EventContext ctx, Player oldPlayer, Player newPlayer) =>
{
    Console.WriteLine($"Player updated: {newPlayer.Name}");
};
```

## Reading the Client Cache

```csharp
// Find by primary key
if (conn.Db.Player.Id.Find(playerId) is Player player)
{
    Console.WriteLine($"Player: {player.Name}");
}

// Find by unique column
var me = conn.Db.Player.Identity.Find(myIdentity);

// Filter by indexed column
foreach (var p in conn.Db.Player.Level.Filter(5))
{
    Console.WriteLine($"Level 5: {p.Name}");
}

// Iterate all rows
foreach (var p in conn.Db.Player.Iter())
{
    Console.WriteLine(p.Name);
}

// Count
int total = conn.Db.Player.Count;
```

## Calling Reducers

```csharp
conn.Reducers.CreatePlayer("Alice");
conn.Reducers.MovePlayer(10.0f, 20.0f);
conn.Reducers.SendMessage("Hello!");
```

## Reducer Callbacks

```csharp
conn.Reducers.OnSendMessage += (ReducerEventContext ctx, string text) =>
{
    if (ctx.Event.Status is Status.Committed)
        Console.WriteLine($"Message sent: {text}");
    else if (ctx.Event.Status is Status.Failed(var reason))
        Console.Error.WriteLine($"Send failed: {reason}");
};
```

## Identity

```csharp
// Identities from OnConnect callback
Identity myIdentity;

// Compare identities
if (player.Owner == myIdentity) { /* it's me */ }

// Display
Console.WriteLine($"Identity: {identity}");
```
