---
title: Table Storage and Cache Policies
slug: /tables/storage-and-cache-policies
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

SpacetimeDB uses tiered storage to keep frequently accessed data in memory while allowing colder data to move to local disk or object storage. A table's **cache policy** determines which storage tiers its data can use. You declare the policy in your module's table definition.

## Choose a Cache Policy

| Allowed storage tiers | TypeScript / Rust value | C# value | Behavior |
|-----------------------|-------------------------|----------|----------|
| Memory only (default) | `required_in_memory` | `RequiredInMemory` | Load all the table's pages and blobs into memory at start up. |
| Memory and local disk | `required_on_disk` | `RequiredOnDisk` | Allow colder data to page out of memory to local disk. Hot data can remain in memory. |
| Any available tier | `no_requirement` | `NoRequirement` | Allow memory, local disk, and object storage where the deployment enables it. |

Omitting the setting selects the in-memory policy. SpacetimeDB moves eligible data out of memory as it needs room and loads it again when accessed. See [Resident memory limits](../../00250-how-to/00100-deploy/00100-maincloud.md#resident-memory-limits) for how this works on Maincloud.

## Declare the Policy in Your Module

These examples define a table that can page to local disk and a table that can page to any available tier.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Use the `cachePolicy` option of `table()`:

```typescript
import { schema, table, t } from 'spacetimedb/server';

const localHistory = table(
  { name: 'local_history', cachePolicy: 'required_on_disk' },
  {
    id: t.u64().primaryKey(),
    details: t.string(),
  }
);

const archivedHistory = table(
  { name: 'archived_history', cachePolicy: 'no_requirement' },
  {
    id: t.u64().primaryKey(),
    details: t.string(),
  }
);

const spacetimedb = schema({ localHistory, archivedHistory });
export default spacetimedb;
```

</TabItem>
<TabItem value="csharp" label="C#">

Set `CachePolicy` on the table attribute:

```csharp
using SpacetimeDB;

[Table(Name = "local_history", Accessor = "LocalHistory",
    CachePolicy = TableCachePolicy.RequiredOnDisk)]
public partial struct LocalHistory
{
    [PrimaryKey]
    public ulong Id;
    public string Details;
}

[Table(Name = "archived_history", Accessor = "ArchivedHistory",
    CachePolicy = TableCachePolicy.NoRequirement)]
public partial struct ArchivedHistory
{
    [PrimaryKey]
    public ulong Id;
    public string Details;
}
```

</TabItem>
<TabItem value="rust" label="Rust">

Use `cache_policy` in the table macro:

```rust
use spacetimedb::table;

#[table(accessor = local_history, cache_policy = required_on_disk)]
pub struct LocalHistory {
    #[primary_key]
    id: u64,
    details: String,
}

#[table(accessor = archived_history, cache_policy = no_requirement)]
pub struct ArchivedHistory {
    #[primary_key]
    id: u64,
    details: String,
}
```

</TabItem>
</Tabs>

## Residency, Durability, and Performance

Cache policies control where data can reside while the database is running. They do not affect durability. For example, when a database restarts, SpacetimeDB recovers data even for tables configured to reside fully in memory.

The policy applies to table pages and large values stored as blobs. A blob shared by multiple tables stays in memory if at least one of their policies requires it.

Accessing cold data may require storage reads, increasing latency. Use indexed lookups to limit the pages an operation touches, and reserve the in-memory policy for data that needs consistently fast access. Large scans can bring substantial amounts of cold data into the cache. See [Performance Best Practices](./00600-performance.md).

## Change an Existing Table's Policy

Update the policy in your module and republish. To return to the default, select the in-memory policy or remove the setting. See [Changing a table's cache policy](../00100-databases/00500-migrations/00200-automatic-migrations.md#changing-a-tables-cache-policy) for the publish command and memory requirements.
