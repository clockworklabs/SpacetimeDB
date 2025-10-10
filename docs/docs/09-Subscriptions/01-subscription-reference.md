---
title: Subscription Reference
slug: /subscriptions
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# The SpacetimeDB Subscription API

The subscription API allows a client to replicate a subset of a database.
It does so by registering SQL queries, which we call subscriptions, through a database connection.
A client will only receive updates for rows that match the subscriptions it has registered.

For more information on syntax and requirements see the [SQL docs](/sql#subscriptions).

This guide describes the two main interfaces that comprise the API - `SubscriptionBuilder` and `SubscriptionHandle`.
By using these interfaces, you can create efficient and responsive client applications that only receive the data they need.

## SubscriptionBuilder

<Tabs groupId="server-language" defaultValue="rust">
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
    /// <a href="https://spacetimedb.com/docs/sql">https://spacetimedb.com/docs/sql</a>
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
</Tabs>

A `SubscriptionBuilder` provides an interface for registering subscription queries with a database.
It allows you to register callbacks that run when the subscription is successfully applied or when an error occurs.
Once applied, a client will start receiving row updates to its client cache.
A client can react to these updates by registering row callbacks for the appropriate table.

### Example Usage

<Tabs groupId="server-language" defaultValue="rust">
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
</Tabs>

## SubscriptionHandle

<Tabs groupId="server-language" defaultValue="rust">
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
</Tabs>

When you register a subscription, you receive a `SubscriptionHandle`.
A `SubscriptionHandle` manages the lifecycle of each subscription you register.
In particular, it provides methods to check the status of the subscription and to unsubscribe if necessary.
Because each subscription has its own independently managed lifetime,
clients can dynamically subscribe to different subsets of the database as their application requires.

### Example Usage

<Tabs groupId="server-language" defaultValue="rust">
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
</Tabs>

## Best Practices for Optimizing Server Compute and Reducing Serialization Overhead

### 1. Writing Efficient SQL Queries

For writing efficient SQL queries, see our [SQL Best Practices Guide](/sql#best-practices-for-performance-and-scalability).

### 2. Group Subscriptions with the Same Lifetime Together

Subscriptions with the same lifetime should be grouped together.

For example, you may have certain data that is required for the lifetime of your application,
but you may have other data that is only sometimes required by your application.

By managing these sets as two independent subscriptions,
your application can subscribe and unsubscribe from the latter,
without needlessly unsubscribing and resubscribing to the former.

This will improve throughput by reducing the amount of data transferred from the database to your application.

#### Example

<Tabs groupId="server-language" defaultValue="rust">
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
</Tabs>

### 3. Subscribe Before Unsubscribing

If you want to update or modify a subscription by dropping it and subscribing to a new set,
you should subscribe to the new set before unsubscribing from the old one.

This is because SpacetimeDB subscriptions are zero-copy.
Subscribing to the same query more than once doesn't incur additional processing or serialization overhead.
Likewise, if a query is subscribed to more than once,
unsubscribing from it does not result in any server processing or data serializtion.

#### Example

<Tabs groupId="server-language" defaultValue="rust">
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
