---
title: C# Reference
toc_max_heading_level: 6
slug: /sdks/c-sharp
---

# The SpacetimeDB C# client SDK

The SpacetimeDB client for C# contains all the tools you need to build native clients for SpacetimeDB modules using C#.

| Name                                                              | Description                                                                                             |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| [Project setup](#project-setup)                                   | Configure a C# project to use the SpacetimeDB C# client SDK.                                            |
| [Generate module bindings](#generate-module-bindings)             | Use the SpacetimeDB CLI to generate module-specific types and interfaces.                               |
| [`DbConnection` type](#type-dbconnection)                         | A connection to a remote database.                                                                      |
| [`IDbContext` interface](#interface-idbcontext)                   | Methods for interacting with the remote database.                                                       |
| [`EventContext` type](#type-eventcontext)                         | Implements [`IDbContext`](#interface-idbcontext) for [row callbacks](#callback-oninsert).               |
| [`ReducerEventContext` type](#type-reducereventcontext)           | Implements [`IDbContext`](#interface-idbcontext) for [reducer callbacks](#observe-and-invoke-reducers). |
| [`SubscriptionEventContext` type](#type-subscriptioneventcontext) | Implements [`IDbContext`](#interface-idbcontext) for [subscription callbacks](#subscribe-to-queries).   |
| [`ErrorContext` type](#type-errorcontext)                         | Implements [`IDbContext`](#interface-idbcontext) for error-related callbacks.                           |
| [Access the client cache](#access-the-client-cache)               | Access to your local view of the database.                                                              |
| [Observe and invoke reducers](#observe-and-invoke-reducers)       | Send requests to the database to run reducers, and register callbacks to run when notified of reducers. |
| [Identify a client](#identify-a-client)                           | Types for identifying users and client connections.                                                     |

## Project setup

### Using the `dotnet` CLI tool

If you would like to create a console application using .NET, you can create a new project using `dotnet new console` and add the SpacetimeDB SDK to your dependencies:

```bash
dotnet add package SpacetimeDB.ClientSDK
```

(See also the [CSharp Quickstart](/modules/c-sharp/quickstart) for an in-depth example of such a console application.)

### Using Unity

Add the SpacetimeDB Unity Package using the Package Manager. Open the Package Manager window by clicking on Window -> Package Manager. Click on the + button in the top left corner of the window and select "Add package from git URL". Enter the following URL and click Add.

```bash
https://github.com/clockworklabs/com.clockworklabs.spacetimedbsdk.git
```

(See also the [Unity Tutorial](/unity/part-1))

## Generate module bindings

Each SpacetimeDB client depends on some bindings specific to your module. Create a `module_bindings` directory in your project's directory and generate the C# interface files using the Spacetime CLI. From your project directory, run:

```bash
mkdir -p module_bindings
spacetime generate --lang cs --out-dir module_bindings --project-path PATH-TO-MODULE-DIRECTORY
```

Replace `PATH-TO-MODULE-DIRECTORY` with the path to your SpacetimeDB module.

## Type `DbConnection`

A connection to a remote database is represented by the `DbConnection` class. This class is generated per module and contains information about the types, tables, and reducers defined by your module.

| Name                                                                   | Description                                                                   |
| ---------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| [Connect to a database](#connect-to-a-database)                        | Construct a `DbConnection` instance.                                          |
| [Advance the connection](#advance-the-connection-and-process-messages) | Poll the `DbConnection` or run it in the background.                          |
| [Access tables and reducers](#access-tables-and-reducers)              | Access the client cache, request reducer invocations, and register callbacks. |

### Connect to a database

```csharp
class DbConnection
{
    public static DbConnectionBuilder<DbConnection> Builder();
}
```

Construct a `DbConnection` by calling `DbConnection.Builder()`, chaining configuration methods, and finally calling `.Build()`. At a minimum, you must specify `WithUri` to provide the URI of the SpacetimeDB instance, and `WithModuleName` to specify the database's name or identity.

| Name                                                | Description                                                                          |
| --------------------------------------------------- | ------------------------------------------------------------------------------------ |
| [WithUri method](#method-withuri)                   | Set the URI of the SpacetimeDB instance hosting the remote database.                 |
| [WithModuleName method](#method-withmodulename)     | Set the name or identity of the remote database.                                     |
| [OnConnect callback](#callback-onconnect)           | Register a callback to run when the connection is successfully established.          |
| [OnConnectError callback](#callback-onconnecterror) | Register a callback to run if the connection is rejected or the host is unreachable. |
| [OnDisconnect callback](#callback-ondisconnect)     | Register a callback to run when the connection ends.                                 |
| [WithToken method](#method-withtoken)               | Supply a token to authenticate with the remote database.                             |
| [Build method](#method-build)                       | Finalize configuration and open the connection.                                      |

#### Method `WithUri`

```csharp
class DbConnectionBuilder<DbConnection>
{
    public DbConnectionBuilder<DbConnection> WithUri(Uri uri);
}
```

Configure the URI of the SpacetimeDB instance or cluster which hosts the remote module and database.

#### Method `WithModuleName`

```csharp
class DbConnectionBuilder
{
    public DbConnectionBuilder<DbConnection> WithModuleName(string nameOrIdentity);
}
```

Configure the SpacetimeDB domain name or `Identity` of the remote database which identifies it within the SpacetimeDB instance or cluster.

#### Callback `OnConnect`

```csharp
class DbConnectionBuilder<DbConnection>
{
    public DbConnectionBuilder<DbConnection> OnConnect(Action<DbConnection, Identity, string> callback);
}
```

Chain a call to `.OnConnect(callback)` to your builder to register a callback to run when your new `DbConnection` successfully initiates its connection to the remote database. The callback accepts three arguments: a reference to the `DbConnection`, the `Identity` by which SpacetimeDB identifies this connection, and a private access token which can be saved and later passed to [`WithToken`](#method-withtoken) to authenticate the same user in future connections.

#### Callback `OnConnectError`

```csharp
class DbConnectionBuilder<DbConnection>
{
    public DbConnectionBuilder<DbConnection> OnConnectError(Action<ErrorContext, SpacetimeDbException> callback);
}
```

Chain a call to `.OnConnectError(callback)` to your builder to register a callback to run when your connection fails.

A known bug in the SpacetimeDB Rust client SDK currently causes this callback never to be invoked. [`OnDisconnect`](#callback-ondisconnect) callbacks are invoked instead.

#### Callback `OnDisconnect`

```csharp
class DbConnectionBuilder<DbConnection>
{
    public DbConnectionBuilder<DbConnection> OnDisconnect(Action<ErrorContext, SpacetimeDbException> callback);
}
```

Chain a call to `.OnDisconnect(callback)` to your builder to register a callback to run when your `DbConnection` disconnects from the remote database, either as a result of a call to [`Disconnect`](#method-disconnect) or due to an error.

#### Method `WithToken`

```csharp
class DbConnectionBuilder<DbConnection>
{
    public DbConnectionBuilder<DbConnection> WithToken(string token = null);
}
```

Chain a call to `.WithToken(token)` to your builder to provide an OpenID Connect compliant JSON Web Token to authenticate with, or to explicitly select an anonymous connection. If this method is not called or `None` is passed, SpacetimeDB will generate a new `Identity` and sign a new private access token for the connection.

#### Method `Build`

```csharp
class DbConnectionBuilder<DbConnection>
{
    public DbConnection Build();
}
```

After configuring the connection and registering callbacks, attempt to open the connection.

### Advance the connection and process messages

In the interest of supporting a wide variety of client applications with different execution strategies, the SpacetimeDB SDK allows you to choose when the `DbConnection` spends compute time and processes messages. If you do not arrange for the connection to advance by calling one of these methods, the `DbConnection` will never advance, and no callbacks will ever be invoked.

| Name                                    | Description                                           |
| --------------------------------------- | ----------------------------------------------------- |
| [`FrameTick` method](#method-frametick) | Process messages on the main thread without blocking. |

#### Method `FrameTick`

```csharp
class DbConnection {
    public void FrameTick();
}
```

`FrameTick` will advance the connection until no work remains or until it is disconnected, then return rather than blocking. Games might arrange for this message to be called every frame.

It is not advised to run `FrameTick` on a background thread, since it modifies [`dbConnection.Db`](#property-db). If main thread code is also accessing the `Db`, it may observe data races when `FrameTick` runs on another thread.

(Note that the SDK already does most of the work for parsing messages on a background thread. `FrameTick()` does the minimal amount of work needed to apply updates to the `Db`.)

### Access tables and reducers

#### Property `Db`

```csharp
class DbConnection
{
    public RemoteTables Db;
    /* other members */
}
```

The `Db` property of the `DbConnection` provides access to the subscribed view of the remote database's tables. See [Access the client cache](#access-the-client-cache).

#### Property `Reducers`

```csharp
class DbConnection
{
    public RemoteReducers Reducers;
    /* other members */
}
```

The `Reducers` field of the `DbConnection` provides access to reducers exposed by the module of the remote database. See [Observe and invoke reducers](#observe-and-invoke-reducers).

### Interface `IDbContext`

```csharp
interface IDbContext<DbView, RemoteReducers, ..>
{
    /* methods */
}
```

[`DbConnection`](#type-dbconnection), [`EventContext`](#type-eventcontext), [`ReducerEventContext`](#type-reducereventcontext), [`SubscriptionEventContext`](#type-subscriptioneventcontext) and [`ErrorContext`](#type-errorcontext) all implement `IDbContext`. `IDbContext` has methods for inspecting and configuring your connection to the remote database.

The `IDbContext` interface is implemented by connections and contexts to _every_ module - hence why it takes [`DbView`](#method-db) and [`RemoteReducers`](#method-reducers) as type parameters.

| Name                                                        | Description                                                             |
| ----------------------------------------------------------- | ----------------------------------------------------------------------- |
| [`IRemoteDbContext` interface](#interface-iremotedbcontext) | Module-specific `IDbContext`.                                           |
| [`Db` method](#method-db)                                   | Provides access to the subscribed view of the remote database's tables. |
| [`Reducers` method](#method-reducers)                       | Provides access to reducers exposed by the remote module.               |
| [`Disconnect` method](#method-disconnect)                   | End the connection.                                                     |
| [Subscribe to queries](#subscribe-to-queries)               | Register SQL queries to receive updates about matching rows.            |
| [Read connection metadata](#read-connection-metadata)       | Access the connection's `Identity` and `ConnectionId`                   |

### Interface `IRemoteDbContext`

Each module's `module_bindings` exports an interface `IRemoteDbContext` which inherits from `IDbContext`, with the type parameters `DbView` and `RemoteReducers` bound to the types defined for that module. This can be more convenient when creating functions that can be called from any callback for a specific module, but which access the database or invoke reducers, and so must know the type of the `DbView` or `Reducers`.

#### Method `Db`

```csharp
interface IRemoteDbContext
{
    public DbView Db { get; }
}
```

`Db` will have methods to access each table defined by the module.

##### Example

```csharp
var conn = ConnectToDB();

// Get a handle to the User table
var tableHandle = conn.Db.User;
```

#### Method `Reducers`

```csharp
interface IRemoteDbContext
{
    public RemoteReducers Reducers { get; }
}
```

`Reducers` will have methods to invoke each reducer defined by the module,
plus methods for adding and removing callbacks on each of those reducers.

##### Example

```csharp
var conn = ConnectToDB();

// Register a callback to be run every time the SendMessage reducer is invoked
conn.Reducers.OnSendMessage += Reducer_OnSendMessageEvent;
```

#### Method `Disconnect`

```csharp
interface IRemoteDbContext
{
    public void Disconnect();
}
```

Gracefully close the `DbConnection`. Throws an error if the connection is already closed.

### Subscribe to queries

| Name                                                    | Description                                                 |
| ------------------------------------------------------- | ----------------------------------------------------------- |
| [`SubscriptionBuilder` type](#type-subscriptionbuilder) | Builder-pattern constructor to register subscribed queries. |
| [`SubscriptionHandle` type](#type-subscriptionhandle)   | Manage an active subscripion.                               |

#### Type `SubscriptionBuilder`

| Name                                                                           | Description                                                     |
| ------------------------------------------------------------------------------ | --------------------------------------------------------------- |
| [`ctx.SubscriptionBuilder()` constructor](#constructor-ctxsubscriptionbuilder) | Begin configuring a new subscription.                           |
| [`OnApplied` callback](#callback-onapplied)                                    | Register a callback to run when matching rows become available. |
| [`OnError` callback](#callback-onerror)                                        | Register a callback to run if the subscription fails.           |
| [`Subscribe` method](#method-subscribe)                                        | Finish configuration and subscribe to one or more SQL queries.  |
| [`SubscribeToAllTables` method](#method-subscribetoalltables)                  | Convenience method to subscribe to the entire database.         |

##### Constructor `ctx.SubscriptionBuilder()`

```csharp
interface IRemoteDbContext
{
    public SubscriptionBuilder SubscriptionBuilder();
}
```

Subscribe to queries by calling `ctx.SubscriptionBuilder()` and chaining configuration methods, then calling `.Subscribe(queries)`.

##### Callback `OnApplied`

```csharp
class SubscriptionBuilder
{
    public SubscriptionBuilder OnApplied(Action<SubscriptionEventContext> callback);
}
```

Register a callback to run when the subscription is applied and the matching rows are inserted into the client cache.

##### Callback `OnError`

```csharp
class SubscriptionBuilder
{
    public SubscriptionBuilder OnError(Action<ErrorContext, Exception> callback);
}
```

Register a callback to run if the subscription is rejected or unexpectedly terminated by the server. This is most frequently caused by passing an invalid query to [`Subscribe`](#method-subscribe).

##### Method `Subscribe`

```csharp
class SubscriptionBuilder
{
    public SubscriptionHandle Subscribe(string[] querySqls);
}
```

Subscribe to a set of queries. `queries` should be an array of SQL query strings.

See [the SpacetimeDB SQL Reference](/sql#subscriptions) for information on the queries SpacetimeDB supports as subscriptions.

##### Method `SubscribeToAllTables`

```csharp
class SubscriptionBuilder
{
    public void SubscribeToAllTables();
}
```

Subscribe to all rows from all public tables. This method is provided as a convenience for simple clients. The subscription initiated by `SubscribeToAllTables` cannot be canceled after it is initiated. You should [`subscribe` to specific queries](#method-subscribe) if you need fine-grained control over the lifecycle of your subscriptions.

#### Type `SubscriptionHandle`

A `SubscriptionHandle` represents a subscribed query or a group of subscribed queries.

The `SubscriptionHandle` does not contain or provide access to the subscribed rows. Subscribed rows of all subscriptions by a connection are contained within that connection's [`ctx.Db`](#property-db). See [Access the client cache](#access-the-client-cache).

| Name                                                | Description                                                                                                      |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| [`IsEnded` property](#property-isended)             | Determine whether the subscription has ended.                                                                    |
| [`IsActive` property](#property-isactive)           | Determine whether the subscription is active and its matching rows are present in the client cache.              |
| [`Unsubscribe` method](#method-unsubscribe)         | Discard a subscription.                                                                                          |
| [`UnsubscribeThen` method](#method-unsubscribethen) | Discard a subscription, and register a callback to run when its matching rows are removed from the client cache. |

##### Property `IsEnded`

```csharp
class SubscriptionHandle
{
    public bool IsEnded;
}
```

True if this subscription has been terminated due to an unsubscribe call or an error.

##### Property `IsActive`

```csharp
class SubscriptionHandle
{
    public bool IsActive;
}
```

True if this subscription has been applied and has not yet been unsubscribed.

##### Method `Unsubscribe`

```csharp
class SubscriptionHandle
{
    public void Unsubscribe();
}
```

Terminate this subscription, causing matching rows to be removed from the client cache. Any rows removed from the client cache this way will have [`OnDelete` callbacks](#callback-ondelete) run for them.

Unsubscribing is an asynchronous operation. Matching rows are not removed from the client cache immediately. Use [`UnsubscribeThen`](#method-unsubscribethen) to run a callback once the unsubscribe operation is completed.

Returns an error if the subscription has already ended, either due to a previous call to `Unsubscribe` or [`UnsubscribeThen`](#method-unsubscribethen), or due to an error.

##### Method `UnsubscribeThen`

```csharp
class SubscriptionHandle
{
    public void UnsubscribeThen(Action<SubscriptionEventContext>? onEnded);
}
```

Terminate this subscription, and run the `onEnded` callback when the subscription is ended and its matching rows are removed from the client cache. Any rows removed from the client cache this way will have [`OnDelete` callbacks](#callback-ondelete) run for them.

Returns an error if the subscription has already ended, either due to a previous call to [`Unsubscribe`](#method-unsubscribe) or `UnsubscribeThen`, or due to an error.

### Read connection metadata

#### Property `Identity`

```csharp
interface IDbContext
{
    public Identity? Identity { get; }
}
```

Get the `Identity` with which SpacetimeDB identifies the connection. This method returns null if the connection was initiated anonymously and the newly-generated `Identity` has not yet been received, i.e. if called before the [`OnConnect` callback](#callback-onconnect) is invoked.

#### Property `ConnectionId`

```csharp
interface IDbContext
{
    public ConnectionId ConnectionId { get; }
}
```

Get the [`ConnectionId`](#type-connectionid) with which SpacetimeDB identifies the connection.

#### Property `IsActive`

```csharp
interface IDbContext
{
    public bool IsActive { get; }
}
```

`true` if the connection has not yet disconnected. Note that a connection `IsActive` when it is constructed, before its [`OnConnect` callback](#callback-onconnect) is invoked.

## Type `EventContext`

An `EventContext` is an [`IDbContext`](#interface-idbcontext) augmented with an [`Event`](#record-event) property. `EventContext`s are passed as the first argument to row callbacks [`OnInsert`](#callback-oninsert), [`OnDelete`](#callback-ondelete) and [`OnUpdate`](#callback-onupdate).

| Name                                      | Description                                                   |
| ----------------------------------------- | ------------------------------------------------------------- |
| [`Event` property](#property-event)       | Enum describing the cause of the current row callback.        |
| [`Db` property](#property-db)             | Provides access to the client cache.                          |
| [`Reducers` property](#property-reducers) | Allows requesting reducers run on the remote database.        |
| [`Event` record](#record-event)           | Possible events which can cause a row callback to be invoked. |

### Property `Event`

```csharp
class EventContext {
    public readonly Event<Reducer> Event;
    /* other fields */
}
```

The [`Event`](#record-event) contained in the `EventContext` describes what happened to cause the current row callback to be invoked.

### Property `Db`

```csharp
class EventContext {
    public RemoteTables Db;
    /* other fields */
}
```

The `Db` property of the context provides access to the subscribed view of the remote database's tables. See [Access the client cache](#access-the-client-cache).

### Field `Reducers`

```csharp
class EventContext {
    public RemoteReducers Reducers;
    /* other fields */
}
```

The `Reducers` property of the context provides access to reducers exposed by the remote module. See [Observe and invoke reducers](#observe-and-invoke-reducers).

### Record `Event`

| Name                                                        | Description                                                                                                                              |
| ----------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| [`Reducer` variant](#variant-reducer)                       | A reducer ran in the remote database.                                                                                                    |
| [`SubscribeApplied` variant](#variant-subscribeapplied)     | A new subscription was applied to the client cache.                                                                                      |
| [`UnsubscribeApplied` variant](#variant-unsubscribeapplied) | A previous subscription was removed from the client cache after a call to [`Unsubscribe`](#method-unsubscribe).                          |
| [`SubscribeError` variant](#variant-subscribeerror)         | A previous subscription was removed from the client cache due to an error.                                                               |
| [`UnknownTransaction` variant](#variant-unknowntransaction) | A transaction ran in the remote database, but was not attributed to a known reducer.                                                     |
| [`ReducerEvent` record](#record-reducerevent)               | Metadata about a reducer run. Contained in a [`Reducer` event](#variant-reducer) and [`ReducerEventContext`](#type-reducereventcontext). |
| [`Status` record](#record-status)                           | Completion status of a reducer run.                                                                                                      |
| [`Reducer` record](#record-reducer)                         | Module-specific generated record with a variant for each reducer defined by the module.                                                  |

#### Variant `Reducer`

```csharp
record Event<R>
{
    public record Reducer(ReducerEvent<R> ReducerEvent) : Event<R>;
}
```

Event when we are notified that a reducer ran in the remote database. The [`ReducerEvent`](#record-reducerevent) contains metadata about the reducer run, including its arguments and termination [`Status`](#record-status).

This event is passed to row callbacks resulting from modifications by the reducer.

#### Variant `SubscribeApplied`

```csharp
record Event<R>
{
    public record SubscribeApplied : Event<R>;
}
```

Event when our subscription is applied and its rows are inserted into the client cache.

This event is passed to [row `OnInsert` callbacks](#callback-oninsert) resulting from the new subscription.

#### Variant `UnsubscribeApplied`

```csharp
record Event<R>
{
    public record UnsubscribeApplied : Event<R>;
}
```

Event when our subscription is removed after a call to [`SubscriptionHandle.Unsubscribe`](#method-unsubscribe) or [`SubscriptionHandle.UnsubscribeTthen`](#method-unsubscribethen) and its matching rows are deleted from the client cache.

This event is passed to [row `OnDelete` callbacks](#callback-ondelete) resulting from the subscription ending.

#### Variant `SubscribeError`

```csharp
record Event<R>
{
    public record SubscribeError(Exception Exception) : Event<R>;
}
```

Event when a subscription ends unexpectedly due to an error.

This event is passed to [row `OnDelete` callbacks](#callback-ondelete) resulting from the subscription ending.

#### Variant `UnknownTransaction`

```csharp
record Event<R>
{
    public record UnknownTransaction : Event<R>;
}
```

Event when we are notified of a transaction in the remote database which we cannot associate with a known reducer. This may be an ad-hoc SQL query or a reducer for which we do not have bindings.

This event is passed to [row callbacks](#callback-oninsert) resulting from modifications by the transaction.

### Record `ReducerEvent`

```csharp
record ReducerEvent<R>(
    Timestamp Timestamp,
    Status Status,
    Identity CallerIdentity,
    ConnectionId? CallerConnectionId,
    U128? EnergyConsumed,
    R Reducer
)
```

A `ReducerEvent` contains metadata about a reducer run.

### Record `Status`

```csharp
record Status : TaggedEnum<(
    Unit Committed,
    string Failed,
    Unit OutOfEnergy
)>;
```

<!-- TODO: Link to the definition of TaggedEnum in the module docs -->

| Name                                          | Description                                         |
| --------------------------------------------- | --------------------------------------------------- |
| [`Committed` variant](#variant-committed)     | The reducer ran successfully.                       |
| [`Failed` variant](#variant-failed)           | The reducer errored.                                |
| [`OutOfEnergy` variant](#variant-outofenergy) | The reducer was aborted due to insufficient energy. |

#### Variant `Committed`

The reducer returned successfully and its changes were committed into the database state. An [`Event.Reducer`](#variant-reducer) passed to a row callback must have this status in its [`ReducerEvent`](#record-reducerevent).

#### Variant `Failed`

The reducer returned an error, panicked, or threw an exception. The record payload is the stringified error message. Formatting of the error message is unstable and subject to change, so clients should use it only as a human-readable diagnostic, and in particular should not attempt to parse the message.

#### Variant `OutOfEnergy`

The reducer was aborted due to insufficient energy balance of the module owner.

### Record `Reducer`

The module bindings contains an record `Reducer` with a variant for each reducer defined by the module. Each variant has a payload containing the arguments to the reducer.

## Type `ReducerEventContext`

A `ReducerEventContext` is an [`IDbContext`](#interface-idbcontext) augmented with an [`Event`](#record-reducerevent) property. `ReducerEventContext`s are passed as the first argument to [reducer callbacks](#observe-and-invoke-reducers).

| Name                                      | Description                                                         |
| ----------------------------------------- | ------------------------------------------------------------------- |
| [`Event` property](#property-event)       | [`ReducerEvent`](#record-reducerevent) containing reducer metadata. |
| [`Db` property](#property-db)             | Provides access to the client cache.                                |
| [`Reducers` property](#property-reducers) | Allows requesting reducers run on the remote database.              |

### Property `Event`

```csharp
class ReducerEventContext {
    public readonly ReducerEvent<Reducer> Event;
    /* other fields */
}
```

The [`ReducerEvent`](#record-reducerevent) contained in the `ReducerEventContext` has metadata about the reducer which ran.

### Property `Db`

```csharp
class ReducerEventContext {
    public RemoteTables Db;
    /* other fields */
}
```

The `Db` property of the context provides access to the subscribed view of the remote database's tables. See [Access the client cache](#access-the-client-cache).

### Property `Reducers`

```csharp
class ReducerEventContext {
    public RemoteReducers Reducers;
    /* other fields */
}
```

The `Reducers` property of the context provides access to reducers exposed by the remote module. See [Observe and invoke reducers](#observe-and-invoke-reducers).

## Type `SubscriptionEventContext`

A `SubscriptionEventContext` is an [`IDbContext`](#interface-idbcontext). Unlike the other context types, `SubscriptionEventContext` doesn't have an `Event` property. `SubscriptionEventContext`s are passed to subscription [`OnApplied`](#callback-onapplied) and [`UnsubscribeThen`](#method-unsubscribethen) callbacks.

| Name                                      | Description                                            |
| ----------------------------------------- | ------------------------------------------------------ |
| [`Db` property](#property-db)             | Provides access to the client cache.                   |
| [`Reducers` property](#property-reducers) | Allows requesting reducers run on the remote database. |

### Property `Db`

```csharp
class SubscriptionEventContext {
    public RemoteTables Db;
    /* other fields */
}
```

The `Db` property of the context provides access to the subscribed view of the remote database's tables. See [Access the client cache](#access-the-client-cache).

### Property `Reducers`

```csharp
class SubscriptionEventContext {
    public RemoteReducers Reducers;
    /* other fields */
}
```

The `Reducers` property of the context provides access to reducers exposed by the remote module. See [Observe and invoke reducers](#observe-and-invoke-reducers).

## Type `ErrorContext`

An `ErrorContext` is an [`IDbContext`](#interface-idbcontext) augmented with an `Event` property. `ErrorContext`s are to connections' [`OnDisconnect`](#callback-ondisconnect) and [`OnConnectError`](#callback-onconnecterror) callbacks, and to subscriptions' [`OnError`](#callback-onerror) callbacks.

| Name                                      | Description                                            |
| ----------------------------------------- | ------------------------------------------------------ |
| [`Event` property](#property-event)       | The error which caused the current error callback.     |
| [`Db` property](#property-db)             | Provides access to the client cache.                   |
| [`Reducers` property](#property-reducers) | Allows requesting reducers run on the remote database. |

### Property `Event`

```csharp
class SubscriptionEventContext {
    public readonly Exception Event;
    /* other fields */
}
```

### Property `Db`

```csharp
class ErrorContext {
    public RemoteTables Db;
    /* other fields */
}
```

The `Db` property of the context provides access to the subscribed view of the remote database's tables. See [Access the client cache](#access-the-client-cache).

### Property `Reducers`

```csharp
class ErrorContext {
    public RemoteReducers Reducers;
    /* other fields */
}
```

The `Reducers` property of the context provides access to reducers exposed by the remote database. See [Observe and invoke reducers](#observe-and-invoke-reducers).

## Access the client cache

All [`IDbContext`](#interface-idbcontext) implementors, including [`DbConnection`](#type-dbconnection) and [`EventContext`](#type-eventcontext), have `.Db` properties, which in turn have methods for accessing tables in the client cache.

Each table defined by a module has an accessor method, whose name is the table name converted to `snake_case`, on this `.Db` property. The table accessor methods return table handles which inherit from [`RemoteTableHandle`](#type-remotetablehandle) and have methods for searching by index.

| Name                                                              | Description                                                                     |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| [`RemoteTableHandle`](#type-remotetablehandle)                    | Provides access to subscribed rows of a specific table within the client cache. |
| [Unique constraint index access](#unique-constraint-index-access) | Seek a subscribed row by the value in its unique or primary key column.         |
| [BTree index access](#btree-index-access)                         | Seek subscribed rows by the value in its indexed column.                        |

### Type `RemoteTableHandle`

Implemented by all table handles.

| Name                                      | Description                                                                          |
| ----------------------------------------- | ------------------------------------------------------------------------------------ |
| [`Row` type parameter](#type-row)         | The type of rows in the table.                                                       |
| [`Count` property](#property-count)       | The number of subscribed rows in the table.                                          |
| [`Iter` method](#method-iter)             | Iterate over all subscribed rows in the table.                                       |
| [`OnInsert` callback](#callback-oninsert) | Register a callback to run whenever a row is inserted into the client cache.         |
| [`OnDelete` callback](#callback-ondelete) | Register a callback to run whenever a row is deleted from the client cache.          |
| [`OnUpdate` callback](#callback-onupdate) | Register a callback to run whenever a subscribed row is replaced with a new version. |

#### Type `Row`

```csharp
class RemoteTableHandle<EventContext, Row>
{
    /* members */
}
```

The type of rows in the table.

#### Property `Count`

```csharp
class RemoteTableHandle
{
    public int Count;
}
```

The number of rows of this table resident in the client cache, i.e. the total number which match any subscribed query.

#### Method `Iter`

```csharp
class RemoteTableHandle
{
    public IEnumerable<Row> Iter();
}
```

An iterator over all the subscribed rows in the client cache, i.e. those which match any subscribed query.

#### Callback `OnInsert`

```csharp
class RemoteTableHandle
{
    public delegate void RowEventHandler(EventContext context, Row row);
    public event RowEventHandler? OnInsert;
}
```

The `OnInsert` callback runs whenever a new row is inserted into the client cache, either when applying a subscription or being notified of a transaction. The passed [`EventContext`](#type-eventcontext) contains an [`Event`](#record-event) which can identify the change which caused the insertion, and also allows the callback to interact with the connection, inspect the client cache and invoke reducers. Newly registered or canceled callbacks do not take effect until the following event.

See [the quickstart](/sdks/c-sharp/quickstart#register-callbacks) for examples of regstering and unregistering row callbacks.

#### Callback `OnDelete`

```csharp
class RemoteTableHandle
{
    public delegate void RowEventHandler(EventContext context, Row row);
    public event RowEventHandler? OnDelete;
}
```

The `OnDelete` callback runs whenever a previously-resident row is deleted from the client cache. Newly registered or canceled callbacks do not take effect until the following event.

See [the quickstart](/sdks/c-sharp/quickstart#register-callbacks) for examples of regstering and unregistering row callbacks.

#### Callback `OnUpdate`

```csharp
class RemoteTableHandle
{
    public delegate void RowEventHandler(EventContext context, Row row);
    public event RowEventHandler? OnUpdate;
}
```

The `OnUpdate` callback runs whenever an already-resident row in the client cache is updated, i.e. replaced with a new row that has the same primary key. The table must have a primary key for callbacks to be triggered. Newly registered or canceled callbacks do not take effect until the following event.

See [the quickstart](/sdks/c-sharp/quickstart#register-callbacks) for examples of regstering and unregistering row callbacks.

### Unique constraint index access

For each unique constraint on a table, its table handle has a property which is a unique index handle and whose name is the unique column name. This unique index handle has a method `.Find(Column value)`. If a `Row` with `value` in the unique column is resident in the client cache, `.Find` returns it. Otherwise it returns null.

#### Example

Given the following module-side `User` definition:

```csharp
[Table(Name = "User", Public = true)]
public partial class User
{
    [Unique] // Or [PrimaryKey]
    public Identity Identity;
    ..
}
```

a client would lookup a user as follows:

```csharp
User? FindUser(RemoteTables tables, Identity id) => tables.User.Identity.Find(id);
```

### BTree index access

For each btree index defined on a remote table, its corresponding table handle has a property which is a btree index handle and whose name is the name of the index. This index handle has a method `IEnumerable<Row> Filter(Column value)` which will return `Row`s with `value` in the indexed `Column`, if there are any in the cache.

#### Example

Given the following module-side `Player` definition:

```csharp
[Table(Name = "Player", Public = true)]
public partial class Player
{
    [PrimaryKey]
    public Identity id;

    [Index.BTree(Name = "Level")]
    public uint level;
    ..
}
```

a client would count the number of `Player`s at a certain level as follows:

```csharp
int CountPlayersAtLevel(RemoteTables tables, uint level) => tables.Player.Level.Filter(level).Count();
```

## Observe and invoke reducers

All [`IDbContext`](#interface-idbcontext) implementors, including [`DbConnection`](#type-dbconnection) and [`EventContext`](#type-eventcontext), have a `.Reducers` property, which in turn has methods for invoking reducers defined by the module and registering callbacks on it.

Each reducer defined by the module has three methods on the `.Reducers`:

- An invoke method, whose name is the reducer's name converted to snake case, like `set_name`. This requests that the module run the reducer.
- A callback registation method, whose name is prefixed with `on_`, like `on_set_name`. This registers a callback to run whenever we are notified that the reducer ran, including successfully committed runs and runs we requested which failed. This method returns a callback id, which can be passed to the callback remove method.
- A callback remove method, whose name is prefixed with `remove_on_`, like `remove_on_set_name`. This cancels a callback previously registered via the callback registration method.

## Identify a client

### Type `Identity`

A unique public identifier for a client connected to a database.
See the [module docs](/modules/c-sharp#struct-identity) for more details.

### Type `ConnectionId`

An opaque identifier for a client connection to a database, intended to differentiate between connections from the same [`Identity`](#type-identity).
See the [module docs](/modules/c-sharp#struct-connectionid) for more details.

### Type `Timestamp`

A point in time, measured in microseconds since the Unix epoch.
See the [module docs](/modules/c-sharp#struct-timestamp) for more details.

### Type `TaggedEnum`

A [tagged union](https://en.wikipedia.org/wiki/Tagged_union) type.
See the [module docs](/modules/c-sharp#record-taggedenum) for more details.
