# Rust Client SDK Quick Start

In this guide we'll show you how to get up and running with a simple SpacetimDB app with a client written in Rust.

We'll implement a command-line client for the module created in our Rust or C# Module Quickstart guides. Make sure you follow one of these guides before you start on this one.

## Project structure

Enter the directory `quickstart-chat` you created in the [Rust Module Quickstart](/docs/modules/rust/quickstart) or [C# Module Quickstart](/docs/modules/c-sharp/quickstart) guides:

```bash
cd quickstart-chat
```

Within it, create a `client` crate, our client application, which users run locally:

```bash
cargo new client
```

## Depend on `spacetimedb-sdk` and `hex`

`client/Cargo.toml` should be initialized without any dependencies. We'll need two:

- [`spacetimedb-sdk`](https://crates.io/crates/spacetimedb-sdk), which defines client-side interfaces for interacting with a remote SpacetimeDB module.
- [`hex`](https://crates.io/crates/hex), which we'll use to print unnamed users' identities as hexadecimal strings.

Below the `[dependencies]` line in `client/Cargo.toml`, add:

```toml
spacetimedb-sdk = "0.12"
hex = "0.4"
```

Make sure you depend on the same version of `spacetimedb-sdk` as is reported by the SpacetimeDB CLI tool's `spacetime version`!

## Clear `client/src/main.rs`

`client/src/main.rs` should be initialized with a trivial "Hello world" program. Clear it out so we can write our chat client.

In your `quickstart-chat` directory, run:

```bash
rm client/src/main.rs
touch client/src/main.rs
```

## Generate your module types

The `spacetime` CLI's `generate` command will generate client-side interfaces for the tables, reducers and types referenced by tables or reducers defined in your server module.

In your `quickstart-chat` directory, run:

```bash
mkdir -p client/src/module_bindings
spacetime generate --lang rust --out-dir client/src/module_bindings --project-path server
```

Take a look inside `client/src/module_bindings`. The CLI should have generated a few files:

```
module_bindings
├── message_table.rs
├── message_type.rs
├── mod.rs
├── send_message_reducer.rs
├── set_name_reducer.rs
├── user_table.rs
└── user_type.rs
```

To use these, we'll declare the module in our client crate and import its definitions.

In `client/src/main.rs`, add:

```rust
mod module_bindings;
use module_bindings::*;
```

## Add more imports

We'll need additional imports from `spacetimedb_sdk` for interacting with the database, handling credentials, and managing events.

In `client/src/main.rs`, add:

```rust
use spacetimedb_sdk::{anyhow, DbContext, Event, Identity, Status, Table, TableWithPrimaryKey};
use spacetimedb_sdk::credentials::File;
```

## Define the main function

Our `main` function will do the following:
1. Connect to the database.
2. Spawn a thread to handle network messages.
3. Handle user input from the command line.

To `client/src/main.rs`, add:

```rust
fn main() {
    // Connect to the database
    let conn = connect_to_db();
    // Let SpacetimeDB spawn a thread and handle network messages
    conn.run_threaded();
    // Handle CLI input
    user_input_loop(&conn);
}
```


## Register callbacks

We'll define several callbacks to handle database events such as user connection, disconnection, or receiving new messages. These callbacks will be used to register our application's response to these events.

To `client/src/main.rs`, add:

```rust
/// Register all the callbacks our app will use to respond to database events.
fn register_callbacks(conn: &DbConnection) {
    // When a new user joins, print a notification.
    conn.db.user().on_insert(on_user_inserted);

    // When a user's status changes, print a notification.
    conn.db.user().on_update(on_user_updated);

    // When a new message is received, print it.
    conn.db.message().on_insert(on_message_inserted);

    // When we receive the message backlog, print it in timestamp order.
    conn.subscription_builder().on_applied(on_sub_applied);

    // When we fail to set our name, print a warning.
    conn.reducers.on_set_name(on_name_set);

    // When we fail to send a message, print a warning.
    conn.reducers.on_send_message(on_message_sent);
}
```

## Save credentials

Each user has a set of credentials, including a unique identity and a token. We'll save credentials to a file and load them on subsequent connections.

To `client/src/main.rs`, add:

```rust
/// Our `on_connect` callback: save our credentials to a file.
fn on_connected(conn: &DbConnection, ident: Identity, token: &str) {
    let file = File::new(CREDS_NAME);
    if let Err(e) = file.save(ident, token) {
        eprintln!("Failed to save credentials: {:?}", e);
    }

    println!("Connected to SpacetimeDB.");
    println!("Use /name to set your username, otherwise enter your message!");

    // Subscribe to the data we care about
    subscribe_to_tables(&conn);
    // Register callbacks for reducers
    register_callbacks(&conn);
}
```

## Handle errors and disconnections

We need to handle connection errors and disconnections by printing appropriate messages and exiting the program.

To `client/src/main.rs`, add:

```rust
/// Our `on_connect_error` callback: print the error, then exit the process.
fn on_connect_error(err: &anyhow::Error) {
    eprintln!("Connection error: {:?}", err);
}

/// Our `on_disconnect` callback: print a note, then exit the process.
fn on_disconnected(_conn: &DbConnection, _err: Option<&anyhow::Error>) {
    eprintln!("Disconnected!");
    std::process::exit(0)
}
```

## Define user-related event handlers

To handle users joining, leaving, or changing names, we'll define functions that respond to these events.

To `client/src/main.rs`, add:

```rust
/// Our `User::on_insert` callback:
/// if the user is online, print a notification.
fn on_user_inserted(_ctx: &EventContext, user: &User) {
    if user.online {
        println!("User {} connected.", user_name_or_identity(user));
    }
}

fn user_name_or_identity(user: &User) -> String {
    user.name
        .clone()
        .unwrap_or_else(|| identity_leading_hex(&user.identity))
}

fn identity_leading_hex(id: &Identity) -> String {
    id.to_hex().to_string()
}
```


## Define message-related event handlers

We'll handle message-related events, such as receiving new messages or loading past messages.

To `client/src/main.rs`, add:

```rust
/// Our `Message::on_insert` callback: print new messages.
fn on_message_inserted(ctx: &EventContext, message: &Message) {
    if let Event::Reducer(_) = ctx.event {
        print_message(ctx, message)
    }
}

fn print_message(ctx: &EventContext, message: &Message) {
    let sender = ctx.db.user().identity().find(&message.sender.clone())
        .map(|u| user_name_or_identity(&u))
        .unwrap_or_else(|| "unknown".to_string());
    println!("{}: {}", sender, message.text);
}

/// Our `on_subscription_applied` callback:
/// sort all past messages and print them in timestamp order.
fn on_sub_applied(ctx: &EventContext) {
    let mut messages = ctx.db.message().iter().collect::<Vec<_>>();
    messages.sort_by_key(|m| m.sent);
    for message in messages {
        print_message(ctx, &message);
    }
}
```

## Handle reducer failures

We need to handle failures when reducers like `set_name` or `send_message` fail to execute.

To `client/src/main.rs`, add:

```rust
/// Our `on_set_name` callback: print a warning if the reducer failed.
fn on_name_set(ctx: &EventContext, name: &String) {
    if let Event::Reducer(reducer) = &ctx.event {
        if let Status::Failed(err) = reducer.status.clone() {
            eprintln!("Failed to change name to {:?}: {}", name, err);
        }
    }
}

/// Our `on_send_message` callback: print a warning if the reducer failed.
fn on_message_sent(ctx: &EventContext, text: &String) {
    if let Event::Reducer(reducer) = &ctx.event {
        if let Status::Failed(err) = reducer.status.clone() {
            eprintln!("Failed to send message {:?}: {}", text, err);
        }
    }
}
```

## Connect to the database

Now that our callbacks are all set up, we can connect to the database. We'll store the URI of the SpacetimeDB instance and our module name in constants `SPACETIMEDB_URI` and `DB_NAME`. Replace `<module-name>` with the name you chose when publishing your module during the module quickstart.

To `client/src/main.rs`, add:

```rust
/// The URL of the SpacetimeDB instance hosting our chat module.
const SPACETIMEDB_URI: &str = "http://localhost:3000";

/// The module name we chose when we published our module.
const DB_NAME: &str = "<module-name>";

/// Load credentials from a file and connect to the database.
fn connect_to_db() -> DbConnection {
    let credentials = File::new(CREDS_NAME);
    DbConnection::builder()
        .on_connect(on_connected)
        .on_connect_error(on_connect_error)
        .on_disconnect(on_disconnected)
        .with_uri(SPACETIMEDB_URI)
        .with_module_name(DB_NAME)
        .with_credentials(credentials.load().unwrap())
        .build().expect("Failed to connect")
}
```


## Subscribe to queries

SpacetimeDB is set up so that each client subscribes to some subset of the database using SQL queries, and is notified about changes only to that subset. In our app, we subscribe to the entire `User` and `Message` tables.

To `client/src/main.rs`, add:

```rust
/// Register subscriptions for all rows of both tables.
fn subscribe_to_tables(conn: &DbConnection) {
    conn.subscription_builder().subscribe([
        "SELECT * FROM user;",
        "SELECT * FROM message;",
    ]);
}
```

## Handle user input

Our app should allow the user to interact by typing lines into their terminal. If the line starts with `/name `, we'll change the user's name. Any other line will send a message.

The functions `set_name` and `send_message` are autogenerated from the server module. We pass them a `String`, which gets sent to the server to execute the corresponding reducer.

To `client/src/main.rs`, add:

```rust
/// Read each line of standard input, and either set our name or send a message as appropriate.
fn user_input_loop(conn: &DbConnection) {
    for line in std::io::stdin().lines() {
        let Ok(line) = line else {
            panic!("Failed to read from stdin.");
        };
        if let Some(name) = line.strip_prefix("/name ") {
            conn.reducers.set_name(name.to_string()).unwrap();
        } else {
            conn.reducers.send_message(line).unwrap();
        }
    }
}
```

## Run it

After setting everything up, change your directory to the client app, then compile and run it. From the `quickstart-chat` directory, run:

```bash
cd client
cargo run
```

You should see something like:

```
User d9e25c51996dea2f connected.
```

Now try sending a message by typing `Hello, world!` and pressing enter. You should see:

```
d9e25c51996dea2f: Hello, world!
```

Next, set your name by typing `/name <my-name>`, replacing `<my-name>` with your desired username. You should see:

```
User d9e25c51996dea2f renamed to <my-name>.
```

Then, send another message:

```
<my-name>: Hello after naming myself.
```

Now, close the app by hitting `Ctrl+C`, and start it again with `cargo run`. You'll see yourself connecting, and your past messages will load in order:

```
User <my-name> connected.
<my-name>: Hello, world!
<my-name>: Hello after naming myself.
```

## What's next?

You can find the full code for this client [in the Rust SDK's examples](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/sdk/examples/quickstart-chat).

Check out the [Rust SDK Reference](/docs/sdks/rust) for a more comprehensive view of the SpacetimeDB Rust SDK.

Our basic terminal interface has some limitations. Incoming messages can appear while the user is typing, which is less than ideal. Additionally, the user's input gets mixed with the program's output, making messages the user sends appear twice. You might want to try improving the interface by using [Rustyline](https://crates.io/crates/rustyline), [Cursive](https://crates.io/crates/cursive), or even creating a full-fledged GUI.

We've tried using Cursive for the interface, and you can check out our implementation in the [Rust SDK's examples](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/sdk/examples/cursive-chat).

Once your chat server runs for a while, you might want to limit the messages your client loads by refining your `Message` subscription query, only subscribing to messages sent within the last half-hour.

You could also add features like:

- Styling messages by interpreting HTML tags and printing appropriate [ANSI escapes](https://en.wikipedia.org/wiki/ANSI_escape_code).
- Adding a `moderator` flag to the `User` table, allowing moderators to manage users (e.g., time-out, ban).
- Adding rooms or channels that users can join or leave.
- Supporting direct messages or displaying user statuses next to their usernames.

