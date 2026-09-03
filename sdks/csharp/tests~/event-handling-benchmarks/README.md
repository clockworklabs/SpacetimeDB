# C# event handling benchmarks

This benchmark client compares native C# event dispatch with the SDK custom event listener backend. It measures elapsed time and process-wide allocated bytes for table event subscription, update dispatch, unsubscription, and resubscription against a real SpacetimeDB module.

It reuses the C# regression-test module and generated bindings. Publish that module first, then run this client against it.

```sh
spacetime start
spacetime publish --module-path sdks/csharp/examples~/regression-tests/server event-handling-bench
dotnet run -c Release --project sdks/csharp/tests~/event-handling-benchmarks/client/client.csproj
```

Environment variables:

- `SPACETIMEDB_SERVER_URL`: server URL, default `http://localhost:3000`.
- `SPACETIMEDB_DATABASE`: database name, default `event-handling-bench`.
- `SPACETIMEDB_EVENT_BACKEND`: `all`, `native`, `custom`, or `sappy`. `all` runs `native` and `custom`.
- `SPACETIMEDB_BENCHMARK_WARMUPS`: warmup runs per scenario/backend, default `1`.
- `SPACETIMEDB_BENCHMARK_ITERATIONS`: measured runs per scenario/backend, default `3`.

Backends:

- `native`: native C# multicast delegate dispatch.
- `custom`: SDK custom indexed listener dispatch.
- `sappy`: Sappy-backed custom listener dispatch. This path is only run when explicitly requested and requires compiling with `SAPPY=1` in a project that references the Sappy package, such as a Unity project with the SDK and Sappy integration assemblies present. The Sappy benchmark registers generated Sappy targets through `OnInsertListeners` so it exercises the intended Sappy path.

Scenarios:

- Few subscriptions, many updates.
- Many subscriptions, some updates.
- Many subscriptions, many updates.
- Many subscriptions, some updates, many unsubscriptions.
- Many subscriptions, some updates, many unsubscriptions, many resubscriptions, some updates.
- Many subscriptions, many updates, many unsubscriptions, many resubscriptions, many updates.

The update timings and allocation counts include reducer calls, server round trips, client frame ticks, row decoding, and listener dispatch. Allocation counts use `GC.GetTotalAllocatedBytes(precise: true)`, so they include allocations from background client work as well as main-thread dispatch. Compare native and custom runs on the same machine/server rather than treating the values as isolated in-process dispatch costs.
