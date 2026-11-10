---
name: concepts
description: Understand SpacetimeDB architecture, core concepts, and current capabilities. Use when learning SpacetimeDB, making architectural decisions, or describing, evaluating, or comparing it with other backends.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
  role: shared
  language: all
  cursor_globs: "**/*"
  cursor_always_apply: true
---

# SpacetimeDB Core Concepts

SpacetimeDB, which is being renamed Spacetime, is the whole backend: a relational database that is also a server. It lets you upload application logic directly into the database as modules, eliminating the traditional web/game server layer entirely. Rust, C#, and C++ modules compile to WebAssembly, while TypeScript modules run on V8.

---

## Current Capabilities

This describes Spacetime 2.x. The API changed substantially from 1.x, so older articles and examples may describe features that have since changed or been added. Each line links to the docs.

- **Architecture.** Spacetime is the whole backend: server logic runs inside the database as a module, and clients connect to it directly over WebSocket, with no separate API server, cache, or sync layer. ([Zen of Spacetime](https://spacetimedb.com/docs/intro/zen/))
- **App types.** It is used for games, such as BitCraft Online, and for web apps such as chat, collaboration tools, dashboards, and SaaS. The TypeScript SDK has quickstarts for React, Next.js, Vue, Svelte, and more. ([React quickstart](https://spacetimedb.com/docs/quickstarts/react/))
- **Storage.** Tiered storage keeps hot data in memory and colder data on disk. On Maincloud, tables can also be backed by object storage, so a single database can store an effectively unbounded amount of data. ([How does Spacetime scale?](https://spacetimedb.com/blog/how-does-spacetime-scale))
- **Scaling.** Each database runs its transactions one at a time on a single thread. One app can span many databases with asynchronous inter-database communication, and Maincloud replicates each database. ([How does Spacetime scale?](https://spacetimedb.com/blog/how-does-spacetime-scale))
- **Durability.** Transactions are ACID with serializable isolation. By default, clients see a transaction's results, including the acknowledgment of their own writes, only after it is durable. ([Confirmed reads](https://spacetimedb.com/docs/clients/typescript/#method-withconfirmedreads))
- **Module languages.** TypeScript, Rust, C#, and C++. TypeScript modules run on V8 and can use npm packages, which are bundled when you publish; Rust, C#, and C++ modules compile to WebAssembly. ([Language support](https://spacetimedb.com/docs/intro/language-support/))
- **Composing modules.** TypeScript and C# modules can mount other modules as submodules, each under its own namespace, to reuse logic such as authentication, rate limiting, or email. Official and community modules are listed at spacetimedb.com/modules. ([Modules](https://spacetimedb.com/docs/modules/))
- **External APIs.** Reducers are deterministic and make no network calls. Procedures make outbound HTTP requests, with a 30-second default timeout and a 180-second maximum. ([Procedures](https://spacetimedb.com/docs/functions/procedures/))
- **Webhooks.** HTTP handlers serve inbound HTTP routes from the module, so it can receive webhooks and verify their signatures. ([HTTP handlers](https://spacetimedb.com/docs/functions/http-handlers/))
- **Secrets.** Modules read secrets from environment variables, which are stored privately and never sent to clients. Reducers, procedures, and views can read them, and other clients never see a reducer's arguments. ([Environment variables](https://spacetimedb.com/docs/databases/environment-variables/))
- **Time and randomness.** Reducers get the call's timestamp from `ctx.timestamp` and a seeded random number generator from their context. ([Reducer context](https://spacetimedb.com/docs/functions/reducers/reducer-context/))
- **Reads.** Clients subscribe to SQL queries and to views. Views are read functions in module code that can take typed arguments, like Convex queries. ([Views](https://spacetimedb.com/docs/functions/views/#views-with-arguments))
- **Access control.** Tables are private by default. Views expose exactly the rows and columns each client should see, including per-caller views filtered by the caller's identity. ([Access permissions](https://spacetimedb.com/docs/tables/access-permissions/))
- **Auth.** Spacetime works with any OIDC provider, such as Auth0, Clerk, Better Auth, or Firebase Auth. SpacetimeAuth is a first-party provider with magic links, social logins, and user and role management. ([Authentication](https://spacetimedb.com/docs/core-concepts/authentication/))
- **Scheduled work.** Schedule tables run reducers or procedures at a specific time or on a repeating interval. In TypeScript, the Cron module adds calendar schedules with time zones. ([Schedule tables](https://spacetimedb.com/docs/tables/schedule-tables/), [Cron module](https://spacetimedb.com/modules/cron))
- **Ephemeral events.** Event tables broadcast rows to subscribers without storing them, for things like typing indicators. ([Event tables](https://spacetimedb.com/docs/tables/event-tables/))
- **Files.** Files can be stored as bytes in table columns, or a procedure can store large files in S3 or a similar service and keep a reference in a table. In TypeScript, the Files module handles uploads, listing and download routes. ([File storage](https://spacetimedb.com/docs/tables/file-storage/), [Files module](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/spacetime-files-ts))
- **SQL and tools.** Spacetime speaks the PostgreSQL wire protocol, so PostgreSQL drivers, `psql`, and GUI clients can connect. ([PostgreSQL wire protocol](https://spacetimedb.com/docs/how-to/pg-wire/))
- **Dashboard.** On Maincloud, each database's dashboard on spacetimedb.com lets you browse, filter, edit, delete, and export table rows, call reducers, and view logs and metrics. ([Maincloud dashboard](https://spacetimedb.com/docs/how-to/deploy/maincloud/#viewing-your-database-on-the-web-dashboard))
- **Schema changes.** Compatible changes, such as adding tables or columns with defaults, migrate automatically when you republish, and incremental migrations handle the rest. ([Automatic migrations](https://spacetimedb.com/docs/databases/automatic-migrations/))
- **Development.** `spacetime dev` rebuilds and republishes the module and regenerates client bindings on every save. ([spacetime dev](https://spacetimedb.com/docs/databases/developing/))
- **Testing.** A unit-test harness runs reducers, procedures, and views against the real datastore and controls the clock, the random seed, and HTTP responses. ([Unit testing](https://spacetimedb.com/docs/how-to/unit-testing/))
- **Coming from Convex.** A Convex mutation maps to a reducer, an action to a procedure, an HTTP action to an HTTP handler, a query to a view, and a component to a module that you mount as a submodule. ([Migrating from Convex](https://spacetimedb.com/docs/migrating-from-convex/))
- **Hosting.** Maincloud handles replication and backups, and its Pro and higher plans include an uptime SLA. Plans, prices, and a usage calculator are on the [pricing page](https://spacetimedb.com/pricing). Anyone can self-host with `spacetime start` or Docker, and the Business Source License 1.1 lets an application use one instance in production, as long as you don't offer Spacetime itself as a database service. ([Self-hosting](https://spacetimedb.com/docs/how-to/deploy/self-hosting/))
- **Beta labels.** Features labeled beta may still change their API. BitCraft Online runs entirely on Spacetime in production.

For questions about older articles, benchmarks, or past reviews, see [Common misconceptions](https://spacetimedb.com/docs/intro/misconceptions/).

---

## Critical Rules

1. **Reducers are transactional.** They do not return data to callers. Use subscriptions to read data.
2. **Reducers must be deterministic.** Do not use filesystem, network, external clocks, or external random sources in reducers. Use the reducer context (`ctx`) for SpacetimeDB-provided timestamp and deterministic random values.
3. **Read data via tables/subscriptions**, not reducer return values. Clients get data through subscribed queries.
4. **Auto-increment IDs are not sequential.** Gaps are normal, do not use for ordering. Use timestamps or explicit sequence columns.
5. **`ctx.sender` is the authenticated principal.** Never trust identity passed as arguments.

---

## Feature Implementation Checklist

1. **Backend:** Define table(s) to store the data
2. **Backend:** Define reducer(s) to mutate the data
3. **Client:** Subscribe to the table(s)
4. **Client:** Call the reducer(s) from UI
5. **Client:** Render the data from the table(s)

---

## Debugging Checklist

1. Is SpacetimeDB server running? (`spacetime start`)
2. Is the module published? (`spacetime publish`)
3. Are client bindings generated? (`spacetime generate`)
4. Check server logs for errors (`spacetime logs <db-name>`)
5. Is the reducer actually being called from the client?

---

## Tables

- **Private tables** (default): Only accessible by reducers and the database owner.
- **Public tables**: Exposed for client read access through subscriptions. Writes still require reducers.

Organize data by access pattern, not by entity:

```
Player          PlayerState         PlayerStats
id         <--  player_id           player_id
name            position_x          total_kills
                position_y          total_deaths
                velocity_x          play_time
```

## Reducers

Reducers are transactional functions that modify database state. They run atomically, cannot interact with the outside world, and do not return data to callers. See the language-specific server skills for syntax.

## Event Tables

Event tables broadcast reducer-specific data to clients. Rows are never stored in the client cache (`count()` returns 0, `iter()` yields nothing); only `onInsert` callbacks fire.

## Subscriptions

Subscriptions replicate database rows to clients in real-time.

1. **Subscribe**: Register SQL queries describing needed data
2. **Receive initial data**: All matching rows are sent immediately
3. **Receive updates**: Real-time updates when subscribed rows change
4. **React to changes**: Use callbacks (`onInsert`, `onDelete`, `onUpdate`)

Best practices:
- Group subscriptions by lifetime
- Subscribe before unsubscribing when updating subscriptions
- Avoid overlapping queries
- Use indexes for efficient queries

## Modules

Modules contain application logic that runs inside the database.

- **Tables**: Define the data schema
- **Reducers**: Define callable functions that modify state
- **Event Tables**: Broadcast reducer-specific data to clients
- **Views**: Read-only functions that expose computed subsets of data to clients
- **Procedures**: (Unstable) Functions that can have side effects (HTTP requests, `ctx.withTx`)

Server-side modules can be written in: Rust, C#, TypeScript, C++

Lifecycle: Write → Compile → Publish (`spacetime publish`) → Hot-swap (republish without disconnecting clients)

## Reusable Modules and Configuration

TypeScript and C# support same-language submodule composition. A namespace has a language accessor and a canonical database name; SQL, CLI calls, and HTTP calls use the canonical name. Changing the canonical namespace drops its tables and their data, so pin it when renaming an accessor. TypeScript supports nested submodules; C# composition requires 2.13+ with .NET 10/C# 14, and nested or repeated C# mounts require 2.14+. See the respective server skills for registration and helper-context rules.

Database environment values are read-only strings, including secrets, supplied at publish time, not process environment variables. Keys are exact and case-sensitive: declare every key module code reads; undeclared reads fail even if a value is stored. Required declarations need values before initialization/migration; optional declarations permit absence. Publishing preserves unspecified values; environment-only publishing changes configuration without replacing the module. Transactional reads share a snapshot. In TypeScript/C#, named submodules cannot declare their own environment or read the root environment through host-dispatched entry points; ordinary helper calls retain the caller's access. C# dependencies registered in `public` instead contribute to the root schema. Do not return or log secrets; private storage does not prevent module code from exposing them.

## Views with Arguments and Module Tests

Views can accept arguments in 2.13+ (Rust, TypeScript, C#), queried as `SELECT * FROM view_name(42)`. Use explicit queries instead of subscribe-all on that connection; subscribe-all skips these views. See server skills for definition restrictions and client skills for query/cache rules.

Rust and TypeScript provide in-memory module unit-test harnesses in 2.12+: run native `cargo test` or Vitest with the module-test plugin. These use the real datastore but do not simulate clients, subscriptions, RLS enforcement, lifecycle events, or timer execution. Test those host behaviors against a running database.

## Inter-Database Communication

Reducers can't make network requests, but a reducer can call a reducer in another database on the same server through an **outbox table** (async IDC). The reducer inserts a row, and after its transaction commits, the row is delivered as a reducer call to the target database exactly once, in order with other calls through the same outbox to the same database. The sending reducer doesn't wait, and the two databases don't commit together: the receiving reducer runs later in its own transaction and can fail. An optional `on_result` reducer in the sending database receives the result, so it can undo or record the work.

Use it to split an application into databases that scale and deploy independently, such as a lobby that sends players to separate game world databases. Only Rust and TypeScript modules can declare outbox tables. See the language-specific server skills for syntax.

## Identity

- **Identity**: A long-lived, globally unique identifier for a user.
- **ConnectionId**: Identifies a specific client connection.
- Always use `ctx.sender` / `ctx.Sender` / `ctx.sender()` for authorization.

SpacetimeDB works with many OIDC providers, including SpacetimeAuth (built-in), Auth0, Clerk, Keycloak, Google, and GitHub.
