<p align="center">
    <a href="https://spacetimedb.com#gh-dark-mode-only" target="_blank">
	<img width="320" src="./images/dark/logo.svg" alt="SpacetimeDB Logo">
    </a>
    <a href="https://spacetimedb.com#gh-light-mode-only" target="_blank">
	<img width="320" src="./images/light/logo.svg" alt="SpacetimeDB Logo">
    </a>
</p>
<p align="center">
    <a href="https://spacetimedb.com#gh-dark-mode-only" target="_blank">
        <img width="250" src="./images/dark/logo-text.svg" alt="SpacetimeDB">
    </a>
    <a href="https://spacetimedb.com#gh-light-mode-only" target="_blank">
        <img width="250" src="./images/light/logo-text.svg" alt="SpacetimeDB">
    </a>
    <h3 align="center">
        Multiplayer at the speed of light.
    </h3>
</p>
<p align="center">
    <a href="https://github.com/clockworklabs/spacetimedb"><img src="https://img.shields.io/github/v/release/clockworklabs/spacetimedb?color=%23ff00a0&include_prereleases&label=version&sort=semver&style=flat-square"></a>
    &nbsp;
    <a href="https://github.com/clockworklabs/spacetimedb"><img src="https://img.shields.io/badge/built_with-Rust-dca282.svg?style=flat-square"></a>
    &nbsp;
	<a href="https://github.com/clockworklabs/spacetimedb/actions"><img src="https://img.shields.io/github/actions/workflow/status/clockworklabs/spacetimedb/ci.yml?style=flat-square&branch=master"></a>
    &nbsp;
    <a href="https://status.spacetimedb.com"><img src="https://img.shields.io/uptimerobot/ratio/7/m784409192-e472ca350bb615372ededed7?label=cloud%20uptime&style=flat-square"></a>
    &nbsp;
    <a href="https://hub.docker.com/r/clockworklabs/spacetimedb"><img src="https://img.shields.io/docker/pulls/clockworklabs/spacetimedb?style=flat-square"></a>
    &nbsp;
    <a href="https://github.com/clockworklabs/spacetimedb/blob/master/LICENSE.txt"><img src="https://img.shields.io/badge/license-BSL_1.1-00bfff.svg?style=flat-square"></a>
</p>
<p align="center">
    <a href="https://crates.io/crates/spacetimedb"><img src="https://img.shields.io/crates/d/spacetimedb?color=e45928&label=Rust%20Crate&style=flat-square"></a>
    &nbsp;
    <a href="https://www.nuget.org/packages/SpacetimeDB.Runtime"><img src="https://img.shields.io/nuget/dt/spacetimedb.runtime?color=0b6cff&label=NuGet%20Package&style=flat-square"></a>
</p>
<p align="center">
    <a href="https://discord.gg/spacetimedb"><img src="https://img.shields.io/discord/1037340874172014652?label=discord&style=flat-square&color=5a66f6"></a>
    &nbsp;
    <a href="https://twitter.com/spacetime_db"><img src="https://img.shields.io/badge/twitter-Follow_us-1d9bf0.svg?style=flat-square"></a>
    &nbsp;
    <a href="https://clockworklabs.io/join"><img src="https://img.shields.io/badge/careers-Join_us-86f7b7.svg?style=flat-square"></a>
    &nbsp;
    <a href="https://www.linkedin.com/company/clockworklabs/"><img src="https://img.shields.io/badge/linkedin-Connect_with_us-0a66c2.svg?style=flat-square"></a>
</p>

<p align="center">
    <a href="https://discord.gg/spacetimedb"><img height="25" src="./images/social/discord.svg" alt="Discord"></a>
    &nbsp;
    <a href="https://twitter.com/spacetime_db"><img height="25" src="./images/social/twitter.svg" alt="Twitter"></a>
    &nbsp;
    <a href="https://github.com/clockworklabs/spacetimedb"><img height="25" src="./images/social/github.svg" alt="Github"></a>
    &nbsp;
    <a href="https://twitch.tv/SpacetimeDB"><img height="25" src="./images/social/twitch.svg" alt="Twitch"></a>
    &nbsp;
    <a href="https://youtube.com/@SpacetimeDB"><img height="25" src="./images/social/youtube.svg" alt="YouTube"></a>
    &nbsp;
    <a href="https://www.linkedin.com/company/clockwork-labs/"><img height="25" src="./images/social/linkedin.svg" alt="LinkedIn"></a>
    &nbsp;
    <a href="https://stackoverflow.com/questions/tagged/spacetimedb"><img height="25" src="./images/social/stackoverflow.svg" alt="StackOverflow"></a>
</p>

<br>

## What is [SpacetimeDB](https://spacetimedb.com)?

You can think of SpacetimeDB as both a database and server combined into one.

It is a relational database system that lets you upload your application logic directly into the database by way of fancy stored procedures called "modules."

Instead of deploying a web or game server that sits in between your clients and your database, your clients connect directly to the database and execute your application logic inside the database itself. You can write all of your permission and authorization logic right inside your module just as you would in a normal server.

This means that you can write your entire application in a single language, Rust, and deploy it as a single binary. No more microservices, no more containers, no more Kubernetes, no more Docker, no more VMs, no more DevOps, no more infrastructure, no more ops, no more servers.

<figure>
    <img src="./images/basic-architecture-diagram.png" alt="SpacetimeDB Architecture" style="width:100%">
    <figcaption align="center">
        <p align="center"><b>SpacetimeDB application architecture</b><br /><sup><sub>(elements in white are provided by SpacetimeDB)</sub></sup></p>
    </figcaption>
</figure>

It's actually similar to the idea of smart contracts, except that SpacetimeDB is a database, has nothing to do with blockchain, and is orders of magnitude faster than any smart contract system.

So fast, in fact, that the entire backend of our MMORPG [BitCraft Online](https://bitcraftonline.com) is just a SpacetimeDB module. We don't have any other servers or services running, which means that everything in the game, all of the chat messages, items, resources, terrain, and even the locations of the players are stored and processed by the database before being synchronized out to all of the clients in real-time.

SpacetimeDB is optimized for maximum speed and minimum latency rather than batch processing or OLAP workloads. It is designed to be used for real-time applications like games, chat, and collaboration tools.

This speed and latency is achieved by holding all of application state in memory, while persisting the data in a write-ahead-log (WAL) which is used to recover application state.

## Installation

You can run SpacetimeDB as a standalone database server via the `spacetime` CLI tool.
Install instructions for supported platforms are outlined below.
The same install instructions can be found on our website at https://spacetimedb.com/install.

#### Install on macOS

Installing on macOS is as simple as running our install script. After that you can use the spacetime command to manage versions.

```bash
curl -sSf https://install.spacetimedb.com | sh
```

#### Install on Linux

Installing on Linux is as simple as running our install script. After that you can use the spacetime command to manage versions.

```bash
curl -sSf https://install.spacetimedb.com | sh
```

#### Install on Windows

Installing on Windows is as simple as pasting the above snippet into PowerShell. If you would like to use WSL instead, please follow the Linux install instructions.

```ps1
iwr https://windows.spacetimedb.com -useb | iex
```

#### Running with Docker

If you prefer to run Spacetime in a container, you can use the following command to start a new instance.

```bash
docker run --rm --pull always -p 3000:3000 clockworklabs/spacetime start
```

## Documentation

For more information about SpacetimeDB, getting started guides, game development guides, and reference material please see our [documentation](https://spacetimedb.com/docs).

## Getting Started

We've prepared several getting started guides in each of our supported languages to help you get up and running with SpacetimeDB as quickly as possible. You can find them on our [docs page](https://spacetimedb.com/docs).

In summary there are only 4 steps to getting started with SpacetimeDB.

1. Install the `spacetime` CLI tool.
2. Start a SpacetimeDB standalone node with `spacetime start`.
3. Write and upload a module in one of our supported module languages.
4. Connect to the database with one of our client libraries.

You can see a summary of the supported languages below with a link to the getting started guide for each.

## Language Support

You can write SpacetimeDB modules in several popular languages, with more to come in the future!

#### Serverside Libraries

- [Rust](https://spacetimedb.com/docs/modules/rust/quickstart)
- [C#](https://spacetimedb.com/docs/modules/c-sharp/quickstart)

#### Client Libraries

- [Rust](https://spacetimedb.com/docs/sdks/rust/quickstart)
- [C#](https://spacetimedb.com/docs/sdks/c-sharp/quickstart)
- [Typescript](https://spacetimedb.com/docs/sdks/typescript/quickstart)

## License

SpacetimeDB is licensed under the BSL 1.1 license. This is not an open source or free software license, however, it converts to the AGPL v3.0 license with a linking exception after a few years.

Note that the AGPL v3.0 does not typically include a linking exception. We have added a custom linking exception to the AGPL license for SpacetimeDB. Our motivation for choosing a free software license is to ensure that contributions made to SpacetimeDB are propagated back to the community. We are expressly not interested in forcing users of SpacetimeDB to open source their own code if they link with SpacetimeDB, so we needed to include a linking exception.
