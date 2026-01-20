---
title: Indexes
slug: /tables/indexes
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Indexes accelerate queries by maintaining sorted data structures alongside your tables. Without an index, finding rows that match a condition requires scanning every row. With an index, the database locates matching rows directly.

## When to Use Indexes

Add an index when you frequently query a column with equality or range conditions. Common scenarios include:

- **Filtering by foreign key**: A `player_id` column in an inventory table benefits from an index when you query items belonging to a specific player.
- **Range queries**: An `age` column benefits from an index when you query users within an age range.
- **Sorting**: Columns used in ORDER BY clauses benefit from indexes that maintain sort order.

Indexes consume additional memory and slow down inserts and updates, since the database must maintain the index structure. Add indexes based on your actual query patterns rather than speculatively.

Primary keys and unique constraints automatically create indexes. You do not need to add a separate index for columns that already have these constraints.

## Index Types

SpacetimeDB supports two index types:

| Type | Use Case | Key Types | Multi-Column |
|------|----------|-----------|--------------|
| B-tree | General purpose | Any | Yes |
| Direct | Dense integer sequences | `u8`, `u16`, `u32`, `u64` | No |

### B-tree Indexes

B-trees maintain data in sorted order, enabling both equality lookups (`x = 5`) and range queries (`x > 5`, `x BETWEEN 1 AND 10`). The sorted structure also supports prefix matching on multi-column indexes. B-tree is the default and most commonly used index type.

### Direct Indexes

Direct indexes use array indexing instead of tree traversal, providing O(1) lookups for unsigned integer keys. SpacetimeDB uses the key value directly as an array offset, eliminating the need to search through a tree structure.

Direct indexes perform well when:
- Keys are dense (few gaps between values)
- Keys start near zero
- Insert patterns are sequential rather than random

Direct indexes perform poorly when:
- Keys are sparse (large gaps between values)
- The first key inserted is a large number
- Insert patterns are highly random

Direct indexes only support single-column indexes on unsigned integer types. Use them for auto-increment primary keys or other dense sequential identifiers where you need maximum lookup performance.

:::note
Direct indexes are currently available in Rust and TypeScript. C# support is planned.
:::

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const position = table(
  { name: 'position', public: true },
  {
    id: t.u32().primaryKey().index('direct'),
    x: t.f32(),
    y: t.f32(),
    z: t.f32(),
  }
);
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = position, public)]
pub struct Position {
    #[primary_key]
    #[index(direct)]
    id: u32,
    x: f32,
    y: f32,
    z: f32,
}
```

</TabItem>
</Tabs>

This example from the SpacetimeDB benchmarks uses direct indexes for a million entities with sequential IDs starting at 0, enabling O(1) lookups when joining position and velocity data by entity ID.

For most use cases, B-tree indexes provide good performance without these restrictions. Consider direct indexes only when profiling reveals that index lookups are a bottleneck and your key distribution matches the ideal pattern.

## Single-Column Indexes

A single-column index accelerates queries that filter on one column. You can define the index at the field level or the table level.

### Field-Level Syntax

The field-level syntax places the index declaration directly on the column:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const user = table(
  { name: 'user', public: true },
  {
    id: t.u32().primaryKey(),
    name: t.string().index('btree'),
    age: t.u8().index('btree'),
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Name = "User", Public = true)]
public partial struct User
{
    [SpacetimeDB.PrimaryKey]
    public uint Id;

    [SpacetimeDB.Index.BTree]
    public string Name;

    [SpacetimeDB.Index.BTree]
    public byte Age;
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = user, public)]
pub struct User {
    #[primary_key]
    id: u32,
    #[index(btree)]
    name: String,
    #[index(btree)]
    age: u8,
}
```

</TabItem>
</Tabs>

### Table-Level Syntax

The table-level syntax defines indexes separately from columns. This approach allows you to name the index explicitly:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const user = table(
  {
    name: 'user',
    public: true,
    indexes: [
      { name: 'idx_age', algorithm: 'btree', columns: ['age'] },
    ],
  },
  {
    id: t.u32().primaryKey(),
    name: t.string(),
    age: t.u8(),
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Name = "User", Public = true)]
[SpacetimeDB.Index.BTree(Name = "idx_age", Columns = new[] { "Age" })]
public partial struct User
{
    [SpacetimeDB.PrimaryKey]
    public uint Id;

    public string Name;

    public byte Age;
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = user, public, index(name = idx_age, btree(columns = [age])))]
pub struct User {
    #[primary_key]
    id: u32,
    name: String,
    age: u8,
}
```

</TabItem>
</Tabs>

## Multi-Column Indexes

A multi-column index (also called a composite index) spans multiple columns. The index maintains rows sorted by the first column, then by the second column within equal values of the first, and so on.

Multi-column indexes support:
- **Full match**: Queries that specify all indexed columns
- **Prefix match**: Queries that specify the leftmost columns in order
- **Range on trailing column**: A prefix of equality conditions followed by a range on the next column

A multi-column index on `(player_id, level)` accelerates these queries:
- `player_id = 123` (prefix match on first column)
- `player_id = 123 AND level = 5` (full match)
- `player_id = 123 AND level > 5` (prefix match with range)

The same index does not accelerate a query on `level` alone, since `level` is not a prefix of the index.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const score = table(
  {
    name: 'score',
    public: true,
    indexes: [
      { name: 'by_player_and_level', algorithm: 'btree', columns: ['player_id', 'level'] },
    ],
  },
  {
    player_id: t.u32(),
    level: t.u32(),
    points: t.i64(),
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Name = "Score", Public = true)]
[SpacetimeDB.Index.BTree(Name = "by_player_and_level", Columns = new[] { "PlayerId", "Level" })]
public partial struct Score
{
    public uint PlayerId;
    public uint Level;
    public long Points;
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = score, public, index(name = by_player_and_level, btree(columns = [player_id, level])))]
pub struct Score {
    player_id: u32,
    level: u32,
    points: i64,
}
```

</TabItem>
</Tabs>

## Querying with Indexes

SpacetimeDB generates type-safe accessor methods for each index. These methods accept filter arguments and return matching rows.

### Equality Queries

Pass a single value to find rows where the indexed column equals that value:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Find users with a specific name
for (const user of ctx.db.user.name.filter('Alice')) {
  console.log(`Found user: ${user.id}`);
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Find users with a specific name
foreach (var user in ctx.Db.User.Name.Filter("Alice"))
{
    Log.Info($"Found user: {user.Id}");
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Find users with a specific name
for user in ctx.db.user().name().filter("Alice") {
    log::info!("Found user: {}", user.id);
}
```

</TabItem>
</Tabs>

### Range Queries

Pass a `Range` object to find rows where the indexed column falls within bounds. The `Range` constructor accepts `from` and `to` bounds, each specified as `{ tag: 'included', value }`, `{ tag: 'excluded', value }`, or `{ tag: 'unbounded' }`:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { Range } from 'spacetimedb/server';

// Find users aged 18 to 65 (inclusive)
for (const user of ctx.db.user.age.filter(
  new Range({ tag: 'included', value: 18 }, { tag: 'included', value: 65 })
)) {
  console.log(`${user.name} is ${user.age}`);
}

// Find users aged 18 or older (from 18 inclusive, unbounded above)
for (const user of ctx.db.user.age.filter(
  new Range({ tag: 'included', value: 18 }, { tag: 'unbounded' })
)) {
  console.log(`${user.name} is an adult`);
}

// Find users younger than 18 (unbounded below, to 18 exclusive)
for (const user of ctx.db.user.age.filter(
  new Range({ tag: 'unbounded' }, { tag: 'excluded', value: 18 })
)) {
  console.log(`${user.name} is a minor`);
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Find users aged 18 or older
foreach (var user in ctx.Db.User.Age.Filter(new Bound<byte>.Inclusive(18), null))
{
    Log.Info($"{user.Name} is an adult");
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Find users aged 18 to 65 (inclusive)
for user in ctx.db.user().age().filter(18..=65) {
    log::info!("{} is {}", user.name, user.age);
}

// Find users aged 18 or older
for user in ctx.db.user().age().filter(18..) {
    log::info!("{} is an adult", user.name);
}

// Find users younger than 18
for user in ctx.db.user().age().filter(..18) {
    log::info!("{} is a minor", user.name);
}
```

</TabItem>
</Tabs>

### Multi-Column Queries

For multi-column indexes, pass a tuple of values. You can specify exact values for prefix columns and optionally a range for the trailing column:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { Range } from 'spacetimedb/server';

// Find all scores for player 123 (prefix match on first column)
for (const score of ctx.db.score.by_player_and_level.filter(123)) {
  console.log(`Level ${score.level}: ${score.points} points`);
}

// Find scores for player 123 at levels 1-10 (inclusive)
for (const score of ctx.db.score.by_player_and_level.filter([
  123,
  new Range({ tag: 'included', value: 1 }, { tag: 'included', value: 10 })
])) {
  console.log(`Level ${score.level}: ${score.points} points`);
}

// Find the exact score for player 123 at level 5
for (const score of ctx.db.score.by_player_and_level.filter([123, 5])) {
  console.log(`Points: ${score.points}`);
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Find all scores for player 123
foreach (var score in ctx.Db.Score.by_player_and_level.Filter(123u))
{
    Log.Info($"Level {score.Level}: {score.Points} points");
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Find all scores for player 123 (prefix match)
for score in ctx.db.score().by_player_and_level().filter(&123u32) {
    log::info!("Level {}: {} points", score.level, score.points);
}

// Find scores for player 123 at levels 1-10
for score in ctx.db.score().by_player_and_level().filter((123u32, 1u32..=10u32)) {
    log::info!("Level {}: {} points", score.level, score.points);
}

// Find the exact score for player 123 at level 5
for score in ctx.db.score().by_player_and_level().filter((123u32, 5u32)) {
    log::info!("Points: {}", score.points);
}
```

</TabItem>
</Tabs>

## Deleting with Indexes

Indexes also accelerate deletions. Instead of scanning the entire table to find rows to delete, you can delete directly by index value:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { Range } from 'spacetimedb/server';

// Delete all users named "Alice"
const deleted = ctx.db.user.name.delete('Alice');
console.log(`Deleted ${deleted} user(s)`);

// Delete users younger than 18
const deletedMinors = ctx.db.user.age.delete(
  new Range({ tag: 'unbounded' }, { tag: 'excluded', value: 18 })
);
console.log(`Deleted ${deletedMinors} minor(s)`);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Delete all users named "Alice"
var deleted = ctx.Db.User.Name.Delete("Alice");
Log.Info($"Deleted {deleted} user(s)");
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Delete all users named "Alice"
let deleted = ctx.db.user().name().delete("Alice");
log::info!("Deleted {} user(s)", deleted);

// Delete users in an age range
let deleted = ctx.db.user().age().delete(..18);
log::info!("Deleted {} minor(s)", deleted);
```

</TabItem>
</Tabs>

## Index Design Guidelines

**Choose columns based on query patterns.** Index the columns that appear in your WHERE clauses and JOIN conditions. An unused index wastes memory.

**Consider column order in multi-column indexes.** Place the most selective column (the one that narrows results most) first, followed by columns used in range conditions. An index on `(country, city)` works for queries on `country` alone or `country AND city`, but not for queries on `city` alone.

**Avoid redundant indexes.** A multi-column index on `(a, b)` makes a separate index on `(a)` redundant, since the multi-column index handles prefix queries. However, an index on `(b)` is not redundant if you query `b` independently.

**Balance read and write performance.** Each index speeds up reads but slows down writes. Tables with high write volume and few reads may benefit from fewer indexes.

## Next Steps

- Learn about [Constraints](/tables/constraints) for primary keys and unique indexes
- See [Access Permissions](/tables/access-permissions) for querying tables from reducers
