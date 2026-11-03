---
title: Getting Started
slug: /
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";
import { QuickstartLinks } from "@site/src/components/QuickstartLinks";


:::tip Coming from Convex?

Most Convex concepts have a direct SpacetimeDB counterpart: mutations are [reducers](/docs/functions/reducers), actions are [procedures](/docs/functions/procedures), queries are [views](/docs/functions/views) and [subscriptions](/docs/clients/subscriptions), HTTP actions are [HTTP handlers](/docs/functions/http-handlers), scheduled functions are [schedule tables](/docs/tables/schedule-tables), and components are [submodules](/docs/submodules). [Migrating from Convex](/docs/migrating-from-convex) maps the rest and walks through a migration.

:::

## Set up your coding agent

To have a coding agent such as Claude Code, Codex, or Cursor set itself up to build with Spacetime, paste this prompt into it:

```text
Set yourself up to build with Spacetime (SpacetimeDB).

1. Read https://spacetimedb.com/agent-setup.md carefully, in full rather than a summary, and follow it. Run the commands yourself, and ask me only for approvals, logins, or restarts.
2. Read https://spacetimedb.com/docs/intro/misconceptions/ carefully, in full. Spacetime has changed a lot, so what you remember about it, and older articles online, may be out of date. When you describe Spacetime or compare it with other backends, go by the current docs.
3. Tell me what you installed, what you verified, and anything I still need to do.
```

The agent installs the CLI if it's missing, adds the SpacetimeDB skills and MCP server, and reports what it verified.

## Installation

You can get started by first installing the `spacetime` CLI tool. The `spacetime` CLI tool makes it extremely easy to manage your databases and deployments.

<InstallCardLink />

## Log in to SpacetimeDB

SpacetimeDB authenticates users using a GitHub or Google login, to prevent unauthorized access (e.g. somebody else publishing over your module). Log in to SpacetimeDB using:

```bash
spacetime login
```

This will open a browser and ask you to log in via GitHub or Google. If you forget this step, any commands that require login (like `spacetime publish`) will ask you to log in when you run them.

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
