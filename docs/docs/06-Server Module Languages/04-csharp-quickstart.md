---
title: C# Quickstart
slug: /modules/c-sharp/quickstart
---

# C# Module Quickstart

In this tutorial, we'll implement a simple chat server as a SpacetimeDB module.

A SpacetimeDB module is code that gets compiled to WebAssembly and is uploaded to SpacetimeDB. This code becomes server-side logic that interfaces directly with the Spacetime relational database.

Each SpacetimeDB module defines a set of tables and a set of reducers.

Each table is defined as a C# `class` annotated with `[SpacetimeDB.Table]`, where an instance represents a row, and each field represents a column.
By default, tables are **private**. This means that they are only readable by the table owner, and by server module code.
The `[SpacetimeDB.Table(Public = true))]` annotation makes a table public. **Public** tables are readable by all users, but can still only be modified by your server module code.

A reducer is a function which traverses and updates the database. Each reducer call runs in its own transaction, and its updates to the database are only committed if the reducer returns successfully. In C#, reducers are defined as functions annotated with `[SpacetimeDB.Reducer]`. If an exception is thrown, the reducer call fails, the database is not updated, and a failed message is reported to the client.

## Install SpacetimeDB

If you haven't already, start by [installing SpacetimeDB](https://spacetimedb.com/install). This will install the `spacetime` command line interface (CLI), which contains all the functionality for interacting with SpacetimeDB.

## Install .NET 8

Next we need to [install .NET 8 SDK](https://dotnet.microsoft.com/en-us/download/dotnet/8.0) so that we can build and publish our module.

You may already have .NET 8 and can be checked:

```bash
dotnet --list-sdks
```

.NET 8.0 is the earliest to have the `wasi-experimental` workload that we rely on, but requires manual activation:

```bash
dotnet workload install wasi-experimental
```

## Project structure

Let's start by running `spacetime init` to initialize our project's directory structure:

```bash
spacetime init --lang csharp quickstart-chat
```

`spacetime init` will ask you for a project path in which to put your project. By default this will be `./quickstart-chat`. This basic project will have a few helper files like Cursor rules for SpacetimeDB and a `spacetimedb` directory which is where your SpacetimeDB module code will go.

## Declare imports

`spacetime init` generated a few files:

1. Open `spacetimedb/StdbModule.csproj` to generate a .sln file for intellisense/validation support.
2. Open `spacetimedb/Lib.cs`, a trivial module.
3. Clear it out, so we can write a new module that's still pretty simple: a bare-bones chat server.

To start, we'll need to add `SpacetimeDB` to our using statements. This will give us access to everything we need to author our SpacetimeDB server module.

To the top of `spacetimedb/Lib.cs`, add some imports we'll be using:

```csharp
using SpacetimeDB;
```

We also need to create our static module class which all of the module code will live in. In `spacetimedb/Lib.cs`, add:

```csharp
public static partial class Module
{
}
```

## Define tables

To get our chat server running, we'll need to store two kinds of data: information about each user, and records of all the messages that have been sent.

For each `User`, we'll store their `Identity`, an optional name they can set to identify themselves to other users, and whether they're online or not. We'll designate the `Identity` as our primary key, which enforces that it must be unique, indexes it for faster lookup, and allows clients to track updates.

In `spacetimedb/Lib.cs`, add the definition of the table `User` to the `Module` class:

```csharp
[Table(Name = "user", Public = true)]
public partial class User
{
    [PrimaryKey]
    public Identity Identity;
    public string? Name;
    public bool Online;
}
```

For each `Message`, we'll store the `Identity` of the user who sent it, the `Timestamp` when it was sent, and the text of the message.

In `spacetimedb/Lib.cs`, add the definition of the table `Message` to the `Module` class:

```csharp
[Table(Name = "message", Public = true)]
public partial class Message
{
    public Identity Sender;
    public Timestamp Sent;
    public string Text = "";
}
```

## Set users' names

We want to allow users to set their names, because `Identity` is not a terribly user-friendly identifier. To that effect, we define a reducer `SetName` which clients can invoke to set their `User.Name`. It will validate the caller's chosen name, using a function `ValidateName` which we'll define next, then look up the `User` record for the caller and update it to store the validated name. If the name fails the validation, the reducer will fail.

Each reducer must accept as its first argument a `ReducerContext`, which includes contextual data such as the `Sender` which contains the Identity of the client that called the reducer, and the `Timestamp` when it was invoked. For now, we only need the `Sender`.

It's also possible to call `SetName` via the SpacetimeDB CLI's `spacetime call` command without a connection, in which case no `User` record will exist for the caller. We'll return an error in this case, but you could alter the reducer to insert a `User` row for the module owner. You'll have to decide whether the module owner is always online or always offline, though.

In `spacetimedb/Lib.cs`, add to the `Module` class:

```csharp
[Reducer]
public static void SetName(ReducerContext ctx, string name)
{
    name = ValidateName(name);

    if (ctx.Db.user.Identity.Find(ctx.Sender) is User user)
    {
        user.Name = name;
        ctx.Db.user.Identity.Update(user);
    }
}
```

For now, we'll just do a bare minimum of validation, rejecting the empty name. You could extend this in various ways, like:

- Comparing against a blacklist for moderation purposes.
- Unicode-normalizing names.
- Rejecting names that contain non-printable characters, or removing characters or replacing them with a placeholder.
- Rejecting or truncating long names.
- Rejecting duplicate names.

In `spacetimedb/Lib.cs`, add to the `Module` class:

```csharp
/// Takes a name and checks if it's acceptable as a user's name.
private static string ValidateName(string name)
{
    if (string.IsNullOrEmpty(name))
    {
        throw new Exception("Names must not be empty");
    }
    return name;
}
```

## Send messages

We define a reducer `SendMessage`, which clients will call to send messages. It will validate the message's text, then insert a new `Message` record using `Message.Insert`, with the `Sender` identity and `Time` timestamp taken from the `ReducerContext`.

In `spacetimedb/Lib.cs`, add to the `Module` class:

```csharp
[Reducer]
public static void SendMessage(ReducerContext ctx, string text)
{
    text = ValidateMessage(text);
    Log.Info(text);
    ctx.Db.message.Insert(
        new Message
        {
            Sender = ctx.Sender,
            Text = text,
            Sent = ctx.Timestamp,
        }
    );
}
```

We'll want to validate messages' texts in much the same way we validate users' chosen names. As above, we'll do the bare minimum, rejecting only empty messages.

In `spacetimedb/Lib.cs`, add to the `Module` class:

```csharp
/// Takes a message's text and checks if it's acceptable to send.
private static string ValidateMessage(string text)
{
    if (string.IsNullOrEmpty(text))
    {
        throw new ArgumentException("Messages must not be empty");
    }
    return text;
}
```

You could extend the validation in `ValidateMessage` in similar ways to `ValidateName`, or add additional checks to `SendMessage`, like:

- Rejecting messages from senders who haven't set their names.
- Rate-limiting users so they can't send new messages too quickly.

## Set users' online status

In C# modules, you can register for `Connect` and `Disconnect` events by using a special `ReducerKind`. We'll use the `Connect` event to create a `User` record for the client if it doesn't yet exist, and to set its online status.

We'll use `reducerContext.Db.User.Identity.Find` to look up a `User` row for `ctx.Sender`, if one exists. If we find one, we'll use `reducerContext.Db.User.Identity.Update` to overwrite it with a row that has `Online: true`. If not, we'll use `User.Insert` to insert a new row for our new user. All three of these methods are generated by the `[SpacetimeDB.Table]` attribute, with rows and behavior based on the row attributes. `User.Identity.Find` returns a nullable `User`, because the unique constraint from the `[PrimaryKey]` attribute means there will be either zero or one matching rows. `Insert` will throw an exception if the insert violates this constraint; if we want to overwrite a `User` row, we need to do so explicitly using `User.Identity.Update`.

In `spacetimedb/Lib.cs`, add the definition of the connect reducer to the `Module` class:

```csharp
[Reducer(ReducerKind.ClientConnected)]
public static void ClientConnected(ReducerContext ctx)
{
    Log.Info($"Connect {ctx.Sender}");

    if (ctx.Db.user.Identity.Find(ctx.Sender) is User user)
    {
        // If this is a returning user, i.e., we already have a `User` with this `Identity`,
        // set `Online: true`, but leave `Name` and `Identity` unchanged.
        user.Online = true;
        ctx.Db.user.Identity.Update(user);
    }
    else
    {
        // If this is a new user, create a `User` object for the `Identity`,
        // which is online, but hasn't set a name.
        ctx.Db.user.Insert(
            new User
            {
                Name = null,
                Identity = ctx.Sender,
                Online = true,
            }
        );
    }
}
```

Similarly, whenever a client disconnects, the database will execute the `OnDisconnect` event if it's registered with `ReducerKind.ClientDisconnected`. We'll use it to un-set the `Online` status of the `User` for the disconnected client.

Add the following code after the `OnConnect` handler:

```csharp
[Reducer(ReducerKind.ClientDisconnected)]
public static void ClientDisconnected(ReducerContext ctx)
{
    if (ctx.Db.user.Identity.Find(ctx.Sender) is User user)
    {
        // This user should exist, so set `Online: false`.
        user.Online = false;
        ctx.Db.user.Identity.Update(user);
    }
    else
    {
        // User does not exist, log warning
        Log.Warn("Warning: No user found for disconnected client.");
    }
}
```

## Start the Server

If you haven't already started the SpacetimeDB server, run the `spacetime start` command in a _separate_ terminal and leave it running while you continue following along.

## Publish the module

And that's all of our module code! We'll run `spacetime publish` to compile our module and publish it on SpacetimeDB. `spacetime publish` takes an optional name which will map to the database's unique address. Clients can connect either by name or by address, but names are much more pleasant. In this example, we'll be using `quickstart-chat`. Feel free to come up with a unique name, and in the CLI commands, replace where we've written `quickstart-chat` with the name you chose.

From the `quickstart-chat` directory, run:

```bash
spacetime publish --server local --project-path spacetimedb quickstart-chat
```

Note: If the WebAssembly optimizer `wasm-opt` is installed, `spacetime publish` will automatically optimize the Web Assembly output of the published module. Instruction for installing the `wasm-opt` binary can be found in [Rust's wasm-opt documentation](https://docs.rs/wasm-opt/latest/wasm_opt/).

## Call Reducers

You can use the CLI (command line interface) to run reducers. The arguments to the reducer are passed in JSON format.

```bash
spacetime call --server local quickstart-chat SendMessage "Hello, World!"
```

Once we've called our `SendMessage` reducer, we can check to make sure it ran by running the `logs` command.

```bash
spacetime logs --server local quickstart-chat
```

You should now see the output that your module printed in the database.

```bash
info: Hello, World!
```

## SQL Queries

SpacetimeDB supports a subset of the SQL syntax so that you can easily query the data of your database. We can run a query using the `sql` command.

```bash
spacetime sql --server local quickstart-chat "SELECT * FROM message"
```

```bash
 sender                                                             | sent                             | text
--------------------------------------------------------------------+----------------------------------+-----------------
 0x93dda09db9a56d8fa6c024d843e805d8262191db3b4ba84c5efcd1ad451fed4e | 2025-04-08T15:47:46.935402+00:00 | "Hello, world!"
```

## What's next?

You've just set up your first database in SpacetimeDB! You can find the full code for this client [in the C# server module example](https://github.com/clockworklabs/SpacetimeDB/tree/master/sdks/csharp/examples~/quickstart-chat/server).

The next step would be to create a client that interacts with this module. You can use any of SpacetimeDB's supported client languages to do this. Take a look at the quick start guide for your client language of choice: [Rust](/sdks/rust/quickstart), [C#](/sdks/c-sharp/quickstart), or [TypeScript](/sdks/typescript/quickstart).

If you are planning to use SpacetimeDB with the Unity game engine, you can skip right to the [Unity Comprehensive Tutorial](/unity/part-1).
