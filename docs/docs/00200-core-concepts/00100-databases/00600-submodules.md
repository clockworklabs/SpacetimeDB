---
title: Submodules
slug: /submodules
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

A **submodule** is a SpacetimeDB module that can be included in another module's database. The submodule's tables and functions register under a **namespace** you choose, keeping them separate from the consumer's own tables and from other submodules.

Submodules let you package reusable database logic as a library that any consumer can integrate without coordinating table names.

:::note
Submodules are currently supported in TypeScript only. Support for Rust, C#, and C++ is coming soon.
:::

## Writing a Submodule

A submodule is a regular SpacetimeDB module. Nothing special marks a module as a submodule. Export the schema as the default export and export every function (reducers, procedures, views, HTTP handlers) the consumer needs to register.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// auth_lib/src/index.ts
import { schema, table, t, SyncResponse, Router } from 'spacetimedb/server';

const users = table(
  { name: 'users', public: true },
  { identity: t.identity().primaryKey(), username: t.string() }
);

const sessions = table(
  { name: 'sessions' },
  {
    id: t.u64().primaryKey().autoInc(),
    user_identity: t.identity(),
    token: t.string(),
  }
);

const spacetimedb = schema({ users, sessions });
export default spacetimedb;

export const verify_token = spacetimedb.reducer(
  { token: t.string() },
  (ctx, { token }) => { /* ... */ }
);

export const session_count = spacetimedb.procedure(
  t.u64(),
  (ctx) => ctx.withTx(tx => tx.db.sessions.count())
);

export const active_sessions = spacetimedb.anonymousView(
  { name: 'active_sessions', public: true },
  t.array(sessions.rowType),
  (ctx) => [...ctx.db.sessions.iter()]
);

export const health = spacetimedb.httpHandler(
  (_ctx, _req) => new SyncResponse('ok')
);

export const router = spacetimedb.httpRouter(
  new Router().get('/health', health)
);
```

The same file can be published as a standalone database or used as a submodule by another module. The module does not need to declare which role it plays.

</TabItem>
</Tabs>

## Using a Submodule

The consumer controls the namespace name. Pass the submodule's module-namespace object under the alias you choose.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// my-database/src/index.ts
import { schema } from 'spacetimedb/server';
import * as authLib from 'auth_lib';

const players = table({ name: 'players', public: true }, { /* ... */ });

const spacetimedb = schema({
  players,
  myauth: authLib,   // register auth_lib under the namespace "myauth"
});
export default spacetimedb;
```

`export default spacetimedb` is the only JS export required from the consumer. Registering the submodule adds all of its reducers, procedures, views, scheduled tables, and HTTP handlers automatically.

:::warning Use `import * as`, not a default import
```typescript
import authLib from 'auth_lib';        // ❌ misses all named exports
import * as authLib from 'auth_lib';   // ✅ correct
```
A default-only import exposes only the submodule's schema, not its named exports (reducers, procedures, views, handlers). The submodule walker requires the full module-namespace object. A clear error is raised if you use the wrong form.
:::

</TabItem>
</Tabs>

## Accessing Submodule Tables and Views

Submodule tables appear under a namespace field on `ctx.db`. The field matches the alias you chose. Views exported by a submodule behave like tables from the client's perspective: they are accessible as `<namespace>.<view_name>` in subscriptions and SQL queries.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
spacetimedb.reducer('example', {}, (ctx) => {
  // Consumer's own tables (no namespace)
  for (const player of ctx.db.players.iter()) { /* ... */ }

  // Submodule tables, registered as "myauth"
  const user = ctx.db.myauth.users.identity.find(ctx.sender);
  for (const session of ctx.db.myauth.sessions.iter()) { /* ... */ }
});
```

</TabItem>
</Tabs>

## Calling Submodule Functions

A submodule can expose reducers, procedures, views, and HTTP handlers that the consumer calls from its own functions. Because the submodule's context type and the consumer's context type are distinct, use `ctx.as.<alias>` to narrow the context before passing it to a submodule function.

### From a Reducer

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Call a submodule reducer or a plain helper function typed against the submodule's schema using `ctx.as.<alias>`:

```typescript
// auth_lib: plain helper function typed against the submodule's own schema
export function sessionCountHelper(ctx: ReducerContext<typeof spacetimedb>): number {
  return ctx.db.sessions.count();
}

// my-database: call submodule reducer and helper from a consumer reducer
spacetimedb.reducer('on_login', { token: t.string() }, (ctx, { token }) => {
  // call a submodule reducer
  authLib.verify_token(ctx.as.myauth, { token });

  // call a submodule helper function
  const count = authLib.sessionCountHelper(ctx.as.myauth);
  console.log(`Active sessions: ${count}`);
});
```

`ctx.as.myauth` is a `ReducerContext` scoped to the `myauth` namespace. It shares the same sender, timestamp, and connectionId as the parent context, but its `ctx.db` points at `ctx.db.myauth`.

For reducers registered through the submodule's own schema (via `schema.reducer(...)`), the host passes a scoped context automatically when invoked directly. `ctx.as` is only needed when the consumer calls a submodule function explicitly.

</TabItem>
</Tabs>

### From a Procedure

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Use `ctx.as.<alias>` to pass a submodule-scoped `ProcedureContext` to a submodule procedure. To call a submodule reducer from inside a procedure, open a transaction first with `ctx.withTx` and then narrow with `tx.as.<alias>`:

```typescript
// call a submodule procedure
export const stats = spacetimedb.procedure(
  t.u64(),
  (ctx) => authLib.session_count(ctx.as.myauth)
);

// call a submodule reducer inside a withTx block
export const transact_and_count = spacetimedb.procedure(
  { token: t.string() },
  t.u64(),
  (ctx, { token }) => {
    ctx.withTx(tx => {
      // tx is a root ReducerContext; narrow to the submodule namespace
      authLib.verify_token(tx.as.myauth, { token });
    });
    return authLib.session_count(ctx.as.myauth);
  }
);
```

</TabItem>
</Tabs>

### From an HTTP Handler

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Delegate to a submodule's HTTP handler by passing `ctx.as.<alias>` and the request to the submodule handler function, then register it on the consumer's router:

```typescript
import { Router } from 'spacetimedb/server';

// delegate the /health route to the submodule's handler
export const health_check = spacetimedb.httpHandler((ctx, req) => {
  return authLib.health(ctx.as.myauth, req);
});

export const router = spacetimedb.httpRouter(
  new Router().get('/health', health_check)
);
```

</TabItem>
</Tabs>

## Multiple and Nested Submodules

Multiple submodules compose freely. Each gets its own namespace, so name collisions between submodules are impossible.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import * as authLib    from 'auth_lib';
import * as paymentLib from 'payment_lib';

const spacetimedb = schema({
  players,
  myauth:   authLib,
  payments: paymentLib,
});
export default spacetimedb;
```

A submodule can itself include other submodules using the same syntax. The nested submodule's tables appear under a two-level path in the top-level consumer:

```typescript
// auth_lib includes session_lib as "sessions"
const authSchema = schema({ users, sessions: sessionLib });
export default authSchema;

// consumer includes auth_lib as "myauth"
// session_lib's tables are at ctx.db.myauth.sessions.<table>
```

**Lifecycle reducers** (`init`, `clientConnected`, `clientDisconnected`) are an exception: these are only allowed for the root module. Modules containing submodules which define lifecycle reducers will fail to publish.

</TabItem>
</Tabs>

## Client Subscriptions

Client subscriptions use the same namespace structure as server-side access. Submodule tables and views are queried as `<namespace>.<name>`.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
conn.subscriptionBuilder()
  .addQuery(q => q.from.players.build())                   // public.players
  .addQuery(q => q.from.myauth.users.build())              // myauth.users
  .addQuery(q => q.from.myauth.activeSessions.build())     // myauth.active_sessions view
  .subscribe();
```

</TabItem>
</Tabs>

## Calling Submodule Reducers and Procedures from the Client

Submodule reducers and procedures are identified by their fully-qualified name, using `/` as the separator between namespace and function name.

### Client SDK

In generated bindings, submodule tables, views, reducers, and procedures appear as nested objects under the namespace alias. The `tables`, `reducers`, and `procedures` exports all reflect the same nesting.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

React hooks:

```typescript
import { tables, reducers, procedures } from './module_bindings';
import { useTable, useReducer, useProcedure } from 'spacetimedb/react';

// subscribe to a submodule table and view
const [users] = useTable(tables.myauth.users);
const [activeSessions] = useTable(tables.myauth.activeSessions);

// call a submodule reducer
const verifyToken = useReducer(reducers.myauth.verifyToken);
verifyToken({ token: 'abc123' });

// call a submodule procedure
const sessionCount = useProcedure(procedures.myauth.sessionCount);
sessionCount().then(count => console.log(`Sessions: ${count}`));
```

Vanilla (non-React):

```typescript
import { DbConnection, tables, reducers, procedures } from './module_bindings';

const conn = DbConnection.builder()
  .withUri(SPACETIMEDB_URI)
  .withDatabaseName('my-database')
  .onConnect(ctx => {
    ctx.subscriptionBuilder()
      .subscribe([tables.myauth.users, tables.myauth.activeSessions]);
  })
  .build();

conn.reducers.myauth.verifyToken({ token: 'abc123' });
```

</TabItem>
</Tabs>

### HTTP API

```
POST /v1/database/my-database/call/myauth/verify_token
```

### CLI

```bash
spacetime call my-database "myauth/verify_token" '{"token": "abc123"}'
```

The namespace prefix is the alias you chose, and the function name after `/` is the snake_case export name from the submodule.

## Namespace Name Rules

The alias you choose becomes the SQL-level namespace name. It must be a valid SpacetimeDB identifier: starts with a letter or underscore, continues with letters, digits, or underscores, maximum 63 characters, case-insensitive for resolution. A submodule can be registered under at most one alias per consumer module.

The reserved namespaces `public`, `st`, `spacetimedb`, and `pg_*` cannot be used as submodule aliases.

## Limitations

### Submodule routers are not applied automatically

A submodule can define its own `httpRouter`, but when used as a submodule that router is ignored. Only the consumer's root router is used. To expose a submodule's HTTP handlers, register them explicitly on the consumer's router using `ctx.as.<alias>` as shown in the [HTTP handler section](#from-an-http-handler).
