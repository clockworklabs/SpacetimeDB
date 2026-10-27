# Architecture

This is a map of what runs inside SpacetimeDB Standalone (`spacetimedb-standalone`) and how a request moves through it. It is meant for people working on SpacetimeDB itself. If you want to know how to build an app on SpacetimeDB, start with the [README](README.md) and the [docs](https://spacetimedb.com/docs).

<p align="center">
    <img src="./images/server-architecture.svg" alt="SpacetimeDB server architecture" width="100%">
</p>

## Where things run

At startup, SpacetimeDB splits the machine's cores into pools ([`startup.rs`](crates/core/src/startup.rs)). The yellow areas in the diagram run on the shared Tokio worker cores: the HTTP, WebSocket and Postgres servers, ControlDB, the HostController, and each database's background workers.

The purple area runs on a thread of the database's own. Each database gets one thread, pinned to a core from the database pool ([`jobs.rs`](crates/core/src/util/jobs.rs)). Several databases can share a core, and they are rebalanced as databases come and go. Reducers for a database therefore run one at a time, on one core.

## A reducer call, step by step

The numbers match the green arrows in the diagram.

1. A client calls a reducer, either with a call message on its WebSocket or with `POST /v1/database/:name/call/:reducer` ([`routes/database.rs`](crates/client-api/src/routes/database.rs)). The API resolves the database name through ControlDB and finds the host for that database.
2. The call goes onto the database's [`ModuleHost`](crates/core/src/host/module_host.rs) queue. Everything that touches the database goes through this queue: reducers, new subscriptions, SQL, and scheduled calls.
3. The ModuleHost runs the reducer in the module's Wasm instance ([`wasmtime`](crates/core/src/host/wasmtime)) or V8 isolate ([`v8`](crates/core/src/host/v8)), inside a mutable transaction on the in-memory [datastore](crates/datastore).
4. Before committing, the host re-runs any views whose inputs the transaction touched and writes their results into the views' backing tables, all in the same transaction ([`module_host_actor.rs`](crates/core/src/host/wasm_common/module_host_actor.rs)). If a view fails, the whole transaction rolls back.
5. The transaction commits and produces a `TxData` describing its inserts and deletes ([`commit_and_broadcast_event`](crates/core/src/subscription/module_subscription_actor.rs)). The `TxData` goes to the durability worker, which appends it to the [commitlog](crates/commitlog), and to subscription evaluation, which computes each subscribed query's changes from the `TxData` rather than re-running the query.
6. The send worker batches each client's updates and hands them to that client's WebSocket ([`module_subscription_manager.rs`](crates/core/src/subscription/module_subscription_manager.rs)).

Procedures are the exception to the queue: they run on module instances borrowed from a separate pool, so a procedure waiting on the network does not hold up reducers.

## Other paths

- **Subscribing.** A subscribe message goes through the same queue, because the module may need to materialize views before the query can run. The subscription manager then sends the initial rows and keeps the query registered for step 5.
- **SQL.** `POST /v1/database/:name/sql` and the [Postgres wire server](crates/pg/src/pg_server.rs) share one handler, `sql_direct`, which runs the statement on the database's thread through the ModuleHost ([`sql/execute.rs`](crates/core/src/sql/execute.rs)). Queries go through the query engine crates, [`sql-parser`](crates/sql-parser), [`expr`](crates/expr), [`physical-plan`](crates/physical-plan) and [`execution`](crates/execution). DML statements commit and broadcast like a reducer does.
- **Publishing.** `PUT /v1/database/:name` (or `POST /v1/database` for an unnamed database) records the database in ControlDB and asks the [`HostController`](crates/core/src/host/host_controller.rs) to start the module or hot-swap it.
- **Schedules.** Inserting a row into a schedule table registers a timer with the [scheduler](crates/core/src/host/scheduler.rs). When the timer fires, the scheduler puts the scheduled reducer or procedure on the queue. On startup, the scheduler reloads every schedule table listed in `st_scheduled`.
- **Inter-database calls.** Rows inserted into an outbox table wake the [IDC actor](crates/core/src/host/idc_actor.rs), which delivers them to the target database's `/call-from-database/:reducer` route and then runs the module's result callback.
- **Recovery.** When a database opens, [`RelationalDB`](crates/engine/src/relational_db.rs) restores the latest [snapshot](crates/snapshot) and replays the commitlog from that point. The [snapshot worker](crates/engine/src/snapshot.rs) takes a new snapshot every 1,000,000 transactions.

## ControlDB

ControlDB holds the server's view of the world: which databases exist, who owns them, their names, their replicas, and energy budgets. It is a [sled](https://github.com/spacejam/sled) database ([`control_db.rs`](crates/standalone/src/control_db.rs)).

## Updating the diagram

The diagram is a plain SVG at [`images/server-architecture.svg`](images/server-architecture.svg), so you can edit it directly. Its colors and fonts come from the docs site theme in [`docs/src/css/custom.css`](docs/src/css/custom.css).
