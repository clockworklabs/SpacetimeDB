---
title: Overview
slug: /
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# SpacetimeDB Documentation

## Installation

You can run SpacetimeDB as a standalone database server via the `spacetime` CLI tool.

You can find the instructions to install the CLI tool for your platform [here](https://spacetimedb.com/install).

To get started running your own standalone instance of SpacetimeDB check out our [Getting Started Guide](/getting-started).

## What is SpacetimeDB?

SpacetimeDB is a database that is also a server.

SpacetimeDB is a full-featured relational database system that lets you run your application logic **inside** the database. You no longer need to deploy a separate web or game server. [Several programming languages](#module-libraries) are supported, including C# and Rust. You can still write authorization logic, just like you would in a traditional server.

This means that you can write your entire application in a single language and deploy it as a single binary. No more microservices, no more containers, no more Kubernetes, no more Docker, no more VMs, no more DevOps, no more infrastructure, no more ops, no more servers.

<figure>
  <img
    src="/images/basic-architecture-diagram.png"
    alt="SpacetimeDB Architecture"
    style={{ width: '100%' }}
  />
  <figcaption style={{ marginTop: '10px', textAlign: 'center' }} align="center">
    <b align="center">SpacetimeDB application architecture</b>
    <span style={{ fontSize: '14px' }}>
      {' '}
      (elements in white are provided by SpacetimeDB)
    </span>
  </figcaption>
</figure>

This is similar to ["smart contracts"](https://en.wikipedia.org/wiki/Smart_contract), except that SpacetimeDB is a **database** and has nothing to do with blockchain. Because it isn't a blockchain, it can be dramatically faster than many "smart contract" systems.

In fact, it's so fast that we've been able to write the entire backend of our MMORPG [BitCraft Online](https://bitcraftonline.com) as a single SpacetimeDB database. Everything in the game -- chat messages, items, resources, terrain, and player locations -- is stored and processed by the database. SpacetimeDB [automatically mirrors](#state-mirroring) relevant state to connected players in real-time.

SpacetimeDB is optimized for maximum speed and minimum latency, rather than batch processing or analytical workloads. It is designed for real-time applications like games, chat, and collaboration tools.

Speed and latency is achieved by holding all of your application state in memory, while persisting data to a commit log which is used to recover data after restarts and system crashes.

## Application Workflow Preview

<figure>
  <img
    src="/images/workflow-preview-diagram.png"
    alt="SpacetimeDB Application Workflow Preview"
    style={{ width: '100%' }}
  />
  <figcaption style={{ marginTop: '10px', textAlign: 'center' }} align="center">
    <b align="center">SpacetimeDB Application Workflow Preview</b>
  </figcaption>
</figure>

The above illustrates the workflow when using SpacetimeDB.

- All client-side reads happen with the data view that is cached locally.

- Client-side subscriptions tell the server what data client cares about and wants to be synced within its data view. Changes to data will be pushed by the server to the client cache.

- RLS filters restrict the data view server-side before subscriptions are evaluated. These filters can be used for access control or client scoping.

- Reducers are effectively async RPC's. The request is sent off and if the results of that reducer makes changes to data, it will be written to the database directly. As a result of that, if those changes make it through the two layers above, then the client will see the result when it queries its local cache.

## State Mirroring

SpacetimeDB can generate client code in a [variety of languages](#client-side-sdks). This creates a client library custom-designed to talk to your database. It provides easy-to-use interfaces for connecting to the database and submitting requests. It can also **automatically mirror state** from your database to client applications.

You write SQL queries specifying what information a client is interested in -- for instance, the terrain and items near a player's avatar. SpacetimeDB will generate types in your client language for the relevant tables, and feed clients a stream of live updates whenever the database state changes. Note that this is a **read-only** mirror -- the only way to change the database is to submit requests, which are validated on the server.

## Language Support

### Module Libraries

Every SpacetimeDB database contains a collection of [stored procedures](https://en.wikipedia.org/wiki/Stored_procedure) and schema definitions. Such a collection is called a **module**, which can be written in C# or Rust. They specify a database schema and the business logic that responds to client requests. Modules are administered using the `spacetime` CLI tool.

- [Rust](/modules/rust) - [(Quickstart)](/modules/rust/quickstart)
- [C#](/modules/c-sharp) - [(Quickstart)](/modules/c-sharp/quickstart)

### Client-side SDKs

**Clients** are applications that connect to SpacetimeDB databases. The `spacetime` CLI tool supports automatically generating interface code that makes it easy to interact with a particular database.

- [Rust](/sdks/rust) - [(Quickstart)](/sdks/rust/quickstart)
- [C#](/sdks/c-sharp) - [(Quickstart)](/sdks/c-sharp/quickstart)
- [TypeScript](/sdks/typescript) - [(Quickstart)](/sdks/typescript/quickstart)

### Unity

SpacetimeDB was designed first and foremost as the backend for multiplayer Unity games. To learn more about using SpacetimeDB with Unity, jump on over to the [SpacetimeDB Unity Tutorial](/unity/part-1).

## Key architectural concepts

### Host

A SpacetimeDB **host** is a server that hosts [databases](#database). You can run your own host, or use the SpacetimeDB maincloud. Many databases can run on a single host.

### Database

A SpacetimeDB **database** is an application that runs on a [host](#host).

A database exports [tables](#table), which store data, and [reducers](#reducer), which allow [clients](#client) to make requests.

A database's schema and business logic is specified by a piece of software called a **module**. Modules can be written in C# or Rust.

(Technically, a SpacetimeDB module is a [WebAssembly module](https://developer.mozilla.org/en-US/docs/WebAssembly) that imports a specific low-level [WebAssembly ABI](/webassembly-abi) and exports a small number of special functions. However, the SpacetimeDB [server-side libraries](#module-libraries) hide these low-level details. As a developer, writing a module is mostly like writing any other C# or Rust application, except for the fact that a [special CLI tool](https://spacetimedb.com/install) is used to deploy the application.)

### Table

A SpacetimeDB **table** is a SQL database table. Tables are declared in a module's native language. For instance, in C#, a table is declared like so:

<Tabs groupId="syntax" queryString>

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

</Tabs>

The contents of a table can be read and updated by [reducers](#reducer).
Tables marked `public` can also be read by [clients](#client).

### Reducer

A **reducer** is a function exported by a [database](#database).
Connected [clients](#client-side-sdks) can call reducers to interact with the database.
This is a form of [remote procedure call](https://en.wikipedia.org/wiki/Remote_procedure_call).

<Tabs groupId="syntax" queryString>
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
a reducer can [schedule another reducer](/modules/c-sharp#scheduled-reducers) to run at an interval,
or at a specific time.

</TabItem>
</Tabs>

### Client

A **client** is an application that connects to a [database](#database). A client logs in using an [identity](#identity) and receives an [connection id](#connectionid) to identify the connection. After that, it can call [reducers](#reducer) and query public [tables](#table).

Clients are written using the [client-side SDKs](#client-side-sdks). The `spacetime` CLI tool allows automatically generating code that works with the client-side SDKs to talk to a particular database.

Clients are regular software applications that developers can choose how to deploy (through Steam, app stores, package managers, or any other software deployment method, depending on the needs of the application.)

### Identity

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

### ConnectionId

A `ConnectionId` identifies client connections to a SpacetimeDB database.

A user has a single [`Identity`](#identity), but may open multiple connections to your database. Each of these will receive a unique `ConnectionId`.

### Energy

**Energy** is the currency used to pay for data storage and compute operations in a SpacetimeDB host.

<!-- TODO(1.0): Rewrite this section after finalizing energy SKUs. -->

## FAQ

1. What is SpacetimeDB?
   It's a cloud platform within a database that's fast enough to run real-time games.

1. How do I use SpacetimeDB?
   Install the `spacetime` command line tool, choose your favorite language, import the SpacetimeDB library, write your module, compile it to WebAssembly, and upload it to the SpacetimeDB cloud platform. Once it's uploaded you can call functions directly on your application and subscribe to changes in application state.

1. How do I get/install SpacetimeDB?
   Just install our command line tool and then upload your application to the cloud.

1. How do I create a new database with SpacetimeDB?
   Follow our [Quick Start](/getting-started) guide!

1. How do I create a Unity game with SpacetimeDB?
   Follow our [Unity Tutorial](/unity) guide!
