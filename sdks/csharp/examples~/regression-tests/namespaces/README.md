# Namespace Integration Test

Runs a generated C# 9 / .NET 8 client against the .NET 10 module in
`modules/namespace-test-cs`. It uses `Accessor` as the namespace name throughout.

The existing `sdks/csharp/tools~/run-regression-tests.sh 10` harness generates the
bindings, publishes a fresh `namespace-tests` database, and runs this client.
The namespace scenario is skipped in the .NET 8 **module** pass. CI already runs
both harness passes.

For a focused run, from the repository root, with a local server running and local
C# packages configured as described in `sdks/csharp/DEVELOP.md`:

```sh
cargo spacetime generate -y -l csharp -o sdks/csharp/examples~/regression-tests/namespaces/module_bindings --module-path modules/namespace-test-cs --build-options="--dotnet-version 10"
cargo spacetime publish --dotnet-version 10 -c -y --server local -p modules/namespace-test-cs namespace-tests
dotnet run --project sdks/csharp/examples~/regression-tests/namespaces/client.csproj
```

The publish command deletes existing data in the test database. Republish before
each client run. For a nondefault server, pass its URL to publish and set
`SPACETIMEDB_SERVER_URL` to that same URL for the client.

Coverage:

- Root and two mounted libraries with different row types named `User`, plus an
  dependency automatically registered in `public`, without a namespace declaration.
- Explicit table names and a C# keyword namespace accessor.
- Typed subscriptions, filtered queries, both semijoin directions, and overlapping
  subscriptions without duplicate rows or premature cache removal.
- Cross-library helper writes and callbacks observing the complete transaction.
- A procedure transaction writing root and both namespace tables, including a
  library-local transaction helper. Checks commit, rollback after an exception,
  atomic subscription callbacks, and persisted results through one-off queries.
- Same-named reducers/procedures in different namespaces, procedure success/error
  callbacks, and child reducer failures forwarded to the root connection.
- Root environment reads through typed accessors, a library helper and a procedure
  transaction; rejection of environment reads in host-dispatched namespace procedures.
- Insert/update/delete callbacks, unique/B-tree indexes, and reducer rollback.
- Event rows, private-table access rejection, procedural/anonymous/query views.
- Root-defined RLS on a public namespaced table: two distinct non-owner clients,
  typed/raw subscriptions, initial rows, live inserts/updates, ownership transfer,
  filtered one-off queries, and unsubscribe cleanup. No library-defined RLS rules.
- `RemoteQuery` result decoding and no subscription-cache population by one-off queries.
- Subscribe-all initial rows, no replay of past events, and unsubscribe cache cleanup.
- Same-named scheduled reducers/procedures in both namespaces, with explicit function
  and table names and custom scheduled-at columns. Covers one-shot and immediate
  execution, repeating reducer cancellation, argument payloads, generated keys,
  namespace isolation, and absence of scheduled functions from client call APIs.

Assertions throw in both Debug and Release. Every asynchronous phase has a timeout.
The generated bindings are committed and regenerated through the normal CLI path.
