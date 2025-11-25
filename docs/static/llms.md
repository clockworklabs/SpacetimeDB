# SpacetimeDB

> SpacetimeDB is a fully-featured relational database system that integrates
> application logic directly within the database, eliminating the need for
> separate web or game servers. It supports multiple programming languages,
> including C# and Rust, allowing developers to write and deploy entire
> applications as a single binary. It is optimized for high-throughput and low
> latency multiplayer applications like multiplayer games.

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
Byte Index: |  0  |  1  |  2  |  3  |  4  |  5  |  6  |  7  | ... | 31  |
            +-----+-----+-----+-----+-----+-----+-----+-----+-----+-----+
Contents:   | 0xc2| 0x00| Checksum Hash (4 bytes) |  ID Hash (26 bytes) |
            +-----+-----+-------------------------+---------------------+
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
    - **macOS:**
      ```bash
      curl -sSf https://install.spacetimedb.com | sh
      ```
    - **Windows (PowerShell):**
      ```powershell
      iwr https://windows.spacetimedb.com -useb | iex
      ```
    - **Linux:**
      ```bash
      curl -sSf https://install.spacetimedb.com | sh
      ```
    - **Docker (to run the server):**
      ```bash
      # This command starts a SpacetimeDB server instance in Docker
      docker run --rm --pull always -p 3000:3000 clockworklabs/spacetime start
      # Note: While the CLI can be installed separately (see above), you can also execute
      # CLI commands *within* the running Docker container (e.g., using `docker exec`)
      # or use the image as a base for a custom image containing your module management tools.
      ```
    - **Docker (to execute CLI commands directly):**
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

    - For more details or troubleshooting, see the official [Getting Started Guide](https://spacetimedb.com/docs/getting-started) and [Installation Page](https://spacetimedb.com/install).

1.b **Log In (If Necessary):** If you plan to publish to a server that requires authentication (like the public Maincloud at `maincloud.spacetimedb.com`), you generally need to log in first using `spacetime login`. This associates your actions with your global SpacetimeDB identity (e.g., linked to your spacetimedb.com account).
`bash
    spacetime login
    # Follow the prompts to authenticate via web browser
    `
If you attempt commands like `publish` against an authenticated server without being logged in, the CLI will prompt you: `You are not logged in. Would you like to log in with spacetimedb.com? [y/N]`.
_ Choosing `y` initiates the standard browser login flow.
_ Choosing `n` proceeds without a global login for this operation. The CLI will confirm `We have logged in directly to your target server. WARNING: This login will NOT work for any other servers.` This uses or creates a server-issued identity specific to that server (see Step 5).

    In general, using `spacetime login` (which authenticates via spacetimedb.com) is recommended, as the resulting identities are portable across different SpacetimeDB servers.

2.  **Initialize Server Module:** Create a new directory for your project and use the CLI to initialize the server module structure:
    ```bash
    # For Rust
    spacetime init --lang rust --project-path my_server_module my-server-module
    # For C#
    spacetime init --lang csharp --project-path my_server_module my-server-module
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
    - Providing a `[name|identity]` for the database is **optional**. If omitted, a nameless database will be created and assigned a unique `Identity` automatically. If providing a _name_, it must match the regex `^[a-z0-9]+(-[a-z0-9]+)*$`.
    - By default (`--project-path`), it builds the module before publishing. Use `--bin-path <wasm_file>` to publish a pre-compiled WASM instead.
    - Use `-s, --server <server>` to specify the target instance (e.g., `maincloud.spacetimedb.com` or the nickname `maincloud`). If omitted, it targets a local instance or uses your configured default (check with `spacetime server list`).
    - Use `-c, --delete-data` when updating an existing database identity to destroy all existing data first.

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
    - Connect to the database (`my-database-name`).
    - Subscribe to data in public tables.
    - Register callbacks to react to data changes.
    - Call reducers defined in your module.
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

### Reducer Context: Understanding Identities and Execution Information

When a reducer function executes, it is provided with a **Reducer Context**. This context contains vital information about the call's origin and environment, crucial for logic, especially security checks. Key pieces of information typically available within the context include:

- **Sender Identity**: The authenticated [`Identity`](#identity) of the entity that invoked the reducer. This could be:
  - A client application connected to the database.
  - The module itself, if the reducer was triggered by the internal scheduler (for scheduled reducers).
  - The module itself, if the reducer was called internally by another reducer function within the same module.
- **Module Identity**: The authenticated [`Identity`](#identity) representing the database (module) itself. This is useful for checks where an action should only be performed by the module (e.g., in scheduled reducers).
- **Database Access**: Handles or interfaces for interacting with the database tables defined in the module. This allows the reducer to perform operations like inserting, updating, deleting, and querying rows based on primary keys or indexes.
- **Timestamp**: A [`Timestamp`](#timestamp) indicating precisely when the current reducer execution began.
- **Connection ID**: A [`ConnectionId`](#connectionid) representing the specific network connection instance (like a WebSocket session or a stateless HTTP request) that invoked the reducer. This is a unique, server-assigned identifier that persists only for the duration of that connection (from connection start to disconnect).
  - **Important Distinction**: Unlike the **Sender Identity** (which represents the _authenticated user or module_), the **Connection ID** solely identifies the _transient network session_. It is assigned by the server and is not based on client-provided authentication credentials. Use the Connection ID for logic tied to a specific connection instance (e.g., tracking session state, rate limiting per connection), and use the Sender Identity for logic related to the persistent, authenticated user or the module itself.

Understanding the difference between the **Sender Identity** and the **Module Identity** is particularly important for security. For example, when writing scheduled reducers, you often need to verify that the **Sender Identity** matches the **Module Identity** to ensure the action wasn't improperly triggered by an external client.

### Server Module (Rust)

#### Defining Types

Custom structs or enums intended for use as fields within database tables or as parameters/return types in reducers must derive `SpacetimeType`. This derivation enables SpacetimeDB to handle the serialization and deserialization of these types.

- **Basic Usage:** Apply `#[derive(SpacetimeType, ...)]` to your structs and enums. Other common derives like `Clone`, `Debug`, `PartialEq` are often useful.
- **Cross-Language Naming:** Use the `#[sats(name = "Namespace.TypeName")]` attribute _on the type definition_ to explicitly control the name exposed in generated client bindings (e.g., for C# or TypeScript). This helps prevent naming collisions and provides better organization. You can also use `#[sats(name = "VariantName")]` _on enum variants_ to control their generated names.
- **Type Aliases:** Standard Rust `pub type` aliases can be used for clarity (e.g., `pub type PlayerScore = u32;`). The underlying primitive type must still be serializable by SpacetimeDB.
- **Advanced Deserialization:** For types with complex requirements (like lifetimes or custom binary representations), you might need manual implementation using `spacetimedb::Deserialize` and the `bsatn` crate (available via `spacetimedb::spacetimedb_lib`), though this is uncommon for typical application types.

```rust
use spacetimedb::{SpacetimeType, Identity, Timestamp};

// Example Struct
#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

// Example Enum
#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub enum PlayerStatus {
    Idle,
    Walking(Position),
    Fighting(Identity), // Store the identity of the opponent
}

// Example Enum with Cross-Language Naming Control
// This enum will appear as `Game.ItemType` in C# bindings.
#[derive(SpacetimeType, Clone, Debug, PartialEq)]
#[sats(name = "Game.ItemType")]
pub enum ItemType {
    Weapon,
    Armor,
    // This specific variant will be `ConsumableItem` in C# bindings.
    #[sats(name = "ConsumableItem")]
    Potion,
}

// Example Type Alias
pub type PlayerScore = u32;

// Advanced: For types with lifetimes or custom binary representations,
// you can derive `spacetimedb::Deserialize` and use the `bsatn` crate
// (provided by spacetimedb::spacetimedb_lib) for manual deserialization if needed.
```

:::info Rust `crate-type = ["cdylib"]`
The `[lib]` section in your module's `Cargo.toml` must contain `crate-type = ["cdylib"]`. This tells the Rust compiler to produce a dynamic system library compatible with the C ABI, which allows the SpacetimeDB host (written in Rust) to load and interact with your compiled WebAssembly module.
:::

#### Defining Tables

Database tables store the application's persistent state. They are defined using Rust structs annotated with the `#[table]` macro.

- **Core Attribute:** `#[table(name = my_table_name, ...)]` marks a struct as a database table definition. The specified `name` (an identifier, _not_ a string literal) is how the table will be referenced in SQL queries and generated APIs.
- **Derivations:** The `#[table]` macro automatically handles deriving necessary traits like `SpacetimeType`, `Serialize`, `Deserialize`, and `Debug`. **Do not** manually add `#[derive(SpacetimeType)]` to a `#[table]` struct, as it will cause compilation conflicts.
- **Public vs. Private:** By default, tables are **private**, accessible only by server-side reducer code. To allow clients to read or subscribe to a table's data, mark it as `public` using `#[table(..., public)]`. This is a common source of errors if forgotten.
- **Primary Keys:** Designate a single field as the primary key using `#[primary_key]`. This ensures uniqueness, creates an efficient index, and allows clients to track row updates.
- **Auto-Increment:** Mark an integer-typed primary key field with `#[auto_inc]` to have SpacetimeDB automatically assign unique, sequentially increasing values upon insertion. Provide `0` as the value for this field when inserting a new row to trigger the auto-increment mechanism.
- **Unique Constraints:** Enforce uniqueness on non-primary key fields using `#[unique]`. Attempts to insert or update rows violating this constraint will fail.
- **Indexes:** Create B-tree indexes for faster lookups on specific fields or combinations of fields. Use `#[index(btree)]` on a single field for a simple index, or `#[table(index(name = my_index_name, btree(columns = [col_a, col_b])))])` within the `#[table(...)]` attribute for named, multi-column indexes.
- **Nullable Fields:** Use standard Rust `Option<T>` for fields that can hold null values.
- **Instances vs. Database:** Remember that table struct instances (e.g., `let player = PlayerState { ... };`) are just data. Modifying an instance does **not** automatically update the database. Interaction happens through generated handles accessed via the `ReducerContext` (e.g., `ctx.db.player_state().insert(...)`).
- **Case Sensitivity:** Table names specified via `name = ...` are case-sensitive and must be matched exactly in SQL queries.
- **Pitfalls:**
  - Avoid manually inserting values into `#[auto_inc]` fields that are also `#[unique]`, especially values larger than the current sequence counter, as this can lead to future unique constraint violations when the counter catches up.
  - Ensure `public` is set if clients need access.
  - Do not manually derive `SpacetimeType`.
  - Define indexes _within_ the main `#[table(name=..., index=...)]` attribute. Each `#[table]` macro invocation defines a _distinct_ table and requires a `name`; separate `#[table]` attributes cannot be used solely to add indexes to a previously named table.

```rust
use spacetimedb::{table, Identity, Timestamp, SpacetimeType, Table}; // Added Table import

// Assume Position, PlayerStatus, ItemType are defined as types

// Example Table Definition
#[table(
    name = player_state,
    public,
    // Index definition is included here
    index(name = idx_level_btree, btree(columns = [level]))
)]
#[derive(Clone, Debug)] // No SpacetimeType needed here
pub struct PlayerState {
    #[primary_key]
    player_id: Identity,
    #[unique] // Player names must be unique
    name: String,
    conn_id: Option<ConnectionId>, // Nullable field
    health: u32,
    level: u16,
    position: Position, // Custom type field
    status: PlayerStatus, // Custom enum field
    last_login: Option<Timestamp>, // Nullable timestamp
}

#[table(name = inventory_item, public)]
#[derive(Clone, Debug)]
pub struct InventoryItem {
    #[primary_key]
    #[auto_inc] // Automatically generate IDs
    item_id: u64,
    owner_id: Identity,
    #[index(btree)] // Simple index on this field
    item_type: ItemType,
    quantity: u32,
}

// Example of a private table
#[table(name = internal_game_data)] // No `public` flag
#[derive(Clone, Debug)]
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
    - The struct you defined (e.g., `PlayerState`).
    - A generated struct representing the table handle (e.g., `player_state__TableHandle`), which implements `spacetimedb::Table` and contains methods for accessing indexes and unique columns.
    - A generated trait (e.g., `player_state`) used to access the table handle via `ctx.db.{table_name}()`.

Reviewing this generated documentation is the best way to understand the specific methods available for interacting with your defined tables and their indexes.

#### Defining Reducers

Reducers are the functions within your server module responsible for atomically modifying the database state in response to client requests or internal events (like lifecycle triggers or schedules).

- **Core Attribute:** Reducers are defined as standard Rust functions annotated with `#[reducer]`.
- **Signature:** Every reducer function must accept `&ReducerContext` as its first argument. Subsequent arguments represent data passed from the client caller or scheduler, and their types must derive `SpacetimeType`.
- **Return Type:** Reducers typically return `()` for success or `Result<(), E>` (where `E: Display`) to signal recoverable errors.
- **Necessary Imports:** To perform table operations (insert, update, delete, query indexes), the `spacetimedb::Table` trait must be in scope. Add `use spacetimedb::Table;` to the top of your `lib.rs`.
- **Reducer Context:** The `ReducerContext` (`ctx`) provides access to:
  - `ctx.db`: Handles for interacting with database tables.
  - `ctx.sender`: The `Identity` of the caller.
  - `ctx.identity`: The `Identity` of the module itself.
  - `ctx.timestamp`: The `Timestamp` of the invocation.
  - `ctx.connection_id`: The optional `ConnectionId` of the caller.
  - `ctx.rng`: A source for deterministic random number generation (if needed).
- **Transactionality:** Each reducer call executes within a single, atomic database transaction. If the function returns `()` or `Ok(())`, all database changes are committed. If it returns `Err(...)` or panics, the transaction is aborted, and **all changes are rolled back**, preserving data integrity.
- **Execution Environment:** Reducers run in a sandbox and **cannot** directly perform network I/O (`std::net`) or filesystem operations (`std::fs`, `std::io`). External interaction primarily occurs through database table modifications (observed by clients) and logging (`spacetimedb::log`).
- **Calling Other Reducers:** A reducer can directly call another reducer defined in the same module. This is a standard function call and executes within the _same_ transaction; it does not create a sub-transaction.

```rust
use spacetimedb::{reducer, ReducerContext, Table, Identity, Timestamp, log};

// Assume User and Message tables are defined as previously
#[table(name = user, public)]
#[derive(Clone, Debug)] pub struct User { #[primary_key] identity: Identity, name: Option<String>, online: bool }
#[table(name = message, public)]
#[derive(Clone, Debug)] pub struct Message { #[primary_key] #[auto_inc] id: u64, sender: Identity, text: String, sent: Timestamp }

// Example: Basic reducer to set a user's name
#[reducer]
pub fn set_name(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let sender_id = ctx.sender;
    let name = validate_name(name)?; // Use helper for validation

    // Find the user row by primary key
    if let Some(mut user) = ctx.db.user().identity().find(&sender_id) {
        // Update the field
        user.name = Some(name);
        // Persist the change using the PK index update method
        ctx.db.user().identity().update(user);
        log::info!("User {} set name", sender_id);
        Ok(())
    } else {
        Err(format!("User not found: {}", sender_id))
    }
}

// Example: Basic reducer to send a message
#[reducer]
pub fn send_message(ctx: &ReducerContext, text: String) -> Result<(), String> {
    let text = validate_message(text)?; // Use helper for validation
    log::info!("User {} sent message: {}", ctx.sender, text);

    // Insert a new row into the Message table
    // Note: id is auto_inc, so we provide 0. insert() panics on constraint violation.
    let new_message = Message {
        id: 0,
        sender: ctx.sender,
        text,
        sent: ctx.timestamp,
    };
    ctx.db.message().insert(new_message);
    // For Result-based error handling on insert, use try_insert() - see below

    Ok(())
}

// Helper validation functions (example)
fn validate_name(name: String) -> Result<String, String> {
    if name.is_empty() { Err("Name cannot be empty".to_string()) } else { Ok(name) }
}

fn validate_message(text: String) -> Result<String, String> {
    if text.is_empty() { Err("Message cannot be empty".to_string()) } else { Ok(text) }
}
```

##### Error Handling: `Result` vs. Panic

Reducers can indicate failure either by returning `Err` from a function with a `Result` return type or by panicking (e.g., using `panic!`, `unwrap`, `expect`). Both methods trigger a transaction rollback, ensuring atomicity.

- **Returning `Err(E)**:\*\*
  - This is generally preferred for handling _expected_ or recoverable failures (e.g., invalid input, failed validation checks).
  - The error value `E` (which must implement `Display`) is propagated back to the calling client and can be observed in the `ReducerEventContext` status.
  - Crucially, returning `Err` does **not** destroy the underlying WebAssembly (WASM) instance.

- **Panicking:**
  - This typically represents an _unexpected_ bug, violated invariant, or unrecoverable state (e.g., assertion failure, unexpected `None` value).
  - The client **will** receive an error message derived from the panic payload (the argument provided to `panic!`, or the messages from `unwrap`/`expect`).
  - Panicking does **not** cause the client to be disconnected.
  - However, a panic **destroys the current WASM instance**. This means the _next_ reducer call (from any client) that runs on this module will incur additional latency as SpacetimeDB needs to create and initialize a fresh WASM instance.

**Choosing between them:** While both ensure data consistency via rollback, returning `Result::Err` is generally better for predictable error conditions as it avoids the performance penalty associated with WASM instance recreation caused by panics. Use `panic!` for truly exceptional circumstances where state is considered unrecoverable or an unhandled bug is detected.

##### Lifecycle Reducers

Special reducers handle specific events:

- `#[reducer(init)]`: Runs once when the module is first published **and** any time the database is manually cleared (e.g., via `spacetime publish -c` or `spacetime server clear`). Failure prevents publishing or clearing. Often used for initial data setup.
- `#[reducer(client_connected)]`: Runs when any distinct client connection (e.g., WebSocket, HTTP call) is established. Failure disconnects the client. `ctx.connection_id` is guaranteed to be `Some(...)` within this reducer.
- `#[reducer(client_disconnected)]`: Runs when any distinct client connection terminates. Failure is logged but does not prevent disconnection. `ctx.connection_id` is guaranteed to be `Some(...)` within this reducer.

These reducers cannot take arguments beyond `&ReducerContext`.

```rust
use spacetimedb::{reducer, table, ReducerContext, Table, log};

#[table(name = settings)]
#[derive(Clone, Debug)]
pub struct Settings {
    #[primary_key]
    key: String,
    value: String,
}

// Example init reducer: Insert default settings if the table is empty
#[reducer(init)]
pub fn initialize_database(ctx: &ReducerContext) {
    log::info!(
        "Database Initializing! Module Identity: {}, Timestamp: {}",
        ctx.identity(),
        ctx.timestamp
    );
    // Check if settings table is empty
    if ctx.db.settings().count() == 0 {
        log::info!("Settings table is empty, inserting default values...");
        // Insert default settings
        ctx.db.settings().insert(Settings {
            key: "welcome_message".to_string(),
            value: "Hello from SpacetimeDB!".to_string(),
        });
        ctx.db.settings().insert(Settings {
            key: "default_score".to_string(),
            value: "0".to_string(),
        });
    } else {
        log::info!("Settings table already contains data.");
    }
}

// Example client_connected reducer
#[reducer(client_connected)]
pub fn handle_connect(ctx: &ReducerContext) {
    log::info!("Client connected: {}, Connection ID: {:?}", ctx.sender, ctx.connection_id);
    // ... setup initial state for ctx.sender ...
}

// Example client_disconnected reducer
#[reducer(client_disconnected)]
pub fn handle_disconnect(ctx: &ReducerContext) {
    log::info!("Client disconnected: {}, Connection ID: {:?}", ctx.sender, ctx.connection_id);
    // ... cleanup state for ctx.sender ...
}
```

##### Filtering and Deleting with Indexes

SpacetimeDB provides powerful ways to filter and delete table rows using B-tree indexes. The generated accessor methods accept various argument types:

- **Single Value (Equality):**
  - For columns of type `String`, you can pass `&String` or `&str`.
  - For columns of a type `T` that implements `Copy`, you can pass `&T` or an owned `T`.
  - For other column types `T`, pass a reference `&T`.
- **Ranges:** Use Rust's range syntax (`start..end`, `start..=end`, `..end`, `..=end`, `start..`). Values within the range can typically be owned or references.
- **Multi-Column Indexes:**
  - To filter on an exact match for a _prefix_ of the index columns, provide a tuple containing single values (following the rules above) for that prefix (e.g., `filter((val_a, val_b))` for an index on `[a, b, c]`).
  - To filter using a range, you **must** provide single values for all preceding columns in the index, and the range can **only** be applied to the _last_ column in your filter tuple (e.g., `filter((val_a, val_b, range_c))` is valid, but `filter((val_a, range_b, val_c))` or `filter((range_a, val_b))` are **not** valid tuple filters).
  - Filtering or deleting using a range on _only the first column_ of the index (without using a tuple) remains valid (e.g., `filter(range_a)`).

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

````rust
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

In addition to lifecycle annotations, reducers can be scheduled. This allows calling the reducers at a particular time, or in a loop. This can be used for game loops.

The scheduling information for a reducer is stored in a table. This table has two mandatory fields:

*   A primary key that identifies scheduled reducer calls (often using `#[auto_inc]`).
*   A field of type `spacetimedb::ScheduleAt` that says when to call the reducer.

The table definition itself links to the reducer function using the `scheduled(reducer_function_name)` parameter within the `#[table(...)]` attribute.

Managing timers with a scheduled table is as simple as inserting or deleting rows from the table. This makes scheduling transactional in SpacetimeDB. If a reducer A first schedules B but then errors for some other reason, B will not be scheduled to run.

A `ScheduleAt` value can be created using `.into()` from:

*   A `spacetimedb::Timestamp`: Schedules the reducer to run **once** at that specific time.
*   A `spacetimedb::TimeDuration` or `std::time::Duration`: Schedules the reducer to run **periodically** with that duration as the interval.

The scheduled reducer function itself is defined like a normal reducer (`#[reducer]`), taking `&ReducerContext` and an instance of the schedule table struct as arguments.

```rust
use spacetimedb::{table, reducer, ReducerContext, Timestamp, TimeDuration, ScheduleAt, Table};
use log::debug;

// 1. Declare the table with scheduling information, linking it to `send_message`.
#[table(name = send_message_schedule, scheduled(send_message))]
struct SendMessageSchedule {
    // Mandatory fields:
    // ============================

    /// An identifier for the scheduled reducer call.
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,

    /// Information about when the reducer should be called.
    scheduled_at: ScheduleAt,

    // In addition to the mandatory fields, any number of fields can be added.
    // These can be used to provide extra information to the scheduled reducer.

    // Custom fields:
    // ============================

    /// The text of the scheduled message to send.
    text: String,
}

// 2. Declare the scheduled reducer.
// The second argument is a row of the scheduling information table.
#[reducer]
fn send_message(ctx: &ReducerContext, args: SendMessageSchedule) -> Result<(), String> {
    // Security check is important!
    if ctx.sender != ctx.identity() {
        return Err("Reducer `send_message` may not be invoked by clients, only via scheduling.".into());
    }

    let message_to_send = &args.text;
    log::info!("Scheduled SendMessage: {}", message_to_send);

    // ... potentially send the message or perform other actions ...

    Ok(())
}

// 3. Example of scheduling reducers (e.g., in init)
#[reducer(init)]
fn init(ctx: &ReducerContext) -> Result<(), String> {

    let current_time = ctx.timestamp;
    let ten_seconds = TimeDuration::from_micros(10_000_000);
    let future_timestamp: Timestamp = ctx.timestamp + ten_seconds;

    // Schedule a one-off message
    ctx.db.send_message_schedule().insert(SendMessageSchedule {
        scheduled_id: 0, // Use 0 for auto_inc
        text: "I'm a bot sending a message one time".to_string(),
        // Creating a `ScheduleAt` from a `Timestamp` results in the reducer
        // being called once, at exactly the time `future_timestamp`.
        scheduled_at: future_timestamp.into()
    });
    log::info!("Scheduled one-off message.");

    // Schedule a periodic message (every 10 seconds)
    let loop_duration: TimeDuration = ten_seconds;
    ctx.db.send_message_schedule().insert(SendMessageSchedule {
        scheduled_id: 0, // Use 0 for auto_inc
        text: "I'm a bot sending a message every 10 seconds".to_string(),
        // Creating a `ScheduleAt` from a `Duration`/`TimeDuration` results in the reducer
        // being called in a loop, once every `loop_duration`.
        scheduled_at: loop_duration.into()
    });
    log::info!("Scheduled periodic message.");

    Ok(())
}
````

Refer to the [official Rust Module SDK documentation on docs.rs](https://docs.rs/spacetimedb/latest/spacetimedb/attr.reducer.html#scheduled-reducers) for more detailed syntax and alternative scheduling approaches (like using `schedule::periodic`).

##### Scheduled Reducer Details

- **Best-Effort Scheduling:** Scheduled reducers are called on a best-effort basis and may be slightly delayed in their execution when a database is under heavy load.

- **Restricting Access (Security):** Scheduled reducers are normal reducers and _can_ still be called directly by clients. If a scheduled reducer should _only_ be called by the scheduler, it is crucial to begin the reducer with a check comparing the caller's identity (`ctx.sender`) to the module's own identity (`ctx.identity()`).

  ```rust
  use spacetimedb::{reducer, ReducerContext};
  // Assuming MyScheduleArgs table is defined
  struct MyScheduleArgs {/*...*/}

  #[reducer]
  fn my_scheduled_reducer(ctx: &ReducerContext, args: MyScheduleArgs) -> Result<(), String> {
      if ctx.sender != ctx.identity() {
          return Err("Reducer `my_scheduled_reducer` may not be invoked by clients, only via scheduling.".into());
      }
      // ... Reducer body proceeds only if called by scheduler ...
      Ok(())
  }
  ```

:::info Scheduled Reducers and Connections
Scheduled reducer calls originate from the SpacetimeDB scheduler itself, not from an external client connection. Therefore, within a scheduled reducer, `ctx.sender` will be the module's own identity, and `ctx.connection_id` will be `None`.
:::

#### Row-Level Security (RLS)

Row Level Security (RLS) allows module authors to restrict client access to specific rows
of tables that are marked as `public`. By default, tables _without_ the `public`
attribute are private and completely inaccessible to clients. Tables _with_ the `public`
attribute are, by default, fully visible to any client that subscribes to them. RLS provides
a mechanism to selectively restrict access to certain rows of these `public` tables based
on rules evaluated for each client.

Private tables (those _without_ the `public` attribute) are always completely inaccessible
to clients, and RLS rules do not apply to them. RLS rules are defined for `public` tables
to filter which rows are visible.

These access-granting rules are expressed in SQL and evaluated automatically for queries
and subscriptions made by clients against private tables with associated RLS rules.

:::info Version-Specific Status
Row-Level Security (RLS) was introduced as an **unstable** feature in **SpacetimeDB v1.1.0**.
It requires explicit opt-in via feature flags or pragmas.
:::

**Enabling RLS**

RLS is currently **unstable** and must be explicitly enabled in your module.

To enable RLS, activate the `unstable` feature in your project's `Cargo.toml`:

```toml
spacetimedb = { version = "1.1.0", features = ["unstable"] } # at least version 1.1.0
```

**How It Works**

RLS rules are attached to `public` tables (tables with `#[table(..., public)]`)
and are expressed in SQL using constants of type `Filter`.

```rust
use spacetimedb::{client_visibility_filter, Filter, table, Identity};

// Define a public table for RLS
#[table(name = account, public)] // Now a public table
struct Account {
    #[primary_key]
    identity: Identity,
    email: String,
    balance: u32,
}

/// RLS Rule: Allow a client to see *only* their own account record.
#[client_visibility_filter]
const ACCOUNT_VISIBILITY: Filter = Filter::Sql(
    // This query is evaluated per client request.
    // :sender is automatically bound to the requesting client's identity.
    // Only rows matching this filter are returned to the client from the public 'account' table,
    // overriding its default full visibility for matching clients.
    "SELECT * FROM account WHERE identity = :sender"
);
```

A module will fail to publish if any of its RLS rules are invalid or malformed.

**`:sender`**

You can use the special `:sender` parameter in your rules for user-specific access control.
This parameter is automatically bound to the requesting client's [Identity](#identity).

Note that module owners have unrestricted access to all tables, including all rows of
`public` tables (bypassing RLS rules) and `private` tables.

**Semantic Constraints**

RLS rules act as filters defining which rows of a `public` table are visible to a client.
Like subscriptions, arbitrary column projections are **not** allowed.
Joins **are** allowed (e.g., to check permissions in another table), but each rule must
ultimately return rows from the single public table it applies to.

**Multiple Rules Per Table**

Multiple RLS rules may be declared for the same `public` table. They are evaluated as a
logical `OR`, meaning clients can see any row that matches at least one rule.

**Example (Building on previous Account table)**

```rust
# use spacetimedb::{client_visibility_filter, Filter, table, Identity};
# #[table(name = account)] struct Account { #[primary_key] identity: Identity, email: String, balance: u32 }
// Assume an 'admin' table exists to track administrator identities
#[table(name = admin)] struct Admin { #[primary_key] identity: Identity }

/// RLS Rule 1: A client can see their own account.
#[client_visibility_filter]
const ACCOUNT_OWNER_VISIBILITY: Filter = Filter::Sql(
    "SELECT * FROM account WHERE identity = :sender"
);

/// RLS Rule 2: An admin client can see *all* accounts.
#[client_visibility_filter]
const ACCOUNT_ADMIN_VISIBILITY: Filter = Filter::Sql(
    // This join checks if the requesting client (:sender) exists in the admin table.
    // If they do, the join succeeds, and all rows from 'account' are potentially visible.
    "SELECT account.* FROM account JOIN admin WHERE admin.identity = :sender"
);

// Result: A non-admin client sees only their own account row.
// An admin client sees all account rows because they match the second rule.
```

**Recursive Application**

RLS rules can reference other tables that might _also_ have RLS rules. These rules are applied recursively.
For instance, if Rule A depends on Table B, and Table B has its own RLS rules, a client only gets results
from Rule A if they also have permission to see the relevant rows in Table B according to Table B's rules.
This ensures that the intended row visibility on `public` tables is maintained even through indirect access patterns.

**Example (Building on previous Account/Admin tables)**

```rust
# use spacetimedb::{client_visibility_filter, Filter, table, Identity};
# #[table(name = account)] struct Account { #[primary_key] identity: Identity, email: String, balance: u32 }
# #[table(name = admin)] struct Admin { #[primary_key] identity: Identity }
// Define a private player table linked to account
#[table(name = player)] // Private table
struct Player { #[primary_key] id: Identity, level: u32 }

# /// RLS Rule 1: A client can see their own account.
# #[client_visibility_filter] const ACCOUNT_OWNER_VISIBILITY: Filter = Filter::Sql( "SELECT * FROM account WHERE identity = :sender" );
# /// RLS Rule 2: An admin client can see *all* accounts.
# #[client_visibility_filter] const ACCOUNT_ADMIN_VISIBILITY: Filter = Filter::Sql( "SELECT account.* FROM account JOIN admin WHERE admin.identity = :sender" );

/// RLS Rule for Player table: Players are visible if the associated account is visible.
#[client_visibility_filter]
const PLAYER_VISIBILITY: Filter = Filter::Sql(
    // This rule joins Player with Account.
    // Crucially, the client running this query must *also* satisfy the RLS rules
    // defined for the `account` table for the specific account row being joined.
    // Therefore, non-admins see only their own player, admins see all players.
    "SELECT p.* FROM account a JOIN player p ON a.identity = p.id"
);
```

Self-joins are allowed within RLS rules. However, RLS rules cannot be mutually recursive
(e.g., Rule A depends on Table B, and Rule B depends on Table A), as this would cause
infinite recursion during evaluation.

**Example: Self-Join (Valid)**

```rust
# use spacetimedb::{client_visibility_filter, Filter, table, Identity};
# #[table(name = player)] struct Player { #[primary_key] id: Identity, level: u32 }
# // Dummy account table for join context
# #[table(name = account)] struct Account { #[primary_key] identity: Identity }

/// RLS Rule: A client can see other players at the same level as their own player.
#[client_visibility_filter]
const PLAYER_SAME_LEVEL_VISIBILITY: Filter = Filter::Sql("
    SELECT q.*
    FROM account a -- Find the requester's account
    JOIN player p ON a.identity = p.id -- Find the requester's player
    JOIN player q on p.level = q.level -- Find other players (q) at the same level
    WHERE a.identity = :sender -- Ensure we start with the requester
");
```

**Example: Mutually Recursive Rules (Invalid)**

This module would fail to publish because the `ACCOUNT_NEEDS_PLAYER` rule depends on the
`player` table, while the `PLAYER_NEEDS_ACCOUNT` rule depends on the `account` table.

```rust
use spacetimedb::{client_visibility_filter, Filter, table, Identity};

#[table(name = account)] struct Account { #[primary_key] id: u64, identity: Identity }
#[table(name = player)] struct Player { #[primary_key] id: u64 }

/// RLS: An account is visible only if a corresponding player exists.
#[client_visibility_filter]
const ACCOUNT_NEEDS_PLAYER: Filter = Filter::Sql(
    "SELECT a.* FROM account a JOIN player p ON a.id = p.id WHERE a.identity = :sender"
);

/// RLS: A player is visible only if a corresponding account exists.
#[client_visibility_filter]
const PLAYER_NEEDS_ACCOUNT: Filter = Filter::Sql(
    // This rule requires access to 'account', which itself requires access to 'player' -> recursion!
    "SELECT p.* FROM account a JOIN player p ON a.id = p.id WHERE a.identity = :sender"
);
```

**Usage in Subscriptions**

When a client subscribes to a `public` table that has RLS rules defined,
the server automatically applies those rules. The subscription results (both initial
and subsequent updates) will only contain rows that the specific client is allowed to
see based on the RLS rules evaluating successfully for that client.

While the SQL constraints and limitations outlined in the [SQL reference docs](/docs/sql/index.md#subscriptions)
(like limitations on complex joins or aggregations) do not apply directly to the definition
of RLS rules themselves, these constraints _do_ apply to client subscriptions that _use_ those rules.
For example, an RLS rule might use a complex join not normally supported in subscriptions.
If a client tries to subscribe directly to the table governed by that complex RLS rule,
the subscription itself might fail, even if the RLS rule is valid for direct queries.

**Best Practices**

1.  Define RLS rules for `public` tables where you need to restrict row visibility for different clients.
2.  Use `:sender` for client-specific filtering within your rules.
3.  Keep RLS rules as simple as possible while enforcing desired access.
4.  Be mindful of potential performance implications of complex joins in RLS rules, especially when combined with subscriptions.
5.  Follow the general [SQL best practices](/docs/sql/index.md#best-practices-for-performance-and-scalability) for optimizing your RLS rules.

### Client SDK (Rust)

This section details how to build native Rust client applications that interact with a SpacetimeDB module.

#### 1. Project Setup

Start by creating a standard Rust binary project and adding the `spacetimedb_sdk` crate as a dependency:

```bash
cargo new my_rust_client
cd my_rust_client
cargo add spacetimedb_sdk # Ensure version matches your SpacetimeDB installation
```

#### 2. Generate Module Bindings

Client code relies on generated bindings specific to your server module. Use the `spacetime generate` command, pointing it to your server module project:

```bash
# From your client project directory
mkdir -p src/module_bindings
spacetime generate --lang rust \
    --out-dir src/module_bindings \
    --project-path ../path/to/your/server_module
```

Then, declare the generated module in your `main.rs` or `lib.rs`:

```rust
mod module_bindings;
// Optional: bring generated types into scope
// use module_bindings::*;
```

#### 3. Connecting to the Database

The core type for managing a connection is `module_bindings::DbConnection`. You configure and establish a connection using a builder pattern.

- **Builder:** Start with `DbConnection::builder()`.
- **URI & Name:** Specify the SpacetimeDB instance URI (`.with_uri("http://localhost:3000")`) and the database name or identity (`.with_module_name("my_database")`).
- **Authentication:** Provide an identity token using `.with_token(Option<String>)`. If `None` or omitted for the first connection, the server issues a new identity and token (retrieved via the `on_connect` callback).
- **Callbacks:** Register callbacks for connection lifecycle events:
  - `.on_connect(|conn, identity, token| { ... })`: Runs on successful connection. Often used to store the `token` for future connections.
  - `.on_connect_error(|err_ctx, error| { ... })`: Runs if connection fails.
  - `.on_disconnect(|err_ctx, maybe_error| { ... })`: Runs when the connection closes, either gracefully or due to an error.
- **Build:** Call `.build()` to initiate the connection attempt.

```rust
use spacetimedb_sdk::{identity, DbContext, Identity, credentials};
use crate::module_bindings::{DbConnection, connect_event_callbacks, table_update_callbacks};

const HOST: &str = "http://localhost:3000";
const DB_NAME: &str = "my_database"; // Or your specific DB name/identity

fn connect_to_db() -> DbConnection {
    // Helper for storing/loading auth token
    fn creds_store() -> credentials::File {
        credentials::File::new(".my_client_creds") // Unique filename
    }

    DbConnection::builder()
        .with_uri(HOST)
        .with_module_name(DB_NAME)
        .with_token(creds_store().load().ok()) // Load token if exists
        .on_connect(|conn, identity, auth_token| {
            println!("Connected. Identity: {}", identity.to_hex());
            // Save the token for future connections
            if let Err(e) = creds_store().save(auth_token) {
                eprintln!("Failed to save auth token: {}", e);
            }
            // Register other callbacks *after* successful connection
            connect_event_callbacks(conn);
            table_update_callbacks(conn);
            // Initiate subscriptions
            subscribe_to_tables(conn);
        })
        .on_connect_error(|err_ctx, err| {
            eprintln!("Connection Error: {}", err);
            std::process::exit(1);
        })
        .on_disconnect(|err_ctx, maybe_err| {
            println!("Disconnected. Reason: {:?}", maybe_err);
            std::process::exit(0);
        })
        .build()
        .expect("Failed to connect")
}
```

#### 4. Managing the Connection Loop

After establishing the connection, you need to continuously process incoming messages and trigger callbacks. The SDK offers several ways:

- **Threaded:** `connection.run_threaded()`: Spawns a dedicated background thread that automatically handles message processing.
- **Async:** `async connection.run_async()`: Integrates with async runtimes like Tokio or async-std.
- **Manual Tick:** `connection.frame_tick()`: Processes pending messages without blocking. Suitable for integrating into game loops or other manual polling scenarios. You must call this repeatedly.

```rust
// Example using run_threaded
fn main() {
    let connection = connect_to_db();
    let handle = connection.run_threaded(); // Spawns background thread

    // Main thread can now do other work, like handling user input
    // handle_user_input(&connection);

    handle.join().expect("Connection thread panicked");
}
```

#### 5. Subscribing to Data

Clients receive data by subscribing to SQL queries against the database's public tables.

- **Builder:** Start with `connection.subscription_builder()`.
- **Callbacks:**
  - `.on_applied(|sub_ctx| { ... })`: Runs when the initial data for the subscription arrives.
  - `.on_error(|err_ctx, error| { ... })`: Runs if the subscription fails (e.g., invalid SQL).
- **Subscribe:** Call `.subscribe(vec!["SELECT * FROM table_a", "SELECT * FROM table_b WHERE some_col > 10"])` with a list of query strings. This returns a `SubscriptionHandle`.
- **All Tables:** `.subscribe_to_all_tables()` is a convenience for simple clients but cannot be easily unsubscribed.
- **Unsubscribing:** Use `handle.unsubscribe()` or `handle.unsubscribe_then(|sub_ctx| { ... })` to stop receiving updates for specific queries.

```rust
use crate::module_bindings::{SubscriptionEventContext, ErrorContext};

fn subscribe_to_tables(conn: &DbConnection) {
    println!("Subscribing to tables...");
    conn.subscription_builder()
        .on_applied(on_subscription_applied)
        .on_error(|err_ctx, err| {
            eprintln!("Subscription failed: {}", err);
        })
        // Example: Subscribe to all rows from 'player' and 'message' tables
        .subscribe(vec!["SELECT * FROM player", "SELECT * FROM message"]);
}

fn on_subscription_applied(ctx: &SubscriptionEventContext) {
    println!("Subscription applied! Initial data received.");
    // Example: Print initial messages sorted by time
    let mut messages: Vec<_> = ctx.db().message().iter().collect();
    messages.sort_by_key(|m| m.sent);
    for msg in messages {
        // print_message(ctx.db(), &msg); // Assuming a print_message helper
    }
}
```

#### 6. Accessing Cached Data & Handling Row Callbacks

Subscribed data is stored locally in the client cache, accessible via `ctx.db()` (where `ctx` can be a `DbConnection` or any event context).

- **Accessing Tables:** Use `ctx.db().table_name()` to get a handle to a table.
- **Iterating:** `table_handle.iter()` returns an iterator over all cached rows.
- **Filtering/Finding:** Use index accessors like `table_handle.primary_key_field().find(&pk_value)` or `table_handle.indexed_field().filter(value_or_range)` for efficient lookups (similar to server-side).
- **Row Callbacks:** Register callbacks to react to changes in the cache:
  - `table_handle.on_insert(|event_ctx, inserted_row| { ... })`
  - `table_handle.on_delete(|event_ctx, deleted_row| { ... })`
  - `table_handle.on_update(|event_ctx, old_row, new_row| { ... })` (Only for tables with a `#[primary_key]`)

```rust
use crate::module_bindings::{Player, Message, EventContext, Event, DbView};

// Placeholder for where other callbacks are registered
fn table_update_callbacks(conn: &DbConnection) {
    conn.db().player().on_insert(handle_player_insert);
    conn.db().player().on_update(handle_player_update);
    conn.db().message().on_insert(handle_message_insert);
}

fn handle_player_insert(ctx: &EventContext, player: &Player) {
    // Only react to updates caused by reducers, not initial subscription load
    if let Event::Reducer(_) = ctx.event {
       println!("Player joined: {}", player.name.as_deref().unwrap_or("Unknown"));
    }
}

fn handle_player_update(ctx: &EventContext, old: &Player, new: &Player) {
    if old.name != new.name {
        println!("Player renamed: {} -> {}",
            old.name.as_deref().unwrap_or("??"),
            new.name.as_deref().unwrap_or("??")
        );
    }
    // ... handle other changes like online status ...
}

fn handle_message_insert(ctx: &EventContext, message: &Message) {
    if let Event::Reducer(_) = ctx.event {
        // Find sender name from cache
        let sender_name = ctx.db().player().identity().find(&message.sender)
            .map_or("Unknown".to_string(), |p| p.name.clone().unwrap_or("??".to_string()));
        println!("{}: {}", sender_name, message.text);
    }
}
```

:::info Handling Initial Data vs. Live Updates in Callbacks
Callbacks like `on_insert` and `on_update` are triggered for both the initial data received when a subscription is first applied _and_ for subsequent live changes caused by reducers. If you need to differentiate (e.g., only react to _new_ messages, not the backlog), you can inspect the `ctx.event` type. For example, `if let Event::Reducer(_) = ctx.event { ... }` checks if the change came from a reducer call.
:::

#### 7. Invoking Reducers & Handling Reducer Callbacks

Clients trigger state changes by calling reducers defined in the server module.

- **Invoking:** Access generated reducer functions via `ctx.reducers().reducer_name(arg1, arg2, ...)`.
- **Reducer Callbacks:** Register callbacks to react to the _outcome_ of reducer calls (especially useful for handling failures or confirming success if not directly observing table changes):
  - `ctx.reducers().on_reducer_name(|reducer_event_ctx, arg1, ...| { ... })`
  - The `reducer_event_ctx.event` contains:
    - `reducer`: The specific reducer variant and its arguments.
    - `status`: `Status::Committed`, `Status::Failed(reason)`, or `Status::OutOfEnergy`.
    - `caller_identity`, `timestamp`, etc.

```rust
use crate::module_bindings::{ReducerEventContext, Status};

// Placeholder for where other callbacks are registered
fn connect_event_callbacks(conn: &DbConnection) {
    conn.reducers().on_set_name(handle_set_name_result);
    conn.reducers().on_send_message(handle_send_message_result);
}

fn handle_set_name_result(ctx: &ReducerContext, name: &String) {
    if let Status::Failed(reason) = &ctx.event.status {
        // Check if the failure was for *our* call (important in multi-user contexts)
        if ctx.event.caller_identity == ctx.identity() {
             eprintln!("Error setting name to '{}': {}", name, reason);
        }
    }
}

fn handle_send_message_result(ctx: &ReducerContext, text: &String) {
    if let Status::Failed(reason) = &ctx.event.status {
        if ctx.event.caller_identity == ctx.identity() { // Our call failed
             eprintln!("[Error] Failed to send message '{}': {}", text, reason);
        }
    }
}

// Example of calling a reducer (e.g., from user input handler)
fn send_chat_message(conn: &DbConnection, message: String) {
    if !message.is_empty() {
        conn.reducers().send_message(message); // Fire-and-forget style
    }
}
```

// ... (Keep the second info box about C# callbacks, it will be moved later) ...
:::info Handling Initial Data vs. Live Updates in Callbacks
Callbacks like `OnInsert` and `OnUpdate` are triggered for both the initial data received when a subscription is first applied _and_ for subsequent live changes caused by reducers. If you need to differentiate (e.g., only react to _new_ messages, not the backlog), you can inspect the `ctx.Event` type. For example, checking `if (ctx.Event is not Event<Reducer>.SubscribeApplied) { ... }` ensures the code only runs for events triggered by reducers, not the initial subscription data load.
:::

### Server Module (C#)

#### Defining Types

Custom classes, structs, or records intended for use as fields within database tables or as parameters/return types in reducers must be marked with the `[Type]` attribute. This attribute enables SpacetimeDB to handle the serialization and deserialization of these types.

- **Basic Usage:** Apply `[Type]` to your classes, structs, or records. Use the `partial` modifier to allow SpacetimeDB's source generators to augment the type definition.
- **Cross-Language Naming:** Currently, the C# module SDK does **not** provide a direct equivalent to Rust's `#[sats(name = "...")]` attribute for controlling the generated names in _other_ client languages (like TypeScript). The C# type name itself (including its namespace) is typically used. Standard C# namespacing (`namespace MyGame.SharedTypes { ... }`) is the primary way to organize and avoid collisions.
- **Enums:** Standard C# enums can be marked with `[Type]`. For "tagged unions" or "discriminated unions" (like Rust enums with associated data), use the pattern of an abstract base record/class with the `[Type]` attribute, and derived records/classes for each variant, also marked with `[Type]`. Then, define a final `[Type]` record that inherits from `TaggedEnum<(...)>` listing the variants.
- **Type Aliases:** Use standard C# `using` aliases for clarity (e.g., `using PlayerScore = System.UInt32;`). The underlying primitive type must still be serializable by SpacetimeDB.

```csharp
using SpacetimeDB;
using System; // Required for System.UInt32 if using aliases like below

// Example Struct
[Type]
public partial struct Position { public int X; public int Y; }

// Example Tagged Union (Enum with Data) Pattern:
// 1. Base abstract record
[Type] public abstract partial record PlayerStatusBase { }
// 2. Derived records for variants
[Type] public partial record IdleStatus : PlayerStatusBase { }
[Type] public partial record WalkingStatus : PlayerStatusBase { public Position Target; }
[Type] public partial record FightingStatus : PlayerStatusBase { public Identity OpponentId; }
// 3. Final type inheriting from TaggedEnum
[Type]
public partial record PlayerStatus : TaggedEnum<(
    IdleStatus Idle,
    WalkingStatus Walking,
    FightingStatus Fighting
)> { }

// Example Standard Enum
[Type]
public enum ItemType { Weapon, Armor, Potion }

// Example Type Alias
using PlayerScore = System.UInt32;

```

:::info C# `partial` Keyword
Table and Type definitions in C# should use the `partial` keyword (e.g., `public partial class MyTable`). This allows the SpacetimeDB source generator to add necessary internal methods and serialization logic to your types without requiring you to write boilerplate code.
:::

#### Defining Tables

Database tables store the application's persistent state. They are defined using C# classes or structs marked with the `[Table]` attribute.

- **Core Attribute:** `[Table(Name = "my_table_name", ...)]` marks a class or struct as a database table definition. The specified string `Name` is how the table will be referenced in SQL queries and generated APIs.
- **Partial Modifier:** Use the `partial` keyword (e.g., `public partial class MyTable`) to allow SpacetimeDB's source generators to add necessary methods and logic to your definition.
- **Public vs. Private:** By default, tables are **private**, accessible only by server-side reducer code. To allow clients to read or subscribe to a table's data, set `Public = true` within the attribute: `[Table(..., Public = true)]`. This is a common source of errors if forgotten.
- **Primary Keys:** Designate a single **public field** as the primary key using `[PrimaryKey]`. This ensures uniqueness, creates an efficient index, and allows clients to track row updates.
- **Auto-Increment:** Mark an integer-typed primary key **public field** with `[AutoInc]` to have SpacetimeDB automatically assign unique, sequentially increasing values upon insertion. Provide `0` as the value for this field when inserting a new row to trigger the auto-increment mechanism.
- **Unique Constraints:** Enforce uniqueness on non-primary key **public fields** using `[Unique]`. Attempts to insert or update rows violating this constraint will fail (throw an exception).
- **Indexes:** Create B-tree indexes for faster lookups on specific **public fields** or combinations of fields. Use `[Index.BTree]` on a single field for a simple index, or define indexes at the class/struct level using `[Index.BTree(Name = "MyIndexName", Columns = new[] { nameof(ColA), nameof(ColB) })]`.
- **Nullable Fields:** Use standard C# nullable reference types (`string?`) or nullable value types (`int?`, `Timestamp?`) for fields that can hold null values.
- **Instances vs. Database:** Remember that table class/struct instances (e.g., `var player = new PlayerState { ... };`) are just data objects. Modifying an instance does **not** automatically update the database. Interaction happens through generated handles accessed via the `ReducerContext` (e.g., `ctx.Db.player_state.Insert(...)`).
- **Case Sensitivity:** Table names specified via `Name = "..."` are case-sensitive and must be matched exactly in SQL queries.
- **Pitfalls:**
  - SpacetimeDB attributes (`[PrimaryKey]`, `[AutoInc]`, `[Unique]`, `[Index.BTree]`) **must** be applied to **public fields**, not properties (`{ get; set; }`). Using properties can cause build errors or runtime issues.
  - Avoid manually inserting values into `[AutoInc]` fields that are also `[Unique]`, especially values larger than the current sequence counter, as this can lead to future unique constraint violations when the counter catches up.
  - Ensure `Public = true` is set if clients need access.
  - Always use the `partial` keyword on table definitions.
  - Define indexes _within_ the main `#[table(name=..., index=...)]` attribute. Each `#[table]` macro invocation defines a _distinct_ table and requires a `name`; separate `#[table]` attributes cannot be used solely to add indexes to a previously named table.

```csharp
using SpacetimeDB;
using System; // For Nullable types if needed

// Assume Position, PlayerStatus, ItemType are defined as types

// Example Table Definition
[Table(Name = "player_state", Public = true)]
[Index.BTree(Name = "idx_level", Columns = new[] { nameof(Level) })] // Table-level index
public partial class PlayerState
{
    [PrimaryKey]
    public Identity PlayerId; // Public field
    [Unique]
    public string Name = ""; // Public field (initialize to avoid null warnings if needed)
    public uint Health; // Public field
    public ushort Level; // Public field
    public Position Position; // Public field (custom struct type)
    public PlayerStatus Status; // Public field (custom record type)
    public Timestamp? LastLogin; // Public field, nullable struct
}

[Table(Name = "inventory_item", Public = true)]
public partial class InventoryItem
{
    [PrimaryKey]
    [AutoInc] // Automatically generate IDs
    public ulong ItemId; // Public field
    public Identity OwnerId; // Public field
    [Index.BTree] // Simple index on this field
    public ItemType ItemType; // Public field
    public uint Quantity; // Public field
}

// Example of a private table
[Table(Name = "internal_game_data")] // Public = false is default
public partial class InternalGameData
{
    [PrimaryKey]
    public string Key = ""; // Public field
    public string Value = ""; // Public field
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

Reducers are the functions within your server module responsible for atomically modifying the database state in response to client requests or internal events (like lifecycle triggers or schedules).

- **Core Attribute:** Reducers are defined as `static` methods within a (typically `static partial`) class, annotated with `[SpacetimeDB.Reducer]`.
- **Signature:** Every reducer method must accept `ReducerContext` as its first argument. Subsequent arguments represent data passed from the client caller or scheduler, and their types must be marked with `[Type]`.
- **Return Type:** Reducers should typically return `void`. Errors are signaled by throwing exceptions.
- **Reducer Context:** The `ReducerContext` (`ctx`) provides access to:
  - `ctx.Db`: Handles for interacting with database tables.
  - `ctx.Sender`: The `Identity` of the caller.
  - `ctx.Identity`: The `Identity` of the module itself.
  - `ctx.Timestamp`: The `Timestamp` of the invocation.
  - `ctx.ConnectionId`: The nullable `ConnectionId` of the caller.
  - `ctx.Rng`: A `System.Random` instance for deterministic random number generation (if needed).
- **Transactionality:** Each reducer call executes within a single, atomic database transaction. If the method completes without an unhandled exception, all database changes are committed. If an exception is thrown, the transaction is aborted, and **all changes are rolled back**, preserving data integrity.
- **Execution Environment:** Reducers run in a sandbox and **cannot** directly perform network I/O (`System.Net`) or filesystem operations (`System.IO`). External interaction primarily occurs through database table modifications (observed by clients) and logging (`SpacetimeDB.Log`).
- **Calling Other Reducers:** A reducer can directly call another static reducer method defined in the same module. This is a standard method call and executes within the _same_ transaction; it does not create a sub-transaction.

```csharp
using SpacetimeDB;
using System;
using System.Linq; // Used in more complex examples later

public static partial class Module
{
    // Assume PlayerState and InventoryItem tables are defined as previously
    [Table(Name = "player_state", Public = true)] public partial class PlayerState {
        [PrimaryKey] public Identity PlayerId;
        [Unique] public string Name = "";
        public uint Health; public ushort Level; /* ... other fields */ }
    [Table(Name = "inventory_item", Public = true)] public partial class InventoryItem {
        [PrimaryKey] #[AutoInc] public ulong ItemId;
        public Identity OwnerId; /* ... other fields */ }

    // Example: Basic reducer to update player data
    [Reducer]
    public static void UpdatePlayerData(ReducerContext ctx, string? newName)
    {
        var playerId = ctx.Sender;

        // Find player by primary key
        var player = ctx.Db.player_state.PlayerId.Find(playerId);
        if (player == null)
        {
            throw new Exception($"Player not found: {playerId}");
        }

        // Update fields conditionally
        bool requiresUpdate = false;
        if (!string.IsNullOrWhiteSpace(newName))
        {
             // Basic check for name uniqueness (simplified)
             var existing = ctx.Db.player_state.Name.Find(newName);
             if(existing != null && !existing.PlayerId.Equals(playerId)) {
                 throw new Exception($"Name '{newName}' already taken.");
             }
             if (player.Name != newName) {
            player.Name = newName;
                requiresUpdate = true;
        }
        }

        if (player.Level < 100) { // Example simple update
        player.Level += 1;
            requiresUpdate = true;
        }

        // Persist changes if any were made
        if (requiresUpdate) {
        ctx.Db.player_state.PlayerId.Update(player);
        Log.Info($"Updated player data for {playerId}");
        }
    }

    // Example: Basic reducer to register a player
    [Reducer]
    public static void RegisterPlayer(ReducerContext ctx, string name)
    {
        if (string.IsNullOrWhiteSpace(name)) {
             throw new ArgumentException("Name cannot be empty.");
        }
        Log.Info($"Attempting to register player: {name} ({ctx.Sender})");

        // Check if player identity or name already exists
        if (ctx.Db.player_state.PlayerId.Find(ctx.Sender) != null || ctx.Db.player_state.Name.Find(name) != null)
        {
             throw new Exception("Player already registered or name taken.");
        }

        // Create new player instance
        var newPlayer = new PlayerState
        {
            PlayerId = ctx.Sender,
            Name = name,
            Health = 100,
            Level = 1,
            // Initialize other fields as needed...
        };

        // Insert the new player. This will throw on constraint violation.
            ctx.Db.player_state.Insert(newPlayer);
            Log.Info($"Player registered successfully: {ctx.Sender}");
    }

    // Example: Basic reducer showing deletion
    [Reducer]
    public static void DeleteMyItems(ReducerContext ctx)
    {
        var ownerId = ctx.Sender;
        int deletedCount = 0;

        // Find items by owner (Requires an index on OwnerId for efficiency)
        // This example iterates if no index exists.
        var itemsToDelete = ctx.Db.inventory_item.Iter()
                                  .Where(item => item.OwnerId.Equals(ownerId))
                                  .ToList(); // Collect IDs to avoid modification during iteration

        foreach(var item in itemsToDelete)
        {
            // Delete using the primary key index
            if (ctx.Db.inventory_item.ItemId.Delete(item.ItemId)) {
                     deletedCount++;
                 }
            }
        Log.Info($"Deleted {deletedCount} items for player {ownerId}.");
    }
}
```

##### Handling Insert Constraint Violations

Unlike Rust's `try_insert` which returns a `Result`, the C# `Insert` method throws an exception if a constraint (like a primary key or unique index violation) occurs. There are two main ways to handle this in C# reducers:

1.  **Pre-checking:** Before calling `Insert`, explicitly query the database using the relevant indexes to check if the insertion would violate any constraints (e.g., check if a user with the same ID or unique name already exists). This is often cleaner if the checks are straightforward. The `RegisterPlayer` example above demonstrates this pattern.

2.  **Using `try-catch`:** Wrap the `Insert` call in a `try-catch` block. This allows you to catch the specific exception (often a `SpacetimeDB.ConstraintViolationException` or potentially a more general `Exception` depending on the SDK version and error type) and handle the failure gracefully (e.g., log an error, return a specific error message to the client via a different mechanism if applicable, or simply allow the transaction to roll back cleanly without crashing the reducer unexpectedly).

```csharp
using SpacetimeDB;
using System;

public static partial class Module
{
    [Table(Name = "unique_items")]
    public partial class UniqueItem {
        [PrimaryKey] public string ItemName;
        public int Value;
    }

    // Example using try-catch for insertion
    [Reducer]
    public static void AddUniqueItemWithCatch(ReducerContext ctx, string name, int value)
    {
        var newItem = new UniqueItem { ItemName = name, Value = value };
        try
        {
            // Attempt to insert
            ctx.Db.unique_items.Insert(newItem);
            Log.Info($"Successfully inserted item: {name}");
        }
        catch (Exception ex) // Catch a general exception or a more specific one if available
        {
            // Log the specific error
            Log.Error($"Failed to insert item '{name}': Constraint violation or other error. Details: {ex.Message}");
            // Optionally, re-throw a custom exception or handle differently
            // Throwing ensures the transaction is rolled back
            throw new Exception($"Item name '{name}' might already exist.");
        }
    }
}
```

Choosing between pre-checking and `try-catch` depends on the complexity of the constraints and the desired flow. Pre-checking can avoid the overhead of exception handling for predictable violations, while `try-catch` provides a direct way to handle unexpected insertion failures.

:::note C# `Insert` vs Rust `try_insert`
Unlike Rust, the C# SDK does not currently provide a `TryInsert` method that returns a result. The standard `Insert` method will throw an exception if a constraint (primary key, unique index) is violated. Therefore, C# reducers should typically check for potential constraint violations _before_ calling `Insert`, or be prepared to handle the exception (which will likely roll back the transaction).
:::

##### Lifecycle Reducers

Special reducers handle specific events:

- `[Reducer(ReducerKind.Init)]`: Runs once when the module is first published **and** any time the database is manually cleared (e.g., via `spacetime publish -c` or `spacetime server clear`). Failure prevents publishing or clearing. Often used for initial data setup.
- `[Reducer(ReducerKind.ClientConnected)]`: Runs when any distinct client connection (e.g., WebSocket, HTTP call) is established. Failure disconnects the client. `ctx.connection_id` is guaranteed to have a value within this reducer.
- `[Reducer(ReducerKind.ClientDisconnected)]`: Runs when any distinct client connection terminates. Failure is logged but does not prevent disconnection. `ctx.connection_id` is guaranteed to have a value within this reducer.

These reducers cannot take arguments beyond `&ReducerContext`.

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

#### Scheduled Reducers (C#)

In addition to lifecycle annotations, reducers can be scheduled. This allows calling the reducers at a particular time, or periodically for loops (e.g., game loops).

The scheduling information for a reducer is stored in a table. This table links to the reducer function and has specific mandatory fields:

1.  **Define the Schedule Table:** Create a table class/struct using `[Table(Name = ..., Scheduled = nameof(YourReducerName), ScheduledAt = nameof(YourScheduleAtColumnName))]`.
    - The `Scheduled` parameter links this table to the static reducer method `YourReducerName`.
    - The `ScheduledAt` parameter specifies the name of the field within this table that holds the scheduling information. This field **must** be of type `SpacetimeDB.ScheduleAt`.
    - The table **must** also have a primary key field (often `[AutoInc] ulong Id`).
    - Additional fields can be included to pass arguments to the scheduled reducer.
2.  **Define the Scheduled Reducer:** Create the `static` reducer method (`YourReducerName`) specified in the table attribute. It takes `ReducerContext` and an instance of the schedule table class/struct as arguments.
3.  **Schedule an Invocation:** Inside another reducer, create an instance of your schedule table struct.
    - Set the `ScheduleAt` field (using the name specified in the `ScheduledAt` parameter) to either:
      - `new ScheduleAt.Time(timestamp)`: Schedules the reducer to run **once** at the specified `Timestamp`.
      - `new ScheduleAt.Interval(timeDuration)`: Schedules the reducer to run **periodically** with the specified `TimeDuration` interval.
    - Set the primary key (e.g., to `0` if using `[AutoInc]`) and any other argument fields.
    - Insert this instance into the schedule table using `ctx.Db.your_schedule_table_name.Insert(...)`.

Managing timers with a scheduled table is as simple as inserting or deleting rows. This makes scheduling transactional in SpacetimeDB. If a reducer A schedules B but then throws an exception, B will not be scheduled.

```csharp
using SpacetimeDB;
using System;

public static partial class Module
{
    // 1. Define the table with scheduling information, linking to `SendMessage` reducer.
    // Specifies that the `ScheduledAt` field holds the schedule info.
    [Table(Name = "send_message_schedule", Scheduled = nameof(SendMessage), ScheduledAt = nameof(ScheduledAt))]
    public partial struct SendMessageSchedule
    {
        // Mandatory fields:
        [PrimaryKey]
        [AutoInc]
        public ulong Id; // Identifier for the scheduled call

        public ScheduleAt ScheduledAt; // Holds the schedule timing

        // Custom fields (arguments for the reducer):
        public string Message;
    }

    // 2. Define the scheduled reducer.
    // It takes the schedule table struct as its second argument.
    [Reducer]
    public static void SendMessage(ReducerContext ctx, SendMessageSchedule scheduleArgs)
    {
        // Security check is important!
        if (!ctx.Sender.Equals(ctx.Identity))
        {
            throw new Exception("Reducer SendMessage may not be invoked by clients, only via scheduling.");
        }

        Log.Info($"Scheduled SendMessage: {scheduleArgs.Message}");
        // ... perform action with scheduleArgs.Message ...
    }

    // 3. Example of scheduling reducers (e.g., in Init)
    [Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        // Avoid rescheduling if Init runs again
        if (ctx.Db.send_message_schedule.Count > 0) {
             return;
        }

        var tenSeconds = new TimeDuration { Microseconds = 10_000_000 };
        var futureTimestamp = ctx.Timestamp + tenSeconds;

        // Schedule a one-off message
        ctx.Db.send_message_schedule.Insert(new SendMessageSchedule
        {
            Id = 0, // Let AutoInc assign ID
            // Use ScheduleAt.Time for one-off execution at a specific Timestamp
            ScheduledAt = new ScheduleAt.Time(futureTimestamp),
            Message = "I'm a bot sending a message one time!"
        });
        Log.Info("Scheduled one-off message.");

        // Schedule a periodic message (every 10 seconds)
        ctx.Db.send_message_schedule.Insert(new SendMessageSchedule
        {
            Id = 0, // Let AutoInc assign ID
             // Use ScheduleAt.Interval for periodic execution with a TimeDuration
            ScheduledAt = new ScheduleAt.Interval(tenSeconds),
            Message = "I'm a bot sending a message every 10 seconds!"
        });
        Log.Info("Scheduled periodic message.");
    }
}
```

##### Scheduled Reducer Details

- **Best-Effort Scheduling:** Scheduled reducers are called on a best-effort basis and may be slightly delayed in their execution when a database is under heavy load.

- **Restricting Access (Security):** Scheduled reducers are normal reducers and _can_ still be called directly by clients. If a scheduled reducer should _only_ be called by the scheduler, it is crucial to begin the reducer with a check comparing the caller's identity (`ctx.Sender`) to the module's own identity (`ctx.Identity`).
  ```csharp
  [Reducer] // Assuming linked via [Table(Scheduled=...)]
  public static void MyScheduledTask(ReducerContext ctx, MyScheduleArgs args)
  {
      if (!ctx.Sender.Equals(ctx.Identity))
      {
          throw new Exception("Reducer MyScheduledTask may not be invoked by clients, only via scheduling.");
      }
      // ... Reducer body proceeds only if called by scheduler ...
      Log.Info("Executing scheduled task...");
  }
  // Define MyScheduleArgs table elsewhere with [Table(Scheduled=nameof(MyScheduledTask), ...)]
  public partial struct MyScheduleArgs { /* ... fields including ScheduleAt ... */ }
  ```

:::info Scheduled Reducers and Connections
Scheduled reducer calls originate from the SpacetimeDB scheduler itself, not from an external client connection. Therefore, within a scheduled reducer, `ctx.Sender` will be the module's own identity, and `ctx.ConnectionId` will be `null`.
:::

##### Error Handling: Exceptions

Throwing an unhandled exception within a C# reducer will cause the transaction to roll back.

- **Expected Failures:** For predictable errors (e.g., invalid arguments, state violations), explicitly `throw` an `Exception`. The exception message can be observed by the client in the `ReducerEventContext` status.
- **Unexpected Errors:** Unhandled runtime exceptions (e.g., `NullReferenceException`) also cause rollbacks but might provide less informative feedback to the client, potentially just indicating a general failure.

It's generally good practice to validate input and state early in the reducer and `throw` specific exceptions for handled error conditions.

#### Row-Level Security (RLS)

Row Level Security (RLS) allows module authors to restrict which rows of a public table each client can access.
These access rules are expressed in SQL and evaluated automatically for queries and subscriptions.

:::info Version-Specific Status
Row-Level Security (RLS) was introduced as an **unstable** feature in **SpacetimeDB v1.1.0**.
It requires explicit opt-in via feature flags or pragmas.
:::

**Enabling RLS**

RLS is currently **unstable** and must be explicitly enabled in your module.

To enable RLS, include the following preprocessor directive at the top of your module files:

```cs
#pragma warning disable STDB_UNSTABLE
```

**How It Works**

RLS rules are attached to `public` tables (tables with `#[table(..., public)]`)
and are expressed in SQL using public static readonly fields of type `Filter` annotated with
`[SpacetimeDB.ClientVisibilityFilter]`.

```cs
using SpacetimeDB;

#pragma warning disable STDB_UNSTABLE

// Define a public table for RLS
[Table(Name = "account", Public = true)] // Ensures correct C# syntax for public table
public partial class Account
{
    [PrimaryKey] public Identity Identity;
    public string Email = "";
    public uint Balance;
}

public partial class Module
{
    /// <summary>
    /// RLS Rule: Allow a client to see *only* their own account record.
    /// This rule applies to the public 'account' table.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_VISIBILITY = new Filter.Sql(
        // This query is evaluated per client request.
        // :sender is automatically bound to the requesting client's identity.
        // Only rows matching this filter are returned to the client from the public 'account' table,
        // overriding its default full visibility for matching clients.
        "SELECT * FROM account WHERE identity = :sender"
    );
}
```

A module will fail to publish if any of its RLS rules are invalid or malformed.

**`:sender`**

You can use the special `:sender` parameter in your rules for user specific access control.
This parameter is automatically bound to the requesting client's [Identity](#identity).

Note that module owners have unrestricted access to all tables, including all rows of
`public` tables (bypassing RLS rules) and `private` tables.

**Semantic Constraints**

RLS rules are similar to subscriptions in that logically they act as filters on a particular table.
Also like subscriptions, arbitrary column projections are **not** allowed.
Joins **are** allowed, but each rule must return rows from one and only one table.

**Multiple Rules Per Table**

Multiple rules may be declared for the same `public` table. They are evaluated as a logical `OR`.
This means clients will be able to see to any row that matches at least one rule.

**Example**

```cs
using SpacetimeDB;

#pragma warning disable STDB_UNSTABLE

public partial class Module
{
    /// <summary>
    /// A client can only see their account.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_FILTER = new Filter.Sql(
        "SELECT * FROM account WHERE identity = :sender"
    );

    /// <summary>
    /// An admin can see all accounts.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_FILTER_FOR_ADMINS = new Filter.Sql(
        "SELECT account.* FROM account JOIN admin WHERE admin.identity = :sender"
    );
}
```

**Recursive Application**

RLS rules can reference other tables with RLS rules, and they will be applied recursively.
This ensures that data is never leaked through indirect access patterns.

**Example**

```cs
using SpacetimeDB;

public partial class Module
{
    /// <summary>
    /// A client can only see their account.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_FILTER = new Filter.Sql(
        "SELECT * FROM account WHERE identity = :sender"
    );

    /// <summary>
    /// An admin can see all accounts.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_FILTER_FOR_ADMINS = new Filter.Sql(
        "SELECT account.* FROM account JOIN admin WHERE admin.identity = :sender"
    );

    /// <summary>
    /// Explicitly filtering by client identity in this rule is not necessary,
    /// since the above RLS rules on `account` will be applied automatically.
    /// Hence a client can only see their player, but an admin can see all players.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter PLAYER_FILTER = new Filter.Sql(
        "SELECT p.* FROM account a JOIN player p ON a.id = p.id"
    );
}
```

And while self-joins are allowed, in general RLS rules cannot be self-referential,
as this would result in infinite recursion.

**Example: Self-Join**

```cs
using SpacetimeDB;

public partial class Module
{
    /// <summary>
    /// A client can only see players on their same level.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter PLAYER_FILTER = new Filter.Sql(@"
        SELECT q.*
        FROM account a
        JOIN player p ON u.id = p.id
        JOIN player q on p.level = q.level
        WHERE a.identity = :sender
    ");
}
```

**Example: Recursive Rules**

This module will fail to publish because each rule depends on the other one.

```cs
using SpacetimeDB;

public partial class Module
{
    /// <summary>
    /// An account must have a corresponding player.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_FILTER = new Filter.Sql(
        "SELECT a.* FROM account a JOIN player p ON a.id = p.id WHERE a.identity = :sender"
    );

    /// <summary>
    /// A player must have a corresponding account.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_FILTER = new Filter.Sql(
        "SELECT p.* FROM account a JOIN player p ON a.id = p.id WHERE a.identity = :sender"
    );
}
```

**Usage in Subscriptions**

RLS rules automatically apply to subscriptions so that if a client subscribes to a table with RLS filters,
the subscription will only return rows that the client is allowed to see.

While the constraints and limitations outlined in the [SQL reference docs](/docs/sql/index.md#subscriptions) do not apply to RLS rules,
they do apply to the subscriptions that use them.
For example, it is valid for an RLS rule to have more joins than are supported by subscriptions.
However a client will not be able to subscribe to the table for which that rule is defined.

**Best Practices**

1. Use `:sender` for client specific filtering.
2. Follow the [SQL best practices](/docs/sql/index.md#best-practices-for-performance-and-scalability) for optimizing your RLS rules.

### Client SDK (C#)

This section details how to build native C# client applications (including Unity games) that interact with a SpacetimeDB module.

#### 1. Project Setup

- **For .NET Console/Desktop Apps:** Create a new project and add the `SpacetimeDB.ClientSDK` NuGet package:
  ```bash
  dotnet new console -o my_csharp_client
  cd my_csharp_client
  dotnet add package SpacetimeDB.ClientSDK
  ```
- **For Unity:** Add the SDK to the Unity package manager by the URL: https://github.com/clockworklabs/com.clockworklabs.spacetimedbsdk.

#### 2. Generate Module Bindings

Client code relies on generated bindings specific to your server module. Use the `spacetime generate` command, pointing it to your server module project:

```bash
# From your client project directory
mkdir -p module_bindings # Or your preferred output location
spacetime generate --lang csharp \
    --out-dir module_bindings \
    --project-path ../path/to/your/server_module
```

Include the generated `.cs` files in your C# project or Unity Assets folder.

#### 3. Connecting to the Database

The core type for managing a connection is `SpacetimeDB.Types.DbConnection` (this type name comes from the generated bindings). You configure and establish a connection using a builder pattern.

- **Builder:** Start with `DbConnection.Builder()`.
- **URI & Name:** Specify the SpacetimeDB instance URI (`.WithUri("http://localhost:3000")`) and the database name or identity (`.WithModuleName("my_database")`).
- **Authentication:** Provide an identity token using `.WithToken(string?)`. The SDK provides a helper `AuthToken.Token` which loads a token from a local file (initialized via `AuthToken.Init(".credentials_filename")`). If `null` or omitted for the first connection, the server issues a new identity and token (retrieved via the `OnConnect` callback).
- **Callbacks:** Register callbacks (as delegates or lambda expressions) for connection lifecycle events:
  - `.OnConnect((conn, identity, token) => { ... })`: Runs on successful connection. Often used to save the `token` using `AuthToken.SaveToken(token)`.
  - `.OnConnectError((exception) => { ... })`: Runs if connection fails.
  - `.OnDisconnect((conn, maybeException) => { ... })`: Runs when the connection closes, either gracefully (`maybeException` is null) or due to an error.
- **Build:** Call `.Build()` to initiate the connection attempt.

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;
using System;

public class ClientManager // Example class
{
    const string HOST = "http://localhost:3000";
    const string DB_NAME = "my_database"; // Or your specific DB name/identity
    private DbConnection connection;

    public void StartConnecting()
    {
        // Initialize token storage (e.g., in AppData)
        AuthToken.Init(".my_client_creds");

        connection = DbConnection.Builder()
            .WithUri(HOST)
            .WithModuleName(DB_NAME)
            .WithToken(AuthToken.Token) // Load token if exists
            .OnConnect(HandleConnect)
            .OnConnectError(HandleConnectError)
            .OnDisconnect(HandleDisconnect)
            .Build();

        // Need to call FrameTick regularly - see next section
    }

    private void HandleConnect(DbConnection conn, Identity identity, string authToken)
    {
        Console.WriteLine($"Connected. Identity: {identity}");
        AuthToken.SaveToken(authToken); // Save token for future connections

        // Register other callbacks after connecting
        RegisterEventCallbacks(conn);

        // Subscribe to data
        SubscribeToTables(conn);
    }

    private void HandleConnectError(Exception e)
    {
        Console.WriteLine($"Connection Error: {e.Message}");
        // Handle error, e.g., retry or exit
    }

    private void HandleDisconnect(DbConnection conn, Exception? e)
    {
        Console.WriteLine($"Disconnected. Reason: {(e == null ? "Requested" : e.Message)}");
        // Handle disconnection
    }

    // Placeholder methods - implementations shown in later sections
    private void RegisterEventCallbacks(DbConnection conn) { /* ... */ }
    private void SubscribeToTables(DbConnection conn) { /* ... */ }
}
```

#### 4. Managing the Connection Loop

Unlike the Rust SDK's `run_threaded` or `run_async`, the C# SDK primarily uses a manual update loop. You **must** call `connection.FrameTick()` regularly (e.g., every frame in Unity's `Update`, or in a loop in a console app) to process incoming messages and trigger callbacks.

- **`FrameTick()`:** Processes all pending network messages, updates the local cache, and invokes registered callbacks.
- **Threading:** It is generally **not recommended** to call `FrameTick()` on a background thread if your main thread also accesses the connection's data (`connection.Db`), as this can lead to race conditions. Handle computationally intensive logic triggered by callbacks separately if needed.

```csharp
// Example in a simple console app loop:
public void RunUpdateLoop()
{
    Console.WriteLine("Running update loop...");
    bool isRunning = true;
    while(isRunning && connection != null && connection.IsConnected)
    {
        connection.FrameTick(); // Process messages

        // Check for user input or other app logic...
        if (Console.KeyAvailable) {
             var key = Console.ReadKey(true).Key;
             if (key == ConsoleKey.Escape) isRunning = false;
             // Handle other input...
        }

        System.Threading.Thread.Sleep(16); // Avoid busy-waiting
    }
    connection?.Disconnect();
    Console.WriteLine("Update loop stopped.");
}
```

#### 5. Subscribing to Data

Clients receive data by subscribing to SQL queries against the database's public tables.

- **Builder:** Start with `connection.SubscriptionBuilder()`.
- **Callbacks:**
  - `.OnApplied((subCtx) => { ... })`: Runs when the initial data for the subscription arrives.
  - `.OnError((errCtx, exception) => { ... })`: Runs if the subscription fails (e.g., invalid SQL).
- **Subscribe:** Call `.Subscribe(new string[] {"SELECT * FROM table_a", "SELECT * FROM table_b WHERE some_col > 10"})` with a list of query strings. This returns a `SubscriptionHandle`.
- **All Tables:** `.SubscribeToAllTables()` is a convenience for simple clients but cannot be easily unsubscribed.
- **Unsubscribing:** Use `handle.Unsubscribe()` or `handle.UnsubscribeThen((subCtx) => { ... })` to stop receiving updates for specific queries.

```csharp
using SpacetimeDB.Types; // For SubscriptionEventContext, ErrorContext
using System.Linq;

// In ClientManager or similar class...
private void SubscribeToTables(DbConnection conn)
{
    Console.WriteLine("Subscribing to tables...");
    conn.SubscriptionBuilder()
        .OnApplied(on_subscription_applied)
        .OnError((errCtx, err) => {
            Console.WriteLine($"Subscription failed: {err.Message}");
        })
        // Example: Subscribe to all rows from 'player' and 'message' tables
        .Subscribe(new string[] { "SELECT * FROM Player", "SELECT * FROM Message" });
}

private void OnSubscriptionApplied(SubscriptionEventContext ctx)
{
    Console.WriteLine("Subscription applied! Initial data received.");
    // Example: Print initial messages sorted by time
    var messages = ctx.Db.Message.Iter().ToList();
    messages.Sort((a, b) => a.Sent.CompareTo(b.Sent));
    foreach (var msg in messages)
    {
        // PrintMessage(ctx.Db, msg); // Assuming a PrintMessage helper
    }
}
```

#### 6. Accessing Cached Data & Handling Row Callbacks

Subscribed data is stored locally in the client cache, accessible via `ctx.Db` (where `ctx` can be a `DbConnection` or any event context like `EventContext`, `SubscriptionEventContext`).

- **Accessing Tables:** Use `ctx.Db.TableName` (e.g., `ctx.Db.Player`) to get a handle to a table's cache.
- **Iterating:** `tableHandle.Iter()` returns an `IEnumerable<RowType>` over all cached rows.
- **Filtering/Finding:** Use LINQ methods (`.Where()`, `.FirstOrDefault()`, etc.) on the result of `Iter()`, or use generated index accessors like `tableHandle.FindByPrimaryKeyField(pkValue)` or `tableHandle.FilterByIndexField(value)` for efficient lookups.
- **Row Callbacks:** Register callbacks using C# events to react to changes in the cache:
  - `tableHandle.OnInsert += (eventCtx, insertedRow) => { ... };`
  - `tableHandle.OnDelete += (eventCtx, deletedRow) => { ... };`
  - `tableHandle.OnUpdate += (eventCtx, oldRow, newRow) => { ... };` (Only for tables with a `[PrimaryKey]`)

```csharp
using SpacetimeDB.Types; // For EventContext, Event, Reducer
using System.Linq;

// In ClientManager or similar class...
private void RegisterEventCallbacks(DbConnection conn)
{
    conn.Db.Player.OnInsert += HandlePlayerInsert;
    conn.Db.Player.OnUpdate += HandlePlayerUpdate;
    conn.Db.Message.OnInsert += HandleMessageInsert;
    // Remember to unregister callbacks on disconnect/cleanup: -= HandlePlayerInsert;
}

private void HandlePlayerInsert(EventContext ctx, Player insertedPlayer)
{
    // Only react to updates caused by reducers, not initial subscription load
    if (ctx.Event is not Event<Reducer>.SubscribeApplied)
    {
        Console.WriteLine($"Player joined: {insertedPlayer.Name ?? "Unknown"}");
    }
}

private void HandlePlayerUpdate(EventContext ctx, Player oldPlayer, Player newPlayer)
{
    if (oldPlayer.Name != newPlayer.Name)
    {
        Console.WriteLine($"Player renamed: {oldPlayer.Name ?? "??"} -> {newPlayer.Name ?? "??"}");
    }
    // ... handle other changes like online status ...
}

private void HandleMessageInsert(EventContext ctx, Message insertedMessage)
{
    if (ctx.Event is not Event<Reducer>.SubscribeApplied)
    {
        // Find sender name from cache
        var sender = ctx.Db.Player.FindByPlayerId(insertedMessage.Sender);
        string senderName = sender?.Name ?? "Unknown";
        Console.WriteLine($"{senderName}: {insertedMessage.Text}");
    }
}
```

:::info Handling Initial Data vs. Live Updates in Callbacks
Callbacks like `OnInsert` and `OnUpdate` are triggered for both the initial data received when a subscription is first applied _and_ for subsequent live changes caused by reducers. If you need to differentiate (e.g., only react to _new_ messages, not the backlog), you can inspect the `ctx.Event` type. For example, checking `if (ctx.Event is not Event<Reducer>.SubscribeApplied) { ... }` ensures the code only runs for events triggered by reducers, not the initial subscription data load.
:::

#### 7. Invoking Reducers & Handling Reducer Callbacks

Clients trigger state changes by calling reducers defined in the server module.

- **Invoking:** Access generated static reducer methods via `SpacetimeDB.Types.Reducer.ReducerName(arg1, arg2, ...)`.
- **Reducer Callbacks:** Register callbacks using C# events to react to the _outcome_ of reducer calls:
  - `Reducer.OnReducerName += (reducerEventCtx, arg1, ...) => { ... };`
  - The `reducerEventCtx.Event` contains:
    - `Reducer`: The specific reducer variant record and its arguments.
    - `Status`: A tagged union record: `Status.Committed`, `Status.Failed(reason)`, or `Status.OutOfEnergy`.
    - `CallerIdentity`, `Timestamp`, etc.

```csharp
using SpacetimeDB.Types;

// In ClientManager or similar class, likely where HandleConnect is...
private void RegisterEventCallbacks(DbConnection conn) // Updated registration point
{
    // Table callbacks (from previous section)
    conn.Db.Player.OnInsert += HandlePlayerInsert;
    conn.Db.Player.OnUpdate += HandlePlayerUpdate;
    conn.Db.Message.OnInsert += HandleMessageInsert;

    // Reducer callbacks
    Reducer.OnSetName += HandleSetNameResult;
    Reducer.OnSendMessage += HandleSendMessageResult;
}

private void HandleSetNameResult(ReducerEventContext ctx, string name)
{
    // Check if the status is Failed
    if (ctx.Event.Status is Status.Failed failedStatus)
    {
        // Check if the failure was for *our* call
        if (ctx.Event.CallerIdentity == ctx.Identity) {
             Console.WriteLine($"Error setting name to '{name}': {failedStatus.Reason}");
        }
    }
}

private void HandleSendMessageResult(ReducerEventContext ctx, string text)
{
    if (ctx.Event.Status is Status.Failed failedStatus)
    {
        if (ctx.Event.CallerIdentity == ctx.Identity) { // Our call failed
             Console.WriteLine($"[Error] Failed to send message '{text}': {failedStatus.Reason}");
        }
    }
}

// Example of calling a reducer (e.g., from user input handler)
public void SendChatMessage(string message)
{
    if (!string.IsNullOrEmpty(message))
    {
        Reducer.SendMessage(message); // Static method call
    }
}

```

### Client SDK (TypeScript)

This section details how to build TypeScript/JavaScript client applications (for web browsers or Node.js) that interact with a SpacetimeDB module, using a framework-agnostic approach.

#### 1. Project Setup

Install the SDK package into your project:

```bash
# Using npm
npm install spacetimedb

# Or using pnpm
pnpm add spacetimedb

# Or using yarn
yarn add spacetimedb
```

#### 2. Generate Module Bindings

Generate the module-specific bindings using the `spacetime generate` command:

```bash
mkdir -p src/module_bindings
spacetime generate --lang typescript \
    --out-dir src/module_bindings \
    --project-path ../path/to/your/server_module
```

Import the necessary generated types and SDK components:

```typescript
// Import SDK core types
import { Identity, Status } from 'spacetimedb';
// Import generated connection class, event contexts, and table types
import {
  DbConnection,
  EventContext,
  ReducerEventContext,
  Message,
  User,
} from './module_bindings';
// Reducer functions are accessed via conn.reducers
```

#### 3. Connecting to the Database

Use the generated `DbConnection` class and its builder pattern to establish a connection.

```typescript
import {
  DbConnection,
  EventContext,
  ReducerEventContext,
  Message,
  User,
} from './module_bindings';
import { Identity, Status } from 'spacetimedb';

const HOST = 'ws://localhost:3000';
const DB_NAME = 'quickstart-chat';
const CREDS_KEY = 'auth_token';

class ChatClient {
  public conn: DbConnection | null = null;
  public identity: Identity | null = null;
  public connected: boolean = false;
  // Client-side cache for user lookups
  private userMap: Map<string, User> = new Map();

  constructor() {
    // Bind methods to ensure `this` is correct in callbacks
    this.handleConnect = this.handleConnect.bind(this);
    this.handleDisconnect = this.handleDisconnect.bind(this);
    this.handleConnectError = this.handleConnectError.bind(this);
    this.registerTableCallbacks = this.registerTableCallbacks.bind(this);
    this.registerReducerCallbacks = this.registerReducerCallbacks.bind(this);
    this.subscribeToTables = this.subscribeToTables.bind(this);
    this.handleMessageInsert = this.handleMessageInsert.bind(this);
    this.handleUserInsert = this.handleUserInsert.bind(this);
    this.handleUserUpdate = this.handleUserUpdate.bind(this);
    this.handleUserDelete = this.handleUserDelete.bind(this);
    this.handleSendMessageResult = this.handleSendMessageResult.bind(this);
  }

  public connect() {
    console.log('Attempting to connect...');
    const token = localStorage.getItem(CREDS_KEY) || null;

    const connectionInstance = DbConnection.builder()
      .withUri(HOST)
      .withModuleName(DB_NAME)
      .withToken(token)
      .onConnect(this.handleConnect)
      .onDisconnect(this.handleDisconnect)
      .onConnectError(this.handleConnectError)
      .build();

    this.conn = connectionInstance;
  }

  private handleConnect(conn: DbConnection, identity: Identity, token: string) {
    this.identity = identity;
    this.connected = true;
    localStorage.setItem(CREDS_KEY, token); // Save new/refreshed token
    console.log('Connected with identity:', identity.toHexString());

    // Register callbacks and subscribe now that we are connected
    this.registerTableCallbacks();
    this.registerReducerCallbacks();
    this.subscribeToTables();
  }

  private handleDisconnect() {
    console.log('Disconnected');
    this.connected = false;
    this.identity = null;
    this.conn = null;
    this.userMap.clear(); // Clear local cache on disconnect
  }

  private handleConnectError(err: Error) {
    console.error('Connection Error:', err);
    localStorage.removeItem(CREDS_KEY); // Clear potentially invalid token
    this.conn = null; // Ensure connection is marked as unusable
  }

  // Placeholder implementations for callback registration and subscription
  private registerTableCallbacks() {
    /* See Section 6 */
  }
  private registerReducerCallbacks() {
    /* See Section 7 */
  }
  private subscribeToTables() {
    /* See Section 5 */
  }

  // Placeholder implementations for table callbacks
  private handleMessageInsert(ctx: EventContext | undefined, message: Message) {
    /* See Section 6 */
  }
  private handleUserInsert(ctx: EventContext | undefined, user: User) {
    /* See Section 6 */
  }
  private handleUserUpdate(
    ctx: EventContext | undefined,
    oldUser: User,
    newUser: User
  ) {
    /* See Section 6 */
  }
  private handleUserDelete(ctx: EventContext, user: User) {
    /* See Section 6 */
  }

  // Placeholder for reducer callback
  private handleSendMessageResult(
    ctx: ReducerEventContext,
    messageText: string
  ) {
    /* See Section 7 */
  }

  // Public methods for interaction
  public sendChatMessage(message: string) {
    /* See Section 7 */
  }
  public setPlayerName(newName: string) {
    /* See Section 7 */
  }
}

// Example Usage:
// const client = new ChatClient();
// client.connect();
```

#### 4. Managing the Connection Loop

The TypeScript SDK is event-driven. No manual `FrameTick()` is needed.

#### 5. Subscribing to Data

Subscribe to SQL queries to receive data.

```typescript
// Part of the ChatClient class
private subscribeToTables() {
    if (!this.conn) return;

    const queries = ["SELECT * FROM message", "SELECT * FROM user"];

    console.log("Subscribing...");
    this.conn
        .subscriptionBuilder()
        .onApplied(() => {
            console.log(`Subscription applied for: ${queries}`);
            // Initial cache is now populated, process initial data if needed
            this.processInitialCache();
        })
        .onError((error: Error) => {
            console.error(`Subscription error:`, error);
        })
        .subscribe(queries);
}

private processInitialCache() {
    if (!this.conn) return;
    console.log("Processing initial cache...");
    // Populate userMap from initial cache
    this.userMap.clear();
    for (const user of this.conn.db.User.iter()) {
        this.handleUserInsert(undefined, user); // Pass undefined context for initial load
    }
    // Process initial messages, e.g., sort and display
    const initialMessages = Array.from(this.conn.db.Message.iter());
    initialMessages.sort((a, b) => a.sent.getTime() - b.sent.getTime());
    for (const message of initialMessages) {
        this.handleMessageInsert(undefined, message); // Pass undefined context
    }
}
```

#### 6. Accessing Cached Data & Handling Row Callbacks

Maintain your own collections (e.g., `Map`) updated via table callbacks for efficient lookups.

```typescript
// Part of the ChatClient class
private registerTableCallbacks() {
    if (!this.conn) return;

    this.conn.db.Message.onInsert(this.handleMessageInsert);

    // User table callbacks update the local userMap
    this.conn.db.User.onInsert(this.handleUserInsert);
    this.conn.db.User.onUpdate(this.handleUserUpdate);
    this.conn.db.User.onDelete(this.handleUserDelete);

    // Note: In a real app, you might return a cleanup function
    // to unregister these if the ChatClient is destroyed.
    // e.g., return () => { this.conn?.db.Message.removeOnInsert(...) };
}

private handleMessageInsert(ctx: EventContext | undefined, message: Message) {
    const identityStr = message.sender.toHexString();
    // Look up sender in our local map
    const sender = this.userMap.get(identityStr);
    const senderName = sender?.name ?? identityStr.substring(0, 8);

    if (ctx) { // Live update
        console.log(`LIVE MSG: ${senderName}: ${message.text}`);
        // TODO: Update UI (e.g., add to message list)
    } else { // Initial load (handled in processInitialCache)
        // console.log(`Initial MSG loaded: ${message.text} from ${senderName}`);
    }
}

private handleUserInsert(ctx: EventContext | undefined, user: User) {
    const identityStr = user.identity.toHexString();
    this.userMap.set(identityStr, user);
    const name = user.name ?? identityStr.substring(0, 8);
    if (ctx) { // Live update
        if (user.online) console.log(`${name} connected.`);
    } else { // Initial load
        // console.log(`Loaded user: ${name} (Online: ${user.online})`);
    }
    // TODO: Update UI (e.g., user list)
}

private handleUserUpdate(ctx: EventContext | undefined, oldUser: User, newUser: User) {
    const oldIdentityStr = oldUser.identity.toHexString();
    const newIdentityStr = newUser.identity.toHexString();
    if(oldIdentityStr !== newIdentityStr) {
       this.userMap.delete(oldIdentityStr);
    }
    this.userMap.set(newIdentityStr, newUser);

    const name = newUser.name ?? newIdentityStr.substring(0, 8);
    if (ctx) { // Live update
         if (!oldUser.online && newUser.online) console.log(`${name} connected.`);
         else if (oldUser.online && !newUser.online) console.log(`${name} disconnected.`);
         else if (oldUser.name !== newUser.name) console.log(`Rename: ${oldUser.name ?? '...'} -> ${name}.`);
    }
    // TODO: Update UI (e.g., user list, messages from this user)
}

private handleUserDelete(ctx: EventContext, user: User) {
     const identityStr = user.identity.toHexString();
     const name = user.name ?? identityStr.substring(0, 8);
     this.userMap.delete(identityStr);
     console.log(`${name} left/deleted.`);
     // TODO: Update UI
}
```

:::info Handling Initial Data vs. Live Updates in Callbacks
In TypeScript, the first argument (`ctx: EventContext | undefined`) to row callbacks indicates the cause. If `ctx` is defined, it's a live update. If `undefined`, it's part of the initial subscription load.
:::

#### 7. Invoking Reducers & Handling Reducer Callbacks

Call reducers via `conn.reducers`. Register callbacks via `conn.reducers.onReducerName(...)` to observe outcomes.

- **Invoking:** Access generated reducer functions via `conn.reducers.reducerName(arg1, arg2, ...)`. Calling these functions sends the request to the server.
- **Reducer Callbacks:** Register callbacks using `conn.reducers.onReducerName((ctx: ReducerEventContext, arg1, ...) => { ... })` to react to the _outcome_ of reducer calls initiated by _any_ client (including your own).
- **ReducerEventContext (`ctx`)**: Contains information about the completed reducer call:
  - `ctx.event.reducer`: The specific reducer variant record and its arguments.
  - `ctx.event.status`: An object indicating the outcome. Check `ctx.event.status.tag` which will be a string like `"Committed"` or `"Failed"`. If failed, the reason is typically in `ctx.event.status.value`.
  - `ctx.event.callerIdentity`: The `Identity` of the client that originally invoked the reducer.
  - `ctx.event.message`: Contains the failure message if `ctx.event.status.tag === "Failed"`.
  - `ctx.event.timestamp`, etc.

```typescript
// Part of the ChatClient class
private registerReducerCallbacks() {
    if (!this.conn) return;

    this.conn.reducers.onSendMessage(this.handleSendMessageResult);
    // Register other reducer callbacks if needed
    // this.conn.reducers.onSetName(handleSetNameResult);

    // Note: Consider returning a cleanup function to unregister
}

private handleSendMessageResult(ctx: ReducerEventContext, messageText: string) {
    // Check if this callback corresponds to a call made by this client instance
    const wasOurCall = ctx.event.callerIdentity.isEqual(this.identity);
    if (!wasOurCall) return; // Optional: Only react to your own calls

    switch(ctx.event.status.tag) {
    case "Committed":
        console.log(`Our message "${messageText}" sent successfully.`);
        break;
    case "Failed":
        // Access the error message via status.value or event.message
        const errorMessage = ctx.event.status.value || ctx.event.message || "Unknown error";
        console.error(`Failed to send "${messageText}": ${errorMessage}`);
        break;
    case "OutOfEnergy":
        console.error(`Failed to send "${messageText}": Out of Energy!`);
        break;
    }
}

// Public methods to be called from application logic
public sendChatMessage(message: string) {
    if (this.conn && this.connected && message.trim()) {
        this.conn.reducers.sendMessage(message);
    }
}

public setPlayerName(newName: string) {
    if (this.conn && this.connected && newName.trim()) {
        this.conn.reducers.setName(newName);
    }
}
```

## SpacetimeDB Subscription Semantics

This document describes the subscription semantics maintained by the SpacetimeDB host over WebSocket connections. These semantics outline message ordering guarantees, subscription handling, transaction updates, and client cache consistency.

### WebSocket Communication Channels

A single WebSocket connection between a client and the SpacetimeDB host consists of two distinct message channels:

- **Client → Server:** Sends requests such as reducer invocations and subscription queries.
- **Server → Client:** Sends responses to client requests and database transaction updates.

#### Ordering Guarantees

The server maintains the following guarantees:

1. **Sequential Response Ordering:**
   - Responses to client requests are always sent back in the same order the requests were received. If request A precedes request B, the response to A will always precede the response to B, even if A takes longer to process.

2. **Atomic Transaction Updates:**
   - Each database transaction (e.g., reducer invocation, INSERT, UPDATE, DELETE queries) generates exactly zero or one update message sent to clients. These updates are atomic and reflect the exact order of committed transactions.

3. **Atomic Subscription Initialization:**
   - When subscriptions are established, clients receive exactly one response containing all initially matching rows from a consistent database state snapshot taken between two transactions.
   - The state snapshot reflects a committed database state that includes all previous transaction updates received and excludes all future transaction updates.

### Subscription Workflow

When invoking `SubscriptionBuilder::subscribe(QUERIES)` from the client SDK:

1. **Client SDK → Host:**
   - Sends a `Subscribe` message containing the specified QUERIES.

2. **Host Processing:**
   - Captures a snapshot of the committed database state.
   - Evaluates QUERIES against this snapshot to determine matching rows.

3. **Host → Client SDK:**
   - Sends a `SubscribeApplied` message containing the matching rows.

4. **Client SDK Processing:**
   - Receives and processes the message.
   - Locks the client cache and inserts all rows atomically.
   - Invokes relevant callbacks:
     - `on_insert` callback for each row.
     - `on_applied` callback for the subscription.

> **Note:** No relative ordering guarantees are made regarding the invocation order of these callbacks.

### Transaction Update Workflow

Upon committing a database transaction:

1. **Host Evaluates State Delta:**
   - Calculates the state delta (inserts and deletes) resulting from the transaction.

2. **Host Evaluates Queries:**
   - Computes the incremental query updates relevant to subscribed clients.

3. **Host → Client SDK:**
   - Sends a `TransactionUpdate` message if relevant updates exist, containing affected rows and transaction metadata.

4. **Client SDK Processing:**
   - Receives and processes the message.
   - Locks the client cache, applying deletions and insertions atomically.
   - Invokes relevant callbacks:
     - `on_insert`, `on_delete`, `on_update`, and `on_reducer` as necessary.

> **Note:**

- No relative ordering guarantees are made regarding the invocation order of these callbacks.
- Delete and insert operations within a `TransactionUpdate` have no internal order guarantees and are grouped into operation maps.

#### Client Updates and Compute Processing

Client SDKs must explicitly request processing time (e.g., `conn.FrameTick()` in C# or `conn.run_threaded()` in Rust) to receive and process messages. Until such a processing call is made, messages remain queued on the server-to-client channel.

### Multiple Subscription Sets

If multiple subscription sets are active, updates across these sets are bundled together into a single `TransactionUpdate` message.

### Client Cache Guarantees

- The client cache always maintains a consistent and correct subset of the committed database state.
- Callback functions invoked due to events have guaranteed visibility into a fully updated cache state.
- Reads from the client cache are effectively free as they access locally cached data.
- During callback execution, the client cache accurately reflects the database state immediately following the event-triggering transaction.

#### Pending Callbacks and Cache Consistency

Callbacks (`pendingCallbacks`) are queued and deferred until the cache updates (inserts/deletes) from a transaction are fully applied. This ensures all callbacks see the fully consistent state of the cache, preventing callbacks from observing an inconsistent intermediate state.
