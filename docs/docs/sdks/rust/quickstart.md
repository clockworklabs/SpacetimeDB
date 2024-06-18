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
spacetimedb-sdk = "0.7"
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

The `spacetime` CLI's `generate` command will generate client-side interfaces for the tables, reducers and types defined in your server module.

In your `quickstart-chat` directory, run:

```bash
mkdir -p client/src/module_bindings
spacetime generate --lang rust --out-dir client/src/module_bindings --project-path server
```

Take a look inside `client/src/module_bindings`. The CLI should have generated five files:

```
module_bindings
├── message.rs
├── mod.rs
├── send_message_reducer.rs
├── set_name_reducer.rs
└── user.rs
```

We need to declare the module in our client crate, and we'll want to import its definitions.

To `client/src/main.rs`, add:

```rust
mod module_bindings;
use module_bindings::*;
```

## Add more imports

We'll need a whole boatload of imports from `spacetimedb_sdk`, which we'll describe when we use them.

To `client/src/main.rs`, add:

```rust
use spacetimedb_sdk::{
    Address,
    disconnect,
    identity::{load_credentials, once_on_connect, save_credentials, Credentials, Identity},
    on_disconnect, on_subscription_applied,
    reducer::Status,
    subscribe,
    table::{TableType, TableWithPrimaryKey},
};
```

## Define main function

We'll work outside-in, first defining our `main` function at a high level, then implementing each behavior it needs. We need `main` to do five things:

1. Register callbacks on any events we want to handle. These will print to standard output messages received from the database and updates about users' names and online statuses.
2. Establish a connection to the database. This will involve authenticating with our credentials, if we're a returning user.
3. Subscribe to receive updates on tables.
4. Loop, processing user input from standard input. This will be how we enable users to set their names and send messages.
5. Close our connection. This one is easy; we just call `spacetimedb_sdk::disconnect`.

To `client/src/main.rs`, add:

```rust
fn main() {
    register_callbacks();
    connect_to_db();
    subscribe_to_tables();
    user_input_loop();
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
fn register_callbacks() {
    // When we receive our `Credentials`, save them to a file.
    once_on_connect(on_connected);

    // When a new user joins, print a notification.
    User::on_insert(on_user_inserted);

    // When a user's status changes, print a notification.
    User::on_update(on_user_updated);

    // When a new message is received, print it.
    Message::on_insert(on_message_inserted);

    // When we receive the message backlog, print it in timestamp order.
    on_subscription_applied(on_sub_applied);

    // When we fail to set our name, print a warning.
    on_set_name(on_name_set);

    // When we fail to send a message, print a warning.
    on_send_message(on_message_sent);

    // When our connection closes, inform the user and exit.
    on_disconnect(on_disconnected);
}
```

### Save credentials

Each user has a `Credentials`, which consists of two parts:

- An `Identity`, a unique public identifier. We're using these to identify `User` rows.
- A `Token`, a private key which SpacetimeDB uses to authenticate the client.

`Credentials` are generated by SpacetimeDB each time a new client connects, and sent to the client so they can be saved, in order to re-connect with the same identity. The Rust SDK provides a pair of functions, `save_credentials` and `load_credentials`, for storing these credentials in a file. We'll save our credentials into a file in the directory `~/.spacetime_chat`, which should be unintrusive. If saving our credentials fails, we'll print a message to standard error, but otherwise continue normally; even though the user won't be able to reconnect with the same identity, they can still chat normally.

Each client also has an `Address`, which modules can use to distinguish multiple concurrent connections by the same `Identity`. We don't need to know our `Address`, so we'll ignore that argument.

To `client/src/main.rs`, add:

```rust
/// Our `on_connect` callback: save our credentials to a file.
fn on_connected(creds: &Credentials, _client_address: Address) {
    if let Err(e) = save_credentials(CREDS_DIR, creds) {
        eprintln!("Failed to save credentials: {:?}", e);
    }
}

const CREDS_DIR: &str = ".spacetime_chat";
```

### Notify about new users

For each table, we can register on-insert and on-delete callbacks to be run whenever a subscribed row is inserted or deleted. We register these callbacks using the `on_insert` and `on_delete` methods of the trait `TableType`, which is automatically implemented for each table by `spacetime generate`.

These callbacks can fire in two contexts:

- After a reducer runs, when the client's cache is updated about changes to subscribed rows.
- After calling `subscribe`, when the client's cache is initialized with all existing matching rows.

This second case means that, even though the module only ever inserts online users, the client's `User::on_insert` callbacks may be invoked with users who are offline. We'll only notify about online users.

`on_insert` and `on_delete` callbacks take two arguments: the altered row, and an `Option<&ReducerEvent>`. This will be `Some` for rows altered by a reducer run, and `None` for rows inserted when initializing the cache for a subscription. `ReducerEvent` is an enum autogenerated by `spacetime generate` with a variant for each reducer defined by the module. For now, we can ignore this argument.

Whenever we want to print a user, if they have set a name, we'll use that. If they haven't set a name, we'll instead print the first 8 bytes of their identity, encoded as hexadecimal. We'll define functions `user_name_or_identity` and `identity_leading_hex` to handle this.

To `client/src/main.rs`, add:

```rust
/// Our `User::on_insert` callback:
/// if the user is online, print a notification.
fn on_user_inserted(user: &User, _: Option<&ReducerEvent>) {
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
    hex::encode(&id.bytes()[0..8])
}
```

### Notify about updated users

Because we declared a `#[primarykey]` column in our `User` table, we can also register on-update callbacks. These run whenever a row is replaced by a row with the same primary key, like our module's `User::update_by_identity` calls. We register these callbacks using the `on_update` method of the trait `TableWithPrimaryKey`, which is automatically implemented by `spacetime generate` for any table with a `#[primarykey]` column.

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
fn on_user_updated(old: &User, new: &User, _: Option<&ReducerEvent>) {
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

### Print messages

When we receive a new message, we'll print it to standard output, along with the name of the user who sent it. Keep in mind that we only want to do this for new messages, i.e. those inserted by a `send_message` reducer invocation. We have to handle the backlog we receive when our subscription is initialized separately, to ensure they're printed in the correct order. To that effect, our `print_new_message` callback will check if its `reducer_event` argument is `Some`, and only print in that case.

To find the `User` based on the message's `sender` identity, we'll use `User::find_by_identity`, which behaves like the same function on the server. The key difference is that, unlike on the module side, the client's `find_by_identity` accepts an owned `Identity`, rather than a reference. We can `clone` the identity held in `message.sender`.

We'll print the user's name or identity in the same way as we did when notifying about `User` table events, but here we have to handle the case where we don't find a matching `User` row. This can happen when the module owner sends a message using the CLI's `spacetime call`. In this case, we'll print `unknown`.

To `client/src/main.rs`, add:

```rust
/// Our `Message::on_insert` callback: print new messages.
fn on_message_inserted(message: &Message, reducer_event: Option<&ReducerEvent>) {
    if reducer_event.is_some() {
        print_message(message);
    }
}

fn print_message(message: &Message) {
    let sender = User::find_by_identity(message.sender.clone())
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
fn on_sub_applied() {
    let mut messages = Message::iter().collect::<Vec<_>>();
    messages.sort_by_key(|m| m.sent);
    for message in messages {
        print_message(&message);
    }
}
```

### Warn if our name was rejected

We can also register callbacks to run each time a reducer is invoked. We register these callbacks using the `on_reducer` method of the `Reducer` trait, which is automatically implemented for each reducer by `spacetime generate`.

Each reducer callback takes at least three arguments:

1. The `Identity` of the client who requested the reducer invocation.
2. The `Address` of the client who requested the reducer invocation, which may be `None` for scheduled reducers.
3. The `Status` of the reducer run, one of `Committed`, `Failed` or `OutOfEnergy`. `Status::Failed` holds the error which caused the reducer to fail, as a `String`.

In addition, it takes a reference to each of the arguments passed to the reducer itself.

These callbacks will be invoked in one of two cases:

1. If the reducer was successful and altered any of our subscribed rows.
2. If we requested an invocation which failed.

Note that a status of `Failed` or `OutOfEnergy` implies that the caller identity is our own identity.

We already handle successful `set_name` invocations using our `User::on_update` callback, but if the module rejects a user's chosen name, we'd like that user's client to let them know. We define a function `warn_if_name_rejected` as a `SetNameArgs::on_reducer` callback which checks if the reducer failed, and if it did, prints a message including the rejected name and the error.

To `client/src/main.rs`, add:

```rust
/// Our `on_set_name` callback: print a warning if the reducer failed.
fn on_name_set(_sender_id: &Identity, _sender_address: Option<Address>, status: &Status, name: &String) {
    if let Status::Failed(err) = status {
        eprintln!("Failed to change name to {:?}: {}", name, err);
    }
}
```

### Warn if our message was rejected

We handle warnings on rejected messages the same way as rejected names, though the types and the error message are different.

To `client/src/main.rs`, add:

```rust
/// Our `on_send_message` callback: print a warning if the reducer failed.
fn on_message_sent(_sender_id: &Identity, _sender_address: Option<Address>, status: &Status, text: &String) {
    if let Status::Failed(err) = status {
        eprintln!("Failed to send message {:?}: {}", text, err);
    }
}
```

### Exit on disconnect

We can register callbacks to run when our connection ends using `on_disconnect`. These callbacks will run either when the client disconnects by calling `disconnect`, or when the server closes our connection. More involved apps might attempt to reconnect in this case, or do some sort of client-side cleanup, but we'll just print a note to the user and then exit the process.

To `client/src/main.rs`, add:

```rust
/// Our `on_disconnect` callback: print a note, then exit the process.
fn on_disconnected() {
    eprintln!("Disconnected!");
    std::process::exit(0)
}
```

## Connect to the database

Now that our callbacks are all set up, we can connect to the database. We'll store the URI of the SpacetimeDB instance and our module name in constants `SPACETIMEDB_URI` and `DB_NAME`. Replace `<module-name>` with the name you chose when publishing your module during the module quickstart.

`connect` takes an `Option<Credentials>`, which is `None` for a new connection, or `Some` for a returning user. The Rust SDK defines `load_credentials`, the counterpart to the `save_credentials` we used in our `save_credentials_or_log_error`, to load `Credentials` from a file. `load_credentials` returns `Result<Option<Credentials>>`, with `Ok(None)` meaning the credentials haven't been saved yet, and an `Err` meaning reading from disk failed. We can `expect` to handle the `Result`, and pass the `Option<Credentials>` directly to `connect`.

To `client/src/main.rs`, add:

```rust
/// The URL of the SpacetimeDB instance hosting our chat module.
const SPACETIMEDB_URI: &str = "http://localhost:3000";

/// The module name we chose when we published our module.
const DB_NAME: &str = "<module-name>";

/// Load credentials from a file and connect to the database.
fn connect_to_db() {
    connect(
        SPACETIMEDB_URI,
        DB_NAME,
        load_credentials(CREDS_DIR).expect("Error reading stored credentials"),
    )
    .expect("Failed to connect");
}
```

## Subscribe to queries

SpacetimeDB is set up so that each client subscribes via SQL queries to some subset of the database, and is notified about changes only to that subset. For complex apps with large databases, judicious subscriptions can save each client significant network bandwidth, memory and computation compared. For example, in [BitCraft](https://bitcraftonline.com), each player's client subscribes only to the entities in the "chunk" of the world where that player currently resides, rather than the entire game world. Our app is much simpler than BitCraft, so we'll just subscribe to the whole database.

To `client/src/main.rs`, add:

```rust
/// Register subscriptions for all rows of both tables.
fn subscribe_to_tables() {
    subscribe(&["SELECT * FROM User;", "SELECT * FROM Message;"]).unwrap();
}
```

## Handle user input

A user should interact with our client by typing lines into their terminal. A line that starts with `/name ` will set the user's name to the rest of the line. Any other line will send a message.

`spacetime generate` defined two functions for us, `set_name` and `send_message`, which send a message to the database to invoke the corresponding reducer. The first argument, the `ReducerContext`, is supplied by the server, but we pass all other arguments ourselves. In our case, that means that both `set_name` and `send_message` take one argument, a `String`.

To `client/src/main.rs`, add:

```rust
/// Read each line of standard input, and either set our name or send a message as appropriate.
fn user_input_loop() {
    for line in std::io::stdin().lines() {
        let Ok(line) = line else {
            panic!("Failed to read from stdin.");
        };
        if let Some(name) = line.strip_prefix("/name ") {
            set_name(name.to_string());
        } else {
            send_message(line);
        }
    }
}
```

## Run it

Change your directory to the client app, then compile and run it. From the `quickstart-chat` directory, run:

```bash
cd client
cargo run
```

You should see something like:

```
User d9e25c51996dea2f connected.
```

Now try sending a message. Type `Hello, world!` and press enter. You should see something like:

```
d9e25c51996dea2f: Hello, world!
```

Next, set your name. Type `/name <my-name>`, replacing `<my-name>` with your name. You should see something like:

```
User d9e25c51996dea2f renamed to <my-name>.
```

Then send another message. Type `Hello after naming myself.` and press enter. You should see:

```
<my-name>: Hello after naming myself.
```

Now, close the app by hitting control-c, and start it again with `cargo run`. You should see yourself connecting, and your past messages in order:

```
User <my-name> connected.
<my-name>: Hello, world!
<my-name>: Hello after naming myself.
```

## What's next?

You can find the full code for this client [in the Rust SDK's examples](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/sdk/examples/quickstart-chat).

Check out the [Rust SDK Reference](/docs/sdks/rust) for a more comprehensive view of the SpacetimeDB Rust SDK.

Our bare-bones terminal interface has some quirks. Incoming messages can appear while the user is typing and be spliced into the middle of user input, which is less than ideal. Also, the user's input is interspersed with the program's output, so messages the user sends will seem to appear twice. Why not try building a better interface using [Rustyline](https://crates.io/crates/rustyline), [Cursive](https://crates.io/crates/cursive), or even a full-fledged GUI? We went for the Cursive route, and you can check out what we came up with [in the Rust SDK's examples](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/sdk/examples/cursive-chat).

Once our chat server runs for a while, messages will accumulate, and it will get frustrating to see the entire backlog each time you connect. Instead, you could refine your `Message` subscription query, subscribing only to messages newer than, say, half an hour before the user connected.

You could also add support for styling messages, perhaps by interpreting HTML tags in the messages and printing appropriate [ANSI escapes](https://en.wikipedia.org/wiki/ANSI_escape_code).

Or, you could extend the module and the client together, perhaps:

- Adding a `moderator: bool` flag to `User` and allowing moderators to time-out or ban naughty chatters.
- Adding a message of the day which gets shown to users whenever they connect, or some rules which get shown only to new users.
- Supporting separate rooms or channels which users can join or leave, and maybe even direct messages.
- Allowing users to set their status, which could be displayed alongside their username.
