# SpacetimeDB Rust Module Library

<!-- n.b. This file is used as the top-level library documentation in `src/lib.rs`.
          Some of the links in this file are not resolved when previewing on GitHub,
          but *are* resolved when compiled by Rustdoc.
-->

[SpacetimeDB](https://spacetimedb.com/) allows using the Rust language to write server-side applications called **modules**. Modules run **inside** a SQL database. They have direct access to database tables, and expose public functions called **reducers** that can be invoked over the network. Clients connect directly to the database to read data.

```text
    Client Application                          SpacetimeDB
┌───────────────────────┐                ┌───────────────────────┐
│                       │                │                       │
│  ┌─────────────────┐  │    SQL Query   │  ┌─────────────────┐  │
│  │ Subscribed Data │<─────────────────────│    Database     │  │
│  └─────────────────┘  │                │  └─────────────────┘  │
│           │           │                │           ^           │
│           │           │                │           │           │
│           v           │                │           v           │
│  +─────────────────┐  │ call_reducer() │  ┌─────────────────┐  │
│  │   Client Code   │─────────────────────>│   Module Code   │  │
│  └─────────────────┘  │                │  └─────────────────┘  │
│                       │                │                       │
└───────────────────────┘                └───────────────────────┘
```

Rust modules are written with the the Rust Module Library (this crate). They are built using [cargo](https://doc.rust-lang.org/cargo/) and deployed using the [`spacetime` CLI tool](https://spacetimedb.com/install). Rust modules can import any Rust [crate](https://crates.io/) that supports being compiled to WebAssembly.

(Note: Rust can also be used to write **clients** of SpacetimeDB databases, but this requires using a completely different library, the SpacetimeDB Rust Client SDK. See the documentation on [clients] for more information.)

This reference assumes you are familiar with the basics of Rust. If you aren't, check out Rust's [excellent documentation](https://www.rust-lang.org/learn). For a guided introduction to Rust Modules, see the [Rust Module Quickstart](https://spacetimedb.com/docs/modules/rust/quickstart).

## Overview

SpacetimeDB modules have two ways to interact with the outside world: tables and reducers.

- [Tables](#tables) store data and optionally make it readable by [clients]. 

- [Reducers](#reducers) are functions that can modify data and be invoked by [clients] over the network. They can read and write data in tables, and write to a private debug log.

Both of these can be declared in Rust code:

```no_run
use spacetimedb::{table, reducer, ReducerContext, Table};

#[table(name = people)]
pub struct Person {
    id: u32,
    name: String
}

#[reducer]
fn add_person(ctx: &ReducerContext, id: u32, name: String) {
    ctx.db.people().insert(Person { id, name });
}
```

Reducers and tables are the only ways for a SpacetimeDB module to interact with the outside world. Calling functions from `std::net` or `std::fs` inside a reducer will result in runtime errors.

Reducers don't return data directly; they can only modify the database. Clients connect directly to the database and use SQL to query [public](#public-and-private-tables) tables. Clients can also open subscriptions to receive streaming updates as the results of a SQL query change.

Tables and reducers in Rust modules can use any type that implements the [`SpacetimeType`] trait.

<!-- TODO: link to client subscriptions / client one-off queries respectively. -->



<!-- 
SpacetimeDB modules are compiled to WebAssembly by `cargo` and administered using the `spacetime` CLI command. Modules run on a server called a [host]. A host can run many modules at a time. You can run your own host, or use a public host administered by [Clockwork Labs](https://clockworklabs.io/). (TODO: remark about SLAs and SKUs once those are finalized?)

SpacetimeDB is a SQL database, and builds on the long tradition of reliability offered by SQL databases. Tables and reducers are built on SQL concepts:

- Tables are SQL database tables with easy-to-use Rust interfaces. They are declared in Rust code using the `#[spacetime::table]` macro. Clients can open read-only subscriptions to [`public`](#public-and-private-tables) tables, and SpacetimeDB will automatically stream updates to them as those tables change. Tables are automatically logged to disk and are durable across system restarts and crashes. Tables can be queried with SQL; SpacetimeDB supports a subset of ANSI:SQL 2011. (TODO: document precisely which subset this is.)

- Reducers are Rust functions decorated with the `#[spacetime::reducer]` macro. Reducers run in [transactions](#transactions) with read-write access to the entire database; if a reducer returns an error or [panic](std::panic!), its modifications to the database will be rolled back. Reducers run on the server, not on the client; they can see information about the [Identity](#identity) and [Address](#address) of their callers, and use this to determine what clients should be allowed to do. (TODO: what SQL transaction level do we implement?)

Tables can store any any Rust type implementing the [`SpacetimeType`, `Serialize`, and `Deserialize`](#spacetimetype) traits; all of these can be be derived at once using `#[derive(SpacetimeType)]`. Similarly, Rust types implementing these traits can be used for reducer arguments.

`Serialize` and `Deserialize` allow types to automatically serialize and deserialize themselves, in a manner similar to [`serde`](https://serde.rs/). `SpacetimeType` allows types to register their internal structure with `SpacetimeDB`. This allows SpacetimeDB to correctly format tables storing these types.

Importantly, the data provided by `SpacetimeType` also enables the `spacetime generate` CLI command. This command can be used to generate bindings to a module in any supported client language. See the documentation on [client SDKs](https://spacetimedb.com/docs/#client) for more information.
-->

## Setup

To create a Rust module, install [`spacetime` CLI tool](https://spacetimedb.com/install) in your preferred shell. Navigate to your work directory and run the following command:

```text
spacetime init --lang rust my-project-directory
```

This creates a Cargo project in `my-project-directory` with the following `Cargo.toml`:

```text
[package]
name = "spacetime-module"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
spacetimedb = "1.0.0-rc2"
log = "0.4"
```
<!-- TODO: update `spacetimedb` version there. -->

This is a standard `Cargo.toml`, with the exception of the line `crate-type = ["cdylib"]`.
This line is important: it allows the project to be compiled to a WebAssembly module. 

The project's `lib.rs` will contain the following skeleton:

```rust
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String
}

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {
    // Called when the module is initially published
}

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(_ctx: &ReducerContext) {
    // Called everytime a new client connects
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(_ctx: &ReducerContext) {
    // Called everytime a client disconnects
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}
```

This skeleton declares a [table](#tables), some [reducers](#reducers), and some [lifecycle reducers](#lifecycle-reducers).

To compile the project, run the following command:

```text
spacetime build
```

SpacetimeDB requires a WebAssembly-compatible Rust toolchain. If the `spacetime` cli finds a compatible version of [`rustup`](https://rustup.rs/) that it can run, it will automatically install the `wasm32-unknown-unknown` target and use it to build your application. This can also be done manually using the command:

```text
rustup target add wasm32-unknown-unknown
```

If you are managing your Rust installation in some other way, you will need to install the `wasm32-unknown-unknown` target yourself.

To build your application and upload it to the public SpacetimeDB network, run:

```text
spacetime login
spacetime publish
```

When you publish your module, a database will be created with the requested tables, and the module will be installed inside it.

The output of `spacetime publish` will end with a line:
```text
Created new database with identity: <hex string>
```

This hex string is the [`Identity`] of the created database. It distinguishes the created database from the other databases running on the SpacetimeDB network.  It is used when administering the module, for example using the [`spacetime logs <DATABASE_IDENTITY>`](#the-log-crate) command. You should save it to a text file so that you can remember it. <!-- TODO: is there a CLI command that says "list all Identities running this version of a module?" -->

After modifying your project, you can run:

`spacetime publish <DATABASE_IDENTITY>`

to update the module attached to your database. Note that SpacetimeDB tries to [automatically migrate](#automatic-migrations) your database schema whenever you run `spacetime publish`.

You can also generate code for clients of your module using the `spacetime generate` command. See the [client SDK documentation] for more information.

## How it works

Under the hood, SpacetimeDB modules are WebAssembly modules that import a [specific WebAssembly ABI](https://spacetimedb.com/docs/webassembly-abi) and export a small number of special functions. This is automatically configured when you add the `spacetime` crate as a dependency of your application.

The SpacetimeDB host is an application that hosts SpacetimeDB databases. It is [source available](https://github.com/clockworklabs/SpacetimeDB). You can run your own host, or you can upload your module to the public SpacetimeDB network. <!-- TODO: want a link to some dashboard for the public network. --> The network will create a database for you and install your module in it to serve client requests.

#### In More Detail: Publishing a Module

The `spacetime publish [DATABASE_IDENTITY]` command compiles a module and uploads it to a SpacetimeDB host. After this:
- The host finds the database with the requested `DATABASE_IDENTITY`.
  - (Or creates a fresh database and identity, if no identity was provided).
- The host loads the new module and inspects its requested database schema. If there are changes to the schema, the host tries perform an [automatic migration](#automatic-migrations). If the migration fails, publishing fails.
- The host terminates the old module attached to the database.
- The host installs the new module into the database. It begins running the module's [lifecycle reducers](#lifecycle-reducers) and [scheduled reducers](#scheduled-reducers).
- The host begins allowing clients to call the module's reducers.

From the perspective of clients, this process is mostly seamless. Open connections are maintained and subscriptions continue functioning. [Automatic migrations](#automatic-migrations) forbid most table changes except for adding new tables, so client code does not need to be recompiled.
However:
- Clients may witness a brief interruption in the execution of scheduled reducers (for example, game loops.)
- New versions of a module may remove or change reducers that were previously present. Client code calling those reducers will receive runtime errors.


## Tables

Tables are declared using the [`#[table(name = table_name)]` macro](macro@crate::table).

This macro is applied to a Rust struct with named fields. All of the fields of the table must implement [`SpacetimeType`].

The resulting type is used to store rows of the table. It is normal struct type. Row values are not special -- operations on row types do not, by themselves, modify the table. Instead, a [`ReducerContext`](#reducercontext) is needed to get a handle to the table.

```rust
use spacetimedb::{table, reducer, ReducerContext, Table};

/// A `Person` is a row of the table `people`.
#[table(name = people, public)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    name: String,
}

// `Person` is a normal Rust struct type.
// Operations on a `Person` do not, by themselves, do anything.
// The following function does not interact with the database at all.
fn do_nothing() {
    // Creating a `Person` DOES NOT modify the database.
    let mut person = Person { id: 0, name: "Joe Average".to_string() };
    // Updating a `Person` DOES NOT modify the database.
    person.name = "Joanna Average".to_string();
    // Dropping a `Person` DOES NOT modify the database.
    drop(person);
}

// To interact with the database, you need a `ReducerContext`.
// The first argument of a reducer is always a `ReducerContext`.
#[reducer]
fn do_something(ctx: &ReducerContext) {
    // `ctx.db.{table_name}()` gets a handle to a database table.
    let people: people__TableHandle = ctx.db.people();

    // The following inserts a row into the table:
    let mut person = people.insert(Person { id: 0, name: "Joe Average".to_string() });

    // `person` is a COPY of the row stored in the database.
    // If we update it:
    person.name = "Joanna Average".to_string();
    // Our copy is now updated, but the database's copy is UNCHANGED.
    // To push our change through, we can call an `update_by_...` function:
    person = people.update_by_id(person);
    // Now the database and our copy are in sync again.
    
    // We can also delete the row in the database using a `delete_by_...`.
    people.delete_by_id(person.id);
}
```

See [reducers](#reducers) for more information on declaring reducers.
See the [`#[table]` macro](macro@crate::table) for more information on declaring and using tables.

#### Public and Private tables

By default, tables are considered **private**. This means that they are only readable by the table owner and by reducers. Reducers run inside the database, so clients cannot see private tables at all.

The `#[table(name = table_name, public)]` macro makes a table public. **Public** tables are readable by all clients. They can still only be modified by reducers. 

```rust
use spacetimedb::table;

// The `players` table can be read by all connected clients.
#[table(name = players, public)]
pub struct Player {
    /* ... */
}

// The `loot_items` table is invisible to clients, but not to reducers.
#[table(name = loot_items)]
pub struct LootItem {
    /* ... */
}
```

<!-- TODO: can module owner `spacetime sql` write/read private tables? -->

To learn how to subscribe to a public table, see the [client SDK documentation](https://spacetimedb.com/docs/sdks).
<!-- TODO: more specific link. -->

## Reducers

Reducers are declared using the [`#[reducer]` macro](macro@crate::reducer).

`#[reducer]` is always applied to top level Rust functions. Arguments of reducers must implement [`SpacetimeType`]. Reducers can either return nothing, or return a `Result<(), E>`, where `E` implements `Debug`.

```rust
use spacetimedb::{reducer, ReducerContext};

#[derive(Debug)]
enum GiveItemError {
    NoSuchItem(u64),
    NonTransferable(u64)
}

#[reducer]
fn give_player_item(ctx: &ReducerContext, player_id: u64, item_id: u64) {
    /* ... */
}
```

Every reducer runs inside a [database transaction](https://en.wikipedia.org/wiki/Database_transaction). <!-- TODO: specific transaction level guarantees. --> This means that reducers will not observe the effects of other reducers modifying the database while they run. Also, if a reducer fails, all of its changes to the database will automatically be rolled back. Reducers can fail by [panicking](::std::panic!) or by returning an `Err`.

#### The `ReducerContext` Type

Reducers have access to a special [`ReducerContext`] argument. This argument allows reading and writing the database attached to a module. It also provides some additional functionality, like generating random numbers and scheduling future operations.

The most important field of [`ReducerContext`] is [`.db`](ReducerContext#structfield.db). This field provides a view of the module's database. The [`#[table]`](macro@crate::table) macro generates traits that add accessor methods to this field.

<!-- TODO: this seems to work sometimes, but not always... Sometimes the links to downstream trait implementations aren't generated for some reason. Maybe it only works in the same cargo workspace?

To see all of the available methods on `ctx.db`, run `cargo doc` in your module's directory, and navigate to the `spacetimedb::Local` struct in the generated documentation. This will be at the path:
- `[your_project_directory]/target/doc/spacetimedb/struct.Local.html` (non-Windows)
- `[your_project_directory]\target\doc\spacetimedb\struct.Local.html` (Windows)
-->

#### The `log` crate

SpacetimeDB Rust modules have built-in support for the [log crate](log). All modules automatically install a suitable logger when they are first loaded by SpacetimeDB. (At time of writing, this happens [here](https://github.com/clockworklabs/SpacetimeDB/blob/e9e287b8aab638ba6e8bf9c5d41d632db041029c/crates/bindings/src/logger.rs)). Log macros can be used anywhere in module code, and log outputs of a running module can be inspected using the `spacetime logs` command:

```text
spacetime logs <DATABASE_IDENTITY>
```

#### Lifecycle Reducers

A small group of reducers are called at set points in the module lifecycle. These are used to initialize
the database and respond to client connections. See [Lifecycle Reducers](macro@crate::reducer#lifecycle-reducers).

#### Scheduled Reducers

Reducers can be scheduled to run repeatedly. This can be used to implement timers, game loops, and
maintenance tasks. See [Scheduled Reducers](macro@crate::reducer#scheduled-reducers).

## Automatic migrations

[macro library]: https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/bindings-macro
[module library]: https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/lib
[demo]: /#demo
[clients]: https://spacetimedb.com/docs/#client
[client SDK documentation]: https://spacetimedb.com/docs/#client
[host]: https://spacetimedb.com/docs/#host