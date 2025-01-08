# Unity Tutorial - Blackholio - Part 2a - Server Module (Rust)

Need help with the tutorial? [Join our Discord server](https://discord.gg/spacetimedb)!

This progressive tutorial is continued from the [Part 1 Tutorial](/docs/unity/part-1)

## Create a Server Module

Run the following command to initialize the SpacetimeDB server module project with Rust as the language:

```bash
spacetime init --lang=rust rust-server
```

This command creates a new folder named "rust-server" within your Unity project directory and sets up the SpacetimeDB server project with Rust as the programming language.

### SpacetimeDB Tables

In this section we'll be making some edits to the file `server/src/lib.rs`. We recommend you open up this file in an IDE like VSCode or RustRover.

**Important: Open the `server/src/lib.rs` file and delete its contents. We will be writing it from scratch here.**

First we need to add some imports at the top of the file.

**Copy and paste into lib.rs:**

```rust
use spacetimedb::{Identity, SpacetimeType, ReducerContext};
use log;
```

We are going to start by defining a SpacetimeDB *table*. A *table* in SpacetimeDB is a relational database table which stores rows, similar to something you might find in SQL. SpacetimeDB tables differ from normal relational database tables in that they are stored fully in memory, are blazing fast to access, and are defined in your module code, rather than in SQL.

Each row in a SpacetimeDB table is associated with a `struct` type in Rust.

Let's start by defining the `Config` table. This is a simple table which will store some metadata about our game's state. Add the following code to `lib.rs`.

```rust
// We're using this table as a singleton, so in this table
// there only be one element where the `id` is 0.
#[spacetimedb::table(name = config, public)]
pub struct Config {
    #[primary_key]
    pub id: u32,
    pub world_size: u64,
}
```

Let's break down this code. This defines a normal Rust `struct` with two fields: `id` and `world_size`. We have decorated the struct with the `spacetimedb::table` macro. This procedural Rust macro signals to SpacetimeDB that it should create a new SpacetimeDB table with the row type defined by the `Config` type's fields.

> NOTE: It is possible to have two different tables with different table names share the same type.

The `spacetimedb::table` macro takes two parameters, a `name` which is the name of the table and what you will use to query the table in SQL, and a `public` visibility modifier which ensures that the rows of this table are visible to everyone.

The `#[primary_key]` attribute, specifies that the `id` field should be used as the primary key of the table.

> NOTE: The primary key of a row defines the "identity" of the row. A change to a row which doesn't modify the primary key is considered an update, but if you change the primary key, then you have deleted the old row and inserted a new one.

You can learn more the `table` macro in our [Rust module reference](/docs/modules/rust).

### Creating Entities

Next, we're going to define a new `SpacetimeType` called `DbVector3` which we're going to use to store positions. The difference between a `#[derive(SpacetimeType)]` and a `#[spacetimedb(table)]` is that tables actually store data, whereas the deriving `SpacetimeType` just allows you to create a new column of that type in a SpacetimeDB table. Therefore, `DbVector3` is only a type, and does not define a table.

**Append to the bottom of lib.rs:**

```rust
// This allows us to store 2D points in tables.
#[derive(SpacetimeType, Clone)]
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
    pub entity_id: u32,
    pub position: DbVector2,
    pub mass: u32,
}

#[spacetimedb::table(name = circle, public)]
pub struct Circle {
    #[primary_key]
    pub entity_id: u32,
    #[index(btree)]
    pub player_id: u32,
    pub direction: DbVector2,
    pub speed: f32,
    pub last_split_time: Timestamp,
}

#[spacetimedb::table(name = food, public)]
pub struct Food {
    #[primary_key]
    pub entity_id: u32,
}
```

The first table we defined is the `entity` table. An entity represents an object in our game world. We have decided, for convenience, that all entities in our game should share some common fields, namely `position` and `mass`.

We can create different types of entities with additional data by creating a new tables with additional fields that have an `entity_id` which references a row in the `entity` table.

We've created two types of entities in our game world: `Food`s and `Circle`s. `Food` does not have any additional fields beyond the attributes in the `entity` table, so the `food` table simply represents the set of `entity_id`s that we want to recognize as food.

The `Circle` table, however, represents an entity that is controlled by a player. We've added a few additional fields to a `Circle` like `player_id` so that we know which player that circle belongs to.

### Writing a Reducer

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
    if PlayerComponent::find_by_owner_id(&owner_id).is_some() {
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

- `init` - Called the first time you publish your module and anytime you clear the database. We'll learn about publishing later.
- `connect` - Called when a user connects to the SpacetimeDB module. Their identity can be found in the `sender` value of the `ReducerContext`.
- `disconnect` - Called when a user disconnects from the SpacetimeDB module.

Next, we are going to write a custom `Init` reducer that inserts the default message of the day into our `Config` table.

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
    if let Some(player) = PlayerComponent::find_by_owner_id(&ctx.sender) {
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
    if let Some(player) = PlayerComponent::find_by_owner_id(&ctx.sender) {
        if let Some(mut entity) = EntityComponent::find_by_entity_id(&player.entity_id) {
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
#[spacetimedb(table(public))]
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
    if let Some(player) = PlayerComponent::find_by_owner_id(&ctx.sender) {
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
