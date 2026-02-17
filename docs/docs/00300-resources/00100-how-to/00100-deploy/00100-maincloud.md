---
title: Maincloud
slug: /how-to/deploy/maincloud
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

Maincloud is SpacetimeDB's fully managed serverless platform. It handles infrastructure, scaling, replication, and backups so you can focus on building your application. Maincloud scales to zero when your database is idle, so you only pay for what you use.

For pricing details, see the [pricing page](https://spacetimedb.com/pricing).

## Prerequisites

1. Install the SpacetimeDB CLI: [Install SpacetimeDB](https://spacetimedb.com/install)
2. Log in to link your CLI identity with your web account:

```bash
spacetime login
```

This opens a browser window where you sign in with your GitHub account. Once authenticated, your CLI identity is linked to your Maincloud account, and any databases you publish will appear on the web dashboard.

:::tip
If you previously published a database without logging in first, your CLI identity will not be linked to your web account. Run `spacetime logout` followed by `spacetime login` to re-authenticate.
:::

## Publishing to Maincloud

After creating your module (see [Getting Started](/)), publish it to Maincloud:

```bash
spacetime publish my-module --server maincloud
```

SpacetimeDB compiles your module, uploads it, runs your `init` reducer (if defined), and outputs the database identity. Save this identity for administrative tasks.

To update an existing module, run the same command. SpacetimeDB hot-swaps the module code without disconnecting clients. See [Automatic Migrations](/databases/automatic-migrations) for details on schema changes during updates.

To clear all data and start fresh:

```bash
spacetime publish my-module --server maincloud --delete-data
```

## Connecting Clients to Maincloud

To connect your client application to a module running on Maincloud, use `https://maincloud.spacetimedb.com` as the host URL and your database name as the module name:

<Tabs groupId="syntax" queryString>
<TabItem value="typescript" label="TypeScript">

```ts
DbConnection.builder()
  .withUri("https://maincloud.spacetimedb.com")
  .withModuleName("my-module")
  .build();
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
DbConnection.Builder()
    .WithUri("https://maincloud.spacetimedb.com")
    .WithModuleName("my-module")
    .Build();
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
DbConnection::builder()
    .with_uri("https://maincloud.spacetimedb.com")
    .with_module_name("my-module")
    .build()
    .expect("Failed to connect");
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
auto conn = DbConnection::builder()
    .with_uri("https://maincloud.spacetimedb.com")
    .with_module_name("my-module")
    .build();
```

</TabItem>
</Tabs>

## Viewing Your Database on the Web Dashboard

After publishing, you can manage your database through the web dashboard at [spacetimedb.com](https://spacetimedb.com).

### Finding your database

There are two ways to navigate to your database:

1. **Direct URL**: Go to `https://spacetimedb.com/my-module` (replacing `my-module` with your database name).
2. **Profile page**: Click your profile picture in the top-right corner of [spacetimedb.com](https://spacetimedb.com) and select "My profile". All of your published databases are listed there. You can also navigate directly to `https://spacetimedb.com/@your-username`.

### Dashboard features

The database dashboard gives you access to:

- **Database info**: View your database identity, name, and current status.
- **Logs**: View your module's log output in real time.
- **SQL console**: Run ad-hoc SQL queries against your database.
- **SpacetimeAuth**: Enable and configure the built-in authentication provider (see [SpacetimeAuth](/spacetimeauth)).

## Database Lifecycle

Maincloud databases have two states:

- **Running**: The database is actively serving requests. Any client connection, reducer call, or dashboard visit will keep it in this state.
- **Suspended**: After a period of inactivity (no client connections, no reducer calls), Maincloud automatically suspends the database to save resources. This is not the same as deleting it; all data is preserved.

A suspended database resumes automatically when it receives a connection or request. The first request after suspension may take a moment while the database wakes up and replays its commit log. Subsequent requests are served at normal speed.

There is currently no manual way to pause or resume a database from the dashboard or CLI. Suspension and resumption are handled automatically by Maincloud.

## Deleting a Database

To permanently delete a database and all its data:

```bash
spacetime delete my-module --server maincloud
```

This action cannot be undone.

## Next Steps

- **Explore the dashboard**: Visit [spacetimedb.com](https://spacetimedb.com) to view your database, check logs, and run queries.
- **Set up authentication**: Enable [SpacetimeAuth](/spacetimeauth) or connect a third-party [OIDC provider](/authentication) to authenticate your users.
- **Connect a client**: Follow a [quickstart guide](/quickstart) to build a client that connects to your Maincloud database.
- **Monitor your usage**: Check your energy consumption and plan limits on the [pricing page](https://spacetimedb.com/pricing).
