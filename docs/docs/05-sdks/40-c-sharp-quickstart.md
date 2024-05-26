---
title: C# Client SDK Quick Start
navTitle: C# Quickstart
---

In this guide we'll show you how to get up and running with a simple SpacetimeDB app with a client written in C#.

We'll implement a command-line client for the module created in our [Rust](../../modules/rust/quickstart) or [C# Module](../../modules/c-sharp/quickstart) Quickstart guides. Ensure you followed one of these guides before continuing.

## Project structure

Enter the directory `quickstart-chat` you created in the [Rust Module Quickstart](/docs/modules/rust/quickstart) or [C# Module Quickstart](/docs/modules/c-sharp/quickstart) guides:

```bash
cd quickstart-chat
```

Within it, create a new C# console application project called `client` using either Visual Studio, Rider or the .NET CLI:

```bash
dotnet new console -o client
```

Open the project in your IDE of choice.

## Add the NuGet package for the C# SpacetimeDB SDK

Add the `SpacetimeDB.ClientSDK` [NuGet package](https://www.nuget.org/packages/spacetimedbsdk) using Visual Studio or Rider _NuGet Package Manager_ or via the .NET CLI:

```bash
dotnet add package SpacetimeDB.ClientSDK
```

## Generate your module types

The `spacetime` CLI's `generate` command will generate client-side interfaces for the tables, reducers and types defined in your server module.

In your `quickstart-chat` directory, run:

```bash
mkdir -p client/module_bindings
spacetime generate --lang csharp --out-dir client/module_bindings --project-path server
```

Take a look inside `client/module_bindings`. The CLI should have generated five files:

```
module_bindings
├── Message.cs
├── ReducerEvent.cs
├── SendMessageReducer.cs
├── SetNameReducer.cs
└── User.cs
```

## Add imports to Program.cs

Open `client/Program.cs` and add the following imports:

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;
using System.Collections.Concurrent;
```

We will also need to create some global variables that will be explained when we use them later. Add the following to the top of `Program.cs`:

```csharp
// our local client SpacetimeDB identity
Identity? local_identity = null;

// declare a thread safe queue to store commands in format (command, args)
ConcurrentQueue<(string,string)> input_queue = new ConcurrentQueue<(string, string)>();

// declare a threadsafe cancel token to cancel the process loop
CancellationTokenSource cancel_token = new CancellationTokenSource();
```

## Define Main function

We'll work outside-in, first defining our `Main` function at a high level, then implementing each behavior it needs. We need `Main` to do several things:

1. Initialize the `AuthToken` module, which loads and stores our authentication token to/from local storage.
2. Create the `SpacetimeDBClient` instance.
3. Register callbacks on any events we want to handle. These will print to standard output messages received from the database and updates about users' names and online statuses.
4. Start our processing thread which connects to the SpacetimeDB module, updates the SpacetimeDB client and processes commands that come in from the input loop running in the main thread.
5. Start the input loop, which reads commands from standard input and sends them to the processing thread.
6. When the input loop exits, stop the processing thread and wait for it to exit.

```csharp
void Main()
{
    AuthToken.Init(".spacetime_csharp_quickstart");

    // create the client, pass in a logger to see debug messages
    SpacetimeDBClient.CreateInstance(new ConsoleLogger());

    RegisterCallbacks();

    // spawn a thread to call process updates and process commands
    var thread = new Thread(ProcessThread);
    thread.Start();

    InputLoop();

    // this signals the ProcessThread to stop
    cancel_token.Cancel();
    thread.Join();
}
```

## Register callbacks

We need to handle several sorts of events:

1. `onConnect`: When we connect, we will call `Subscribe` to tell the module what tables we care about.
2. `onIdentityReceived`: When we receive our credentials, we'll use the `AuthToken` module to save our token so that the next time we connect, we can re-authenticate as the same user.
3. `onSubscriptionApplied`: When we get the onSubscriptionApplied callback, that means our local client cache has been fully populated. At this time we'll print the user menu.
4. `User.OnInsert`: When a new user joins, we'll print a message introducing them.
5. `User.OnUpdate`: When a user is updated, we'll print their new name, or declare their new online status.
6. `Message.OnInsert`: When we receive a new message, we'll print it.
7. `Reducer.OnSetNameEvent`: If the server rejects our attempt to set our name, we'll print an error.
8. `Reducer.OnSendMessageEvent`: If the server rejects a message we send, we'll print an error.

```csharp
void RegisterCallbacks()
{
    SpacetimeDBClient.instance.onConnect += OnConnect;
    SpacetimeDBClient.instance.onIdentityReceived += OnIdentityReceived;
    SpacetimeDBClient.instance.onSubscriptionApplied += OnSubscriptionApplied;

    User.OnInsert += User_OnInsert;
    User.OnUpdate += User_OnUpdate;

    Message.OnInsert += Message_OnInsert;

    Reducer.OnSetNameEvent += Reducer_OnSetNameEvent;
    Reducer.OnSendMessageEvent += Reducer_OnSendMessageEvent;
}
```

### Notify about new users

For each table, we can register on-insert and on-delete callbacks to be run whenever a subscribed row is inserted or deleted. We register these callbacks using the `OnInsert` and `OnDelete` methods, which are automatically generated for each table by `spacetime generate`.

These callbacks can fire in two contexts:

-   After a reducer runs, when the client's cache is updated about changes to subscribed rows.
-   After calling `subscribe`, when the client's cache is initialized with all existing matching rows.

This second case means that, even though the module only ever inserts online users, the client's `User.OnInsert` callbacks may be invoked with users who are offline. We'll only notify about online users.

`OnInsert` and `OnDelete` callbacks take two arguments: the altered row, and a `ReducerEvent`. This will be `null` for rows inserted when initializing the cache for a subscription. `ReducerEvent` is an enum autogenerated by `spacetime generate` with a variant for each reducer defined by the module. For now, we can ignore this argument.

Whenever we want to print a user, if they have set a name, we'll use that. If they haven't set a name, we'll instead print the first 8 bytes of their identity, encoded as hexadecimal. We'll define a function `UserNameOrIdentity` to handle this.

```csharp
string UserNameOrIdentity(User user) => user.Name ?? user.Identity.ToString()!.Substring(0, 8);

void User_OnInsert(User insertedValue, ReducerEvent? dbEvent)
{
    if (insertedValue.Online)
    {
        Console.WriteLine($"{UserNameOrIdentity(insertedValue)} is online");
    }
}
```

### Notify about updated users

Because we declared a primary key column in our `User` table, we can also register on-update callbacks. These run whenever a row is replaced by a row with the same primary key, like our module's `User::update_by_identity` calls. We register these callbacks using the `OnUpdate` method, which is automatically implemented by `spacetime generate` for any table with a primary key column.

`OnUpdate` callbacks take three arguments: the old row, the new row, and a `ReducerEvent`.

In our module, users can be updated for three reasons:

1. They've set their name using the `SetName` reducer.
2. They're an existing user re-connecting, so their `Online` has been set to `true`.
3. They've disconnected, so their `Online` has been set to `false`.

We'll print an appropriate message in each of these cases.

```csharp
void User_OnUpdate(User oldValue, User newValue, ReducerEvent dbEvent)
{
    if (oldValue.Name != newValue.Name)
    {
        Console.WriteLine($"{UserNameOrIdentity(oldValue)} renamed to {newValue.Name}");
    }

    if (oldValue.Online == newValue.Online)
        return;

    if (newValue.Online)
    {
        Console.WriteLine($"{UserNameOrIdentity(newValue)} connected.");
    }
    else
    {
        Console.WriteLine($"{UserNameOrIdentity(newValue)} disconnected.");
    }
}
```

### Print messages

When we receive a new message, we'll print it to standard output, along with the name of the user who sent it. Keep in mind that we only want to do this for new messages, i.e. those inserted by a `SendMessage` reducer invocation. We have to handle the backlog we receive when our subscription is initialized separately, to ensure they're printed in the correct order. To that effect, our `OnInsert` callback will check if its `ReducerEvent` argument is not `null`, and only print in that case.

To find the `User` based on the message's `Sender` identity, we'll use `User::FilterByIdentity`, which behaves like the same function on the server. The key difference is that, unlike on the module side, the client's `FilterByIdentity` accepts a `byte[]`, rather than an `Identity`. The `Sender` identity stored in the message is also a `byte[]`, not an `Identity`, so we can just pass it to the filter method.

We'll print the user's name or identity in the same way as we did when notifying about `User` table events, but here we have to handle the case where we don't find a matching `User` row. This can happen when the module owner sends a message using the CLI's `spacetime call`. In this case, we'll print `unknown`.

```csharp
void PrintMessage(Message message)
{
    var sender = User.FilterByIdentity(message.Sender);
    var senderName = "unknown";
    if (sender != null)
    {
        senderName = UserNameOrIdentity(sender);
    }

    Console.WriteLine($"{senderName}: {message.Text}");
}

void Message_OnInsert(Message insertedValue, ReducerEvent? dbEvent)
{
    if (dbEvent != null)
    {
        PrintMessage(insertedValue);
    }
}
```

### Warn if our name was rejected

We can also register callbacks to run each time a reducer is invoked. We register these callbacks using the `OnReducerEvent` method of the `Reducer` namespace, which is automatically implemented for each reducer by `spacetime generate`.

Each reducer callback takes one fixed argument:

The ReducerEvent that triggered the callback. It contains several fields. The ones we care about are:

1. The `Identity` of the client that called the reducer.
2. The `Status` of the reducer run, one of `Committed`, `Failed` or `OutOfEnergy`.
3. The error message, if any, that the reducer returned.

It also takes a variable amount of additional arguments that match the reducer's arguments.

These callbacks will be invoked in one of two cases:

1. If the reducer was successful and altered any of our subscribed rows.
2. If we requested an invocation which failed.

Note that a status of `Failed` or `OutOfEnergy` implies that the caller identity is our own identity.

We already handle successful `SetName` invocations using our `User.OnUpdate` callback, but if the module rejects a user's chosen name, we'd like that user's client to let them know. We define a function `Reducer_OnSetNameEvent` as a `Reducer.OnSetNameEvent` callback which checks if the reducer failed, and if it did, prints an error message including the rejected name.

We'll test both that our identity matches the sender and that the status is `Failed`, even though the latter implies the former, for demonstration purposes.

```csharp
void Reducer_OnSetNameEvent(ReducerEvent reducerEvent, string name)
{
    bool localIdentityFailedToChangeName =
        reducerEvent.Identity == local_identity &&
        reducerEvent.Status == ClientApi.Event.Types.Status.Failed;

    if (localIdentityFailedToChangeName)
    {
        Console.Write($"Failed to change name to {name}");
    }
}
```

### Warn if our message was rejected

We handle warnings on rejected messages the same way as rejected names, though the types and the error message are different.

```csharp
void Reducer_OnSendMessageEvent(ReducerEvent reducerEvent, string text)
{
    bool localIdentityFailedToSendMessage =
        reducerEvent.Identity == local_identity &&
        reducerEvent.Status == ClientApi.Event.Types.Status.Failed;

    if (localIdentityFailedToSendMessage)
    {
        Console.Write($"Failed to send message {text}");
    }
}
```

## Connect callback

Once we are connected, we can send our subscription to the SpacetimeDB module. SpacetimeDB is set up so that each client subscribes via SQL queries to some subset of the database, and is notified about changes only to that subset. For complex apps with large databases, judicious subscriptions can save each client significant network bandwidth, memory and computation compared. For example, in [BitCraft](https://bitcraftonline.com), each player's client subscribes only to the entities in the "chunk" of the world where that player currently resides, rather than the entire game world. Our app is much simpler than BitCraft, so we'll just subscribe to the whole database.

```csharp
void OnConnect()
{
    SpacetimeDBClient.instance.Subscribe(new List<string>
    {
        "SELECT * FROM User", "SELECT * FROM Message"
    });
}
```

## OnIdentityReceived callback

This callback is executed when we receive our credentials from the SpacetimeDB module. We'll use the `AuthToken` module to save our token to local storage, so that we can re-authenticate as the same user the next time we connect. We'll also store the identity in a global variable `local_identity` so that we can use it to check if we are the sender of a message or name change. This callback also notifies us of our client's `Address`, an opaque identifier SpacetimeDB modules can use to distinguish connections by the same `Identity`, but we won't use it in our app.

```csharp
void OnIdentityReceived(string authToken, Identity identity, Address _address)
{
    local_identity = identity;
    AuthToken.SaveToken(authToken);
}
```

## OnSubscriptionApplied callback

Once our subscription is applied, we'll print all the previously sent messages. We'll define a function `PrintMessagesInOrder` to do this. `PrintMessagesInOrder` calls the automatically generated `Iter` function on our `Message` table, which returns an iterator over all rows in the table. We'll use the `OrderBy` method on the iterator to sort the messages by their `Sent` timestamp.

```csharp
void PrintMessagesInOrder()
{
    foreach (Message message in Message.Iter().OrderBy(item => item.Sent))
    {
        PrintMessage(message);
    }
}

void OnSubscriptionApplied()
{
    Console.WriteLine("Connected");
    PrintMessagesInOrder();
}
```

<!-- FIXME: isn't OnSubscriptionApplied invoked every time the subscription results change? -->

## Process thread

Since the input loop will be blocking, we'll run our processing code in a separate thread. This thread will:

1. Connect to the module. We'll store the SpacetimeDB host name and our module name in constants `HOST` and `DB_NAME`. We will also store if SSL is enabled in a constant called `SSL_ENABLED`. This only needs to be `true` if we are using `SpacetimeDB Cloud`. Replace `<module-name>` with the name you chose when publishing your module during the module quickstart.

`Connect` takes an auth token, which is `null` for a new connection, or a stored string for a returning user. We are going to use the optional AuthToken module which uses local storage to store the auth token. If you want to use your own way to associate an auth token with a user, you can pass in your own auth token here.

2. Loop until the thread is signaled to exit, calling `Update` on the SpacetimeDBClient to process any updates received from the module, and `ProcessCommand` to process any commands received from the input loop.

3. Finally, Close the connection to the module.

```csharp
const string HOST = "http://localhost:3000";
const string DBNAME = "module";

void ProcessThread()
{
    SpacetimeDBClient.instance.Connect(AuthToken.Token, HOST, DBNAME);

    // loop until cancellation token
    while (!cancel_token.IsCancellationRequested)
    {
        SpacetimeDBClient.instance.Update();

        ProcessCommands();

        Thread.Sleep(100);
    }

    SpacetimeDBClient.instance.Close();
}
```

## Input loop and ProcessCommands

The input loop will read commands from standard input and send them to the processing thread using the input queue. The `ProcessCommands` function is called every 100ms by the processing thread to process any pending commands.

Supported Commands:

1. Send a message: `message`, send the message to the module by calling `Reducer.SendMessage` which is automatically generated by `spacetime generate`.

2. Set name: `name`, will send the new name to the module by calling `Reducer.SetName` which is automatically generated by `spacetime generate`.

```csharp
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
            input_queue.Enqueue(("name", input.Substring(6)));
            continue;
        }
        else
        {
            input_queue.Enqueue(("message", input));
        }
    }
}

void ProcessCommands()
{
    // process input queue commands
    while (input_queue.TryDequeue(out var command))
    {
        switch (command.Item1)
        {
            case "message":
                Reducer.SendMessage(command.Item2);
                break;
            case "name":
                Reducer.SetName(command.Item2);
                break;
        }
    }
}
```

## Run the client

Finally we just need to add a call to `Main` in `Program.cs`:

```csharp
Main();
```

Now, we can run the client by hitting start in Visual Studio or Rider; or by running the following command in the `client` directory:

```bash
dotnet run --project client
```

## What's next?

Congratulations! You've built a simple chat app using SpacetimeDB. You can look at the C# SDK Reference for more information about the client SDK. If you are interested in developing in the Unity game engine, check out our Unity3d Comprehensive Tutorial and BitcraftMini game example.
