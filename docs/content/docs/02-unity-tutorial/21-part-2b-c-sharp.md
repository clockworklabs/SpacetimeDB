---
title: Unity Tutorial - Basic Multiplayer - Part 2a - Server Module (C#)
navTitle: 2b - Server (C#)
---

Need help with the tutorial? [Join our Discord server](https://discord.gg/spacetimedb)!

This progressive tutorial is continued from the [Part 1 Tutorial](/docs/unity/part-1)

## Create a Server Module

Run the following command to initialize the SpacetimeDB server module project with C# as the language:

```bash
spacetime init --lang=csharp server
```

This command creates a new folder named "server" within your Unity project directory and sets up the SpacetimeDB server project with C# as the programming language.

### SpacetimeDB Tables

In this section we'll be making some edits to the file `server/src/lib.cs`. We recommend you open up this file in an IDE like VSCode.

**Important: Open the `server/src/lib.cs` file and delete its contents. We will be writing it from scratch here.**

First we need to add some imports at the top of the file.

**Copy and paste into lib.cs:**

```csharp
// using SpacetimeDB; // Uncomment to omit `SpacetimeDB` attribute prefixes
using SpacetimeDB.Module;
using static SpacetimeDB.Runtime;
```

Then we are going to start by adding the global `Config` table. Right now it only contains the "message of the day" but it can be extended to store other configuration variables. This also uses a couple of attributes, like `[SpacetimeDB.Table]` which you can learn more about in our [C# module reference](/docs/modules/c-sharp). Simply put, this just tells SpacetimeDB to create a table which uses this struct as the schema for the table.

**Append to the bottom of lib.cs:**

```csharp
/// We're using this table as a singleton,
/// so there should typically only be one element where the version is 0.
[SpacetimeDB.Table(Public = true)]
public partial class Config
{
   [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
   public uint Version;
   public string? MessageOfTheDay;
}
```

Next, we're going to define a new `SpacetimeType` called `StdbVector3` which we're going to use to store positions. The difference between a `[SpacetimeDB.Type]` and a `[SpacetimeDB.Table]` is that tables actually store data, whereas the deriving `SpacetimeType` just allows you to create a new column of that type in a SpacetimeDB table. Therefore, `StdbVector3` is not, itself, a table.

**Append to the bottom of lib.cs:**

```csharp
/// This allows us to store 3D points in tables.
[SpacetimeDB.Type]
public partial class StdbVector3
{
   public float X;
   public float Y;
   public float Z;
}
```

Now we're going to create a table which actually uses the `StdbVector3` that we just defined. The `EntityComponent` is associated with all entities in the world, including players.

```csharp
/// This stores information related to all entities in our game. In this tutorial
/// all entities must at least have an entity_id, a position, a direction and they
/// must specify whether or not they are moving.
[SpacetimeDB.Table(Public = true)]
public partial class EntityComponent
{
   [SpacetimeDB.Column(ColumnAttrs.PrimaryKeyAuto)]
   public ulong EntityId;
   public StdbVector3 Position;
   public float Direction;
   public bool Moving;
}
```

Next, we will define the `PlayerComponent` table. The `PlayerComponent` table is used to store information related to players. Each player will have a row in this table, and will also have a row in the `EntityComponent` table with a matching `EntityId`. You'll see how this works later in the `CreatePlayer` reducer.

**Append to the bottom of lib.cs:**

```csharp
/// All players have this component and it associates an entity with the user's
/// Identity. It also stores their username and whether or not they're logged in.
[SpacetimeDB.Table(Public = true)]
public partial class PlayerComponent
{
   // An EntityId that matches an EntityId in the `EntityComponent` table.
   [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
   public ulong EntityId;

   // The user's identity, which is unique to each player
   [SpacetimeDB.Column(ColumnAttrs.Unique)]
   public Identity Identity;
   public string? Username;
   public bool LoggedIn;
}
```

Next, we write our very first reducer, `CreatePlayer`. From the client we will call this reducer when we create a new player:

**Append to the bottom of lib.cs:**

```csharp
/// This reducer is called when the user logs in for the first time and
/// enters a username.
[SpacetimeDB.Reducer]
public static void CreatePlayer(ReducerContext ctx, string username)
{
   // Get the Identity of the client who called this reducer
   Identity sender = ctx.Sender;

   // Make sure we don't already have a player with this identity
   PlayerComponent? user = PlayerComponent.FindByIdentity(sender);
   if (user is null)
   {
       throw new ArgumentException("Player already exists");
   }

   // Create a new entity for this player
   try
   {
       new EntityComponent
       {
           // EntityId = 0, // 0 is the same as leaving null to get a new, unique Id
           Position = new StdbVector3 { X = 0, Y = 0, Z = 0 },
           Direction = 0,
           Moving = false,
       }.Insert();
   }
   catch
   {
       Log("Error: Failed to create a unique EntityComponent", LogLevel.Error);
       throw;
   }

   // The PlayerComponent uses the same entity_id and stores the identity of
   // the owner, username, and whether or not they are logged in.
   try
   {
       new PlayerComponent
       {
           // EntityId = 0, // 0 is the same as leaving null to get a new, unique Id
           Identity = ctx.Sender,
           Username = username,
           LoggedIn = true,
       }.Insert();
   }
   catch
   {
       Log("Error: Failed to insert PlayerComponent", LogLevel.Error);
       throw;
   }
   Log($"Player created: {username}");
}
```

---

**SpacetimeDB Reducers**

"Reducer" is a term coined by Clockwork Labs that refers to a function which when executed "reduces" into a list of inserts and deletes, which is then packed into a single database transaction. Reducers can be called remotely using the CLI, client SDK or can be scheduled to be called at some future time from another reducer call.

---

SpacetimeDB gives you the ability to define custom reducers that automatically trigger when certain events occur.

- `Init` - Called the first time you publish your module and anytime you clear the database. We'll learn about publishing later.
- `Connect` - Called when a user connects to the SpacetimeDB module. Their identity can be found in the `Sender` value of the `ReducerContext`.
- `Disconnect` - Called when a user disconnects from the SpacetimeDB module.

Next, we are going to write a custom `Init` reducer that inserts the default message of the day into our `Config` table.

**Append to the bottom of lib.cs:**

```csharp
/// Called when the module is initially published
[SpacetimeDB.Reducer(ReducerKind.Init)]
public static void OnInit()
{
   try
   {
       new Config
       {
           Version = 0,
           MessageOfTheDay = "Hello, World!",
       }.Insert();
   }
   catch
   {
       Log("Error: Failed to insert Config", LogLevel.Error);
       throw;
   }
}
```

We use the `Connect` and `Disconnect` reducers to update the logged in state of the player. The `UpdatePlayerLoginState` helper function we are about to define looks up the `PlayerComponent` row using the user's identity and if it exists, it updates the `LoggedIn` variable and calls the auto-generated `Update` function on `PlayerComponent` to update the row.

**Append to the bottom of lib.cs:**

```csharp
/// Called when the client connects, we update the LoggedIn state to true
[SpacetimeDB.Reducer(ReducerKind.Init)]
public static void ClientConnected(ReducerContext ctx) =>
   UpdatePlayerLoginState(ctx, loggedIn:true);
```

```csharp
/// Called when the client disconnects, we update the logged_in state to false
[SpacetimeDB.Reducer(ReducerKind.Disconnect)]
public static void ClientDisonnected(ReducerContext ctx) =>
   UpdatePlayerLoginState(ctx, loggedIn:false);
```

```csharp
/// This helper function gets the PlayerComponent, sets the LoggedIn
/// variable and updates the PlayerComponent table row.
private static void UpdatePlayerLoginState(ReducerContext ctx, bool loggedIn)
{
   PlayerComponent? player = PlayerComponent.FindByIdentity(ctx.Sender);
   if (player is null)
   {
       throw new ArgumentException("Player not found");
   }

   player.LoggedIn = loggedIn;
   PlayerComponent.UpdateByIdentity(ctx.Sender, player);
}
```

Our final reducer handles player movement. In `UpdatePlayerPosition` we look up the `PlayerComponent` using the user's Identity. If we don't find one, we return an error because the client should not be sending moves without calling `CreatePlayer` first.

Using the `EntityId` in the `PlayerComponent` we retrieved, we can lookup the `EntityComponent` that stores the entity's locations in the world. We update the values passed in from the client and call the auto-generated `Update` function.

**Append to the bottom of lib.cs:**

```csharp
/// Updates the position of a player. This is also called when the player stops moving.
[SpacetimeDB.Reducer]
private static void UpdatePlayerPosition(
   ReducerContext ctx,
   StdbVector3 position,
   float direction,
   bool moving)
{
   // First, look up the player using the sender identity
   PlayerComponent? player = PlayerComponent.FindByIdentity(ctx.Sender);
   if (player is null)
   {
       throw new ArgumentException("Player not found");
   }
   // Use the Player's EntityId to retrieve and update the EntityComponent
   ulong playerEntityId = player.EntityId;
   EntityComponent? entity = EntityComponent.FindByEntityId(playerEntityId);
   if (entity is null)
   {
       throw new ArgumentException($"Player Entity '{playerEntityId}' not found");
   }

   entity.Position = position;
   entity.Direction = direction;
   entity.Moving = moving;
   EntityComponent.UpdateByEntityId(playerEntityId, entity);
}
```

---

**Server Validation**

In a fully developed game, the server would typically perform server-side validation on player movements to ensure they comply with game boundaries, rules, and mechanics. This validation, which we omit for simplicity in this tutorial, is essential for maintaining game integrity, preventing cheating, and ensuring a fair gaming experience. Remember to incorporate appropriate server-side validation in your game's development to ensure a secure and fair gameplay environment.

---

### Finally, Add Chat Support

The client project has a chat window, but so far, all it's used for is the message of the day. We are going to add the ability for players to send chat messages to each other.

First lets add a new `ChatMessage` table to the SpacetimeDB module. Add the following code to `lib.cs`.

**Append to the bottom of server/src/lib.cs:**

```csharp
[SpacetimeDB.Table(Public = true)]
public partial class ChatMessage
{
   // The primary key for this table will be auto-incremented
   [SpacetimeDB.Column(ColumnAttrs.PrimaryKeyAuto)]

   // The entity id of the player that sent the message
   public ulong SenderId;

   // Message contents
   public string? Text;
}
```

Now we need to add a reducer to handle inserting new chat messages.

**Append to the bottom of server/src/lib.cs:**

```csharp
/// Adds a chat entry to the ChatMessage table
[SpacetimeDB.Reducer]
public static void SendChatMessage(ReducerContext ctx, string text)
{
   // Get the player's entity id
   PlayerComponent? player = PlayerComponent.FindByIdentity(ctx.Sender);
   if (player is null)
   {
       throw new ArgumentException("Player not found");
   }


   // Insert the chat message
   new ChatMessage
   {
       SenderId = player.EntityId,
       Text = text,
   }.Insert();
}
```

## Wrapping Up

### Publishing a Module to SpacetimeDB

💡View the [entire lib.cs file](https://gist.github.com/dylanh724/68067b4e843ea6e99fbd297fe1a87c49)

Now that we've written the code for our server module and reached a clean checkpoint, we need to publish it to SpacetimeDB. This will create the database and call the init reducer. In your terminal or command window, run the following commands.

```bash
cd server
spacetime publish -c unity-tutorial
```

From here, the [next tutorial](/docs/unity/part-3) continues with a Client (Unity) focus.
