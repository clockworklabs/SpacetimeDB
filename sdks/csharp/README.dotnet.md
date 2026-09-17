# SpacetimeDB SDK for C#

## Overview

This repository contains the [C#](https://learn.microsoft.com/en-us/dotnet/csharp/) SDK for SpacetimeDB. The SDK allows to interact with the database server and is prepared to work with code generated from a SpacetimeDB backend code.

## Documentation

The C# SDK has a [Quick Start](https://spacetimedb.com/docs/sdks/c-sharp/quickstart) guide and a [Reference](https://spacetimedb.com/docs/sdks/c-sharp).

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

`WithTokenProvider` is optional. The SDK retains the issued token, refreshes it near expiry when a provider is set, and checks that reconnects preserve the identity. Retries use exponential backoff with jitter and a 30-second cap. Initial connection failures do not retry; `Disconnect()` cancels pending attempts and token refreshes.

`IsActive` is false during recovery and `IsReconnecting` is true. Cached rows remain readable. Existing subscription handles and row callbacks survive; subscriptions replay in one batch, `OnApplied` fires again, and row callbacks report only net changes. Register subscriptions once, since `OnConnect` also fires on each reconnect. Create subscriptions in the first `OnConnect`, or immediately after `Build()` when automatic reconnect is enabled.

Reducer, procedure, and query calls made while disconnected fail immediately. In-flight queries and procedures fail with `UnknownResultException`; reducer callbacks receive `Status.UnknownResult`, since the server may have executed the call. Regenerate bindings to route this status to `OnUnhandledReducerError` when a reducer has no registered handler.

Existing callback overloads remain available. The overloads with `NextReconnect?` report the upcoming attempt and delay, or `null` when no retry is scheduled. `SpacetimeDBNetworkManager` keeps ticking reconnecting Unity connections.

See the [reconnect test application](examples~/reconnect/README.md) for executable outage, batch subscription, and JWT refresh scenarios.
