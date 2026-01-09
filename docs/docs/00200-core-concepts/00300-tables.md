---
title: Tables
slug: /tables
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Tables are the way to store data in SpacetimeDB. All data in SpacetimeDB is stored in memory for extremely low latency and high throughput access. SpacetimeDB also automatically persists all data to disk.

## Defining Tables

Tables are defined in your module code with a name, columns, and optional configuration.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Use the `table` function to declare a new table:

```typescript
import { table, t } from 'spacetimedb/server';

const people = table(
  { name: 'people', public: true },
  {
    id: t.u32().primaryKey().autoInc(),
    name: t.string().index('btree'),
    email: t.string().unique(),
  }
);
```

The first argument defines table options, and the second defines columns.

</TabItem>
<TabItem value="csharp" label="C#">

Use the `[SpacetimeDB.Table]` attribute on a `partial struct` or `partial class`:

```csharp
[SpacetimeDB.Table(Name = "people", Public = true)]
public partial struct People
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
#[spacetimedb::table(name = people, public)]
pub struct People {
    #[primary_key]
    #[auto_inc]
    id: u32,
    #[index(btree)]
    name: String,
    #[unique]
    email: String,
}
```

</TabItem>
</Tabs>

## Table Visibility

Tables can be **private** (default) or **public**:

- **Private tables**: Visible only to [reducers](/functions/reducers) and the database owner. Clients cannot access them.
- **Public tables**: Exposed for client read access through [subscriptions](/subscriptions). Writes still occur only through reducers.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const publicTable = table({ name: 'user', public: true }, { /* ... */ });
const privateTable = table({ name: 'secret', public: false }, { /* ... */ });
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Name = "user", Public = true)]
public partial struct User { /* ... */ }

[SpacetimeDB.Table(Name = "secret", Public = false)]
public partial struct Secret { /* ... */ }
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = user, public)]
pub struct User { /* ... */ }

#[spacetimedb::table(name = secret)]
pub struct Secret { /* ... */ }
```

</TabItem>
</Tabs>

## Next Steps

- [Column Types and Constraints](/tables/columns) - Define table structure with types, unique columns, primary keys, and auto-increment
- [Indexes](/tables/indexes) - Speed up queries with single and multi-column indexes
- [Access Permissions](/tables/access-permissions) - Query and modify tables from reducers, views, and clients
- Learn about [Scheduling tables](/tables/scheduled-tables)
- Learn about [Performance Best Practices](/tables/performance)
