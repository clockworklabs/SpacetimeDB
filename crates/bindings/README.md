# SpacetimeDB Rust Module SDK

<!-- n.b. This file is used as the top-level library documentation in `src/lib.rs`. -->

SpacetimeDB allows using the Rust language to write server-side applications. These applications are called **modules** and have access to a built-in database.

Rust modules are written with the the Rust Module SDK (this crate). They are built using [cargo](https://doc.rust-lang.org/cargo/) and deployed using the [`spacetime` CLI tool](https://spacetimedb.com/install). Rust modules can import any Rust [crate](https://crates.io/) that supports being compiled to WebAssembly.

This reference assumes you are familiar with the basics of Rust. If you aren't, check out Rust's [excellent documentation](https://www.rust-lang.org/learn). For a guided introduction to Rust Modules, see the [rust module quickstart](https://spacetimedb.com/docs/modules/rust/quickstart).

## Overview

SpacetimeDB modules have two ways to interact with the outside world. They can:

- Declare [tables](#tables), which are exactly like tables in a SQL database.
- Declare [reducers](#reducers), which are public functions that can be invoked by [clients](https://spacetimedb.com/docs/#client) over the network.

Tables and reducers are declared using ordinary Rust code, annotated with special macros. Declarations can use any type deriving the [`SpacetimeType`](#spacetimetype) trait.

The `spacetime publish` command compiles a module and uploads it to the public SpacetimeDB host. After this:
- The host loads the module into memory and starts running it.
- If needed, the host creates a new [`Identity`](#identity) and assigns it to the module.
  If the module is already running, its existing `Identity` will be reused.
- The host creates a persistent database attached to the module, with all of the requested tables.
  If a database already exists, the host tries to [automatically migrate](#automatic-migrations) it to the current
  schema.
- The host begins running the module's [lifecycle reducers](#life-cycle-annotations) and [scheduled reducers](#scheduled-reducers).
- The host allows clients to connect to the module.
  Connected clients can subscribe to [public tables](#public-and-private-tables) and call [reducers](#reducers).

(The easiest way to make requests to a module is to use the [SpacetimeDB client SDKs](https://spacetimedb.com/docs/sdks).)

Reducers run in [transactions](https://en.wikipedia.org/wiki/Database_transaction) that allow access to the database. Reducers can see information about the [Identity](#identity) and [Address](#address) of their callers, and use this to determine what a client should be allowed to do. Reducers that [`panic!()`](https://doc.rust-lang.org/std/macro.panic.html) have any modifications they made to the database automatically rolled back.

The module SDK has built-in support for the [log crate](https://docs.rs/log/latest/log/index.html). All modules automatically install a suitable logger when they are first loaded by SpacetimeDB. Log macros can be used anywhere in module code, and log outputs can be inspected using the `spacetime logs` command.

## Setup

To create a Rust module, install [`spacetime` CLI tool](https://spacetimedb.com/install) in your preferred shell. Navigate to your work directory and run the following command:

```text
spacetime init --lang rust my-project-directory
```

This creates a Cargo project with the following `Cargo.toml`:

```text
[package]
name = "spacetime-module"
version = "0.1.0"
edition = "2021"

# See more keys and their definitions at https://doc.rust-lang.org/cargo/reference/manifest.html

[lib]
crate-type = ["cdylib"]

[dependencies]
spacetimedb = "1.0.0-rc2"
log = "0.4"
```

This is a standard `Cargo.toml`, with the exception of the line `crate-type = ["cdylib"]`.
This line is important: it allows the project to be compiled to a WebAssembly module. 

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

After modifying your project, you can run `spacetime publish` again to rebuild and upload it. SpacetimeDB [automatically migrates](#automatic-migrations) your database schema whenever you run `spacetime publish`.

## How it works

Under the hood, SpacetimeDB modules are WebAssembly modules that import a [specific WebAssembly ABI](https://spacetimedb.com/docs/webassembly-abi) and export a small number of special functions. This is automatically configured when you add the `spacetime` crate as a dependency of your application.

The SpacetimeDB host is an application that knows how to load and run SpacetimeDB modules. It is [open source](https://github.com/clockworklabs/SpacetimeDB). You can run your own host, or you can upload your module to the public SpacetimeDB network. <!-- TODO(1.0): want a link to some dashboard for the public network. -->

## Tables

Tables are declared using the [`#[table(name = table_name)]` macro](https://docs.rs/spacetimedb/latest/spacetimedb/attr.table.html).   

This macro is applied to a Rust struct with named fields. All of the fields of the table must implement the [`SpacetimeType` trait](#spacetimetype).

The resulting type is used to store rows of the table. It is normal struct type. Row values are not special -- operations on them do not, by themselves, modify the table. Instead, a [`ReducerContext`](#reducercontext) is used to get access to the global database.

```rust
use spacetimedb::{table, ReducerCtx};

/// A `Person` is a row of the table `people`.
#[table(name = people, public)]
pub struct Person {
    #[unique]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    name: String,
}

// `Person` is a normal Rust struct type.
// Operations on a `Person` do not, by themselves, do anything.
// The following function does not interact with the database at all.
fn does_nothing() {
    // Creating a `Person` does not modify the database.
    let mut person = Person { id: 0, name: "Joe Banana".to_string() };
    // Updating a `Person` does not modify the database.
    person.name = "Joanna Banana";
    // Dropping a `Person` does not modify the database.
    drop(person);
}

// To interact with the database, you need a `ReducerContext`.
fn does_something(ctx: &ReducerCtx) {
    // `ctx.db.table_name()` gets a handle to a table.
    let people = ctx.db.people();

    // The following inserts a row into the global database:
    let mut person = people.insert(Person { id: 0, name: "Joe Banana".to_string() });

    // Next, the row is updated:
    person.name = "Joanna Banana".to_string();
    person = people.update_by_id(person);
    
    // And then removed:
    people.delete_by_id(person.id);
}
```

### Public and Private tables

By default, tables are considered **private**. This means that they are only readable by the table owner, and by server module code.
The `#[table(name = table_name, public)]` macro makes a table public. **Public** tables are readable by all users, but can still only be modified by your server module code.

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

To learn how to subscribe to a table, see the [client SDK documentation](https://spacetimedb.com/docs/sdks).

### Generated functions on a SpacetimeDB table

<!-- TODO: rewrite this section -->

We'll work off these structs to see what functions SpacetimeDB generates:

This table has a plain old column.

```rust
#[table(name = ordinary, public)]
struct Ordinary {
    ordinary_field: u64,
}
```

This table has a unique column. Every row in the `Unique` table must have distinct values of the `unique_field` column. Attempting to insert a row with a duplicate value will fail.

```rust
#[table(name = unique, public)]
struct Unique {
    // A unique column:
    #[unique]
    unique_field: u64,
}
```

This table has an automatically incrementing column. SpacetimeDB automatically provides an incrementing sequence of values for this field, and sets the field to that value when you insert the row.

Only integer types can be `#[unique]`: `u8`, `u16`, `u32`, `u64`, `u128`, `i8`, `i16`, `i32`, `i64` and `i128`.

```rust
#[table(name = autoinc, public)]
struct Autoinc {
    #[autoinc]
    autoinc_field: u64,
}
```

These attributes can be combined, to create an automatically assigned ID usable for filtering.

```rust
#[table(name = identity, public)]
struct Identity {
    #[autoinc]
    #[unique]
    id_field: u64,
}
```

#### Insertion

We'll talk about insertion first, as there a couple of special semantics to know about.

When we define |Ordinary| as a SpacetimeDB table, we get the ability to insert into it with the generated `ctx.db.ordinary().insert(..)` method.

Inserting takes a single argument, the row to insert. When there are no unique fields in the row, the return value is the inserted row.

```rust
#[reducer]
fn insert_ordinary(ctx: &ReducerContext, value: u64) {
    let ordinary = Ordinary { ordinary_field: value };
    let result = ctx.db.ordinary().insert(ordinary);
    assert_eq!(ordinary.ordinary_field, result.ordinary_field);
}
```

When there is a unique column constraint on the table, insertion can fail if a uniqueness constraint is violated.

If we insert two rows which have the same value of a unique column, the second will fail.

```rust
#[reducer]
fn insert_unique(ctx: &ReducerContext, value: u64) {
    let result = ctx.db.unique().insert(Unique { unique_field: value });
    assert!(result.is_ok());

    let result = ctx.db.unique().insert(Unique { unique_field: value });
    assert!(result.is_err());
}
```

When inserting a table with an `#[autoinc]` column, the database will automatically overwrite whatever we give it with an atomically increasing value.

The returned row has the `autoinc` column set to the value that was actually written into the database.

```rust
#[reducer]
fn insert_autoinc(ctx: &ReducerContext) {
    for i in 1..=10 {
        // These will have values of 1, 2, ..., 10
        // at rest in the database, regardless of
        // what value is actually present in the
        // insert call.
        let actual = ctx.db.autoinc().insert(Autoinc { autoinc_field: 23 })
        assert_eq!(actual.autoinc_field, i);
    }
}

#[reducer]
fn insert_id(ctx: &ReducerContext) {
    for _ in 0..10 {
        // These also will have values of 1, 2, ..., 10.
        // There's no collision and silent failure to insert,
        // because the value of the field is ignored and overwritten
        // with the automatically incremented value.
        ctx.db.identity().insert(Identity { id_field: 23 })
    }
}
```

#### Iterating

Given a table, we can iterate over all the rows in it.

```rust
#[table(name = person, public)]
struct Person {
    #[unique]
    id: u64,

    #[index(btree)]
    age: u32,
    name: String,
    address: String,
}
```

// Every table structure has a generated iter function, like:

```rust
ctx.db.my_table().iter()
```

`iter()` returns a regular old Rust iterator, giving us a sequence of `Person`. The database sends us over rows, one at a time, for each time through the loop. This means we get them by value, and own the contents of `String` fields and so on.

```
#[reducer]
fn iteration(ctx: &ReducerContext) {
    let mut addresses = HashSet::new();

    for person in ctx.db.person().iter() {
        addresses.insert(person.address);
    }

    for address in addresses.iter() {
        println!("{address}");
    }
}
```

#### Filtering

Often, we don't need to look at the entire table, and instead are looking for rows with specific values in certain columns.

Our `Person` table has a unique id column, so we can filter for a row matching that ID. Since it is unique, we will find either 0 or 1 matching rows in the database. This gets represented naturally as an `Option<Person>` in Rust. SpacetimeDB automatically creates and uses indexes for filtering on unique columns, so it is very efficient.

The name of the filter method just corresponds to the column name.

```rust
#[reducer]
fn filtering(ctx: &ReducerContext, id: u64) {
    match ctx.db.person().id().find(id) {
        Some(person) => println!("Found {person}"),
        None => println!("No person with id {id}"),
    }
}
```

Our `Person` table also has an index on its `age` column. Unlike IDs, ages aren't unique. Filtering for every person who is 21, then, gives us an `Iterator<Item = Person>` rather than an `Option<Person>`.

```rust
#[reducer]
fn filtering_non_unique(ctx: &ReducerContext) {
    for person in ctx.db.person().age().filter(21u32) {
        println!("{} has turned 21", person.name);
    }
}
```

> NOTE: An unfortunate interaction between Rust's trait solver and integer literal defaulting rules means that you must specify the types of integer literals passed to `filter` and `find` methods via the suffix syntax, like `21u32`. If you don't, you'll see a compiler error like:
> ```
> error[E0271]: type mismatch resolving `<i32 as FilterableValue>::Column == u32`
>    --> modules/rust-wasm-test/src/lib.rs:356:48
>     |
> 356 |     for person in ctx.db.person().age().filter(21) {
>     |                                         ------ ^^ expected `u32`, found `i32`
>     |                                         |
>     |                                         required by a bound introduced by this call
>     |
>     = note: required for `i32` to implement `BTreeIndexBounds<(u32,), SingleBound>`
> note: required by a bound in `BTreeIndex::<Tbl, IndexType, Idx>::filter`
>     |
> 410 |     pub fn filter<B, K>(&self, b: B) -> impl Iterator<Item = Tbl::Row>
>     |            ------ required by a bound in this associated function
> 411 |     where
> 412 |         B: BTreeIndexBounds<IndexType, K>,
>     |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ required by this bound in `BTreeIndex::<Tbl, IndexType, Idx>::filter`
> ```

#### Deleting

Like filtering, we can delete by an indexed or unique column instead of the entire row.

```rust
#[reducer]
fn delete_id(ctx: &ReducerContext, id: u64) {
    ctx.db.person().id().delete(id)
}
```

## Reducers

Reducers are declared using the [`#[reducer]` macro](https://docs.rs/spacetimedb/latest/spacetimedb/attr.reducer.html).

`#[reducer]` is always applied to top level Rust functions. They can take arguments of types implementing [`SpacetimeType`](#spacetimetype), and either return nothing, or return a `Result<(), E>`, where `E` implements `Debug`.

```rust
#[reducer]
fn give_player_item(ctx: &ReducerContext, player_id: u64, item_id: u64) {
    /* ... */
}
```

### `ReducerContext`

Reducers have access to a special [`ReducerContext`](https://docs.rs/spacetimedb/latest/spacetimedb/struct.ReducerContext.html) argument. This argument allows reading and writing the database attached to a module. It also provides some additional functionality, like generating random numbers and scheduling future operations.

The most important field of `ReducerContext` is `.db`. This field provides a [local view](https://docs.rs/spacetimedb/latest/spacetimedb/struct.Local.html) of the module's database. The `#[table]` macro generates traits that add accessor methods to this field.

To see all of the available methods on `ReducerContext.db`, run `cargo doc` in your module's directory, and navigate to the `spacetimedb::Local` struct in the generated documentation.

### Life cycle annotations

### Scheduled reducers

In addition to life cycle annotations, reducers can be made **scheduled** using the `scheduled` attribute. 

```rust
// The `scheduled` attribute links this table to a reducer.
#[table(name = send_message_timer, scheduled(send_message)]
struct SendMessageTimer {
    text: String,
}
```

The `scheduled` attribute adds a couple of default fields and expands as follows:

```rust
#[table(name = send_message_timer, scheduled(send_message)]
 struct SendMessageTimer {
    text: String,   // original field
    #[primary_key]
    #[autoinc]
    scheduled_id: u64, // identifier for internal purpose
    scheduled_at: ScheduleAt, //schedule details
}

pub enum ScheduleAt {
    /// A specific time at which the reducer is scheduled.
    /// Value is a UNIX timestamp in microseconds.
    Time(u64),
    /// A regular interval at which the repeated reducer is scheduled.
    /// Value is a duration in microseconds.
    Interval(u64),
}
```

Managing timers with a scheduled table is as simple as inserting or deleting rows from the table.

```rust
#[reducer]
// Reducers linked to the scheduler table should have their first argument as `&ReducerContext`
// and the second as an instance of the table struct it is linked to.
fn send_message(ctx: &ReducerContext, arg: SendMessageTimer) -> Result<(), String> {
    // ...
}

// Scheduling reducers inside `init` reducer
#[reducer(init)]
fn init(ctx: &ReducerContext) {
    // Scheduling a reducer for a specific Timestamp
    ctx.db.send_message_timer().insert(SendMessageTimer {
        scheduled_id: 1,
        text:"bot sending a message".to_string(),
        //`spacetimedb::Timestamp` implements `From` trait to `ScheduleAt::Time`.
        scheduled_at: ctx.timestamp.plus(Duration::from_secs(10)).into()
    });

    // Scheduling a reducer to be called at fixed interval of 100 milliseconds.
    ctx.db.send_message_timer().insert(SendMessageTimer {
        scheduled_id: 0,
        text:"bot sending a message".to_string(),
        //`std::time::Duration` implements `From` trait to `ScheduleAt::Duration`.
        scheduled_at: duration!(100ms).into(),
    });
}
```

## `SpacetimeType`

Any Rust type implementing the [`SpacetimeType` trait](https://docs.rs/spacetimedb/latest/spacetimedb/trait.SpacetimeType.html) can be used in table and reducer declarations. A derive macro is provided, and can be used on both structs and enums:

```rust
use spacetimedb::SpacetimeType;

#[derive(SpacetimeType)]
struct Location {
    x: u32,
    y: u32
}

#[derive(SpacetimeType)]
enum FruitCrate {
    Bananas { count: u32, freshness: u32 },
    Plastic { count: u32 }
}
```

The fields of the struct/enum must also implement `SpacetimeType`.

SpacetimeType is implemented for many of the primitive types in the standard library:

- `bool`
- `u8`, `u16`, `u32`, `u64`, `u128`
- `i8`, `i16`, `i32`, `i64`, `i128`
- `f32`, `f64`

And common data structures:

- `String` and `&str`, utf-8 string data
- `()`, the unit type
- `Option<T> where T: SpacetimeType`
- `Vec<T> where T: SpacetimeType`

(Storing collections in database tables is a form of [denormalization](https://en.wikipedia.org/wiki/Denormalization).)

All `#[table(..)]` types automatically derive `SpacetimeType`.

Types deriving `SpacetimeType` also automatically derive the [`Serialize`](https://docs.rs/spacetimedb/latest/spacetimedb/trait.Serialize.html) and [`Deserialize`](https://docs.rs/spacetimedb/latest/spacetimedb/trait.Deserialize.html) traits, as well as the [`std::Debug`](https://doc.rust-lang.org/std/fmt/trait.Debug.html) trait. (There are currently no trait bounds on `SpacetimeType` documenting this fact.)

The `Serialize` and `Deserialize` traits are used to convert Rust data structures to other formats, suitable for storing on disk or passing over the network.

## Automatic migrations


<!-- TODO: consume or destroy the following

Now we'll get into details on all the macro APIs SpacetimeDB provides, starting with all the variants of the `spacetimedb` attribute.

### Defining tables

```rust
#[table(name = my_table, public)]
struct MyTable {
    field1: String,
    field2: u32,
}
```

This attribute is applied to Rust structs in order to create corresponding tables in SpacetimeDB. Fields of the Rust struct correspond to columns of the database table.

```rust
#[table(name = another_table, public)]
struct AnotherTable {
    // Fine, some builtin types.
    id: u64,
    name: Option<String>,

    // Fine, another table type.
    table: Table,

    // Fine, another type we explicitly make serializable.
    serial: Serial,
}
```

If you want to have a field that is not one of the above primitive types, and not a table of its own, you can derive the `SpacetimeType` attribute on it.

We can derive `SpacetimeType` on `struct`s and `enum`s with members that are themselves `SpacetimeType`s.

```rust
#[derive(SpacetimeType)]
enum Serial {
    Builtin(f64),
    Compound {
        s: String,
        bs: Vec<bool>,
    }
}
```

Once the table is created via the macro, other attributes described below can control more aspects of the table. For instance, a particular column can be indexed, or take on values of an automatically incremented counter. These are described in detail below.

```rust
#[table(name = person, public)]
struct Person {
    #[unique]
    id: u64,

    name: String,
    address: String,
}
```

### Defining reducers


Note that reducers can call non-reducer functions, including standard library functions.


## Client API

Besides the macros for creating tables and reducers, there's two other parts of the Rust SpacetimeDB library. One is a collection of macros for logging, and the other is all the automatically generated functions for operating on those tables.

### `println!` and friends

Because reducers run in a WASM sandbox, they don't have access to general purpose I/O from the Rust standard library. There's no filesystem or network access, and no input or output. This means no access to things like `std::println!`, which prints to standard output.

SpacetimeDB modules have access to logging output. These are exposed as macros, just like their `std` equivalents. The names, and all the Rust formatting machinery, work the same; just the location of the output is different.

Logs for a module can be viewed with the `spacetime logs` command from the CLI.

```rust
use spacetimedb::{
    println,
    print,
    eprintln,
    eprint,
    dbg,
};

#[reducer]
fn output(ctx: &ReducerContext, i: i32) {
    // These will be logged at log::Level::Info.
    println!("an int with a trailing newline: {i}");
    print!("some more text...\n");

    // These log at log::Level::Error.
    eprint!("Oops...");
    eprintln!(", we hit an error");

    // Just like std::dbg!, this prints its argument and returns the value,
    // as a drop-in way to print expressions. So this will print out |i|
    // before passing the value of |i| along to the calling function.
    //
    // The output is logged log::Level::Debug.
    ctx.db.outputted_number().insert(dbg!(i));
}
```
-->

[macro library]: https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/bindings-macro
[module library]: https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/lib
[demo]: /#demo
