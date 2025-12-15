---
title: Launchpad
slug: /
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";
import { QuickstartLinks } from "@site/src/components/QuickstartLinks";

# Getting Started with SpacetimeDB

## Installation

You can get started by first installing the `spacetime` CLI tool. The `spacetime` CLI tool makes it extremely easy to manage your databases and deployments.

<InstallCardLink />

## Running SpacetimeDB Locally

To develop SpacetimeDB databases locally, you will need to run the Standalone version of the server.

After installing the SpacetimeDB CLI, run the start command:

```bash
spacetime start
```

The server listens on port `3000` by default, customized via `--listen-addr`.

💡 Standalone mode will run in the foreground.
⚠️ SSL is not supported in standalone mode.

## Log in to SpacetimeDB

SpacetimeDB authenticates users using a GitHub login, to prevent unauthorized access (e.g. somebody else publishing over your module). Log in to SpacetimeDB using:

```bash
spacetime login
```

This will open a browser and ask you to log in via GitHub. If you forget this step, any commands that require login (like `spacetime publish`) will ask you to log in when you run them.

## Quickstart Guides

You are now ready to start developing SpacetimeDB modules. Choose your favorite language and follow one of our quickstart guides to get started building your first app with SpacetimeDB.

<QuickstartLinks />

### Server (Module)

- [Rust](/docs/quickstarts/rust)
- [C#](/docs/quickstarts/c-sharp)
- [TypeScript](/docs/quickstarts/typescript)

⚡**Note:** Rust is [roughly 2x faster](https://faun.dev/c/links/faun/c-vs-rust-vs-go-a-performance-benchmarking-in-kubernetes/) than C#

### Client

- [Rust](/docs/quickstarts/rust)
- [C# (Standalone)](/docs/quickstarts/c-sharp)
- [C# (Unity)](/docs/tutorials/unity/part-1)
- [Typescript](/docs/quickstarts/typescript)
