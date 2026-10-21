---
name: concepts
description: Understand SpacetimeDB architecture and core concepts. Use when learning SpacetimeDB or making architectural decisions.
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

SpacetimeDB is a relational database that is also a server. It lets you upload application logic directly into the database as modules, eliminating the traditional web/game server layer entirely. Rust, C#, and C++ modules compile to WebAssembly, while TypeScript modules run on V8.

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

TypeScript and C# support same-language submodule composition. A namespace has a language accessor and a canonical database name; SQL, CLI calls, and HTTP calls use the canonical name. Changing the canonical namespace re-creates its tables, so pin it when renaming an accessor. TypeScript supports nested submodules; C# composition requires 2.13+, .NET 10/C# 14 and root-controlled assembly placement without nested mounts. See the respective server skills for registration and helper-context rules.

Database environment variables are declared, validated string configuration, including secrets. Publishing preserves unspecified values; environment-only publishing changes configuration without replacing the module. Required values are validated before initialization/migration. Named submodules cannot declare their own environment or read the root environment through host-dispatched entry points. C# dependencies registered in `public` instead contribute to the root schema. Private storage does not prevent module code from exposing a secret through results or logs.

## Views with Arguments and Module Tests

Views can accept arguments in 2.13+, queried as `SELECT * FROM view_name(42)`. Currently argument-bearing views do not support outer filters, projections, joins, floating-point arguments, or primary keys. Subscribe-all skips them; subscribe explicitly. See server skills for definitions and client skills for subscription syntax.

Rust and TypeScript provide in-memory module unit-test harnesses in 2.12+: run native `cargo test` or Vitest with the module-test plugin. These use the real datastore but do not simulate clients, subscriptions, RLS enforcement, lifecycle events, or timer execution. Test those host behaviors against a running database.

## Identity

- **Identity**: A long-lived, globally unique identifier for a user.
- **ConnectionId**: Identifies a specific client connection.
- Always use `ctx.sender` / `ctx.Sender` / `ctx.sender()` for authorization.

SpacetimeDB works with many OIDC providers, including SpacetimeAuth (built-in), Auth0, Clerk, Keycloak, Google, and GitHub.
