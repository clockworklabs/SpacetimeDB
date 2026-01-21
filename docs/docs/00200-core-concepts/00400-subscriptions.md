---
title: Subscription Reference
slug: /subscriptions
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Subscriptions replicate database rows to your client in real-time. When you subscribe to a query, SpacetimeDB sends you the matching rows immediately and then pushes updates whenever those rows change.

## Quick Start

Here's a complete example showing how to subscribe to data and react to changes:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, User, Message } from './module_bindings';

// Connect to the database
const conn = DbConnection.builder()
  .withUri('wss://maincloud.spacetimedb.com')
  .withModuleName('my_module')
  .onConnect((ctx) => {
    // Subscribe to users and messages
    ctx.subscriptionBuilder()
      .onApplied(() => {
        console.log('Subscription ready!');
        // Initial data is now in the client cache
        for (const user of ctx.db.user.iter()) {
          console.log(`User: ${user.name}`);
        }
      })
      .subscribe(['SELECT * FROM user', 'SELECT * FROM message']);
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
    .WithModuleName("my_module")
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
            .Subscribe(new[] { "SELECT * FROM user", "SELECT * FROM message" });
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
    .with_module_name("my_module")
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
            .subscribe(["SELECT * FROM user", "SELECT * FROM message"]);
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

## How Subscriptions Work

1. **Subscribe**: Register SQL queries describing the data you need
2. **Receive initial data**: SpacetimeDB sends all matching rows immediately
3. **Receive updates**: When subscribed rows change, you get real-time updates
4. **React to changes**: Use row callbacks (`onInsert`, `onDelete`, `onUpdate`) to handle changes

The client maintains a local cache of subscribed data. Reading from the cache is instant since it's local memory.

For more information on subscription SQL syntax see the [SQL docs](/reference/sql#subscriptions).

## API Reference

This section describes the two main interfaces: `SubscriptionBuilder` and `SubscriptionHandle`.

## SubscriptionBuilder

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
interface SubscriptionBuilder {
  // Register a callback to run when the subscription is applied.
  onApplied(callback: (ctx: SubscriptionEventContext) => void): SubscriptionBuilder;

  // Register a callback to run when the subscription fails.
  // This callback may run when attempting to apply the subscription,
  // or later during the subscription's lifetime if the module's interface changes.
  onError(callback: (ctx: ErrorContext, error: Error) => void): SubscriptionBuilder;

  // Subscribe to the following SQL queries.
  // Returns immediately; callbacks are invoked when data arrives from the server.
  subscribe(querySqls: string[]): SubscriptionHandle;

  // Subscribe to all rows from all tables.
  // Intended for applications where memory and bandwidth are not concerns.
  subscribeToAllTables(): void;
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```cs
public sealed class SubscriptionBuilder
{
    /// <summary>
    /// Register a callback to run when the subscription is applied.
    /// </summary>
    public SubscriptionBuilder OnApplied(
        Action<SubscriptionEventContext> callback
    );

    /// <summary>
    /// Register a callback to run when the subscription fails.
    ///
    /// Note that this callback may run either when attempting to apply the subscription,
    /// in which case <c>Self::on_applied</c> will never run,
    /// or later during the subscription's lifetime if the module's interface changes,
    /// in which case <c>Self::on_applied</c> may have already run.
    /// </summary>
    public SubscriptionBuilder OnError(
        Action<ErrorContext, Exception> callback
    );

    /// <summary>
    /// Subscribe to the following SQL queries.
    ///
    /// This method returns immediately, with the data not yet added to the DbConnection.
    /// The provided callbacks will be invoked once the data is returned from the remote server.
    /// Data from all the provided queries will be returned at the same time.
    ///
    /// See the SpacetimeDB SQL docs for more information on SQL syntax:
    /// <a href="pathname:///docs/sql">pathname:///docs/sql</a>
    /// </summary>
    public SubscriptionHandle Subscribe(
        string[] querySqls
    );

    /// <summary>
    /// Subscribe to all rows from all tables.
    ///
    /// This method is intended as a convenience
    /// for applications where client-side memory use and network bandwidth are not concerns.
    /// Applications where these resources are a constraint
    /// should register more precise queries via <c>Self.Subscribe</c>
    /// in order to replicate only the subset of data which the client needs to function.
    /// </summary>
    public void SubscribeToAllTables();
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
pub struct SubscriptionBuilder<M: SpacetimeModule> { /* private fields */ }

impl<M: SpacetimeModule> SubscriptionBuilder<M> {
    /// Register a callback that runs when the subscription has been applied.
    /// This callback receives a context containing the current state of the subscription.
    pub fn on_applied(mut self, callback: impl FnOnce(&M::SubscriptionEventContext) + Send + 'static);

    /// Register a callback to run when the subscription fails.
    ///
    /// Note that this callback may run either when attempting to apply the subscription,
    /// in which case [`Self::on_applied`] will never run,
    /// or later during the subscription's lifetime if the module's interface changes,
    /// in which case [`Self::on_applied`] may have already run.
    pub fn on_error(mut self, callback: impl FnOnce(&M::ErrorContext, crate::Error) + Send + 'static);

    /// Subscribe to a subset of the database via a set of SQL queries.
    /// Returns a handle which you can use to monitor or drop the subscription later.
    pub fn subscribe<Queries: IntoQueries>(self, query_sql: Queries) -> M::SubscriptionHandle;

    /// Subscribe to all rows from all tables.
    ///
    /// This method is intended as a convenience
    /// for applications where client-side memory use and network bandwidth are not concerns.
    /// Applications where these resources are a constraint
    /// should register more precise queries via [`Self::subscribe`]
    /// in order to replicate only the subset of data which the client needs to function.
    pub fn subscribe_to_all_tables(self);
}

/// Types which specify a list of query strings.
pub trait IntoQueries {
    fn into_queries(self) -> Box<[Box<str>]>;
}
```

</TabItem>
</Tabs>

A `SubscriptionBuilder` provides an interface for registering subscription queries with a database.
It allows you to register callbacks that run when the subscription is successfully applied or when an error occurs.
Once applied, a client will start receiving row updates to its client cache.
A client can react to these updates by registering row callbacks for the appropriate table.

### Example Usage

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Establish a database connection
import { DbConnection } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('https://maincloud.spacetimedb.com')
  .withModuleName('my_module')
  .build();

// Register a subscription with the database
const userSubscription = conn
  .subscriptionBuilder()
  .onApplied((ctx) => { /* handle applied state */ })
  .onError((ctx, error) => { /* handle error */ })
  .subscribe(['SELECT * FROM user', 'SELECT * FROM message']);
```

</TabItem>
<TabItem value="csharp" label="C#">

```cs
// Establish a database connection
var conn = ConnectToDB();

// Register a subscription with the database
var userSubscription = conn
    .SubscriptionBuilder()
    .OnApplied((ctx) => { /* handle applied state */ })
    .OnError((errorCtx, error) => { /* handle error */ })
    .Subscribe(new string[] { "SELECT * FROM user", "SELECT * FROM message" });
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Establish a database connection
let conn: DbConnection = connect_to_db();

// Register a subscription with the database
let subscription_handle = conn
    .subscription_builder()
    .on_applied(|ctx| { /* handle applied state */ })
    .on_error(|error_ctx, error| { /* handle error */ })
    .subscribe(["SELECT * FROM user", "SELECT * FROM message"]);
```

</TabItem>
</Tabs>

## SubscriptionHandle

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
interface SubscriptionHandle {
  // Whether the subscription has ended (unsubscribed or terminated due to error).
  isEnded(): boolean;

  // Whether the subscription is currently active.
  isActive(): boolean;

  // Unsubscribe from the query controlled by this handle.
  // Throws if called more than once.
  unsubscribe(): void;

  // Unsubscribe and call onEnded when rows are removed from the client cache.
  unsubscribeThen(onEnded?: (ctx: SubscriptionEventContext) => void): void;
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```cs
    public class SubscriptionHandle<SubscriptionEventContext, ErrorContext> : ISubscriptionHandle
        where SubscriptionEventContext : ISubscriptionEventContext
        where ErrorContext : IErrorContext
    {
        /// <summary>
        /// Whether the subscription has ended.
        /// </summary>
        public bool IsEnded;

        /// <summary>
        /// Whether the subscription is active.
        /// </summary>
        public bool IsActive;

        /// <summary>
        /// Unsubscribe from the query controlled by this subscription handle.
        ///
        /// Calling this more than once will result in an exception.
        /// </summary>
        public void Unsubscribe();

        /// <summary>
        /// Unsubscribe from the query controlled by this subscription handle,
        /// and call onEnded when its rows are removed from the client cache.
        /// </summary>
        public void UnsubscribeThen(Action<SubscriptionEventContext>? onEnded);
    }
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
pub trait SubscriptionHandle: InModule + Clone + Send + 'static
where
    Self::Module: SpacetimeModule<SubscriptionHandle = Self>,
{
    /// Returns `true` if the subscription has been ended.
    /// That is, if it has been unsubscribed or terminated due to an error.
    fn is_ended(&self) -> bool;

    /// Returns `true` if the subscription is currently active.
    fn is_active(&self) -> bool;

    /// Unsubscribe from the query controlled by this `SubscriptionHandle`,
    /// then run `on_end` when its rows are removed from the client cache.
    /// Returns an error if the subscription is already ended,
    /// or if unsubscribe has already been called.
    fn unsubscribe_then(self, on_end: OnEndedCallback<Self::Module>) -> crate::Result<()>;

    /// Unsubscribe from the query controlled by this `SubscriptionHandle`.
    /// Returns an error if the subscription is already ended,
    /// or if unsubscribe has already been called.
    fn unsubscribe(self) -> crate::Result<()>;
}
```

</TabItem>
</Tabs>

When you register a subscription, you receive a `SubscriptionHandle`.
A `SubscriptionHandle` manages the lifecycle of each subscription you register.
In particular, it provides methods to check the status of the subscription and to unsubscribe if necessary.
Because each subscription has its own independently managed lifetime,
clients can dynamically subscribe to different subsets of the database as their application requires.

### Example Usage

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

Consider a game client that displays shop items and discounts based on a player's level.
You subscribe to `shop_items` and `shop_discounts` when a player is at level 5:

```typescript
const conn = DbConnection.builder()
  .withUri('https://maincloud.spacetimedb.com')
  .withModuleName('my_module')
  .build();

const shopItemsSubscription = conn
  .subscriptionBuilder()
  .onApplied((ctx) => { /* handle applied state */ })
  .onError((ctx, error) => { /* handle error */ })
  .subscribe([
    'SELECT * FROM shop_items WHERE required_level <= 5',
    'SELECT * FROM shop_discounts WHERE required_level <= 5',
  ]);
```

Later, when the player reaches level 6 and new items become available,
you can subscribe to the new queries and unsubscribe from the old ones:

```typescript
const newShopItemsSubscription = conn
  .subscriptionBuilder()
  .onApplied((ctx) => { /* handle applied state */ })
  .onError((ctx, error) => { /* handle error */ })
  .subscribe([
    'SELECT * FROM shop_items WHERE required_level <= 6',
    'SELECT * FROM shop_discounts WHERE required_level <= 6',
  ]);

if (shopItemsSubscription.isActive()) {
  shopItemsSubscription.unsubscribe();
}
```

All other subscriptions continue to remain in effect.

</TabItem>
<TabItem value="csharp" label="C#">

Consider a game client that displays shop items and discounts based on a player's level.
You subscribe to `shop_items` and `shop_discounts` when a player is at level 5:

```cs
var conn = ConnectToDB();

var shopItemsSubscription = conn
    .SubscriptionBuilder()
    .OnApplied((ctx) => { /* handle applied state */ })
    .OnError((errorCtx, error) => { /* handle error */ })
    .Subscribe(new string[] {
        "SELECT * FROM shop_items WHERE required_level <= 5",
        "SELECT * FROM shop_discounts WHERE required_level <= 5",
    });
```

Later, when the player reaches level 6 and new items become available,
you can subscribe to the new queries and unsubscribe from the old ones:

```cs
var newShopItemsSubscription = conn
    .SubscriptionBuilder()
    .OnApplied((ctx) => { /* handle applied state */ })
    .OnError((errorCtx, error) => { /* handle error */ })
    .Subscribe(new string[] {
        "SELECT * FROM shop_items WHERE required_level <= 6",
        "SELECT * FROM shop_discounts WHERE required_level <= 6",
    });

if (shopItemsSubscription.IsActive)
{
    shopItemsSubscription.Unsubscribe();
}
```

All other subscriptions continue to remain in effect.
</TabItem>
<TabItem value="rust" label="Rust">
Consider a game client that displays shop items and discounts based on a player's level.
You subscribe to `shop_items` and `shop_discounts` when a player is at level 5:

```rust
let conn: DbConnection = connect_to_db();

let shop_items_subscription = conn
    .subscription_builder()
    .on_applied(|ctx| { /* handle applied state */ })
    .on_error(|error_ctx, error| { /* handle error */ })
    .subscribe([
        "SELECT * FROM shop_items WHERE required_level <= 5",
        "SELECT * FROM shop_discounts WHERE required_level <= 5",
    ]);
```

Later, when the player reaches level 6 and new items become available,
you can subscribe to the new queries and unsubscribe from the old ones:

```rust
let new_shop_items_subscription = conn
    .subscription_builder()
    .on_applied(|ctx| { /* handle applied state */ })
    .on_error(|error_ctx, error| { /* handle error */ })
    .subscribe([
        "SELECT * FROM shop_items WHERE required_level <= 6",
        "SELECT * FROM shop_discounts WHERE required_level <= 6",
    ]);

if shop_items_subscription.is_active() {
    shop_items_subscription
        .unsubscribe()
        .expect("Unsubscribing from shop_items failed");
}
```

All other subscriptions continue to remain in effect.
</TabItem>
</Tabs>

## Best Practices for Optimizing Server Compute and Reducing Serialization Overhead

### 1. Writing Efficient SQL Queries

For writing efficient SQL queries, see our [SQL Best Practices Guide](/reference/sql#best-practices-for-performance-and-scalability).

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
const conn = DbConnection.builder()
  .withUri('https://maincloud.spacetimedb.com')
  .withModuleName('my_module')
  .build();

// Never need to unsubscribe from global subscriptions
const globalSubscriptions = conn
  .subscriptionBuilder()
  .subscribe([
    // Global messages the client should always display
    'SELECT * FROM announcements',
    // A description of rewards for in-game achievements
    'SELECT * FROM badges',
  ]);

// May unsubscribe to shop_items as player advances
const shopSubscription = conn
  .subscriptionBuilder()
  .subscribe([
    'SELECT * FROM shop_items WHERE required_level <= 5',
  ]);
```

</TabItem>
<TabItem value="csharp" label="C#">

```cs
var conn = ConnectToDB();

// Never need to unsubscribe from global subscriptions
var globalSubscriptions = conn
    .SubscriptionBuilder()
    .Subscribe(new string[] {
        // Global messages the client should always display
        "SELECT * FROM announcements",
        // A description of rewards for in-game achievements
        "SELECT * FROM badges",
    });

// May unsubscribe to shop_items as player advances
var shopSubscription = conn
    .SubscriptionBuilder()
    .Subscribe(new string[] {
        "SELECT * FROM shop_items WHERE required_level <= 5"
    });
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let conn: DbConnection = connect_to_db();

// Never need to unsubscribe from global subscriptions
let global_subscriptions = conn
    .subscription_builder()
    .subscribe([
        // Global messages the client should always display
        "SELECT * FROM announcements",
        // A description of rewards for in-game achievements
        "SELECT * FROM badges",
    ]);

// May unsubscribe to shop_items as player advances
let shop_subscription = conn
    .subscription_builder()
    .subscribe([
        "SELECT * FROM shop_items WHERE required_level <= 5",
    ]);
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
const conn = DbConnection.builder()
  .withUri('https://maincloud.spacetimedb.com')
  .withModuleName('my_module')
  .build();

// Initial subscription: player at level 5.
const shopSubscription = conn
  .subscriptionBuilder()
  .subscribe([
    // For displaying the price of shop items in the player's currency of choice
    'SELECT * FROM exchange_rates',
    'SELECT * FROM shop_items WHERE required_level <= 5',
  ]);

// New subscription: player now at level 6, which overlaps with the previous query.
const newShopSubscription = conn
  .subscriptionBuilder()
  .subscribe([
    // For displaying the price of shop items in the player's currency of choice
    'SELECT * FROM exchange_rates',
    'SELECT * FROM shop_items WHERE required_level <= 6',
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
    .Subscribe(new string[] {
        // For displaying the price of shop items in the player's currency of choice
        "SELECT * FROM exchange_rates",
        "SELECT * FROM shop_items WHERE required_level <= 5"
    });

// New subscription: player now at level 6, which overlaps with the previous query.
var newShopSubscription = conn
    .SubscriptionBuilder()
    .Subscribe(new string[] {
        // For displaying the price of shop items in the player's currency of choice
        "SELECT * FROM exchange_rates",
        "SELECT * FROM shop_items WHERE required_level <= 6"
    });

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
    .subscribe([
        // For displaying the price of shop items in the player's currency of choice
        "SELECT * FROM exchange_rates",
        "SELECT * FROM shop_items WHERE required_level <= 5",
    ]);

// New subscription: player now at level 6, which overlaps with the previous query.
let new_shop_subscription = conn
    .subscription_builder()
    .subscribe([
        // For displaying the price of shop items in the player's currency of choice
        "SELECT * FROM exchange_rates",
        "SELECT * FROM shop_items WHERE required_level <= 6",
    ]);

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

Consider the following two queries:

```sql
SELECT * FROM User
SELECT * FROM User WHERE id = 5
```

If `User.id` is a unique or primary key column,
the cost of subscribing to both queries is minimal.
This is because the server will use an index when processing the 2nd query,
and it will only serialize a single row for the 2nd query.

In contrast, consider these two queries:

```sql
SELECT * FROM User
SELECT * FROM User WHERE id != 5
```

The server must now process each row of the `User` table twice,
since the 2nd query cannot be processed using an index.
It must also serialize all but one row of the `User` table twice,
due to the significant overlap between the two queries.

By following these best practices, you can optimize your data replication strategy and ensure your application remains efficient and responsive.
