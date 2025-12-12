---
title: Overview
slug: /databases
---

# Databases

A **database** is a running instance of a [module](/new-modules). While a module is the code you write (schema and reducers), a database is the actual deployed entity running on a SpacetimeDB **host** with stored data and active connections.

## Database vs Module

Understanding this distinction is important:

- A **module** is your source code - it defines tables, reducers, and business logic
- A **database** is a running instance with the module's code plus actual data
- One module can be deployed to multiple databases (e.g., `dev`, `staging`, `production`)
- Each database has its own independent data and state

## Database Names

When you publish a module, you give the database a name. Database names must match the regex `/^[a-z0-9]+(-[a-z0-9]+)*$/`, i.e. only lowercase ASCII letters and numbers, separated by dashes.

**Examples of valid names:**

- `my-game-server`
- `chat-app-production`
- `test123`

Each database also receives a unique **identity** (a hex string) when created. Clients can connect using either the name or identity.

## Managing Databases

### Creating and Updating Databases

Create or update a database by publishing your module:

```bash
spacetime publish <DATABASE_NAME>
```

See [Building and Publishing](/new-modules/building-publishing) for details on the publishing workflow.

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

## Next Steps

- Learn about [Tables](/tables) to define your database schema
- Create [Reducers](/functions/reducers) to modify database state
- Understand [Subscriptions](/subscriptions) for real-time data sync
- Review the [CLI Reference](/cli-reference) for all available commands
