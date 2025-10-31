---
title: Rust 
slug: /quickstarts/rust
id: rust
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";

# Quickstart Chat App 

In this tutorial, we'll implement a simple chat server as a SpacetimeDB module.

A SpacetimeDB module is code that gets compiled to a WebAssembly binary and is uploaded to SpacetimeDB. This code becomes server-side logic that interfaces directly with the SpacetimeDB relational database.

Each SpacetimeDB module defines a set of tables and a set of reducers.

Each table is defined as a Rust struct annotated with `#[table(name = table_name)]`. An instance of the struct represents a row, and each field represents a column.

By default, tables are **private**. This means that they are only readable by the table owner, and by server module code.
The `#[table(name = table_name, public)]` macro makes a table public. **Public** tables are readable by all users but can still only be modified by your server module code.

A reducer is a function that traverses and updates the database. Each reducer call runs in its own transaction, and its updates to the database are only committed if the reducer returns successfully. In Rust, reducers are defined as functions annotated with `#[reducer]`, and may return a `Result<()>`, with an `Err` return aborting the transaction.

## Install SpacetimeDB

If you haven't already, start by [installing SpacetimeDB](pathname:///install). This will install the `spacetime` command line interface (CLI), which provides all the functionality needed to interact with SpacetimeDB.

<InstallCardLink />

## Install Rust

Next we need to [install Rust](https://www.rust-lang.org/tools/install) so that we can create our database module.

On macOS and Linux run this command to install the Rust compiler:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

If you're on Windows, go [here](https://learn.microsoft.com/en-us/windows/dev-environment/rust/setup).

## Project structure

Let's start by running `spacetime init` to initialize our project's directory structure:

```bash
spacetime init --lang rust quickstart-chat
```

`spacetime init` will ask you for a project path in which to put your project. By default this will be `./quickstart-chat`. This basic project will have a few helper files like Cursor rules for SpacetimeDB and a `spacetimedb` directory which is where your SpacetimeDB module code will go.

## How to Compile

> [!IMPORTANT]
> While it is possible to use the traditional `cargo build` to build SpacetimeDB server modules, `spacetime build` makes this process easier. Keep this in mind when using an IDE that assumes using _cargo_ for building.

```bash
cd spacetimedb 
spacetime build
```

## Declare imports

`spacetime init` should have pre-populated `spacetimedb/src/lib.rs` with a trivial module. Clear it out so we can write a new, simple module: a bare-bones chat server.

To the top of `spacetimedb/src/lib.rs`, add some imports we'll be using:

```rust
use spacetimedb::{table, reducer, Table, ReducerContext, Identity, Timestamp};
```

From `spacetimedb`, we import:

- `table`, a macro used to define SpacetimeDB tables.
- `reducer`, a macro used to define SpacetimeDB reducers.
- `Table`, a rust trait which allows us to interact with tables.
- `ReducerContext`, a special argument passed to each reducer.
- `Identity`, a unique identifier for each user.
- `Timestamp`, a point in time. Specifically, an unsigned 64-bit count of milliseconds since the UNIX epoch.

## Define tables

To get our chat server running, we'll need to store two kinds of data: information about each user, and records of all the messages that have been sent.

For each `User`, we'll store their `Identity`, an optional name they can set to identify themselves to other users, and whether they're online or not. We'll designate the `Identity` as our primary key, which enforces that it must be unique, indexes it for faster lookup, and allows clients to track updates.

To `spacetimedb/src/lib.rs`, add the definition of the table `User`:

```rust
#[table(name = user, public)]
pub struct User {
    #[primary_key]
    identity: Identity,
    name: Option<String>,
    online: bool,
}
```

For each `Message`, we'll store the `Identity` of the user who sent it, the `Timestamp` when it was sent, and the text of the message.

To `spacetimedb/src/lib.rs`, add the definition of the table `Message`:

```rust
#[table(name = message, public)]
pub struct Message {
    sender: Identity,
    sent: Timestamp,
    text: String,
}
```

## Set users' names

We want to allow users to set their names, because `Identity` is not a terribly user-friendly identifier. To that effect, we define a reducer `set_name` which clients can invoke to set their `User.name`. It will validate the caller's chosen name, using a function `validate_name` which we'll define next, then look up the `User` record for the caller and update it to store the validated name. If the name fails the validation, the reducer will fail.

Each reducer must accept as its first argument a `ReducerContext`, which includes the `Identity` and `ConnectionId` of the client that called the reducer, and the `Timestamp` when it was invoked. It also allows us access to the `db`, which is used to read and manipulate rows in our tables. For now, we only need the `db`, `Identity`, and `ctx.sender`.

It's also possible to call `set_name` via the SpacetimeDB CLI's `spacetime call` command without a connection, in which case no `User` record will exist for the caller. We'll return an error in this case, but you could alter the reducer to insert a `User` row for the module owner. You'll have to decide whether the module owner is always online or always offline, though.

To `spacetimedb/src/lib.rs`, add:

```rust
#[reducer]
/// Clients invoke this reducer to set their user names.
pub fn set_name(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let name = validate_name(name)?;
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User { name: Some(name), ..user });
        Ok(())
    } else {
        Err("Cannot set name for unknown user".to_string())
    }
}
```

For now, we'll just do a bare minimum of validation, rejecting the empty name. You could extend this in various ways, like:

- Comparing against a blacklist for moderation purposes.
- Unicode-normalizing names.
- Rejecting names that contain non-printable characters, or removing characters or replacing them with a placeholder.
- Rejecting or truncating long names.
- Rejecting duplicate names.

To `spacetimedb/src/lib.rs`, add:

```rust
/// Takes a name and checks if it's acceptable as a user's name.
fn validate_name(name: String) -> Result<String, String> {
    if name.is_empty() {
        Err("Names must not be empty".to_string())
    } else {
        Ok(name)
    }
}
```

## Send messages

We define a reducer `send_message`, which clients will call to send messages. It will validate the message's text, then insert a new `Message` record using `ctx.db.message().insert(..)`, with the `sender` identity and `sent` timestamp taken from the `ReducerContext`. Because the `Message` table does not have any columns with a unique constraint, `ctx.db.message().insert()` is infallible and does not return a `Result`.

To `spacetimedb/src/lib.rs`, add:

```rust
#[reducer]
/// Clients invoke this reducer to send messages.
pub fn send_message(ctx: &ReducerContext, text: String) -> Result<(), String> {
    let text = validate_message(text)?;
    log::info!("{}", text);
    ctx.db.message().insert(Message {
        sender: ctx.sender,
        text,
        sent: ctx.timestamp,
    });
    Ok(())
}
```

We'll want to validate messages' texts in much the same way we validate users' chosen names. As above, we'll do the bare minimum, rejecting only empty messages.

To `spacetimedb/src/lib.rs`, add:

```rust
/// Takes a message's text and checks if it's acceptable to send.
fn validate_message(text: String) -> Result<String, String> {
    if text.is_empty() {
        Err("Messages must not be empty".to_string())
    } else {
        Ok(text)
    }
}
```

You could extend the validation in `validate_message` in similar ways to `validate_name`, or add additional checks to `send_message`, like:

- Rejecting messages from senders who haven't set their names.
- Rate-limiting users so they can't send new messages too quickly.

## Set users' online status

Whenever a client connects, the database will run a special reducer, annotated with `#[reducer(client_connected)]`, if it's defined. By convention, it's named `client_connected`. We'll use it to create a `User` record for the client if it doesn't yet exist, and to set its online status.

We'll use `ctx.db.user().identity().find(ctx.sender)` to look up a `User` row for `ctx.sender`, if one exists. If we find one, we'll use `ctx.db.user().identity().update(..)` to overwrite it with a row that has `online: true`. If not, we'll use `ctx.db.user().insert(..)` to insert a new row for our new user. All three of these methods are generated by the `#[table(..)]` macro, with rows and behavior based on the row attributes. `ctx.db.user().find(..)` returns an `Option<User>`, because of the unique constraint from the `#[primary_key]` attribute. This means there will be either zero or one matching rows. If we used `try_insert` here it would return a `Result<(), UniqueConstraintViolation>` because of the same unique constraint. However, because we're already checking if there is a user with the given sender identity we know that inserting into this table will not fail. Therefore, we use `insert`, which automatically unwraps the result, simplifying the code. If we want to overwrite a `User` row, we need to do so explicitly using `ctx.db.user().identity().update(..)`.

To `spacetimedb/src/lib.rs`, add the definition of the connect reducer:

```rust
#[reducer(client_connected)]
// Called when a client connects to a SpacetimeDB database 
pub fn client_connected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        // If this is a returning user, i.e. we already have a `User` with this `Identity`,
        // set `online: true`, but leave `name` and `identity` unchanged.
        ctx.db.user().identity().update(User { online: true, ..user });
    } else {
        // If this is a new user, create a `User` row for the `Identity`,
        // which is online, but hasn't set a name.
        ctx.db.user().insert(User {
            name: None,
            identity: ctx.sender,
            online: true,
        });
    }
}
```

Similarly, whenever a client disconnects, the database will run the `#[reducer(client_disconnected)]` reducer if it's defined. By convention, it's named `client_disconnected`. We'll use it to un-set the `online` status of the `User` for the disconnected client.

```rust
#[reducer(client_disconnected)]
// Called when a client disconnects from SpacetimeDB database 
pub fn identity_disconnected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User { online: false, ..user });
    } else {
        // This branch should be unreachable,
        // as it doesn't make sense for a client to disconnect without connecting first.
        log::warn!("Disconnect event for unknown user with identity {:?}", ctx.sender);
    }
}
```

## Start the Server

If you haven't already started the SpacetimeDB server, run the `spacetime start` command in a _separate_ terminal and leave it running while you continue following along.

## Publish the module

And that's all of our module code! We'll run `spacetime publish` to compile our module and publish it on SpacetimeDB. `spacetime publish` takes an optional name which will map to the database's unique `Identity`. Clients can connect either by name or by `Identity`, but names are much more user-friendly. If you'd like, come up with a unique name that contains only URL-safe characters (letters, numbers, hyphens and underscores), and fill it in where we've written `quickstart-chat`.

From the `quickstart-chat` directory, run in another tab:

```bash
spacetime publish --server local --project-path spacetimedb quickstart-chat
```

## Call Reducers

You can use the CLI (command line interface) to run reducers. The arguments to the reducer are passed in JSON format.

```bash
spacetime call --server local quickstart-chat send_message "Hello, World!"
```

Once we've called our `send_message` reducer, we can check to make sure it ran by running the `logs` command.

```bash
spacetime logs --server local quickstart-chat
```

You should now see the output that your module printed in the database.

```bash
<timestamp>  INFO: spacetimedb: Creating table `message`
<timestamp>  INFO: spacetimedb: Creating table `user`
<timestamp>  INFO: spacetimedb: Database initialized
<timestamp>  INFO: src/lib.rs:43: Hello, world!
```

## SQL Queries

SpacetimeDB supports a subset of the SQL syntax so that you can easily query the data of your database. We can run a query using the `sql` command.

```bash
spacetime sql --server local quickstart-chat "SELECT * FROM message"
```

```bash
 sender                                                             | sent                             | text
--------------------------------------------------------------------+----------------------------------+-----------------
 0x93dda09db9a56d8fa6c024d843e805d8262191db3b4ba84c5efcd1ad451fed4e | 2025-04-08T15:47:46.935402+00:00 | "Hello, world!"
```

You've just set up your first Rust module in SpacetimeDB! You can find the full code for this module [in the SpacetimeDB module examples](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/quickstart-chat).

# Creating the client 

Next, we'll show you how to get up and running with a simple SpacetimeDB app with a client written in Rust.

We'll implement a command-line client for the module created in our Rust or C# Module Quickstart guides. Make sure you follow one of these guides before you start on this one.

## Project structure

Enter the directory `quickstart-chat` you created in the [Rust Module Quickstart](/docs/quickstarts/rust) or [C# Module Quickstart](/docs/quickstarts/c-sharp) guides:

```bash
cd quickstart-chat
```

Within it, create a `client` crate, our client application, which users run locally:

```bash
cargo new client
```

## Depend on `spacetimedb-sdk` and `hex`

`client/Cargo.toml` should be initialized without any dependencies. We'll need two:

- [`spacetimedb-sdk`](https://crates.io/crates/spacetimedb-sdk), which defines client-side interfaces for interacting with a remote SpacetimeDB database.
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
use spacetimedb_sdk::{credentials, DbContext, Error, Event, Identity, Status, Table, TableWithPrimaryKey};
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

## Connect to the database

A connection to a SpacetimeDB database is represented by a `DbConnection`. We configure `DbConnection`s using the builder pattern, by calling `DbConnection::builder()`, chaining method calls to set various connection parameters and register callbacks, then we cap it off with a call to `.build()` to begin the connection.

In our case, we'll supply the following options:

1. An `on_connect` callback, to run when the remote database acknowledges and accepts our connection.
2. An `on_connect_error` callback, to run if the remote database is unreachable or it rejects our connection.
3. An `on_disconnect` callback, to run when our connection ends.
4. A `with_token` call, to supply a token to authenticate with.
5. A `with_module_name` call, to specify the name or `Identity` of our database. Make sure to pass the same name here as you supplied to `spacetime publish`.
6. A `with_uri` call, to specify the URI of the SpacetimeDB host where our database is running.

To `client/src/main.rs`, add:

```rust
/// The URI of the SpacetimeDB instance hosting our chat database and module.
const HOST: &str = "http://localhost:3000";

/// The database name we chose when we published our module.
const DB_NAME: &str = "quickstart-chat";

/// Load credentials from a file and connect to the database.
fn connect_to_db() -> DbConnection {
    DbConnection::builder()
        // Register our `on_connect` callback, which will save our auth token.
        .on_connect(on_connected)
        // Register our `on_connect_error` callback, which will print a message, then exit the process.
        .on_connect_error(on_connect_error)
        // Our `on_disconnect` callback, which will print a message, then exit the process.
        .on_disconnect(on_disconnected)
        // If the user has previously connected, we'll have saved a token in the `on_connect` callback.
        // In that case, we'll load it and pass it to `with_token`,
        // so we can re-authenticate as the same `Identity`.
        .with_token(creds_store().load().expect("Error loading credentials"))
        // Set the database name we chose when we called `spacetime publish`.
        .with_module_name(DB_NAME)
        // Set the URI of the SpacetimeDB host that's running our database.
        .with_uri(HOST)
        // Finalize configuration and connect!
        .build()
        .expect("Failed to connect")
}
```

### Save credentials

SpacetimeDB will accept any [OpenID Connect](https://openid.net/developers/how-connect-works/) compliant [JSON Web Token](https://jwt.io/) and use it to compute an `Identity` for the user. More complex applications will generally authenticate their user somehow, generate or retrieve a token, and attach it to their connection via `with_token`. In our case, though, we'll connect anonymously the first time, let SpacetimeDB generate a fresh `Identity` and corresponding JWT for us, and save that token locally to re-use the next time we connect.

The Rust SDK provides a pair of functions in `File`, `save` and `load`, for saving and storing these credentials in a file. By default the `save` and `load` will look for credentials in the `$HOME/.spacetimedb_client_credentials/` directory, which should be unintrusive. If saving our credentials fails, we'll print a message to standard error, but otherwise continue; even though the user won't be able to reconnect with the same identity, they can still chat normally.

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

### Handle errors and disconnections

We need to handle connection errors and disconnections by printing appropriate messages and exiting the program. These callbacks take an `ErrorContext`, a `DbConnection` that's been augmented with information about the error that occured.

To `client/src/main.rs`, add:

```rust
/// Our `on_connect_error` callback: print the error, then exit the process.
fn on_connect_error(_ctx: &ErrorContext, err: Error) {
    eprintln!("Connection error: {:?}", err);
    std::process::exit(1);
}

/// Our `on_disconnect` callback: print a note, then exit the process.
fn on_disconnected(_ctx: &ErrorContext, err: Option<Error>) {
    if let Some(err) = err {
        eprintln!("Disconnected: {}", err);
        std::process::exit(1);
    } else {
        println!("Disconnected.");
        std::process::exit(0);
    }
}
```

## Register callbacks

We need to handle several sorts of events:

1. When a new user joins, we'll print a message introducing them.
2. When a user is updated, we'll print their new name, or declare their new online status.
3. When we receive a new message, we'll print it.
4. If the server rejects our attempt to set our name, we'll print an error.
5. If the server rejects a message we send, we'll print an error.

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

    // When we fail to set our name, print a warning.
    ctx.reducers.on_set_name(on_name_set);

    // When we fail to send a message, print a warning.
    ctx.reducers.on_send_message(on_message_sent);
}
```

### Notify about new users

For each table, we can register on-insert and on-delete callbacks to be run whenever a subscribed row is inserted or deleted. We register these callbacks using the `on_insert` and `on_delete`, which is automatically implemented for each table by `spacetime generate`.

These callbacks can fire in several contexts, of which we care about two:

- After a reducer runs, when the client's cache is updated about changes to subscribed rows.
- After calling `subscribe`, when the client's cache is initialized with all existing matching rows.

This second case means that, even though the module only ever inserts online users, the client's `conn.db.user().on_insert(..)` callbacks may be invoked with users who are offline. We'll only notify about online users.

`on_insert` and `on_delete` callbacks take two arguments: an `&EventContext` and the modified row. Like the `ErrorContext` above, `EventContext` is a `DbConnection` that's been augmented with information about the event that caused the row to be modified. You can determine whether the insert/delete operation was caused by a reducer, a newly-applied subscription, or some other event by pattern-matching on `ctx.event`.

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

Because we declared a `#[primary_key]` column in our `User` table, we can also register on-update callbacks. These run whenever a row is replaced by a row with the same primary key, like our module's `ctx.db.user().identity().update(..)` calls. We register these callbacks using the `on_update` method of the trait `TableWithPrimaryKey`, which is automatically implemented by `spacetime generate` for any table with a `#[primary_key]` column.

`on_update` callbacks take three arguments: the `&EventContext`, the old row, and the new row.

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

### Print messages

When we receive a new message, we'll print it to standard output, along with the name of the user who sent it. Keep in mind that we only want to do this for new messages, i.e. those inserted by a `send_message` reducer invocation. We have to handle the backlog we receive when our subscription is initialized separately, to ensure they're printed in the correct order. To that effect, our `on_message_inserted` callback will check if the ctx.event type is an `Event::Reducer`, and only print in that case.

To find the `User` based on the message's `sender` identity, we'll use `ctx.db.user().identity().find(..)`, which behaves like the same function on the server.

We'll print the user's name or identity in the same way as we did when notifying about `User` table events, but here we have to handle the case where we don't find a matching `User` row. This can happen when the module owner sends a message using the CLI's `spacetime call`. In this case, we'll print `unknown`.

Notice that our `print_message` function takes an `&impl RemoteDbContext` as an argument. This is a trait, defined in our `module_bindings` by `spacetime generate`, which is implemented by `DbConnection`, `EventContext`, `ErrorContext` and a few other similar types. (`RemoteDbContext` is actually a shorthand for `DbContext`, which applies to connections to _any_ module, with its associated types locked to module-specific ones.) Later on, we're going to call `print_message` with a `ReducerEventContext`, so we need to be more generic than just accepting `EventContext`.

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

### Handle reducer failures

We can also register callbacks to run each time a reducer is invoked. We register these callbacks using the `on_reducer` method of the `Reducer` trait, which is automatically implemented for each reducer by `spacetime generate`.

Each reducer callback first takes a `&ReducerEventContext` which contains metadata about the reducer call, including the identity of the caller and whether or not the reducer call suceeded.

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

## Subscribe to queries

SpacetimeDB is set up so that each client subscribes via SQL queries to some subset of the database, and is notified about changes only to that subset. For complex apps with large databases, judicious subscriptions can save each client significant network bandwidth, memory and computation. For example, in [BitCraft](https://bitcraftonline.com), each player's client subscribes only to the entities in the "chunk" of the world where that player currently resides, rather than the entire game world. Our app is much simpler than BitCraft, so we'll just subscribe to the whole database.

When we specify our subscriptions, we can supply an `on_applied` callback. This will run when the subscription is applied and the matching rows become available in our client cache. We'll use this opportunity to print the message backlog in proper order.

We'll also provide an `on_error` callback. This will run if the subscription fails, usually due to an invalid or malformed SQL queries. We can't handle this case, so we'll just print out the error and exit the process.

To `client/src/main.rs`, add:

```rust
/// Register subscriptions for all rows of both tables.
fn subscribe_to_tables(ctx: &DbConnection) {
    ctx.subscription_builder()
        .on_applied(on_sub_applied)
        .on_error(on_sub_error)
        .subscribe(["SELECT * FROM user", "SELECT * FROM message"]);
}
```

### Print past messages in order

Messages we receive live will come in order, but when we connect, we'll receive all the past messages at once. We can't just print these in the order we receive them; the logs would be all shuffled around, and would make no sense. Instead, when we receive the log of past messages, we'll sort them by their sent timestamps and print them in order.

We'll handle this in our function `print_messages_in_order`, which we registered as an `on_applied` callback. `print_messages_in_order` iterates over all the `Message`s we've received, sorts them, and then prints them. `ctx.db.message().iter()` is defined on the trait `Table`, and returns an iterator over all the messages in the client cache. Rust iterators can't be sorted in-place, so we'll collect it to a `Vec`, then use the `sort_by_key` method to sort by timestamp.

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

### Notify about failed subscriptions

It's possible for SpacetimeDB to reject subscriptions. This happens most often because of a typo in the SQL queries, but can be due to use of SQL features that SpacetimeDB doesn't support. See [SQL Support: Subscriptions](/sql#subscriptions) for more information about what subscription queries SpacetimeDB supports.

In our case, we're pretty confident that our queries are valid, but if SpacetimeDB rejects them, we want to know about it. Our callback will print the error, then exit the process.

```rust
/// Or `on_error` callback:
/// print the error, then exit the process.
fn on_sub_error(_ctx: &ErrorContext, err: Error) {
    eprintln!("Subscription failed: {}", err);
    std::process::exit(1);
}
```

## Handle user input

Our app should allow the user to interact by typing lines into their terminal. If the line starts with `/name`, we'll change the user's name. Any other line will send a message.

For each reducer defined by our module, `ctx.reducers` has a method to request an invocation. In our case, we pass `set_name` and `send_message` a `String`, which gets sent to the server to execute the corresponding reducer.

To `client/src/main.rs`, add:

```rust
/// Read each line of standard input, and either set our name or send a message as appropriate.
fn user_input_loop(ctx: &DbConnection) {
    for line in std::io::stdin().lines() {
        let Ok(line) = line else {
            panic!("Failed to read from stdin.");
        };
        if let Some(name) = line.strip_prefix("/name ") {
            ctx.reducers.set_name(name.to_string()).unwrap();
        } else {
            ctx.reducers.send_message(line).unwrap();
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

Check out the [Rust client SDK Reference](/sdks/rust) for a more comprehensive view of the SpacetimeDB Rust client SDK.

Our basic terminal interface has some limitations. Incoming messages can appear while the user is typing, which is less than ideal. Additionally, the user's input gets mixed with the program's output, making messages the user sends appear twice. You might want to try improving the interface by using [Rustyline](https://crates.io/crates/rustyline), [Cursive](https://crates.io/crates/cursive), or even creating a full-fledged GUI.

Once your chat server runs for a while, you might want to limit the messages your client loads by refining your `Message` subscription query, only subscribing to messages sent within the last half-hour.

You could also add features like:

- Styling messages by interpreting HTML tags and printing appropriate [ANSI escapes](https://en.wikipedia.org/wiki/ANSI_escape_code).
- Adding a `moderator` flag to the `User` table, allowing moderators to manage users (e.g., time-out, ban).
- Adding rooms or channels that users can join or leave.
- Supporting direct messages or displaying user statuses next to their usernames.
