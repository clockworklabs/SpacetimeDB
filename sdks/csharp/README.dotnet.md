# SpacetimeDB SDK for C#

## Overview

This repository contains the [C#](https://learn.microsoft.com/en-us/dotnet/csharp/) SDK for SpacetimeDB. The SDK allows to interact with the database server and is prepared to work with code generated from a SpacetimeDB backend code.

## Documentation

The C# SDK has a [Quick Start](https://spacetimedb.com/docs/quickstarts/c-sharp) guide and a [Reference](https://spacetimedb.com/docs/clients/c-sharp).

## Automatic reconnect

Enable reconnect on the builder and keep calling `FrameTick` during outages:

```csharp
var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-database")
    .WithToken(savedToken)
    .WithAutomaticReconnect()
    .WithTokenProvider(() => RefreshAccessTokenAsync())
    .OnConnect((conn, identity, token) => SaveToken(token))
    .OnDisconnect((conn, error, next) =>
    {
        if (next is { } retry)
            Console.WriteLine($"Retry {retry.Attempt} in {retry.Delay}.");
    })
    .OnConnectError((error, next) => Console.WriteLine(error.Message))
    .Build();
```

Automatic reconnect is opt-in. Retries use exponential backoff with jitter and a 30-second cap, with no attempt limit. Initial connection failures do not retry. `Disconnect()` cancels scheduled attempts and ignores pending token-provider results; it does not cancel the provider's own asynchronous work. Terminal authentication or protocol errors and identity changes also stop retries.

`WithTokenProvider(Func<Task<string>>)` is optional and is used only during automatic reconnect. Supply the initial token through `WithToken`; the provider is not called for the first connection. Before each retry, the SDK calls the provider when the token has at most 30 seconds or 5% of its original lifetime remaining, whichever is greater. If expiry cannot be read, it calls the provider every attempt. It also forces refresh after a reused token is rejected. Provider failures retry; rejection of a freshly provided token, or rejection without a provider, is terminal. The provider must return a non-empty token for the same identity. No periodic refresh runs while connected.

`IsActive` is false while waiting for a retry, token provider, or reconnect handshake, and `IsReconnecting` is true. After the handshake, `IsActive` becomes true before subscriptions finish replaying; use `OnApplied` to know when data is ready. Cached rows remain readable during outages. Existing subscription handles and row callbacks survive; subscriptions replay in one batch, `OnApplied` fires again, and row callbacks report only net changes. Register subscriptions once, since `OnConnect` also fires on each reconnect. Create subscriptions in the first `OnConnect`, or immediately after `Build()` when automatic reconnect is enabled. The identity stays the same, while each attempt gets a fresh `ConnectionId`.

Reducer, procedure, and query calls made while disconnected fail immediately. In-flight queries and procedures fail with `UnknownResultException`; reducer callbacks receive `Status.UnknownResult`, since the server may have executed the call. Regenerate bindings to route this status to `OnUnhandledReducerError` when a reducer has no registered handler.

Existing callback overloads remain available. The overloads with `NextReconnect?` report the upcoming attempt and delay, or `null` when no retry is scheduled. `SpacetimeDBNetworkManager` keeps ticking reconnecting Unity connections.

See the [reconnect test application](examples~/reconnect/README.md) for executable outage, batch subscription, and JWT refresh scenarios.
