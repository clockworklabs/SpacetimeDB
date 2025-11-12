---
title: 2 - Connecting to SpacetimeDB
slug: /unity/part-2
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Connecting to SpacetimeDB

Need help with the tutorial? [Join our Discord server](https://discord.gg/spacetimedb)!

This progressive tutorial is continued from [part 1](/unity/part-1).

## Project Structure

Now that we have our client project setup we can configure the module directory. Regardless of what language you choose, your module will always go into a `spacetimedb` directory within your client directory like this:

```
blackholio/                     # This is the directory for your Unity project lives
├── Assembly-CSharp.csproj
├── Assets/
│   └── module_bindings/        # This directory contains the client logic to communicate with the module
├── Library/
├── ...                         # rest of the Unity files
└── spacetimedb/                # This is where your server module lives
```

Your `module_bindings` directory can go wherever you want as long as it is inside of `Assets/` in your Unity project. We'll configure this in a later step. For now we will create a new module in the `blackholio` directory which will generate the `spacetimedb` directory for us.


## Create a Server Module

If you have not already installed the `spacetime` CLI, check out our [Getting Started](/getting-started) guide for instructions on how to install.

In the same directory that contains your `blackholio` project, run the following command to initialize the SpacetimeDB server module project with your desired language:

:::warning
The `blackholio` directory specified here is the same `blackholio` directory you created during part 1.
:::

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
Run the following command to initialize the SpacetimeDB server module project with Rust as the language:

```bash
spacetime init --lang rust --server-only blackholio
```

This command creates a new folder named `spacetimedb` inside of your Unity project `blackholio` directory and sets up the SpacetimeDB server project with Rust as the programming language.

</TabItem>
<TabItem value="csharp" label="C#">
Run the following command to initialize the SpacetimeDB server module project with C# as the language:

```bash
spacetime init --lang csharp --server-only blackholio
```

This command creates a new folder named `spacetimedb` inside of your Unity project `blackholio` directory and sets up the SpacetimeDB server project with C# as the programming language.

</TabItem>
</Tabs>

### SpacetimeDB Tables

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
In this section we'll be making some edits to the file `blackholio/spacetimedb/src/lib.rs`. We recommend you open up this file in an IDE like VSCode or RustRover.

**Important: Open the `blackholio/spacetimedb/src/lib.rs` file and delete its contents. We will be writing it from scratch here.**

</TabItem>
<TabItem value="csharp" label="C#">
In this section we'll be making some edits to the file `blackholio/spacetimedb/Lib.cs`. We recommend you open up this file in an IDE like VSCode or Rider.

**Important: Open the `blackholio/spacetimedb/Lib.cs` file and delete its contents. We will be writing it from scratch here.**

</TabItem>
</Tabs>

First we need to add some imports at the top of the file. Some will remain unused for now.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
**Copy and paste into lib.rs:**

```rust
use std::time::Duration;
use spacetimedb::{rand::Rng, Identity, SpacetimeType, ReducerContext, ScheduleAt, Table, Timestamp};
```

</TabItem>
<TabItem value="csharp" label="C#">
**Copy and paste into Lib.cs:**

```csharp
using SpacetimeDB;

public static partial class Module
{

}
```

</TabItem>
</Tabs>

We are going to start by defining a SpacetimeDB _table_. A _table_ in SpacetimeDB is a relational database table which stores rows, similar to something you might find in SQL. SpacetimeDB tables differ from normal relational database tables in that they are stored fully in memory, are blazing fast to access, and are defined in your module code, rather than in SQL.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
Each row in a SpacetimeDB table is associated with a `struct` type in Rust.

Let's start by defining the `Config` table. This is a simple table which will store some metadata about our game's state. Add the following code to `lib.rs`.

```rust
// We're using this table as a singleton, so in this table
// there only be one element where the `id` is 0.
#[spacetimedb::table(name = config, public)]
pub struct Config {
    #[primary_key]
    pub id: i32,
    pub world_size: i64,
}
```

Let's break down this code. This defines a normal Rust `struct` with two fields: `id` and `world_size`. We have decorated the struct with the `spacetimedb::table` macro. This procedural Rust macro signals to SpacetimeDB that it should create a new SpacetimeDB table with the row type defined by the `Config` type's fields.

The `spacetimedb::table` macro takes two parameters, a `name` which is the name of the table and what you will use to query the table in SQL, and a `public` visibility modifier which ensures that the rows of this table are visible to everyone.

The `#[primary_key]` attribute, specifies that the `id` field should be used as the primary key of the table.

</TabItem>
<TabItem value="csharp" label="C#">
Each row in a SpacetimeDB table is associated with a `struct` type in C#.

Let's start by defining the `Config` table. This is a simple table which will store some metadata about our game's state. Add the following code inside the `Module` class in `Lib.cs`.

```csharp
// We're using this table as a singleton, so in this table
// there will only be one element where the `id` is 0.
[Table(Name = "config", Public = true)]
public partial struct Config
{
    [PrimaryKey]
    public int id;
    public long world_size;
}
```

Let's break down this code. This defines a normal C# `struct` with two fields: `id` and `world_size`. We have added the `[Table(Name = "config", Public = true)]` attribute the struct. This attribute signals to SpacetimeDB that it should create a new SpacetimeDB table with the row type defined by the `Config` type's fields.

> Although we're using `lower_snake_case` for our column names to have consistent column names across languages in this tutorial, you can also use `camelCase` or `PascalCase` if you prefer. See [#2168](https://github.com/clockworklabs/SpacetimeDB/issues/2168) for more information.

The `Table` attribute takes two parameters, a `Name` which is the name of the table and what you will use to query the table in SQL, and a `Public` visibility modifier which ensures that the rows of this table are visible to everyone.

The `[PrimaryKey]` attribute, specifies that the `id` field should be used as the primary key of the table.

</TabItem>
</Tabs>

> NOTE: The primary key of a row defines the "identity" of the row. A change to a row which doesn't modify the primary key is considered an update, but if you change the primary key, then you have deleted the old row and inserted a new one.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
You can learn more the `table` macro in our [Rust module reference](/modules/rust).

</TabItem>
<TabItem value="csharp" label="C#">
You can learn more the `Table` attribute in our [C# module reference](/modules/c-sharp).
</TabItem>
</Tabs>

### Creating Entities

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
Next, we're going to define a new `SpacetimeType` called `DbVector2` which we're going to use to store positions. The difference between a `#[derive(SpacetimeType)]` and a `#[spacetimedb(table)]` is that tables actually store data, whereas the deriving `SpacetimeType` just allows you to create a new column of that type in a SpacetimeDB table. Therefore, `DbVector2` is only a type, and does not define a table.

**Append to the bottom of lib.rs:**

```rust
// This allows us to store 2D points in tables.
#[derive(SpacetimeType, Clone, Debug)]
pub struct DbVector2 {
    pub x: f32,
    pub y: f32,
}
```

Let's create a few tables to represent entities in our game.

```rust
#[spacetimedb::table(name = entity, public)]
#[derive(Debug, Clone)]
pub struct Entity {
    // The `auto_inc` attribute indicates to SpacetimeDB that
    // this value should be determined by SpacetimeDB on insert.
    #[auto_inc]
    #[primary_key]
    pub entity_id: i32,
    pub position: DbVector2,
    pub mass: i32,
}

#[spacetimedb::table(name = circle, public)]
pub struct Circle {
    #[primary_key]
    pub entity_id: i32,
    #[index(btree)]
    pub player_id: i32,
    pub direction: DbVector2,
    pub speed: f32,
    pub last_split_time: Timestamp,
}

#[spacetimedb::table(name = food, public)]
pub struct Food {
    #[primary_key]
    pub entity_id: i32,
}
```

</TabItem>
<TabItem value="csharp" label="C#">
Next, we're going to define a new `SpacetimeType` called `DbVector2` which we're going to use to store positions. The difference between a `[SpacetimeDB.Type]` and a `[SpacetimeDB.Table]` is that tables actually store data, whereas the deriving `SpacetimeType` just allows you to create a new column of that type in a SpacetimeDB table. Therefore, `DbVector2` is only a type, and does not define a table.

**Append to the bottom of Lib.cs:**

```csharp
// This allows us to store 2D points in tables.
[SpacetimeDB.Type]
public partial struct DbVector2
{
    public float x;
    public float y;

    public DbVector2(float x, float y)
    {
        this.x = x;
        this.y = y;
    }
}
```

Let's create a few tables to represent entities in our game by adding the following to the end of the `Module` class.

```csharp
[Table(Name = "entity", Public = true)]
public partial struct Entity
{
    [PrimaryKey, AutoInc]
    public int entity_id;
    public DbVector2 position;
    public int mass;
}

[Table(Name = "circle", Public = true)]
public partial struct Circle
{
    [PrimaryKey]
    public int entity_id;
    [SpacetimeDB.Index.BTree]
    public int player_id;
    public DbVector2 direction;
    public float speed;
    public SpacetimeDB.Timestamp last_split_time;
}

[Table(Name = "food", Public = true)]
public partial struct Food
{
    [PrimaryKey]
    public int entity_id;
}
```

</TabItem>
</Tabs>

The first table we defined is the `entity` table. An entity represents an object in our game world. We have decided, for convenience, that all entities in our game should share some common fields, namely `position` and `mass`.

We can create different types of entities with additional data by creating new tables with additional fields that have an `entity_id` which references a row in the `entity` table.

We've created two types of entities in our game world: `Food`s and `Circle`s. `Food` does not have any additional fields beyond the attributes in the `entity` table, so the `food` table simply represents the set of `entity_id`s that we want to recognize as food.

The `Circle` table, however, represents an entity that is controlled by a player. We've added a few additional fields to a `Circle` like `player_id` so that we know which player that circle belongs to.

### Representing Players

Next, let's create a table to store our player data.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = player, public)]
#[derive(Debug, Clone)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    #[unique]
    #[auto_inc]
    player_id: i32,
    name: String,
}
```

There's a few new concepts we should touch on. First of all, we are using the `#[unique]` attribute on the `player_id` field. This attribute adds a constraint to the table that ensures that only one row in the player table has a particular `player_id`.

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[Table(Name = "player", Public = true)]
public partial struct Player
{
    [PrimaryKey]
    public Identity identity;
    [Unique, AutoInc]
    public int player_id;
    public string name;
}
```

There are a few new concepts we should touch on. First of all, we are using the `[Unique]` attribute on the `player_id` field. This attribute adds a constraint to the table that ensures that only one row in the player table has a particular `player_id`. We are also using the `[AutoInc]` attribute on the `player_id` field, which indicates "this field should get automatically assigned an auto-incremented value".

</TabItem>
</Tabs>

We also have an `identity` field which uses the `Identity` type. The `Identity` type is an identifier that SpacetimeDB uses to uniquely assign and authenticate SpacetimeDB users.

### Writing a Reducer

Next, we write our very first reducer. A reducer is a module function which can be called by clients. Let's write a simple debug reducer to see how they work.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::reducer]
pub fn debug(ctx: &ReducerContext) -> Result<(), String> {
    log::debug!("This reducer was called by {}.", ctx.sender);
    Ok(())
}
```

</TabItem>
<TabItem value="csharp" label="C#">

Add this function to the `Module` class in `Lib.cs`:

```csharp
[Reducer]
public static void Debug(ReducerContext ctx)
{
    Log.Info($"This reducer was called by {ctx.Sender}");
}
```

</TabItem>
</Tabs>

This reducer doesn't update any tables, it just prints out the `Identity` of the client that called it.

---

**SpacetimeDB Reducers**

"Reducer" is a term coined by Clockwork Labs that refers to a function which when executed "reduces" a set of inserts and deletes into the database state. The term derives from functional programming and is closely related to [similarly named concepts](https://redux.js.org/tutorials/fundamentals/part-2-concepts-data-flow#reducers) in other frameworks like React Redux. Reducers can be called remotely using the CLI, client SDK or can be scheduled to be called at some future time from another reducer call.

All reducers execute _transactionally_ and _atomically_, meaning that from within the reducer it will appear as though all changes are being applied to the database immediately, however from the outside changes made in a reducer will only be applied to the database once the reducer completes successfully. If you return an error from a reducer or panic within a reducer, all changes made to the database will be rolled back, as if the function had never been called. If you're unfamiliar with atomic transactions, it may not be obvious yet just how useful and important this feature is, but once you build a somewhat complex application it will become clear just how invaluable this feature is.

---

### Publishing the Module

Now that we have some basic functionality, let's publish the module to SpacetimeDB and call our debug reducer.

In a new terminal window, run a local version of SpacetimeDB with the command:

```sh
spacetime start
```

This following log output indicates that SpacetimeDB is successfully running on your machine.

```
Starting SpacetimeDB listening on 127.0.0.1:3000
```

Now that SpacetimeDB is running we can publish our module to the SpacetimeDB host. In a separate terminal window, navigate to the `blackholio/spacetimedb` directory.

If you are not already logged in to the `spacetime` CLI, run the `spacetime login` command to log in to your SpacetimeDB website account. Once you are logged in, run `spacetime publish --server local blackholio`. This will publish our Blackholio server logic to SpacetimeDB.

If the publish completed successfully, you will see something like the following in the logs:

```
Build finished successfully.
Uploading to local => http://127.0.0.1:3000
Publishing module...
Created new database with name: blackholio, identity: c200d2c69b4524292b91822afac8ab016c15968ac993c28711f68c6bc40b89d5
```

> If you sign into `spacetime login` via GitHub, the token you get will be issued by `auth.spacetimedb.com`. This will also ensure that you can recover your identity in case you lose it. On the other hand, if you do `spacetime login --server-issued-login local`, you will get an identity which is issued directly by your local server. Do note, however, that `--server-issued-login` tokens are not recoverable if lost, and are only recognized by the server that issued them.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

Next, use the `spacetime` command to call our newly defined `debug` reducer:

```sh
spacetime call --server local blackholio debug
```

</TabItem>
<TabItem value="csharp" label="C#">
Next, use the `spacetime` command to call our newly defined `Debug` reducer:

```sh
spacetime call --server local blackholio Debug
```

</TabItem>
</Tabs>

If the call completed successfully, that command will have no output, but we can see the debug logs by running:

```sh
spacetime logs --server local blackholio
```

You should see something like the following output:

```sh
2025-01-09T16:08:38.144299Z  INFO: spacetimedb: Creating table `circle`
2025-01-09T16:08:38.144438Z  INFO: spacetimedb: Creating table `config`
2025-01-09T16:08:38.144451Z  INFO: spacetimedb: Creating table `entity`
2025-01-09T16:08:38.144470Z  INFO: spacetimedb: Creating table `food`
2025-01-09T16:08:38.144479Z  INFO: spacetimedb: Creating table `player`
2025-01-09T16:08:38.144841Z  INFO: spacetimedb: Database initialized
2025-01-09T16:08:47.306823Z  INFO: src/lib.rs:68: This reducer was called by c200e1a6494dbeeb0bbf49590b8778abf94fae4ea26faf9769c9a8d69a3ec348.
```

### Connecting our Client

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
Next let's connect our client to our database. Let's start by modifying our `debug` reducer. Rename the reducer to be called `connect` and add `client_connected` in parentheses after `spacetimedb::reducer`. The end result should look like this:

```rust
#[spacetimedb::reducer(client_connected)]
pub fn connect(ctx: &ReducerContext) -> Result<(), String> {
    log::debug!("{} just connected.", ctx.sender);
    Ok(())
}
```

The `client_connected` argument to the `spacetimedb::reducer` macro indicates to SpacetimeDB that this is a special reducer. This reducer is only ever called by SpacetimeDB itself when a client connects to your database.

> SpacetimeDB gives you the ability to define custom reducers that automatically trigger when certain events occur.
>
> - `init` - Called the first time you publish your module and anytime you clear the database with `spacetime publish --server local <name> --delete-data`.
> - `client_connected` - Called when a user connects to the SpacetimeDB database. Their identity can be found in the `sender` value of the `ReducerContext`.
> - `client_disconnected` - Called when a user disconnects from the SpacetimeDB database.

</TabItem>
<TabItem value="csharp" label="C#">
Next let's connect our client to our database. Let's start by modifying our `Debug` reducer. Rename the reducer to be called `Connect` and add `ReducerKind.ClientConnected` in parentheses after `SpacetimeDB.Reducer`. The end result should look like this:

```csharp
[Reducer(ReducerKind.ClientConnected)]
public static void Connect(ReducerContext ctx)
{
    Log.Info($"{ctx.Sender} just connected.");
}
```

The `ReducerKind.ClientConnected` argument to the `SpacetimeDB.Reducer` attribute indicates to SpacetimeDB that this is a special reducer. This reducer is only ever called by SpacetimeDB itself when a client connects to your database.

> SpacetimeDB gives you the ability to define custom reducers that automatically trigger when certain events occur.
>
> - `ReducerKind.Init` - Called the first time you publish your module and anytime you clear the database with `spacetime publish --server local <name> --delete-data`.
> - `ReducerKind.ClientConnected` - Called when a user connects to the SpacetimeDB database. Their identity can be found in the `Sender` value of the `ReducerContext`.
> - `ReducerKind.ClientDisconnected` - Called when a user disconnects from the SpacetimeDB database.

</TabItem>
</Tabs>

Publish your module again by running:

```sh
spacetime publish --server local blackholio
```

### Generating the Client

The `spacetime` CLI has built in functionality to let us generate C# types that correspond to our tables, types, and reducers that we can use from our Unity client.

<Tabs groupId="server-language" defaultValue="rust">
  <TabItem value="rust" label="Rust">
    Let's generate our types for our module. In the `blackholio/server-rust`
    directory run the following command:
  </TabItem>
  <TabItem value="csharp" label="C#">
    Let's generate our types for our module. In the `blackholio/spacetimedb`
    directory run the following command:
  </TabItem>
</Tabs>

```sh
spacetime generate --lang csharp --out-dir ../client-unity/Assets/autogen # you can call this anything, I have chosen `autogen`
```

This will generate a set of files in the `client-unity/Assets/autogen` directory which contain the code generated types and reducer functions that are defined in your module, but usable on the client.

```
├── Reducers
│   └── Connect.g.cs
├── Tables
│   ├── Circle.g.cs
│   ├── Config.g.cs
│   ├── Entity.g.cs
│   ├── Food.g.cs
│   └── Player.g.cs
├── Types
│   ├── Circle.g.cs
│   ├── Config.g.cs
│   ├── DbVector2.g.cs
│   ├── Entity.g.cs
│   ├── Food.g.cs
│   └── Player.g.cs
└── SpacetimeDBClient.g.cs
```

This will also generate a file in the `client-unity/Assets/autogen/SpacetimeDBClient.g.cs` directory with a type aware `DbConnection` class. We will use this class to connect to your database from Unity.

> IMPORTANT! At this point there will be an error in your Unity project. Due to a [known issue](https://docs.unity3d.com/6000.0/Documentation/Manual/csharp-compiler.html) with Unity and C# 9 you need to insert the following code into your Unity project.
>
> ```csharp
> namespace System.Runtime.CompilerServices
> {
>     internal static class IsExternalInit { }
> }
> ```
>
> Add this snippet to the bottom of your `GameManager.cs` file in your Unity project. This will hopefully be resolved in Unity soon.

### Connecting to the Database

At this point we can set up Unity to connect your Unity client to the server. Replace your imports at the top of the `GameManager.cs` file with:

```cs
using System;
using System.Collections;
using System.Collections.Generic;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;
```

Replace the implementation of the `GameManager` class with the following.

```cs
public class GameManager : MonoBehaviour
{
    const string SERVER_URL = "http://127.0.0.1:3000";
    const string MODULE_NAME = "blackholio";

    public static event Action OnConnected;
    public static event Action OnSubscriptionApplied;

    public float borderThickness = 2;
    public Material borderMaterial;

    public static GameManager Instance { get; private set; }
    public static Identity LocalIdentity { get; private set; }
    public static DbConnection Conn { get; private set; }

    private void Start()
    {
        Instance = this;
        Application.targetFrameRate = 60;

        // In order to build a connection to SpacetimeDB we need to register
        // our callbacks and specify a SpacetimeDB server URI and module name.
        var builder = DbConnection.Builder()
            .OnConnect(HandleConnect)
            .OnConnectError(HandleConnectError)
            .OnDisconnect(HandleDisconnect)
            .WithUri(SERVER_URL)
            .WithModuleName(MODULE_NAME);

        // If the user has a SpacetimeDB auth token stored in the Unity PlayerPrefs,
        // we can use it to authenticate the connection.
        if (AuthToken.Token != "")
        {
            builder = builder.WithToken(AuthToken.Token);
        }

        // Building the connection will establish a connection to the SpacetimeDB
        // server.
        Conn = builder.Build();
    }

    // Called when we connect to SpacetimeDB and receive our client identity
    void HandleConnect(DbConnection _conn, Identity identity, string token)
    {
        Debug.Log("Connected.");
        AuthToken.SaveToken(token);
        LocalIdentity = identity;

        OnConnected?.Invoke();

        // Request all tables
        Conn.SubscriptionBuilder()
            .OnApplied(HandleSubscriptionApplied)
            .SubscribeToAllTables();
    }

    void HandleConnectError(Exception ex)
    {
        Debug.LogError($"Connection error: {ex}");
    }

    void HandleDisconnect(DbConnection _conn, Exception ex)
    {
        Debug.Log("Disconnected.");
        if (ex != null)
        {
            Debug.LogException(ex);
        }
    }

    private void HandleSubscriptionApplied(SubscriptionEventContext ctx)
    {
        Debug.Log("Subscription applied!");
        OnSubscriptionApplied?.Invoke();
    }

    public static bool IsConnected()
    {
        return Conn != null && Conn.IsActive;
    }

    public void Disconnect()
    {
        Conn.Disconnect();
        Conn = null;
    }
}
```

Here we configure the connection to the database, by passing it some callbacks in addition to providing the `SERVER_URI` and `MODULE_NAME` to the connection. When the client connects, the SpacetimeDB SDK will call the `HandleConnect` method, allowing us to start up the game.

In our `HandleConnect` callback we build a subscription and are calling `Subscribe` and subscribing to all data in the database. This will cause SpacetimeDB to synchronize the state of all your tables with your Unity client's SpacetimeDB SDK's "client cache". You can also subscribe to specific tables using SQL syntax, e.g. `SELECT * FROM my_table`. Our [SQL documentation](/sql) enumerates the operations that are accepted in our SQL syntax.

---

**SDK Client Cache**

The "SDK client cache" is a client-side view of the database defined by the supplied queries to the `Subscribe` function. SpacetimeDB ensures that the results of subscription queries are automatically updated and pushed to the client cache as they change which allows efficient access without unnecessary server queries.

---

Now we're ready to connect the client and server. Press the play button in Unity.

If all went well you should see the below output in your Unity logs.

```
SpacetimeDBClient: Connecting to ws://127.0.0.1:3000 blackholio
Connected.
Subscription applied!
```

Subscription applied indicates that the SpacetimeDB SDK has evaluated your subscription queries and synchronized your local cache with your database's tables.

We can also see that the server has logged the connection as well.

```sh
spacetime logs --server local blackholio
...
2025-01-10T03:51:02.078700Z DEBUG: src/lib.rs:63: c200fb5be9524bfb8289c351516a1d9ea800f70a17a9a6937f11c0ed3854087d just connected.
```

### Next Steps

You've learned how to setup a Unity project with the SpacetimeDB SDK, write a basic SpacetimeDB server module, and how to connect your Unity client to SpacetimeDB. That's pretty much all there is to the setup. You're now ready to start building the game.

In the [next part](/unity/part-3), we'll build out the functionality of the game and you'll learn how to access your table data and call reducers in Unity.
