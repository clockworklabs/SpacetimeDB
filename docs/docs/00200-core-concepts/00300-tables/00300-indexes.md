---
title: Indexes
slug: /tables/indexes
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Indexes enable efficient querying of table data. SpacetimeDB supports B-Tree indexes on single or multiple columns.

## Single-Column Indexes

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const user = table(
  { name: 'user', public: true },
  {
    id: t.u32().primaryKey(),
    name: t.string().index('btree'),
  }
);

// Query using the index
for (const user of ctx.db.user.name.filter('Alice')) {
  // users with name = 'Alice'
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Public = true)]
public partial struct User
{
    [SpacetimeDB.PrimaryKey]
    public uint Id;

    [SpacetimeDB.Index.BTree]
    public string Name;
}

// Query using the index
foreach (var user in ctx.Db.User.Name.Filter("Alice"))
{
    // users with Name == "Alice"
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
}

// Query using the index
for user in ctx.db.user().name().filter("Alice") {
    // users with name == "Alice"
}
```

</TabItem>
</Tabs>

## Multi-Column Indexes

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const score = table(
  {
    name: 'score',
    public: true,
    indexes: [
      {
        name: 'byPlayerAndLevel',
        algorithm: 'btree',
        columns: ['player_id', 'level'],
      },
    ],
  },
  {
    player_id: t.u32(),
    level: t.u32(),
    points: t.i64(),
  }
);

// Query with prefix match
for (const score of ctx.db.score.byPlayerAndLevel.filter(123)) {
  // scores with player_id = 123
}

// Query with range
for (const score of ctx.db.score.byPlayerAndLevel.filter([123, [1, 10]])) {
  // player_id = 123, 1 <= level <= 10
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Public = true)]
[SpacetimeDB.Index.BTree(Name = "byPlayerAndLevel", Columns = new[] { "PlayerId", "Level" })]
public partial struct Score
{
    public uint PlayerId;
    public uint Level;
    public long Points;
}

// Query with prefix match
foreach (var score in ctx.Db.Score.byPlayerAndLevel.Filter(123u))
{
    // scores with PlayerId == 123
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

// Query with prefix match
for score in ctx.db.score().by_player_and_level().filter(&123) {
    // scores with player_id == 123
}
```

</TabItem>
</Tabs>
