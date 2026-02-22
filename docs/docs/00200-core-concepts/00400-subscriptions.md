---
title: Subscriptions
slug: /clients/subscriptions
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Subscriptions replicate database rows to your client in real-time. When you subscribe to a query, SpacetimeDB sends you the matching rows immediately and then pushes updates whenever those rows change.

## Quick Start

Here's a complete example showing how to subscribe to data and react to changes:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';

// Connect to the database
const conn = DbConnection.builder()
  .withUri('wss://maincloud.spacetimedb.com')
  .withDatabaseName('my_module')
  .onConnect((ctx) => {
    // Subscribe to users and messages using query builders
    ctx.subscriptionBuilder()
      .onApplied(() => {
        console.log('Subscription ready!');
        // Initial data is now in the client cache
        for (const user of ctx.db.user.iter()) {
          console.log(`User: ${user.name}`);
        }
      })
      .subscribe([tables.user, tables.message]);
  })
  .build();

// React to new rows being inserted
conn.db.user.onInsert((ctx, user) => {
  console.log(`New user joined: ${user.name}`);
});

// React to rows being deleted
conn.db.user.onDelete((ctx, user) => {
  console.log(`User left: ${user.name}`);
});

// React to rows being updated
conn.db.user.onUpdate((ctx, oldUser, newUser) => {
  console.log(`${oldUser.name} changed name to ${newUser.name}`);
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Connect to the database
var conn = DbConnection.Builder()
    .WithUri("wss://maincloud.spacetimedb.com")
    .WithDatabaseName("my_module")
    .OnConnect((ctx) =>
    {
        // Subscribe to users and messages
        ctx.SubscriptionBuilder()
            .OnApplied(() =>
            {
                Console.WriteLine("Subscription ready!");
                // Initial data is now in the client cache
                foreach (var user in ctx.Db.User.Iter())
                {
                    Console.WriteLine($"User: {user.Name}");
                }
            })
            .AddQuery(q => q.From.User())
            .AddQuery(q => q.From.Message())
            .Subscribe();
    })
    .Build();

// React to new rows being inserted
conn.Db.User.OnInsert += (ctx, user) =>
{
    Console.WriteLine($"New user joined: {user.Name}");
};

// React to rows being deleted
conn.Db.User.OnDelete += (ctx, user) =>
{
    Console.WriteLine($"User left: {user.Name}");
};

// React to rows being updated
conn.Db.User.OnUpdate += (ctx, oldUser, newUser) =>
{
    Console.WriteLine($"{oldUser.Name} changed name to {newUser.Name}");
};
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Connect to the database
let conn = DbConnection::builder()
    .with_uri("wss://maincloud.spacetimedb.com")
    .with_database_name("my_module")
    .on_connect(|ctx| {
        // Subscribe to users and messages
        ctx.subscription_builder()
            .on_applied(|ctx| {
                println!("Subscription ready!");
                // Initial data is now in the client cache
                for user in ctx.db.user().iter() {
                    println!("User: {}", user.name);
                }
            })
            .add_query(|q| q.from.user())
            .add_query(|q| q.from.message())
            .subscribe();
    })
    .build();

// React to new rows being inserted
conn.db().user().on_insert(|ctx, user| {
    println!("New user joined: {}", user.name);
});

// React to rows being deleted
conn.db().user().on_delete(|ctx, user| {
    println!("User left: {}", user.name);
});

// React to rows being updated
conn.db().user().on_update(|ctx, old_user, new_user| {
    println!("{} changed name to {}", old_user.name, new_user.name);
});
```

</TabItem>
</Tabs>

:::tip Typed Query Builders
Type-safe query builders are available in TypeScript, C#, and Rust and are the recommended default. They provide auto-completion and compile-time type checking. For complete API details, see [TypeScript](./00600-clients/00700-typescript-reference.md#query-builder-api), [C#](./00600-clients/00600-csharp-reference.md#query-builder-api), and [Rust](./00600-clients/00500-rust-reference.md#query-builder-api) references.
:::

## How Subscriptions Work

1. **Subscribe**: Subscribe with queries to the data you need
2. **Receive initial data**: SpacetimeDB sends all matching rows immediately
3. **Receive updates**: When subscribed rows change, you get real-time updates
4. **React to changes**: Use row callbacks (`onInsert`, `onDelete`, `onUpdate`) to handle changes

The client maintains a local cache of subscribed data. Reading from the cache is instant since it's local memory.

For advanced raw SQL subscription syntax, see the [SQL docs](../00300-resources/00200-reference/00400-sql-reference.md#subscriptions).

## Common API Concepts

This page focuses on subscription behavior and usage patterns that apply across SDKs. For exact method signatures and SDK-specific overloads, use the language references.

### Builder and Lifecycle Callbacks

All SDKs expose a builder API for creating subscriptions:

- Register an applied callback: runs once initial matching rows are present in the local cache.
- Register an error callback: runs if subscription registration fails or a subscription later terminates with an error.
- Subscribe with one or more queries.

### Query Forms

All SDKs support subscriptions. TypeScript, C#, and Rust support query builders (recommended), while Unreal uses query strings:

| SDK | Typed Query Builder Support | Entry Point |
| --- | --- | --- |
| TypeScript | Yes | `tables.<table>.where(...)` passed to `subscribe(...)` |
| C# | Yes | `SubscriptionBuilder.AddQuery(...).Subscribe()` |
| Rust | Yes | `subscription_builder().add_query(...).subscribe()` |
| Unreal | No | Query strings passed to `Subscribe(...)` |

### Subscription Handles

Subscribing returns a handle that manages an individual subscription lifecycle.

- `isActive` / `IsActive` / `is_active` indicates that matching rows are currently active in the cache.
- `isEnded` / `IsEnded` / `is_ended` indicates a subscription has ended, either from unsubscribe or error.
- Unsubscribe is asynchronous: rows are removed after the unsubscribe operation is applied.
- `subscribeToAllTables` / `SubscribeToAllTables` / `subscribe_to_all_tables` is a convenience entry point intended for simple clients and is not individually cancelable.

### API References

- [TypeScript subscription API](./00600-clients/00700-typescript-reference.md#subscribe-to-queries)
- [TypeScript query builder API](./00600-clients/00700-typescript-reference.md#query-builder-api)
- [C# subscription API](./00600-clients/00600-csharp-reference.md#subscribe-to-queries)
- [C# query builder API](./00600-clients/00600-csharp-reference.md#query-builder-api)
- [Rust subscription API](./00600-clients/00500-rust-reference.md#subscribe-to-queries)
- [Rust query builder API](./00600-clients/00500-rust-reference.md#query-builder-api)
- [Unreal subscription API](./00600-clients/00800-unreal-reference.md#subscriptions)

## Best Practices for Optimizing Server Compute and Reducing Serialization Overhead

### 1. Writing Efficient Subscription Queries

Use the typed query builder to express precise filters and keep subscriptions small. If you use raw SQL subscriptions, see [SQL Best Practices](../00300-resources/00200-reference/00400-sql-reference.md#best-practices-for-performance-and-scalability).

### 2. Group Subscriptions with the Same Lifetime Together

Subscriptions with the same lifetime should be grouped together.

For example, you may have certain data that is required for the lifetime of your application,
but you may have other data that is only sometimes required by your application.

By managing these sets as two independent subscriptions,
your application can subscribe and unsubscribe from the latter,
without needlessly unsubscribing and resubscribing to the former.

This will improve throughput by reducing the amount of data transferred from the database to your application.

#### Example

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('https://maincloud.spacetimedb.com')
  .withDatabaseName('my_module')
  .build();

// Never need to unsubscribe from global subscriptions
const globalSubscriptions = conn
  .subscriptionBuilder()
  .subscribe([
    // Global messages the client should always display
    tables.announcements,
    // A description of rewards for in-game achievements
    tables.badges,
  ]);

// May unsubscribe to shop_items as player advances
const shopSubscription = conn
  .subscriptionBuilder()
  .subscribe([
    tables.shopItems.where(r => r.requiredLevel.lte(5)),
  ]);
```

</TabItem>
<TabItem value="csharp" label="C#">

```cs
var conn = ConnectToDB();

// Never need to unsubscribe from global subscriptions
var globalSubscriptions = conn
    .SubscriptionBuilder()
    .AddQuery(q => q.From.Announcements())
    .AddQuery(q => q.From.Badges())
    .Subscribe();

// May unsubscribe to shop_items as player advances
var shopSubscription = conn
    .SubscriptionBuilder()
    .AddQuery(q => q.From.ShopItems().Where(r => r.RequiredLevel.Lte(5U)))
    .Subscribe();
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let conn: DbConnection = connect_to_db();

// Never need to unsubscribe from global subscriptions
let global_subscriptions = conn
    .subscription_builder()
    .add_query(|q| q.from.announcements())
    .add_query(|q| q.from.badges())
    .subscribe();

// May unsubscribe to shop_items as player advances
let shop_subscription = conn
    .subscription_builder()
    .add_query(|q| q.from.shop_items().r#where(|r| r.required_level.lte(5u32)))
    .subscribe();
```

</TabItem>
</Tabs>

### 3. Subscribe Before Unsubscribing

If you want to update or modify a subscription by dropping it and subscribing to a new set,
you should subscribe to the new set before unsubscribing from the old one.

This is because SpacetimeDB subscriptions are zero-copy.
Subscribing to the same query more than once doesn't incur additional processing or serialization overhead.
Likewise, if a query is subscribed to more than once,
unsubscribing from it does not result in any server processing or data serializtion.

#### Example

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('https://maincloud.spacetimedb.com')
  .withDatabaseName('my_module')
  .build();

// Initial subscription: player at level 5.
const shopSubscription = conn
  .subscriptionBuilder()
  .subscribe([
    // For displaying the price of shop items in the player's currency of choice
    tables.exchangeRates,
    tables.shopItems.where(r => r.requiredLevel.lte(5)),
  ]);

// New subscription: player now at level 6, which overlaps with the previous query.
const newShopSubscription = conn
  .subscriptionBuilder()
  .subscribe([
    // For displaying the price of shop items in the player's currency of choice
    tables.exchangeRates,
    tables.shopItems.where(r => r.requiredLevel.lte(6)),
  ]);

// Unsubscribe from the old subscription once the new one is in place.
if (shopSubscription.isActive()) {
  shopSubscription.unsubscribe();
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```cs
var conn = ConnectToDB();

// Initial subscription: player at level 5.
var shopSubscription = conn
    .SubscriptionBuilder()
    .AddQuery(q => q.From.ExchangeRates())
    .AddQuery(q => q.From.ShopItems().Where(r => r.RequiredLevel.Lte(5U)))
    .Subscribe();

// New subscription: player now at level 6, which overlaps with the previous query.
var newShopSubscription = conn
    .SubscriptionBuilder()
    .AddQuery(q => q.From.ExchangeRates())
    .AddQuery(q => q.From.ShopItems().Where(r => r.RequiredLevel.Lte(6U)))
    .Subscribe();

// Unsubscribe from the old subscription once the new one is in place.
if (shopSubscription.IsActive)
{
    shopSubscription.Unsubscribe();
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let conn: DbConnection = connect_to_db();

// Initial subscription: player at level 5.
let shop_subscription = conn
    .subscription_builder()
    .add_query(|q| q.from.exchange_rates())
    .add_query(|q| q.from.shop_items().r#where(|r| r.required_level.lte(5u32)))
    .subscribe();

// New subscription: player now at level 6, which overlaps with the previous query.
let new_shop_subscription = conn
    .subscription_builder()
    .add_query(|q| q.from.exchange_rates())
    .add_query(|q| q.from.shop_items().r#where(|r| r.required_level.lte(6u32)))
    .subscribe();

// Unsubscribe from the old subscription once the new one is active.
if shop_subscription.is_active() {
    shop_subscription.unsubscribe();
}
```

</TabItem>
</Tabs>

### 4. Avoid Overlapping Queries

This refers to distinct queries that return intersecting data sets,
which can result in the server processing and serializing the same row multiple times.
While SpacetimeDB can manage this redundancy, it may lead to unnecessary inefficiencies.

Consider the following two query builder subscriptions:

```typescript
tables.user
tables.user.where(r => r.id.eq(5))
```

If `User.id` is a unique or primary key column,
the cost of subscribing to both queries is minimal.
This is because the server will use an index when processing the 2nd query,
and it will only serialize a single row for the 2nd query.

In contrast, consider these two query builder subscriptions:

```typescript
tables.user
tables.user.where(r => r.id.ne(5))
```

The server must now process each row of the `User` table twice,
since the 2nd query cannot be processed using an index.
It must also serialize all but one row of the `User` table twice,
due to the significant overlap between the two queries.

By following these best practices, you can optimize your data replication strategy and ensure your application remains efficient and responsive.
