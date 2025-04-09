# SpacetimeDB

> SpacetimeDB is a fully-featured relational database system that integrates
application logic directly within the database, eliminating the need for
separate web or game servers. It supports multiple programming languages,
including C# and Rust, allowing developers to write and deploy entire
applications as a single binary. It is optimized for high-throughput and low
latency multiplayer applications like multiplayer games.

Users upload their application logic to run inside SpacetimeDB as a WebAssembly
module. There are three main features of SpacetimeDB: tables, reducers, and
subscription queries. Tables are relational database tables like you would find
in a database like Postgres. Reducers are atomic, transactional, RPC functions
that are defined in the WebAssembly module which can be called by clients.
Subscription queries are SQL queries which are made over a WebSocket connection
which are initially evaluated by SpacetimeDB and then incrementally evaluated
sending changes to the query result over the WebSocket.

All data in the tables are stored in memory, but are persisted to the disk via a
Write-Ahead Log (WAL) called the Commitlog. All tables are persistent in
SpacetimeDB.

SpacetimeDB allows users to code generate type-safe client libraries based on
the tables, types, and reducers defined in their module. Subscription queries
allows the client SDK to store a partial, live updating, replica of the servers
state. This makes reading database state on the client extremely low-latency.

Authentication is implemented in SpacetimeDB using the OpenID Connect protocol.
An OpenID Connect token with a valid `iss`/`sub` pair constitutes a unique and
authenticable SpacetimeDB identity. SpacetimeDB uses the `Identity` type as an
identifier for all such identities. `Identity` is computed from the `iss`/`sub`
pair using the following algorithm:

1. Concatenate the issuer and subject with a pipe symbol (`|`).
2. Perform the first BLAKE3 hash on the concatenated string.
3. Get the first 26 bytes of the hash (let's call this `idHash`).
4. Create a 28-byte sequence by concatenating the bytes `0xc2`, `0x00`, and `idHash`.
5. Compute the BLAKE3 hash of the 28-byte sequence from step 4 (let's call this `checksumHash`).
6. Construct the final 32-byte `Identity` by concatenating: the two prefix bytes (`0xc2`, `0x00`), the first 4 bytes of `checksumHash`, and the 26-byte `idHash`.
7. This final 32-byte value is typically represented as a hexadecimal string.

```ascii
Byte Index: |  0  |  1  |  2  |  3  |  4  |  5  |  6  | ... | 31  |
            +-----+-----+-----+-----+-----+-----+-----+-----+-----+
Contents:   | 0xc2| 0x00| Checksum Hash (4 bytes) |  ID Hash (26 bytes)   |
            +-----+-----+-------------------------+-----------------------+
                      (First 4 bytes of           (First 26 bytes of
                       BLAKE3(0xc200 || idHash))    BLAKE3(iss|sub))
```

This allows SpacetimeDB to easily integrate with OIDC authentication
providers like FirebaseAuth, Auth0, or SuperTokens.

Clockwork Labs, the developers of SpacetimeDB, offers three products:

1. SpacetimeDB Standalone: a source available (Business Source License), single node, self-hosted version
2. SpacetimeDB Maincloud: a hosted, managed-service, serverless cluster
3. SpacetimeDB Enterprise: a closed-source, clusterized version of SpacetimeDB which can be licensed for on-prem hosting or dedicated hosting

## Basic Project Workflow

Getting started with SpacetimeDB involves a few key steps:

1.  **Install SpacetimeDB:** Install the `spacetime` CLI tool for your operating system. This tool is used for managing modules, databases, and local instances.

    *   **macOS:**
        ```bash
        curl -sSf https://install.spacetimedb.com | sh
        ```
    *   **Windows (PowerShell):**
        ```powershell
        iwr https://windows.spacetimedb.com -useb | iex
        ```
    *   **Linux:**
        ```bash
        curl -sSf https://install.spacetimedb.com | sh
        ```
    *   **Docker (to run the server):**
        ```bash
        # This command starts a SpacetimeDB server instance in Docker
        docker run --rm --pull always -p 3000:3000 clockworklabs/spacetime start 
        # Note: While the CLI can be installed separately (see above), you can also execute 
        # CLI commands *within* the running Docker container (e.g., using `docker exec`) 
        # or use the image as a base for a custom image containing your module management tools.
        ```
    *   **Docker (to execute CLI commands directly):**
        You can also use the Docker image to run `spacetime` CLI commands without installing the CLI locally. For commands that operate on local files (like `build`, `publish`, `generate`), this involves mounting your project directory into the container. For commands that only interact with a database instance (like `sql`, `status`), mounting is typically not required, but network access to the database is.
        ```bash
        # Example: Build a module located in the current directory (.)
        # Mount current dir to /module inside container, set working dir to /module
        docker run --rm -v "$(pwd):/module" -w /module clockworklabs/spacetime build --project-path .

        # Example: Publish the module after building
        # Assumes a local server is running (or use --host for Maincloud/other)
        docker run --rm -v "$(pwd):/module" -w /module --network host clockworklabs/spacetime publish --project-path . my-database-name
        # Note: `--network host` is often needed to connect to a local server from the container.
        ```
    *   For more details or troubleshooting, see the official [Getting Started Guide](https://spacetimedb.com/docs/getting-started) and [Installation Page](https://spacetimedb.com/install).

1.b **Log In (If Necessary):** If you plan to publish to a server that requires authentication (like the public Maincloud at `maincloud.spacetimedb.com`), you generally need to log in first using `spacetime login`. This associates your actions with your global SpacetimeDB identity (e.g., linked to your spacetimedb.com account).
    ```bash
    spacetime login
    # Follow the prompts to authenticate via web browser
    ```
    If you attempt commands like `publish` against an authenticated server without being logged in, the CLI will prompt you: `You are not logged in. Would you like to log in with spacetimedb.com? [y/N]`. 
    *   Choosing `y` initiates the standard browser login flow.
    *   Choosing `n` proceeds without a global login for this operation. The CLI will confirm `We have logged in directly to your target server. WARNING: This login will NOT work for any other servers.` This uses or creates a server-issued identity specific to that server (see Step 5).

    In general, using `spacetime login` (which authenticates via spacetimedb.com) is recommended, as the resulting identities are portable across different SpacetimeDB servers.

2.  **Initialize Server Module:** Create a new directory for your project and use the CLI to initialize the server module structure:
    ```bash
    # For Rust
    spacetime init --lang rust my_server_module
    # For C#
    spacetime init --lang csharp my_server_module
    ```
    :::note C# Project Filename Convention (SpacetimeDB CLI)
    The `spacetime` CLI tool (particularly `publish` and `build`) follows a convention and often expects the C# project file (`.csproj`) to be named `StdbModule.csproj`, matching the default generated by `spacetime init`. This **is** a requirement of the SpacetimeDB tool itself (due to how it locates build artifacts), not the underlying .NET build system. This is a known issue tracked [here](https://github.com/clockworklabs/SpacetimeDB/issues/2475). If you encounter issues where the build succeeds but publishing fails (e.g., "couldn't find the output file" or silent failures after build), ensure your `.csproj` file is named `StdbModule.csproj` within your module's directory.
    :::
3.  **Define Schema & Logic:** Edit the generated module code (`lib.rs` for Rust, `Lib.cs` for C#) to define your custom types (`[SpacetimeType]`/`[Type]`), database tables (`#[table]`/`[Table]`), and reducers (`#[reducer]`/`[Reducer]`).
4.  **Build Module:** Compile your module code into WebAssembly using the CLI:
    ```bash
    # Run from the directory containing your module folder
    spacetime build --project-path my_server_module 
    ```
    :::note C# Build Prerequisite (.NET SDK)
    Building a **C# module** (on any platform: Windows, macOS, Linux) requires the .NET SDK to be installed. If the build fails with an error mentioning `dotnet workload list` or `No .NET SDKs were found`, you need to install the SDK first. Download and install the **.NET 8 SDK** specifically from the official Microsoft website: [https://dotnet.microsoft.com/download](https://dotnet.microsoft.com/download). Newer versions (like .NET 9) are not currently supported for building SpacetimeDB modules, although they can be installed alongside .NET 8 without conflicting.
    :::
5.  **Publish Module:** Deploy your compiled module to a SpacetimeDB instance (either a local one started with `spacetime start` or the managed Maincloud). Publishing creates or updates a database associated with your module.

    *   Providing a `[name|identity]` for the database is **optional**. If omitted, a nameless database will be created and assigned a unique `Identity` automatically. If providing a *name*, it must match the regex `^[a-z0-9]+(-[a-z0-9]+)*$`.
    *   By default (`--project-path`), it builds the module before publishing. Use `--bin-path <wasm_file>` to publish a pre-compiled WASM instead.
    *   Use `-s, --server <server>` to specify the target instance (e.g., `maincloud.spacetimedb.com` or the nickname `maincloud`). If omitted, it targets a local instance or uses your configured default (check with `spacetime server list`).
    *   Use `-c, --delete-data` when updating an existing database identity to destroy all existing data first.

    :::note Server-Issued Identities
    If you publish without being logged in (and choose to proceed without a global login when prompted), the SpacetimeDB server instance will generate or use a unique "server-issued identity" for the database operation. This identity is specific to that server instance. Its issuer (`iss`) is specifically `http://localhost`, and its subject (`sub`) will be a generated UUIDv4. This differs from the global identities derived from OIDC providers (like spacetimedb.com) when you use `spacetime login`. The token associated with this identity is signed by the issuing server, and the signature will be considered invalid if the token is presented to any other SpacetimeDB server instance.
    :::

    ```bash
    # Build and publish from source to 'my-database-name' on the default server
    spacetime publish --project-path my_server_module my-database-name

    # Example: Publish a pre-compiled wasm to Maincloud using its nickname, clearing existing data
    spacetime publish --bin-path ./my_module/target/wasm32-wasi/debug/my_module.wasm -s maincloud -c my-cloud-db-identity
    ```

6.  **List Databases (Optional):** Use `spacetime list` to see the databases associated with your logged-in identity on the target server (defaults to your configured server). This is helpful to find the `Identity` of databases, especially unnamed ones.
    ```bash
    # List databases on the default server
    spacetime list

    # List databases on Maincloud
    # spacetime list -s maincloud
    ```

7.  **Generate Client Bindings:** Create type-safe client code based on your module's definitions.
    This command inspects your compiled module's schema (tables, types, reducers) and generates corresponding code (classes, structs, functions) for your target client language. This allows you to interact with your SpacetimeDB module in a type-safe way on the client.
    ```bash
    # For Rust client (output to src/module_bindings)
    spacetime generate --lang rust --out-dir path/to/client/src/module_bindings --project-path my_server_module
    # For C# client (output to module_bindings directory)
    spacetime generate --lang csharp --out-dir path/to/client/module_bindings --project-path my_server_module
    ```
8.  **Develop Client:** Create your client application (e.g., Rust binary, C# console app, Unity game). Use the generated bindings and the appropriate client SDK to:
    *   Connect to the database (`my-database-name`).
    *   Subscribe to data in public tables.
    *   Register callbacks to react to data changes.
    *   Call reducers defined in your module.
9.  **Run:** Start your SpacetimeDB instance (if local or Docker), then run your client application.

10. **Inspect Data (Optional):** Use the `spacetime sql` command to run SQL queries directly against your database to view or verify data.
    ```bash
    # Query all data from the 'player_state' table in 'my-database-name'
    # Note: Table names are case-sensitive (match your definition)
    spacetime sql my-database-name "SELECT * FROM PlayerState"

    # Use --interactive for a SQL prompt
    # spacetime sql --interactive my-database-name
    ```

11. **View Logs (Optional):** Use the `spacetime logs` command to view logs generated by your module's reducers (e.g., using `log::info!` in Rust or `Log.Info()` in C#).
    ```bash
    # Show all logs for 'my-database-name'
    spacetime logs my-database-name

    # Follow the logs in real-time (like tail -f)
    # spacetime logs -f my-database-name

    # Show the last 50 log lines
    # spacetime logs -n 50 my-database-name
    ```

12. **Delete Database (Optional):** When you no longer need a database (e.g., after testing), you can delete it using `spacetime delete` with its name or identity.
    ```bash
    # Delete the database named 'my-database-name'
    spacetime delete my-database-name

    # Delete a database by its identity (replace with actual identity)
    # spacetime delete 0x123abc...
    ```

## Core Concepts and Syntax Examples

### Server Module (Rust)

#### Defining Types

Any custom struct or enum used as a field in a table or as a parameter/return type in a reducer must derive `SpacetimeType`. This allows SpacetimeDB to serialize and deserialize the type.

Use `#[sats(name = "...")]` to explicitly control the type name exposed to other languages (like C#) through generated bindings. This is useful for namespacing or avoiding conflicts.

```rust
use spacetimedb::{SpacetimeType, Identity, Timestamp};

#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub enum PlayerStatus {
    Idle,
    Walking(Position),
    Fighting(Identity), // Store the identity of the opponent
}

// Specify a cross-language name for the enum itself and one variant
#[derive(SpacetimeType, Clone, Debug, PartialEq)]
#[sats(name = "Game.ItemType")] // Will be Game.ItemType in C# bindings
pub enum ItemType {
    Weapon,
    Armor,
    #[sats(name = "ConsumableItem")] // This variant will be ConsumableItem in C#
    Potion,
}

// Type aliases can be defined using `pub type`
pub type PlayerScore = u32;

// Advanced: For types with lifetimes or custom binary representations,
// you can derive `spacetimedb::Deserialize` and use the `bsatn` crate
// (provided by spacetimedb::spacetimedb_lib) for manual deserialization if needed.
```

:::info Rust `crate-type = ["cdylib"]`
The `[lib]` section in your module's `Cargo.toml` must contain `crate-type = ["cdylib"]`. This tells the Rust compiler to produce a dynamic system library compatible with the C ABI, which allows the SpacetimeDB host (written in Rust) to load and interact with your compiled WebAssembly module.
:::

#### Defining Tables

Tables store the application's data. They are defined using Rust structs annotated with `#[table]`.
This attribute automatically derives `SpacetimeType`, `Serialize`, `Deserialize`, and `Debug` for the struct.

:::caution `#[derive(SpacetimeType)]` Conflict
Do **not** explicitly add `#[derive(SpacetimeType)]` to a struct that also has the `#[table]` attribute. The `#[table]` macro handles this automatically. Including both will lead to `E0119: conflicting implementations` compilation errors. Simply use `#[derive(Clone, Debug)]` or other necessary traits.
:::

Importantly, instances of the table struct are just plain data. Modifying a struct instance **does not** automatically update the database. Instead, you interact with the database tables through generated handles obtained from the `ReducerContext` (e.g., `ctx.db.my_table()`). See the Reducers section for more details on interaction and necessary imports (`use spacetimedb::Table;`).

Fields can be marked with `#[primary_key]`, `#[auto_inc]`, `#[unique]`, or indexed using `#[index(...)]` or `#[table(index(...))]`.
Use `Option<T>` for nullable fields.

:::caution `name = identifier` Syntax
Note that the `name` parameter within the `#[table(...)]` attribute expects a plain **identifier** (like `player_state`), not a string literal (`"player_state"`). Using a string literal may lead to compilation errors.
:::

:::caution Important: Public Tables
By default, tables are **private** and only accessible by server-side reducer code. If clients need to read or subscribe to a table's data, you **must** mark the table as `public` using `#[table(..., public)]`.

*Common Pitfall:* If your client subscriptions fail with "table not found" or "not a valid table" errors, or if subscribed tables appear empty on the client despite having data on the server, double-check that the relevant tables are marked `public`.
:::

:::caution Case Sensitivity
The identifier specified in `name = ...` within the `#[table(...)]` attribute is case-sensitive. When referring to this table in SQL queries (e.g., in client-side `subscribe` calls), you **must** use the exact same casing.

*Example:* If defined as `#[table(name = PlayerState)]`, querying `SELECT * FROM playerstate` or `SELECT * FROM player_state` will fail. You must use `SELECT * FROM PlayerState`.
:::

:::caution Note on Modifying Instances
Instances of your table classes/structs are plain data objects. Modifying an instance **does not** automatically update the corresponding row in the database. You must explicitly call update methods (e.g., `ctx.Db.my_table.PrimaryKey.Update(modifiedInstance)`) to persist changes.
:::

:::danger `#[auto_inc]` + `#[unique]` Pitfall
Be cautious when manually inserting rows into a table that uses both `#[auto_inc]` and `#[unique]` on the same field. If you manually insert a row with a value for that field that is *larger* than the current internal sequence counter, the sequence will eventually increment to that manually inserted value. When it attempts to assign this value to a new row (inserted with 0), it will cause a unique constraint violation error (or panic with `insert()`). Avoid manually inserting values into auto-incrementing unique fields unless you fully understand the sequence behavior.
:::

```rust
use spacetimedb::{table, Identity, Timestamp, SpacetimeType};

// Assume Position, PlayerStatus, ItemType are defined as above

// NOTE: `name` uses an identifier, not a string.
#[table(name = player_state, public)]
// Define indexes directly in the table attribute or on fields
#[table(index(name = idx_level_btree, btree(columns = [level])))]
// NOTE: Do not derive SpacetimeType here, #[table] does it.
#[derive(Clone, Debug)]
pub struct PlayerState {
    #[primary_key]
    player_id: Identity,
    #[unique] // Player names must be unique
    name: String,
    conn_id: Option<ConnectionId>, // Store the connection ID when online
    health: u32,
    level: u16,
    position: Position,
    status: PlayerStatus,
    last_login: Option<Timestamp>, // Optional timestamp
}

// NOTE: `name` uses an identifier, not a string.
#[table(name = inventory_item, public)]
#[derive(Clone, Debug)] // No SpacetimeType derive needed
pub struct InventoryItem {
    #[primary_key]
    #[auto_inc] // Automatically generate unique IDs for items
    item_id: u64,
    owner_id: Identity,
    #[index(btree)] // Shorthand for single-column B-tree index on this field
    item_type: ItemType,
    quantity: u32,
}

// Example of a private table (not marked public)
// NOTE: `name` uses an identifier, not a string.
#[table(name = internal_game_data)]
#[derive(Clone, Debug)] // No SpacetimeType derive needed
struct InternalGameData {
    #[primary_key]
    key: String,
    value: String,
}
```

##### Multiple Tables from One Struct

:::caution Wrapper Struct Pattern Not Supported for This Use Case
Defining multiple tables using wrapper tuple structs (e.g., `struct ActiveCharacter(CharacterInfo);`) where field attributes like `#[primary_key]`, `#[unique]`, etc., are defined only on fields inside the inner struct (`CharacterInfo` in this example) is **not supported**. This pattern can lead to macro expansion issues and compilation errors because the `#[table]` macro applied to the wrapper struct cannot correctly process attributes defined within the inner type.
:::

**Recommended Pattern:** Apply multiple `#[table(...)]` attributes directly to the single struct definition that contains the necessary fields and field-level attributes (like `#[primary_key]`). This maps the same underlying type definition to multiple distinct tables reliably:

```rust
use spacetimedb::{table, Identity, Timestamp, Table}; // Added Table import

// Define the core data structure once
// Note: #[table] automatically derives SpacetimeType, Serialize, Deserialize
// Do NOT add #[derive(SpacetimeType)] here.
#[derive(Clone, Debug)]
#[table(name = logged_in_players, public)]  // Identifier name
#[table(name = players_in_lobby, public)]   // Identifier name
pub struct PlayerSessionData {
    #[primary_key]
    player_id: Identity,
    #[unique]
    #[auto_inc]
    session_id: u64,
    last_activity: Timestamp,
}

// Example Reducer demonstrating interaction
#[spacetimedb::reducer]
fn example_reducer(ctx: &spacetimedb::ReducerContext) {
    // Reducers interact with the specific table handles:
    let session = PlayerSessionData {
        player_id: ctx.sender, // Example: Use sender identity
        session_id: 0, // Assuming auto_inc
        last_activity: ctx.timestamp,
    };

    // Insert into the 'logged_in_players' table
    match ctx.db.logged_in_players().try_insert(session.clone()) {
        Ok(inserted) => spacetimedb::log::info!("Player {} logged in, session {}", inserted.player_id, inserted.session_id),
        Err(e) => spacetimedb::log::error!("Failed to insert into logged_in_players: {}", e),
    }

    // Find a player in the 'players_in_lobby' table by primary key
    if let Some(lobby_player) = ctx.db.players_in_lobby().player_id().find(&ctx.sender) {
        spacetimedb::log::info!("Player {} found in lobby.", lobby_player.player_id);
    }

    // Delete from the 'logged_in_players' table using the PK index
    ctx.db.logged_in_players().player_id().delete(&ctx.sender);
}
```

##### Browsing Generated Table APIs

The `#[table]` macro generates specific accessor methods based on your table definition (name, fields, indexes, constraints). To see the exact API generated for your tables:

1.  Run `cargo doc --open` in your module project directory.
2.  This compiles your code and opens the generated documentation in your web browser.
3.  Navigate to your module's documentation. You will find:
    *   The struct you defined (e.g., `PlayerState`).
    *   A generated struct representing the table handle (e.g., `player_state__TableHandle`), which implements `spacetimedb::Table` and contains methods for accessing indexes and unique columns.
    *   A generated trait (e.g., `player_state`) used to access the table handle via `ctx.db.{table_name}()`.

Reviewing this generated documentation is the best way to understand the specific methods available for interacting with your defined tables and their indexes.

#### Defining Reducers

Reducers are functions that modify table data atomically. They are annotated
with `#[reducer]`. 

:::info `use spacetimedb::Table;` Required for Table Operations
To call methods like `.insert()`, `.try_insert()`, `.update()`, `.delete()`, or access index/primary key handles (e.g., `.pk_field_name()`) on table handles returned by `ctx.db.table_name()`, you **must** bring the `spacetimedb::Table` trait into scope by adding `use spacetimedb::Table;` at the top of your `lib.rs` file. Without this import, the compiler will report errors like "method not found".
:::

:::info Transactionality
Crucially, **every reducer call executes within a single, atomic database transaction.** If the reducer function completes successfully (returns `()` or `Ok(())`), all database modifications made within it are committed together. If the reducer fails (panics or returns `Err(...)`), the transaction is aborted, and **all database changes made during that specific call are automatically rolled back**, ensuring data consistency.
:::

Reducers operate within a sandbox and have limitations:
*   They cannot directly perform network I/O (e.g., using `std::net`).
*   They cannot directly access the filesystem (e.g., using `std::fs` or `std::io`).
*   External communication happens primarily through database table modifications (which clients can subscribe to) and logging (`log` crate).

Reducers *can* call other reducers defined within the same module. This is a direct function call, not a network request, and executes within the same single database transaction (i.e., it does **not** start a sub-transaction).

```rust
use spacetimedb::{reducer, ReducerContext, Table, Identity, Timestamp, log};

#[table(name = user, public)]
#[derive(Clone, Debug)]
pub struct User { /* ... fields ... */ }
#[table(name = message, public)]
#[derive(Clone, Debug)]
pub struct Message { /* ... fields ... */ }

// Reducer to set a user's name
#[reducer]
pub fn set_name(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let name = validate_name(name)?; // Basic validation
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        // Update the user's name using the primary key index handle
        // Note: Update requires the full struct instance.
        ctx.db.user().identity().update(User { name: Some(name), ..user });
        Ok(())
    } else {
        Err("Cannot set name for unknown user".to_string())
    }
}

// Reducer to send a message
#[reducer]
pub fn send_message(ctx: &ReducerContext, text: String) -> Result<(), String> {
    let text = validate_message(text)?; // Basic validation
    log::info!("Received message: {}", text);
    // Insert the new message into the table.
    // insert() panics on constraint violation (e.g., duplicate PK).
    // Use try_insert() for Result-based error handling.
    ctx.db.message().insert(Message {
        sender: ctx.sender,
        text,
        sent: ctx.timestamp,
    });
    Ok(())
}

// Reducer called when a client connects
#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        // Mark existing user as online
        ctx.db.user().identity().update(User { online: true, ..user });
    } else {
        // Insert a new user record for the new connection
        ctx.db.user().insert(User {
            name: None,
            identity: ctx.sender,
            online: true,
        });
    }
}

// Reducer called when a client disconnects
#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        // Mark user as offline
        ctx.db.user().identity().update(User { online: false, ..user });
    } else {
        log::warn!("Disconnect event for unknown user: {:?}", ctx.sender);
    }
}

// Reducer called once when the module is loaded/database is created
#[reducer(init)]
pub fn initialize_database(ctx: &ReducerContext) {
    log::info!("Database Initializing! Module Identity: {}", ctx.identity());
    // Perform one-time setup, like inserting initial data if tables are empty
    if ctx.db.user().count() == 0 {
        // Add an admin user or default settings
    }
}

// Helper validation functions (example)
fn validate_name(name: String) -> Result<String, String> {
    if name.is_empty() { Err("Name cannot be empty".to_string()) } else { Ok(name) }
}

fn validate_message(text: String) -> Result<String, String> {
    if text.is_empty() { Err("Message cannot be empty".to_string()) } else { Ok(text) }
}

##### Error Handling: `Result` vs. Panic

Reducers can indicate failure either by returning `Err` from a function with a `Result` return type or by panicking (e.g., using `panic!`, `unwrap`, `expect`). Both methods trigger a transaction rollback, ensuring atomicity.

*   **Returning `Err(E)**:**
    *   This is generally preferred for handling *expected* or recoverable failures (e.g., invalid input, failed validation checks).
    *   The error value `E` (which must implement `Display`) is propagated back to the calling client and can be observed in the `ReducerEventContext` status.
    *   Crucially, returning `Err` does **not** destroy the underlying WebAssembly (WASM) instance.

*   **Panicking:**
    *   This typically represents an *unexpected* bug, violated invariant, or unrecoverable state (e.g., assertion failure, unexpected `None` value).
    *   The client **will** receive an error message derived from the panic payload (the argument provided to `panic!`, or the messages from `unwrap`/`expect`).
    *   Panicking does **not** cause the client to be disconnected.
    *   However, a panic **destroys the current WASM instance**. This means the *next* reducer call (from any client) that runs on this module will incur additional latency as SpacetimeDB needs to create and initialize a fresh WASM instance.

**Choosing between them:** While both ensure data consistency via rollback, returning `Result::Err` is generally better for predictable error conditions as it avoids the performance penalty associated with WASM instance recreation caused by panics. Use `panic!` for truly exceptional circumstances where state is considered unrecoverable or an unhandled bug is detected.

##### Lifecycle Reducers

Special reducers handle specific events:
*   `#[reducer(init)]`: Runs once when the module is first published **and** any time the database is manually cleared. Failure prevents publishing or clearing.
*   `#[reducer(client_connected)]`: Runs when any distinct client connection (e.g., WebSocket, HTTP call) is established. Failure disconnects the client. `ctx.connection_id` is guaranteed to be `Some(...)` within this reducer.
*   `#[reducer(client_disconnected)]`: Runs when any distinct client connection terminates. Failure is logged but does not prevent disconnection. `ctx.connection_id` is guaranteed to be `Some(...)` within this reducer.

These reducers cannot take arguments beyond `&ReducerContext`.

```rust
// Example init reducer was shown previously
#[reducer(client_connected)]
pub fn handle_connect(ctx: &ReducerContext) {
    log::info!("Client connected: {}", ctx.sender);
    // ... setup initial state for ctx.sender ...
}

#[reducer(client_disconnected)]
pub fn handle_disconnect(ctx: &ReducerContext) {
    log::info!("Client disconnected: {}", ctx.sender);
    // ... cleanup state for ctx.sender ...
}
```

#### Filtering and Deleting with Indexes

SpacetimeDB provides powerful ways to filter and delete table rows using B-tree indexes. The generated accessor methods accept various argument types:

*   **Single Value (Equality):** 
    *   For columns of type `String`, you can pass `&String` or `&str`.
    *   For columns of a type `T` that implements `Copy`, you can pass `&T` or an owned `T`.
    *   For other column types `T`, pass a reference `&T`.
*   **Ranges:** Use Rust's range syntax (`start..end`, `start..=end`, `..end`, `..=end`, `start..`). Values within the range can typically be owned or references.
*   **Multi-Column Indexes:** 
    *   To filter on an exact match for a *prefix* of the index columns, provide a tuple containing single values (following the rules above) for that prefix (e.g., `filter((val_a, val_b))` for an index on `[a, b, c]`).
    *   To filter using a range, you **must** provide single values for all preceding columns in the index, and the range can **only** be applied to the *last* column in your filter tuple (e.g., `filter((val_a, val_b, range_c))` is valid, but `filter((val_a, range_b, val_c))` or `filter((range_a, val_b))` are **not** valid tuple filters).
    *   Filtering or deleting using a range on *only the first column* of the index (without using a tuple) remains valid (e.g., `filter(range_a)`).

```rust
use spacetimedb::{table, reducer, ReducerContext, Table, log};

#[table(name = points, index(name = idx_xy, btree(columns = [x, y])))]
#[derive(Clone, Debug)]
pub struct Point { #[primary_key] id: u64, x: i64, y: i64 }
#[table(name = items, index(btree(columns = [name])))]
#[derive(Clone, Debug)] // No SpacetimeType derive
pub struct Item { #[primary_key] item_key: u32, name: String }

#[reducer]
fn index_operations(ctx: &ReducerContext) {
    // Example: Find items named "Sword" using the generated 'name' index handle
    // Passing &str for a String column is allowed.
    for item in ctx.db.items().name().filter("Sword") {
        // ...
    }

    // Example: Delete points where x is between 5 (inclusive) and 10 (exclusive)
    // using the multi-column index 'idx_xy' - filtering on first column range is OK.
    let num_deleted = ctx.db.points().idx_xy().delete(5i64..10i64);
    log::info!("Deleted {} points", num_deleted);

    // Example: Find points where x = 3 and y >= 0
    // using the multi-column index 'idx_xy' - (value, range) is OK.
    // Note: x is i64 which is Copy, so passing owned 3i64 is allowed.
    for point in ctx.db.points().idx_xy().filter((3i64, 0i64..)) {
        // ...
    }

    // Example: Find points where x > 5 and y = 1
    // This is INVALID: Cannot use range on non-last element of tuple filter.
    // for point in ctx.db.points().idx_xy().filter((5i64.., 1i64)) { ... }

    // Example: Delete all points where x = 7 (filtering on index prefix with single value)
    // using the multi-column index 'idx_xy'. Passing owned 7i64 is allowed (Copy type).
    ctx.db.points().idx_xy().delete(7i64);

    // Example: Delete a single item by its primary key 'item_key'
    // Use the PK field name as the method to get the PK index handle, then call delete.
    // item_key is u32 (Copy), passing owned value is allowed.
    let item_id_to_delete = 101u32;
    ctx.db.items().item_key().delete(item_id_to_delete);

    // Using references for a range filter on the first column - OK
    let min_x = 100i64;
    let max_x = 200i64;
    for point in ctx.db.points().idx_xy().filter(&min_x..=&max_x) {
         // ...
    }
}
```

##### Using `try_insert()`

Instead of `insert()`, which panics or throws if a constraint (like a primary key or unique index violation) occurs, Rust modules can use `try_insert()`. This method returns a `Result<RowType, spacetimedb::TryInsertError<TableHandleType>>`, allowing you to gracefully handle potential insertion failures without aborting the entire reducer transaction due to a panic. 

The `TryInsertError` enum provides specific variants detailing the cause of failure, such as `UniqueConstraintViolation` or `AutoIncOverflow`. These variants contain associated types specific to the table's constraints (e.g., `TableHandleType::UniqueConstraintViolation`). If a table lacks a certain constraint (like a unique index), the corresponding associated type might be uninhabited.

```rust
use spacetimedb::{table, reducer, ReducerContext, Table, log, TryInsertError};

#[table(name = items)]
#[derive(Clone, Debug)]
pub struct Item { 
    #[primary_key] #[auto_inc] id: u64, 
    #[unique] name: String 
}

#[reducer]
pub fn try_add_item(ctx: &ReducerContext, name: String) -> Result<(), String> {
    // Assume Item has an auto-incrementing primary key 'id' and a unique 'name'
    let new_item = Item { id: 0, name }; // Provide 0 for auto_inc

    // try_insert returns Result<Item, TryInsertError<items__TableHandle>>
    match ctx.db.items().try_insert(new_item) {
        Ok(inserted_item) => {
            // try_insert returns the inserted row (with assigned PK if auto_inc) on success
            log::info!("Successfully inserted item with ID: {}", inserted_item.id);
            Ok(())
        }
        Err(e) => {
            // Match on the specific TryInsertError variant
            match e {
                TryInsertError::UniqueConstraintViolation(constraint_error) => {
                    // constraint_error is of type items__TableHandle::UniqueConstraintViolation
                    // This type often provides details about the violated constraint.
                    // For simplicity, we just log a generic message here.
                    let error_msg = format!("Failed to insert item: Name '{}' already exists.", name);
                    log::error!("{}", error_msg);
                    // Return an error to the calling client
                    Err(error_msg)
                }
                TryInsertError::AutoIncOverflow(_) => {
                    // Handle potential overflow of the auto-incrementing key
                    let error_msg = "Failed to insert item: Auto-increment counter overflow.".to_string();
                    log::error!("{}", error_msg);
                    Err(error_msg)
                }
                // Use a wildcard for other potential errors or uninhabited variants
                _ => {
                    let error_msg = format!("Failed to insert item: Unknown constraint violation.");
                    log::error!("{}", error_msg);
                    Err(error_msg)
                }
            }
        }
    }
}

#### Scheduled Reducers (Rust)

Rust modules also support scheduled reducers. The mechanism involves defining a scheduling table similar to C#, but the specific annotations and API calls differ slightly.
The `spacetimedb::duration!` macro can be a convenient way to specify durations.

Refer to the [official Rust Module SDK documentation on docs.rs](https://docs.rs/spacetimedb/latest/spacetimedb/attr.reducer.html#scheduled-reducers) for detailed syntax and examples, including usage of the `duration!` macro (e.g., `duration!("5s").into()` to create a `ScheduleAt`).

##### Scheduled Reducer Details

*   **Best-Effort Scheduling:** Scheduled reducers run on a best-effort basis and may be slightly delayed under heavy database load.
*   **Security:** Since scheduled reducers can also be called directly by clients if not secured, it's crucial to verify the caller identity if the reducer performs sensitive operations:
    ```rust
    #[reducer]
    fn scheduled_task(ctx: &ReducerContext, args: MyScheduleTable) -> Result<(), String> {
        if ctx.sender != ctx.identity() {
            return Err("Permission denied: Task can only be run by scheduler.".into());
        }
        // ... proceed with scheduled logic ...
        Ok(())
    }
    ```

:::info Scheduled Reducers and Connections
Scheduled reducer calls originate from the SpacetimeDB scheduler itself, not from an external client connection. Therefore, within a scheduled reducer, `ctx.sender` will be the module's own identity, and `ctx.connection_id` will not represent an external client connection.
:::

#### Row-Level Security (Client Visibility Filters)

(Unstable Feature)

SpacetimeDB allows defining row-level security rules using the `#[spacetimedb::client_visibility_filter]` attribute. This attribute is applied to a `const` binding of type `Filter` and defines an SQL-like query that determines which rows of a table are visible to clients making subscription requests.

*   The query uses `:sender` to refer to the identity of the subscribing client.
*   Multiple filters on the same table are combined with `OR` logic.
*   Query errors (syntax, type errors, unknown tables) are reported during `spacetime publish`.

```rust
use spacetimedb::{client_visibility_filter, Filter, table, Identity};

#[table(name = "location_state")]
struct LocationState { #[primary_key] entity_id: u64, chunk_index: u32 }
#[table(name = "user_state")]
struct UserState { #[primary_key] identity: Identity, entity_id: u64 }

/// Players can only see entities located in the same chunk as their own entity.
#[client_visibility_filter]
const PLAYERS_SEE_ENTITIES_IN_SAME_CHUNK: Filter = Filter::Sql("
    SELECT * FROM LocationState WHERE chunk_index IN (
        SELECT chunk_index FROM LocationState WHERE entity_id IN (
            SELECT entity_id FROM UserState WHERE identity = :sender
        )
    )
");
```

:::info Version-Specific Status and Usage

*   **SpacetimeDB 1.0:** The Row-Level Security feature was not fully implemented or enforced in version 1.0. Modules developed for SpacetimeDB 1.0 should **not** use this feature.
*   **SpacetimeDB 1.1:** The feature is available but considered **unstable** in version 1.1. To use it, you must explicitly opt-in by enabling the `unstable` feature flag for the `spacetimedb` crate in your module's `Cargo.toml`:
    ```toml
    [dependencies]
    spacetimedb = { version = "1.1", features = ["unstable"] }
    # ... other dependencies
    ```
    Modules developed for 1.1 can use row-level security only if this feature flag is enabled.
:::

### Server Module (C#)

#### Defining Types

Custom classes, structs, or records used in tables or reducers must be marked with the `[Type]` attribute.
Use `partial` to allow code generation.
Tagged enums are represented using `TaggedEnum<(...)` with intermediate records defining variants.

```csharp
using SpacetimeDB;
using System.Collections.Generic;

[Type]
public partial struct Position { public int X; public int Y; }

// C# Tagged Enum Pattern using intermediate records:
[Type] public abstract partial record PlayerStatusBase { }
[Type] public partial record IdleStatus : PlayerStatusBase { }
[Type] public partial record WalkingStatus : PlayerStatusBase { public Position Target; }
[Type] public partial record FightingStatus : PlayerStatusBase { public Identity OpponentId; }

[Type]
public partial record PlayerStatus : TaggedEnum<(
    IdleStatus Idle,
    WalkingStatus Walking,
    FightingStatus Fighting
)> { }

[Type]
public enum ItemType { Weapon, Armor, Potion }

// Type aliases can be defined using the `using` directive
using PlayerScore = System.UInt32;

// Note: A [Sats(Name = "...")] attribute similar to Rust's for cross-language naming
// might not be fully supported or available in C# modules currently.
// Check latest SDK documentation for updates.
```

:::info C# `partial` Keyword
Table and Type definitions in C# should use the `partial` keyword (e.g., `public partial class MyTable`). This allows the SpacetimeDB source generator to add necessary internal methods and serialization logic to your types without requiring you to write boilerplate code.
:::

#### Defining Tables

Tables are defined using C# partial classes or structs annotated with `[Table]`.
Use `[PrimaryKey]`, `[AutoInc]`, `[Unique]`, and `[Index.BTree(...)]` attributes.
Use nullable types (`T?`) for optional fields.

:::caution Public Fields Required for Attributes
When using SpacetimeDB-specific attributes like `[PrimaryKey]`, `[AutoInc]`, `[Unique]`, or `[Index.BTree]` in C# table definitions, you **must** apply them to **public fields**, not C# properties (`{ get; set; }`). The SpacetimeDB code generator relies on direct field access for these attributes to function correctly. Using properties with these attributes can lead to build errors or unexpected runtime behavior.

Properties can generally be used for simple data fields that do not have these special SpacetimeDB attributes applied.
:::

:::caution Important: Public Tables
By default, tables are **private** and only accessible by server-side reducer code. If clients need to read or subscribe to a table's data, you **must** mark the table as `public` by setting `Public = true` in the `[Table]` attribute (e.g., `[Table(Name = "my_table", Public = true)]`).

*Common Pitfall:* If your client subscriptions fail with "table not found" or "not a valid table" errors, or if subscribed tables appear empty on the client despite having data on the server, double-check that the relevant tables have `Public = true` set in their `[Table]` attribute.
:::

:::caution Case Sensitivity
The `Name = "..."` specified in the `[Table(...)]` attribute is case-sensitive. When referring to this table in SQL queries (e.g., in client-side `subscribe` calls), you **must** use the exact same casing.

*Example:* If defined as `[Table(Name = "PlayerState")]`, querying `SELECT * FROM player_state` will fail. You must use `SELECT * FROM PlayerState`.
:::

:::caution Note on Modifying Instances
Instances of your table classes/structs are plain data objects. Modifying an instance **does not** automatically update the corresponding row in the database. You must explicitly call update methods (e.g., `ctx.Db.my_table.PrimaryKey.Update(modifiedInstance)`) to persist changes.
:::

:::danger `#[auto_inc]` + `#[unique]` Pitfall
Be cautious when manually inserting rows into a table that uses both `#[auto_inc]` and `#[unique]` on the same field. If you manually insert a row with a value for that field that is *larger* than the current internal sequence counter, the sequence will eventually increment to that manually inserted value. When it attempts to assign this value to a new row (inserted with 0), it will cause a unique constraint violation error (or panic with `insert()`). Avoid manually inserting values into auto-incrementing unique fields unless you fully understand the sequence behavior.
```

```csharp
using SpacetimeDB;
using System;

// Assume Position, PlayerStatus, ItemType are defined as above

[Table(Name = "player_state", Public = true)]
[Index.BTree(Name = "idx_level", Columns = new[] { nameof(Level) })] // Table-level index
public partial class PlayerState
{
    [PrimaryKey]
    public Identity PlayerId; // Field
    [Unique]
    public string Name = ""; // Field (initialize to avoid null warnings if Nullable enabled)
    public ConnectionId? ConnId; // Field
    public uint Health; // Field
    public ushort Level; // Field
    public Position Position; // Field
    public PlayerStatus Status; // Field
    public Timestamp? LastLogin; // Field
}

[Table(Name = "inventory_item", Public = true)]
public partial class InventoryItem
{
    [PrimaryKey]
    [AutoInc]
    public ulong ItemId; // Field
    public Identity OwnerId; // Field
    [Index.BTree] // Index on this field
    public ItemType ItemType; // Field
    public uint Quantity; // Field
}

// Example of a private table
[Table(Name = "internal_game_data")] // Public = false is default
public partial class InternalGameData
{
    [PrimaryKey]
    public string Key = ""; // Field
    public string Value = ""; // Field
}
```

##### Multiple Tables from One Class

You can use the same underlying data class for multiple tables, often using inheritance. Ensure SpacetimeDB attributes like `[PrimaryKey]` are applied to **public fields**, not properties.

```csharp
using SpacetimeDB;

// Define the core data structure (must be [Type] if used elsewhere)
[Type]
public partial class CharacterInfo
{
     [PrimaryKey]
     public ulong CharacterId; // Use public field
     public string Name = "";   // Use public field
     public ushort Level;      // Use public field
}

// Define derived classes, each with its own table attribute
[Table(Name = "active_characters")]
public partial class ActiveCharacter : CharacterInfo { 
    // Can add specific public fields if needed
    public bool IsOnline;
}

[Table(Name = "deleted_characters")]
public partial class DeletedCharacter : CharacterInfo { 
    // Can add specific public fields if needed
    public Timestamp DeletionTime;
}

// Reducers would interact with ActiveCharacter or DeletedCharacter tables
// E.g., ctx.Db.active_characters.Insert(new ActiveCharacter { CharacterId = 1, Name = "Hero", Level = 10, IsOnline = true });
```

Alternatively, you can define multiple `[Table]` attributes directly on a single class or struct. This maps the same underlying type to multiple distinct tables:

```csharp
using SpacetimeDB;

// Define the core data structure once
// Apply multiple [Table] attributes to map it to different tables
[Type] // Mark as a type if used elsewhere (e.g., reducer args)
[Table(Name = "logged_in_players", Public = true)]
[Table(Name = "players_in_lobby", Public = true)]
public partial class PlayerSessionData
{
    [PrimaryKey]
    public Identity PlayerId; // Use public field
    [Unique]
    [AutoInc]
    public ulong SessionId; // Use public field
    public Timestamp LastActivity;
}

// Reducers would interact with the specific table handles:
// E.g., ctx.Db.logged_in_players.Insert(new PlayerSessionData { ... });
// E.g., var lobbyPlayer = ctx.Db.players_in_lobby.PlayerId.Find(someId);
```

#### Defining Reducers

Reducers are static methods annotated with `[SpacetimeDB.Reducer]`. Lifecycle reducers use `ReducerKind`.

:::info Transactionality
Crucially, **every reducer call executes within a single, atomic database transaction.** If the reducer method completes successfully without throwing an unhandled exception, all database modifications made within it are committed together. If the reducer fails by throwing an unhandled exception, the transaction is aborted, and **all database changes made during that specific call are automatically rolled back**, ensuring data consistency.
:::

:::info Reducer Environment
*   **Sandbox:** Reducers run in a restricted environment. They cannot directly perform network I/O or access the local filesystem.
*   **External Interaction:** Communication with the outside world is done by modifying database tables (which clients can subscribe to) or through logging (`SpacetimeDB.Log`).
*   **Calling Other Reducers:** Reducers can call other static methods within the module, including other reducers. Such calls execute within the same database transaction.
:::

```csharp
using SpacetimeDB;
using System;
using System.Linq;

public static partial class Module
{
    // Assume PlayerState, InventoryItem tables and Position, PlayerStatus types are defined

    // Example Reducer showing various operations
    [Reducer]
    public static void UpdatePlayerData(ReducerContext ctx, string? newName)
    {
        var playerId = ctx.Sender;

        // 1. Find a player by primary key
        var player = ctx.Db.player_state.PlayerId.Find(playerId);
        if (player == null)
        {
            throw new Exception($"Player not found: {playerId}");
        }

        // 2. Update fields
        if (!string.IsNullOrWhiteSpace(newName))
        {
            // Check for uniqueness using the unique index accessor
            var existingPlayerWithNewName = ctx.Db.player_state.Name.Find(newName);
            if (existingPlayerWithNewName != null && existingPlayerWithNewName.PlayerId != playerId)
            {
                 throw new Exception($"Name already taken: {newName}");
            }
            player.Name = newName;
        }
        player.Level += 1;

        // 3. Update the row using the primary key index
        ctx.Db.player_state.PlayerId.Update(player);
        Log.Info($"Updated player data for {playerId}");
    }

    // Example: Handling Insert Exceptions
    [Reducer]
    public static void RegisterPlayer(ReducerContext ctx, string name)
    {
        if (string.IsNullOrWhiteSpace(name)) {
             throw new ArgumentException("Name cannot be empty.");
        }
        Log.Info($"Attempting to register player: {name}");

        // Check if player already exists (by PK or unique name)
        if (ctx.Db.player_state.PlayerId.Find(ctx.Sender) != null || ctx.Db.player_state.Name.Find(name) != null)
        {
             throw new Exception("Player already registered or name taken.");
        }

        var newPlayer = new PlayerState
        {
            PlayerId = ctx.Sender,
            Name = name,
            Health = 100,
            Level = 1,
            Position = new Position { X = 0, Y = 0 },
            Status = PlayerStatus.Idle(new IdleStatus()),
            LastLogin = ctx.Timestamp,
        };

        // Insert will throw an exception if constraints (PK, Unique) are violated
        // A try-catch block could handle this, but checking first is often cleaner.
        try {
            ctx.Db.player_state.Insert(newPlayer);
            Log.Info($"Player registered successfully: {ctx.Sender}");
        } catch (Exception ex) {
            // This might catch more than just constraint violations
            Log.Error($"Failed to register player {ctx.Sender}: {ex.Message}");
            throw; // Re-throw to ensure transaction rollback
        }
    }

    // Example: Filtering and Deleting with Indexes
    [Reducer]
    public static void CleanupLowLevelItems(ReducerContext ctx, ushort maxLevelToKeep)
    {
        var owner = ctx.Sender;
        var playerLevel = ctx.Db.player_state.PlayerId.Find(owner)?.Level ?? 0;

        if (playerLevel > maxLevelToKeep)
        {
            Log.Info($"Player level {playerLevel} exceeds threshold {maxLevelToKeep}. Cleaning up items...");

            // Get items owned by the player
            // Note: Filtering directly by OwnerId requires an index on that field.
            // If no index, iterate and filter manually.
            var itemsToCheck = ctx.Db.inventory_item.Iter()
                                    .Where(item => item.OwnerId == owner)
                                    // .Where(item => item.LevelRequirement < maxLevelToKeep) // Assuming LevelRequirement exists
                                    .ToList(); // Collect to avoid modifying while iterating

            uint deletedCount = 0;
            foreach (var item in itemsToCheck)
            {
                 // Add logic here based on ItemType or other properties if needed
                 Log.Info($"Deleting item ID: {item.ItemId} for owner {owner}");
                 // Delete using the primary key index accessor
                 if (ctx.Db.inventory_item.ItemId.Delete(item.ItemId))
                 {
                     deletedCount++;
                 }
            }
            Log.Info($"Deleted {deletedCount} low-level items for player {owner}");
        }
    }

    // Example: Interacting with a Private Table
    [Reducer]
    private static void UpdateInternalData(ReducerContext ctx, string key, string value)
    {
        // Example of a private helper reducer, possibly called by another reducer.
        // Assume InternalGameData table exists (defined as private)

        var data = ctx.Db.internal_game_data.Key.Find(key);
        if (data != null)
        {
            data.Value = value;
            ctx.Db.internal_game_data.Key.Update(data);
            Log.Info($"Updated internal key: {key}");
        }
        else
        {
            ctx.Db.internal_game_data.Insert(new InternalGameData { Key = key, Value = value });
            Log.Info($"Inserted internal key: {key}");
        }
    }

    // Example: Getting Table Row Count
    [Reducer]
    public static void CountPlayers(ReducerContext ctx)
    {
        var count = ctx.Db.player_state.Count; // Use the Count property
        Log.Info($"Current player count: {count}");
    }

    // Example: Timestamp/Duration Calculation
    [Reducer]
    public static void CheckLastLogin(ReducerContext ctx)
    {
        var player = ctx.Db.player_state.PlayerId.Find(ctx.Sender);
        if (player != null && player.LastLogin.HasValue)
        {
            TimeSpan? durationSinceLogin = ctx.Timestamp.TimeDurationSince(player.LastLogin.Value);
            if (durationSinceLogin.HasValue)
            {
                 Log.Info($"Player {ctx.Sender} last logged in {durationSinceLogin.Value} ago.");
            }
            else
            {
                 Log.Warn($"Player {ctx.Sender} last login time is in the future?");
            }
        }
        else if (player != null)
        {
             Log.Info($"Player {ctx.Sender} has no recorded login time.");
        }
    }

    // Example: Filtering and Deleting with Indexes
    [Reducer]
    public static void IndexOperations(ReducerContext ctx)
    {
        // Example: Find items named "Sword"
        var items = ctx.Db.items.name.Filter("Sword");
        foreach (var item in items) { /* ... */ }

        // Example: Delete points where x is 5
        bool deleted = ctx.Db.points.idx_xy.Delete(5L); // Filter on index prefix

        // Example: Find points where x = 3 and y = 7
        var specificPoint = ctx.Db.points.idx_xy.Filter((3L, 7L));
        foreach(var pt in specificPoint) { /* ... should be at most one */}

        // Example: Find points where x is between 100 and 200 (inclusive)
        var pointsInRange = ctx.Db.points.idx_xy.Filter((100L, 200L));
        foreach(var pt in pointsInRange) { /* ... */ }

        // Example: Delete items named "Shield"
        uint numDeleted = ctx.Db.items.name.Delete("Shield");
        Log.Info($"Deleted {numDeleted} Shield(s)");
    }
}
```

:::note C# `Insert` vs Rust `try_insert`
Unlike Rust, the C# SDK does not currently provide a `TryInsert` method that returns a result. The standard `Insert` method will throw an exception if a constraint (primary key, unique index) is violated. Therefore, C# reducers should typically check for potential constraint violations *before* calling `Insert`, or be prepared to handle the exception (which will likely roll back the transaction).
:::

##### Module Identity vs Sender Identity

Inside a reducer, the `ReducerContext` provides two important identities:

*   `ctx.Sender`: The [`Identity`](#identity) of the client who called the reducer, or the identity of the module itself if the reducer was called by the scheduler (for scheduled reducers) or internally by another reducer.
*   `ctx.Identity`: The [`Identity`](#identity) of the module (database) itself.
*   `ctx.Timestamp`: A [`Timestamp`](#timestamp) indicating when the reducer execution began.
*   `ctx.ConnectionId`: A [`ConnectionId`](#connectionid) representing the specific connection that invoked the reducer. This is a large (u128-based), randomly generated identifier **assigned by the SpacetimeDB server** for each distinct client connection (e.g., a WebSocket session or a stateless HTTP call). It is unique for the duration of that connection instance (from `client_connected` to `client_disconnected`). Unlike `ctx.Sender` (the authenticated [`Identity`](#identity)), the `ConnectionId` is **not verified** based on client input; it solely identifies the server-side connection object. Use `ConnectionId` for logic specific to a *transient connection* (e.g., tracking session state, rate limiting) and `Sender` (`Identity`) for logic tied to the *persistent, authenticated user account*.

This distinction is crucial for security, especially with scheduled reducers. You often want to ensure that only the scheduler (i.e., the module itself) can trigger certain actions.

##### Lifecycle Reducers

Special reducers handle specific events:
*   `[Reducer(ReducerKind.Init)]`: Runs once when the module is first published **and** any time the database is manually cleared. Failure prevents publishing or clearing.
*   `[Reducer(ReducerKind.ClientConnected)]`: Runs when any distinct client connection (e.g., WebSocket, HTTP call) is established. Failure disconnects the client. `ctx.ConnectionId` is guaranteed to have a value within this reducer.
*   `[Reducer(ReducerKind.ClientDisconnected)]`: Runs when any distinct client connection terminates. Failure is logged but does not prevent disconnection. `ctx.ConnectionId` is guaranteed to have a value within this reducer.

These reducers cannot take arguments beyond `ReducerContext`.

```csharp
// Example init reducer is shown in Scheduled Reducers section
[Reducer(ReducerKind.ClientConnected)]
public static void HandleConnect(ReducerContext ctx) {
    Log.Info($"Client connected: {ctx.Sender}");
    // ... setup initial state for ctx.sender ...
}

[Reducer(ReducerKind.ClientDisconnected)]
public static void HandleDisconnect(ReducerContext ctx) {
    Log.Info($"Client disconnected: {ctx.Sender}");
    // ... cleanup state for ctx.sender ...
}
```

#### Filtering and Deleting with Indexes

SpacetimeDB provides powerful ways to filter and delete table rows using B-tree indexes. The generated accessor methods accept various argument types:

*   **Single Value:** Pass a reference (`&T`) for the indexed column type.
*   **Ranges:** Use Rust's range syntax (`start..end`, `start..=end`, `..end`, `..=end`, `start..`). Values can be owned or references.
*   **Multi-Column Indexes:** Pass a tuple containing values or ranges for each indexed column. The types must match the column order in the index definition. You can filter on a prefix of the index columns.

```csharp
using SpacetimeDB;
using System;
using System.Linq;

public static partial class Module
{
    // Example: Filtering and Deleting with Indexes
    [Reducer]
    public static void IndexOperations(ReducerContext ctx)
    {
        // Example: Find items named "Sword"
        var items = ctx.Db.items.name.Filter("Sword");
        foreach (var item in items) { /* ... */ }

        // Example: Delete points where x is 5
        bool deleted = ctx.Db.points.idx_xy.Delete(5L); // Filter on index prefix

        // Example: Find points where x = 3 and y = 7
        var specificPoint = ctx.Db.points.idx_xy.Filter((3L, 7L));
        foreach(var pt in specificPoint) { /* ... should be at most one */}

        // Example: Find points where x is between 100 and 200 (inclusive)
        var pointsInRange = ctx.Db.points.idx_xy.Filter((100L, 200L));
        foreach(var pt in pointsInRange) { /* ... */ }

        // Example: Delete items named "Shield"
        uint numDeleted = ctx.Db.items.name.Delete("Shield");
        Log.Info($"Deleted {numDeleted} Shield(s)");
    }
}
```

:::note C# `Insert` vs Rust `try_insert`
Unlike Rust, the C# SDK does not currently provide a `TryInsert` method that returns a result. The standard `Insert` method will throw an exception if a constraint (primary key, unique index) is violated. Therefore, C# reducers should typically check for potential constraint violations *before* calling `Insert`, or be prepared to handle the exception (which will likely roll back the transaction).
:::

##### Scheduled Reducer Details

*   **Best-Effort Scheduling:** Scheduled reducers run on a best-effort basis and may be slightly delayed under heavy database load.
*   **Security:** Since scheduled reducers can also be called directly by clients if not secured, it's crucial to verify the caller identity if the reducer performs sensitive operations:
    ```csharp
    [Reducer]
    public static void ScheduledTask(ReducerContext ctx, GameTickSchedule args) // Use the actual schedule table type
    {
        if (!ctx.Sender.Equals(ctx.Identity))
        {
            throw new Exception("Permission denied: Task can only be run by scheduler.");
        }
        // ... proceed with scheduled logic ...
        Log.Info($"Executing scheduled task for tick {args.TickNumber}");
    }
    ```

##### Error Handling: Exceptions

Throwing an unhandled exception within a C# reducer will cause the transaction to roll back.
*   **Expected Failures:** For predictable errors (e.g., invalid arguments, state violations), explicitly `throw` an `Exception`. The exception message can often be observed by the client in the `ReducerEventContext` status (though the exact behavior might vary).
*   **Unexpected Errors:** Unhandled runtime exceptions (e.g., `NullReferenceException`) also cause rollbacks but might provide less informative feedback to the client, potentially just indicating a general failure.

It's generally good practice to validate input and state early in the reducer and `throw` specific exceptions for handled error conditions.

### Client SDK (Rust)

fn on_message_sent_result(ctx: &ReducerEventContext, text: &String) {
    if let Status::Failed(err) = &ctx.event.status {
        eprintln!("[Error] Failed to send message '{}': {}", text, err);
    }
}

:::info Handling Initial Data vs. Live Updates in Callbacks
Callbacks like `on_insert` and `on_update` are triggered for both the initial data received when a subscription is first applied *and* for subsequent live changes caused by reducers. If you need to differentiate (e.g., only react to *new* messages, not the backlog), you can inspect the `ctx.event` type. For example, `if let Event::Reducer(_) = ctx.event { ... }` checks if the change came from a reducer call.
:::

:::info Handling Initial Data vs. Live Updates in Callbacks
Callbacks like `OnInsert` and `OnUpdate` are triggered for both the initial data received when a subscription is first applied *and* for subsequent live changes caused by reducers. If you need to differentiate (e.g., only react to *new* messages, not the backlog), you can inspect the `ctx.Event` type. For example, checking `if (ctx.Event is not Event<Reducer>.SubscribeApplied) { ... }` ensures the code only runs for events triggered by reducers, not the initial subscription data load.
:::