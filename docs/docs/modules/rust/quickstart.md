# Rust Module Quickstart

In this tutorial, we'll implement a simple chat server as a SpacetimeDB module.

A SpacetimeDB module is code that gets compiled to WebAssembly and is uploaded to SpacetimeDB. This code becomes server-side logic that interfaces directly with the Spacetime relational database.

Each SpacetimeDB module defines a set of tables and a set of reducers.

Each table is defined as a Rust `struct` annotated with `#[spacetimedb(table(...))]`, where an instance represents a row, and each field represents a column.
By default, tables are **private**. This means that they are only readable by the table owner, and by server module code.
The `#[spacetimedb(table(public))]` macro makes a table public. **Public** tables are readable by all users, but can still only be modified by your server module code.

_Coming soon: We plan to add much more robust access controls than just `public` or `private`. Stay tuned!_

A reducer is a function which traverses and updates the database. Each reducer call runs in its own transaction, and its updates to the database are only committed if the reducer returns successfully. In Rust, reducers are defined as functions annotated with `#[spacetimedb(reducer)]`, and may return a `Result<()>`, with an `Err` return aborting the transaction.

## Install SpacetimeDB

If you haven't already, start by [installing SpacetimeDB](/install). This will install the `spacetime` command line interface (CLI), which contains all the functionality for interacting with SpacetimeDB.

## Install Rust

Next we need to [install Rust](https://www.rust-lang.org/tools/install) so that we can create our database module.

On MacOS and Linux run this command to install the Rust compiler:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

If you're on Windows, go [here](https://learn.microsoft.com/en-us/windows/dev-environment/rust/setup).

## Project structure

Create and enter a directory `quickstart-chat`:

```bash
mkdir quickstart-chat
cd quickstart-chat
```

Now create `server`, our module, which runs in the database:

```bash
spacetime init --lang rust server
```

## Declare imports

`spacetime init` should have pre-populated `server/src/lib.rs` with a trivial module. Clear it out, so we can write a module that's still pretty simple: a bare-bones chat server.

To the top of `server/src/lib.rs`, add some imports we'll be using:

```rust
use spacetimedb::{spacetimedb, ReducerContext, Identity, Timestamp};
```

From `spacetimedb`, we import:

- `spacetimedb`, an attribute macro we'll use to define tables and reducers.
- `ReducerContext`, a special argument passed to each reducer.
- `Identity`, a unique identifier for each user.
- `Timestamp`, a point in time. Specifically, an unsigned 64-bit count of milliseconds since the UNIX epoch.

## Define tables

To get our chat server running, we'll need to store two kinds of data: information about each user, and records of all the messages that have been sent.

For each `User`, we'll store their `Identity`, an optional name they can set to identify themselves to other users, and whether they're online or not. We'll designate the `Identity` as our primary key, which enforces that it must be unique, indexes it for faster lookup, and allows clients to track updates.

To `server/src/lib.rs`, add the definition of the table `User`:

```rust
#[spacetimedb(table(public))]
pub struct User {
    #[primarykey]
    identity: Identity,
    name: Option<String>,
    online: bool,
}
```

For each `Message`, we'll store the `Identity` of the user who sent it, the `Timestamp` when it was sent, and the text of the message.

To `server/src/lib.rs`, add the definition of the table `Message`:

```rust
#[spacetimedb(table(public))]
pub struct Message {
    sender: Identity,
    sent: Timestamp,
    text: String,
}
```

## Set users' names

We want to allow users to set their names, because `Identity` is not a terribly user-friendly identifier. To that effect, we define a reducer `set_name` which clients can invoke to set their `User.name`. It will validate the caller's chosen name, using a function `validate_name` which we'll define next, then look up the `User` record for the caller and update it to store the validated name. If the name fails the validation, the reducer will fail.

Each reducer may accept as its first argument a `ReducerContext`, which includes the `Identity` and `Address` of the client that called the reducer, and the `Timestamp` when it was invoked. For now, we only need the `Identity`, `ctx.sender`.

It's also possible to call `set_name` via the SpacetimeDB CLI's `spacetime call` command without a connection, in which case no `User` record will exist for the caller. We'll return an error in this case, but you could alter the reducer to insert a `User` row for the module owner. You'll have to decide whether the module owner is always online or always offline, though.

To `server/src/lib.rs`, add:

```rust
#[spacetimedb(reducer)]
/// Clientss invoke this reducer to set their user names.
pub fn set_name(ctx: ReducerContext, name: String) -> Result<(), String> {
    let name = validate_name(name)?;
    if let Some(user) = User::filter_by_identity(&ctx.sender) {
        User::update_by_identity(&ctx.sender, User { name: Some(name), ..user });
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

To `server/src/lib.rs`, add:

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

We define a reducer `send_message`, which clients will call to send messages. It will validate the message's text, then insert a new `Message` record using `Message::insert`, with the `sender` identity and `sent` timestamp taken from the `ReducerContext`. Because `Message` does not have any columns with unique constraints, `Message::insert` is infallible; it does not return a `Result`.

To `server/src/lib.rs`, add:

```rust
#[spacetimedb(reducer)]
/// Clients invoke this reducer to send messages.
pub fn send_message(ctx: ReducerContext, text: String) -> Result<(), String> {
    let text = validate_message(text)?;
    log::info!("{}", text);
    Message::insert(Message {
        sender: ctx.sender,
        text,
        sent: ctx.timestamp,
    });
    Ok(())
}
```

We'll want to validate messages' texts in much the same way we validate users' chosen names. As above, we'll do the bare minimum, rejecting only empty messages.

To `server/src/lib.rs`, add:

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

Whenever a client connects, the module will run a special reducer, annotated with `#[spacetimedb(connect)]`, if it's defined. By convention, it's named `identity_connected`. We'll use it to create a `User` record for the client if it doesn't yet exist, and to set its online status.

We'll use `User::filter_by_identity` to look up a `User` row for `ctx.sender`, if one exists. If we find one, we'll use `User::update_by_identity` to overwrite it with a row that has `online: true`. If not, we'll use `User::insert` to insert a new row for our new user. All three of these methods are generated by the `#[spacetimedb(table(...))]` attribute, with rows and behavior based on the row attributes. `filter_by_identity` returns an `Option<User>`, because the unique constraint from the `#[primarykey]` attribute means there will be either zero or one matching rows. `insert` returns a `Result<(), UniqueConstraintViolation>` because of the same unique constraint; if we want to overwrite a `User` row, we need to do so explicitly using `update_by_identity`.

To `server/src/lib.rs`, add the definition of the connect reducer:

```rust
#[spacetimedb(connect)]
// Called when a client connects to the SpacetimeDB
pub fn identity_connected(ctx: ReducerContext) {
    if let Some(user) = User::filter_by_identity(&ctx.sender) {
        // If this is a returning user, i.e. we already have a `User` with this `Identity`,
        // set `online: true`, but leave `name` and `identity` unchanged.
        User::update_by_identity(&ctx.sender, User { online: true, ..user });
    } else {
        // If this is a new user, create a `User` row for the `Identity`,
        // which is online, but hasn't set a name.
        User::insert(User {
            name: None,
            identity: ctx.sender,
            online: true,
        }).unwrap();
    }
}
```

Similarly, whenever a client disconnects, the module will run the `#[spacetimedb(disconnect)]` reducer if it's defined. By convention, it's named `identity_disconnect`. We'll use it to un-set the `online` status of the `User` for the disconnected client.

```rust
#[spacetimedb(disconnect)]
// Called when a client disconnects from SpacetimeDB
pub fn identity_disconnected(ctx: ReducerContext) {
    if let Some(user) = User::filter_by_identity(&ctx.sender) {
        User::update_by_identity(&ctx.sender, User { online: false, ..user });
    } else {
        // This branch should be unreachable,
        // as it doesn't make sense for a client to disconnect without connecting first.
        log::warn!("Disconnect event for unknown user with identity {:?}", ctx.sender);
    }
}
```

## Publish the module

And that's all of our module code! We'll run `spacetime publish` to compile our module and publish it on SpacetimeDB. `spacetime publish` takes an optional name which will map to the database's unique address. Clients can connect either by name or by address, but names are much more pleasant. Come up with a unique name that contains only URL-safe characters (letters, numbers, hyphens and underscores), and fill it in where we've written `<module-name>`.

From the `quickstart-chat` directory, run:

```bash
spacetime publish --project-path server <module-name>
```

## Call Reducers

You can use the CLI (command line interface) to run reducers. The arguments to the reducer are passed in JSON format.

```bash
spacetime call <module-name> send_message 'Hello, World!'
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

You can find the full code for this module [in the SpacetimeDB module examples](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/quickstart-chat).

You've just set up your first database in SpacetimeDB! The next step would be to create a client module that interacts with this module. You can use any of SpacetimDB's supported client languages to do this. Take a look at the quickstart guide for your client language of choice: [Rust](/docs/sdks/rust/quickstart), [C#](/docs/sdks/c-sharp/quickstart), [TypeScript](/docs/sdks/typescript/quickstart) or [Python](/docs/sdks/python/quickstart).

If you are planning to use SpacetimeDB with the Unity game engine, you can skip right to the [Unity Comprehensive Tutorial](/docs/unity/part-1) or check out our example game, [BitcraftMini](/docs/unity/part-3).
