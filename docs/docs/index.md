# SpacetimeDB Documentation

## Installation

You can run SpacetimeDB as a standalone database server via the `spacetime` CLI tool.

You can find the instructions to install the CLI tool for your platform [here](/install).

<button to="/install">Click here to install</button>

To get started running your own standalone instance of SpacetimeDB check out our [Getting Started Guide](/docs/getting-started).

<button to="/docs/getting-started">Getting Started</button>

## What is SpacetimeDB?

SpacetimeDB is a database that is also a server.

SpacetimeDB is a full-featured relational database system that lets you run your application logic **inside** the database. You no longer need to deploy a separate web or game server. [Several programming languages](#module-libraries) are supported, including C# and Rust. You can still write authorization logic, just like you would in a traditional server.

This means that you can write your entire application in a single language and deploy it as a single binary. No more microservices, no more containers, no more Kubernetes, no more Docker, no more VMs, no more DevOps, no more infrastructure, no more ops, no more servers. An application deployed this way is called a **module**.

<figure>
    <img src="/images/basic-architecture-diagram.png" alt="SpacetimeDB Architecture" style="width:100%">
    <figcaption style="margin-top: 10px;" align="center">
        <b align="center">SpacetimeDB application architecture</b>
        <span style="font-size: 14px">(elements in white are provided by SpacetimeDB)</span>
    </figcaption>
</figure>

This is similar to ["smart contracts"](https://en.wikipedia.org/wiki/Smart_contract), except that SpacetimeDB is a **database** and has nothing to do with blockchain. Because it isn't a blockchain, it can be dramatically faster than many "smart contract" systems.

In fact, it's so fast that we've been able to write the entire backend of our MMORPG [BitCraft Online](https://bitcraftonline.com) as a Spacetime module. Everything in the game -- chat messages, items, resources, terrain, and player locations -- is stored and processed by the database. SpacetimeDB [automatically mirrors](#state-mirroring) relevant state to connected players in real-time.

SpacetimeDB is optimized for maximum speed and minimum latency, rather than batch processing or analytical workloads. It is designed for real-time applications like games, chat, and collaboration tools.

Speed and latency is achieved by holding all of your application state in memory, while persisting data to a commit log which is used to recover data after restarts and system crashes.

## State Mirroring

SpacetimeDB can generate client code in a [variety of languages](#client-side-sdks). This creates a client library custom-designed to talk to your module. It provides easy-to-use interfaces for connecting to a module and submitting requests. It can also **automatically mirror state** from your module's database.

You write SQL queries specifying what information a client is interested in -- for instance, the terrain and items near a player's avatar. SpacetimeDB will generate types in your client language for the relevant tables, and feed your client live updates whenever the database state changes. Note that this is a **read-only** mirror -- the only way to change the database is to submit requests, which are validated on the server.

## Language Support

### Module Libraries

SpacetimeDB modules are server-side applications that are deployed using the `spacetime` CLI tool.

- [Rust](/docs/modules/rust) - [(Quickstart)](/docs/modules/rust/quickstart)
- [C#](/docs/modules/c-sharp) - [(Quickstart)](/docs/modules/c-sharp/quickstart)

### Client-side SDKs

SpacetimeDB clients are applications that connect to SpacetimeDB modules. The `spacetime` CLI tool supports automatically generating interface code that makes it easy to interact with a particular module.

- [Rust](/docs/sdks/rust) - [(Quickstart)](/docs/sdks/rust/quickstart)
- [C#](/docs/sdks/c-sharp) - [(Quickstart)](/docs/sdks/c-sharp/quickstart)
- [TypeScript](/docs/sdks/typescript) - [(Quickstart)](/docs/sdks/typescript/quickstart)

### Unity

SpacetimeDB was designed first and foremost as the backend for multiplayer Unity games. To learn more about using SpacetimeDB with Unity, jump on over to the [SpacetimeDB Unity Tutorial](/docs/unity/part-1).

## Key architectural concepts

### Host
A SpacetimeDB **host** is a combination of a database and server that runs [modules](#module). You can run your own SpacetimeDB host, or use the SpacetimeDB maincloud.

### Module
A SpacetimeDB **module** is an application that runs on a [host](#host).

A module exports [tables](#table), which store data, and [reducers](#reducer), which allow [clients](#client) to make requests.

Technically, a SpacetimeDB module is a [WebAssembly module](https://developer.mozilla.org/en-US/docs/WebAssembly) that imports a specific low-level [WebAssembly ABI](/docs/webassembly-abi) and exports a small number of special functions. However, the SpacetimeDB [server-side libraries](#module-libraries) hide these low-level details. As a developer, writing a module is mostly like writing any other C# or Rust application, except for the fact that a [special CLI tool](/install) is used to build and deploy the application.

### Table
A SpacetimeDB **table** is a database table. Tables are declared in a module's native language. For instance, in Rust, a table is declared like so:

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
<!-- TODO: switchable language widget.
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
-->

The contents of a table can be read and updated by [reducers](#reducer).
Tables marked `public` can also be read by [clients](#client).

### Reducer
A **reducer** is a function exported by a [module](#module).
Connected [clients](#client-side-sdks) can call reducers to interact with the module.
This is a form of [remote procedure call](https://en.wikipedia.org/wiki/Remote_procedure_call).
Reducers can be invoked across languages. For example, a Rust [module](#module) can export a reducer like so:

```csharp
[SpacetimeDB.Reducer]
public static void SetPlayerName(ReducerContext ctx, uint playerId, string name)
{
    // ...
}
```
<!-- TODO: switchable language widget.
```rust
#[spacetimedb::reducer]
pub fn set_player_name(ctx: &spacetimedb::ReducerContext, id: u64, name: String) -> Result<(), String> {
   // ...
}
```
-->

And a C# [client](#client) can call that reducer:

```cs
void Main() {
   // ...setup code, then...
   Connection.Reducer.SetPlayerName(57, "Marceline");
}
```

These look mostly like regular function calls, but under the hood, the client sends a request over the internet, which the module processes and responds to.

The `ReducerContext` passed into a reducer includes information about the caller's [identity](#identity) and [address](#address).
It also allows accessing the database and scheduling future operations.

### Client
A **client** is an application that connects to a [module](#module). A client logs in using an [identity](#identity) and receives an [address](#address) to identify the connection. After that, it can call [reducers](#reducer) and query public [tables](#table).

Clients are written using the [client-side SDKs](#client-side-sdks). The `spacetime` CLI tool allows automatically generating code that works with the client-side SDKs to talk to a particular module.

Clients are regular software applications that module developers can choose how to deploy (through Steam, app stores, package managers, or any other software deployment method, depending on the needs of the application.)

### Identity

A SpacetimeDB `Identity` identifies someone interacting with a module. It is a long lived, public, globally valid identifier that will always refer to the same end user, even across different connections.

A user's `Identity` is attached to every [reducer call](#reducer) they make, and you can use this to decide what they are allowed to do.

Modules themselves also have Identities. When you `spacetime publish` a module, it will automatically be issued an `Identity` to distinguish it from other modules. Your client application will need to provide this `Identity` when connecting to the [host](#host).

Identities are issued using the [OpenID Connect](https://openid.net/developers/how-connect-works/) specification. Typically, module authors are responsible for issuing Identities to their end users. OpenID Connect makes it easy to allow users to authenticate to these accounts through standard services like Google and Facebook. (The idea is that you issue user accounts -- `Identities` -- but it's easy to let users log in to those accounts through Google or Facebook.)

<!-- TODO(1.0): link to a page on setting up your own identity provider and/or using our turnkey solution. -->

### Address

<!-- TODO(1.0): Rewrite this section after reworking `Address`es into `ConnectionID`s. -->

An `Address` identifies client connections to a SpacetimeDB module.

A user has a single [`Identity`](#identity), but may open multiple connections to your module. Each of these will receive a unique `Address`.

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
   Follow our [Quick Start](/docs/getting-started) guide!

5. How do I create a Unity game with SpacetimeDB?
   Follow our [Unity Project](/docs/unity-tutorial) guide!
