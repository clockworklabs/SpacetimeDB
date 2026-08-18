# C# event handling benchmarks

This benchmark client measures table event subscription, update dispatch, unsubscription, and resubscription against a real SpacetimeDB module.

It reuses the C# regression-test module and generated bindings. Publish that module first, then run this client against it.

```sh
spacetime start
spacetime publish event-handling-bench sdks/csharp/examples~/regression-tests/server
dotnet run -c Release --project sdks/csharp/examples~/event-handling-benchmarks/client/client.csproj
```

Environment variables:

- `SPACETIMEDB_SERVER_URL`: server URL, default `http://localhost:3000`.
- `SPACETIMEDB_DATABASE`: database name, default `event-handling-bench`.
- `SPACETIMEDB_EVENT_BACKEND`: `all`, `native`, `custom`, or `sappy`.

Backends:

- `native`: native C# multicast delegate dispatch.
- `custom`: SDK custom indexed listener dispatch.
- `sappy`: Sappy-backed custom listener dispatch. This path requires compiling with `SAPPY=1` in a project that references the Sappy package, such as a Unity project with the SDK and Sappy integration assemblies present.

Scenarios:

- Few subscriptions, many updates.
- Many subscriptions, some updates.
- Many subscriptions, many updates.
- Many subscriptions, some updates, many unsubscriptions.
- Many subscriptions, some updates, many unsubscriptions, many resubscriptions, some updates.
