# SpacetimeDB C# SDK

## Overview
This repository contains the C#, [Unity](https://unity.com/) and [Godot](https://godotengine.org/) client SDKs for SpacetimeDB. These SDKs contain all the tools you need to build native clients for SpacetimeDB modules using C#.

## Documentation
### Unity
The Unity SDK uses the same code as the C# SDK. You can find the documentation for the C# SDK in the [C# SDK Reference](https://spacetimedb.com/docs/clients/c-sharp). For a guided tutorial, see the [C# SDK Quickstart](https://spacetimedb.com/docs/quickstarts/c-sharp).

There is also a comprehensive Unity tutorial/demo available:
- [Unity Tutorial](https://spacetimedb.com/docs/tutorials/unity) Doc
- [Unity Demo](https://github.com/clockworklabs/SpacetimeDB/tree/master/demo/Blackholio/client-unity) Repo

### Godot
The Godot SDK uses the same code as the C# SDK. You can find the documentation for the C# SDK in the [C# SDK Reference](https://spacetimedb.com/docs/clients/c-sharp). For a guided tutorial, see the [C# SDK Quickstart](https://spacetimedb.com/docs/quickstarts/c-sharp).

There is also a comprehensive Godot tutorial/demo available:
- [Godot Tutorial](https://spacetimedb.com/docs/tutorials/godot) Doc
- [Godot Demo](https://github.com/clockworklabs/SpacetimeDB/tree/master/demo/Blackholio/client-godot) Repo

## Internal developer documentation
See [`DEVELOP.md`](./DEVELOP.md).

## Automatic reconnect

C#, Unity, and Godot clients can opt in with `.WithAutomaticReconnect()` on the connection builder, optionally passing `AutomaticReconnectOptions` with `MinDelay` and `MaxDelay` to tune the backoff. The SDK recovers lost connections, retains cached rows and subscription handles, and replays subscriptions in one batch. Keep calling `FrameTick()` during outages; Unity's `SpacetimeDBNetworkManager` does this automatically. Register subscriptions and row callbacks once, because `OnConnect` and subscription `OnApplied` run again after recovery.

For expiring credentials, supply the initial token with `.WithToken(initialToken)` and add `.WithTokenProvider(() => RefreshTokenAsync())`. The provider obtains a token for the same identity before a reconnect when needed. Initial connection failures do not retry, and `Disconnect()` stops recovery. Handle `Status.UnknownResult` for pending reducer calls whose outcome was lost; the SDK does not repeat them.

See the [C# SDK Reference](https://spacetimedb.com/docs/clients/c-sharp#method-withautomaticreconnect) for retry callbacks, token refresh, and subscription behavior, or the [reconnect test application](examples~/reconnect/README.md) for runnable scenarios.
