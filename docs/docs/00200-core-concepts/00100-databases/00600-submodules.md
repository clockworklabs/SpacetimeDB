---
title: Submodules
slug: /submodules
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

A **submodule** is a SpacetimeDB module that can be included in another module's database. The submodule's tables and functions register under a **namespace** you choose, keeping them separate from the consumer's own tables and from other submodules.

Submodules let you package reusable database logic as a library that any consumer can integrate without coordinating table names.

:::note
Submodules are supported in TypeScript and C# (.NET 10 only). Cross-language composition is not supported.
:::

## Writing a Submodule

A submodule is a regular SpacetimeDB module. The same module can be published independently or included by another module; no special root/library build role is required.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Export the schema as the default export and export every function the consumer needs to register.

```typescript
// auth_lib/src/index.ts
import { schema, table, t, SyncResponse, Router, type ReducerCtx } from 'spacetimedb/server';

const users = table(
  { name: 'users', public: true },
  { identity: t.identity().primaryKey(), username: t.string() }
);

const sessions = table(
  { name: 'sessions' },
  {
    id: t.u64().primaryKey().autoInc(),
    userIdentity: t.identity(),
    token: t.string(),
  }
);

const spacetimedb = schema({ users, sessions });
export default spacetimedb;

export const verifyToken = spacetimedb.reducer(
  { token: t.string() },
  (ctx, { token }) => { /* ... */ }
);

export const sessionCount = spacetimedb.procedure(
  t.u64(),
  (ctx) => ctx.withTx(tx => tx.db.sessions.count())
);

export const activeSessions = spacetimedb.anonymousView(
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
<TabItem value="csharp" label="C#">

Create a normal C# module project targeting `net10.0`, with `SpacetimeDB.Runtime` 2.13 or later. Its public types and functions can be called by the consumer:

```csharp
// AuthLib/Lib.cs
using SpacetimeDB;

namespace AuthLib;

// Any accessible type in this assembly can identify it to the consumer.
public class Marker { }

[Table(Accessor = "User", Name = "auth_users", Public = true)]
public partial struct User
{
    [PrimaryKey]
    public uint Id;
    public uint Score;
}

public static partial class Functions
{
    public static void Insert(ReducerContext ctx, uint id) =>
        ctx.Db.User.Insert(new User { Id = id, Score = 42 });

    public static void Insert(ProcedureTxContext ctx, uint id) =>
        ctx.Db.User.Insert(new User { Id = id, Score = 42 });

    public static ulong Count(ReducerContext ctx) => ctx.Db.User.Count;

    [Reducer]
    public static void Add(ReducerContext ctx, uint id) => Insert(ctx, id);

    [Procedure]
    public static ulong CountUsers(ProcedureContext ctx) =>
        ctx.WithTx(tx => tx.Db.User.Count);

    [View(Accessor = "Users", Public = true)]
    public static User? Users(AnonymousViewContext ctx) => ctx.Db.User.Id.Find(2);
}
```

Library code uses its own flat accessors, such as `ctx.Db.User`, regardless of the namespace the consumer selects. The `Users` view returns the user with ID `2`, if present.

</TabItem>
</Tabs>

## Using a Submodule

The consumer controls the namespace name. The accessor name is used in module code and generated bindings; the canonical name is used in SQL and network calls.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// my-database/src/index.ts
import { schema, table } from 'spacetimedb/server';
import * as authLib from 'auth_lib';

const players = table({ name: 'players', public: true }, { /* ... */ });

const spacetimedb = schema({
  players,
  myauth: authLib,   // register auth_lib under the namespace "myauth"
});
export default spacetimedb;
```

`export default spacetimedb` is the only JS export required from the consumer. Registering the submodule adds all of its reducers, procedures, views, scheduled tables, and HTTP handlers automatically.

### Accessor and canonical namespace names

Like a table, a namespace has two names. The key you register the submodule under is its **accessor name**: the name module code and generated client bindings use, such as `ctx.db.myAuth` or `tables.myAuth.users`. The **canonical name** is what the database stores and what SQL, subscriptions, the CLI, and the HTTP API use, such as `my_auth.users`. By default the canonical name is derived from the accessor name by the module's case conversion policy, exactly as it is for tables and reducers: `myAuth` in TypeScript becomes `my_auth` in the database.

To store a namespace under a specific canonical name instead, register the submodule as a `{ name, module }` object. `name` is used verbatim, and the key remains the accessor name:

```typescript
const spacetimedb = schema({
  players,
  myAuth: { name: 'myAuth', module: authLib },   // ctx.db.myAuth, stored as "myAuth"
  payments: { name: 'billing', module: paymentLib },   // ctx.db.payments, stored as "billing"
});
```

:::warning Changing a canonical namespace re-creates its tables
The canonical namespace is part of every table's identity. Publishing a module whose namespace canonicalizes differently than before, for example after renaming the key from `myauth` to `myAuth` without a `name`, is a migration that removes the tables under the old namespace and creates empty ones under the new namespace. To rename the key without touching the tables, pin the canonical name with `name`: changing only the key while keeping the same `name` only updates the accessor aliases.
:::

:::warning Use `import * as`, not a default import
```typescript
import authLib from 'auth_lib';        // ❌ misses all named exports
import * as authLib from 'auth_lib';   // ✅ correct
```
A default-only import exposes only the submodule's schema, not its named exports (reducers, procedures, views, handlers). The submodule walker requires the full module-namespace object. A clear error is raised if you use the wrong form.
:::

</TabItem>
<TabItem value="csharp" label="C#">

Reference the library with a normal `ProjectReference`, or a package containing its compiled module assembly. Both projects must target .NET 10 and use C# 14. For example, add this to the root module's project file:

```xml
<ItemGroup>
  <ProjectReference Include="libraries/AuthLib/AuthLib.csproj" />
  <!-- Prevent the root project from also compiling the library's source. -->
  <Compile Remove="libraries/**/*.cs" />
</ItemGroup>
```

Referenced module assemblies, including transitive dependencies, are discovered automatically. Without a namespace attribute, their tables and functions register once in the default `public` scope with flat accessors. To place a dependency in a named namespace, add an assembly attribute to the root:

```csharp
// Root module's Lib.cs, before any namespace or type declarations
using SpacetimeDB;

[assembly: Namespace(typeof(AuthLib.Marker), Accessor = "MyAuth", Name = "auth_data")]
```

`typeof(AuthLib.Marker)` identifies the dependency assembly. `Accessor = "MyAuth"` creates C# members such as `ctx.Db.MyAuth.User`. `Name = "auth_data"` pins the canonical database namespace, so SQL uses `auth_data.auth_users`.

When `Name` is omitted, the root module's case-conversion policy derives it from `Accessor`: under the default `SnakeCase` policy, `MyAuth` becomes `my_auth`. An explicit `Name` is used verbatim. Named dependencies retain their own table/function naming policy; dependencies in `public` inherit the root's policy and cannot explicitly declare a conflicting one.

:::warning Changing a canonical namespace re-creates its tables
The canonical namespace is part of each table's identity. Changing it removes the old namespace's tables and creates empty tables in the new namespace. To change only C# accessor aliases, keep `Name` fixed while changing `Accessor`.
:::

</TabItem>
</Tabs>

## Accessing Submodule Tables and Views

Submodule tables appear under a namespace accessor. The accessor does not change when you pin a different canonical name. Submodule views are also accessible in client subscriptions and SQL as `<namespace>.<view_name>`, using canonical names.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Access submodule tables through `ctx.db.myauth`. The `activeSessions` view is `myauth.active_sessions` in SQL.

```typescript
export const example = spacetimedb.reducer((ctx) => {
  // Consumer's own tables (no namespace)
  for (const player of ctx.db.players.iter()) { /* ... */ }

  // Submodule tables, registered as "myauth"
  const user = ctx.db.myauth.users.identity.find(ctx.sender);
  for (const session of ctx.db.myauth.sessions.iter()) { /* ... */ }
});
```

</TabItem>
<TabItem value="csharp" label="C#">

Access submodule tables through `ctx.Db.MyAuth`. The explicitly named table is `auth_data.auth_users` in SQL, and the `Users` view is `auth_data.users`.

```csharp
[Reducer]
public static void LookupAuthUser(ReducerContext ctx, uint id)
{
    AuthLib.User? user = ctx.Db.MyAuth.User.Id.Find(id);
    foreach (var row in ctx.Db.MyAuth.User.Iter())
    {
        Log.Info($"Auth user: {row.Id}");
    }
}
```

Root views can also query submodule tables using `ctx.From.MyAuth.User()`.

```csharp
[View(Accessor = "AuthUsers", Public = true)]
public static IQuery<AuthLib.User> AuthUsers(AnonymousViewContext ctx) =>
    ctx.From.MyAuth.User().Where(user => user.Score.Eq(42u));
```

</TabItem>
</Tabs>

## Calling Submodule Functions

A consumer can call public submodule functions directly. A namespace is not a security boundary between ordinary helper calls.

### From a Reducer

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Call a submodule reducer or a plain helper function typed against the submodule's schema using `ctx.as.<alias>`:

```typescript
// auth_lib: plain helper function typed against the submodule's own schema
export function sessionCountHelper(ctx: ReducerCtx<typeof spacetimedb.schemaType>): bigint {
  return ctx.db.sessions.count();
}

// my-database: call submodule reducer and helper from a consumer reducer
export const onLogin = spacetimedb.reducer({ token: t.string() }, (ctx, { token }) => {
  // call a submodule reducer
  authLib.verifyToken(ctx.as.myauth, { token });

  // call a submodule helper function
  const count = authLib.sessionCountHelper(ctx.as.myauth);
  console.log(`Active sessions: ${count}`);
});
```

`ctx.as.myauth` is a `ReducerCtx` scoped to the `myauth` namespace. It shares the same sender, timestamp, and connectionId as the parent context, but its `ctx.db` points at `ctx.db.myauth`.

For reducers registered through the submodule's own schema (via `schema.reducer(...)`), the host passes a scoped context automatically when invoked directly. `ctx.as` is only needed when the consumer calls a submodule function explicitly.

</TabItem>
<TabItem value="csharp" label="C#">

Call the library's reducer or helper as an ordinary C# method. Compiled library helpers resolve their own table accessors and share the caller's context and transaction:

```csharp
[Reducer]
public static void AddAuthUser(ReducerContext ctx, uint id)
{
    AuthLib.Functions.Add(ctx, id);
    Log.Info($"Auth users: {AuthLib.Functions.Count(ctx)}");
}
```

These calls run in the root reducer's transaction. If the reducer fails, its writes to both root and submodule tables roll back together.

</TabItem>
</Tabs>

### From a Procedure

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Use `ctx.as.<alias>` to pass a submodule-scoped `ProcedureCtx` to a submodule procedure. To call a submodule reducer from inside a procedure, open a transaction first with `ctx.withTx` and then narrow with `tx.as.<alias>`:

```typescript
// call a submodule procedure
export const stats = spacetimedb.procedure(
  t.u64(),
  (ctx) => authLib.sessionCount(ctx.as.myauth)
);

// call a submodule reducer inside a withTx block
export const transactAndCount = spacetimedb.procedure(
  { token: t.string() },
  t.u64(),
  (ctx, { token }) => {
    ctx.withTx(tx => {
      // tx is a root ReducerCtx; narrow to the submodule namespace
      authLib.verifyToken(tx.as.myauth, { token });
    });
    return authLib.sessionCount(ctx.as.myauth);
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

Pass the procedure context to a library procedure. Inside `WithTx`, use helpers accepting `ProcedureTxContext`, such as the `Insert` overload above:

```csharp
[Procedure]
public static ulong CountAuthUsers(ProcedureContext ctx) =>
    AuthLib.Functions.CountUsers(ctx);

[Procedure]
public static ulong AddAndCountAuthUsers(ProcedureContext ctx, uint id) =>
    ctx.WithTx(tx =>
    {
        AuthLib.Functions.Insert(tx, id);
        return tx.Db.MyAuth.User.Count;
    });
```

Root and library writes within the same `WithTx` block share a transaction.

</TabItem>
</Tabs>

### From an HTTP Handler

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Delegate to a submodule's HTTP handler by passing `ctx.as.<alias>` and the request to the submodule handler function, then register it on the consumer's router:

```typescript
import { Router } from 'spacetimedb/server';

// delegate the /health route to the submodule's handler
export const healthCheck = spacetimedb.httpHandler((ctx, req) => {
  return authLib.health(ctx.as.myauth, req);
});

export const router = spacetimedb.httpRouter(
  new Router().get('/health', healthCheck)
);
```

</TabItem>
<TabItem value="csharp" label="C#">

Pass the handler context directly to a library handler. Declare the route in the root, because routes from named dependencies are ignored:

```csharp
// Add to AuthLib.Functions. Handler transactions are an unstable API.
#pragma warning disable STDB_UNSTABLE
[HttpHandler]
public static HttpResponse AuthCount(HandlerContext ctx, HttpRequest request) =>
    new(200, HttpVersion.Http11, [],
        HttpBody.FromString(ctx.WithTx(tx => tx.Db.User.Count).ToString()));
```

```csharp
// Add to the root module's partial functions class.
[HttpHandler]
public static HttpResponse RootAuthCount(HandlerContext ctx, HttpRequest request) =>
    AuthLib.Functions.AuthCount(ctx, request);

[HttpRouter]
public static Router Routes() =>
    Router.New().Get("/auth-count", SpacetimeDB.Handlers.RootAuthCount);
```

</TabItem>
</Tabs>

## Multiple and Nested Submodules

You can include multiple distinct submodules under different namespaces. Tables and functions in separate namespaces can use the same names. Nested composition support differs by language.

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
<TabItem value="csharp" label="C#">

Declare a namespace for each dependency assembly in the root:

```csharp
[assembly: SpacetimeDB.Namespace(typeof(AuthLib.Marker), Accessor = "MyAuth", Name = "auth_data")]
[assembly: SpacetimeDB.Namespace(typeof(PaymentLib.Marker), Accessor = "Payments")]
```

The root uses `ctx.Db.MyAuth.User` and `ctx.Db.Payments.<table>`. **Nested namespace mounts are not supported in C#.** A dependency that declares its own namespace mounts cannot be composed, even in `public`. Only the root chooses placement, and each assembly can be included only once.

Named C# dependencies cannot define lifecycle reducers (`Init`, `ClientConnected`, `ClientDisconnected`). Such reducers remain valid when the dependency is published independently or registered in `public`, subject to the usual lifecycle uniqueness checks.

</TabItem>
</Tabs>

## Environment variables

Named submodules cannot declare a nonempty [environment](./00700-environment-variables.md). Such declarations cause composition or publication to fail. A module with environment declarations can still be published independently as a root module.

In C#, dependencies registered in the default `public` scope are an exception: their declarations contribute to the root environment schema.

Named submodules have no separate environment-variable namespace, and their host-dispatched entry points cannot read the root module's environment. Root module code can pass configuration values to helpers explicitly. Ordinary helper calls retain the calling entry point's access, including calls to helpers defined in submodules.

## Client Subscriptions

Client subscriptions use the same namespace structure as server-side access. Submodule tables and views are queried as `<namespace>.<name>`, using canonical names on the wire and accessor names in generated bindings.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
conn.subscriptionBuilder().subscribe(tables => [
  tables.players.build(),                   // public.players
  tables.myauth.users.build(),              // myauth.users
  tables.myauth.activeSessions.build(),     // myauth.active_sessions view
]);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// conn is a connected, generated DbConnection.
conn.SubscriptionBuilder()
    .AddQuery(q => q.From.MyAuth.User())   // auth_data.auth_users
    .AddQuery(q => q.From.MyAuth.Users())  // auth_data.users view
    .Subscribe();
```

The generated query builder translates C# accessors to canonical SQL names automatically. Subscribed rows are available through `conn.Db.MyAuth.User` and `conn.Db.MyAuth.Users`, using the same connection and cache as the root tables.

```csharp
// After the subscription has been applied, read the local cache.
foreach (var user in conn.Db.MyAuth.User.Iter())
{
    Console.WriteLine($"Auth user: {user.Id}, score: {user.Score}");
}

var cachedUser = conn.Db.MyAuth.User.Id.Find(10);
```

</TabItem>
</Tabs>

## Calling Submodule Reducers and Procedures from the Client

Submodule reducers and procedures are identified by their fully-qualified name, using `.` as the separator between namespace and function name.

### Client SDK

In generated bindings, submodule tables, views, reducers, and procedures appear under the namespace accessor.

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
<TabItem value="csharp" label="C#">

```csharp
// conn is a connected, generated DbConnection.
conn.Reducers.MyAuth.Add(10);

conn.Procedures.MyAuth.CountUsers((ctx, result) =>
{
    if (result.IsSuccess)
    {
        Console.WriteLine($"Auth users: {result.Value}");
    }
});
```

As with other C# client calls, keep processing connection events with `conn.FrameTick()` (or your SDK integration's event loop) to receive subscription updates and callbacks.

</TabItem>
</Tabs>

### HTTP API

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```
POST /v1/database/my-database/call/myauth.verify_token
```

</TabItem>
<TabItem value="csharp" label="C#">

```
POST /v1/database/my-database/call/auth_data.add
```

</TabItem>
</Tabs>

### CLI

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```bash
spacetime call my-database "myauth.verify_token" '"abc123"'
```

The accessor `verifyToken` becomes `verify_token` on the wire under the default naming policy.

</TabItem>
<TabItem value="csharp" label="C#">

```bash
spacetime call my-database "auth_data.add" 10
```

The accessor `Add` becomes `add` on the wire under the default naming policy.

</TabItem>
</Tabs>

Both parts of the wire name are canonical: the namespace prefix and the function name after `.`.

## Namespace Name Rules

Both the accessor name and the canonical name must be valid SpacetimeDB identifiers: start with a letter or underscore, continue with letters, digits, or underscores, maximum 63 UTF-8 bytes, case-insensitive for resolution. Two submodules cannot share a canonical name, so `myAuth` and `my_auth` cannot both be registered under the default policy. A submodule can be registered under at most one namespace per consumer module.

The reserved namespaces `st`, `spacetimedb`, and names starting with `pg_` cannot be used. `public` is the default scope, not a named submodule namespace.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

The schema key is the accessor name. The canonical name is derived from it unless explicitly set with `name`.

</TabItem>
<TabItem value="csharp" label="C#">

`Accessor` sets the accessor name. The canonical name is derived from it unless explicitly set with `Name`. Use `Accessor = "public"` to register a dependency in the default scope with flat accessors.

Accessors must also be valid C# identifiers. For a keyword, use its plain spelling in the attribute (`Accessor = "class"`) and escape it in expressions (`ctx.Db.@class.User`).

</TabItem>
</Tabs>

## Limitations

### Submodule routers are not applied automatically

Routes from named submodules are ignored. Register handlers explicitly on the root router, as shown in the [HTTP handler section](#from-an-http-handler). In C#, dependencies registered in `public` can contribute routes to root-level routing.

### Row-level security belongs in the root

Define [row-level security](../../00300-resources/00100-how-to/00400-row-level-security.md) filters in the root, referring to canonical qualified table names. Filters declared inside named submodules do not protect their tables.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Submodule filter declarations are accepted by the SDK, but the host does not install them. Export filters from the root schema instead, using qualified table names such as `myauth.users`.

</TabItem>
<TabItem value="csharp" label="C#">

Named dependencies cannot declare filters; the source generator rejects them. Define filters in the root instead, using qualified table names such as `auth_data.auth_users`.

</TabItem>
</Tabs>
