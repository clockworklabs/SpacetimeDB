---
title: Tables
slug: /tables
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";


Tables are the way to store data in SpacetimeDB. All data in SpacetimeDB is stored in memory for extremely low latency and high throughput access. SpacetimeDB also automatically persists all data to disk.

## Why Tables

Tables are the fundamental unit of data organization in SpacetimeDB, just as files are the fundamental unit in Unix. However, tables possess greater generality than files. Unix requires a separate *filesystem* concept to organize and describe files. SpacetimeDB, by contrast, describes itself: it stores the representation of tables and their schemas in tables called **system tables** (such as `st_table` and `st_column`).

You can query these system tables directly:

```sql
SELECT * FROM st_table;
SELECT * FROM st_column;
```

:::warning
You can query system tables, but you should not modify them directly. Make schema changes through the normal definition mechanisms in your module code.
:::

### Tables and Data-Oriented Design

The relational model underlying tables represents the logical endpoint of [data-oriented design](https://spacetimedb.com/blog/databases-and-data-oriented-design). Patterns such as Entity Component Systems (ECS) implement a strict subset of relational capabilities. Tables give you the full power of relational theory: over fifty years of proven techniques for organizing and querying data efficiently.

The central principle of data-oriented design holds that **the purpose of any program is to transform data from one form to another**. Tables provide a principled, universal representation for that data, giving you:

- **Efficient access patterns** through indexes
- **Data integrity** through constraints
- **Flexible queries** through relational operations
- **Real-time synchronization** through subscriptions

For further discussion of this philosophy, see [The Zen of SpacetimeDB](../00100-intro/00100-getting-started/00250-zen-of-spacetimedb.md).

### Physical and Logical Independence

A core goal of the relational model is separating *logical* access patterns from *physical* data representation. When you write a subscription query, you express *what* data you need, not *how* the database should retrieve it. This separation allows SpacetimeDB to change the physical representation of your data for performance reasons without requiring you to rewrite your queries.

The clearest example is indexing. When you add an index to a column, you change how SpacetimeDB physically organizes that data. It builds an additional data structure to accelerate lookups. But your subscription queries continue to work unchanged. The same query that previously scanned the entire table now uses the index automatically. You improve performance by modifying the schema, not the queries.

This independence extends beyond indexes. SpacetimeDB can change internal storage formats, memory layouts, and access algorithms across versions. Your queries remain stable because they operate at the logical level (rows and columns) rather than the physical level of bytes and pointers.

### Table Decomposition

A common concern when designing relational schemas is whether to consolidate data into fewer large tables or distribute it across many smaller ones. In traditional SQL databases, joins require verbose query syntax and incur significant execution cost. This friction pushes developers toward denormalized schemas with fewer, wider tables.

SpacetimeDB operates under different constraints. Your reducers interact with tables through programmatic APIs rather than SQL strings. A join operation reduces to an index lookup: you retrieve a row from one table, extract a key value, and use that key to find related rows in another table. With all data resident in memory, these lookups often complete in nanoseconds.

Consider the following schema for a game application:

**Consolidated approach (not recommended):**

```
Player
├── id
├── name
├── position_x, position_y, velocity_x, velocity_y  (updates: 60Hz)
├── health, max_health, mana, max_mana              (updates: occasional)
├── total_kills, total_deaths, play_time            (updates: rare)
└── audio_volume, graphics_quality                  (updates: very rare)
```

**Decomposed approach (recommended):**

```
Player          PlayerState         PlayerStats         PlayerSettings
├── id     ←──  ├── player_id       ├── player_id       ├── player_id
└── name        ├── position_x      ├── total_kills     ├── audio_volume
                ├── position_y      ├── total_deaths    └── graphics_quality
                ├── velocity_x      └── play_time
                └── velocity_y

PlayerResources
├── player_id
├── health
├── max_health
├── mana
└── max_mana
```

The decomposed approach yields several advantages:

1. **Reduced bandwidth**: Clients subscribing to player positions do not receive updates when settings change. For an application with 1000 concurrent players updating positions at 60Hz, this reduction is substantial.

2. **Cache efficiency**: Data with similar update frequencies resides in contiguous memory. Updating a player's position does not require loading or invalidating cache lines containing lifetime statistics.

3. **Semantic clarity**: Each table maintains a single responsibility. `PlayerState` handles the performance-critical gameplay loop. `PlayerStats` serves leaderboard queries. `PlayerSettings` supports the options interface.

4. **Schema evolution**: You can add columns to `PlayerStats` without affecting the structure or performance characteristics of `PlayerState`.

The guiding principle: **organize data by access pattern, not by the entity it describes**. Keep data you read together in the same table. Separate data you read at different times or frequencies.

## Defining Tables

Tables are defined in your module code with a name, columns, and optional configuration.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Use the `table` function to declare a new table:

```typescript
import { schema, table, t } from 'spacetimedb/server';

const people = table(
  { name: 'people', public: true },
  {
    id: t.u32().primaryKey().autoInc(),
    name: t.string().index('btree'),
    email: t.string().unique(),
  }
);

const spacetimedb = schema({ people });
export default spacetimedb;
```

The first argument to `table()` defines table options, and the second defines columns. Pass your tables to `schema()` as an object: `schema({ people })` or `schema({ table1, table2 })`. The 1.0 syntax `schema(table)` or `schema(t1, t2, t3)` is no longer valid.

</TabItem>
<TabItem value="csharp" label="C#">

Use the `[SpacetimeDB.Table]` attribute on a `partial struct` or `partial class`:

```csharp
[SpacetimeDB.Table(Accessor = "Person", Public = true)]
public partial struct Person
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public uint Id;

    [SpacetimeDB.Index.BTree]
    public string Name;

    [SpacetimeDB.Unique]
    public string Email;
}
```

The `partial` modifier is required to allow code generation.

</TabItem>
<TabItem value="rust" label="Rust">

Use the `#[spacetimedb::table]` macro on a struct:

```rust
#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u32,
    #[index(btree)]
    name: String,
    #[unique]
    email: String,
}
```

:::note Rust Visibility vs SpacetimeDB Visibility
The `pub` modifier on the struct follows normal Rust visibility rules and has no meaning to SpacetimeDB. It controls whether the struct is accessible from other Rust modules in your crate, not whether the table is public to clients. Use the `public` attribute in `#[spacetimedb::table]` to control client visibility.
:::

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

Register the struct with `SPACETIMEDB_STRUCT`, the table with `SPACETIMEDB_TABLE`, then add field constraints:

```cpp
struct Person {
    uint32_t id;
    std::string name;
    std::string email;
};
SPACETIMEDB_STRUCT(Person, id, name, email)
SPACETIMEDB_TABLE(Person, person, Public)
FIELD_PrimaryKeyAutoInc(person, id)
FIELD_Index(person, name)
FIELD_Unique(person, email)
```

</TabItem>
</Tabs>

### Creating the schema (TypeScript)

After defining tables, pass them to `schema()` as a single object. The object keys become the accessor names in `ctx.db`:

```typescript
const people = table({ name: 'people', public: true }, { /* columns */ });
const products = table({ name: 'products', public: true }, { /* columns */ });

const spacetimedb = schema({ people, products });
export default spacetimedb;
```

Use `schema({ table1 })` or `schema({ t1, t2 })`. Never use `schema(table)` or `schema(t1, t2, t3)`.

For a compact list of TypeScript gotchas, see the [cheat sheet](../00100-databases/00500-cheat-sheet.md#common-mistakes).

## Table Naming and Accessors

The table name you specify determines how you access the table in your code and in SQL. Understanding this relationship is essential for writing correct SpacetimeDB modules.

:::note Table names in SQL
The table `name` in your schema is used **verbatim** in SQL queries and subscriptions. There is no automatic pluralization or case conversion. If you define `name: 'position'`, SQL uses `position`; if you define `name: 'positions'`, SQL uses `positions`. Ensure your schema names match what your SQL queries expect.
:::

### How Accessor Names Are Derived

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

The accessor name is converted from snake_case to camelCase:

```typescript
// Table definition
const player_scores = table(
  { name: 'player_scores', public: true },
  { /* columns */ }
);

// Accessor uses camelCase
ctx.db.playerScores.insert({ /* ... */ });
```

| Table Name | Accessor |
|------------|----------|
| `'user'` | `ctx.db.user` |
| `'player_scores'` | `ctx.db.playerScores` |
| `'game_session'` | `ctx.db.gameSession` |

</TabItem>
<TabItem value="csharp" label="C#">

The accessor name **exactly matches** the `Accessor` attribute value. When `Name` is not specified, `Accessor` is also used as the SQL table name:

```csharp
// Table definition
[SpacetimeDB.Table(Accessor = "Player", Public = true)]
public partial struct Player { /* columns */ }

// Accessor matches Accessor value exactly
ctx.Db.Player.Insert(new Player { /* ... */ });
```

| Accessor | API Accessor | SQL Table Name (when Name omitted) |
|----------|---------------|-------------------------------------|
| `"User"` | `ctx.Db.User` | `User` |
| `"Player"` | `ctx.Db.Player` | `Player` |
| `"GameSession"` | `ctx.Db.GameSession` | `GameSession` |

:::warning Case Sensitivity
The accessor is case-sensitive and must match the `Accessor` value exactly. `Accessor = "user"` produces `ctx.Db.user`, not `ctx.Db.User`.
:::

:::tip C# Convention
Use `Accessor = "TableName"` with PascalCase singular (e.g. `User` not `users`). The Accessor value is used verbatim as the SQL table name and API accessor.
:::

</TabItem>
<TabItem value="rust" label="Rust">

The accessor name **exactly matches** the `name` attribute value:

```rust
// Table definition
#[spacetimedb::table(accessor = player, public)]
pub struct Player { /* columns */ }

// Accessor matches name exactly
ctx.db.player().insert(Player { /* ... */ });
```

| name Attribute | Accessor |
|----------------|----------|
| `name = user` | `ctx.db.user()` |
| `name = player` | `ctx.db.player()` |
| `name = game_session` | `ctx.db.game_session()` |

</TabItem>
<TabItem value="cpp" label="C++">

The accessor name matches the table identifier you pass to `SPACETIMEDB_TABLE`:

```cpp
struct PlayerScores {
  uint64_t id;
};
SPACETIMEDB_STRUCT(PlayerScores, id)
SPACETIMEDB_TABLE(PlayerScores, player_scores, Public)
FIELD_PrimaryKeyAutoInc(player_scores, id)

// Accessor matches the table identifier
ctx.db[player_scores].insert(PlayerScores{ /* ... */ });
```

| Table Identifier | Accessor |
|------------------|----------|
| `user` | `ctx.db[user]` |
| `player_scores` | `ctx.db[player_scores]` |
| `game_session` | `ctx.db[game_session]` |

</TabItem>
</Tabs>

### Recommended Naming Conventions

Use idiomatic naming conventions for each language:

| Language | Convention | Example Table | Example Accessor |
|----------|------------|---------------|------------------|
| **TypeScript** | snake_case | `'player_score'` | `ctx.db.playerScore` |
| **C#** | PascalCase | `Name = "PlayerScore"` | `ctx.Db.PlayerScore` |
| **Rust** | lower_snake_case | `name = player_score` | `ctx.db.player_score()` |
| **C++** | lower_snake_case | `player_score` | `ctx.db[player_score]` |

These conventions align with each language's standard style guides and make your code feel natural within its ecosystem.

## Table Visibility

Tables can be **private** (default) or **public**:

- **Private tables**: Visible only to [reducers](./00200-functions/00300-reducers/00300-reducers.md) and the database owner. Clients cannot access them.
- **Public tables**: Exposed for client read access through [subscriptions](./00400-subscriptions.md). Writes still occur only through reducers.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const publicTable = table({ name: 'user', public: true }, { /* ... */ });
const privateTable = table({ name: 'secret', public: false }, { /* ... */ });
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Accessor = "User", Public = true)]
public partial struct User { /* ... */ }

[SpacetimeDB.Table(Accessor = "Secret", Public = false)]
public partial struct Secret { /* ... */ }
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(accessor = user, public)]
pub struct User { /* ... */ }

#[spacetimedb::table(accessor = secret)]
pub struct Secret { /* ... */ }
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
struct User {
  uint64_t id;
};
SPACETIMEDB_STRUCT(User, id)
SPACETIMEDB_TABLE(User, user, Public)
FIELD_PrimaryKeyAutoInc(user, id)

struct Secret {
  uint64_t id;
};
SPACETIMEDB_STRUCT(Secret, id)
SPACETIMEDB_TABLE(Secret, secret, Private)
FIELD_PrimaryKeyAutoInc(secret, id)
```

</TabItem>
</Tabs>

For more fine-grained access control, you can use [view functions](./00200-functions/00500-views.md) to expose computed subsets of your data to clients. Views allow you to filter rows, select specific columns, or join data from multiple tables before exposing it.

See [Access Permissions](./00300-tables/00400-access-permissions.md) for complete details on table visibility and access patterns.

## Multiple Tables for the Same Type

You can create multiple tables that share the same row type by applying multiple table attributes to a single struct. Each table stores its own independent set of rows, but all tables share the same schema.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

In TypeScript, define separate table variables that share the same column schema:

```typescript
import { table, t } from 'spacetimedb/server';

// Define the shared column schema
const playerColumns = {
  identity: t.Identity.primaryKey(),
  playerId: t.i32().unique().autoInc(),
  name: t.string(),
};

// Create two tables with the same schema
const Player = table({ name: 'Player', public: true }, playerColumns);
const LoggedOutPlayer = table({ name: 'LoggedOutPlayer' }, playerColumns);
```

</TabItem>
<TabItem value="csharp" label="C#">

Apply multiple `[Table]` attributes to the same struct:

```csharp
[SpacetimeDB.Table(Accessor = "Player", Public = true)]
[SpacetimeDB.Table(Accessor = "LoggedOutPlayer")]
public partial struct Player
{
    [PrimaryKey]
    public Identity Identity;
    [Unique, AutoInc]
    public int PlayerId;
    public string Name;
}
```

Each table gets its own accessor:

```csharp
// Insert into different tables
ctx.Db.Player.Insert(new Player { /* ... */ });
ctx.Db.LoggedOutPlayer.Insert(new Player { /* ... */ });

// Move a row between tables
var player = ctx.Db.LoggedOutPlayer.Identity.Find(ctx.Sender);
if (player != null)
{
    ctx.Db.Player.Insert(player.Value);
    ctx.Db.LoggedOutPlayer.Identity.Delete(player.Value.Identity);
}
```

</TabItem>
<TabItem value="rust" label="Rust">

Apply multiple `#[spacetimedb::table]` attributes to the same struct:

```rust
#[spacetimedb::table(accessor = player, public)]
#[spacetimedb::table(accessor = logged_out_player)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    #[unique]
    #[auto_inc]
    player_id: i32,
    name: String,
}
```

Each table gets its own accessor:

```rust
// Insert into different tables
ctx.db.player().insert(Player { /* ... */ });
ctx.db.logged_out_player().insert(Player { /* ... */ });

// Move a row between tables
if let Some(player) = ctx.db.logged_out_player().identity().find(&ctx.sender()) {
    ctx.db.player().insert(player.clone());
    ctx.db.logged_out_player().identity().delete(&player.identity);
}
```

</TabItem>
<TabItem value="cpp" label="C++">

Apply multiple `SPACETIMEDB_TABLE` macros to the same struct:

```cpp
struct Player {
  Identity identity;
  int32_t player_id;
  std::string name;
};
SPACETIMEDB_STRUCT(Player, identity, player_id, name)
SPACETIMEDB_TABLE(Player, player, Public)
SPACETIMEDB_TABLE(Player, logged_out_player, Private)
FIELD_PrimaryKey(player, identity)
FIELD_PrimaryKey(logged_out_player, identity)
FIELD_UniqueAutoInc(player, player_id)
FIELD_UniqueAutoInc(logged_out_player, player_id)

// Move a row between tables
auto maybe_logged_out = ctx.db[logged_out_player_identity].find(ctx.sender);
if (maybe_logged_out) {
  ctx.db[player].insert(*maybe_logged_out);
  ctx.db[logged_out_player_identity].delete_by_key(ctx.sender);
}
```

</TabItem>
</Tabs>

This pattern is useful for:

- **State management**: Separate active users from inactive users, online players from offline players
- **Archiving**: Move old records to an archive table while keeping the same schema
- **Staging**: Hold pending records in one table before moving them to a main table

:::note Shared Constraints
Column attributes like `[PrimaryKey]`, `[Unique]`, `[AutoInc]`, and `[Index]` apply to **all tables** defined on the type. Each table will have its own independent primary key, unique constraints, and indexes with the same structure.
:::

## Constraints

Tables support several constraints to enforce data integrity:

- **Primary keys** uniquely identify each row and define how updates and deletes work
- **Unique constraints** ensure no two rows share the same value for a column

See [Constraints](./00300-tables/00240-constraints.md) for details.

## Auto-Increment

Auto-increment columns automatically generate unique integer values for new rows. SpacetimeDB implements auto-increment using sequences, which provide crash-safe value generation with configurable parameters.

See [Auto-Increment](./00300-tables/00230-auto-increment.md) for details.

## Schedule Tables

Tables can trigger reducers at specific times by including a scheduling column. This allows you to schedule future actions like sending reminders, expiring content, or running periodic maintenance.

See [Schedule Tables](./00300-tables/00500-schedule-tables.md) for details.

## Next Steps

- [Column Types](./00300-tables/00200-column-types.md) - Supported column types and performance considerations
- [Constraints](./00300-tables/00240-constraints.md) - Primary keys and unique constraints
- [Auto-Increment](./00300-tables/00230-auto-increment.md) - Automatic ID generation with sequences
- [Default Values](./00300-tables/00250-default-values.md) - Schema evolution with column defaults
- [Indexes](./00300-tables/00300-indexes.md) - Speed up queries with single and multi-column indexes
- [Access Permissions](./00300-tables/00400-access-permissions.md) - Public vs private tables
- [Schedule Tables](./00300-tables/00500-schedule-tables.md) - Time-based reducer execution
- [Performance](./00300-tables/00600-performance.md) - Best practices for table design
