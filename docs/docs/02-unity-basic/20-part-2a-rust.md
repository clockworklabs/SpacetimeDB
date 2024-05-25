---
title: Unity Tutorial - Basic Multiplayer - Part 2a - Server Module (Rust)
---

Need help with the tutorial? [Join our Discord server](https://discord.gg/spacetimedb)!

This progressive tutorial is continued from the [Part 1 Tutorial](/docs/unity/part-1)

## Create a Server Module

Run the following command to initialize the SpacetimeDB server module project with Rust as the language:

```bash
spacetime init --lang=rust server
```

This command creates a new folder named "server" within your Unity project directory and sets up the SpacetimeDB server project with Rust as the programming language.

### SpacetimeDB Tables

In this section we'll be making some edits to the file `server/src/lib.rs`. We recommend you open up this file in an IDE like VSCode or RustRover.

**Important: Open the `server/src/lib.rs` file and delete its contents. We will be writing it from scratch here.**

First we need to add some imports at the top of the file.

**Copy and paste into lib.rs:**

```rust
use spacetimedb::{spacetimedb, Identity, SpacetimeType, ReducerContext};
use log;
```

Then we are going to start by adding the global `Config` table. Right now it only contains the "message of the day" but it can be extended to store other configuration variables. This also uses a couple of macros, like `#[spacetimedb(table)]` which you can learn more about in our [Rust module reference](/docs/modules/rust). Simply put, this just tells SpacetimeDB to create a table which uses this struct as the schema for the table.

**Append to the bottom of lib.rs:**

```rust
// We're using this table as a singleton, so there should typically only be one element where the version is 0.
#[spacetimedb(table)]
#[derive(Clone)]
pub struct Config {
    #[primarykey]
    pub version: u32,
    pub message_of_the_day: String,
}
```

Next, we're going to define a new `SpacetimeType` called `StdbVector3` which we're going to use to store positions. The difference between a `#[derive(SpacetimeType)]` and a `#[spacetimedb(table)]` is that tables actually store data, whereas the deriving `SpacetimeType` just allows you to create a new column of that type in a SpacetimeDB table. Therefore, `StdbVector3` is not, itself, a table.

**Append to the bottom of lib.rs:**

```rust
// This allows us to store 3D points in tables.
#[derive(SpacetimeType, Clone)]
pub struct StdbVector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
```

Now we're going to create a table which actually uses the `StdbVector3` that we just defined. The `EntityComponent` is associated with all entities in the world, including players.

```rust
// This stores information related to all entities in our game. In this tutorial
// all entities must at least have an entity_id, a position, a direction and they
// must specify whether or not they are moving.
#[spacetimedb(table)]
#[derive(Clone)]
pub struct EntityComponent {
    #[primarykey]
    // The autoinc macro here just means every time we insert into this table
    // we will receive a new row where this value will be increased by one. This
    // allows us to easily get rows where `entity_id` is unique.
    #[autoinc]
    pub entity_id: u64,
    pub position: StdbVector3,
    pub direction: f32,
    pub moving: bool,
}
```

Next, we will define the `PlayerComponent` table. The `PlayerComponent` table is used to store information related to players. Each player will have a row in this table, and will also have a row in the `EntityComponent` table with a matching `entity_id`. You'll see how this works later in the `create_player` reducer.

**Append to the bottom of lib.rs:**

```rust
// All players have this component and it associates an entity with the user's
// Identity. It also stores their username and whether or not they're logged in.
#[derive(Clone)]
#[spacetimedb(table)]
pub struct PlayerComponent {
    // An entity_id that matches an entity_id in the `EntityComponent` table.
    #[primarykey]
    pub entity_id: u64,

    // The user's identity, which is unique to each player
    #[unique]
    pub owner_id: Identity,
    pub username: String,
    pub logged_in: bool,
}
```

Next, we write our very first reducer, `create_player`. From the client we will call this reducer when we create a new player:

**Append to the bottom of lib.rs:**

```rust
// This reducer is called when the user logs in for the first time and
// enters a username
#[spacetimedb(reducer)]
pub fn create_player(ctx: ReducerContext, username: String) -> Result<(), String> {
    // Get the Identity of the client who called this reducer
    let owner_id = ctx.sender;

    // Make sure we don't already have a player with this identity
    if PlayerComponent::filter_by_owner_id(&owner_id).is_some() {
        log::info!("Player already exists");
        return Err("Player already exists".to_string());
    }

    // Create a new entity for this player and get a unique `entity_id`.
    let entity_id = EntityComponent::insert(EntityComponent
    {
        entity_id: 0,
        position: StdbVector3 { x: 0.0, y: 0.0, z: 0.0 },
        direction: 0.0,
        moving: false,
    }).expect("Failed to create a unique PlayerComponent.").entity_id;

    // The PlayerComponent uses the same entity_id and stores the identity of
    // the owner, username, and whether or not they are logged in.
    PlayerComponent::insert(PlayerComponent {
        entity_id,
        owner_id,
        username: username.clone(),
        logged_in: true,
    }).expect("Failed to insert player component.");

    log::info!("Player created: {}({})", username, entity_id);

    Ok(())
}
```

---

**SpacetimeDB Reducers**

"Reducer" is a term coined by Clockwork Labs that refers to a function which when executed "reduces" into a list of inserts and deletes, which is then packed into a single database transaction. Reducers can be called remotely using the CLI, client SDK or can be scheduled to be called at some future time from another reducer call.

---

SpacetimeDB gives you the ability to define custom reducers that automatically trigger when certain events occur.

-   `init` - Called the first time you publish your module and anytime you clear the database. We'll learn about publishing later.
-   `connect` - Called when a user connects to the SpacetimeDB module. Their identity can be found in the `sender` value of the `ReducerContext`.
-   `disconnect` - Called when a user disconnects from the SpacetimeDB module.

Next, we are going to write a custom `Init` reducer that inserts the default message of the day into our `Config` table. The `Config` table only ever contains a single row with version 0, which we retrieve using `Config.FilterByVersion(0)`.

**Append to the bottom of lib.rs:**

```rust
// Called when the module is initially published
#[spacetimedb(init)]
pub fn init() {
    Config::insert(Config {
        version: 0,
        message_of_the_day: "Hello, World!".to_string(),
    }).expect("Failed to insert config.");
}
```

We use the `connect` and `disconnect` reducers to update the logged in state of the player. The `update_player_login_state` helper function looks up the `PlayerComponent` row using the user's identity and if it exists, it updates the `logged_in` variable and calls the auto-generated `update` function on `PlayerComponent` to update the row.

**Append to the bottom of lib.rs:**

```rust
// Called when the client connects, we update the logged_in state to true
#[spacetimedb(connect)]
pub fn client_connected(ctx: ReducerContext) {
    update_player_login_state(ctx, true);
}
```

```rust
// Called when the client disconnects, we update the logged_in state to false
#[spacetimedb(disconnect)]
pub fn client_disconnected(ctx: ReducerContext) {
    update_player_login_state(ctx, false);
}
```

```rust
// This helper function gets the PlayerComponent, sets the logged
// in variable and updates the PlayerComponent table row.
pub fn update_player_login_state(ctx: ReducerContext, logged_in: bool) {
    if let Some(player) = PlayerComponent::filter_by_owner_id(&ctx.sender) {
        // We clone the PlayerComponent so we can edit it and pass it back.
        let mut player = player.clone();
        player.logged_in = logged_in;
        PlayerComponent::update_by_entity_id(&player.entity_id.clone(), player);
    }
}
```

Our final reducer handles player movement. In `update_player_position` we look up the `PlayerComponent` using the user's Identity. If we don't find one, we return an error because the client should not be sending moves without calling `create_player` first.

Using the `entity_id` in the `PlayerComponent` we retrieved, we can lookup the `EntityComponent` that stores the entity's locations in the world. We update the values passed in from the client and call the auto-generated `update` function.

**Append to the bottom of lib.rs:**

```rust
// Updates the position of a player. This is also called when the player stops moving.
#[spacetimedb(reducer)]
pub fn update_player_position(
    ctx: ReducerContext,
    position: StdbVector3,
    direction: f32,
    moving: bool,
) -> Result<(), String> {
    // First, look up the player using the sender identity, then use that
    // entity_id to retrieve and update the EntityComponent
    if let Some(player) = PlayerComponent::filter_by_owner_id(&ctx.sender) {
        if let Some(mut entity) = EntityComponent::filter_by_entity_id(&player.entity_id) {
            entity.position = position;
            entity.direction = direction;
            entity.moving = moving;
            EntityComponent::update_by_entity_id(&player.entity_id, entity);
            return Ok(());
        }
    }

    // If we can not find the PlayerComponent or EntityComponent for
    // this player then something went wrong.
    return Err("Player not found".to_string());
}
```

---

**Server Validation**

In a fully developed game, the server would typically perform server-side validation on player movements to ensure they comply with game boundaries, rules, and mechanics. This validation, which we omit for simplicity in this tutorial, is essential for maintaining game integrity, preventing cheating, and ensuring a fair gaming experience. Remember to incorporate appropriate server-side validation in your game's development to ensure a secure and fair gameplay environment.

---

### Publishing a Module to SpacetimeDB

Now that we've written the code for our server module and reached a clean checkpoint, we need to publish it to SpacetimeDB. This will create the database and call the init reducer. In your terminal or command window, run the following commands.

```bash
cd server
spacetime publish -c unity-tutorial
```

### Finally, Add Chat Support

The client project has a chat window, but so far, all it's used for is the message of the day. We are going to add the ability for players to send chat messages to each other.

First lets add a new `ChatMessage` table to the SpacetimeDB module. Add the following code to `lib.rs`.

**Append to the bottom of server/src/lib.rs:**

```rust
#[spacetimedb(table)]
pub struct ChatMessage {
    // The primary key for this table will be auto-incremented
    #[primarykey]
    #[autoinc]
    pub message_id: u64,

    // The entity id of the player that sent the message
    pub sender_id: u64,
    // Message contents
    pub text: String,
}
```

Now we need to add a reducer to handle inserting new chat messages.

**Append to the bottom of server/src/lib.rs:**

```rust
// Adds a chat entry to the ChatMessage table
#[spacetimedb(reducer)]
pub fn send_chat_message(ctx: ReducerContext, text: String) -> Result<(), String> {
    if let Some(player) = PlayerComponent::filter_by_owner_id(&ctx.sender) {
        // Now that we have the player we can insert the chat message using the player entity id.
        ChatMessage::insert(ChatMessage {
            // this column auto-increments so we can set it to 0
            message_id: 0,
            sender_id: player.entity_id,
            text,
        })
        .unwrap();

        return Ok(());
    }

    Err("Player not found".into())
}
```

## Wrapping Up

Now that we added chat support, let's publish the latest module version to SpacetimeDB, assuming we're still in the `server` dir:

```bash
spacetime publish -c unity-tutorial
```

From here, the [next tutorial](/docs/unity/part-3) continues with a Client (Unity) focus.
