# The SpacetimeDB Rust client SDK

The SpacetimeDB client SDK for Rust contains all the tools you need to build native clients for SpacetimeDB modules using Rust.

## Install the SDK

First, create a new project using `cargo new` and add the SpacetimeDB SDK to your dependencies:

```bash
cargo add spacetimedb_sdk
```

## Generate module bindings

Each SpacetimeDB client depends on some bindings specific to your module. Create a `module_bindings` directory in your project's `src` directory and generate the Rust interface files using the Spacetime CLI. From your project directory, run:

```bash
mkdir -p src/module_bindings
spacetime generate --lang rust \
    --out-dir src/module_bindings \
    --project-path PATH-TO-MODULE-DIRECTORY
```

Replace `PATH-TO-MODULE-DIRECTORY` with the path to your SpacetimeDB module.

Declare a `mod` for the bindings in your client's `src/main.rs`:

```rust
mod module_bindings;
```

## Type `DbConnection`

```rust
module_bindings::DbConnection {
    db: RemoteTables,
    /* private members */
}
```

A connection to a remote database is represented by the `module_bindings::DbConnection` type. This type is generated per-module, and contains information about the types, tables and reducers defined by your module.

### Connect to a module - `DbConnection::builder()` and `.build()`

```rust
impl DbConnection {
    fn builder() -> DbConnectionBuilder;
}
```

Construct a `DbConnection` by calling `DbConnection::builder()` and chaining configuration methods, then calling `.build()`. You must at least specify `with_uri`, to supply the URI of the SpacetimeDB to which you published your module, and `with_module_name`, to supply the human-readable SpacetimeDB domain name or the raw `Identity` which identifies the module.

#### Method `with_uri`

```rust
impl DbConnectionBuilder {
    fn with_uri(self, uri: impl TryInto<Uri>) -> Self;
}
```

Configure the URI of the SpacetimeDB instance or cluster which hosts the remote module.

#### Method `with_module_name`

```rust
impl DbConnectionBuilder {
    fn with_module_name(self, name_or_identity: impl ToString) -> Self;
}
```

Configure the SpacetimeDB domain name or `Identity` of the remote module which identifies it within the SpacetimeDB instance or cluster.

#### Callback `on_connect`

```rust
impl DbConnectionBuilder {
    fn on_connect(self, callback: impl FnOnce(&DbConnection, Identity, &str)) -> DbConnectionBuilder;
}
```

Chain a call to `.on_connect(callback)` to your builder to register a callback to run when your new `DbConnection` successfully initiates its connection to the remote module. The callback accepts three arguments: a reference to the `DbConnection`, the `Identity` by which SpacetimeDB identifies this connection, and a private access token which can be saved and later passed to [`with_token`](#method-with_token) to authenticate the same user in future connections.

This interface may change in an upcoming release as we rework SpacetimeDB's authentication model.

#### Callback `on_connect_error`

```rust
impl DbConnectionBuilder {
    fn on_connect_error(
        self,
        callback: impl FnOnce(&ErrorContext, spacetimedb_sdk::Error),
    ) -> DbConnectionBuilder;
}
```

Chain a call to `.on_connect_error(callback)` to your builder to register a callback to run when your connection fails.

A known bug in the SpacetimeDB Rust client SDK currently causes this callback never to be invoked. [`on_disconnect`](#callback-on_disconnect) callbacks are invoked instead.

#### Callback `on_disconnect`

```rust
impl DbConnectionBuilder {
    fn on_disconnect(
        self,
        callback: impl FnOnce(&ErrorContext, Option<spacetimedb_sdk::Error>),
    ) -> DbConnectionBuilder;
}
```

Chain a call to `.on_connect(callback)` to your builder to register a callback to run when your `DbConnection` disconnects from the remote module, either as a result of a call to [`disconnect`](#method-disconnect) or due to an error.

#### Method `with_token`

```rust
impl DbConnectionBuilder {
    fn with_token(self, token: Option<impl ToString>>) -> Self;
}
```

Chain a call to `.with_token(token)` to your builder to provide an OpenID Connect compliant JSON Web Token to authenticate with, or to explicitly select an anonymous connection. If this method is not called or `None` is passed, SpacetimeDB will generate a new `Identity` and sign a new private access token for the connection.

This interface may change in an upcoming release as we rework SpacetimeDB's authentication model.

#### Method `build`

```rust
impl DbConnectionBuilder {
    fn build(self) -> Result<DbConnection, spacetimedb_sdk::Error>;
}
```

After configuring the connection and registering callbacks, attempt to open the connection.

### Advance the connection and process messages

In the interest of supporting a wide variety of client applications with different execution strategies, the SpacetimeDB SDK allows you to choose when the `DbConnection` spends compute time and processes messages. If you do not arrange for the connection to advance by calling one of these methods, the `DbConnection` will never advance, and no callbacks will ever be invoked.

#### Run in the background - method `run_threaded`

```rust
impl DbConnection {
    fn run_threaded(&self) -> std::thread::JoinHandle<()>;
}
```

`run_threaded` spawns a thread which will continuously advance the connection, sleeping when there is no work to do. The thread will panic if the connection disconnects erroneously, or return if it disconnects as a result of a call to [`disconnect`](#method-disconnect).

#### Run asynchronously - method `run_async`

```rust
impl DbConnection {
    async fn run_async(&self) -> Result<(), spacetimedb_sdk::Error>;
}
```

`run_async` will continuously advance the connection, `await`-ing when there is no work to do. The task will return an `Err` if the connection disconnects erroneously, or return `Ok(())` if it disconnects as a result of a call to [`disconnect`](#method-disconnect).

#### Run on the main thread without blocking - method `frame_tick`

```rust
impl DbConnection {
    fn frame_tick(&self) -> Result<(), spacetimedb_sdk::Error>;
}
```

`frame_tick` will advance the connection until no work remains, then return rather than blocking or `await`-ing. Games might arrange for this message to be called every frame. `frame_tick` returns `Ok` if the connection remains active afterwards, or `Err` if the connection disconnected before or during the call.

### Access tables and reducers

#### Field `db`

```rust
struct DbConnection {
    pub db: RemoteTables,
    /* other members */
}
```

The `db` field of the `DbConnection` provides access to the subscribed view of the remote database's tables. See [Access the client cache](#access-the-client-cache).

#### Field `reducers`

```rust
struct DbConnection {
    pub reducers: RemoteReducers,
    /* other members */
}
```

The `reducers` field of the `DbConnection` provides access to reducers exposed by the remote module. See [Observe and invoke reducers](#observe-and-invoke-reducers).

## Trait `DbContext`

```rust
trait spacetimedb_sdk::DbContext {
    /* methods */
}
```

[`DbConnection`](#type-dbconnection), [`EventContext`](#type-eventcontext), [`ReducerEventContext`](#type-reducereventcontext), [`SubscriptionEventContext`](#type-subscriptioneventcontext) and [`ErrorContext`](#type-errorcontext) all implement `DbContext`. `DbContext` has methods for inspecting and configuring your connection to the remote database, including [`ctx.db()`](#method-db), a trait-generic alternative to reading the `.db` property on a concrete-typed context object.

The `DbContext` trait is implemented by connections and contexts to *every* module. This means that its [`DbView`](#method-db) and [`Reducers`](#method-reducers) are associated types.

### Trait `RemoteDbContext`

```rust
trait module_bindings::RemoteDbContext
    : spacetimedb_sdk::DbContext</* Associated type constraints */> {}
```

Each module's `module_bindings` exports a trait `RemoteDbContext` which extends `DbContext`, with the associated types `DbView` and `Reducers` bound to the types defined for that module. This can be more convenient when creating functions that can be called from any callback for a specific module, but which access the database or invoke reducers, and so must know the type of the `DbView` or `Reducers`.

### Method `db`

```rust
trait DbContext {
    fn db(&self) -> &Self::DbView;
}
```

When operating in trait-generic contexts, it is necessary to call the `ctx.db()` method, rather than accessing the `ctx.db` field, as Rust traits cannot expose fields.

#### Example

```rust
fn print_users(ctx: &impl RemoteDbContext) {
    for user in ctx.db().user().iter() {
        println!("{}", user.name);
    }
}
```

### Method `reducers`

```rust
trait DbContext {
    fn reducerrs(&self) -> &Self::Reducers;
}
```

When operating in trait-generic contexts, it is necessary to call the `ctx.reducers()` method, rather than accessing the `ctx.reducers` field, as Rust traits cannot expose fields.

#### Example

```rust
fn call_say_hello(ctx: &impl RemoteDbContext) {
    ctx.reducers.say_hello();
}
```

### Method `disconnect`

```rust
trait DbContext {
    fn disconnect(&self) -> spacetimedb_sdk::Result<()>;
}
```

Gracefully close the `DbConnection`. Returns an `Err` if the connection is already disconnected.

### Subscribe to queries

#### Type `SubscriptionBuilder`

```rust
spacetimedb_sdk::SubscriptionBuilder
```

##### Constructor `ctx.subscription_builder()`

```rust
trait DbContext {
    fn subscription_builder(&self) -> SubscriptionBuilder;
}
```

Subscribe to queries by calling `ctx.subscription_builder()` and chaining configuration methods, then calling `.subscribe(queries)`.

##### Callback `on_applied`

```rust
impl SubscriptionBuilder {
    fn on_applied(self, callback: impl FnOnce(&SubscriptionEventContext)) -> Self;
}
```

Register a callback to run when the subscription is applied and the matching rows are inserted into the client cache.

##### Callback `on_error`

```rust
impl SubscriptionBuilder {
    fn on_applied(self, callback: impl FnOnce(&ErrorContext, spacetimedb_sdk::Error)) -> Self;
}
```

Register a callback to run if the subscription is rejected or unexpectedly terminated by the server. This is most frequently caused by passing an invalid query to [`subscribe`](#method-subscribe).


##### Method `subscribe`

```rust
impl SubscriptionBuilder {
    fn subscribe(self, queries: impl IntoQueries) -> SubscriptionHandle;
}
```

Subscribe to a set of queries. `queries` should be a string or an array, vec or slice of strings.

##### Method `subscribe_to_all_tables`

```rust
impl SubscriptionBuilder {
    fn subscribe_to_all_tables(self);
}
```

Subscribe to all rows from all tables. This method is provided as a convenience for simple clients. The same connection must not mix `subscribe_to_all_tables` with [`subscribe` to specific queries](#method subscribe). Doing so may cause errors or corrupt the client cache, leading to local queries returning incorrect results. The subscription initiated by `subscribe_to_all_tables` cannot be canceled after it is initiated.

#### Type `SubscriptionHandle`

```rust
module_bindings::SubscriptionHandle
```

A `SubscriptionHandle` represents a subscribed query or a group of subscribed queries.

The `SubscriptionHandle` does not contain or provide access to the subscribed rows. Subscribed rows of all subscriptions by a connection are contained within that connection's [`ctx.db`](#field-db). See [Access the client cache](#access-the-client-cache).

##### Method `is_ended`

```rust
impl SubscriptionHandle {
    fn is_ended(&self) -> bool;
}
```

Returns true if this subscription has been terminated due to an unsubscribe call or an error.

##### Method `is_active`

```rust
impl SubscriptionHandle {
    fn is_active(&self) -> bool;
}
```

Returns true if this subscription has been applied and has not yet been unsubscribed.

##### Method `unsubscribe`

```rust
impl SubscriptionHandle {
    fn unsubscribe(&self) -> Result<(), spacetimedb_sdk::Error>;
}
```

Terminate this subscription, causing matching rows to be removed from the client cache. Any rows removed from the client cache this way will have [`on_delete` callbacks](#callback-on_delete) run for them.

Unsubscribing is an asynchronous operation. Matching rows are not removed from the client cache immediately. Use [`unsubscribe_then`](#method-unsubscribe_then) to run a callback once the unsubscribe operation is completed.

Returns an error if the subscription has already ended, either due to a previous call to `unsubscribe` or [`unsubscribe_then`](#method-unsubscribe_then), or due to an error.

##### Method `unsubscribe_then`

```rust
impl SubscriptionHandle {
    fn unsubscribe_then(
        self,
        on_end: impl FnOnce(&SubscriptionEventContext),
    ) -> Result<(), spacetimedb_sdk::Error>;
}
```

Terminate this subscription, and run the `on_end` callback when the subscription is ended and its matching rows are removed from the client cache. Any rows removed from the client cache this way will have [`on_delete` callbacks](#callback-on_delete) run for them.

Returns an error if the subscription has already ended, either due to a previous call to `unsubscribe` or [`unsubscribe_then`](#method-unsubscribe_then), or due to an error.

### Identify a client

#### Method `identity`

```rust
trait DbContext {
    fn identity(&self) -> Identity;
}
```

Get the `Identity` with which SpacetimeDB identifies the connection. This method may panic if the connection was initiated anonymously and the newly-generated `Identity` has not yet been received, i.e. if called before the [`on_connect` callback](#callback-on_connect) is invoked.

#### Method `try_identity`

```rust
trait DbContext {
    fn try_identity(&self) -> Option<Identity>;
}
```

Like [`DbContext::identity`](#method-identity), but returns `None` instead of panicking if the `Identity` is not yet available.

#### Method `connection_id`

```rust
trait DbContext {
    fn connection_id(&self) -> ConnectionId;
}
```

Get the [`ConnectionId`](#type-connectionid) with which SpacetimeDB identifies the connection.

#### Method `is_active`

```rust
trait DbContext {
    fn is_active(&self) -> bool;
}
```

`true` if the connection has not yet disconnected. Note that a connection `is_active` when it is constructed, before its [`on_connect` callback](#callback-on_connect) is invoked.

## Type `EventContext`

```rust
module_bindings::EventContext
```

An `EventContext` is a [`DbContext`](#trait-dbcontext) augmented with a field [`event: Event`](#enum-event). `EventContext`s are passed as the first argument to row callbacks [`on_insert`](#callback-on_insert), [`on_delete`](#callback-on_delete) and [`on_update`](#callback-on_update).

### Field `event`

```rust
struct EventContext {
    pub event: spacetimedb_sdk::Event<module_bindings::Reducer>,
    /* other fields */
}
```

TODO

### Field `db`

```rust
struct EventContext {
    pub db: RemoteTables,
    /* other members */
}
```

The `db` field of the context provides access to the subscribed view of the remote database's tables. See [Access the client cache](#access-the-client-cache).

### Field `reducers`

```rust
struct EventContext {
    pub reducers: RemoteReducers,
    /* other members */
}
```

The `reducers` field of the context provides access to reducers exposed by the remote module. See [Observe and invoke reducers](#observe-and-invoke-reducers).

### Enum `Event`

```rust
spacetimedb_sdk::Event<module_bindings::Reducer>
```

#### Variant `Reducer`

```rust
spacetimedb_sdk::Event::Reducer(spacetimedb_sdk::ReducerEvent<module_bindings::Reducer>)
```

Event when we are notified that a reducer ran in the remote module. The [`ReducerEvent`](#struct-reducerevent) contains metadata about the reducer run, including its arguments and termination [`Status`](#enum-status).

This event is passed to row callbacks resulting from modifications by the reducer.

#### Variant `SubscribeApplied`

```rust
spacetimedb_sdk::Event::SubscribeApplied
```

Event when our subscription is applied and its rows are inserted into the client cache.

This event is passed to [row `on_insert` callbacks](#callback-on_insert) resulting from the new subscription.

#### Variant `UnsubscribeApplied`

TODO

#### Variant `SubscribeError`

TODO

#### Variant `UnknownTransaction`

Event when we are notified of a transaction in the remote module which we cannot associate with a known reducer. This may be an ad-hoc SQL query or a reducer for which we do not have bindings.

This event is passed to row callbacks resulting from modifications by the transaction.

### Struct `ReducerEvent`

```rust
spacetimedb_sdk::ReducerEvent<module_bindings::Reducer>
```

A `ReducerEvent` contains metadata about a reducer run.

```rust
struct spacetimedb_sdk::ReducerEvent<R> {
    /// The time at which the reducer was invoked.
    timestamp: SystemTime,

    /// Whether the reducer committed, was aborted due to insufficient energy, or failed with an error message.
    status: Status,

    /// The `Identity` of the SpacetimeDB actor which invoked the reducer.
    caller_identity: Identity,

    /// The `ConnectionId` of the SpacetimeDB actor which invoked the reducer,
    /// or `None` for scheduled reducers.
    caller_connection_id: Option<ConnectionId>,

    /// The amount of energy consumed by the reducer run, in eV.
    /// (Not literal eV, but our SpacetimeDB energy unit eV.)
    ///
    /// May be `None` if the module is configured not to broadcast energy consumed.
    energy_consumed: Option<u128>,

    /// The `Reducer` enum defined by the `module_bindings`, which encodes which reducer ran and its arguments.
    reducer: R,

    // ...private fields
}
```

### Enum `Status`

```rust
spacetimedb_sdk::Status
```

#### Variant `Committed`

```rust
spacetimedb_sdk::Status::Committed
```

The reducer returned successfully and its changes were committed into the database state. An [`Event::Reducer`](#variant-reducer) passed to a row callback must have this status in its [`ReducerEvent`](#struct-reducerevent).

#### Variant `Failed`

```rust
spacetimedb_sdk::Status::Failed(Box<str>)
```

The reducer returned an error, panicked, or threw an exception. The enum payload is the stringified error message. Formatting of the error message is unstable and subject to change, so clients should use it only as a human-readable diagnostic, and in particular should not attempt to parse the message.

#### Variant `OutOfEnergy`

The reducer was aborted due to insufficient energy balance of the module owner.

### Enum `Reducer`

```rust
module_bindings::Reducer
```

The module bindings contains an enum `Reducer` with a variant for each reducer defined by the module. Each variant has a payload containing the arguments to the reducer.

## Type `ReducerEventContext`

A `ReducerEventContext` is a [`DbContext`](#trait-dbcontext) augmented with a field [`event: ReducerEvent`](#struct-reducerevent). `ReducerEventContext`s are passed as the first argument to [reducer callbacks](#observe-and-invoke-reducers).

### Field `event`

```rust
struct ReducerEventContext {
    pub event: spacetimedb_sdk::ReducerEvent<module_bindings::Reducer>,
    /* other fields */
}
```

TODO

### Field `db`

```rust
struct ReducerEventContext {
    pub db: RemoteTables,
    /* other members */
}
```

The `db` field of the context provides access to the subscribed view of the remote database's tables. See [Access the client cache](#access-the-client-cache).

### Field `reducers`

```rust
struct ReducerEventContext {
    pub reducers: RemoteReducers,
    /* other members */
}
```

The `reducers` field of the context provides access to reducers exposed by the remote module. See [Observe and invoke reducers](#observe-and-invoke-reducers).

## Type `SubscriptionEventContext`

A `SubscriptionEventContext` is a [`DbContext`](#trait-dbcontext). Unlike the other context types, `SubscriptionEventContext` doesn't have an `event` field. `SubscriptionEventContext`s are passed to subscription [`on_applied`](#callback-on_applied) and [`unsubscribe_then`](#method-unsubscribe_then) callbacks.

### Field `db`

```rust
struct SubscriptionEventContext {
    pub db: RemoteTables,
    /* other members */
}
```

The `db` field of the context provides access to the subscribed view of the remote database's tables. See [Access the client cache](#access-the-client-cache).

### Field `reducers`

```rust
struct SubscriptionEventContext {
    pub reducers: RemoteReducers,
    /* other members */
}
```

The `reducers` field of the context provides access to reducers exposed by the remote module. See [Observe and invoke reducers](#observe-and-invoke-reducers).

## Type `ErrorContext`

An `ErrorContext` is a [`DbContext`](#trait-dbcontext) augmented with a field `event: spacetimedb_sdk::Error`. `ErrorContext`s are to connections' [`on_disconnect`](#callback-on_disconnect) and [`on_connect_error`](#callback-on_connect_error) callbacks, and to subscriptions' [`on_error`](#callback-on_error) callbacks.

### Field `event`

```rust
struct ErrorContext {
    pub event: spacetimedb_sdk::Error,
    /* other fields */
}
```

TODO

### Field `db`

```rust
struct ErrorContext {
    pub db: RemoteTables,
    /* other members */
}
```

The `db` field of the context provides access to the subscribed view of the remote database's tables. See [Access the client cache](#access-the-client-cache).

### Field `reducers`

```rust
struct ErrorContext {
    pub reducers: RemoteReducers,
    /* other members */
}
```

The `reducers` field of the context provides access to reducers exposed by the remote module. See [Observe and invoke reducers](#observe-and-invoke-reducers).

## Access the client cache

Both [`DbConnection`](#type-dbconnection) and [`EventContext`](#type-eventcontext) have fields `.db`, which in turn has methods for accessing tables in the client cache. The trait method `DbContext::db(&self)` can also be used in contexts with an `impl DbContext` rather than a concrete-typed `EventContext` or `DbConnection`.

Each table defined by a module has an accessor method, whose name is the table name converted to `snake_case`, on this `.db` field. The methods are defined via extension traits, which `rustc` or your IDE should help you identify and import where necessary. The table accessor methods return table handles, which implement [`Table`](#trait-table), may implement [`TableWithPrimaryKey`](#trait-tablewithprimarykey), and have methods for searching by unique index.

### Trait `Table`

```rust
spacetimedb_sdk::Table
```

Implemented by all table handles.

#### Associated type `Row`

```rust
trait spacetimedb_sdk::Table {
    type Table::Row;
}
```

The type of rows in the table.

#### Method `count`

```rust
trait spacetimedb_sdk::Table {
    fn count(&self) -> u64;
}
```

Returns the number of rows of this table resident in the client cache, i.e. the total number which match any subscribed query.

#### Method `iter`

```rust
trait spacetimedb_sdk::Table {
    fn iter(&self) -> impl Iterator<Item = Self::Row>;
}
```

An iterator over all the subscribed rows in the client cache, i.e. those which match any subscribed query.

#### Callback `on_insert`

```rust
trait spacetimedb_sdk::Table {
    type InsertCallbackId;
    
    fn on_insert(&self, callback: impl FnMut(&EventContext, &Self::Row)) -> Self::InsertCallbackId;

    fn remove_on_insert(&self, callback: Self::InsertCallbackId);
}
```

The `on_insert` callback runs whenever a new row is inserted into the client cache, either when applying a subscription or being notified of a transaction. The passed [`EventContext`](#type-eventcontext) contains an [`Event`](#enum-event) which can identify the change which caused the insertion, and also allows the callback to interact with the connection, inspect the client cache and invoke reducers.

Registering an `on_insert` callback returns a callback id, which can later be passed to `remove_on_insert` to cancel the callback. Newly registered or canceled callbacks do not take effect until the following event.

#### Callback `on_delete`

```rust
trait spacetimedb_sdk::Table {
    type DeleteCallbackId;
    
    fn on_delete(&self, callback: impl FnMut(&EventContext, &Self::Row)) -> Self::DeleteCallbackId;

    fn remove_on_delete(&self, callback: Self::DeleteCallbackId);
}
```

The `on_delete` callback runs whenever a previously-resident row is deleted from the client cache. Registering an `on_delete` callback returns a callback id, which can later be passed to `remove_on_delete` to cancel the callback. Newly registered or canceled callbacks do not take effect until the following event.

### Trait `TableWithPrimaryKey`

```rust
spacetimedb_sdk::TableWithPrimaryKey
```

Implemented for table handles whose tables have a primary key.

#### Callback `on_delete`

```rust
trait spacetimedb_sdk::TableWithPrimaryKey {
    type UpdateCallbackId;
    
    fn on_update(&self, callback: impl FnMut(&EventContext, &Self::Row, &Self::Row)) -> Self::UpdateCallbackId;

    fn remove_on_update(&self, callback: Self::UpdateCallbackId);
}
```

The `on_update` callback runs whenever an already-resident row in the client cache is updated, i.e. replaced with a new row that has the same primary key. Registering an `on_update` callback returns a callback id, which can later be passed to `remove_on_update` to cancel the callback. Newly registered or canceled callbacks do not take effect until the following event.

### Unique constraint index access

For each unique constraint on a table, its table handle has a method whose name is the unique column name which returns a unique index handle. The unique index handle has a method `.find(desired_val: &Col) -> Option<Row>`, where `Col` is the type of the column, and `Row` the type of rows. If a row with `desired_val` in the unique column is resident in the client cache, `.find` returns it.

### BTree index access

Not currently implemented in the Rust SDK. Coming soon!

## Observe and invoke reducers

Both [`DbConnection`](#type-dbconnection) and [`EventContext`](#type-eventcontext) have fields `.reducers`, which in turn has methods for invoking reducers defined by the module and registering callbacks on it. The trait method `DbContext::reducers(&self)` can also be used in contexts with an `impl DbContext` rather than a concrete-typed `EventContext` or `DbConnection`.

Each reducer defined by the module has three methods on the `.reducers`:

- An invoke method, whose name is the reducer's name converted to snake case. This requests that the module run the reducer.
- A callback registation method, whose name is prefixed with `on_`. This registers a callback to run whenever we are notified that the reducer ran, including successfully committed runs and runs we requested which failed. This method returns a callback id, which can be passed to the callback remove method.
- A callback remove method, whose name is prefixed with `remove_`. This cancels a callback previously registered via the callback registration method.

## Identify a client

### Type `Identity`

```rust
spacetimedb_sdk::Identity
```

A unique public identifier for a client connected to a database.

### Type `ConnectionId`

```rust
spacetimedb_sdk::ConnectionId
```

An opaque identifier for a client connection to a database, intended to differentiate between connections from the same [`Identity`](#type-identity).
