# C# Module Quickstart

In this tutorial, we'll implement a simple chat server as a SpacetimeDB module.

A SpacetimeDB module is code that gets compiled to WebAssembly and is uploaded to SpacetimeDB. This code becomes server-side logic that interfaces directly with the Spacetime relational database.

Each SpacetimeDB module defines a set of tables and a set of reducers.

Each table is defined as a C# `class` annotated with `[SpacetimeDB.Table]`, where an instance represents a row, and each field represents a column.

A reducer is a function which traverses and updates the database. Each reducer call runs in its own transaction, and its updates to the database are only committed if the reducer returns successfully. In C#, reducers are defined as functions annotated with `[SpacetimeDB.Reducer]`. If an exception is thrown, the reducer call fails, the database is not updated, and a failed message is reported to the client.

## Install SpacetimeDB

If you haven't already, start by [installing SpacetimeDB](/install). This will install the `spacetime` command line interface (CLI), which contains all the functionality for interacting with SpacetimeDB.

## Install .NET

Next we need to [install .NET](https://dotnet.microsoft.com/en-us/download/dotnet) so that we can build and publish our module.

## Project structure

Create and enter a directory `quickstart-chat`:

```bash
mkdir quickstart-chat
cd quickstart-chat
```

Now create `server`, our module, which runs in the database:

```bash
spacetime init --lang csharp server
```

## Declare imports

`spacetime init` should have pre-populated `server/Lib.cs` with a trivial module. Clear it out, so we can write a module that's still pretty simple: a bare-bones chat server.

To the top of `server/Lib.cs`, add some imports we'll be using:

```C#
using System.Runtime.CompilerServices;
using SpacetimeDB.Module;
using static SpacetimeDB.Runtime;
```

- `System.Runtime.CompilerServices` allows us to use the `ModuleInitializer` attribute, which we'll use to register our `OnConnect` and `OnDisconnect` callbacks.
- `SpacetimeDB.Module` contains the special attributes we'll use to define our module.
- `SpacetimeDB.Runtime` contains the raw API bindings SpacetimeDB uses to communicate with the database.

We also need to create our static module class which all of the module code will live in. In `server/Lib.cs`, add:

```csharp
static partial class Module
{
}
```

## Define tables

To get our chat server running, we'll need to store two kinds of data: information about each user, and records of all the messages that have been sent.

For each `User`, we'll store the `Identity` of their client connection, an optional name they can set to identify themselves to other users, and whether they're online or not. We'll designate the `Identity` as our primary key, which enforces that it must be unique, indexes it for faster lookup, and allows clients to track updates.

In `server/Lib.cs`, add the definition of the table `User` to the `Module` class:

```C#
    [SpacetimeDB.Table]
    public partial class User
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public Identity Identity;
        public string? Name;
        public bool Online;
    }
```

For each `Message`, we'll store the `Identity` of the user who sent it, the `Timestamp` when it was sent, and the text of the message.

In `server/Lib.cs`, add the definition of the table `Message` to the `Module` class:

```C#
    [SpacetimeDB.Table]
    public partial class Message
    {
        public Identity Sender;
        public long Sent;
        public string Text = "";
    }
```

## Set users' names

We want to allow users to set their names, because `Identity` is not a terribly user-friendly identifier. To that effect, we define a reducer `SetName` which clients can invoke to set their `User.Name`. It will validate the caller's chosen name, using a function `ValidateName` which we'll define next, then look up the `User` record for the caller and update it to store the validated name. If the name fails the validation, the reducer will fail.

Each reducer may accept as its first argument a `DbEventArgs`, which includes the `Identity` of the client that called the reducer, and the `Timestamp` when it was invoked. For now, we only need the `Identity`, `dbEvent.Sender`.

It's also possible to call `SetName` via the SpacetimeDB CLI's `spacetime call` command without a connection, in which case no `User` record will exist for the caller. We'll return an error in this case, but you could alter the reducer to insert a `User` row for the module owner. You'll have to decide whether the module owner is always online or always offline, though.

In `server/Lib.cs`, add to the `Module` class:

```C#
    [SpacetimeDB.Reducer]
    public static void SetName(DbEventArgs dbEvent, string name)
    {
        name = ValidateName(name);

        var user = User.FindByIdentity(dbEvent.Sender);
        if (user is not null)
        {
            user.Name = name;
            User.UpdateByIdentity(dbEvent.Sender, user);
        }
    }
```

For now, we'll just do a bare minimum of validation, rejecting the empty name. You could extend this in various ways, like:

- Comparing against a blacklist for moderation purposes.
- Unicode-normalizing names.
- Rejecting names that contain non-printable characters, or removing characters or replacing them with a placeholder.
- Rejecting or truncating long names.
- Rejecting duplicate names.

In `server/Lib.cs`, add to the `Module` class:

```C#
    /// Takes a name and checks if it's acceptable as a user's name.
    public static string ValidateName(string name)
    {
        if (string.IsNullOrEmpty(name))
        {
            throw new Exception("Names must not be empty");
        }
        return name;
    }
```

## Send messages

We define a reducer `SendMessage`, which clients will call to send messages. It will validate the message's text, then insert a new `Message` record using `Message.Insert`, with the `Sender` identity and `Time` timestamp taken from the `DbEventArgs`.

In `server/Lib.cs`, add to the `Module` class:

```C#
    [SpacetimeDB.Reducer]
    public static void SendMessage(DbEventArgs dbEvent, string text)
    {
        text = ValidateMessage(text);
        Log(text);
        new Message
        {
            Sender = dbEvent.Sender,
            Text = text,
            Sent = dbEvent.Time.ToUnixTimeMilliseconds(),
        }.Insert();
    }
```

We'll want to validate messages' texts in much the same way we validate users' chosen names. As above, we'll do the bare minimum, rejecting only empty messages.

In `server/Lib.cs`, add to the `Module` class:

```C#
    /// Takes a message's text and checks if it's acceptable to send.
    public static string ValidateMessage(string text)
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

In C# modules, you can register for OnConnect and OnDisconnect events in a special initializer function that uses the attribute `ModuleInitializer`. We'll use the `OnConnect` event to create a `User` record for the client if it doesn't yet exist, and to set its online status.

We'll use `User.FilterByOwnerIdentity` to look up a `User` row for `dbEvent.Sender`, if one exists. If we find one, we'll use `User.UpdateByOwnerIdentity` to overwrite it with a row that has `Online: true`. If not, we'll use `User.Insert` to insert a new row for our new user. All three of these methods are generated by the `[SpacetimeDB.Table]` attribute, with rows and behavior based on the row attributes. `FilterByOwnerIdentity` returns a nullable `User`, because the unique constraint from the `[SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]` attribute means there will be either zero or one matching rows. `Insert` will throw an exception if the insert violates this constraint; if we want to overwrite a `User` row, we need to do so explicitly using `UpdateByOwnerIdentity`.

In `server/Lib.cs`, add the definition of the connect reducer to the `Module` class:

```C#
    [ModuleInitializer]
    public static void Init()
    {
        OnConnect += (dbEventArgs) =>
        {
            Log($"Connect {dbEventArgs.Sender}");
            var user = User.FindByIdentity(dbEventArgs.Sender);

            if (user is not null)
            {
                // If this is a returning user, i.e., we already have a `User` with this `Identity`,
                // set `Online: true`, but leave `Name` and `Identity` unchanged.
                user.Online = true;
                User.UpdateByIdentity(dbEventArgs.Sender, user);
            }
            else
            {
                // If this is a new user, create a `User` object for the `Identity`,
                // which is online, but hasn't set a name.
                new User
                {
                    Name = null,
                    Identity = dbEventArgs.Sender,
                    Online = true,
                }.Insert();
            }
        };
    }
```

Similarly, whenever a client disconnects, the module will execute the `OnDisconnect` event if it's registered. We'll use it to un-set the `Online` status of the `User` for the disconnected client.

Add the following code after the `OnConnect` lambda:

```C#
        OnDisconnect += (dbEventArgs) =>
        {
            var user = User.FindByIdentity(dbEventArgs.Sender);

            if (user is not null)
            {
                // This user should exist, so set `Online: false`.
                user.Online = false;
                User.UpdateByIdentity(dbEventArgs.Sender, user);
            }
            else
            {
                // User does not exist, log warning
                Log($"Warning: No user found for disconnected client.");
            }
        };
```

## Publish the module

And that's all of our module code! We'll run `spacetime publish` to compile our module and publish it on SpacetimeDB. `spacetime publish` takes an optional name which will map to the database's unique address. Clients can connect either by name or by address, but names are much more pleasant. Come up with a unique name, and fill it in where we've written `<module-name>`.

From the `quickstart-chat` directory, run:

```bash
spacetime publish --project-path server <module-name>
```

## Call Reducers

You can use the CLI (command line interface) to run reducers. The arguments to the reducer are passed in JSON format.

```bash
spacetime call <module-name> send_message '["Hello, World!"]'
```

Once we've called our `send_message` reducer, we can check to make sure it ran by running the `logs` command.

```bash
spacetime logs <module-name>
```

You should now see the output that your module printed in the database.

```bash
info: Hello, World!
```

## SQL Queries

SpacetimeDB supports a subset of the SQL syntax so that you can easily query the data of your database. We can run a query using the `sql` command.

```bash
spacetime sql <module-name> "SELECT * FROM Message"
```

```bash
 text
---------
 "Hello, World!"
```

## What's next?

You've just set up your first database in SpacetimeDB! The next step would be to create a client module that interacts with this module. You can use any of SpacetimDB's supported client languages to do this. Take a look at the quick start guide for your client language of choice: [Rust](/docs/languages/rust/rust-sdk-quickstart-guide), [C#](/docs/languages/csharp/csharp-sdk-quickstart-guide), [TypeScript](/docs/languages/typescript/typescript-sdk-quickstart-guide) or [Python](/docs/languages/python/python-sdk-quickstart-guide).

If you are planning to use SpacetimeDB with the Unity3d game engine, you can skip right to the [Unity Comprehensive Tutorial](/docs/game-dev/unity-tutorial) or check out our example game, [BitcraftMini](/docs/game-dev/unity-tutorial-bitcraft-mini).
