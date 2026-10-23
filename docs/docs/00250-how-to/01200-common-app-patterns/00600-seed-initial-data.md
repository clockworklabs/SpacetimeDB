---
title: Seed initial data on first publish
slug: /how-to/app-patterns/seed-initial-data
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
You need a module you can publish. If you don't have one yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.
:::

In this guide, you'll fill your game shop's item catalog when you first publish your module, and update it when the catalog changes. You'll insert the items from the `init` [lifecycle reducer](../../00200-core-concepts/00200-functions/00300-reducers/00500-lifecycle.md), and store who published the database, so that only they can update the catalog later.

## How it works

- A public `shop_item` table holds the catalog. Each item has a fixed `sku`, such as `sword`, and a name and price that can change.
- The catalog is a list in your module's code. A `seed_items` function inserts each item, or updates it if it's already there.
- The `init` reducer calls `seed_items` when the database is created. It also stores the identity of the database owner, the user who published it, in a private `config` table.
- Publishing new code to an existing database doesn't run `init` again. To update the catalog there, the owner calls the `seed` reducer, which runs `seed_items` again.

Running `seed_items` twice changes nothing, so the owner can call `seed` after every publish.

## Define the tables

Add two tables:

- `shop_item` holds the catalog. It's public, so clients can subscribe to it. Its primary key is `sku`, so `seed_items` can find each item.
- `config` holds the owner's identity, in a single row with `id` 0. It's private, so clients can't read it.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema, table, t, SenderError, type InferSchema, type ReducerCtx } from 'spacetimedb/server';

const shopItem = table(
  { name: 'shop_item', public: true },
  {
    sku: t.string().primaryKey(),
    name: t.string(),
    price: t.u32(),
  }
);

const config = table(
  { name: 'config' },
  {
    id: t.u8().primaryKey(),
    owner: t.identity(),
  }
);

const spacetimedb = schema({ shopItem, config });
export default spacetimedb;

type Ctx = ReducerCtx<InferSchema<typeof spacetimedb>>;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "ShopItem", Public = true)]
    public partial struct ShopItem
    {
        [SpacetimeDB.PrimaryKey]
        public string Sku;
        public string Name;
        public uint Price;
    }

    [SpacetimeDB.Table(Accessor = "Config")]
    public partial struct Config
    {
        [SpacetimeDB.PrimaryKey]
        public byte Id;
        public Identity Owner;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{reducer, table, Identity, ReducerContext, Table};

#[table(accessor = shop_item, public)]
pub struct ShopItem {
    #[primary_key]
    sku: String,
    name: String,
    price: u32,
}

#[table(accessor = config)]
pub struct Config {
    #[primary_key]
    id: u8,
    owner: Identity,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

struct ShopItem {
    std::string sku;
    std::string name;
    uint32_t price;
};
SPACETIMEDB_STRUCT(ShopItem, sku, name, price)
SPACETIMEDB_TABLE(ShopItem, shop_item, Public)
FIELD_PrimaryKey(shop_item, sku)

struct Config {
    uint8_t id;
    Identity owner;
};
SPACETIMEDB_STRUCT(Config, id, owner)
SPACETIMEDB_TABLE(Config, config, Private)
FIELD_PrimaryKey(config, id)
```

</TabItem>
</Tabs>

## Seed the catalog when the database is created

Add the catalog, and a `seed_items` function that goes through it: it updates the items that are already in `shop_item`, and inserts the others.

Then add the `init` reducer. It stores the caller's identity in `config`: in `init`, the caller is the user who published the database, and it's the only place where SpacetimeDB tells you who that is. Then it calls `seed_items`.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// The shop's catalog. To change it, edit it here, publish, then call `seed`.
const CATALOG = [
  { sku: 'sword', name: 'Iron sword', price: 100 },
  { sku: 'shield', name: 'Wooden shield', price: 60 },
  { sku: 'potion', name: 'Health potion', price: 25 },
];

// Inserts each catalog item, or updates it if it's already there.
function seedItems(ctx: Ctx) {
  for (const item of CATALOG) {
    if (ctx.db.shopItem.sku.find(item.sku)) {
      ctx.db.shopItem.sku.update(item);
    } else {
      ctx.db.shopItem.insert(item);
    }
  }
}

export const init = spacetimedb.init(ctx => {
  ctx.db.config.insert({ id: 0, owner: ctx.sender });
  seedItems(ctx);
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // The shop's catalog. To change it, edit it here, publish, then call `Seed`.
    static readonly ShopItem[] Catalog =
    {
        new ShopItem { Sku = "sword", Name = "Iron sword", Price = 100 },
        new ShopItem { Sku = "shield", Name = "Wooden shield", Price = 60 },
        new ShopItem { Sku = "potion", Name = "Health potion", Price = 25 },
    };

    // Inserts each catalog item, or updates it if it's already there.
    static void SeedItems(ReducerContext ctx)
    {
        foreach (var item in Catalog)
        {
            if (ctx.Db.ShopItem.Sku.Find(item.Sku) is null)
            {
                ctx.Db.ShopItem.Insert(item);
            }
            else
            {
                ctx.Db.ShopItem.Sku.Update(item);
            }
        }
    }

    [SpacetimeDB.Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        ctx.Db.Config.Insert(new Config { Id = 0, Owner = ctx.Sender });
        SeedItems(ctx);
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
/// The shop's catalog: each item's sku, name and price. To change it, edit it here, publish, then call `seed`.
const CATALOG: &[(&str, &str, u32)] = &[
    ("sword", "Iron sword", 100),
    ("shield", "Wooden shield", 60),
    ("potion", "Health potion", 25),
];

/// Inserts each catalog item, or updates it if it's already there.
fn seed_items(ctx: &ReducerContext) {
    for &(sku, name, price) in CATALOG {
        let item = ShopItem {
            sku: sku.to_string(),
            name: name.to_string(),
            price,
        };
        if ctx.db.shop_item().sku().find(&item.sku).is_some() {
            ctx.db.shop_item().sku().update(item);
        } else {
            ctx.db.shop_item().insert(item);
        }
    }
}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.config().insert(Config {
        id: 0,
        owner: ctx.sender(),
    });
    seed_items(ctx);
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// The shop's catalog. To change it, edit it here, publish, then call `seed`.
const std::vector<ShopItem> CATALOG = {
    {"sword", "Iron sword", 100},
    {"shield", "Wooden shield", 60},
    {"potion", "Health potion", 25},
};

// Inserts each catalog item, or updates it if it's already there.
void seed_items(ReducerContext& ctx) {
    for (const auto& item : CATALOG) {
        if (ctx.db[shop_item_sku].find(item.sku)) {
            ctx.db[shop_item_sku].update(item);
        } else {
            ctx.db[shop_item].insert(item);
        }
    }
}

SPACETIMEDB_INIT(init, ReducerContext ctx) {
    ctx.db[config].insert(Config{0, ctx.sender()});
    seed_items(ctx);
    return Ok();
}
```

</TabItem>
</Tabs>

`init` runs in a transaction, like any reducer. If it fails, its changes are undone and the publish fails, so a database never starts with half a catalog.

:::note
`init` only runs when the database is created, or when you publish with `--delete-data`, which erases all data and creates the database again. Publishing new code to an existing database doesn't run it, so changes to the catalog don't reach that database until you call `seed`, below.
:::

## Update the catalog of an existing database

Add a `seed` reducer that runs `seed_items` again. It rejects every caller except the owner stored in `config`, so players can't call it.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const seed = spacetimedb.reducer(ctx => {
  const owner = ctx.db.config.id.find(0)?.owner;
  if (!owner || !owner.isEqual(ctx.sender)) {
    throw new SenderError('Only the owner can seed the database');
  }
  seedItems(ctx);
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void Seed(ReducerContext ctx)
    {
        if (ctx.Db.Config.Id.Find(0) is not Config config || config.Owner != ctx.Sender)
        {
            throw new Exception("Only the owner can seed the database");
        }
        SeedItems(ctx);
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn seed(ctx: &ReducerContext) -> Result<(), String> {
    match ctx.db.config().id().find(0) {
        Some(config) if config.owner == ctx.sender() => {}
        _ => return Err("Only the owner can seed the database".to_string()),
    }
    seed_items(ctx);
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_REDUCER(seed, ReducerContext ctx) {
    auto row = ctx.db[config_id].find(0);
    if (!row || !(row->owner == ctx.sender())) {
        return Err("Only the owner can seed the database");
    }
    seed_items(ctx);
    return Ok();
}
```

</TabItem>
</Tabs>

After you change the catalog, publish, then call `seed` from the CLI. The CLI calls reducers with your identity, the one that published the database, so `seed` accepts the call:

```bash
spacetime publish my-game
spacetime call my-game seed
```

## Check the data

To see the catalog, query `shop_item` from the CLI:

```bash
spacetime sql my-game "SELECT * FROM shop_item"
```

Right after the first publish, it shows the three items:

```text
 sku      | name            | price
----------+-----------------+-------
 "sword"  | "Iron sword"    | 100
 "shield" | "Wooden shield" | 60
 "potion" | "Health potion" | 25
```

Clients read `shop_item` like any other public table: subscribe to it, and the catalog is in the client cache once the subscription is applied.

## What's next?

You now fill your shop's catalog when you first publish, and update it when it changes:

- The catalog is a list in your module's code, and `seed_items` inserts or updates each item, so running it again changes nothing.
- The `init` reducer seeds new databases, and stores the identity of the user who published them.
- An owner-only `seed` reducer updates the catalog of databases that already exist.

To adapt it to your app:

- **Items you remove from the catalog.** `seed_items` doesn't delete them. If nothing refers to them, delete the rows that aren't in the catalog in `seed_items`. If players can own them, add an `available` column instead, and set it to false.
- **Large catalogs.** Keep the catalog in a JSON file, build it into your module, for example with `include_str!` in Rust, and parse it in `seed_items`.
- **The first admin.** In `init`, give the owner the admin role from [Admin and moderator roles](./00200-admin-roles.md). Then check that only admins can call `set_role`, so the owner can give roles to others.
- **Test data.** To fill a development database with test players, add another owner-only reducer, and only call it on that database.

To learn more about the features used here, see [Lifecycle Reducers](../../00200-core-concepts/00200-functions/00300-reducers/00500-lifecycle.md) and [Access Permissions](../../00200-core-concepts/00300-tables/00400-access-permissions.md).

Next, you can [store files in S3](./00700-store-files-in-s3.md).
