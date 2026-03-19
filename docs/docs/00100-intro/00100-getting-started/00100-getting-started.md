---
title: Getting Started
slug: /
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";
import { QuickstartLinks } from "@site/src/components/QuickstartLinks";


## Installation

You can get started by first installing the `spacetime` CLI tool. The `spacetime` CLI tool makes it extremely easy to manage your databases and deployments.

<InstallCardLink />

## Log in to SpacetimeDB

SpacetimeDB authenticates users using a GitHub login, to prevent unauthorized access (e.g. somebody else publishing over your module). Log in to SpacetimeDB using:

```bash
spacetime login
```

This will open a browser and ask you to log in via GitHub. If you forget this step, any commands that require login (like `spacetime publish`) will ask you to log in when you run them.

## Quickstart Guides

You are now ready to start developing SpacetimeDB modules. Choose your favorite language and follow one of our quickstart guides to get started building your first app with SpacetimeDB.

<QuickstartLinks />

## Running SpacetimeDB Locally

To develop SpacetimeDB databases locally, you will need to run the Standalone version of the server.

After installing the SpacetimeDB CLI, run the start command:

```bash
spacetime start
```

The server listens on port `3000` by default, customized via `--listen-addr`.

💡 Standalone mode will run in the foreground.
⚠️ SSL is not supported in standalone mode.

## Next Steps: Learn SpacetimeDB

After completing a quickstart guide, explore these core concepts to deepen your understanding:

### Core Concepts

- **[Databases](../../00200-core-concepts/00100-databases.md)** - Understand database lifecycle, publishing, and management
- **[Tables](../../00200-core-concepts/00300-tables.md)** - Define your data structure with tables, columns, and indexes
- **[Functions](../../00200-core-concepts/00200-functions.md)** - Write reducers, procedures, and views to implement your server logic
- **[Subscriptions](../../00200-core-concepts/00400-subscriptions.md)** - Enable real-time data synchronization with clients
- **[Client SDKs](../../00200-core-concepts/00600-clients.md)** - Connect your client applications to SpacetimeDB
