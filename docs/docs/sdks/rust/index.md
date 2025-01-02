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
module_bindings::DbConnection
```

A connection to a remote database is represented by the `module_bindings::DbConnection` type. This type is generated per-module, and contains information about the types, tables and reducers defined by your module.

### Connect to a module - `DbConnection::builder()` and `.build()`

```rust
impl DbConnection {
    fn builder() -> DbConnectionBuilder;
}
```

Construct a `DbConnection` by calling `DbConnection::builder()` and chaining configuration methods, then calling `.build()`. You must at least specify `with_uri`, to supply the URI of the SpacetimeDB to which you published your module, and `with_module_name`, to supply the human-readable SpacetimeDB domain name or the raw address which identifies the module.

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
    fn with_module_name(self, name_or_address: impl ToString) -> Self;
}
```

Configure the SpacetimeDB domain name or address of the remote module which identifies it within the SpacetimeDB instance or cluster.

#### Callback `on_connect`

```rust
impl DbConnectionBuilder {
    fn on_connect(self, callback: impl FnOnce(&DbConnection, Identity, &str)) -> DbConnectionBuilder;
}
```

Chain a call to `.on_connect(callback)` to your builder to register a callback to run when your new `DbConnection` successfully initiates its connection to the remote module. The callback accepts three arguments: a reference to the `DbConnection`, the `Identity` by which SpacetimeDB identifies this connection, and a private access token which can be saved and later passed to [`with_credentials`](#method-with_credentials) to authenticate the same user in future connections.

This interface may change in an upcoming release as we rework SpacetimeDB's authentication model.

#### Callback `on_connect_error`

Currently unused.

#### Callback `on_disconnect`

```rust
impl DbConnectionBuilder {
    fn on_disconnect(self, callback: impl FnOnce(&DbConnection, Option<&anyhow::Error>)) -> DbConnectionBuilder;
}
```

Chain a call to `.on_connect(callback)` to your builder to register a callback to run when your `DbConnection` disconnects from the remote module, either as a result of a call to [`disconnect`](#method-disconnect) or due to an error.

#### Method `with_credentials`

```rust
impl DbConnectionBuilder {
    fn with_credentials(self, credentials: Option<(Identity, String)>) -> Self;
}
```

Chain a call to `.with_credentials(credentials)` to your builder to provide an `Identity` and private access token to authenticate with, or to explicitly select an anonymous connection. If this method is not called or `None` is passed, SpacetimeDB will generate a new `Identity` and sign a new private access token for the connection.

This interface may change in an upcoming release as we rework SpacetimeDB's authentication model.

#### Method `build`

```rust
impl DbConnectionBuilder {
    fn build(self) -> anyhow::Result<DbConnection>;
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
    async fn run_async(&self) -> anyhow::Result<()>;
}
```

`run_async` will continuously advance the connection, `await`-ing when there is no work to do. The task will return an `Err` if the connection disconnects erroneously, or return `Ok(())` if it disconnects as a result of a call to [`disconnect`](#method-disconnect).

#### Run on the main thread without blocking - method `frame_tick`

```rust
impl DbConnection {
    fn frame_tick(&self) -> anyhow::Result<()>;
}
```

`frame_tick` will advance the connection until no work remains, then return rather than blocking or `await`-ing. Games might arrange for this message to be called every frame. `frame_tick` returns `Ok` if the connection remains active afterwards, or `Err` if the connection disconnected before or during the call.

## Trait `DbContext`

[`DbConnection`](#type-dbconnection) and [`EventContext`](#type-eventcontext) both implement `DbContext`, which allows 

### Method `disconnect`

```rust
trait DbContext {
    fn disconnect(&self) -> anyhow::Result<()>;
}
```

Gracefully close the `DbConnection`. Returns an `Err` if the connection is already disconnected.

### Subscribe to queries - `DbContext::subscription_builder` and `.subscribe()`

This interface is subject to change in an upcoming SpacetimeDB release.

A known issue in the SpacetimeDB Rust SDK causes inconsistent behaviors after re-subscribing. This will be fixed in an upcoming SpacetimeDB release. For now, Rust clients should issue only one subscription per `DbConnection`.

```rust
trait DbContext {
    fn subscription_builder(&self) -> SubscriptionBuilder;
}
```

Subscribe to queries by calling `ctx.subscription_builder()` and chaining configuration methods, then calling `.subscribe(queries)`.

#### Callback `on_applied`

```rust
impl SubscriptionBuilder {
    fn on_applied(self, callback: impl FnOnce(&EventContext)) -> Self;
}
```

Register a callback to run when the subscription is applied and the matching rows are inserted into the client cache. The [`EventContext`](#type-eventcontext) passed to the callback will have `Event::SubscribeApplied` as its `event`.

#### Method `subscribe`

```rust
impl SubscriptionBuilder {
    fn subscribe(self, queries: impl IntoQueries) -> SubscriptionHandle;
}
```

Subscribe to a set of queries. `queries` should be an array or slice of strings.

The returned `SubscriptionHandle` is currently not useful, but will become significant in a future version of SpacetimeDB.

### Identity a client

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

An `EventContext` is a [`DbContext`](#trait-dbcontext) augmented with a field `event: Event`.

### Enum `Event`

```rust
spacetimedb_sdk::Event<module_bindings::Reducer>
```

#### Variant `Reducer`

```rust
spacetimedb_sdk::Event::Reducer(spacetimedb_sdk::ReducerEvent<module_bindings::Reducer>)
```

Event when we are notified that a reducer ran in the remote module. The [`ReducerEvent`](#struct-reducerevent) contains metadata about the reducer run, including its arguments and termination [`Status`](#enum-status).

This event is passed to reducer callbacks, and to row callbacks resulting from modifications by the reducer.

#### Variant `SubscribeApplied`

```rust
spacetimedb_sdk::Event::SubscribeApplied
```

Event when our subscription is applied and its rows are inserted into the client cache.

This event is passed to [subscription `on_applied` callbacks](#callback-on_applied), and to [row `on_insert` callbacks](#callback-on_insert) resulting from the new subscription.

#### Variant `UnsubscribeApplied`

Currently unused.

#### Variant `SubscribeError`

Currently unused.

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

    /// The `Address` of the SpacetimeDB actor which invoked the reducer,
    /// or `None` if the actor did not supply an address.
    caller_address: Option<Address>,

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

### Type `Address`

```rust
spacetimedb_sdk::Address
```

An opaque identifier for a client connection to a database, intended to differentiate between connections from the same [`Identity`](#type-identity). This will be removed in a future SpacetimeDB version in favor of a connection or session ID.
