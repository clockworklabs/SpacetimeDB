# SpacetimeDB Documentation

## Installation

You can run SpacetimeDB as a standalone database server via the `spacetime` CLI tool.

You can find the instructions to install the CLI tool for your platform [here](/install).

<button to="/install">Click here to install</button>

To get started running your own standalone instance of SpacetimeDB check out our [Getting Started Guide](/docs/getting-started).

<button to="/docs/getting-started">Getting Started</button>

## What is SpacetimeDB?

You can think of SpacetimeDB as a database that is also a server.

It is a relational database system that lets you upload your application logic directly into the database by way of very fancy stored procedures called "modules".

Instead of deploying a web or game server that sits in between your clients and your database, your clients connect directly to the database and execute your application logic inside the database itself. You can write all of your permission and authorization logic right inside your module just as you would in a normal server.

This means that you can write your entire application in a single language, Rust, and deploy it as a single binary. No more microservices, no more containers, no more Kubernetes, no more Docker, no more VMs, no more DevOps, no more infrastructure, no more ops, no more servers.

<figure>
    <img src="/images/basic-architecture-diagram.png" alt="SpacetimeDB Architecture" style="width:100%">
    <figcaption style="margin-top: -55px;" align="center">
        <b align="center">SpacetimeDB application architecture</b>
        <span style="font-size: 14px">(elements in white are provided by SpacetimeDB)</span>
    </figcaption>
</figure>

It's actually similar to the idea of smart contracts, except that SpacetimeDB is a database, has nothing to do with blockchain, and it's a lot faster than any smart contract system.

So fast, in fact, that the entire backend our MMORPG [BitCraft Online](https://bitcraftonline.com) is just a SpacetimeDB module. We don't have any other servers or services running, which means that everything in the game, all of the chat messages, items, resources, terrain, and even the locations of the players are stored and processed by the database before being synchronized out to all of the clients in real-time.

SpacetimeDB is optimized for maximum speed and minimum latency rather than batch processing or OLAP workloads. It is designed to be used for real-time applications like games, chat, and collaboration tools.

This speed and latency is achieved by holding all of application state in memory, while persisting the data in a write-ahead-log (WAL) which is used to recover application state.

## State Synchronization

SpacetimeDB syncs client and server state for you so that you can just write your application as though you're accessing the database locally. No more messing with sockets for a week before actually writing your game.

## Identities

An important concept in SpacetimeDB is that of an `Identity`. An `Identity` represents who someone is. It is a unique identifier that is used to authenticate and authorize access to the database. Importantly, while it represents who someone is, does NOT represent what they can do. Your application's logic will determine what a given identity is able to do by allowing or disallowing a transaction based on the `Identity`.

SpacetimeDB associates each client with a 256-bit (32-byte) integer `Identity`. These identities are usually formatted as 64-digit hexadecimal strings. Identities are public information, and applications can use them to identify users. Identities are a global resource, so a user can use the same identity with multiple applications, so long as they're hosted by the same SpacetimeDB instance.

Each identity has a corresponding authentication token. The authentication token is private, and should never be shared with anyone. Specifically, authentication tokens are [JSON Web Tokens](https://datatracker.ietf.org/doc/html/rfc7519) signed by a secret unique to the SpacetimeDB instance.

Additionally, each database has an owner `Identity`. Many database maintenance operations, like publishing a new version or evaluating arbitrary SQL queries, are restricted to only authenticated connections by the owner.

SpacetimeDB provides tools in the CLI and the [client SDKs](/docs/client-languages/client-sdk-overview) for managing credentials.

## Language Support

### Server-side Libraries

Currently, Rust is the best-supported language for writing SpacetimeDB modules. Support for lots of other languages is in the works!

- [Rust](/docs/server-languages/rust/rust-module-reference) - [(Quickstart)](/docs/server-languages/rust/rust-module-quickstart-guide)
- [C#](/docs/server-languages/csharp/csharp-module-reference) - [(Quickstart)](/docs/server-languages/csharp/csharp-module-quickstart-guide)
- Python (Coming soon)
- C# (Coming soon)
- Typescript (Coming soon)
- C++ (Planned)
- Lua (Planned)

### Client-side SDKs

- [Rust](/docs/client-languages/rust/rust-sdk-reference) - [(Quickstart)](/docs/client-languages/rust/rust-sdk-quickstart-guide)
- [C#](/docs/client-languages/csharp/csharp-sdk-reference) - [(Quickstart)](/docs/client-languages/csharp/csharp-sdk-quickstart-guide)
- [TypeScript](/docs/client-languages/typescript/typescript-sdk-reference) - [(Quickstart)](client-languages/typescript/typescript-sdk-quickstart-guide)
- [Python](/docs/client-languages/python/python-sdk-reference) - [(Quickstart)](/docs/python/python-sdk-quickstart-guide)
- C++ (Planned)
- Lua (Planned)

### Unity

SpacetimeDB was designed first and foremost as the backend for multiplayer Unity games. To learn more about using SpacetimeDB with Unity, jump on over to the [SpacetimeDB Unity Tutorial](/docs/unity-tutorial/unity-tutorial-part-1).

## FAQ

1. What is SpacetimeDB?
   It's a whole cloud platform within a database that's fast enough to run real-time games.

1. How do I use SpacetimeDB?
   Install the `spacetime` command line tool, choose your favorite language, import the SpacetimeDB library, write your application, compile it to WebAssembly, and upload it to the SpacetimeDB cloud platform. Once it's uploaded you can call functions directly on your application and subscribe to changes in application state.

1. How do I get/install SpacetimeDB?
   Just install our command line tool and then upload your application to the cloud.

1. How do I create a new database with SpacetimeDB?
   Follow our [Quick Start](/docs/quick-start) guide!

TL;DR in an empty directory:

```bash
spacetime init --lang=rust
spacetime publish
```

5. How do I create a Unity game with SpacetimeDB?
   Follow our [Unity Project](/docs/unity-project) guide!

TL;DR in an empty directory:

```bash
spacetime init --lang=rust
spacetime publish
spacetime generate --out-dir <path-to-unity-project> --lang=csharp
```
