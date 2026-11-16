# Nested Namespace Client Regression

This C# 9 client runs on .NET 8 and .NET 10 against the .NET 10
`modules/nested-namespace-test-cs` module. The regression scripts generate these
bindings from that module and publish a fresh database for each client runtime.

The same Leaf assembly appears six times, including three levels deep at
`conn.Db.@class.Branch.Leaf`. C# paths use accessors; SQL and wire names use each
level's canonical name (`outer_data.branch_data.nested_data`).

The test covers separate table caches, typed subscriptions, reducer calls/events
and error forwarding, procedure results/errors, one-off queries, subscribe-all
including views, same-leaf cross-namespace joins, and unsubscribe cleanup.
An insert callback checks the shared post-transaction cache.

Run through `sdks/csharp/tools~/run-regression-tests.sh 10` with a local server
and the local SDK packages configured as described in `sdks/csharp/DEVELOP.md`.
That runs both client runtimes.
