---
name: unity
description: Integrate SpacetimeDB with Unity game projects. Use when building Unity clients with MonoBehaviour lifecycle, FrameTick, and PlayerPrefs token persistence.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
  role: client
  language: csharp
  cursor_globs: "**/*.cs"
  cursor_always_apply: false
  tested_with: "SpacetimeDB 2.0, Unity 2022.3+"
---

# SpacetimeDB Unity Integration

This skill covers Unity-specific patterns for connecting to SpacetimeDB. For server-side module development, see the `csharp-server` skill.

---

## Installation

Add via Unity Package Manager using the git URL:

```
https://github.com/clockworklabs/com.clockworklabs.spacetimedbsdk.git
```

**Window > Package Manager > + > Add package from git URL**

---

## Generate Module Bindings

```bash
spacetime generate --lang csharp --out-dir Assets/SpacetimeDB/module_bindings --module-path PATH_TO_MODULE
```

Place generated files in your Assets folder so Unity compiles them.

---

## SpacetimeManager Singleton

The core pattern for Unity integration. This MonoBehaviour manages the connection lifecycle.

```csharp
using UnityEngine;
using SpacetimeDB;
using SpacetimeDB.Types;

public class SpacetimeManager : MonoBehaviour
{
    private const string TOKEN_KEY = "SpacetimeAuthToken";
    private const string SERVER_URI = "http://localhost:3000";
    private const string DATABASE_NAME = "my-game";

    public static SpacetimeManager Instance { get; private set; }
    public DbConnection Connection { get; private set; }
    public Identity LocalIdentity { get; private set; }

    void Awake()
    {
        if (Instance != null && Instance != this) { Destroy(gameObject); return; }
        Instance = this;
        DontDestroyOnLoad(gameObject);
    }

    void Start()
    {
        string savedToken = PlayerPrefs.GetString(TOKEN_KEY, null);

        Connection = DbConnection.Builder()
            .WithUri(SERVER_URI)
            .WithDatabaseName(DATABASE_NAME)
            .WithToken(savedToken)
            .OnConnect(OnConnected)
            .OnConnectError(err => Debug.LogError($"Connection failed: {err}"))
            .OnDisconnect((conn, err) => {
                if (err != null) Debug.LogError($"Disconnected: {err}");
            })
            .Build();
    }

    void Update()
    {
        Connection?.FrameTick();
    }

    void OnDestroy()
    {
        Connection?.Disconnect();
    }

    private void OnConnected(DbConnection conn, Identity identity, string authToken)
    {
        LocalIdentity = identity;
        PlayerPrefs.SetString(TOKEN_KEY, authToken);
        PlayerPrefs.Save();

        Debug.Log($"Connected as: {identity}");

        conn.SubscriptionBuilder()
            .OnApplied(OnSubscriptionApplied)
            .SubscribeToAllTables();
    }

    private void OnSubscriptionApplied(SubscriptionEventContext ctx)
    {
        Debug.Log("Subscription applied: game state loaded");
    }
}
```

---

## FrameTick (Critical)

**`FrameTick()` must be called every frame in `Update()`.** The SDK queues all network messages and only processes them when you call `FrameTick()`. Without it, no callbacks fire and the client appears frozen. See the `Update()` method in the SpacetimeManager above.

**Thread safety**: `FrameTick()` processes messages on the calling thread (the main thread in Unity). Do NOT call it from a background thread. Do NOT access `conn.Db` from background threads.

## Automatic Reconnect

Add `.WithAutomaticReconnect()` to the builder to recover an established connection after an outage. In the singleton above, create subscriptions only on the first `OnConnected`, or move them after `Build()`: `OnConnected` runs again after each successful reconnect. Keep calling `FrameTick()` during outages, or use `SpacetimeDBNetworkManager`, which also ticks reconnecting connections. Do not gate ticking on `IsActive`.

The SDK retains the identity, table handles, callbacks, and subscriptions, and replays subscriptions in one batch. Cached rows stay readable during an outage; row callbacks report net changes after recovery. `OnApplied` runs again, so separate one-time object setup from marking data ready. Each reconnect attempt has a fresh `ConnectionId`.

`IsReconnecting` reports recovery before the next successful handshake. The `OnDisconnect((conn, error, next) => ...)` and `OnConnectError((error, next) => ...)` overloads expose `NextReconnect?`, with the upcoming `Attempt` and `Delay`, or null for a terminal failure. Initial connection failures do not retry. Retries use exponential backoff and jitter from `MinDelay` (default 1 s) up to `MaxDelay` (default 30 s), tunable with `.WithAutomaticReconnect(new AutomaticReconnectOptions { MinDelay = ..., MaxDelay = ... })`; values below the 500 ms and 1 s floors are raised with a warning, so that retrying clients cannot overwhelm the database. `Disconnect()` stops them.

Calls made while disconnected fail immediately. Pending reducer calls may report `Status.UnknownResult`, meaning the server may have executed them; they are not replayed. Pending procedures and one-off queries fail with `UnknownResultException`. Regenerate bindings when upgrading to include unknown-outcome handling.

---

## Row Callbacks for Game State

Register callbacks to update Unity GameObjects when table data changes.

```csharp
void RegisterCallbacks()
{
    Connection.Db.Player.OnInsert += (EventContext ctx, Player player) => {
        SpawnPlayerObject(player);
    };

    Connection.Db.Player.OnDelete += (EventContext ctx, Player player) => {
        DestroyPlayerObject(player.Id);
    };

    Connection.Db.Player.OnUpdate += (EventContext ctx, Player oldPlayer, Player newPlayer) => {
        UpdatePlayerObject(newPlayer);
    };
}
```

Register these once after building the connection, before the first `FrameTick()`. Do not register them repeatedly in `OnConnected` or `OnSubscriptionApplied` when automatic reconnect is enabled.

---

## Calling Reducers from UI

```csharp
public class GameUI : MonoBehaviour
{
    public void OnMoveButtonClicked(Vector2 direction)
    {
        SpacetimeManager.Instance.Connection.Reducers.MovePlayer(direction.x, direction.y);
    }

    public void OnSendChat(string message)
    {
        SpacetimeManager.Instance.Connection.Reducers.SendMessage(message);
    }
}
```

### Reducer Callbacks

```csharp
SpacetimeManager.Instance.Connection.Reducers.OnSendMessage += (ReducerEventContext ctx, string text) => {
    if (ctx.Event.Status is Status.Committed)
        Debug.Log($"Message sent: {text}");
    else if (ctx.Event.Status is Status.Failed(var reason))
        Debug.LogError($"Send failed: {reason}");
    else if (ctx.Event.Status is Status.UnknownResult)
        Debug.LogWarning("Connection lost before the result arrived; the message may have been sent.");
};
```

---

## Reading the Client Cache

```csharp
// Find by primary key
if (Connection.Db.Player.Id.Find(playerId) is Player player)
{
    Debug.Log($"Player: {player.Name}");
}

// Iterate all
foreach (var p in Connection.Db.Player.Iter())
{
    Debug.Log(p.Name);
}

// Filter by index
foreach (var p in Connection.Db.Player.Level.Filter(5))
{
    Debug.Log($"Level 5: {p.Name}");
}

// Count
int total = Connection.Db.Player.Count;
```

---

## Unity-Specific Considerations

### Main Thread Only
All SpacetimeDB SDK calls (`FrameTick`, `conn.Db` access, reducer calls) must happen on the main thread. If you need to pass data to a background thread, copy it first in the callback.

### Scene Loading
Use `DontDestroyOnLoad(gameObject)` on the SpacetimeManager to prevent the connection from being destroyed during scene transitions. Without it, the connection drops every time you load a new scene.

### IL2CPP / AOT
The SpacetimeDB SDK uses code generation. If you encounter issues with IL2CPP builds:
- Ensure generated bindings are up to date
- Check that `link.xml` preserves SpacetimeDB types if you use assembly stripping

### Token Persistence
Token save/load via `PlayerPrefs` is demonstrated in the SpacetimeManager singleton above. Persisting the server-issued token and passing it back on reconnect keeps the same identity; without a saved token the server issues a new identity in the `OnConnect` callback. This token does not expire and a lost one can't be recovered, so self-issued identities are for development. For production, authenticate with an OIDC provider such as SpacetimeAuth, which handles token lifecycle.

With automatic reconnect enabled, the SDK retains the authentication token and passes it to `OnConnect`, including on WebGL. Without automatic reconnect, a WebGL connection using a saved token may return a short-lived WebSocket token instead; in that case, keep the original saved token rather than overwriting it in the callback above.

For expiring credentials, configure `.WithToken(initialToken)`, `.WithAutomaticReconnect()`, and `.WithTokenProvider(() => RefreshTokenAsync())`. The provider returns `Task<string>` for the same identity. It is used before reconnect attempts when expiry is unreadable, remaining validity is at most 30 seconds or 5% of the original lifetime, or a reused token is rejected. It is not called for the initial connection or periodically while connected. Provider failures retry; rejection of a freshly provided token stops recovery. `Disconnect()` ignores a pending provider result without canceling the provider's own asynchronous work.
