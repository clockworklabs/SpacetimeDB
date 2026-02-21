---
title: The Database Module
slug: /databases
---

import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

A **module** is a collection of functions and schema definitions, which can be written in TypeScript, C#, Rust, or C++. Modules define the structure of your database and the server-side logic that processes and handles client requests.

A **database** is a running instance of a module. While a module is the code you write (schema and reducers), a database is the actual deployed entity running on a SpacetimeDB **host** with stored data and active connections.

## Module vs Database

Understanding this distinction is important:

- A **module** is the code you write; it defines your schema (tables) and business logic (reducers, procedures, and views). Modules are compiled and deployed to SpacetimeDB. Rust, C#, and C++ modules compile to WebAssembly, while TypeScript modules run on V8.
- A **database** is a *running instance* of a module; it has the module's schema and logic, plus actual stored data.

You can deploy the same module to multiple databases (e.g. separate environments for testing, staging, production), each with its own independent data. When you update your module code and re-publish, SpacetimeDB will update the database's schema/logic — the existing data remains (though for complicated schema changes you may need to handle migrations carefully).

## What's in a Module?

A module contains:

- **[Tables](./00300-tables.md)** - Define your data structure and storage.
- **[Reducers](./00200-functions/00300-reducers/00300-reducers.md)** - Server-side functions that modify your data transactionally.
- **[Procedures](./00200-functions/00400-procedures.md)** - Functions that can perform external operations like HTTP requests and return results.
- **[Views](./00200-functions/00500-views.md)** - Read-only computed queries over your data.

The logic is contained within these three categories of server-side functions: reducers (transactional state changes), procedures (functions with external capabilities), and views (read-only queries).

## Supported Languages

SpacetimeDB modules can be written in multiple languages:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

TypeScript is fully supported for server modules. TypeScript is ideal for developers familiar with JavaScript/Node.js.

- [TypeScript Quickstart Guide](../00100-intro/00200-quickstarts/00400-typescript.md)

</TabItem>
<TabItem value="csharp" label="C#">

C# is fully supported for server modules. C# is an excellent choice for developers using Unity or .NET.

- [C# Quickstart Guide](../00100-intro/00200-quickstarts/00600-c-sharp.md)

</TabItem>
<TabItem value="rust" label="Rust">

Rust is fully supported for server modules. Rust is a great choice for performance-critical applications.

- The Rust Module SDK docs are [hosted on docs.rs](https://docs.rs/spacetimedb/latest/spacetimedb/).
- [Rust Quickstart Guide](../00100-intro/00200-quickstarts/00500-rust.md)

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

C++ is fully supported for server modules. C++ is an excellent choice for developers working with Unreal Engine or those who prefer to stay in the C++ ecosystem.

- [C++ Quickstart Guide](../00100-intro/00200-quickstarts/00700-cpp.md)

</TabItem>
</Tabs>

## Database Names

When you publish a module, you give the database a name. Database names must match the regex `/^[a-z0-9]+(-[a-z0-9]+)*$/`, i.e. only lowercase ASCII letters and numbers, separated by dashes.

**Examples of valid names:**

- `my-game-server`
- `chat-app-production`
- `test123`

Each database also receives a unique **identity** (a hex string) when created. Clients can connect using either the name or identity.

## Managing Databases

Modules and databases are administered using the `spacetime` CLI tool.

### Creating and Updating Databases

Create or update a database by publishing your module:

```bash
spacetime publish <DATABASE_NAME>
```

See [`spacetime publish`](./00100-databases/00300-spacetime-publish.md) for details on the publishing workflow.

When you republish to an existing database, SpacetimeDB attempts to automatically migrate the schema. For details on what changes are supported and migration strategies:

- [1.x to 2.0 Upgrade Notes](./00100-databases/00500-migrations/00100-upgrade-notes-2-0.md) - Required reading before major-version upgrades.
- [Automatic Migrations](./00100-databases/00500-migrations/00200-automatic-migrations.md) - Learn which schema changes are safe, breaking, or forbidden.
- [Incremental Migrations](./00100-databases/00500-migrations/00300-incremental-migrations.md) - Advanced pattern for complex schema changes.

For all available publish options, see the [`spacetime publish` CLI reference](../00300-resources/00200-reference/00100-cli-reference/00100-cli-reference.md#spacetime-publish).

### Deleting a Database

To permanently delete a database and all its data:

```bash
spacetime delete <DATABASE_NAME>
```

You'll be prompted to confirm the deletion. Use `--yes` to skip the confirmation in scripts.

:::warning
Deleting a database is permanent and cannot be undone. All data will be lost.
:::

For more options, see the [`spacetime delete` CLI reference](../00300-resources/00200-reference/00100-cli-reference/00100-cli-reference.md#spacetime-delete).

### Querying with SQL

You can run SQL queries directly against your database:

```bash
spacetime sql <DATABASE_NAME> "SELECT * FROM user"
```

#### Owner Privileges

**Important:** When you run SQL queries as the database owner, you bypass table visibility restrictions. This means you can query private tables that normal clients cannot access.

To test queries as an unprivileged client would see them, use the `--anonymous` flag:

```bash
spacetime sql --anonymous <DATABASE_NAME> "SELECT * FROM user"
```

This executes the query as an anonymous client, respecting table visibility rules.

For more SQL options, see the [`spacetime sql` CLI reference](../00300-resources/00200-reference/00100-cli-reference/00100-cli-reference.md#spacetime-sql).

### Viewing Logs

View logs from your database:

```bash
spacetime logs <DATABASE_NAME>
```

#### Following Logs in Real-Time

To stream logs as they're generated (similar to `tail -f`):

`spacetime logs --follow <DATABASE_NAME>`

This keeps the connection open and displays new log entries as they occur. Press Ctrl+C to stop following.

#### Limiting Log Output

To view only the last N lines:

```bash
spacetime logs --num-lines 100 <DATABASE_NAME>
```

For more logging options, see the [`spacetime logs` CLI reference](../00300-resources/00200-reference/00100-cli-reference/00100-cli-reference.md#spacetime-logs).

### Listing Your Databases

To see all databases associated with your identity:

```bash
spacetime list
```

This shows database names, identities, and host servers.

## Managing Databases via the Website

You can also manage your databases through the SpacetimeDB web interface at [spacetimedb.com](https://spacetimedb.com):

- **View metrics** - Monitor database performance, connection counts, and resource usage
- **Browse tables** - Inspect table schemas and data
- **View logs** - Access historical logs with filtering and search
- **Manage access** - Control database permissions and team access
- **Monitor queries** - See subscription queries and reducer calls

:::tip
The website provides a graphical interface for many CLI operations, making it easier to visualize and manage your databases.
:::

## Projects and Teams

SpacetimeDB supports organizing databases into projects and managing team access. This allows you to:

- Group related databases together
- Share access with team members
- Manage permissions at the project level

## Learning Path

### Getting Started

If you're new to SpacetimeDB, follow this recommended learning path:

1. **[Create Your First Database Module](./00100-databases/00200-spacetime-dev.md)** - Set up a new module project with `spacetime init` or `spacetime dev`
2. **[Build and Publish](./00100-databases/00300-spacetime-publish.md)** - Learn how to compile and deploy your module
3. **[Define Tables](./00300-tables.md)** - Structure your data with tables, columns, and indexes
4. **[Write Reducers](./00200-functions/00300-reducers/00300-reducers.md)** - Create transactional functions that modify your database
5. **[Connect a Client](./00600-clients.md)** - Build a client application that connects to your database

### Core Concepts

Once you have the basics down, explore these essential topics:

- **[Error Handling](./00200-functions/00300-reducers/00600-error-handling.md)** - Handle errors gracefully in reducers
- **[Lifecycle Reducers](./00200-functions/00300-reducers/00500-lifecycle.md)** - Respond to system events like initialization and client connections
- **[Automatic Migrations](./00100-databases/00500-migrations/00200-automatic-migrations.md)** - Understand how schema changes work
- **[Logging](../00300-resources/00100-how-to/00300-logging.md)** - Debug and monitor your module with logging

### Advanced Features

Ready to level up? Dive into these advanced capabilities:

- **[Procedures](./00200-functions/00400-procedures.md)** - Make HTTP requests and interact with external services
- **[Views](./00200-functions/00500-views.md)** - Create computed, subscribable queries
- **[Schedule Tables](./00300-tables/00500-schedule-tables.md)** - Schedule reducers to run at specific times
- **[Incremental Migrations](./00100-databases/00500-migrations/00300-incremental-migrations.md)** - Handle complex schema changes
- **[SQL Queries](../00300-resources/00200-reference/00400-sql-reference.md)** - Query your database with SQL

### Deployment

When you're ready to go live:

- **[Deploy to MainCloud](../00300-resources/00100-how-to/00100-deploy/00100-maincloud.md)** - Host your database on SpacetimeDB's managed service
- **[Self-Hosting](../00300-resources/00100-how-to/00100-deploy/00200-self-hosting.md)** - Run your own SpacetimeDB instance

## Next Steps

- Learn about [Tables](./00300-tables.md) to define your database schema
- Create [Reducers](./00200-functions/00300-reducers/00300-reducers.md) to modify database state
- Understand [Subscriptions](./00400-subscriptions.md) for real-time data sync
- Review the [CLI Reference](../00300-resources/00200-reference/00100-cli-reference/00100-cli-reference.md) for all available commands
