---
title: Overview
slug: /unreal
---

# Unreal Tutorial - Overview

Need help with the tutorial or CLI commands? [Join our Discord server](https://discord.gg/spacetimedb)!

In this tutorial you'll learn how to build a small-scoped massive multiplayer online action game in Unreal, from scratch, using SpacetimeDB. Although, the game we're going to build is small in scope, it'll scale to hundreds of players and will help you get acquainted with all the features and best practices of SpacetimeDB, while building [a fun little game](https://github.com/ClockworkLabs/Blackholio).

By the end, you should have a basic understanding of what SpacetimeDB offers for developers making multiplayer games.

The game is inspired by [agar.io](https://agar.io), but SpacetimeDB themed with some fun twists. If you're not familiar [agar.io](https://agar.io), it's a web game in which you and hundreds of other players compete to cultivate mass to become the largest cell in the Petri dish.

Our game, called [Blackhol.io](https://github.com/clockworklabs/SpacetimeDB/tree/master/demo/Blackholio), will be similar but space themed. It should give you a great idea of the types of games you can develop easily with SpacetimeDB.

This tutorial assumes that you have a basic understanding of the Unreal Engine, using a command line terminal and programming in C++. We'll give you some CLI commands to execute. If you are using Windows, we recommend using Git Bash or PowerShell. For Mac, we recommend Terminal.

We’ll keep things intentionally simple: a single “Game Manager” class, minimal error handling, and hardcoded settings where convenient. This makes the SDK flow easy to see. For production, prefer Unreal’s Subsystems, move secrets out of code, follow best practices, and add proper logging/retry.

SpacetimeDB supports Unreal Engine version `5.6`. This tutorial has been tested only with that version.

This tutorial is written for C++, but the SpacetimeDB Unreal client SDK also supports Blueprints! Stay tuned for a Blueprint-based tutorial.

Please file an issue [here](https://github.com/clockworklabs/SpacetimeDB/issues) if you encounter an issue with a specific Unreal version, but please be aware that the SpacetimeDB team is unable to offer support for issues related to versions of Unreal prior to `5.6`.

## Blackhol.io Tutorial - Basic Multiplayer

First you'll get started with the core client/server setup. For part 2, you'll be able to choose between [Rust](/modules/rust) or [C#](/modules/c-sharp) for your server module language:

- [Part 1 - Setup](/unreal/part-1)
- [Part 2 - Connecting to SpacetimeDB](/unreal/part-2)
- [Part 3 - Gameplay](/unreal/part-3)
- [Part 4 - Moving and Colliding](/unreal/part-4)

## Blackhol.io Tutorial - Advanced

If you already have a good understanding of the SpacetimeDB client and server, check out our completed tutorial project!

[https://github.com/clockworklabs/SpacetimeDB/tree/master/demo/Blackholio](https://github.com/clockworklabs/SpacetimeDB/tree/master/demo/Blackholio)
