---
title: Quick Reference
slug: /new-modules/quick-reference
---

# Quick Reference

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

Server modules are the core of a SpacetimeDB application. A module is a server-side application—a collection of [stored procedures](https://en.wikipedia.org/wiki/Stored_procedure) and schema definitions—that runs inside the database. Modules can be written in TypeScript, C#, or Rust.

Modules define the structure of your database and the business logic that responds to client requests. They interact with the outside world via tables and reducers:

- **Tables** store your data. Public tables are directly queryable and subscribable by clients via SQL.
- **Reducers** are server-side functions that read and write tables. They're callable over the network and are transactional, ensuring data consistency and integrity.

Clients connect directly to the database to read public data via SQL subscriptions and queries, and they invoke reducers to mutate state.

<figure>
  ![SpacetimeDB Architecture](/images/basic-architecture-diagram.png)
  <figcaption style={{ marginTop: '10px', textAlign: 'center' }} align="center">
    <b align="center">SpacetimeDB application architecture</b>
    <span style={{ fontSize: '14px' }}>
      {' '}
      (elements in white are provided by SpacetimeDB)
    </span>
  </figcaption>
</figure>

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">
Rust modules are written with the the Rust Module Library. They are built using [cargo](https://doc.rust-lang.org/cargo/) and deployed using the [`spacetime` CLI tool](https://spacetimedb.com/install). Rust modules can import any Rust [crate](https://crates.io/) that supports being compiled to WebAssembly.

This reference assumes you are familiar with the basics of Rust. If you aren't, check out Rust's [excellent documentation](https://www.rust-lang.org/learn). For a guided introduction to Rust Modules, see the [Rust Module Quickstart](https://spacetimedb.com/docs/modules/rust/quickstart).
</TabItem>
<TabItem value="csharp" label="C#">
C# modules are written with the the C# Module Library (this package). They are built using the [dotnet CLI tool](https://learn.microsoft.com/en-us/dotnet/core/tools/) and deployed using the [`spacetime` CLI tool](https://spacetimedb.com/install). C# modules can import any [NuGet package](https://www.nuget.org/packages) that supports being compiled to WebAssembly.

(Note: C# can also be used to write **clients** of SpacetimeDB databases, but this requires using a different library, the SpacetimeDB C# Client SDK. See the documentation on [clients] for more information.)

This reference assumes you are familiar with the basics of C#. If you aren't, check out the [C# language documentation](https://learn.microsoft.com/en-us/dotnet/csharp/). For a guided introduction to C# Modules, see the [C# Module Quickstart](/docs/quickstarts/c-sharp).
</TabItem>
<TabItem value="typescript" label="TypeScript">
TypeScript modules are built with the TypeScript Module Library from [`spacetimedb/server`](https://www.npmjs.com/package/spacetimedb). You define your schema and reducers in TypeScript, and then build and deploy with the [`spacetime` CLI](https://spacetimedb.com/install) using the `spacetime publish` command. Under the hood, SpacetimeDB uses [Rolldown](https://rolldown.rs/) to bundle your application into a single JavaScript artifact before uploading it to the SpacetimeDB host.

:::note
SpacetimeDB also provides a TypeScript **client** SDK at `spacetimedb/sdk`, as well as integrations for frameworks like `spacetimedb/react`. This guide focuses exclusively on the **server-side module** library.
:::

If you’re new to TypeScript, see the [TypeScript Handbook](https://www.typescriptlang.org/docs/handbook/intro.html). For a guided introduction to modules, see the [TypeScript Module Quickstart](/docs/quickstarts/typescript).
</TabItem>
</Tabs>
## Setup

:::note
Make sure to [**install the CLI**](https://spacetimedb.com/install) in your preferred shell.
:::

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

1. **Initialize a Rust module project**

   ```bash
   spacetime init --lang rust --project-path my-project my-project
   cd my-project
   ```

   This creates a scaffold with a sample module and a `Cargo.toml` project file in `my-project/spacetimedb`.

2. **Develop**

   - Add tables with `#[table]` and reducers with `#[reducer]` in your source.

3. **Build and publish**

   ```bash
   spacetime login
   spacetime publish <DATABASE_NAME>
   # Example: spacetime publish my-project
   ```

</TabItem>
<TabItem value="csharp" label="C#">

1. **Initialize a C# module project**

   ```bash
   spacetime init --lang csharp --project-path my-project my-project
   cd my-project
   ```

   This creates a scaffold dotnet project in `my-project/spacetimedb` with the following `StdbModule.csproj`.

2. **Develop**

   - Add tables with `[SpacetimeDB.Table]` and reducers with `[SpacetimeDB.Reducer]` in your source.

3. **Build and publish**

   ```bash
   spacetime login
   spacetime publish <DATABASE_NAME>
   # Example: spacetime publish my-project
   ```

</TabItem>
<TabItem value="typescript" label="TypeScript">

1. **Initialize a TypeScript module project**

   ```bash
   spacetime init --lang typescript --project-path my-project my-project
   cd my-project
   ```

   This creates a scaffold TypeScript project with a sample module entrypoint and `spacetimedb/package.json`.

2. **Develop**

   - Add tables with `table(...)` and reducers with `spacetimedb.reducer(...)` in your source.

3. **Build and publish**

   ```bash
   spacetime login
   spacetime publish <DATABASE_NAME>
   # Example: spacetime publish my-project
   ```

Publishing bundles your code into a JavaScript bundle, and creates a database and installs your bundle in that database. The CLI outputs the database’s **name** and **Identity** (a hex string). Save this identity for administration tasks like `spacetime logs <DATABASE_NAME>`.

:::warning
IMPORTANT! In order to build and publish your module, you must have a `src/index.ts` file in your project. If you have multiple files that define reducers, you must import them from that file. e.g.

```ts
import './schema';
import './my_reducers';
import './my_other_reducers';
```

This ensures that those files are included in the bundle.
:::

</TabItem>
</Tabs>

To update an existing module:

```bash
spacetime publish <DATABASE_NAME>
```

SpacetimeDB attempts [automatic migrations](/databases/automatic-migrations) when you republish.

## How it works
<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">
Under the hood, SpacetimeDB modules are WebAssembly modules that import a [specific WebAssembly ABI](https://spacetimedb.com/docs/webassembly-abi) and export a small number of special functions. This is automatically configured when you add the `spacetime` crate as a dependency of your application.

The SpacetimeDB host is an application that hosts SpacetimeDB databases. [Its source code is available](https://github.com/clockworklabs/SpacetimeDB) under [the Business Source License with an Additional Use Grant](https://github.com/clockworklabs/SpacetimeDB/blob/master/LICENSE.txt). You can run your own host, or you can upload your module to the public SpacetimeDB network. <!-- TODO: want a link to some dashboard for the public network. --> The network will create a database for you and install your module in it to serve client requests.

#### In More Detail: Publishing a Module

The `spacetime publish [DATABASE_IDENTITY]` command compiles a module and uploads it to a SpacetimeDB host. After this:
- The host finds the database with the requested `DATABASE_IDENTITY`.
  - (Or creates a fresh database and identity, if no identity was provided).
- The host loads the new module and inspects its requested database schema. If there are changes to the schema, the host tries perform an [automatic migration](/databases/automatic-migrations). If the migration fails, publishing fails.
- The host terminates the old module attached to the database.
- The host installs the new module into the database. It begins running the module's [lifecycle reducers](/reducers/lifecycle) and [scheduled reducers](/tables/scheduled-tables), starting with the [`#[init]` reducer](/reducers/lifecycle).
- The host begins allowing clients to call the module's reducers.

From the perspective of clients, this process is seamless. Open connections are maintained and subscriptions continue functioning. [Automatic migrations](/databases/automatic-migrations) forbid most table changes except for adding new tables, so client code does not need to be recompiled.
However:
- Clients may witness a brief interruption in the execution of scheduled reducers (for example, game loops.)
- New versions of a module may remove or change reducers that were previously present. Client code calling those reducers will receive runtime errors.

</TabItem>
<TabItem value="csharp" label="C#">
Under the hood, SpacetimeDB modules are WebAssembly modules that import a [specific WebAssembly ABI](https://spacetimedb.com/docs/webassembly-abi) and export a small number of special functions. This is automatically configured when you add the `SpacetimeDB.Runtime` package as a dependency of your application.

The SpacetimeDB host is an application that hosts SpacetimeDB databases. [Its source code is available](https://github.com/clockworklabs/SpacetimeDB) under [the Business Source License with an Additional Use Grant](https://github.com/clockworklabs/SpacetimeDB/blob/master/LICENSE.txt). You can run your own host, or you can upload your module to the public SpacetimeDB network. <!-- TODO: want a link to some dashboard for the public network. --> The network will create a database for you and install your module in it to serve client requests.

#### In More Detail: Publishing a Module

The `spacetime publish [DATABASE_IDENTITY]` command compiles a module and uploads it to a SpacetimeDB host. After this:

- The host finds the database with the requested `DATABASE_IDENTITY`.
  - (Or creates a fresh database and identity, if no identity was provided).
- The host loads the new module and inspects its requested database schema. If there are changes to the schema, the host tries perform an [automatic migration](/databases/automatic-migrations). If the migration fails, publishing fails.
- The host terminates the old module attached to the database.
- The host installs the new module into the database. It begins running the module's [lifecycle reducers](/reducers/lifecycle) and [scheduled reducers](/tables/scheduled-tables), starting with the `Init` reducer.
- The host begins allowing clients to call the module's reducers.

From the perspective of clients, this process is seamless. Open connections are maintained and subscriptions continue functioning. [Automatic migrations](/databases/automatic-migrations) forbid most table changes except for adding new tables, so client code does not need to be recompiled.
However:

- Clients may witness a brief interruption in the execution of scheduled reducers (for example, game loops.)
- New versions of a module may remove or change reducers that were previously present. Client code calling those reducers will receive runtime errors.

</TabItem>
<TabItem value="typescript" label="TypeScript">
SpacetimeDB transpiles and bundles your code into a JavaScript bundle that conform to its host ABI (application binary interface). The **host** loads your module, applies schema migrations, initializes lifecycle reducers, and serves client calls. During module updates, active connections and subscriptions remain intact, allowing you to hotswap your server code without affecting or disconnecting any clients.

#### Publishing Flow

When you run `spacetime publish <DATABASE_NAME>`, the following happens:

- The host locates or creates the target database.
- The new schema is compared against the current version; if compatible, an [automatic migration](/databases/automatic-migrations) runs.
- The host atomically swaps in the new module, invoking lifecycle reducers such as `Init`.
- The module becomes live, serving new reducer calls.

</TabItem>
</Tabs>

## Tables

All data in SpacetimeDB is stored in the form of **tables**. SpacetimeDB tables are hosted in memory, in the same process as your code, for extremely low latency and high throughput access to your data. SpacetimeDB also automatically persists all data in tables to disk behind the scenes.

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{table, Table};

#[table(name = player, public)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[unique]
    username: String,
    score: i32,
}
```

- `#[table())]` declares a table
- `public` makes it readable by clients
- `#[primary_key]` marks a unique identifier
- `#[auto_inc]` auto-assigns increasing IDs
- `#[unique]` enforces uniqueness on a column

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Public = true)]
public partial struct Player
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    
    [SpacetimeDB.Unique]
    public string Username;
    
    public int Score;
}
```

- `[SpacetimeDB.Table]` declares a table
- `Public = true` makes it readable by clients
- `[PrimaryKey]` marks a unique identifier
- `[AutoInc]` auto-assigns increasing IDs
- `[Unique]` enforces uniqueness on a column

</TabItem>
<TabItem value="typescript" label="TypeScript">

```typescript
import { table, t } from 'spacetimedb/server';

const player = table(
  { name: 'player', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    username: t.string().unique(),
    score: t.i32(),
  }
);
```

- `table()` declares a table
- `public: true` makes it readable by clients
- `.primaryKey()` marks a unique identifier
- `.autoInc()` auto-assigns increasing IDs
- `.unique()` enforces uniqueness on a column

</TabItem>
</Tabs>

### Public and Private Tables

By default, tables are **private**—only visible to reducers and the database owner. Set tables as **public** to make them readable by all clients via SQL queries and subscriptions.

### Indexes

Indexes speed up lookups and filtering operations.

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

```rust
#[table(name = game_score, public)]
pub struct GameScore {
    player_id: u64,
    #[index(btree)]
    level: u32,
    points: i64,
}
```

Multi-column indexes:

```rust
#[table(name = game_score, public, index(name = player_level, btree(columns = [player_id, level])))]
pub struct GameScore {
    player_id: u64,
    level: u32,
    points: i64,
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Name = "game_score")]
public partial struct GameScore
{
    public ulong PlayerId;
    
    [SpacetimeDB.Index.BTree]
    public uint Level;
    
    public long Points;
}
```

Multi-column indexes:

```csharp
[SpacetimeDB.Table(Name = "game_score")]
[SpacetimeDB.Index.BTree(
    Name = "PlayerLevel", 
    Columns = [nameof(PlayerId), nameof(Level)]
)]
public partial struct GameScore
{
    public ulong PlayerId;
    public uint Level;
    public long Points;
}
```

</TabItem>
<TabItem value="typescript" label="TypeScript">

```typescript
const gameScores = table(
  { name: 'game_scores', public: true },
  {
    player_id: t.u64(),
    level: t.u32().index('btree'),
    points: t.i64(),
  }
);
```

Multi-column indexes:

```typescript
const gameScores = table(
  {
    name: 'game_scores',
    public: true,
    indexes: [
      {
        name: 'player_level',
        algorithm: 'btree',
        columns: ['player_id', 'level'],
      },
    ],
  },
  {
    player_id: t.u64(),
    level: t.u32(),
    points: t.i64(),
  }
);
```

</TabItem>
</Tabs>

:::note
Follow this link to get a deeper understanding of [Tables](/tables).
:::

## Reducers

Reducers are server-side functions that modify database state. They execute transactionally—either all changes apply or none do.

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn give_points(ctx: &ReducerContext, player_id: u64, points: i64) {
    if let Some(mut player) = ctx.db.player().id().find(player_id) {
        player.score += points as i32;
        ctx.db.player().id().update(player);
    }
}
```

Reducers have access to a `ReducerContext` with:
- Database access via generated table methods
- `ctx.sender` - caller's `Identity`
- `ctx.timestamp` - invocation time

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
  [SpacetimeDB.Reducer]
  public static void GivePoints(ReducerContext ctx, ulong playerId, long points)
  {
      if (ctx.Db.Player.Id.Find(playerId) is Player player)
      {
          player.Score += (int)points;
          ctx.Db.Player.Id.Update(player);
      }
  }
```

Reducers have access to a `ReducerContext` with:
- `ctx.Db` - database tables and indexes
- `ctx.Sender` - caller's `Identity`
- `ctx.Timestamp` - invocation time
- `ctx.ConnectionId` - caller's connection ID

</TabItem>
<TabItem value="typescript" label="TypeScript">

```typescript
spacetimedb.reducer('give_points', { player_id: t.u64(), points: t.i64() }, (ctx, { player_id, points }) => {
  const player = ctx.db.player.id.find(player_id);
  if (player) {
    player.score += Number(points);
    ctx.db.player.id.update(player);
  }
});
```

Reducers have access to a `ctx` with:
- `ctx.db` - database tables and indexes
- `ctx.sender` - caller's `Identity`
- `ctx.timestamp` - invocation time
- `ctx.connectionId` - caller's connection ID

</TabItem>
</Tabs>

### Error Handling

Reducers can fail by throwing errors. All changes are automatically rolled back on failure.

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn admin_only(ctx: &ReducerContext) -> Result<(), String> {
    if ctx.sender != ctx.identity() {
        return Err("Unauthorized".to_string());
    }
    // ... admin operations
    Ok(())
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Reducer]
public static void AdminOnly(ReducerContext ctx)
{
    if (ctx.Sender != ctx.Identity)
    {
        throw new Exception("Unauthorized");
    }
    // ... admin operations
}
```

</TabItem>
<TabItem value="typescript" label="TypeScript">

```typescript
import { SenderError } from 'spacetimedb/server';

spacetimedb.reducer('admin_only', {}, ctx => {
  if (ctx.sender != ctx.identity) {
    throw new SenderError('Unauthorized');
  }
  // ... admin operations
});
```

</TabItem>
</Tabs>

:::note
Follow this link to get a deeper understanding of [Reducers](/reducers).
:::

## Logging

Log messages for debugging and monitoring. Logs are private to the database owner.

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn write_debug(_: &ReducerContext) {
    log::info!("Debug information");
    log::warn!("Warning message");
    log::error!("Error message");
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

[SpacetimeDB.Reducer]
public static void DebugInfo(ReducerContext ctx)
{
    Log.Info("Debug information");
    Log.Warn("Warning message");
    Log.Error("Error message");
}
```

</TabItem>
<TabItem value="typescript" label="TypeScript">

```typescript
spacetimedb.reducer('debug_info', ctx => {
  console.log('Debug information');
  console.warn('Warning message');
  console.error('Error message');
});
```

</TabItem>
</Tabs>

View logs with:

```bash
spacetime logs <DATABASE_NAME>
```

## Next Steps

- Learn about [client SDKs](/sdks) to connect to your module
- Lean more about [Tables](/tables), [Reducers](/reducers), [Views](/), and [Procedures](/procedures).
- Understand [automatic migrations](/databases/automatic-migrations) when updating your module
