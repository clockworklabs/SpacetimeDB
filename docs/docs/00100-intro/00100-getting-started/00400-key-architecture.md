---
title: Key Architecture
slug: /intro/key-architecture
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


## Host

A SpacetimeDB **host** is a server that hosts [databases](#database). You can run your own host, or use the SpacetimeDB maincloud. Many databases can run on a single host.

## Database

A SpacetimeDB **database** is an application that runs on a [host](#host).

A database exports [tables](#table), which store data, and [reducers](#reducer), which allow [clients](#client) to make requests.

A database's schema and business logic is specified by a piece of software called a **module**. Modules can be written in C# or Rust.

(Technically, a SpacetimeDB module is a [WebAssembly module](https://developer.mozilla.org/en-US/docs/WebAssembly) or JavaScript bundle, that imports a specific low-level [WebAssembly ABI](/webassembly-abi) and exports a small number of special functions. However, the SpacetimeDB [server-side libraries](/databases) hide these low-level details. As a developer, writing a module is mostly like writing any other C# or Rust application, except for the fact that a [special CLI tool](https://spacetimedb.com/install) is used to deploy the application.)

## Table

A SpacetimeDB **table** is a SQL database table. Tables are declared in a module's native language. For instance, in C#, a table is declared like so:

<Tabs groupId="syntax" queryString>

<TabItem value="typescript" label="TypeScript">

```typescript
import { table, t } from 'spacetimedb/server';

const players = table(
  { name: 'players', public: true },
  {
    id: t.u64().primaryKey(),
    name: t.string(),
    age: t.u32(),
    user: t.identity(),
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Name = "players", Public = true)]
public partial struct Player
{
    [SpacetimeDB.PrimaryKey]
    uint playerId;
    string name;
    uint age;
    Identity user;
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = players, public)]
pub struct Player {
   #[primary_key]
   id: u64,
   name: String,
   age: u32,
   user: Identity,
}
```

</TabItem>

</Tabs>

The contents of a table can be read and updated by [reducers](#reducer).
Tables marked `public` can also be read by [clients](#client).

## Reducer

A **reducer** is a function exported by a [database](#database).
Connected [clients](/sdks) can call reducers to interact with the database.
This is a form of [remote procedure call](https://en.wikipedia.org/wiki/Remote_procedure_call).

<Tabs groupId="syntax" queryString>
<TabItem value="typescript" label="TypeScript">

A reducer can be written in a TypeScript module like so:

```typescript
spacetimedb.reducer('set_player_name', { id: t.u64(), name: t.string() }, (ctx, { id, name }) => {
   // ...
});
```

And a TypeScript [client](#client) can call that reducer:

```typescript
function main() {
   // ...setup code, then...
   ctx.reducers.setPlayerName(57n, "Marceline");
}
```

</TabItem>
<TabItem value="csharp" label="C#">

A reducer can be written in C# like so:

```csharp
[SpacetimeDB.Reducer]
public static void SetPlayerName(ReducerContext ctx, uint playerId, string name)
{
    // ...
}
```

And a C# [client](#client) can call that reducer:

```cs
void Main() {
   // ...setup code, then...
   Connection.Reducer.SetPlayerName(57, "Marceline");
}
```

</TabItem>
<TabItem value="rust" label="Rust">

A reducer can be written in Rust like so:

```rust
#[spacetimedb::reducer]
pub fn set_player_name(ctx: &spacetimedb::ReducerContext, id: u64, name: String) -> Result<(), String> {
   // ...
}
```

And a Rust [client](#client) can call that reducer:

```rust
fn main() {
   // ...setup code, then...
   ctx.reducers.set_player_name(57, "Marceline".into());
}
```

</TabItem>
</Tabs>

These look mostly like regular function calls, but under the hood,
the client sends a request over the internet, which the database processes and responds to.

The `ReducerContext` is a reducer's only mandatory parameter
and includes information about the caller's [identity](#identity).
This can be used to authenticate the caller.

Reducers are run in their own separate and atomic [database transactions](https://en.wikipedia.org/wiki/Database_transaction).
When a reducer completes successfully, the changes the reducer has made,
such as inserting a table row, are _committed_ to the database.
However, if the reducer instead returns an error, or throws an exception,
the database will instead reject the request and _revert_ all those changes.
That is, reducers and transactions are all-or-nothing requests.
It's not possible to keep the first half of a reducer's changes and discard the last.

Transactions are only started by requests from outside the database.
When a reducer calls another reducer directly, as in the example below,
the changes in the called reducer does not happen in its own child transaction.
Instead, when the nested reducer gracefully errors,
and the overall reducer completes successfully,
the changes in the nested one are still persisted.

<Tabs groupId="syntax" queryString>

<TabItem value="typescript" label="TypeScript">

```typescript
spacetimedb.reducer('hello', (ctx) => {
   try {
      world(ctx);
   } catch {
      otherChanges(ctx);
   }
});

spacetimedb.reducer('world', (ctx) => {
   clearAllTables(ctx);
   // ...
});
```

While SpacetimeDB doesn't support nested transactions,
a reducer can [schedule another reducer](/tables/scheduled-tables) to run at an interval,
or at a specific time.

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Reducer]
public static void Hello(ReducerContext ctx)
{
   if(!World(ctx))
   {
      OtherChanges(ctx);
   }
}

[SpacetimeDB.Reducer]
public static void World(ReducerContext ctx)
{
   ClearAllTables(ctx);
   // ...
}
```

While SpacetimeDB doesn't support nested transactions,
a reducer can [schedule another reducer](/tables/scheduled-tables) to run at an interval,
or at a specific time.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::reducer]
pub fn hello(ctx: &spacetimedb::ReducerContext) -> Result<(), String> {
   if world(ctx).is_err() {
      other_changes(ctx);
   }
}

#[spacetimedb::reducer]
pub fn world(ctx: &spacetimedb::ReducerContext) -> Result<(), String> {
    clear_all_tables(ctx);
}

```

While SpacetimeDB doesn't support nested transactions,
a reducer can [schedule another reducer](https://docs.rs/spacetimedb/latest/spacetimedb/attr.reducer.html#scheduled-reducers) to run at an interval,
or at a specific time.

</TabItem>
</Tabs>

See [Reducers](/functions/reducers) for more details about reducers.

## Procedure

A **procedure** is a function exported by a [database](#database), similar to a [reducer](#reducer).
Connected [clients](#client) can call procedures.
Procedures can perform additional operations not possible in reducers, including making HTTP requests to external services.
However, procedures don't automatically run in database transactions,
and must manually open and commit a transaction in order to read from or modify the database state.

Procedures are currently in beta, and their API may change in upcoming SpacetimeDB releases.

<Tabs groupId="syntax" queryString>
<TabItem value="typescript" label="TypeScript">

A procedure can be defined in a TypeScript module:

```typescript
spacetimedb.procedure("make_request", t.string(), ctx => {
   // ...
})
```

And a TypeScript [client](#client) can call that procedure:

```typescript
ctx.procedures.makeRequest();
```

A TypeScript [client](#client) can also register a callback to run when a procedure call finishes, which will be invoked with that procedure's return value:

```typescript
ctx.procedures.makeRequest().then(
    res => console.log(`Procedure make_request returned ${res}`),
    err => console.error(`Procedure make_request failed! ${err}`),
);
```

</TabItem>
<TabItem value="csharp" label="C#">

C# modules currently cannot define procedures. Support for defining procedures in C# modules will be released shortly.

A C# [client](#client) can call a procedure defined by a Rust or TypeScript module:

```csharp
void Main()
{
    // ...setup code, then...
    ctx.Procedures.MakeRequest();
}
```

A C# [client](#client) can also register a callback to run when a procedure call finishes, which will be invoked with that procedure's return value:

```csharp
void Main()
{
    // ...setup code, then...
    ctx.Procedures.MakeRequestThen((ctx, res) =>
    {
        if (res.IsSuccess)
        {
            Log.Debug($"Procedure `make_request` returned {res.Value!}");
        }
        else
        {
            throw new Exception($"Procedure `make_request` failed: {res.Error!}");
        }
    });
}
```

</TabItem>
<TabItem value="rust" label="Rust">

Because procedures are unstable, Rust modules that define them must opt in to the `unstable` feature in their `Cargo.toml`:

```toml
[dependencies]
spacetimedb = { version = "1.x", features = ["unstable"] }
```

Then, that module can define a procedure:

```rust
#[spacetimedb::procedure]
pub fn make_request(ctx: &mut spacetimedb::ProcedureContext) -> String {
    // ...
}
```

And a Rust [client](#client) can call that procedure:

```rust
fn main() {
    // ...setup code, then...
    ctx.procedures.make_request();
}
```

A Rust [client](#client) can also register a callback to run when a procedure call finishes, which will be invoked with that procedure's return value:

```rust
fn main() {
    // ...setup code, then...
    ctx.procedures.make_request_then(|ctx, res| {
        match res {
            Ok(string) => log::info!("Procedure `make_request` returned {string}"),
            Err(e) => log::error!("Procedure  `make_request` failed! {e:?}"),
        }
    })
}
```

</TabItem>
<TabItem value="cpp" label="Unreal C++">

An Unreal C++ [client](#client) can call a procedure defined by a Rust or TypeScript module:

```cpp
{
...
   // Call the procedure without a callback
   Context.Procedures->MakeRequest({});
}

```

An Unreal C++ [client](#client) can also register a callback to run when a procedure call finishes, which will be invoked with that procedure's return value:

```cpp
{
...
   FOnMakeRequestComplete Callback;
   BIND_DELEGATE_SAFE(Callback, this, AGameManager, OnMakeRequestComplete);
   Context.Procedures->MakeRequest(Callback);
}

// Make sure to mark any callback functions as UFUNCTION() or they will not be executed
void AGameManager::OnMakeRequestComplete(const FProcedureEventContext& Context, const FString& Result, bool bSuccess)
{
   UE_LOG(LogTemp, Log, TEXT("Procedure `MakeRequest` returned %s"), *Result);
}

```

</TabItem>
<TabItem value="blueprint" label="Unreal Blueprint">

An Unreal [client](#client) can call a procedure defined by a Rust or TypeScript module:

![MakeRequest without callback](/images/unreal/intro/ue-blueprint-makerequest-nocallback.png)

An Unreal [client](#client) can also register a callback to run when a procedure call finishes, which will be invoked with that procedure's return value:

![MakeRequest with callback](/images/unreal/intro/ue-blueprint-makerequest-with-callback.png)

</TabItem>
</Tabs>

See [Procedures](/functions/procedures) for more details about procedures.

## View

A **view** is a read-only function exported by a [database](#database) that computes and returns results from tables. Unlike [reducers](#reducer), views do not modify database state - they only query and return data. Views are useful for computing derived data, aggregations, or joining multiple tables before sending results to clients.

Views must be declared as `public` and accept only a context parameter. They can return either a single row or multiple rows. Like tables, views can be subscribed to and automatically update when their underlying data changes.

<Tabs groupId="syntax" queryString>
<TabItem value="typescript" label="TypeScript">

A view can be written in a TypeScript module like so:

```typescript
spacetimedb.view(
  { name: 'my_player', public: true },
  t.option(players.row()),
  (ctx) => {
    const row = ctx.db.players.identity.find(ctx.sender);
    return row ?? null;
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

A view can be written in C# like so:

```csharp
[SpacetimeDB.View(Name = "MyPlayer", Public = true)]
public static Player? MyPlayer(ViewContext ctx)
{
    return ctx.Db.Player.Identity.Find(ctx.Sender) as Player;
}
```

</TabItem>
<TabItem value="rust" label="Rust">

A view can be written in Rust like so:

```rust
#[spacetimedb::view(name = my_player, public)]
fn my_player(ctx: &spacetimedb::ViewContext) -> Option<Player> {
    ctx.db.player().identity().find(ctx.sender)
}
```

</TabItem>
</Tabs>

Views can be queried and subscribed to using SQL:

```sql
SELECT * FROM my_player;
```

See [Views](/functions/views) for more details about views.

## Client

A **client** is an application that connects to a [database](#database). A client logs in using an [identity](#identity) and receives an [connection id](#connectionid) to identify the connection. After that, it can call [reducers](#reducer) and query public [tables](#table).

Clients are written using the [client-side SDKs](/sdks). The `spacetime` CLI tool allows automatically generating code that works with the client-side SDKs to talk to a particular database.

Clients are regular software applications that developers can choose how to deploy (through Steam, app stores, package managers, or any other software deployment method, depending on the needs of the application.)

## Identity

A SpacetimeDB `Identity` identifies someone interacting with a database. It is a long lived, public, globally valid identifier that will always refer to the same end user, even across different connections.

A user's `Identity` is attached to every [reducer call](#reducer) they make, and you can use this to decide what they are allowed to do.

Modules themselves also have Identities. When you `spacetime publish` a module, it will automatically be issued an `Identity` to distinguish it from other modules. Your client application will need to provide this `Identity` when connecting to the [host](#host).

Identities are issued using the [OpenID Connect](https://openid.net/developers/how-connect-works/) specification. Database developers are responsible for issuing Identities to their end users. OpenID Connect lets users log in to these accounts through standard services like Google and Facebook.

Specifically, an identity is derived from the issuer and subject fields of a [JSON Web Token (JWT)](https://jwt.io/) hashed together. The psuedocode for this is as follows:

```python
def identity_from_claims(issuer: str, subject: str) -> [u8; 32]:
   hash1: [u8; 32] = blake3_hash(issuer + "|" + subject)
   id_hash: [u8; 26] = hash1[:26]
   checksum_hash: [u8; 32] = blake3_hash([
      0xC2,
      0x00,
      *id_hash
   ])
   identity_big_endian_bytes: [u8; 32] = [
      0xC2,
      0x00,
      *checksum_hash[:4],
      *id_hash
   ]
   return identity_big_endian_bytes
```

You can obtain a JWT from our turnkey identity provider [SpacetimeAuth](/spacetimeauth), or you can get one from any OpenID Connect compliant identity provider.

## ConnectionId

A `ConnectionId` identifies client connections to a SpacetimeDB database.

A user has a single [`Identity`](#identity), but may open multiple connections to your database. Each of these will receive a unique `ConnectionId`.

## Energy

**Energy** is the currency used to pay for data storage and compute operations in a SpacetimeDB host.

<!-- TODO(1.0): Rewrite this section after finalizing energy SKUs. -->

