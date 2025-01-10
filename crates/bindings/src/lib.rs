#![doc = include_str!("../README.md")]

mod client_visibility_filter;
pub mod log_stopwatch;
mod logger;
#[cfg(feature = "rand")]
mod rng;
#[doc(hidden)]
pub mod rt;
#[doc(hidden)]
pub mod table;
mod timestamp;

use spacetimedb_lib::bsatn;
use std::cell::RefCell;
use std::collections::VecDeque;

pub use log;
#[cfg(feature = "rand")]
pub use rand;

#[doc(hidden)]
pub use client_visibility_filter::Filter;
#[cfg(feature = "rand")]
pub use rng::StdbRng;
pub use sats::SpacetimeType;
#[doc(hidden)]
pub use spacetimedb_bindings_macro::__TableHelper;
pub use spacetimedb_bindings_sys as sys;
pub use spacetimedb_lib;
pub use spacetimedb_lib::de::{Deserialize, DeserializeOwned};
pub use spacetimedb_lib::sats;
pub use spacetimedb_lib::ser::Serialize;
pub use spacetimedb_lib::Address;
pub use spacetimedb_lib::AlgebraicValue;
pub use spacetimedb_lib::Identity;
pub use spacetimedb_lib::ScheduleAt;
pub use spacetimedb_primitives::TableId;
pub use sys::Errno;
pub use table::{AutoIncOverflow, BTreeIndex, Table, TryInsertError, UniqueColumn, UniqueConstraintViolation};
pub use timestamp::Timestamp;

pub type ReducerResult = core::result::Result<(), Box<str>>;

pub use spacetimedb_bindings_macro::duration;

/// Generates code for registering a row-level security rule.
///
/// This attribute must be applied to a `const` binding of type [`Filter`].
/// It will be interpreted as a filter on the table to which it applies, for all client queries.
/// If a module contains multiple `client_visibility_filter`s for the same table,
/// they will be unioned together as if by SQL `OR`,
/// so that any row permitted by at least one filter is visible.
///
/// The `const` binding's identifier must be unique within the module.
///
/// The query follows the same syntax as a subscription query.
///
/// ## Example:
///
/// ```rust,ignore
/// /// Players can only see what's in their chunk
/// #[spacetimedb::client_visibility_filter]
/// const PLAYERS_SEE_ENTITIES_IN_SAME_CHUNK: Filter = Filter::Sql("
///     SELECT * FROM LocationState WHERE chunk_index IN (
///         SELECT chunk_index FROM LocationState WHERE entity_id IN (
///             SELECT entity_id FROM UserState WHERE identity = @sender
///         )
///     )
/// ");
/// ```
///
/// Queries are not checked for syntactic or semantic validity
/// until they are processed by the SpacetimeDB host.
/// This means that errors in queries, such as syntax errors, type errors or unknown tables,
/// will be reported during `spacetime publish`, not at compile time.
#[doc(inline, hidden)] // TODO: RLS filters are currently unimplemented, and are not enforced.
pub use spacetimedb_bindings_macro::client_visibility_filter;

/*
### Generated table functions

<!-- TODO: rewrite this section. -->

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

```rust
# #[table(name = person, public)]
# struct Person {
#     #[unique]
#     id: u64,
#
#     #[index(btree)]
#     age: u32,
#     name: String,
#     address: String,
# }
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

#### Deleting

Like filtering, we can delete by an indexed or unique column instead of the entire row.

```rust
#[reducer]
fn delete_id(ctx: &ReducerContext, id: u64) {
    ctx.db.person().id().delete(id)
}
```

 */

/// Declares a table with a particular row type.
///
/// This attribute is applied to a struct type with named fields.
/// This derives [`Serialize`], [`Deserialize`], [`SpacetimeType`], and [`Debug`] for the annotated struct.
///
/// Elements of the struct type are NOT automatically inserted into any global table.
/// They are regular structs, with no special behavior.
/// In particular, modifying them does not automatically modify the database!
///
/// Instead, a struct implementing [`Table<Row = Self>`] is generated. This can be looked up in a [`ReducerContext`]
/// using `ctx.db.{table_name}()`. This method represents a handle to a database table, and can be used to
/// iterate and modify the table's elements. It is a view of the entire table -- the entire set of rows at the time of the reducer call.
///
/// # Example
///
/// ```ignore
/// use spacetimedb::{table, ReducerContext};
///
/// #[table(name = users, public,
///         index(name = id_and_username, btree(columns = [id, username])),
///         index(name = popularity_and_username, btree(columns = [popularity, username])),
/// )]
/// pub struct User {
///     #[auto_inc]
///     #[primary_key]
///     pub id: u32,
///     #[unique]
///     pub username: String,
///     #[index(btree)]
///     pub popularity: u32,
/// }
///
/// fn demo(ctx: &ReducerContext) {
///     // Use the name of the table to get a struct
///     // implementing `spacetimedb::Table<Row = User>`.
///     let users: users__TableHandle = ctx.db.users();
///
///     // You can use methods from `spacetimedb::Table`
///     // on the table.
///     log::debug!("User count: {}", users.count());
///     for user in users.iter() {
///         log::debug!("{:?}", user);
///     }
///
///     // For every named `index`, the table has an extra method
///     // for getting a corresponding `spacetimedb::BTreeIndex`.
///     let by_id_and_username: spacetimedb::BTreeIndex<_, (u32, String), _> =
///         users.id_and_username();
///     let mut billy: User = by_id_and_username.find((&57, &"Billy".to_string()));
///     billy.popularity += 5;
///     by_id_and_username.update(billy);
///
///     // For every `#[unique]` or `#[primary_key]` field,
///     // the table has an extra method that allows getting a
///     // corresponding `spacetimedb::UniqueColumn`.
///     let by_username: spacetimedb::UniqueColumn<_, String, _> = users.id();
///     by_username.delete(&"test_user".to_string());
/// }
/// ```
///
/// See [`Table`], [`BTreeIndex`], and [`UniqueColumn`] for more information on the methods available on these types.
///
/// # Browsing generated documentation
///
/// The `#[table]` macro generates different APIs depending on the contents of your table.
///
/// To browse the complete generated API for your tables, run `cargo doc` in your SpacetimeDB module project. Navigate to `[YOUR PROJECT/target/doc/spacetime_module/index.html` in your file explorer, and right click -> open it in a web browser.
///
/// For the example above, we would see three items:
/// - A struct `User`. This is the struct you declared. It stores rows of the table `users`.
/// - A struct `users__TableHandle`. This is an opaque handle that allows you to interact with the table `users`.
/// - A trait `users` containing a single `fn users(&self) -> users__TableHandle`.
///   This trait is implemented for the `db` field of a [`ReducerContext`], allowing you to get a
///   `users__TableHandle` using `ctx.db.users()`.
///
/// # Macro arguments
///
/// The `#[table(...)]` attribute accepts any number of the following arguments, separated by commas.
///
/// Multiple `table` annotations can be present on the same type. This will generate
/// multiple tables of the same row type, but with different names.
///
/// * `name = my_table`
///
///    Specify the name of the table in the database, if you want it to be different from
///    the name of the struct. The name can be any valid Rust identifier.
///
///    The table name is used to get a handle to the table from a [`ReducerContext`].
///    For a table *table*, use `ctx.db.{table}()` to do this.
///    For example:
///    ```ignore
///    let users: users__TableHandle = ctx.db.users();
///    ```
///
/// * `public` and `private`
///
///    Tables are private by default. This means that clients cannot read their contents
///    or see that they exist.
///
///    If you'd like to make your table publically accessible by clients,
///    put `public` in the macro arguments (e.g.
///    `#[spacetimedb::table(public)]`). You can also specify `private` if
///    you'd like to be specific.
///
///    This is fully separate from Rust's module visibility
///    system; `pub struct` or `pub(crate) struct` do not affect the table visibility, only
///    the visibility of the items in your own source code.
///
/// * `index(name = my_index, btree(columns = [a, b, c]))`
///
///    You can specify an index on one or more of the table's columns with the above syntax.
///    You can also just put `#[index(btree)]` on the field itself if you only need
///    a single-column attribute; see column attributes below.
///
///    Multiple indexes are permitted.
///
///    You can use indexes to efficiently [`filter`](crate::BTreeIndex::filter) and
///    [`delete`](crate::BTreeIndex::delete) rows. This is encapsulated in the struct [`BTreeIndex`].
///
///    For a table *table* and an index *index*, use:
///    ```text
///    ctx.db.{table}().{index}()
///    ```
///    to get a [`BTreeIndex`] for a [`ReducerContext`].
///
///    For example:
///    ```ignore
///
///    let by_id_and_username: spacetimedb::BTreeIndex<_, (u32, String), _> =
///        ctx.db.users().by_id_and_username();
///    ```
///
/// * `scheduled(reducer_name)`
///
///    Used to declare a [scheduled reducer](crate#scheduled-reducers).
///    
///    The annotated struct type must have at least the following fields:
///    - `scheduled_id: u64`
///    - [`scheduled_at: ScheduleAt`](crate::ScheduleAt)
///
/// # Column (field) attributes
///
/// * `#[auto_inc]`
///
///    Creates an auto-increment constraint.
///
///    When a row is inserted with the annotated field set to `0` (zero),
///    the sequence is incremented, and this value is used instead.
///
///    Can only be used on numeric types.
///
///    May be combined with indexes or unique constraints.
///
///    Note that using `#[auto_inc]` on a field does not also imply `#[primary_key]` or `#[unique]`.
///    If those semantics are desired, those attributes should also be used.
///
///    <!-- TODO: What happens if a reducer tries to insert a row that has an already-existing unique
///               auto-inc column? Like, if the user inserts a row ahead of the auto-inc, then
///               the auto-inc catches up? -->
///
/// * `#[unique]`
///
///    Creates an unique constraint and index for the annotated field.
///
///    You can [`find`](crate::UniqueColumn::find), [`update`](crate::UniqueColumn::update),
///    and [`delete`](crate::UniqueColumn::delete) rows by their unique columns.
///    This is encapsulated in the struct [`UniqueColumn`].
///
///    For a table *table* and a column *column*, use:
///    ```text
///    ctx.db.{table}().{column}()`
///    ```
///    to get a [`UniqueColumn`] from a [`ReducerContext`].
///
///    For example:
///    ```ignore
///    let by_username: spacetimedb::UniqueColumn<_, String, _> = ctx.db.users().username();
///    ```
///
/// * `#[primary_key]`
///
///    Implies `#[unique]`. Also generates additional methods client-side for handling updates to the table.
///    <!-- TODO: link to client-side documentation. -->
///
/// * `#[index(btree)]`
///
///    Creates a single-column index with the specified algorithm.
///
///    It is an error, and also redundant, to specify this attribute together with `#[unique]`.
///    Unique constraints implicitly create an index, so you don't need to specify both.
///
///    The created index has the same name as the column. <!-- TODO(1.0): this may change if we do the unify-index-names PR. -->
///    
/// # Generated code
///
/// For each `[table(name = {name})]` annotation on a type `{T}`, generates a struct
/// `{name}__TableHandle` implementing [`Table<Row={T}>`](crate::Table), and a trait that allows looking up such a
/// `{name}Handle` in a [`ReducerContext`].
///
/// The struct `{name}__TableHandle` is public and lives next t
///
/// For each named index declaration, add a method to `{name}__TableHandle` for getting a corresponding
/// [`BTreeIndex`].
///
/// For each field  with a `#[unique]` or `#[primary_key]` annotation,
/// add a method to `{name}Handle` for getting a corresponding [`UniqueColumn`].
///
/// The following pseudocode illustrates the general idea. Curly braces are used to indicate templated
/// names.
///
/// ```ignore
/// use spacetimedb::{BTreeIndex, UniqueColumn, Table, DbView};
///
/// // This generated struct is hidden and cannot be directly accessed.
/// struct {name}__TableHandle { /* ... */ };
///
/// // It is a table handle.
/// impl Table for {name}__TableHandle {
///     type Row = {T};
///     /* ... */
/// }
///
/// // It can be looked up in a `ReducerContext`,
/// // using `ctx.db().{name}()`.
/// trait {name} {
///     fn {name}(&self) -> Row = {T}>;
/// }
/// impl {name} for <ReducerContext as DbContext>::DbView { /* ... */ }
///
/// // Once looked up, it can be used to look up indexes.
/// impl {name}Handle {
///     // For each `#[unique]` or `#[primary_key]` field `{field}` of type `{F}`:
///     fn {field}(&self) -> UniqueColumn<_, {F}, _> { /* ... */ };
///     
///     // For each named index `{index}` on fields of type `{(F1, ..., FN)}`:
///     fn {index}(&self) -> BTreeIndex<_, {(F1, ..., FN)}, _>;
/// }
/// ```
///
/// [`Table<Row = Self>`]: `Table`
#[doc(inline)]
pub use spacetimedb_bindings_macro::table;

/// Marks a function as a spacetimedb reducer.
///
/// A reducer is a function with read/write access to the database
/// that can be invoked remotely by [clients].
///
/// Each reducer call runs in its own database transaction,
/// and its updates to the database are only committed if the reducer returns successfully.
///
/// The first argument of a reducer is always a [`&ReducerContext`]. This context object
/// allows accessing the database and viewing information about the caller, among other things.
///
/// After this, a reducer can take any number of arguments.
/// These arguments must implement the [`SpacetimeType`], [`Serialize`], and [`Deserialize`] traits.
/// All of these traits can be derived at once by marking a type with `#[derive(SpacetimeType)]`.
///
/// Reducers may return either `()` or `Result<(), E>` where `E: Debug`.
///
/// ```rust,ignore
/// use spacetimedb::reducer;
/// use log::info;
///
/// #[reducer]
/// pub fn hello_world(context: &ReducerContext) {
///     info!("Hello, World!");
/// }
///
/// #[reducer]
/// pub fn add_person(context: &ReducerContext, name: String, age: u16) {
///     // add a "person" to the database.
/// }
///
/// #[derive(SpacetimeType)]
/// struct Coordinates {
///     x: f32,
///     y: f32,
/// }
///
/// #[derive(Debug)]
/// enum AddPlaceError {
///     InvalidCoordinates(Coordinates),
///     InvalidName(String),
/// }
///
/// #[reducer]
/// pub fn add_place(
///     context: &ReducerContext,
///     name: String,
///     x: f32,
///     y: f32,
///     area: f32,
/// ) -> Result<(), AddPlaceError> {
///     // add a "place" to the database.
/// }
/// ```
///
/// Reducers may fail by returning a [`Result::Err`](std::result::Result) or by [panicking](std::panic!).
/// Such a failure will be printed to the module logs and abort the active database transaction.
/// Any changes to the database will be rolled back.
///
/// Reducers are limited in their ability to interact with the outside world.
/// They do not directly return data aside from errors, and have no access to any
/// network or filesystem interfaces.
/// Calling methods from [`std::io`], [`std::net`], or [`std::fs`]
/// inside a reducer will result in runtime errors.
///
/// Reducers can communicate information to the outside world in two ways:
/// - They can modify tables in the database.
///   See the `#[table]`(#table) macro documentation for information on how to declare and use tables.
/// - They can call logging macros from the [`log`] crate.
///   This writes to a private debug log attached to the database.
///   Run `spacetime logs <DATABASE_IDENTITY>` to browse these.
///
/// Reducers are permitted to call other reducers, simply by passing their `ReducerContext` as the first argument.
/// This is a regular function call, and does not involve any network communication. The callee will run within the
/// caller's transaction, and any changes made by the callee will be committed or rolled back with the caller.
///
/// # Lifecycle Reducers
///
/// A small group of reducers are called at set points in the module lifecycle.
///
/// See the [Lifecycle Reducers](crate#lifecycle-reducers) documentation at the crate root.
///
/// # Scheduled Reducers
///
/// Reducers can be scheduled to run repeatedly.
///
/// See the [Scheduled Reducers](crate#scheduled-reducers) documentation at the crate root.
///
/// [`&ReducerContext`]: `ReducerContext`
/// [clients]: https://spacetimedb.com/docs/#client
#[doc(inline)]
pub use spacetimedb_bindings_macro::reducer;

/*
#[doc(inline)]
/// Trait that allows looking up methods on a table.
///
/// This trait associates a [table handle](crate::Table) type to a table row type. Code like:
///
/// ```rust
/// #[spacetimedb::table(name = people)]
/// struct Person {
///    #[unique]
///    #[auto_inc]
///    id: u64,
///    name: String,
/// }
/// ```
///
/// will generate accessors that allow looking up the `people` table in a `ReducerContext`.
///
pub use table::__MapRowTypeToTable;
*/

/// The context that any reducer is provided with.
///
/// This must be the first argument of the reducer. Clients of the module will
/// only see arguments after the `ReducerContext`.
///
/// Includes information about the client calling the reducer and the time of invocation,
/// as well as a view into the module's database.
///
/// If the crate was compiled with the `rand` feature, also includes faculties for random
/// number generation.
///
/// Implements the `DbContext` trait for accessing views into a database.
/// Currently, being this generic is only meaningful in clients,
/// as `ReducerContext` is the only implementor of `DbContext` within modules.
#[non_exhaustive]
pub struct ReducerContext {
    /// The `Identity` of the client that invoked the reducer.
    pub sender: Identity,

    /// The time at which the reducer was started.
    pub timestamp: Timestamp,

    /// The `Address` of the client that invoked the reducer.
    ///
    /// `None` if no `Address` was supplied to the `/database/call` HTTP endpoint,
    /// or via the CLI's `spacetime call` subcommand.
    ///
    /// For automatic reducers, i.e. `init`, `update` and scheduled reducers,
    /// this will be the module's `Address`.
    pub address: Option<Address>,

    /// Allows accessing the local database attached to a module.
    ///
    /// This slightly strange type appears to have no methods, but that is misleading.
    /// The `#[table]` macro uses the trait system to add table accessors to this type.
    /// These are generated methods that allow you to access specific tables.
    ///
    /// For a table named *table*, use `ctx.db.{table}()` to get a handle.
    /// For example:
    /// ```no_run
    /// use spacetimedb::{table, reducer, ReducerContext};
    ///
    /// #[table(name = books)]
    /// struct Book {
    ///     #[primary_key]
    ///     id: u64,
    ///     isbn: String,
    ///     name: String,
    ///     #[index(btree)]
    ///     author: String
    /// }
    ///
    /// #[reducer]
    /// fn find_books_by(ctx: &ReducerContext, author: String) {
    ///     let books: books__TableHandle = ctx.books();
    ///
    ///     log::debug("looking up books by {author}...");
    ///     for book in books.author().filter(author) {
    ///         log::debug("- {book:?}");
    ///     }
    /// }
    /// ```
    /// See the [`#[table]`](macro@crate::table) macro for more information.
    pub db: Local,

    #[cfg(feature = "rand")]
    rng: std::cell::OnceCell<StdbRng>,
}

impl ReducerContext {
    #[doc(hidden)]
    pub fn __dummy() -> Self {
        Self {
            db: Local {},
            sender: Identity::__dummy(),
            timestamp: Timestamp::UNIX_EPOCH,
            address: None,
            rng: std::cell::OnceCell::new(),
        }
    }

    /// Read the current module's [`Identity`].
    pub fn identity(&self) -> Identity {
        // Hypothetically, we *could* read the module identity out of the system tables.
        // However, this would be:
        // - Onerous, because we have no tooling to inspect the system tables from module code.
        // - Slow (at least relatively),
        //   because it would involve multiple host calls which hit the datastore,
        //   as compared to a single host call which does not.
        // As such, we've just defined a host call
        // which reads the module identity out of the `InstanceEnv`.
        Identity::from_byte_array(spacetimedb_bindings_sys::identity())
    }
}

/// A handle on a database with a particular table schema.
pub trait DbContext {
    /// A view into the tables of a database.
    ///
    /// This type is specialized on the database's particular schema.
    ///
    /// Methods on the `DbView` type will allow querying tables defined by the module.
    type DbView;

    /// Get a view into the tables.
    ///
    /// This method is provided for times when a programmer wants to be generic over the `DbContext` type.
    /// Concrete-typed code is expected to read the `.db` field off the particular `DbContext` implementor.
    /// Currently, being this generic is only meaningful in clients,
    /// as `ReducerContext` is the only implementor of `DbContext` within modules.
    fn db(&self) -> &Self::DbView;
}

impl DbContext for ReducerContext {
    type DbView = Local;

    fn db(&self) -> &Self::DbView {
        &self.db
    }
}

/// Allows accessing the local database attached to the module.
///
/// This slightly strange type appears to have no methods, but that is misleading.
/// The `#[table]` macro uses the trait system to add table accessors to this type.
/// These are generated methods that allow you to access specific tables.
///
/// <!-- TODO(THIS PR): THIS IS A LIE. In my testing I thought it worked, but it seems less reliable than I thought.
///
/// Run `cargo doc` in your Rust module project and navigate to this type
/// to see the methods have been automatically added. It will be at the path:
/// `[your_project_directory]/target/doc/spacetimedb/struct.Local.html`.
/// (or, `[your_project_directory]\target\doc\spacetimedb\struct.Local.html` on Windows.)
/// -->
#[non_exhaustive]
pub struct Local {}

// #[cfg(target_arch = "wasm32")]
// #[global_allocator]
// static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// This should guarantee in most cases that we don't have to reallocate an iterator
// buffer, unless there's a single row that serializes to >1 MiB.
const DEFAULT_BUFFER_CAPACITY: usize = spacetimedb_primitives::ROW_ITER_CHUNK_SIZE * 2;

/// Queries and returns the `table_id` associated with the given (table) `name`.
///
/// Panics if the table does not exist.
#[doc(hidden)]
pub fn table_id_from_name(table_name: &str) -> TableId {
    sys::table_id_from_name(table_name).unwrap_or_else(|_| {
        panic!("Failed to get table with name: {}", table_name);
    })
}

thread_local! {
    /// A global pool of buffers used for iteration.
    // This gets optimized away to a normal global since wasm32 doesn't have threads by default.
    static ITER_BUFS: RefCell<VecDeque<Vec<u8>>> = const { RefCell::new(VecDeque::new()) };
}

struct IterBuf {
    buf: Vec<u8>,
}

impl IterBuf {
    /// Take a buffer from the pool of buffers for row iterators, if one exists. Otherwise, allocate a new one.
    fn take() -> Self {
        let buf = ITER_BUFS
            .with_borrow_mut(|v| v.pop_front())
            .unwrap_or_else(|| Vec::with_capacity(DEFAULT_BUFFER_CAPACITY));
        Self { buf }
    }

    fn serialize<T: Serialize + ?Sized>(val: &T) -> Result<Self, bsatn::EncodeError> {
        let mut buf = IterBuf::take();
        buf.serialize_into(val)?;
        Ok(buf)
    }

    #[inline]
    fn serialize_into<T: Serialize + ?Sized>(&mut self, val: &T) -> Result<(), bsatn::EncodeError> {
        bsatn::to_writer(&mut **self, val)
    }
}

impl Drop for IterBuf {
    fn drop(&mut self) {
        self.buf.clear();
        let buf = std::mem::take(&mut self.buf);
        ITER_BUFS.with_borrow_mut(|v| v.push_back(buf));
    }
}

impl AsRef<[u8]> for IterBuf {
    fn as_ref(&self) -> &[u8] {
        &self.buf
    }
}

impl std::ops::Deref for IterBuf {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.buf
    }
}
impl std::ops::DerefMut for IterBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buf
    }
}

#[cfg(feature = "unstable")]
#[macro_export]
macro_rules! volatile_nonatomic_schedule_immediate {
    ($($args:tt)*) => {
        $crate::__volatile_nonatomic_schedule_immediate_impl!([] [$($args)*])
    };
}

#[cfg(feature = "unstable")]
#[doc(hidden)]
#[macro_export]
macro_rules! __volatile_nonatomic_schedule_immediate_impl {
    ([$repeater:path] [($($args:tt)*)]) => {
        $crate::__volatile_nonatomic_schedule_immediate_impl!(@process_args $repeater, ($($args)*))
    };
    ([$($cur:tt)*] [$next:tt $($rest:tt)*]) => {
        $crate::__volatile_nonatomic_schedule_immediate_impl!([$($cur)* $next] [$($rest)*])
    };
    (@process_args $repeater:path, ($($args:expr),* $(,)?)) => {
        $crate::__volatile_nonatomic_schedule_immediate_impl!(@call $repeater, ($($args),*))
    };
    (@call $repeater:path, ($($args:expr),*)) => {
        if false {
            let _ = $repeater(&$crate::ReducerContext::__dummy(), $($args,)*);
        } else {
            $crate::rt::volatile_nonatomic_schedule_immediate::<_, _, $repeater>($repeater, ($($args,)*))
        }
    };
}
