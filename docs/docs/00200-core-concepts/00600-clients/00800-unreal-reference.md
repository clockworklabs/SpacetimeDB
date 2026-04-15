---
title: Unreal Reference
slug: /clients/unreal
---


The SpacetimeDB client for Unreal Engine contains all the tools you need to build native clients for SpacetimeDB modules using C++ and Blueprint.

Before diving into the reference, you may want to review:

- [Generating Client Bindings](./00200-codegen.md) - How to generate Unreal bindings from your module
- [Connecting to SpacetimeDB](./00300-connection.md) - Establishing and managing connections
- [SDK API Reference](./00400-sdk-api.md) - Core concepts that apply across all SDKs

| Name                                                        | Description                                                                                             |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| [Project setup](#project-setup)                             | Configure an Unreal project to use the SpacetimeDB Unreal client SDK.                                   |
| [Generate module bindings](#generate-module-bindings)       | Use the SpacetimeDB CLI to generate module-specific types and interfaces.                               |
| [DbConnection type](#type-dbconnection)                     | A connection to a remote database.                                                                      |
| [Context interfaces](#context-interfaces)                   | Context objects for interacting with the remote database in callbacks.                                  |
| [Access the client cache](#access-the-client-cache)         | Access to your local view of the database.                                                              |
| [Observe and invoke reducers](#observe-and-invoke-reducers) | Send requests to the database to run reducers, and register callbacks for reducer results on the calling connection. |
| [Subscriptions](#subscriptions)                             | Subscribe to queries and manage subscription lifecycle.                                                 |
| [Query Builder API](#query-builder-api)                     | Build typed subscription queries in Unreal C++ and Blueprint.                                           |
| [Identify a client](#identify-a-client)                     | Types for identifying users and client connections.                                                     |

## Project setup

### Using the Unreal Engine Plugin

Add the SpacetimeDB Unreal SDK to your project as a plugin. The SDK provides both C++ and Blueprint APIs for connecting to SpacetimeDB modules.

### Generate module bindings

Each SpacetimeDB client depends on some bindings specific to your module. Generate the Unreal interface files using the Spacetime CLI. From your project directory, run:

```bash
spacetime generate --lang unrealcpp --uproject-dir <uproject_directory> --module-path <module_path> --unreal-module-name <module_name>
```

Replace:

- `<uproject_directory>` with the path to your Unreal project directory (containing the `.uproject` file)
- `<module_path>` with the path to your SpacetimeDB module
- `<module_name>` with the name of your Unreal module, typically the name of the project

**Example:**

```bash
spacetime generate --lang unrealcpp --uproject-dir /path/to/MyGame --module-path /path/to/quickstart-chat --unreal-module-name QuickstartChat
```

This generates module-specific bindings in your project's `ModuleBindings` directory.

## Type `DbConnection`

A connection to a remote database is represented by the `UDbConnection` class. This class is generated per module and contains information about the types, tables, and reducers defined by your module.

| Name                                                                   | Description                                                                   |
| ---------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| [Connect to a database](#connect-to-a-database)                        | Construct a UDbConnection instance.                                           |
| [Advance the connection](#advance-the-connection-and-process-messages) | The connection processes messages automatically via WebSocket callbacks.      |
| [Access tables and reducers](#access-tables-and-reducers)              | Access the client cache, request reducer invocations, and register callbacks. |

### Connect to a database

```cpp
class UDbConnection
{
    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    static UDbConnectionBuilder* Builder();
};
```

Construct a `UDbConnection` by calling `UDbConnection::Builder()`, chaining configuration methods, and finally calling `.Build()`. At a minimum, you must specify `WithUri` to provide the URI of the SpacetimeDB instance, and `WithDatabaseName` to specify the database's name or identity.

| Name                                                | Description                                                                          |
|-----------------------------------------------------|--------------------------------------------------------------------------------------|
| [WithUri method](#method-withuri)                   | Set the URI of the SpacetimeDB instance hosting the remote database.                 |
| [WithDatabaseName method](#method-withdatabasename) | Set the name or identity of the remote database.                                     |
| [WithToken method](#method-withtoken)               | Supply a token to authenticate with the remote database.                             |
| [WithCompression method](#method-withcompression)   | Set the compression method for WebSocket communication.                              |
| [OnConnect callback](#callback-onconnect)           | Register a callback to run when the connection is successfully established.          |
| [OnConnectError callback](#callback-onconnecterror) | Register a callback to run if the connection is rejected or the host is unreachable. |
| [OnDisconnect callback](#callback-ondisconnect)     | Register a callback to run when the connection ends.                                 |
| [Build method](#method-build)                       | Finalize configuration and open the connection.                                      |

#### Method `WithUri`

```cpp
class UDbConnectionBuilder
{
    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    UDbConnectionBuilder* WithUri(const FString& InUri);
};
```

Configure the URI of the SpacetimeDB instance or cluster which hosts the remote module and database.

#### Method `WithDatabaseName`

```cpp
class UDbConnectionBuilder
{
    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    UDbConnectionBuilder* WithDatabaseName(const FString& InName);
};
```

Configure the SpacetimeDB domain name or `Identity` of the remote database which identifies it within the SpacetimeDB instance or cluster.

#### Method `WithToken`

```cpp
class UDbConnectionBuilder
{
    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    UDbConnectionBuilder* WithToken(const FString& InToken);
};
```

Chain a call to `.WithToken(token)` to your builder to provide an OpenID Connect compliant JSON Web Token to authenticate with, or to explicitly select an anonymous connection.

#### Method `WithCompression`

```cpp
class UDbConnectionBuilder
{
    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    UDbConnectionBuilder* WithCompression(const ESpacetimeDBCompression& InCompression);
};
```

Set the compression method for WebSocket communication. Available options:

- `ESpacetimeDBCompression::None` - No compression
- `ESpacetimeDBCompression::Gzip` - Gzip compression (default)
- `ESpacetimeDBCompression::Brotli` - Brotli compression (not implemented, defaults to Gzip)

#### Callback `OnConnect`

```cpp
class UDbConnectionBuilder
{
    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    UDbConnectionBuilder* OnConnect(FOnConnectDelegate Callback);
};
```

Chain a call to `.OnConnect(callback)` to your builder to register a callback to run when your new `UDbConnection` successfully initiates its connection to the remote database. The callback accepts three arguments: a reference to the `UDbConnection`, the `FSpacetimeDBIdentity` by which SpacetimeDB identifies this connection, and a private access token which can be saved and later passed to `WithToken` to authenticate the same user in future connections.

#### Callback `OnConnectError`

```cpp
class UDbConnectionBuilder
{
    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    UDbConnectionBuilder* OnConnectError(FOnConnectErrorDelegate Callback);
};
```

Chain a call to `.OnConnectError(callback)` to your builder to register a callback to run when your connection fails.

#### Callback `OnDisconnect`

```cpp
class UDbConnectionBuilder
{
    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    UDbConnectionBuilder* OnDisconnect(FOnDisconnectDelegate Callback);
};
```

Chain a call to `.OnDisconnect(callback)` to your builder to register a callback to run when your `UDbConnection` disconnects from the remote database, either as a result of a call to `Disconnect` or due to an error.

#### Method `Build`

```cpp
class UDbConnectionBuilder
{
    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    UDbConnection* Build();
};
```

Finalize configuration and open the connection. This creates a WebSocket connection to `ws://<uri>/v1/database/<database>/subscribe?compression=<compression>` and begins processing messages using the Unreal SDK's binary v2 WebSocket subprotocol.

### Advance the connection and process messages

The Unreal SDK processes messages automatically via WebSocket callbacks and with UDbConnection which ultimately inherits from FTickableGameObject. No manual polling or advancement is required. Events are dispatched through the registered delegates.

### Access tables and reducers

```cpp
class UDbConnection
{
    UPROPERTY(BlueprintReadOnly, Category="SpacetimeDB")
    URemoteTables* Db;

    UPROPERTY(BlueprintReadOnly, Category="SpacetimeDB")
    URemoteReducers* Reducers;
};
```

The `Db` property provides access to the client cache, and the `Reducers` property allows invoking reducers and handling the results of reducers called by this connection.

## Context interfaces

Context objects provide access to the database and reducers within callback functions. All context types inherit from `FContextBase`.

| Name                                                         | Description                                       |
| ------------------------------------------------------------ | ------------------------------------------------- |
| [FContextBase](#type-fcontextbase)                           | Base context providing access to Db and Reducers. |
| [FEventContext](#type-feventcontext)                         | Context for table row event callbacks.            |
| [FReducerEventContext](#type-freducereventcontext)           | Context for reducer event callbacks.              |
| [FSubscriptionEventContext](#type-fsubscriptioneventcontext) | Context for subscription lifecycle callbacks.     |
| [FErrorContext](#type-ferrorcontext)                         | Context for error callbacks.                      |

### Type `FContextBase`

```cpp
USTRUCT(BlueprintType)
struct FContextBase
{
    GENERATED_BODY()

    UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
    URemoteTables* Db;

    UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
    URemoteReducers* Reducers;

    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    USubscriptionBuilder* SubscriptionBuilder();

    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    bool TryGetIdentity(FSpacetimeDBIdentity& OutIdentity) const;

    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    FSpacetimeDBConnectionId GetConnectionId() const;
};
```

Base context providing access to the client cache, reducers, subscription builder, and connection information.

### Type `FEventContext`

```cpp
USTRUCT(BlueprintType)
struct FEventContext : public FContextBase
{
    GENERATED_BODY()

    UPROPERTY(BlueprintReadOnly, Category="SpacetimeDB")
    FSpacetimeDBEvent Event;
};
```

Context passed to table row event callbacks (OnInsert, OnUpdate, OnDelete).

### Type `FReducerEventContext`

```cpp
USTRUCT(BlueprintType)
struct FReducerEventContext : public FContextBase
{
    GENERATED_BODY()

    UPROPERTY(BlueprintReadOnly, Category="SpacetimeDB")
    FReducerEvent Event;
};
```

Context passed to reducer event callbacks, containing information about the reducer execution.

### Type `FSubscriptionEventContext`

```cpp
USTRUCT(BlueprintType)
struct FSubscriptionEventContext : public FContextBase
{
    GENERATED_BODY()
};
```

Context passed to subscription lifecycle callbacks (OnApplied, OnError).

### Type `FErrorContext`

```cpp
USTRUCT(BlueprintType)
struct FErrorContext : public FContextBase
{
    GENERATED_BODY()

    UPROPERTY(BlueprintReadOnly, Category="SpacetimeDB")
    FString Error;
};
```

Context passed to error callbacks, containing error information.

## Access the client cache

All context types provide access to the client cache through the `.Db` property, which contains generated table classes for each table defined by your module.

Each table defined by a module has a corresponding generated class (e.g., `UUserTable`, `UMessageTable`) that inherits from `URemoteTable` and provides methods for accessing subscribed rows.

| Name                                                              | Description                                                             |
| ----------------------------------------------------------------- | ----------------------------------------------------------------------- |
| [URemoteTable](#type-uremotetable)                                | Base class for all generated table classes.                             |
| [Unique constraint index access](#unique-constraint-index-access) | Seek a subscribed row by the value in its unique or primary key column. |
| [BTree index access](#btree-index-access)                         | Seek subscribed rows by the value in its indexed column.                |

### Type `URemoteTable`

Generated table classes inherit from `URemoteTable` and provide the following interface:

| Name                                    | Description                                                                          |
| --------------------------------------- | ------------------------------------------------------------------------------------ |
| [Count method](#method-count)           | The number of subscribed rows in the table.                                          |
| [Iter method](#method-iter)             | Iterate over all subscribed rows in the table.                                       |
| [OnInsert callback](#callback-oninsert) | Register a callback to run whenever a row is inserted into the client cache.         |
| [OnDelete callback](#callback-ondelete) | Register a callback to run whenever a row is deleted from the client cache.          |
| [OnUpdate callback](#callback-onupdate) | Register a callback to run whenever a subscribed row is replaced with a new version. |

#### Method `Count`

```cpp
UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
int32 Count() const;
```

The number of rows of this table resident in the client cache, i.e. the total number which match any subscribed query.

#### Method `Iter`

```cpp
UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
TArray<RowType> Iter() const;
```

An iterator over all the subscribed rows in the client cache, i.e. those which match any subscribed query.

#### Callback `OnInsert`

```cpp
DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(
    FOnTableInsert,
    const FEventContext&, Context,
    const RowType&, NewRow);

UPROPERTY(BlueprintAssignable, Category = "SpacetimeDB Events")
FOnTableInsert OnInsert;
```

The `OnInsert` callback runs whenever a new row is inserted into the client cache, either when applying a subscription or being notified of a transaction.

#### Callback `OnDelete`

```cpp
DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(
    FOnTableDelete,
    const FEventContext&, Context,
    const RowType&, DeletedRow);

UPROPERTY(BlueprintAssignable, Category = "SpacetimeDB Events")
FOnTableDelete OnDelete;
```

The `OnDelete` callback runs whenever a previously-resident row is deleted from the client cache.

#### Callback `OnUpdate`

```cpp
DECLARE_DYNAMIC_MULTICAST_DELEGATE_ThreeParams(
    FOnTableUpdate,
    const FEventContext&, Context,
    const RowType&, OldRow,
    const RowType&, NewRow);

UPROPERTY(BlueprintAssignable, Category = "SpacetimeDB Events")
FOnTableUpdate OnUpdate;
```

The `OnUpdate` callback runs whenever an already-resident row in the client cache is updated, i.e. replaced with a new row that has the same primary key.

### Unique constraint index access

For each unique constraint on a table, its table class has a property which is a unique index handle. This unique index handle has a method `.Find(Column value)`. If a `Row` with `value` in the unique column is resident in the client cache, `.Find` returns it. Otherwise it returns null.

#### Example

Given the following module-side `User` definition:

```cpp
USTRUCT()
struct FUserType
{
    GENERATED_BODY()

    UPROPERTY()
    FSpacetimeDBIdentity Identity; // Unique constraint
    // ... other fields
};
```

a client would lookup a user as follows:

```cpp
FUserType* FindUser(URemoteTables* Tables, FSpacetimeDBIdentity Id)
{
    return Tables->User->Identity->Find(Id);
}
```

### BTree index access

For each btree index defined on a remote table, its corresponding table class has a property which is a btree index handle. This index handle has a method `TArray<RowType> Filter(Column value)` which will return `Row`s with `value` in the indexed `Column`, if there are any in the cache.

#### Example

Given the following module-side `Player` definition:

```cpp
USTRUCT()
struct FPlayerType
{
    GENERATED_BODY()

    UPROPERTY()
    FSpacetimeDBIdentity Id; // Primary key

    UPROPERTY()
    uint32 Level; // BTree index
    // ... other fields
};
```

a client would count the number of `Player`s at a certain level as follows:

```cpp
int32 CountPlayersAtLevel(URemoteTables* Tables, uint32 Level)
{
    return Tables->Player->Level->Filter(Level).Num();
}
```

## Observe and invoke reducers

All context types provide access to reducers through the `.Reducers` property, which contains generated methods for invoking reducers defined by the module and generated delegates for reducer results.

Each reducer defined by the module has methods on the `.Reducers`:

- An invoke method, whose name matches the reducer's name (e.g., `SendMessage`, `SetName`). This requests that the module run the reducer.
- A generated delegate, whose name is prefixed with `On` (e.g., `OnSendMessage`, `OnSetName`). This runs when the result for a reducer call made by this connection is received and correlated by `request_id`.

### Invoke reducers

```cpp
class URemoteReducers
{
    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    void SendMessage(const FString& Text);

    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    void SetName(const FString& Name);
};
```

### Observe reducer results

```cpp
class URemoteReducers
{
    DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(
        FOnSendMessage,
        const FReducerEventContext&, Context,
        const FString&, Text);

    UPROPERTY(BlueprintAssignable, Category = "SpacetimeDB Events")
    FOnSendMessage OnSendMessage;

    DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(
        FOnSetName,
        const FReducerEventContext&, Context,
        const FString&, Name);

    UPROPERTY(BlueprintAssignable, Category = "SpacetimeDB Events")
    FOnSetName OnSetName;
};
```

The generated `On<Reducer>` delegates are the Unreal equivalent of a per-call callback. They are not global reducer broadcasts for other clients' reducer calls.

## Subscriptions

Create subscriptions to receive updates for specific queries using the `USubscriptionBuilder` and `USubscriptionHandle` classes.

For Unreal C++, the recommended default is to build subscriptions with `AddQuery(...)` and then call parameterless `Subscribe()`. Raw SQL subscriptions remain available when you need to provide SQL directly.

| Name                                               | Description                        |
| -------------------------------------------------- | ---------------------------------- |
| [USubscriptionBuilder](#type-usubscriptionbuilder) | Build and configure subscriptions. |
| [USubscriptionHandle](#type-usubscriptionhandle)   | Manage subscription lifecycle.     |

### Type `USubscriptionBuilder`

```cpp
class USubscriptionBuilder
{
    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    USubscriptionBuilder* OnApplied(FOnSubscriptionApplied Callback);

    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    USubscriptionBuilder* OnError(FOnSubscriptionError Callback);

    USubscriptionBuilder* AddQuery(TFunctionRef<FQuery(const FQueryBuilder&)> BuildQuery);

    UFUNCTION(BlueprintCallable, Category="SpacetimeDB")
    USubscriptionHandle* Subscribe();

    UFUNCTION(BlueprintCallable, Category="SpacetimeDB")
    USubscriptionHandle* Subscribe(const TArray<FString>& SQL);

    UFUNCTION(BlueprintCallable, Category="SpacetimeDB")
    USubscriptionHandle* SubscribeToAllTables();
};
```

#### Method `OnApplied`

```cpp
USubscriptionBuilder* OnApplied(FOnSubscriptionApplied Callback);
```

Register a callback to run when the subscription is successfully applied.

#### Method `OnError`

```cpp
USubscriptionBuilder* OnError(FOnSubscriptionError Callback);
```

Register a callback to run if the subscription fails.

#### Method `AddQuery`

```cpp
USubscriptionBuilder* AddQuery(TFunctionRef<FQuery(const FQueryBuilder&)> BuildQuery);
```

Append a typed query to the builder. The callback receives an `FQueryBuilder`, typically named `Q`, and returns a generated query source or filtered query. Call `AddQuery(...)` once per table or view query you want to subscribe to, then finish with parameterless `Subscribe()`.

#### Method `Subscribe`

```cpp
USubscriptionHandle* Subscribe();
```

Subscribe to the typed queries accumulated with `AddQuery(...)` and return a handle for managing the subscription.

#### Method `Subscribe` (SQL overload)

```cpp
USubscriptionHandle* Subscribe(const TArray<FString>& SQL);
```

Subscribe to the provided SQL queries and return a handle for managing the subscription. Use this when you need to write SQL directly instead of using the typed query builder.

#### Method `SubscribeToAllTables`

```cpp
USubscriptionHandle* SubscribeToAllTables();
```

Subscribe to all public tables in the module.

`SubscribeToAllTables()` is useful for quick prototypes and small modules. Prefer typed queries for production subscriptions so the subscription set stays explicit.

### Type `USubscriptionHandle`

```cpp
class USubscriptionHandle
{
    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    void Unsubscribe();

    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    void UnsubscribeThen(FSubscriptionEventDelegate OnEnd);

    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    bool IsEnded() const;

    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    bool IsActive() const;

    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
    TArray<FString> GetQuerySqls() const;
};
```

#### Method `Unsubscribe`

```cpp
void Unsubscribe();
```

Immediately cancel the subscription.

#### Method `UnsubscribeThen`

```cpp
void UnsubscribeThen(FSubscriptionEventDelegate OnEnd);
```

Cancel the subscription and invoke the provided callback when complete.

#### Method `IsEnded`

```cpp
bool IsEnded() const;
```

True once the subscription has ended.

#### Method `IsActive`

```cpp
bool IsActive() const;
```

True while the subscription is active.

#### Method `GetQuerySqls`

```cpp
TArray<FString> GetQuerySqls() const;
```

Get the SQL queries associated with this subscription.

## Query Builder API

Use the Unreal query builder to build typed subscriptions in C++ and Blueprint.

In C++, a query typically starts from the generated `FQueryBuilder` passed to `AddQuery(...)`, selects a source from `Q.From`, and optionally applies `Where(...)` with generated column objects:

```cpp
USubscriptionHandle* Handle = Conn->SubscriptionBuilder()
    ->OnApplied(FOnSubscriptionApplied::CreateUObject(this, &AMyActor::OnSubscriptionApplied))
    ->OnError(FOnSubscriptionError::CreateUObject(this, &AMyActor::OnSubscriptionError))
    ->AddQuery([](const FQueryBuilder& Q)
    {
        return Q.From.Player().Where([](const FPlayerCols& Cols)
        {
            return Cols.Level.Gte(1).And(Cols.DisplayName.Neq(TEXT("Guest")));
        });
    })
    ->Subscribe();
```

The generated query-builder surface is module-specific. Sources, column sets, and query return types are generated from your schema and views.

### Query sources

Each public table or subscribed query source is available under `Q.From` using a generated method:

```cpp
Q.From.Player()
Q.From.ActivePlayerLocations()
Q.From.PlayersAtLevel0()
```

Event tables are not subscribed implicitly. Subscribe to them with an explicit query just like any other source:

```cpp
Conn->SubscriptionBuilder()
    ->AddQuery([](const FQueryBuilder& Q)
    {
        return Q.From.DamageEvent();
    })
    ->Subscribe();
```

### Predicates

Generated column objects expose typed predicate methods such as:

- `Eq`
- `Neq`
- `Gt`
- `Lt`
- `Gte`
- `Lte`

Predicates can be combined with:

- `And`
- `Or`
- `Not`

Example:

```cpp
Conn->SubscriptionBuilder()
    ->AddQuery([](const FQueryBuilder& Q)
    {
        return Q.From.Player().Where([](const FPlayerCols& Cols)
        {
            return Cols.Level.Gte(3).And(Cols.IsOnline.Eq(true));
        });
    })
    ->AddQuery([](const FQueryBuilder& Q)
    {
        return Q.From.ActivePlayerLocations().Where([](const FActivePlayerLocationsCols& Cols)
        {
            return Cols.X.Gte(10).And(Cols.Y.Lte(400));
        });
    })
    ->Subscribe();
```

### Blueprint availability

The Unreal query builder is available in both C++ and Blueprint.

In Blueprint, generated nodes expose the same overall flow:

- source query nodes
- column nodes
- predicate nodes
- `Where`
- `AddQuery`
- `Subscribe`

Blueprint uses generated source-specific query types for authoring and converts them at the `AddQuery` boundary automatically.

For example, a Blueprint subscription to online players at level 3 or higher would follow this node flow:

```text
From Player
├─> Player Level
│   └─> Int32 Greater Equal (3)
├─> Player IsOnline
│   └─> Bool Equal (true)
└─> Player Where
    └─> AND
        ├─> Int32 Greater Equal (3)
        └─> Bool Equal (true)

Player Where
└─> AddQuery
    └─> Subscribe
```

The exact node names are generated from your schema, so `Player`, `Level`, and `IsOnline` will vary by module.

### Notes and limitations

- `OnApplied` is the right place to inspect the initial subscribed result set in the client cache.
- `OnInsert`, `OnUpdate`, and `OnDelete` are for subsequent live changes after the subscription is active.
- `TimeDuration` query predicates are currently unsupported.
- Raw SQL subscriptions remain available when you need manual SQL control.

## Identify a client

### Type `FSpacetimeDBIdentity`

A unique public identifier for a client connected to a database. This is a 256-bit value.

```cpp
USTRUCT(BlueprintType, Category = "SpacetimeDB")
struct FSpacetimeDBIdentity
{
    GENERATED_BODY()

    UPROPERTY(EditAnywhere, BlueprintReadWrite)
    FSpacetimeDBUInt256 Value;

    // Comparison operators, constructors, etc.
};
```

### Type `FSpacetimeDBConnectionId`

An opaque identifier for a client connection to a database, intended to differentiate between connections from the same Identity. This is a 128-bit value.

```cpp
USTRUCT(BlueprintType, Category = "SpacetimeDB")
struct FSpacetimeDBConnectionId
{
    GENERATED_BODY()

    UPROPERTY(EditAnywhere, BlueprintReadWrite)
    FSpacetimeDBUInt128 Value;

    // Comparison operators, constructors, etc.
};
```

### Type `FSpacetimeDBTimestamp`

A point in time, measured in microseconds since the Unix epoch.

```cpp
USTRUCT(BlueprintType, Category = "SpacetimeDB")
struct FSpacetimeDBTimestamp
{
    GENERATED_BODY()

    UPROPERTY(EditAnywhere, BlueprintReadWrite)
    int64 MicrosSinceEpoch;

    // Comparison operators, constructors, etc.
};
```

## Example usage

Here's a complete example of connecting to SpacetimeDB, subscribing with the typed query builder, and handling events:

```cpp
// In your Actor's BeginPlay()
void AMyActor::BeginPlay()
{
    Super::BeginPlay();

    // Setup connection callbacks
    FOnConnectDelegate ConnectDelegate;
    ConnectDelegate.BindDynamic(this, &AMyActor::OnConnected);

    FOnDisconnectDelegate DisconnectDelegate;
    DisconnectDelegate.BindDynamic(this, &AMyActor::OnDisconnected);

    // Build and connect
    Conn = UDbConnection::Builder()
        ->WithUri(TEXT("127.0.0.1:3000"))
        ->WithDatabaseName(TEXT("my-module"))
        ->OnConnect(ConnectDelegate)
        ->OnDisconnect(DisconnectDelegate)
        ->Build();

    // Register table callbacks
    Conn->Db->User->OnInsert.AddDynamic(this, &AMyActor::OnUserInsert);
    Conn->Db->User->OnUpdate.AddDynamic(this, &AMyActor::OnUserUpdate);
    Conn->Db->User->OnDelete.AddDynamic(this, &AMyActor::OnUserDelete);

    // Register reducer result callbacks for calls made by this connection
    Conn->Reducers->OnSendMessage.AddDynamic(this, &AMyActor::OnSendMessage);
}

void AMyActor::OnConnected(UDbConnection* Connection, FSpacetimeDBIdentity Identity, const FString& Token)
{
    // Save token for future connections
    UCredentials::SaveToken(Token);

    // Subscribe with typed queries
    USubscriptionHandle* Handle = Connection->SubscriptionBuilder()
        ->OnApplied(FOnSubscriptionApplied::CreateUObject(this, &AMyActor::OnSubscriptionApplied))
        ->OnError(FOnSubscriptionError::CreateUObject(this, &AMyActor::OnSubscriptionError))
        ->AddQuery([](const FQueryBuilder& Q)
        {
            return Q.From.User();
        })
        ->AddQuery([](const FQueryBuilder& Q)
        {
            return Q.From.Message().Where([](const FMessageCols& Cols)
            {
                return Cols.ChannelId.Eq(1);
            });
        })
        ->Subscribe();
}

void AMyActor::OnUserInsert(const FEventContext& Context, const FUserType& NewRow)
{
    UE_LOG(LogTemp, Log, TEXT("User inserted: %s"), *NewRow.Name);
}

void AMyActor::OnSendMessage(const FReducerEventContext& Context, const FString& Text)
{
    UE_LOG(LogTemp, Log, TEXT("Message sent: %s"), *Text);
}

void AMyActor::SendMessage(const FString& Text)
{
    if (Conn && Conn->Reducers)
    {
        Conn->Reducers->SendMessage(Text);
    }
}
```

For small modules or quick debugging sessions, you can still subscribe to every public table:

```cpp
USubscriptionHandle* Handle = Conn->SubscriptionBuilder()
    ->OnApplied(FOnSubscriptionApplied::CreateUObject(this, &AMyActor::OnSubscriptionApplied))
    ->SubscribeToAllTables();
```
