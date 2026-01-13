---
title: The Database Module
slug: /databases
---


import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

A **module** is a collection of functions and schema definitions, which can be written in TypeScript, C# or Rust. Modules define the structure of your database and the server-side logic that processes and handles client requests.

A **database** is a running instance of a module. While a module is the code you write (schema and reducers), a database is the actual deployed entity running on a SpacetimeDB **host** with stored data and active connections.

## Module vs Database

Understanding this distinction is important:

- A **module** is the code you write; it defines your schema (tables) and business logic (reducers, procedures, and views). Modules are compiled and deployed to SpacetimeDB. Rust and C# modules compile to WebAssembly, while TypeScript modules run on V8.
- A **database** is a *running instance* of a module; it has the module's schema and logic, plus actual stored data.

You can deploy the same module to multiple databases (e.g. separate environments for testing, staging, production), each with its own independent data. When you update your module code and re-publish, SpacetimeDB will update the database's schema/logic — the existing data remains (though for complicated schema changes you may need to handle migrations carefully).

## What's in a Module?

A module contains:

- **[Tables](/tables)** - Define your data structure and storage.
- **[Reducers](/functions/reducers)** - Server-side functions that modify your data transactionally.
- **[Procedures](/functions/procedures)** - Functions that can perform external operations like HTTP requests and return results.
- **[Views](/functions/views)** - Read-only computed queries over your data.

The logic is contained within these three categories of server-side functions: reducers (transactional state changes), procedures (functions with external capabilities), and views (read-only queries).

## Supported Languages

SpacetimeDB modules can be written in multiple languages:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

TypeScript is fully supported for server modules. TypeScript is ideal for developers familiar with JavaScript/Node.js.

- [TypeScript Quickstart Guide](/quickstarts/typescript)

</TabItem>
<TabItem value="csharp" label="C#">

C# is fully supported for server modules. C# is an excellent choice for developers using Unity or .NET.

- [C# Quickstart Guide](/quickstarts/c-sharp)

</TabItem>
<TabItem value="rust" label="Rust">

Rust is fully supported for server modules. Rust is a great choice for performance-critical applications.

- The Rust Module SDK docs are [hosted on docs.rs](https://docs.rs/spacetimedb/latest/spacetimedb/).
- [Rust Quickstart Guide](/quickstarts/rust)

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

See [`spacetime publish`](/databases/building-publishing) for details on the publishing workflow.

When you republish to an existing database, SpacetimeDB attempts to automatically migrate the schema. For details on what changes are supported and migration strategies:

- [Automatic Migrations](/databases/automatic-migrations) - Learn which schema changes are safe, breaking, or forbidden.
- [Incremental Migrations](/databases/incremental-migrations) - Advanced pattern for complex schema changes.

For all available publish options, see the [`spacetime publish` CLI reference](/cli-reference#spacetime-publish).

### Deleting a Database

To permanently delete a database and all its data:

```bash
spacetime delete <DATABASE_NAME>
```

You'll be prompted to confirm the deletion. Use `--yes` to skip the confirmation in scripts.

:::warning
Deleting a database is permanent and cannot be undone. All data will be lost.
:::

For more options, see the [`spacetime delete` CLI reference](/cli-reference#spacetime-delete).

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

For more SQL options, see the [`spacetime sql` CLI reference](/cli-reference#spacetime-sql).

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

For more logging options, see the [`spacetime logs` CLI reference](/cli-reference#spacetime-logs).

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

1. **[Create Your First Database Module](/databases/developing)** - Set up a new module project with `spacetime init` or `spacetime dev`
2. **[Build and Publish](/databases/building-publishing)** - Learn how to compile and deploy your module
3. **[Define Tables](/tables)** - Structure your data with tables, columns, and indexes
4. **[Write Reducers](/functions/reducers)** - Create transactional functions that modify your database
5. **[Connect a Client](/sdks)** - Build a client application that connects to your database

### Core Concepts

Once you have the basics down, explore these essential topics:

- **[Error Handling](/functions/reducers/error-handling)** - Handle errors gracefully in reducers
- **[Lifecycle Reducers](/functions/reducers/lifecycle)** - Respond to system events like initialization and client connections
- **[Automatic Migrations](/databases/automatic-migrations)** - Understand how schema changes work
- **[Logging](/how-to/logging)** - Debug and monitor your module with logging

### Advanced Features

Ready to level up? Dive into these advanced capabilities:

- **[Procedures](/functions/procedures)** - Make HTTP requests and interact with external services
- **[Views](/functions/views)** - Create computed, subscribable queries
- **[Scheduled Tables](/tables/scheduled-tables)** - Schedule reducers to run at specific times
- **[Incremental Migrations](/databases/incremental-migrations)** - Handle complex schema changes
- **[SQL Queries](/reference/sql)** - Query your database with SQL

### Deployment

When you're ready to go live:

- **[Deploy to MainCloud](/how-to/deploy/maincloud)** - Host your database on SpacetimeDB's managed service
- **[Self-Hosting](/how-to/deploy/self-hosting)** - Run your own SpacetimeDB instance

## Next Steps

- Learn about [Tables](/tables) to define your database schema
- Create [Reducers](/functions/reducers) to modify database state
- Understand [Subscriptions](/subscriptions) for real-time data sync
- Review the [CLI Reference](/cli-reference) for all available commands
