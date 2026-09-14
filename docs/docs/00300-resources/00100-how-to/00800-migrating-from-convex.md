---
title: Migrating from Convex
slug: /migrating-from-convex
---

# Migrating from Convex

This guide is for teams moving an application backend from Convex to a
SpacetimeDB module. Convex and SpacetimeDB both combine database state, server
logic, generated clients, and live updates, but the programming models are not
identical. The most important shift is:

- In Convex, clients call **queries** to read data and **mutations** to write
  data.
- In SpacetimeDB, clients **subscribe** to tables or views to read live data and
  call **reducers** to change state.

Reducers do not return application data. They commit database changes, and
clients observe the resulting rows through subscriptions, views, and event
tables. Use procedures only when you need side effects such as outbound HTTP.

## Terminology Map

| Convex term                               | SpacetimeDB term                                                          | Migration notes                                                                                                                                                                                       |
| ----------------------------------------- | ------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Project                                   | Project plus module source tree                                           | A SpacetimeDB project contains module code, generated bindings, and client code.                                                                                                                      |
| Deployment                                | Database                                                                  | A published SpacetimeDB module creates or updates a database on Maincloud or a self-hosted host.                                                                                                      |
| Backend                                   | Database plus module                                                      | Your module contains tables and callable functions that run inside the database.                                                                                                                      |
| `convex/` directory                       | `spacetimedb` directory                                                   | TypeScript templates usually put module code in `spacetimedb/src/index.ts`. Rust, C#, and C++ use their normal project layouts.                                                                       |
| `schema.ts`                               | Table definitions in module code                                          | SpacetimeDB schema is declared in the module language using [tables](/docs/tables).                                                                                                                   |
| `defineSchema`                            | `schema(...)` or language-specific module schema                          | In TypeScript, pass tables created with `table()` to `schema({ tableName })`.                                                                                                                         |
| `defineTable`                             | `table()`                                                                 | Tables store rows and define columns, constraints, indexes, visibility, and optional scheduling.                                                                                                      |
| Table                                     | Table                                                                     | Both systems organize persistent data into named tables.                                                                                                                                              |
| Document                                  | Row                                                                       | SpacetimeDB tables are relational rows, not JSON-like documents.                                                                                                                                      |
| Document field                            | Column                                                                    | Columns have static SpacetimeDB types.                                                                                                                                                                |
| Document ID / `Id<"table">`               | Primary key, unique key, or `Identity`                                    | Choose the key based on access pattern. Use `autoInc()` for generated numeric IDs, `Identity` for users, and unique constraints for alternate lookup keys.                                            |
| `_id`                                     | Primary key column                                                        | Name it explicitly, commonly `id`. SpacetimeDB does not require a universal `_id` column.                                                                                                             |
| `_creationTime`                           | Explicit timestamp column                                                 | Add a `Timestamp` column and set it from `ctx.timestamp` if you need creation time.                                                                                                                   |
| Validators (`v.string()`, `v.id()`, etc.) | Column and argument types (`t.string()`, `t.u64()`, `t.identity()`, etc.) | SpacetimeDB validates values through the module schema and generated bindings.                                                                                                                        |
| Query                                     | View function, subscription, or SQL query                                 | Use a [view](/docs/functions/views) for server-side computed read results. Use a [subscription](/docs/clients/subscriptions) when the client needs live rows in its cache.                            |
| Mutation                                  | Reducer                                                                   | Use a [reducer](/docs/functions/reducers) for every normal state-changing operation. Reducers are transactional and deterministic.                                                                    |
| Action                                    | Procedure                                                                 | Use a [procedure](/docs/functions/procedures) when the function needs side effects, especially outbound HTTP. If it only updates database state, migrate it to a reducer.                             |
| HTTP Action                               | HTTP handler                                                              | Use an [HTTP handler](/docs/functions/http-handlers) for inbound HTTP routes such as webhooks or public HTTP APIs. Use a procedure for callable side-effecting functions that are not HTTP endpoints. |
| Internal query / mutation / action        | Private reducer, private procedure, helper function, or private table     | Keep helper logic unexported when it should not be callable by clients. Use private tables for server-only data.                                                                                      |
| Function args                             | Reducer, procedure, or view arguments                                     | Reducers and procedures accept typed arguments. Views currently do not accept user-defined arguments beyond the context.                                                                              |
| Function return value                     | Procedure return value, view result, subscribed rows, or event table row  | Reducers should not return data. Put durable state in tables, derived read results in views, one-shot notifications in event tables, and side-effect results in procedure returns.                    |
| `ctx.db`                                  | `ctx.db` / `ctx.Db` / `ctx.db.*()`                                        | Reducers and views get transactional database access through their context. Procedures must open a transaction explicitly with `withTx` / `WithTx`.                                                   |
| `ctx.auth.getUserIdentity()`              | `ctx.sender`, `ctx.Sender`, `ctx.sender()` and `senderAuth` claims        | Use the caller's `Identity` for authorization. Use auth claims when you need issuer, subject, audience, or custom claims.                                                                             |
| Auth provider config                      | OIDC provider plus SpacetimeDB authentication config                      | SpacetimeDB works with OIDC providers including SpacetimeAuth, Auth0, Clerk, and others.                                                                                                              |
| Public function                           | Public reducer, procedure, view, or HTTP route                            | Exported reducers and procedures can be called by connected clients unless you enforce authorization in module code.                                                                                  |
| `api.foo.bar`                             | Generated module bindings                                                 | SpacetimeDB generates strongly typed client APIs from the published module.                                                                                                                           |
| `useQuery`                                | Subscription plus client cache, or view subscription                      | Subscribe to the rows or views your UI needs, then render from the generated client cache.                                                                                                            |
| `useMutation`                             | Generated reducer call                                                    | Client SDKs expose generated methods for calling reducers.                                                                                                                                            |
| `useAction`                               | Generated procedure call                                                  | Use only when the server function needs procedure capabilities.                                                                                                                                       |
| Realtime query updates                    | Subscription updates                                                      | SpacetimeDB pushes table and view changes for active subscriptions.                                                                                                                                   |
| Index                                     | Index                                                                     | Define indexes on columns used for lookup, filtering, joins, and subscription performance.                                                                                                            |
| Filter                                    | SQL predicate or indexed table lookup                                     | Prefer indexed lookups for hot paths. Subscription queries should be supported by suitable indexes.                                                                                                   |
| Pagination                                | Limit/range query, cursor table, or application-level pagination          | Model pagination around stable ordering columns, usually timestamps or monotonic IDs.                                                                                                                 |
| Scheduled function                        | Schedule table                                                            | Insert rows into a schedule table to run a reducer or procedure at a time or interval.                                                                                                                |
| Cron job                                  | Schedule table with interval rows                                         | Use interval schedules for recurring jobs.                                                                                                                                                            |
| File Storage                              | Binary column or external storage reference                               | Store small binary data inline when it should participate in transactions. Store large objects externally and keep metadata/URLs in tables.                                                           |
| Components                                | Submodules or separate modules/databases                                  | Convex Components package isolated code and data. In SpacetimeDB, use submodules where available, or isolate reusable systems as separate modules/databases with explicit APIs.                       |
| Environment variables                     | Module configuration and host environment                                 | Keep secrets out of reducers. Use procedures or external services for operations that require secret-backed side effects.                                                                             |
| Dashboard logs                            | `spacetime logs` and host logs                                            | Use CLI and host logs to debug module execution.                                                                                                                                                      |
| `npx convex dev`                          | `spacetime dev`                                                           | Runs development mode with rebuild, publish, and binding generation.                                                                                                                                  |
| `npx convex deploy`                       | `spacetime publish`                                                       | Publishes a module to a SpacetimeDB database.                                                                                                                                                         |

## Migration Strategy

### 1. Inventory your Convex backend

Start by listing every table and function in the Convex app:

- Tables in `convex/schema.ts`.
- Queries used by UI components.
- Mutations called from user actions.
- Actions used for third-party APIs, emails, payments, search, AI, or other
  side effects.
- HTTP actions used for webhooks or public endpoints.
- Scheduled functions and cron jobs.
- Auth assumptions, especially user ID fields and provider-specific claims.
- File storage usage.
- Components and shared backend packages.

Then classify each function by what it really does:

- **Pure read**: migrate to a view, a subscription, or a client-side read from
  subscribed rows.
- **Database write**: migrate to a reducer.
- **Side effect**: migrate to a procedure, usually with a small transactional
  reducer or `withTx` block around database changes.
- **Inbound HTTP**: migrate to an HTTP handler.
- **Scheduled work**: migrate to a schedule table that triggers a reducer or
  procedure.

### 2. Redesign tables as relational rows

Convex documents are JSON-like objects. SpacetimeDB tables are typed relational
rows. Do not mechanically copy every nested document into one wide table.
Instead, split data by access pattern.

For example, a Convex `users` document like this:

```typescript
users: defineTable({
  name: v.string(),
  avatarUrl: v.optional(v.string()),
  preferences: v.object({
    theme: v.string(),
    emailNotifications: v.boolean(),
  }),
  lastSeenAt: v.number(),
});
```

might become separate SpacetimeDB tables:

```typescript
import { schema, table, t } from 'spacetimedb/server';

const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    avatarUrl: t.option(t.string()),
    lastSeenAt: t.timestamp().index('btree'),
  }
);

const userPreference = table(
  { name: 'user_preference', public: true },
  {
    identity: t.identity().primaryKey(),
    theme: t.string(),
    emailNotifications: t.bool(),
  }
);

const spacetimeDb = schema({ user, userPreference });
export default spacetimeDb;
```

Use this rule of thumb: if two fields are read or updated at different rates,
consider separate tables. This reduces subscription bandwidth and keeps hot data
small.

### 3. Replace mutations with reducers

Convex mutations usually become SpacetimeDB reducers. Move validation,
authorization, and writes into the reducer.

Convex:

```typescript
export const send = mutation({
  args: { channelId: v.id('channels'), body: v.string() },
  handler: async (ctx, args) => {
    const identity = await ctx.auth.getUserIdentity();
    if (identity === null) throw new Error('Not signed in');

    return await ctx.db.insert('messages', {
      channelId: args.channelId,
      author: identity.subject,
      body: args.body,
      createdAt: Date.now(),
    });
  },
});
```

SpacetimeDB:

```typescript
import { schema, table, t, SenderError } from 'spacetimedb/server';

const message = table(
  { name: 'message', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    channelId: t.u64().index('btree'),
    author: t.identity().index('btree'),
    body: t.string(),
    createdAt: t.timestamp().index('btree'),
  }
);

const spacetimeDb = schema({ message });
export default spacetimeDb;

export const sendMessage = spacetimeDb.reducer(
  { channelId: t.u64(), body: t.string() },
  (ctx, { channelId, body }) => {
    if (body.trim() === '') {
      throw new SenderError('Message body cannot be empty');
    }

    ctx.db.message.insert({
      id: 0n,
      channelId,
      author: ctx.sender,
      body,
      createdAt: ctx.timestamp,
    });
  }
);
```

Notice two differences:

- The reducer uses `ctx.sender` as the authenticated caller. Do not accept a
  user identity as a client-provided argument.
- The reducer does not return the inserted row ID. Clients learn about the new
  row through their subscription to `message`.

If the client needs a one-shot success or failure notification, use the SDK's
per-call reducer result callbacks. If other subscribers need a transient event,
insert into an event table inside the reducer.

### 4. Replace queries with subscriptions and views

Convex queries are often used for two different things:

- Fetching live rows for the UI.
- Computing a server-side result from one or more tables.

For live rows, subscribe to tables or SQL queries and render from the client
cache. For computed read models, define a view.

Convex:

```typescript
export const latestMessages = query({
  args: { channelId: v.id('channels') },
  handler: async (ctx, { channelId }) => {
    return await ctx.db
      .query('messages')
      .withIndex('by_channel', q => q.eq('channelId', channelId))
      .order('desc')
      .take(100);
  },
});
```

SpacetimeDB options:

- Subscribe to `SELECT * FROM message WHERE channelId = ... ORDER BY createdAt DESC LIMIT 100`
  when the client knows the channel ID and needs live rows.
- Define a public view when the server should expose a reusable, computed read
  model.
- Add a `channelId` and `createdAt` index pattern that supports the query shape.

Views are especially useful for joins and derived rows:

```typescript
import type { Timestamp } from 'spacetimedb';

const messageWithAuthor = t.row('MessageWithAuthor', {
  id: t.u64(),
  channelId: t.u64(),
  authorName: t.string(),
  body: t.string(),
  createdAt: t.timestamp(),
});

export const messagesWithAuthors = spacetimeDb.anonymousView(
  { name: 'messages_with_authors', public: true },
  t.array(messageWithAuthor),
  ctx => {
    const rows: Array<{
      id: bigint;
      channelId: bigint;
      authorName: string;
      body: string;
      createdAt: Timestamp;
    }> = [];
    for (const msg of ctx.db.message.iter()) {
      const author = ctx.db.user.identity.find(msg.author);
      if (author) {
        rows.push({
          id: msg.id,
          channelId: msg.channelId,
          authorName: author.name,
          body: msg.body,
          createdAt: msg.createdAt,
        });
      }
    }
    return rows;
  }
);
```

Views currently do not take arbitrary client arguments. If a Convex query takes
arguments, either subscribe with a parameterized SQL/query-builder expression
from the client, model the argument as part of the subscribed table data, or
create a view whose result can be filtered by the client's subscription.

### 5. Split actions into reducers and procedures

Convex actions can call third-party services and can call queries or mutations.
In SpacetimeDB, keep deterministic database changes in reducers and move
side-effecting work to procedures.

Use this pattern for workflows such as payment processing:

1. A reducer records the requested operation and validates the caller.
2. A procedure performs the external API call.
3. The procedure commits the resulting database changes with `withTx`, or calls
   a reducer if the operation can be expressed as a normal state transition.

Avoid doing network work in reducers. Reducers must be deterministic and
transactional.

### 6. Replace HTTP actions with HTTP handlers

Convex HTTP actions become SpacetimeDB HTTP handlers. Use them for webhooks,
OAuth callbacks, upload callbacks, and public HTTP APIs.

Use procedures instead when the caller is a SpacetimeDB client and the function
needs side effects but does not need an HTTP route.

### 7. Migrate auth and users

Convex functions often use `ctx.auth.getUserIdentity()`. In SpacetimeDB, every
call has a caller `Identity` available in the function context. Store user rows
keyed by `Identity`:

```typescript
const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    displayName: t.string(),
    createdAt: t.timestamp(),
  }
);

export const createProfile = spacetimeDb.reducer(
  { displayName: t.string() },
  (ctx, { displayName }) => {
    ctx.db.user.insert({
      identity: ctx.sender,
      displayName,
      createdAt: ctx.timestamp,
    });
  }
);
```

When you need provider-specific data, inspect the OIDC claims available through
the auth context. For authorization, check `ctx.sender` and claims inside
reducers, views, procedures, and connection lifecycle reducers.

### 8. Migrate scheduled work

Convex scheduled functions and cron jobs map to schedule tables. A schedule
table stores rows that cause a reducer or procedure to run at a specific time or
on an interval.

Use scheduled reducers for deterministic database maintenance. Use scheduled
procedures for jobs that need external I/O, such as sending email or calling a
third-party API.

### 9. Migrate files

Convex File Storage maps to one of two SpacetimeDB patterns:

- Store small binary data directly in table columns when it should be
  transactional and live-updated with the row.
- Store large files in object storage and keep metadata, ownership, and URLs in
  SpacetimeDB tables.

For browser uploads, a common pattern is:

1. Client uploads the file to object storage using your existing upload flow.
2. Client calls a reducer to register metadata and ownership.
3. Other clients receive the metadata through subscriptions.

### 10. Migrate components and shared backend code

Convex Components package code and data behind explicit interfaces. In
SpacetimeDB, model that boundary explicitly:

- Use submodules where they are available for reusable isolated systems.
- Use separate modules/databases when you need operational isolation.
- Keep reusable pure logic in normal language modules or packages.
- Keep integration boundaries explicit with procedures, HTTP handlers, and
  narrow table schemas.

Do not give shared code direct access to unrelated tables just because it lives
in the same module. Preserve the interface boundary that made the Convex
component safe to reuse.

## Migration Checklist

- [ ] Create a SpacetimeDB project in the server language you plan to use.
- [ ] Define tables for each persistent data access pattern.
- [ ] Replace Convex document IDs with explicit primary keys, unique keys, and
      `Identity` columns.
- [ ] Add explicit timestamp columns for creation or update times you depend on.
- [ ] Add indexes for lookup, filtering, joins, and subscription queries.
- [ ] Convert database-writing mutations to reducers.
- [ ] Replace mutation return values with subscriptions, event tables, views, or
      procedure returns.
- [ ] Convert pure read queries to subscriptions or views.
- [ ] Convert side-effecting actions to procedures.
- [ ] Convert inbound HTTP actions to HTTP handlers.
- [ ] Move scheduled functions and cron jobs to schedule tables.
- [ ] Port auth checks to `ctx.sender` and OIDC claim checks.
- [ ] Generate client bindings with `spacetime generate` or `spacetime dev`.
- [ ] Update clients to connect, subscribe, render from the client cache, and
      call generated reducers/procedures.
- [ ] Publish with `spacetime publish`.

## Common Pitfalls

### Expecting reducers to return data

Reducers are for transactional state changes. If a Convex mutation returned data
that the UI needs, model that data as rows and subscribe to it. Use event tables
for transient messages and procedures for explicit request/response workflows
that require a return value.

### Porting documents without normalization

A direct document-to-row conversion can create large rows that update too often
and produce unnecessary subscription traffic. Split tables by access pattern and
update frequency.

### Passing user IDs from the client

Do not trust a user ID argument from the client for authorization. Use the
caller identity from the context and look up the user's row from that identity.

### Using procedures for normal writes

Procedures are powerful, but reducers are the default write path. Use reducers
unless you need procedure-only capabilities such as outbound HTTP.

### Forgetting indexes

Convex query code often makes index usage visible with `withIndex(...)`.
SpacetimeDB needs the same design step: define indexes for the lookups and
subscriptions your app depends on.

## Where to Go Next

- [Tables](/docs/tables)
- [Reducers](/docs/functions/reducers)
- [Views](/docs/functions/views)
- [Procedures](/docs/functions/procedures)
- [HTTP handlers](/docs/functions/http-handlers)
- [Subscriptions](/docs/clients/subscriptions)
- [Authentication](../../00200-core-concepts/00500-authentication.md)
- [Schedule tables](/docs/tables/schedule-tables)
