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

Generated bindings convert snake_case names to PascalCase, including row fields: a server column `trip_id` is `TripId` on client rows.

## Connection

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-database")
    .WithToken(savedToken)
    .WithCompression(Compression.Brotli) // optional; this is the default
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

Compression options are `Compression.Brotli`, `Compression.Gzip`, and `Compression.None`. The SDK uses Brotli when `WithCompression` is omitted.

## Automatic Reconnect and Token Refresh

Automatic reconnect is opt-in: add `.WithAutomaticReconnect()` to the builder. Initial connection failures do not retry. After an established connection is lost, the SDK retries with exponential backoff and jitter from `MinDelay` (default 1 s) up to `MaxDelay` (default 30 s); tune them with `.WithAutomaticReconnect(new AutomaticReconnectOptions { MinDelay = ..., MaxDelay = ... })`. Values below the 500 ms and 1 s floors are raised with a warning, so that retrying clients cannot overwhelm the database. `Disconnect()` permanently stops recovery.

Keep calling `FrameTick()` while `IsActive` is false. `IsReconnecting` is true while recovering before the next handshake. Use `.OnDisconnect((conn, error, next) => ...)` and `.OnConnectError((error, next) => ...)` to inspect `NextReconnect?`: a non-null value provides `Attempt` and `Delay`; null means no retry is scheduled. Existing callback overloads still work.

`OnConnect` runs on every successful reconnect, so create the subscription in the example above only on the first connection, or move it after `Build()`. Register row callbacks once as well. The same connection, identity, table handles, and subscriptions survive; each attempt gets a fresh `ConnectionId`. Cached rows stay readable but stale during outages. The SDK replays subscriptions in one batch and emits only net row changes. Subscription `OnApplied` runs again after replay; use it to mark data ready, not for repeated one-time setup.

For expiring credentials, combine `.WithToken(initialToken)` with `.WithTokenProvider(() => RefreshTokenAsync())`, where your provider returns `Task<string>` for the same identity. The provider is not called for the initial connection. Before retries, it is called when remaining validity is at most 30 seconds or 5% of the token lifetime, whichever is greater, when expiry cannot be read, or after a reused token is rejected. Provider failures retry; rejection of a freshly provided token is terminal. No periodic refresh runs while connected. Disconnecting ignores a pending provider result but does not cancel the provider's own work.

Calls made while disconnected fail immediately. Pending reducer calls receive `Status.UnknownResult`; pending procedures and one-off queries fail with `UnknownResultException`. These calls may already have executed and are never replayed. Regenerate bindings so unhandled unknown reducer outcomes reach `OnUnhandledReducerError`.

## Event Loop (Critical)

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

Reducer calls return `void`; observe failures via the reducer callbacks below (`Status.Failed`).

## Reducer Callbacks

```csharp
conn.Reducers.OnSendMessage += (ReducerEventContext ctx, string text) =>
{
    if (ctx.Event.Status is Status.Committed)
        Console.WriteLine($"Message sent: {text}");
    else if (ctx.Event.Status is Status.Failed(var reason))
        Console.Error.WriteLine($"Send failed: {reason}");
    else if (ctx.Event.Status is Status.UnknownResult)
        Console.Error.WriteLine("Connection lost before the result arrived; the message may have been sent.");
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
