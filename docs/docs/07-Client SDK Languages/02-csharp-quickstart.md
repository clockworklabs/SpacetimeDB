---
title: C# Quickstart
slug: /sdks/c-sharp/quickstart
---

# C# Client SDK Quick Start

In this guide we'll show you how to get up and running with a simple SpacetimeDB app with a client written in C#.

We'll implement a command-line client for the module created in our [Rust](../../modules/rust/quickstart) or [C# Module](../../modules/c-sharp/quickstart) Quickstart guides. Ensure you followed one of these guides before continuing.

## Project structure

Enter the directory `quickstart-chat` you created in the [Rust Module Quickstart](/modules/rust/quickstart) or [C# Module Quickstart](/modules/c-sharp/quickstart) guides:

```bash
cd quickstart-chat
```

Within it, create a new C# console application project called `client` using either Visual Studio, Rider or the .NET CLI:

```bash
dotnet new console -o client
```

Open the project in your IDE of choice.

## Add the NuGet package for the C# SpacetimeDB SDK

Add the `SpacetimeDB.ClientSDK` [NuGet package](https://www.nuget.org/packages/SpacetimeDB.ClientSDK/) using Visual Studio or Rider _NuGet Package Manager_ or via the .NET CLI:

```bash
dotnet add package SpacetimeDB.ClientSDK
```

## Clear `client/Program.cs`

Clear out any data from `client/Program.cs` so we can write our chat client.

## Generate your module types

The `spacetime` CLI's `generate` command will generate client-side interfaces for the tables, reducers and types defined in your server module.

In your `quickstart-chat` directory, run:

```bash
mkdir -p client/module_bindings
spacetime generate --lang csharp --out-dir client/module_bindings --project-path server
```

Take a look inside `client/module_bindings`. The CLI should have generated three folders and nine files:

```
module_bindings
├── Reducers
│   ├── ClientConnected.g.cs
│   ├── ClientDisconnected.g.cs
│   ├── SendMessage.g.cs
│   └── SetName.g.cs
├── Tables
│   ├── Message.g.cs
│   └── User.g.cs
├── Types
│   ├── Message.g.cs
│   └── User.g.cs
└── SpacetimeDBClient.g.cs
```

## Add imports to Program.cs

Open `client/Program.cs` and add the following imports:

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;
using System.Collections.Concurrent;
```

We will also need to create some global variables. We'll cover the `Identity` later in the `Save credentials` section. Later we'll also be setting up a second thread for handling user input. In the `Process thread` section we'll use this in the `ConcurrentQueue` to store the commands for that thread.

To `Program.cs`, add:

```csharp
// our local client SpacetimeDB identity
Identity? local_identity = null;

// declare a thread safe queue to store commands
var input_queue = new ConcurrentQueue<(string Command, string Args)>();
```

## Define Main function

We'll work outside-in, first defining our `Main` function at a high level, then implementing each behavior it needs. We need `Main` to do several things:

1. Initialize the `AuthToken` module, which loads and stores our authentication token to/from local storage.
2. Connect to the database.
3. Register a number of callbacks to run in response to various database events.
4. Start our processing thread which connects to the SpacetimeDB database, updates the SpacetimeDB client and processes commands that come in from the input loop running in the main thread.
5. Start the input loop, which reads commands from standard input and sends them to the processing thread.
6. When the input loop exits, stop the processing thread and wait for it to exit.

To `Program.cs`, add:

```csharp
void Main()
{
    // Initialize the `AuthToken` module
    AuthToken.Init(".spacetime_csharp_quickstart");
    // Builds and connects to the database
    DbConnection? conn = null;
    conn = ConnectToDB();
    // Registers to run in response to database events.
    RegisterCallbacks(conn);
    // Declare a threadsafe cancel token to cancel the process loop
    var cancellationTokenSource = new CancellationTokenSource();
    // Spawn a thread to call process updates and process commands
    var thread = new Thread(() => ProcessThread(conn, cancellationTokenSource.Token));
    thread.Start();
    // Handles CLI input
    InputLoop();
    // This signals the ProcessThread to stop
    cancellationTokenSource.Cancel();
    thread.Join();
}
```

## Connect to database

Before we connect, we'll store the SpacetimeDB hostname and our database name in constants `HOST` and `DB_NAME`.

A connection to a SpacetimeDB database is represented by a `DbConnection`. We configure `DbConnection`s using the builder pattern, by calling `DbConnection.Builder()`, chaining method calls to set various connection parameters and register callbacks, then we cap it off with a call to `.Build()` to begin the connection.

In our case, we'll supply the following options:

1. A `WithUri` call, to specify the URI of the SpacetimeDB host where our database is running.
2. A `WithModuleName` call, to specify the name or `Identity` of our database. Make sure to pass the same name here as you supplied to `spacetime publish`.
3. A `WithToken` call, to supply a token to authenticate with.
4. An `OnConnect` callback, to run when the remote database acknowledges and accepts our connection.
5. An `OnConnectError` callback, to run if the remote database is unreachable or it rejects our connection.
6. An `OnDisconnect` callback, to run when our connection ends.

To `Program.cs`, add:

```csharp
/// The URI of the SpacetimeDB instance hosting our chat database and module.
const string HOST = "http://localhost:3000";

/// The database name we chose when we published our module.
const string DB_NAME = "quickstart-chat";

/// Load credentials from a file and connect to the database.
DbConnection ConnectToDB()
{
    DbConnection? conn = null;
    conn = DbConnection.Builder()
        .WithUri(HOST)
        .WithModuleName(DB_NAME)
        .WithToken(AuthToken.Token)
        .OnConnect(OnConnected)
        .OnConnectError(OnConnectError)
        .OnDisconnect(OnDisconnected)
        .Build();
    return conn;
}
```

### Save credentials

SpacetimeDB will accept any [OpenID Connect](https://openid.net/developers/how-connect-works/) compliant [JSON Web Token](https://jwt.io/) and use it to compute an `Identity` for the user. More complex applications will generally authenticate their user somehow, generate or retrieve a token, and attach it to their connection via `WithToken`. In our case, though, we'll connect anonymously the first time, let SpacetimeDB generate a fresh `Identity` and corresponding JWT for us, and save that token locally to re-use the next time we connect.

Once we are connected, we'll use the `AuthToken` module to save our token to local storage, so that we can re-authenticate as the same user the next time we connect. We'll also store the identity in a global variable `local_identity` so that we can use it to check if we are the sender of a message or name change. This callback also notifies us of our client's `Address`, an opaque identifier SpacetimeDB modules can use to distinguish connections by the same `Identity`, but we won't use it in our app.

To `Program.cs`, add:

```csharp
/// Our `OnConnected` callback: save our credentials to a file.
void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    local_identity = identity;
    AuthToken.SaveToken(authToken);
}
```

### Connect Error callback

Should we get an error during connection, we'll be given an `Exception` which contains the details about the exception. To keep things simple, we'll just write the exception to the console.

To `Program.cs`, add:

```csharp
/// Our `OnConnectError` callback: print the error, then exit the process.
void OnConnectError(Exception e)
{
    Console.Write($"Error while connecting: {e}");
}
```

### Disconnect callback

When disconnecting, the callback contains the connection details and if an error occurs, it will also contain an `Exception`. If we get an error, we'll write the error to the console, if not, we'll just write that we disconnected.

To `Program.cs`, add:

```csharp
/// Our `OnDisconnect` callback: print a note, then exit the process.
void OnDisconnected(DbConnection conn, Exception? e)
{
    if (e != null)
    {
        Console.Write($"Disconnected abnormally: {e}");
    }
    else
    {
        Console.Write($"Disconnected normally.");
    }
}
```

## Register callbacks

Now we need to handle several sorts of events with Tables and Reducers:

1. `User.OnInsert`: When a new user joins, we'll print a message introducing them.
2. `User.OnUpdate`: When a user is updated, we'll print their new name, or declare their new online status.
3. `Message.OnInsert`: When we receive a new message, we'll print it.
4. `Reducer.OnSetName`: If the server rejects our attempt to set our name, we'll print an error.
5. `Reducer.OnSendMessage`: If the server rejects a message we send, we'll print an error.

To `Program.cs`, add:

```csharp
/// Register all the callbacks our app will use to respond to database events.
void RegisterCallbacks(DbConnection conn)
{
    conn.Db.User.OnInsert += User_OnInsert;
    conn.Db.User.OnUpdate += User_OnUpdate;

    conn.Db.Message.OnInsert += Message_OnInsert;

    conn.Reducers.OnSetName += Reducer_OnSetNameEvent;
    conn.Reducers.OnSendMessage += Reducer_OnSendMessageEvent;
}
```

### Notify about new users

For each table, we can register on-insert and on-delete callbacks to be run whenever a subscribed row is inserted or deleted. We register these callbacks using the `OnInsert` and `OnDelete` methods, which are automatically generated for each table by `spacetime generate`.

These callbacks can fire in two contexts:

- After a reducer runs, when the client's cache is updated about changes to subscribed rows.
- After calling `subscribe`, when the client's cache is initialized with all existing matching rows.

This second case means that, even though the module only ever inserts online users, the client's `User.OnInsert` callbacks may be invoked with users who are offline. We'll only notify about online users.

`OnInsert` and `OnDelete` callbacks take two arguments: an `EventContext` and the altered row. The `EventContext.Event` is an enum which describes the event that caused the row to be inserted or deleted. All SpacetimeDB callbacks accept a context argument, which you can use in place of your top-level `DbConnection`.

Whenever we want to print a user, if they have set a name, we'll use that. If they haven't set a name, we'll instead print the first 8 bytes of their identity, encoded as hexadecimal. We'll define a function `UserNameOrIdentity` to handle this.

To `Program.cs`, add:

```csharp
/// If the user has no set name, use the first 8 characters from their identity.
string UserNameOrIdentity(User user) => user.Name ?? user.Identity.ToString()[..8];

/// Our `User.OnInsert` callback: if the user is online, print a notification.
void User_OnInsert(EventContext ctx, User insertedValue)
{
    if (insertedValue.Online)
    {
        Console.WriteLine($"{UserNameOrIdentity(insertedValue)} is online");
    }
}
```

### Notify about updated users

Because we declared a primary key column in our `User` table, we can also register on-update callbacks. These run whenever a row is replaced by a row with the same primary key, like our module's `User.Identity.Update` calls. We register these callbacks using the `OnUpdate` method, which is automatically implemented by `spacetime generate` for any table with a primary key column.

`OnUpdate` callbacks take three arguments: the old row, the new row, and a `EventContext`.

In our module, users can be updated for three reasons:

1. They've set their name using the `SetName` reducer.
2. They're an existing user re-connecting, so their `Online` has been set to `true`.
3. They've disconnected, so their `Online` has been set to `false`.

We'll print an appropriate message in each of these cases.

To `Program.cs`, add:

```csharp
/// Our `User.OnUpdate` callback:
/// print a notification about name and status changes.
void User_OnUpdate(EventContext ctx, User oldValue, User newValue)
{
    if (oldValue.Name != newValue.Name)
    {
        Console.WriteLine($"{UserNameOrIdentity(oldValue)} renamed to {newValue.Name}");
    }
    if (oldValue.Online != newValue.Online)
    {
        if (newValue.Online)
        {
            Console.WriteLine($"{UserNameOrIdentity(newValue)} connected.");
        }
        else
        {
            Console.WriteLine($"{UserNameOrIdentity(newValue)} disconnected.");
        }
    }
}
```

### Print messages

When we receive a new message, we'll print it to standard output, along with the name of the user who sent it. Keep in mind that we only want to do this for new messages, i.e. those inserted by a `SendMessage` reducer invocation. We have to handle the backlog we receive when our subscription is initialized separately, to ensure they're printed in the correct order. To that effect, our `OnInsert` callback will check if its `ReducerEvent` argument is not `null`, and only print in that case.

To find the `User` based on the message's `Sender` identity, we'll use `User.Identity.Find`, which behaves like the same function on the server.

We'll print the user's name or identity in the same way as we did when notifying about `User` table events, but here we have to handle the case where we don't find a matching `User` row. This can happen when the module owner sends a message using the CLI's `spacetime call`. In this case, we'll print `unknown`.

To `Program.cs`, add:

```csharp
/// Our `Message.OnInsert` callback: print new messages.
void Message_OnInsert(EventContext ctx, Message insertedValue)
{
    // We are filtering out messages inserted during the subscription being applied,
    // since we will be printing those in the OnSubscriptionApplied callback,
    // where we will be able to first sort the messages before printing.
    if (ctx.Event is not Event<Reducer>.SubscribeApplied)
    {
        PrintMessage(ctx.Db, insertedValue);
    }
}

void PrintMessage(RemoteTables tables, Message message)
{
    var sender = tables.User.Identity.Find(message.Sender);
    var senderName = "unknown";
    if (sender != null)
    {
        senderName = UserNameOrIdentity(sender);
    }

    Console.WriteLine($"{senderName}: {message.Text}");
}
```

### Warn if our name was rejected

We can also register callbacks to run each time a reducer is invoked. We register these callbacks using the `OnReducerEvent` method of the `Reducer` namespace, which is automatically implemented for each reducer by `spacetime generate`.

Each reducer callback takes one fixed argument:

The `ReducerEventContext` of the callback, which contains an `Event` that contains several fields. The ones we care about are:

1. The `CallerIdentity`, the `Identity` of the client that called the reducer.
2. The `Status` of the reducer run, one of `Committed`, `Failed` or `OutOfEnergy`.
3. If we get a `Status.Failed`, an error message is nested inside that we'll want to write to the console.

It also takes a variable amount of additional arguments that match the reducer's arguments.

These callbacks will be invoked in one of two cases:

1. If the reducer was successful and altered any of our subscribed rows.
2. If we requested an invocation which failed.

Note that a status of `Failed` or `OutOfEnergy` implies that the caller identity is our own identity.

We already handle successful `SetName` invocations using our `User.OnUpdate` callback, but if the module rejects a user's chosen name, we'd like that user's client to let them know. We define a function `Reducer_OnSetNameEvent` as a `Reducer.OnSetNameEvent` callback which checks if the reducer failed, and if it did, prints an error message including the rejected name.

We'll test both that our identity matches the sender and that the status is `Failed`, even though the latter implies the former, for demonstration purposes.

To `Program.cs`, add:

```csharp
/// Our `OnSetNameEvent` callback: print a warning if the reducer failed.
void Reducer_OnSetNameEvent(ReducerEventContext ctx, string name)
{
    var e = ctx.Event;
    if (e.CallerIdentity == local_identity && e.Status is Status.Failed(var error))
    {
        Console.Write($"Failed to change name to {name}: {error}");
    }
}
```

### Warn if our message was rejected

We handle warnings on rejected messages the same way as rejected names, though the types and the error message are different.

To `Program.cs`, add:

```csharp
/// Our `OnSendMessageEvent` callback: print a warning if the reducer failed.
void Reducer_OnSendMessageEvent(ReducerEventContext ctx, string text)
{
    var e = ctx.Event;
    if (e.CallerIdentity == local_identity && e.Status is Status.Failed(var error))
    {
        Console.Write($"Failed to send message {text}: {error}");
    }
}
```

## Subscribe to queries

SpacetimeDB is set up so that each client subscribes via SQL queries to some subset of the database, and is notified about changes only to that subset. For complex apps with large databases, judicious subscriptions can save each client significant network bandwidth, memory and computation. For example, in [BitCraft](https://bitcraftonline.com), each player's client subscribes only to the entities in the "chunk" of the world where that player currently resides, rather than the entire game world. Our app is much simpler than BitCraft, so we'll just subscribe to the whole database using `SubscribeToAllTables`.

You can also subscribe to specific tables using SQL syntax, e.g. `SELECT * FROM my_table`. Our [SQL documentation](/sql) enumerates the operations that are accepted in our SQL syntax.

When we specify our subscriptions, we can supply an `OnApplied` callback. This will run when the subscription is applied and the matching rows become available in our client cache. We'll use this opportunity to print the message backlog in proper order.

We can also provide an `OnError` callback. This will run if the subscription fails, usually due to an invalid or malformed SQL queries. We can't handle this case, so we'll just print out the error and exit the process.

In `Program.cs`, update our `OnConnected` function to include `conn.SubscriptionBuilder().OnApplied(OnSubscriptionApplied).SubscribeToAllTables();` so that it reads:

```csharp
/// Our `OnConnect` callback: save our credentials to a file.
void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    local_identity = identity;
    AuthToken.SaveToken(authToken);

    conn.SubscriptionBuilder()
        .OnApplied(OnSubscriptionApplied)
        .SubscribeToAllTables();
}
```

## OnSubscriptionApplied callback

Once our subscription is applied, we'll print all the previously sent messages. We'll define a function `PrintMessagesInOrder` to do this. `PrintMessagesInOrder` calls the automatically generated `Iter` function on our `Message` table, which returns an iterator over all rows in the table. We'll use the `OrderBy` method on the iterator to sort the messages by their `Sent` timestamp.

To `Program.cs`, add:

```csharp
/// Our `OnSubscriptionApplied` callback:
/// sort all past messages and print them in timestamp order.
void OnSubscriptionApplied(SubscriptionEventContext ctx)
{
    Console.WriteLine("Connected");
    PrintMessagesInOrder(ctx.Db);
}

void PrintMessagesInOrder(RemoteTables tables)
{
    foreach (Message message in tables.Message.Iter().OrderBy(item => item.Sent))
    {
        PrintMessage(tables, message);
    }
}
```

## Process thread

Since the input loop will be blocking, we'll run our processing code in a separate thread.

This thread will loop until the thread is signaled to exit, calling the update function `FrameTick` on the `DbConnection` to process any updates received from the database, and `ProcessCommand` to process any commands received from the input loop.

Afterward, close the connection to the database.

To `Program.cs`, add:

```csharp
/// Our separate thread from main, where we can call process updates and process commands without blocking the main thread.
void ProcessThread(DbConnection conn, CancellationToken ct)
{
    try
    {
        // loop until cancellation token
        while (!ct.IsCancellationRequested)
        {
            conn.FrameTick();

            ProcessCommands(conn.Reducers);

            Thread.Sleep(100);
        }
    }
    finally
    {
        conn.Disconnect();
    }
}
```

## Handle user input

The input loop will read commands from standard input and send them to the processing thread using the input queue. The `ProcessCommands` function is called every 100ms by the processing thread to process any pending commands.

Supported Commands:

1. Send a message: `message`, send the message to the database by calling `Reducer.SendMessage` which is automatically generated by `spacetime generate`.

2. Set name: `name`, will send the new name to the database by calling `Reducer.SetName` which is automatically generated by `spacetime generate`.

To `Program.cs`, add:

```csharp
/// Read each line of standard input, and either set our name or send a message as appropriate.
void InputLoop()
{
    while (true)
    {
        var input = Console.ReadLine();
        if (input == null)
        {
            break;
        }

        if (input.StartsWith("/name "))
        {
            input_queue.Enqueue(("name", input[6..]));
            continue;
        }
        else
        {
            input_queue.Enqueue(("message", input));
        }
    }
}

void ProcessCommands(RemoteReducers reducers)
{
    // process input queue commands
    while (input_queue.TryDequeue(out var command))
    {
        switch (command.Command)
        {
            case "message":
                reducers.SendMessage(command.Args);
                break;
            case "name":
                reducers.SetName(command.Args);
                break;
        }
    }
}
```

## Run the client

Finally, we just need to add a call to `Main`.

To `Program.cs`, add:

```csharp
Main();
```

Now, we can run the client by hitting start in Visual Studio or Rider; or by running the following command in the `client` directory:

```bash
dotnet run --project client
```

## What's next?

Congratulations! You've built a simple chat app using SpacetimeDB.

You can find the full code for this client [in the C# client SDK's examples](https://github.com/clockworklabs/SpacetimeDB/tree/master/sdks/csharp/examples~/quickstart-chat/client).

Check out the [C# client SDK Reference](/sdks/c-sharp) for a more comprehensive view of the SpacetimeDB C# client SDK.

If you are interested in developing in the Unity game engine, check out our [Unity Comprehensive Tutorial](/unity) and [Blackholio](https://github.com/clockworklabs/SpacetimeDB/tree/master/demo/Blackholio) game example.
