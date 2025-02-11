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
spacetimedb-sdk = "1.0"
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
module_bindings/
├── identity_connected_reducer.rs
├── identity_disconnected_reducer.rs
├── message_table.rs
├── message_type.rs
├── mod.rs
├── send_message_reducer.rs
├── set_name_reducer.rs
├── user_table.rs
└── user_type.rs
```

To use these, we'll declare the module in our client crate and import its definitions.

To `client/src/main.rs`, add:

```rust
mod module_bindings;
use module_bindings::*;
```

## Add more imports

We'll need additional imports from `spacetimedb_sdk` for interacting with the database, handling credentials, and managing events.

To `client/src/main.rs`, add:

```rust
use spacetimedb_sdk::credentials::File;
use spacetimedb_sdk::{DbContext, Error, Event, Identity, Status, Table, TableWithPrimaryKey};
```

## Define the main function

Our `main` function will do the following:
1. Connect to the database.
2. Register a number of callbacks to run in response to various database events.
3. Subscribe to a set of SQL queries, whose results will be replicated and automatically updated in our client.
4. Spawn a background thread where our connection will process messages and invoke callbacks.
5. Enter a loop to handle user input from the command line.

We'll see the implementation of these functions a bit later, but for now add to `client/src/main.rs`:

```rust
fn main() {
    // Connect to the database
    let ctx = connect_to_db();

    // Register callbacks to run in response to database events.
    register_callbacks(&ctx);

    // Subscribe to SQL queries in order to construct a local partial replica of the database.
    subscribe_to_tables(&ctx);

    // Spawn a thread, where the connection will process messages and invoke callbacks.
    ctx.run_threaded();

    // Handle CLI input
    user_input_loop(&ctx);
}
```

## Register callbacks

We need to handle several sorts of events:

1. When we connect and receive our credentials, we'll save them to a file so that the next time we connect, we can re-authenticate as the same user.
2. When a new user joins, we'll print a message introducing them.
3. When a user is updated, we'll print their new name, or declare their new online status.
4. When we receive a new message, we'll print it.
5. When we're informed of the backlog of past messages, we'll sort them and print them in order.
6. If the server rejects our attempt to set our name, we'll print an error.
7. If the server rejects a message we send, we'll print an error.
8. When our connection ends, we'll print a note, then exit the process.

To `client/src/main.rs`, add:

```rust
/// Register all the callbacks our app will use to respond to database events.
fn register_callbacks(ctx: &DbConnection) {
    // When a new user joins, print a notification.
    ctx.db.user().on_insert(on_user_inserted);

    // When a user's status changes, print a notification.
    ctx.db.user().on_update(on_user_updated);

    // When a new message is received, print it.
    ctx.db.message().on_insert(on_message_inserted);

    // When we receive the message backlog, print it in timestamp order.
    ctx.subscription_builder().on_applied(on_sub_applied);

    // When we fail to set our name, print a warning.
    ctx.reducers.on_set_name(on_name_set);

    // When we fail to send a message, print a warning.
    ctx.reducers.on_send_message(on_message_sent);
}
```

## Save credentials

TODO: Revise this section.

Each user has a `Credentials`, which consists of two parts:

- An `Identity`, a unique public identifier. We're using these to identify `User` rows.
- A `Token`, a private key which SpacetimeDB uses to authenticate the client.

`Credentials` are generated by SpacetimeDB each time a new client connects, and sent to the client so they can be saved, in order to re-connect with the same identity. The Rust SDK provides a pair of functions in `File`, `save` and `load`, for saving and storing these credentials in a file. By default the `save` and `load` will look for credentials in the `$HOME/.spacetimedb_client_credentials/` directory, which should be unintrusive. If saving our credentials fails, we'll print a message to standard error, but otherwise continue normally; even though the user won't be able to reconnect with the same identity, they can still chat normally.

To `client/src/main.rs`, add:

```rust
fn creds_store() -> credentials::File {
    credentials::File::new("quickstart-chat")
}

/// Our `on_connect` callback: save our credentials to a file.
fn on_connected(_ctx: &DbConnection, _identity: Identity, token: &str) {
    if let Err(e) = creds_store().save(token) {
        eprintln!("Failed to save credentials: {:?}", e);
    }
}
```

## Handle errors and disconnections

We need to handle connection errors and disconnections by printing appropriate messages and exiting the program.

To `client/src/main.rs`, add:

```rust
/// Our `on_connect_error` callback: print the error, then exit the process.
fn on_connect_error(err: &Error) {
    eprintln!("Connection error: {:?}", err);
}

/// Our `on_disconnect` callback: print a note, then exit the process.
fn on_disconnected(_ctx: &ErrorContext) {
    eprintln!("Disconnected!");
    std::process::exit(0)
}
```

## Notify about new users

For each table, we can register on-insert and on-delete callbacks to be run whenever a subscribed row is inserted or deleted. We register these callbacks using the `on_insert` and `on_delete`, which is automatically implemented for each table by `spacetime generate`.

These callbacks can fire in two contexts:

- After a reducer runs, when the client's cache is updated about changes to subscribed rows.
- After calling `subscribe`, when the client's cache is initialized with all existing matching rows.

This second case means that, even though the module only ever inserts online users, the client's `conn.db.user().on_insert(..)` callbacks may be invoked with users who are offline. We'll only notify about online users.

`on_insert` and `on_delete` callbacks take two arguments: `&EventContext` and the row data (in the case of insert it's a new row and in the case of delete it's the row that was deleted). You can determine whether the insert/delete operation was caused by a reducer or subscription update by checking the type of `ctx.event`. If `ctx.event` is a `Event::Reducer` then the row was changed by a reducer call, otherwise it was modified by a subscription update. `Reducer` is an enum autogenerated by `spacetime generate` with a variant for each reducer defined by the module. For now, we can ignore this argument.

Whenever we want to print a user, if they have set a name, we'll use that. If they haven't set a name, we'll instead print the first 8 bytes of their identity, encoded as hexadecimal. We'll define functions `user_name_or_identity` and `identity_leading_hex` to handle this.

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
        .unwrap_or_else(|| user.identity.to_hex().to_string())
}
```

### Notify about updated users

Because we declared a `#[primary_key]` column in our `User` table, we can also register on-update callbacks. These run whenever a row is replaced by a row with the same primary key, like our module's `ctx.db.user().identity().update(..) calls. We register these callbacks using the `on_update` method of the trait `TableWithPrimaryKey`, which is automatically implemented by `spacetime generate` for any table with a `#[primary_key]` column.

`on_update` callbacks take three arguments: the old row, the new row, and an `Option<&ReducerEvent>`.

In our module, users can be updated for three reasons:

1. They've set their name using the `set_name` reducer.
2. They're an existing user re-connecting, so their `online` has been set to `true`.
3. They've disconnected, so their `online` has been set to `false`.

We'll print an appropriate message in each of these cases.

To `client/src/main.rs`, add:

```rust
/// Our `User::on_update` callback:
/// print a notification about name and status changes.
fn on_user_updated(_ctx: &EventContext, old: &User, new: &User) {
    if old.name != new.name {
        println!(
            "User {} renamed to {}.",
            user_name_or_identity(old),
            user_name_or_identity(new)
        );
    }
    if old.online && !new.online {
        println!("User {} disconnected.", user_name_or_identity(new));
    }
    if !old.online && new.online {
        println!("User {} connected.", user_name_or_identity(new));
    }
}
```

## Print messages

TODO: Describe `RemoteDbContext`.

When we receive a new message, we'll print it to standard output, along with the name of the user who sent it. Keep in mind that we only want to do this for new messages, i.e. those inserted by a `send_message` reducer invocation. We have to handle the backlog we receive when our subscription is initialized separately, to ensure they're printed in the correct order. To that effect, our `on_message_inserted` callback will check if the ctx.event type is an `Event::Reducer`, and only print in that case.

To find the `User` based on the message's `sender` identity, we'll use `ctx.db.user().identity().find(..)`, which behaves like the same function on the server.

We'll print the user's name or identity in the same way as we did when notifying about `User` table events, but here we have to handle the case where we don't find a matching `User` row. This can happen when the module owner sends a message using the CLI's `spacetime call`. In this case, we'll print `unknown`.

We'll handle message-related events, such as receiving new messages or loading past messages.

To `client/src/main.rs`, add:

```rust
/// Our `Message::on_insert` callback: print new messages.
fn on_message_inserted(ctx: &EventContext, message: &Message) {
    if let Event::Reducer(_) = ctx.event {
        print_message(ctx, message)
    }
}

fn print_message(ctx: &impl RemoteDbContext, message: &Message) {
    let sender = ctx
        .db()
        .user()
        .identity()
        .find(&message.sender.clone())
        .map(|u| user_name_or_identity(&u))
        .unwrap_or_else(|| "unknown".to_string());
    println!("{}: {}", sender, message.text);
}
```

### Print past messages in order

Messages we receive live will come in order, but when we connect, we'll receive all the past messages at once. We can't just print these in the order we receive them; the logs would be all shuffled around, and would make no sense. Instead, when we receive the log of past messages, we'll sort them by their sent timestamps and print them in order.


We'll handle this in our function `print_messages_in_order`, which we registered as an `on_subscription_applied` callback. `print_messages_in_order` iterates over all the `Message`s we've received, sorts them, and then prints them. `Message::iter()` is defined on the trait `TableType`, and returns an iterator over all the messages in the client's cache. Rust iterators can't be sorted in-place, so we'll collect it to a `Vec`, then use the `sort_by_key` method to sort by timestamp.

To `client/src/main.rs`, add:

```rust
/// Our `on_subscription_applied` callback:
/// sort all past messages and print them in timestamp order.
fn on_sub_applied(ctx: &SubscriptionEventContext) {
    let mut messages = ctx.db.message().iter().collect::<Vec<_>>();
    messages.sort_by_key(|m| m.sent);
    for message in messages {
        print_message(ctx, &message);
    }
    println!("Fully connected and all subscriptions applied.");
    println!("Use /name to set your name, or type a message!");
}
```

## Handle reducer failures

We can also register callbacks to run each time a reducer is invoked. We register these callbacks using the `on_reducer` method of the `Reducer` trait, which is automatically implemented for each reducer by `spacetime generate`.

Each reducer callback first takes an `&EventContext` which contains all of the information from the reducer call including the reducer arguments, the identity of the caller, and whether or not the reducer call suceeded.

These callbacks will be invoked in one of two cases:

1. If the reducer was successful and altered any of our subscribed rows.
2. If we requested an invocation which failed.

Note that a status of `Failed` or `OutOfEnergy` implies that the caller identity is our own identity.

We already handle successful `set_name` invocations using our `ctx.db.user().on_update(..)` callback, but if the module rejects a user's chosen name, we'd like that user's client to let them know. We define a function `on_set_name` as a `conn.reducers.on_set_name(..)` callback which checks if the reducer failed, and if it did, prints a message including the rejected name and the error.


To `client/src/main.rs`, add:

```rust
/// Our `on_set_name` callback: print a warning if the reducer failed.
fn on_name_set(ctx: &ReducerEventContext, name: &String) {
    if let Status::Failed(err) = &ctx.event.status {
        eprintln!("Failed to change name to {:?}: {}", name, err);
    }
}

/// Our `on_send_message` callback: print a warning if the reducer failed.
fn on_message_sent(ctx: &ReducerEventContext, text: &String) {
    if let Status::Failed(err) = &ctx.event.status {
        eprintln!("Failed to send message {:?}: {}", text, err);
    }
}
```

## Connect to the database

Now that our callbacks are all set up, we can connect to the database. We'll store the URI of the SpacetimeDB instance and our module name in constants `SPACETIMEDB_URI` and `DB_NAME`. Replace `quickstart-chat` with the name you chose when publishing your module during the module quickstart, and `http://localhost:3000` with the URI of the SpacetimeDB server you published to.

To `client/src/main.rs`, add:

```rust
/// The URI of the SpacetimeDB instance hosting our chat module.
const HOST: &str = "http://localhost:3000";

/// The module name we chose when we published our module.
const DB_NAME: &str = "quickstart-chat";

/// Load credentials from a file and connect to the database.
fn connect_to_db() -> DbConnection {
    DbConnection::builder()
        .on_connect(on_connected)
        .on_connect_error(on_connect_error)
        .on_disconnect(on_disconnected)
        .with_token(creds_store().load().expect("Error loading credentials"))
        .with_module_name(DB_NAME)
        .with_uri(HOST)
        .with_compression(Compression::Gzip)
        .build()
        .expect("Failed to connect")
}
```

## Subscribe to queries

TODO: Revise this section

SpacetimeDB is set up so that each client subscribes via SQL queries to some subset of the database, and is notified about changes only to that subset. For complex apps with large databases, judicious subscriptions can save each client significant network bandwidth, memory and computation compared. For example, in [BitCraft](https://bitcraftonline.com), each player's client subscribes only to the entities in the "chunk" of the world where that player currently resides, rather than the entire game world. Our app is much simpler than BitCraft, so we'll just subscribe to the whole database.

To `client/src/main.rs`, add:

```rust
/// A helper function to subscribe to each of the `queries`,
/// and run `callback` only when all of the results are ready.
fn subscribe_to_queries(ctx: &DbConnection, queries: &[&str], callback: fn(&SubscriptionEventContext)) {
    if queries.is_empty() {
        panic!("No queries to subscribe to.");
    }
    let remaining_queries = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(queries.len() as u8));
    for query in queries {
        let remaining_queries = remaining_queries.clone();
        ctx.subscription_builder()
            .on_applied(move |ctx| {
                if remaining_queries.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) == 1 {
                    callback(ctx);
                }
            })
            .subscribe(query);
    }
}

/// Register subscriptions for all rows of both tables.
fn subscribe_to_tables(ctx: &DbConnection) {
    subscribe_to_queries(ctx, &["SELECT * FROM user", "SELECT * FROM message"], on_sub_applied);
}
```

## Handle user input

Our app should allow the user to interact by typing lines into their terminal. If the line starts with `/name `, we'll change the user's name. Any other line will send a message.

The functions `set_name` and `send_message` are generated from the server module via `spacetime generate`. We pass them a `String`, which gets sent to the server to execute the corresponding reducer.

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

You can find the full code for this client [in the Rust client SDK's examples](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/sdk/examples/quickstart-chat).

Check out the [Rust client SDK Reference](/docs/sdks/rust) for a more comprehensive view of the SpacetimeDB Rust client SDK.

Our basic terminal interface has some limitations. Incoming messages can appear while the user is typing, which is less than ideal. Additionally, the user's input gets mixed with the program's output, making messages the user sends appear twice. You might want to try improving the interface by using [Rustyline](https://crates.io/crates/rustyline), [Cursive](https://crates.io/crates/cursive), or even creating a full-fledged GUI.

Once your chat server runs for a while, you might want to limit the messages your client loads by refining your `Message` subscription query, only subscribing to messages sent within the last half-hour.

You could also add features like:

- Styling messages by interpreting HTML tags and printing appropriate [ANSI escapes](https://en.wikipedia.org/wiki/ANSI_escape_code).
- Adding a `moderator` flag to the `User` table, allowing moderators to manage users (e.g., time-out, ban).
- Adding rooms or channels that users can join or leave.
- Supporting direct messages or displaying user statuses next to their usernames.
