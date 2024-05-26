---
title: SpacetimeDB C# client SDK
navTitle: C#
---

The SpacetimeDB client C# for Rust contains all the tools you need to build native clients for SpacetimeDB modules using C#.

## Table of Contents

-   [The SpacetimeDB C# client SDK](#the-spacetimedb-c-client-sdk)
    -   [Table of Contents](#table-of-contents)
    -   [Install the SDK](#install-the-sdk)
        -   [Using the `dotnet` CLI tool](#using-the-dotnet-cli-tool)
        -   [Using Unity](#using-unity)
    -   [Generate module bindings](#generate-module-bindings)
    -   [Initialization](#initialization)
        -   [Static Method `SpacetimeDBClient.CreateInstance`](#static-method-spacetimedbclientcreateinstance)
        -   [Property `SpacetimeDBClient.instance`](#property-spacetimedbclientinstance)
        -   [Class `NetworkManager`](#class-networkmanager)
        -   [Method `SpacetimeDBClient.Connect`](#method-spacetimedbclientconnect)
        -   [Event `SpacetimeDBClient.onIdentityReceived`](#event-spacetimedbclientonidentityreceived)
        -   [Event `SpacetimeDBClient.onConnect`](#event-spacetimedbclientonconnect)
    -   [Query subscriptions & one-time actions](#subscribe-to-queries)
        -   [Method `SpacetimeDBClient.Subscribe`](#method-spacetimedbclientsubscribe)
        -   [Event `SpacetimeDBClient.onSubscriptionApplied`](#event-spacetimedbclientonsubscriptionapplied)
        -   [Method `SpacetimeDBClient.OneOffQuery`](#event-spacetimedbclientoneoffquery)
    -   [View rows of subscribed tables](#view-rows-of-subscribed-tables)
        -   [Class `{TABLE}`](#class-table)
            -   [Static Method `{TABLE}.Iter`](#static-method-tableiter)
            -   [Static Method `{TABLE}.FilterBy{COLUMN}`](#static-method-tablefilterbycolumn)
            -   [Static Method `{TABLE}.Count`](#static-method-tablecount)
            -   [Static Event `{TABLE}.OnInsert`](#static-event-tableoninsert)
            -   [Static Event `{TABLE}.OnBeforeDelete`](#static-event-tableonbeforedelete)
            -   [Static Event `{TABLE}.OnDelete`](#static-event-tableondelete)
            -   [Static Event `{TABLE}.OnUpdate`](#static-event-tableonupdate)
    -   [Observe and invoke reducers](#observe-and-invoke-reducers)
        -   [Class `Reducer`](#class-reducer)
            -   [Static Method `Reducer.{REDUCER}`](#static-method-reducerreducer)
            -   [Static Event `Reducer.On{REDUCER}`](#static-event-reduceronreducer)
        -   [Class `ReducerEvent`](#class-reducerevent)
            -   [Enum `Status`](#enum-status)
                -   [Variant `Status.Committed`](#variant-statuscommitted)
                -   [Variant `Status.Failed`](#variant-statusfailed)
                -   [Variant `Status.OutOfEnergy`](#variant-statusoutofenergy)
    -   [Identity management](#identity-management)
        -   [Class `AuthToken`](#class-authtoken)
            -   [Static Method `AuthToken.Init`](#static-method-authtokeninit)
            -   [Static Property `AuthToken.Token`](#static-property-authtokentoken)
            -   [Static Method `AuthToken.SaveToken`](#static-method-authtokensavetoken)
        -   [Class `Identity`](#class-identity)
        -   [Class `Identity`](#class-identity-1)
    -   [Customizing logging](#customizing-logging)
        -   [Interface `ISpacetimeDBLogger`](#interface-ispacetimedblogger)
        -   [Class `ConsoleLogger`](#class-consolelogger)
        -   [Class `UnityDebugLogger`](#class-unitydebuglogger)

## Install the SDK

### Using the `dotnet` CLI tool

If you would like to create a console application using .NET, you can create a new project using `dotnet new console` and add the SpacetimeDB SDK to your dependencies:

```bash
dotnet add package spacetimedbsdk
```

(See also the [CSharp Quickstart](/docs/modules/c-sharp/quickstart) for an in-depth example of such a console application.)

### Using Unity

To install the SpacetimeDB SDK into a Unity project, [download the SpacetimeDB SDK](https://github.com/clockworklabs/com.clockworklabs.spacetimedbsdk/releases/latest), packaged as a `.unitypackage`.

In Unity navigate to the `Assets > Import Package > Custom Package` menu in the menu bar. Select your `SpacetimeDB.Unity.Comprehensive.Tutorial.unitypackage` file and leave all folders checked.

(See also the [Unity Tutorial](/docs/unity/part-1))

## Generate module bindings

Each SpacetimeDB client depends on some bindings specific to your module. Create a `module_bindings` directory in your project's directory and generate the C# interface files using the Spacetime CLI. From your project directory, run:

```bash
mkdir -p module_bindings
spacetime generate --lang cs --out-dir module_bindings --project-path PATH-TO-MODULE-DIRECTORY
```

Replace `PATH-TO-MODULE-DIRECTORY` with the path to your SpacetimeDB module.

## Initialization

### Static Method `SpacetimeDBClient.CreateInstance`

```cs
namespace SpacetimeDB {

public class SpacetimeDBClient {
    public static void CreateInstance(ISpacetimeDBLogger loggerToUse);
}

}
```

Create a global SpacetimeDBClient instance, accessible via [`SpacetimeDBClient.instance`](#property-spacetimedbclientinstance)

| Argument      | Type                                                  | Meaning                           |
| ------------- | ----------------------------------------------------- | --------------------------------- |
| `loggerToUse` | [`ISpacetimeDBLogger`](#interface-ispacetimedblogger) | The logger to use to log messages |

There is a provided logger called [`ConsoleLogger`](#class-consolelogger) which logs to `System.Console`, and can be used as follows:

```cs
using SpacetimeDB;
using SpacetimeDB.Types;
SpacetimeDBClient.CreateInstance(new ConsoleLogger());
```

### Property `SpacetimeDBClient.instance`

```cs
namespace SpacetimeDB {

public class SpacetimeDBClient {
    public static SpacetimeDBClient instance;
}

}
```

This is the global instance of a SpacetimeDB client in a particular .NET/Unity process. Much of the SDK is accessible through this instance.

### Class `NetworkManager`

The Unity SpacetimeDB SDK relies on there being a `NetworkManager` somewhere in the scene. Click on the GameManager object in the scene, and in the inspector, add the `NetworkManager` component.

![Unity-AddNetworkManager](/images/unity-tutorial/Unity-AddNetworkManager.JPG)

This component will handle calling [`SpacetimeDBClient.CreateInstance`](#static-method-spacetimedbclientcreateinstance) for you, but will not call [`SpacetimeDBClient.Connect`](#method-spacetimedbclientconnect), you still need to handle that yourself. See the [Unity Quickstart](./UnityQuickStart) and [Unity Tutorial](./UnityTutorialPart1) for more information.

### Method `SpacetimeDBClient.Connect`

```cs
namespace SpacetimeDB {

class SpacetimeDBClient {
    public void Connect(
        string? token,
        string host,
        string addressOrName,
        bool sslEnabled = true
    );
}

}
```

<!-- FIXME: `token` is not currently marked as nullable in the API, but it should be. -->

Connect to a database named `addressOrName` accessible over the internet at the URI `host`.

| Argument        | Type      | Meaning                                                                    |
| --------------- | --------- | -------------------------------------------------------------------------- |
| `token`         | `string?` | Identity token to use, if one is available.                                |
| `host`          | `string`  | URI of the SpacetimeDB instance running the module.                        |
| `addressOrName` | `string`  | Address or name of the module.                                             |
| `sslEnabled`    | `bool`    | Whether or not to use SSL when connecting to SpacetimeDB. Default: `true`. |

If a `token` is supplied, it will be passed to the new connection to identify and authenticate the user. Otherwise, a new token and [`Identity`](#class-identity) will be generated by the server and returned in [`onConnect`](#event-spacetimedbclientonconnect).

```cs
using SpacetimeDB;
using SpacetimeDB.Types;

const string DBNAME = "chat";

// Connect to a local DB with a fresh identity
SpacetimeDBClient.instance.Connect(null, "localhost:3000", DBNAME, false);

// Connect to cloud with a fresh identity
SpacetimeDBClient.instance.Connect(null, "dev.spacetimedb.net", DBNAME, true);

// Connect to cloud using a saved identity from the filesystem, or get a new one and save it
AuthToken.Init();
Identity localIdentity;
SpacetimeDBClient.instance.Connect(AuthToken.Token, "dev.spacetimedb.net", DBNAME, true);
SpacetimeDBClient.instance.onIdentityReceived += (string authToken, Identity identity, Address address) {
    AuthToken.SaveToken(authToken);
    localIdentity = identity;
}
```

(You should probably also store the returned `Identity` somewhere; see the [`onIdentityReceived`](#event-spacetimedbclientonidentityreceived) event.)

### Event `SpacetimeDBClient.onIdentityReceived`

```cs
namespace SpacetimeDB {

class SpacetimeDBClient {
    public event Action<string, Identity, Address> onIdentityReceived;
}

}
```

+Called when we receive an auth token, [`Identity`](#class-identity) and [`Address`](#class-address) from the server. The [`Identity`](#class-identity) serves as a unique public identifier for a user of the database. It can be for several purposes, such as filtering rows in a database for the rows created by a particular user. The auth token is a private access token that allows us to assume an identity. The [`Address`](#class-address) is opaque identifier for a client connection to a database, intended to differentiate between connections from the same [`Identity`](#class-identity).

To store the auth token to the filesystem, use the static method [`AuthToken.SaveToken`](#static-method-authtokensavetoken). You may also want to store the returned [`Identity`](#class-identity) in a local variable.

If an existing auth token is used to connect to the database, the same auth token and the identity it came with will be returned verbatim in `onIdentityReceived`.

```cs
// Connect to cloud using a saved identity from the filesystem, or get a new one and save it
AuthToken.Init();
Identity localIdentity;
SpacetimeDBClient.instance.Connect(AuthToken.Token, "dev.spacetimedb.net", DBNAME, true);
SpacetimeDBClient.instance.onIdentityReceived += (string authToken, Identity identity, Address address) {
    AuthToken.SaveToken(authToken);
    localIdentity = identity;
}
```

### Event `SpacetimeDBClient.onConnect`

```cs
namespace SpacetimeDB {

class SpacetimeDBClient {
    public event Action onConnect;
}

}
```

Allows registering delegates to be invoked upon authentication with the database.

Once this occurs, the SDK is prepared for calls to [`SpacetimeDBClient.Subscribe`](#method-spacetimedbclientsubscribe).

## Subscribe to queries

### Method `SpacetimeDBClient.Subscribe`

```cs
namespace SpacetimeDB {

class SpacetimeDBClient {
    public void Subscribe(List<string> queries);
}

}
```

| Argument  | Type           | Meaning                      |
| --------- | -------------- | ---------------------------- |
| `queries` | `List<string>` | SQL queries to subscribe to. |

Subscribe to a set of queries, to be notified when rows which match those queries are altered.

`Subscribe` will return an error if called before establishing a connection with the [`SpacetimeDBClient.Connect`](#method-connect) function. In that case, the queries are not registered.

The `Subscribe` method does not return data directly. `spacetime generate` will generate classes [`SpacetimeDB.Types.{TABLE}`](#class-table) for each table in your module. These classes are used to reecive information from the database. See the section [View Rows of Subscribed Tables](#view-rows-of-subscribed-tables) for more information.

A new call to `Subscribe` will remove all previous subscriptions and replace them with the new `queries`. If any rows matched the previous subscribed queries but do not match the new queries, those rows will be removed from the client cache, and [`{TABLE}.OnDelete`](#event-tableondelete) callbacks will be invoked for them.

```cs
using SpacetimeDB;
using SpacetimeDB.Types;

void Main()
{
    AuthToken.Init();
    SpacetimeDBClient.CreateInstance(new ConsoleLogger());

    SpacetimeDBClient.instance.onConnect += OnConnect;

    // Our module contains a table named "Loot"
    Loot.OnInsert += Loot_OnInsert;

    SpacetimeDBClient.instance.Connect(/* ... */);
}

void OnConnect()
{
    SpacetimeDBClient.instance.Subscribe(new List<string> {
        "SELECT * FROM Loot"
    });
}

void Loot_OnInsert(
    Loot loot,
    ReducerEvent? event
) {
    Console.Log($"Loaded loot {loot.itemType} at coordinates {loot.position}");
}
```

### Event `SpacetimeDBClient.onSubscriptionApplied`

```cs
namespace SpacetimeDB {

class SpacetimeDBClient {
    public event Action onSubscriptionApplied;
}

}
```

Register a delegate to be invoked when a subscription is registered with the database.

```cs
using SpacetimeDB;

void OnSubscriptionApplied()
{
    Console.WriteLine("Now listening on queries.");
}

void Main()
{
    // ...initialize...
    SpacetimeDBClient.instance.onSubscriptionApplied += OnSubscriptionApplied;
}
```

### Method [`OneTimeQuery`](#method-spacetimedbclientsubscribe)

You may not want to subscribe to a query, but instead want to run a query once and receive the results immediately via a `Task` result:

```csharp
// Query all Messages from the sender "bob"
SpacetimeDBClient.instance.OneOffQuery<Message>("WHERE sender = \"bob\"");
```

## View rows of subscribed tables

The SDK maintains a local view of the database called the "client cache". This cache contains whatever rows are selected via a call to [`SpacetimeDBClient.Subscribe`](#method-spacetimedbclientsubscribe). These rows are represented in the SpacetimeDB .Net SDK as instances of [`SpacetimeDB.Types.{TABLE}`](#class-table).

ONLY the rows selected in a [`SpacetimeDBClient.Subscribe`](#method-spacetimedbclientsubscribe) call will be available in the client cache. All operations in the client sdk operate on these rows exclusively, and have no information about the state of the rest of the database.

In particular, SpacetimeDB does not support foreign key constraints. This means that if you are using a column as a foreign key, SpacetimeDB will not automatically bring in all of the rows that key might reference. You will need to manually subscribe to all tables you need information from.

To optimize network performance, prefer selecting as few rows as possible in your [`Subscribe`](#method-spacetimedbclientsubscribe) query. Processes that need to view the entire state of the database are better run inside the database -- that is, inside modules.

### Class `{TABLE}`

For each table defined by a module, `spacetime generate` will generate a class [`SpacetimeDB.Types.{TABLE}`](#class-table) whose name is that table's name converted to `PascalCase`. The generated class contains a property for each of the table's columns, whose names are the column names converted to `camelCase`. It also contains various static events and methods.

Static Methods:

-   [`{TABLE}.Iter()`](#static-method-tableiter) iterates all subscribed rows in the client cache.
-   [`{TABLE}.FilterBy{COLUMN}(value)`](#static-method-tablefilterbycolumn) filters subscribed rows in the client cache by a column value.
-   [`{TABLE}.Count()`](#static-method-tablecount) counts the number of subscribed rows in the client cache.

Static Events:

-   [`{TABLE}.OnInsert`](#static-event-tableoninsert) is called when a row is inserted into the client cache.
-   [`{TABLE}.OnBeforeDelete`](#static-event-tableonbeforedelete) is called when a row is about to be removed from the client cache.
-   If the table has a primary key attribute, [`{TABLE}.OnUpdate`](#static-event-tableonupdate) is called when a row is updated.
-   [`{TABLE}.OnDelete`](#static-event-tableondelete) is called while a row is being removed from the client cache. You should almost always use [`{TABLE}.OnBeforeDelete`](#static-event-tableonbeforedelete) instead.

Note that it is not possible to directly insert into the database from the client SDK! All insertion validation should be performed inside serverside modules for security reasons. You can instead [invoke reducers](#observe-and-invoke-reducers), which run code inside the database that can insert rows for you.

#### Static Method `{TABLE}.Iter`

```cs
namespace SpacetimeDB.Types {

class TABLE {
    public static System.Collections.Generic.IEnumerable<TABLE> Iter();
}

}
```

Iterate over all the subscribed rows in the table. This method is only available after [`SpacetimeDBClient.onSubscriptionApplied`](#event-spacetimedbclientonsubscriptionapplied) has occurred.

When iterating over rows and filtering for those containing a particular column, [`TableType::filter`](#method-filter) will be more efficient, so prefer it when possible.

```cs
using SpacetimeDB;
using SpacetimeDB.Types;

SpacetimeDBClient.instance.onConnect += (string authToken, Identity identity) => {
    SpacetimeDBClient.instance.Subscribe(new List<string> { "SELECT * FROM User" });
};
SpacetimeDBClient.instance.onSubscriptionApplied += () => {
    // Will print a line for each `User` row in the database.
    foreach (var user in User.Iter()) {
        Console.WriteLine($"User: {user.Name}");
    }
};
SpacetimeDBClient.instance.connect(/* ... */);
```

#### Static Method `{TABLE}.FilterBy{COLUMN}`

```cs
namespace SpacetimeDB.Types {

class TABLE {
    // If the column has no #[unique] or #[primarykey] constraint
    public static System.Collections.Generic.IEnumerable<TABLE> FilterBySender(COLUMNTYPE value);

    // If the column has a #[unique] or #[primarykey] constraint
    public static TABLE? FilterBySender(COLUMNTYPE value);
}

}
```

For each column of a table, `spacetime generate` generates a static method on the [table class](#class-table) to filter or seek subscribed rows where that column matches a requested value. These methods are named `filterBy{COLUMN}`, where `{COLUMN}` is the column name converted to `PascalCase`.

The method's return type depends on the column's attributes:

-   For unique columns, including those annotated `#[unique]` and `#[primarykey]`, the `filterBy{COLUMN}` method returns a `{TABLE}?`, where `{TABLE}` is the [table class](#class-table).
-   For non-unique columns, the `filter_by` method returns an `IEnumerator<{TABLE}>`.

#### Static Method `{TABLE}.Count`

```cs
namespace SpacetimeDB.Types {

class TABLE {
    public static int Count();
}

}
```

Return the number of subscribed rows in the table, or 0 if there is no active connection.

```cs
using SpacetimeDB;
using SpacetimeDB.Types;

SpacetimeDBClient.instance.onConnect += (string authToken, Identity identity) => {
    SpacetimeDBClient.instance.Subscribe(new List<string> { "SELECT * FROM User" });
};
SpacetimeDBClient.instance.onSubscriptionApplied += () => {
    Console.WriteLine($"There are {User.Count()} users in the database.");
};
SpacetimeDBClient.instance.connect(/* ... */);
```

#### Static Event `{TABLE}.OnInsert`

```cs
namespace SpacetimeDB.Types {

class TABLE {
    public delegate void InsertEventHandler(
        TABLE insertedValue,
        ReducerEvent? dbEvent
    );
    public static event InsertEventHandler OnInsert;
}

}
```

Register a delegate for when a subscribed row is newly inserted into the database.

The delegate takes two arguments:

-   A [`{TABLE}`](#class-table) instance with the data of the inserted row
-   A [`ReducerEvent?`], which contains the data of the reducer that inserted the row, or `null` if the row is being inserted while initializing a subscription.

```cs
using SpacetimeDB;
using SpacetimeDB.Types;

/* initialize, subscribe to table User... */

User.OnInsert += (User user, ReducerEvent? reducerEvent) => {
    if (reducerEvent == null) {
        Console.WriteLine($"New user '{user.Name}' received during subscription update.");
    } else {
        Console.WriteLine($"New user '{user.Name}' inserted by reducer {reducerEvent.Reducer}.");
    }
};
```

#### Static Event `{TABLE}.OnBeforeDelete`

```cs
namespace SpacetimeDB.Types {

class TABLE {
    public delegate void DeleteEventHandler(
        TABLE deletedValue,
        ReducerEvent dbEvent
    );
    public static event DeleteEventHandler OnBeforeDelete;
}

}
```

Register a delegate for when a subscribed row is about to be deleted from the database. If a reducer deletes many rows at once, this delegate will be invoked for each of those rows before any of them is deleted.

The delegate takes two arguments:

-   A [`{TABLE}`](#class-table) instance with the data of the deleted row
-   A [`ReducerEvent`](#class-reducerevent), which contains the data of the reducer that deleted the row.

This event should almost always be used instead of [`OnDelete`](#static-event-tableondelete). This is because often, many rows will be deleted at once, and `OnDelete` can be invoked in an arbitrary order on these rows. This means that data related to a row may already be missing when `OnDelete` is called. `OnBeforeDelete` does not have this problem.

```cs
using SpacetimeDB;
using SpacetimeDB.Types;

/* initialize, subscribe to table User... */

User.OnBeforeDelete += (User user, ReducerEvent reducerEvent) => {
    Console.WriteLine($"User '{user.Name}' deleted by reducer {reducerEvent.Reducer}.");
};
```

#### Static Event `{TABLE}.OnDelete`

```cs
namespace SpacetimeDB.Types {

class TABLE {
    public delegate void DeleteEventHandler(
        TABLE deletedValue,
        SpacetimeDB.ReducerEvent dbEvent
    );
    public static event DeleteEventHandler OnDelete;
}

}
```

Register a delegate for when a subscribed row is being deleted from the database. If a reducer deletes many rows at once, this delegate will be invoked on those rows in arbitrary order, and data for some rows may already be missing when it is invoked. For this reason, prefer the event [`{TABLE}.OnBeforeDelete`](#static-event-tableonbeforedelete).

The delegate takes two arguments:

-   A [`{TABLE}`](#class-table) instance with the data of the deleted row
-   A [`ReducerEvent`](#class-reducerevent), which contains the data of the reducer that deleted the row.

```cs
using SpacetimeDB;
using SpacetimeDB.Types;

/* initialize, subscribe to table User... */

User.OnBeforeDelete += (User user, ReducerEvent reducerEvent) => {
    Console.WriteLine($"User '{user.Name}' deleted by reducer {reducerEvent.Reducer}.");
};
```

#### Static Event `{TABLE}.OnUpdate`

```cs
namespace SpacetimeDB.Types {

class TABLE {
    public delegate void UpdateEventHandler(
        TABLE oldValue,
        TABLE newValue,
        ReducerEvent dbEvent
    );
    public static event UpdateEventHandler OnUpdate;
}

}
```

Register a delegate for when a subscribed row is being updated. This event is only available if the row has a column with the `#[primary_key]` attribute.

The delegate takes three arguments:

-   A [`{TABLE}`](#class-table) instance with the old data of the updated row
-   A [`{TABLE}`](#class-table) instance with the new data of the updated row
-   A [`ReducerEvent`](#class-reducerevent), which contains the data of the reducer that updated the row.

```cs
using SpacetimeDB;
using SpacetimeDB.Types;

/* initialize, subscribe to table User... */

User.OnUpdate += (User oldUser, User newUser, ReducerEvent reducerEvent) => {
    Debug.Assert(oldUser.UserId == newUser.UserId, "Primary key never changes in an update");

    Console.WriteLine($"User with ID {oldUser.UserId} had name changed "+
    $"from '{oldUser.Name}' to '{newUser.Name}' by reducer {reducerEvent.Reducer}.");
};
```

## Observe and invoke reducers

"Reducer" is SpacetimeDB's name for the stored procedures that run in modules inside the database. You can invoke reducers from a connected client SDK, and also receive information about which reducers are running.

`spacetime generate` generates a class [`SpacetimeDB.Types.Reducer`](#class-reducer) that contains methods and events for each reducer defined in a module. To invoke a reducer, use the method [`Reducer.{REDUCER}`](#static-method-reducerreducer) generated for it. To receive a callback each time a reducer is invoked, use the static event [`Reducer.On{REDUCER}`](#static-event-reduceronreducer).

### Class `Reducer`

```cs
namespace SpacetimeDB.Types {

class Reducer {}

}
```

This class contains a static method and event for each reducer defined in a module.

#### Static Method `Reducer.{REDUCER}`

```cs
namespace SpacetimeDB.Types {
class Reducer {

/* void {REDUCER_NAME}(...ARGS...) */

}
}
```

For each reducer defined by a module, `spacetime generate` generates a static method which sends a request to the database to invoke that reducer. The generated function's name is the reducer's name converted to `PascalCase`.

Reducers don't run immediately! They run as soon as the request reaches the database. Don't assume data inserted by a reducer will be available immediately after you call this method.

For reducers which accept a `ReducerContext` as their first argument, the `ReducerContext` is not included in the generated function's argument list.

For example, if we define a reducer in Rust as follows:

```rust
#[spacetimedb(reducer)]
pub fn set_name(
    ctx: ReducerContext,
    user_id: u64,
    name: String
) -> Result<(), Error>;
```

The following C# static method will be generated:

```cs
namespace SpacetimeDB.Types {
class Reducer {

public static void SendMessage(UInt64 userId, string name);

}
}
```

#### Static Event `Reducer.On{REDUCER}`

```cs
namespace SpacetimeDB.Types {
class Reducer {

public delegate void /*{REDUCER}*/Handler(ReducerEvent reducerEvent, /* {ARGS...} */);

public static event /*{REDUCER}*/Handler On/*{REDUCER}*/Event;

}
}
```

For each reducer defined by a module, `spacetime generate` generates an event to run each time the reducer is invoked. The generated functions are named `on{REDUCER}Event`, where `{REDUCER}` is the reducer's name converted to `PascalCase`.

The first argument to the event handler is an instance of [`SpacetimeDB.Types.ReducerEvent`](#class-reducerevent) describing the invocation -- its timestamp, arguments, and whether it succeeded or failed. The remaining arguments are the arguments passed to the reducer. Reducers cannot have return values, so no return value information is included.

For example, if we define a reducer in Rust as follows:

```rust
#[spacetimedb(reducer)]
pub fn set_name(
    ctx: ReducerContext,
    user_id: u64,
    name: String
) -> Result<(), Error>;
```

The following C# static method will be generated:

```cs
namespace SpacetimeDB.Types {
class Reducer {

public delegate void SetNameHandler(
    ReducerEvent reducerEvent,
    UInt64 userId,
    string name
);
public static event SetNameHandler OnSetNameEvent;

}
}
```

Which can be used as follows:

```cs
/* initialize, wait for onSubscriptionApplied... */

Reducer.SetNameHandler += (
    ReducerEvent reducerEvent,
    UInt64 userId,
    string name
) => {
    if (reducerEvent.Status == ClientApi.Event.Types.Status.Committed) {
        Console.WriteLine($"User with id {userId} set name to {name}");
    } else if (reducerEvent.Status == ClientApi.Event.Types.Status.Failed) {
        Console.WriteLine(
            $"User with id {userId} failed to set name to {name}:"
            + reducerEvent.ErrMessage
        );
    } else if (reducerEvent.Status == ClientApi.Event.Types.Status.OutOfEnergy) {
        Console.WriteLine(
            $"User with id {userId} failed to set name to {name}:"
            + "Invoker ran out of energy"
        );
    }
};
Reducer.SetName(USER_ID, NAME);
```

### Class `ReducerEvent`

`spacetime generate` defines an class `ReducerEvent` containing an enum `ReducerType` with a variant for each reducer defined by a module. The variant's name will be the reducer's name converted to `PascalCase`.

For example, the example project shown in the Rust Module quickstart will generate the following (abridged) code.

```cs
namespace SpacetimeDB.Types {

public enum ReducerType
{
    /* A member for each reducer in the module, with names converted to PascalCase */
    None,
    SendMessage,
    SetName,
}
public partial class SendMessageArgsStruct
{
    /* A member for each argument of the reducer SendMessage, with names converted to PascalCase. */
    public string Text;
}
public partial class SetNameArgsStruct
{
    /* A member for each argument of the reducer SetName, with names converted to PascalCase. */
    public string Name;
}
public partial class ReducerEvent : ReducerEventBase {
    // Which reducer was invoked
    public ReducerType Reducer { get; }
    // If event.Reducer == ReducerType.SendMessage, the arguments
    // sent to the SendMessage reducer. Otherwise, accesses will
    // throw a runtime error.
    public SendMessageArgsStruct SendMessageArgs { get; }
    // If event.Reducer == ReducerType.SetName, the arguments
    // passed to the SetName reducer. Otherwise, accesses will
    // throw a runtime error.
    public SetNameArgsStruct SetNameArgs { get; }

    /* Additional information, present on any ReducerEvent */
    // The name of the reducer.
    public string ReducerName { get; }
    // The timestamp of the reducer invocation inside the database.
    public ulong Timestamp { get; }
    // The identity of the client that invoked the reducer.
    public SpacetimeDB.Identity Identity { get; }
    // Whether the reducer succeeded, failed, or ran out of energy.
    public ClientApi.Event.Types.Status Status { get; }
    // If event.Status == Status.Failed, the error message returned from inside the module.
    public string ErrMessage { get; }
}

}
```

#### Enum `Status`

```cs
namespace ClientApi {
public sealed partial class Event {
public static partial class Types {

public enum Status {
    Committed = 0,
    Failed = 1,
    OutOfEnergy = 2,
}

}
}
}
```

An enum whose variants represent possible reducer completion statuses of a reducer invocation.

##### Variant `Status.Committed`

The reducer finished successfully, and its row changes were committed to the database.

##### Variant `Status.Failed`

The reducer failed, either by panicking or returning a `Err`.

##### Variant `Status.OutOfEnergy`

The reducer was canceled because the module owner had insufficient energy to allow it to run to completion.

## Identity management

### Class `AuthToken`

The AuthToken helper class handles creating and saving SpacetimeDB identity tokens in the filesystem.

#### Static Method `AuthToken.Init`

```cs
namespace SpacetimeDB {

class AuthToken {
    public static void Init(
        string configFolder = ".spacetime_csharp_sdk",
        string configFile = "settings.ini",
        string? configRoot = null
    );
}

}
```

Creates a file `$"{configRoot}/{configFolder}/{configFile}"` to store tokens.
If no arguments are passed, the default is `"%HOME%/.spacetime_csharp_sdk/settings.ini"`.

| Argument       | Type     | Meaning                                                                            |
| -------------- | -------- | ---------------------------------------------------------------------------------- |
| `configFolder` | `string` | The folder to store the config file in. Default is `"spacetime_csharp_sdk"`.       |
| `configFile`   | `string` | The name of the config file. Default is `"settings.ini"`.                          |
| `configRoot`   | `string` | The root folder to store the config file in. Default is the user's home directory. |

#### Static Property `AuthToken.Token`

```cs
namespace SpacetimeDB {

class AuthToken {
    public static string? Token { get; }
}

}
```

The auth token stored on the filesystem, if one exists.

#### Static Method `AuthToken.SaveToken`

```cs
namespace SpacetimeDB {

class AuthToken {
    public static void SaveToken(string token);
}

}
```

Save a token to the filesystem.

### Class `Identity`

```cs
namespace SpacetimeDB
{
    public struct Identity : IEquatable<Identity>
    {
        public byte[] Bytes { get; }
        public static Identity From(byte[] bytes);
        public bool Equals(Identity other);
        public static bool operator ==(Identity a, Identity b);
        public static bool operator !=(Identity a, Identity b);
    }
}
```

A unique public identifier for a user of a database.

<!-- FIXME: this is no longer accurate; `Identity` columns are properly `Identity`-type. -->

Columns of type `Identity` inside a module will be represented in the C# SDK as properties of type `byte[]`. `Identity` is essentially just a wrapper around `byte[]`, and you can use the `Bytes` property to get a `byte[]` that can be used to filter tables and so on.

### Class `Identity`

```cs
namespace SpacetimeDB
{
    public struct Address : IEquatable<Address>
    {
        public byte[] Bytes { get; }
        public static Address? From(byte[] bytes);
        public bool Equals(Address other);
        public static bool operator ==(Address a, Address b);
        public static bool operator !=(Address a, Address b);
    }
}
```

An opaque identifier for a client connection to a database, intended to differentiate between connections from the same [`Identity`](#class-identity).

## Customizing logging

The SpacetimeDB C# SDK performs internal logging. Instances of [`ISpacetimeDBLogger`](#interface-ispacetimedblogger) can be passed to [`SpacetimeDBClient.CreateInstance`](#static-method-spacetimedbclientcreateinstance) to customize how SDK logs are delivered to your application.

This is set up automatically for you if you use Unity-- adding a [`NetworkManager`](#class-networkmanager) component to your unity scene will automatically initialize the `SpacetimeDBClient` with a [`UnityDebugLogger`](#class-unitydebuglogger).

Outside of unity, all you need to do is the following:

```cs
using SpacetimeDB;
using SpacetimeDB.Types;
SpacetimeDBClient.CreateInstance(new ConsoleLogger());
```

### Interface `ISpacetimeDBLogger`

```cs
namespace SpacetimeDB
{

public interface ISpacetimeDBLogger
{
    void Log(string message);
    void LogError(string message);
    void LogWarning(string message);
    void LogException(Exception e);
}

}
```

This interface provides methods that are invoked when the SpacetimeDB C# SDK needs to log at various log levels. You can create custom implementations if needed to integrate with existing logging solutions.

### Class `ConsoleLogger`

```cs
namespace SpacetimeDB {

public class ConsoleLogger : ISpacetimeDBLogger {}

}
```

An `ISpacetimeDBLogger` implementation for regular .NET applications, using `Console.Write` when logs are received.

### Class `UnityDebugLogger`

```cs
namespace SpacetimeDB {

public class UnityDebugLogger : ISpacetimeDBLogger {}

}
```

An `ISpacetimeDBLogger` implementation for Unity, using the Unity `Debug.Log` api.
