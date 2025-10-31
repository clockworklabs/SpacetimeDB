#![doc = include_str!("../README.md")]
// ^ if you are working on docs, go read the top comment of README.md please.

use core::cell::{LazyCell, OnceCell, RefCell};
use core::ops::Deref;
use spacetimedb_lib::bsatn;

#[cfg(feature = "unstable")]
mod client_visibility_filter;
pub mod log_stopwatch;
mod logger;
#[cfg(feature = "rand08")]
mod rng;
#[doc(hidden)]
pub mod rt;
#[doc(hidden)]
pub mod table;

#[cfg(feature = "unstable")]
pub use client_visibility_filter::Filter;
pub use log;
#[cfg(feature = "rand")]
pub use rand08 as rand;
#[cfg(feature = "rand08")]
pub use rng::StdbRng;
pub use sats::SpacetimeType;
#[doc(hidden)]
pub use spacetimedb_bindings_macro::__TableHelper;
pub use spacetimedb_bindings_sys as sys;
pub use spacetimedb_lib;
pub use spacetimedb_lib::de::{Deserialize, DeserializeOwned};
pub use spacetimedb_lib::sats;
pub use spacetimedb_lib::ser::Serialize;
pub use spacetimedb_lib::AlgebraicValue;
pub use spacetimedb_lib::ConnectionId;
// `FilterableValue` re-exported purely for rustdoc.
pub use spacetimedb_lib::FilterableValue;
pub use spacetimedb_lib::Identity;
pub use spacetimedb_lib::ScheduleAt;
pub use spacetimedb_lib::TimeDuration;
pub use spacetimedb_lib::Timestamp;
pub use spacetimedb_primitives::TableId;
pub use sys::Errno;
pub use table::{
    AutoIncOverflow, RangedIndex, RangedIndexReadOnly, Table, TryInsertError, UniqueColumn, UniqueColumnReadOnly,
    UniqueConstraintViolation,
};

pub type ReducerResult = core::result::Result<(), Box<str>>;

pub type ProcedureResult = Vec<u8>;

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
/// ```no_run
/// # #[cfg(target_arch = "wasm32")] mod demo {
/// use spacetimedb::{client_visibility_filter, Filter};
///
/// /// Players can only see what's in their chunk
/// #[client_visibility_filter]
/// const PLAYERS_SEE_ENTITIES_IN_SAME_CHUNK: Filter = Filter::Sql("
///     SELECT * FROM LocationState WHERE chunk_index IN (
///         SELECT chunk_index FROM LocationState WHERE entity_id IN (
///             SELECT entity_id FROM UserState WHERE identity = :sender
///         )
///     )
/// ");
/// # }
/// ```
///
/// Queries are not checked for syntactic or semantic validity
/// until they are processed by the SpacetimeDB host.
/// This means that errors in queries, such as syntax errors, type errors or unknown tables,
/// will be reported during `spacetime publish`, not at compile time.
#[cfg(feature = "unstable")]
#[doc(inline, hidden)] // TODO: RLS filters are currently unimplemented, and are not enforced.
pub use spacetimedb_bindings_macro::client_visibility_filter;

/// Declares a table with a particular row type.
///
/// This attribute is applied to a struct type with named fields.
/// This derives [`Serialize`], [`Deserialize`], [`SpacetimeType`], and [`Debug`] for the annotated struct.
///
/// Elements of the struct type are NOT automatically inserted into any global table.
/// They are regular structs, with no special behavior.
/// In particular, modifying them does not automatically modify the database!
///
/// Instead, a type implementing [`Table<Row = Self>`] is generated. This can be looked up in a [`ReducerContext`]
/// using `ctx.db.{table_name}()`. This type represents a handle to a database table, and can be used to
/// iterate and modify the table's elements. It is a view of the entire table -- the entire set of rows at the time of the reducer call.
///
/// # Example
///
/// ```ignore
/// use spacetimedb::{table, ReducerContext};
///
/// #[table(name = user, public,
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
///     let user: user__TableHandle = ctx.db.user();
///
///     // You can use methods from `spacetimedb::Table`
///     // on the table.
///     log::debug!("User count: {}", user.count());
///     for user in user.iter() {
///         log::debug!("{:?}", user);
///     }
///
///     // For every `#[index(btree)]`, the table has an extra method
///     // for getting a corresponding `spacetimedb::BTreeIndex`.
///     let by_popularity: RangedIndex<_, (u32,), _> =
///         user.popularity();
///     for popular_user in by_popularity.filter(95..) {
///         log::debug!("Popular user: {:?}", popular_user);
///     }
///
///     // There are similar methods for multi-column indexes.
///     let by_popularity_and_username: RangedIndex<_, (u32, String), _> = user.popularity_and_username();
///     for popular_user in by_popularity.filter((100, "a"..)) {
///         log::debug!("Popular user whose name starts with 'a': {:?}", popular_user);
///     }
///
///     // For every `#[unique]` or `#[primary_key]` field,
///     // the table has an extra method that allows getting a
///     // corresponding `spacetimedb::UniqueColumn`.
///     let by_username: spacetimedb::UniqueColumn<_, String, _> = user.id();
///     by_username.delete(&"test_user".to_string());
/// }
/// ```
///
/// See [`Table`], [`RangedIndex`], and [`UniqueColumn`] for more information on the methods available on these types.
///
/// # Browsing generated documentation
///
/// The `#[table]` macro generates different APIs depending on the contents of your table.
///
/// To browse the complete generated API for your tables, run `cargo doc` in your SpacetimeDB module project. Navigate to `[YOUR PROJECT/target/doc/spacetime_module/index.html` in your file explorer, and right click -> open it in a web browser.
///
/// For the example above, we would see three items:
/// - A struct `User`. This is the struct you declared. It stores rows of the table `user`.
/// - A struct `user__TableHandle`. This is an opaque handle that allows you to interact with the table `user`.
/// - A trait `user` containing a single `fn user(&self) -> user__TableHandle`.
///   This trait is implemented for the `db` field of a [`ReducerContext`], allowing you to get a
///   `user__TableHandle` using `ctx.db.user()`.
///
/// # Macro arguments
///
/// The `#[table(...)]` attribute accepts any number of the following arguments, separated by commas.
///
/// Multiple `table` annotations can be present on the same type. This will generate
/// multiple tables of the same row type, but with different names.
///
/// ### `name`
///
/// Specify the name of the table in the database. The name can be any valid Rust identifier.
///
/// The table name is used to get a handle to the table from a [`ReducerContext`].
/// For a table *table*, use `ctx.db.{table}()` to do this.
/// For example:
/// ```ignore
///  #[table(name = user)]
///  pub struct User {
///      #[auto_inc]
///      #[primary_key]
///      pub id: u32,
///      #[unique]
///      pub username: String,
///      #[index(btree)]
///      pub popularity: u32,
///  }
///  #[reducer]
///  fn demo(ctx: &ReducerContext) {
///      let user: user__TableHandle = ctx.db.user();
///  }
///  ```
///
/// ### `public` and `private`
///
/// Tables are private by default. This means that clients cannot read their contents
/// or see that they exist.
///
/// If you'd like to make your table publicly accessible by clients,
/// put `public` in the macro arguments (e.g.
/// `#[spacetimedb::table(public)]`). You can also specify `private` if
/// you'd like to be specific.
///
/// This is fully separate from Rust's module visibility
/// system; `pub struct` or `pub(crate) struct` do not affect the table visibility, only
/// the visibility of the items in your own source code.
///
/// ### `index(...)`
///
/// You can specify an index on one or more of the table's columns with the syntax:
/// `index(name = my_index, btree(columns = [a, b, c]))`
///
/// You can also just put `#[index(btree)]` on the field itself if you only need
/// a single-column index; see column attributes below.
///
/// A table may declare any number of indexes.
///
/// You can use indexes to efficiently [`filter`](crate::RangedIndex::filter) and
/// [`delete`](crate::RangedIndex::delete) rows. This is encapsulated in the struct [`RangedIndex`].
///
/// For a table *table* and an index *index*, use:
/// ```text
/// ctx.db.{table}().{index}()
/// ```
/// to get a [`RangedIndex`] for a [`ReducerContext`].
///
/// For example:
/// ```ignore
/// let by_id_and_username: spacetimedb::RangedIndex<_, (u32, String), _> =
///     ctx.db.user().by_id_and_username();
/// ```
///
/// ### `scheduled(reducer_name)`
///
/// Used to declare a [scheduled reducer](macro@crate::reducer#scheduled-reducers).
///
/// The annotated struct type must have at least the following fields:
/// - `scheduled_id: u64`
/// - [`scheduled_at: ScheduleAt`](crate::ScheduleAt)
///
/// # Column (field) attributes
///
/// ### `#[auto_inc]`
///
/// Creates an auto-increment constraint.
///
/// When a row is inserted with the annotated field set to `0` (zero),
/// the sequence is incremented, and this value is used instead.
///
/// Can only be used on numeric types.
///
/// May be combined with indexes or unique constraints.
///
/// Note that using `#[auto_inc]` on a field does not also imply `#[primary_key]` or `#[unique]`.
/// If those semantics are desired, those attributes should also be used.
///
/// When `#[auto_inc]` is combined with a unique key,
/// be wary not to manually insert values larger than the allocated sequence value.
/// In this case, the sequence will eventually catch up, allocate a value that's already present,
/// and cause a unique constraint violation.
///
/// ### `#[unique]`
///
/// Creates an unique constraint and index for the annotated field.
///
/// You can [`find`](crate::UniqueColumn::find), [`update`](crate::UniqueColumn::update),
/// and [`delete`](crate::UniqueColumn::delete) rows by their unique columns.
/// This is encapsulated in the struct [`UniqueColumn`].
///
/// For a table *table* and a column *column*, use:
/// ```text
/// ctx.db.{table}().{column}()`
/// ```
/// to get a [`UniqueColumn`] from a [`ReducerContext`].
///
/// For example:
/// ```ignore
/// let by_username: spacetimedb::UniqueColumn<_, String, _> = ctx.db.user().username();
/// ```
///
/// When there is a unique column constraint on the table, insertion can fail if a uniqueness constraint is violated.
/// If we insert two rows which have the same value of a unique column, the second will fail.
/// This will be via a panic with [`Table::insert`] or via a `Result::Err` with [`Table::try_insert`].
///
/// For example:
/// ```no_run
/// # #[cfg(target_arch = "wasm32")] mod demo {
/// use spacetimedb::{
///     table,
///     reducer,
///     ReducerContext,
///     // Make sure to import the `Table` trait to use `insert` or `try_insert`.
///     Table
/// };
///
/// type CountryCode = String;
///
/// #[table(name = country)]
/// struct Country {
///     #[unique]
///     code: CountryCode,
///     national_bird: String
/// }
///
/// #[reducer]
/// fn insert_unique_demo(ctx: &ReducerContext) {
///     let result = ctx.db.country().try_insert(Country {
///         code: "AU".into(), national_bird: "Emu".into()
///     });
///     assert!(result.is_ok());
///
///     let result = ctx.db.country().try_insert(Country {
///         code: "AU".into(), national_bird: "Great Egret".into()
///         // Whoops, this was Austria's national bird, not Australia's.
///         // We should have used the country code "AT", not "AU".
///     });
///     // since there's already a country in the database with the code "AU",
///     // SpacetimeDB gives us an error.
///     assert!(result.is_err());
///
///     // The following line would panic, since we use `insert` rather than `try_insert`.
///     // let result = ctx.db.country().insert(Country { code: "CN".into(), national_bird: "Blue Magpie".into() });
///
///     // If we wanted to *update* the row for Australia, we can use the `update` method of `UniqueIndex`.
///     // The following line will succeed:
///     ctx.db.country().code().update(Country {
///         code: "AU".into(), national_bird: "Australian Emu".into()
///     });
/// }
/// # }
/// ```
///
/// ### `#[primary_key]`
///
/// Implies `#[unique]`. Also generates additional methods client-side for handling updates to the table.
/// <!-- TODO: link to client-side documentation. -->
///
/// ### `#[index(btree)]`
///
/// Creates a single-column index with the specified algorithm.
///
/// It is an error to specify this attribute together with `#[unique]`.
/// Unique constraints implicitly create a unique index, which is accessed using the [`UniqueColumn`] struct instead of the
/// [`RangedIndex`] struct.
///
/// The created index has the same name as the column.
///
/// For a table *table* and an indexed *column*, use:
/// ```text
/// ctx.db.{table}().{column}()
/// ```
/// to get a [`RangedIndex`] from a [`ReducerContext`].
///
/// For example:
///
/// ```ignore
/// ctx.db.cities().latitude()
/// ```
///
/// # Generated code
///
/// For each `[table(name = {name})]` annotation on a type `{T}`, generates a struct
/// `{name}__TableHandle` implementing [`Table<Row={T}>`](crate::Table), and a trait that allows looking up such a
/// `{name}Handle` in a [`ReducerContext`].
///
/// The struct `{name}__TableHandle` is public and lives next to the row struct.
/// Users are encouraged not to write the name of this table handle struct,
/// or to store table handles in variables; operate through a `ReducerContext` instead.
///
/// For each named index declaration, add a method to `{name}__TableHandle` for getting a corresponding
/// [`RangedIndex`].
///
/// For each field  with a `#[unique]` or `#[primary_key]` annotation,
/// add a method to `{name}Handle` for getting a corresponding [`UniqueColumn`].
///
/// The following pseudocode illustrates the general idea. Curly braces are used to indicate templated
/// names.
///
/// ```ignore
/// use spacetimedb::{RangedIndex, UniqueColumn, Table, DbView};
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
///     fn {index}(&self) -> RangedIndex<_, {(F1, ..., FN)}, _>;
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
/// Reducers may return either `()` or `Result<(), E>` where [`E: std::fmt::Display`](std::fmt::Display).
///
/// ```no_run
/// # #[cfg(target_arch = "wasm32")] mod demo {
/// use spacetimedb::{reducer, SpacetimeType, ReducerContext};
/// use log::info;
/// use std::fmt;
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
/// #[derive(SpacetimeType, Debug)]
/// struct Coordinates {
///     x: f32,
///     y: f32,
/// }
///
/// enum AddPlaceError {
///     InvalidCoordinates(Coordinates),
///     InvalidName(String),
/// }
///
/// impl fmt::Display for AddPlaceError {
///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
///         match self {
///             AddPlaceError::InvalidCoordinates(coords) => {
///                 write!(f, "invalid coordinates: {coords:?}")
///             },
///             AddPlaceError::InvalidName(name) => {
///                 write!(f, "invalid name: {name:?}")
///             },
///         }
///     }
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
///     // ... add a place to the database...
///     # Ok(())
/// }
/// # }
/// ```
///
/// Reducers may fail by returning a [`Result::Err`](std::result::Result) or by [panicking](std::panic!).
/// Failures will abort the active database transaction.
/// Any changes to the database made by the failed reducer call will be rolled back.
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
/// You can specify special lifecycle reducers that are run at set points in
/// the module's lifecycle. You can have one of each per module.
///
/// These reducers cannot be called manually
/// and may not have any parameters except for `ReducerContext`.
///
/// ### The `init` reducer
///
/// This reducer is marked with `#[spacetimedb::reducer(init)]`. It is run the first time a module is published
/// and any time the database is cleared. (It does not have to be named `init`.)
///
/// If an error occurs when initializing, the module will not be published.
///
/// This reducer can be used to configure any static data tables used by your module. It can also be used to start running [scheduled reducers](#scheduled-reducers).
///
/// ### The `client_connected` reducer
///
/// This reducer is marked with `#[spacetimedb::reducer(client_connected)]`. It is run when a client connects to the SpacetimeDB module.
/// Their identity can be found in the sender value of the `ReducerContext`.
///
/// If an error occurs in the reducer, the client will be disconnected.
///
/// ### The `client_disconnected` reducer
///
/// This reducer is marked with `#[spacetimedb::reducer(client_disconnected)]`. It is run when a client disconnects from the SpacetimeDB module.
/// Their identity can be found in the sender value of the `ReducerContext`.
///
/// If an error occurs in the disconnect reducer,
/// the client is still recorded as disconnected.
///
// TODO(docs): Move these docs to be on `table`, rather than `reducer`. This will reduce duplication with procedure docs.
/// # Scheduled reducers
///
/// In addition to life cycle annotations, reducers can be made **scheduled**.
/// This allows calling the reducers at a particular time, or in a loop.
/// This can be used for game loops.
///
/// The scheduling information for a reducer is stored in a table.
/// This table has two mandatory fields:
/// - A primary key that identifies scheduled reducer calls.
/// - A [`ScheduleAt`] field that says when to call the reducer.
///
/// Managing timers with a scheduled table is as simple as inserting or deleting rows from the table.
/// This makes scheduling transactional in SpacetimeDB. If a reducer A first schedules B but then errors for some other reason, B will not be scheduled to run.
///
/// A [`ScheduleAt`] can be created from a [`spacetimedb::Timestamp`](crate::Timestamp), in which case the reducer will be scheduled once,
/// or from a [`std::time::Duration`], in which case the reducer will be scheduled in a loop. In either case the conversion can be performed using [`Into::into`].
///
/// ```no_run
/// # #[cfg(target_arch = "wasm32")] mod demo {
/// use spacetimedb::{table, reducer, ReducerContext, Timestamp, TimeDuration, ScheduleAt, Table};
/// use log::debug;
///
/// // First, we declare the table with scheduling information.
///
/// #[table(name = send_message_schedule, scheduled(send_message))]
/// struct SendMessageSchedule {
///     // Mandatory fields:
///     // ============================
///
///     /// An identifier for the scheduled reducer call.
///     #[primary_key]
///     #[auto_inc]
///     scheduled_id: u64,
///
///     /// Information about when the reducer should be called.
///     scheduled_at: ScheduleAt,
///
///     // In addition to the mandatory fields, any number of fields can be added.
///     // These can be used to provide extra information to the scheduled reducer.
///
///     // Custom fields:
///     // ============================
///
///     /// The text of the scheduled message to send.
///     text: String,
/// }
///
/// // Then, we declare the scheduled reducer.
/// // The first argument of the reducer should be, as always, a `&ReducerContext`.
/// // The second argument should be a row of the scheduling information table.
///
/// #[reducer]
/// fn send_message(ctx: &ReducerContext, arg: SendMessageSchedule) -> Result<(), String> {
///     let message_to_send = arg.text;
///
///     // ... send the message ...
///
///     Ok(())
/// }
///
/// // Now, we want to actually start scheduling reducers.
/// // It's convenient to do this inside the `init` reducer.
/// #[reducer(init)]
/// fn init(ctx: &ReducerContext) {
///
///     let current_time = ctx.timestamp;
///
///     let ten_seconds = TimeDuration::from_micros(10_000_000);
///
///     let future_timestamp: Timestamp = ctx.timestamp + ten_seconds;
///     ctx.db.send_message_schedule().insert(SendMessageSchedule {
///         scheduled_id: 1,
///         text:"I'm a bot sending a message one time".to_string(),
///
///         // Creating a `ScheduleAt` from a `Timestamp` results in the reducer
///         // being called once, at exactly the time `future_timestamp`.
///         scheduled_at: future_timestamp.into()
///     });
///
///     let loop_duration: TimeDuration = ten_seconds;
///     ctx.db.send_message_schedule().insert(SendMessageSchedule {
///         scheduled_id: 0,
///         text:"I'm a bot sending a message every 10 seconds".to_string(),
///
///         // Creating a `ScheduleAt` from a `Duration` results in the reducer
///         // being called in a loop, once every `loop_duration`.
///         scheduled_at: loop_duration.into()
///     });
/// }
/// # }
/// ```
///
/// Scheduled reducers are called on a best-effort basis and may be slightly delayed in their execution
/// when a database is under heavy load.
///
/// ### Restricting scheduled reducers
///
/// Scheduled reducers are normal reducers, and may still be called by clients.
/// If a scheduled reducer should only be called by the scheduler,
/// consider beginning it with a check that the caller `Identity` is the module:
///
/// ```no_run
/// # #[cfg(target_arch = "wasm32")] mod demo {
/// use spacetimedb::{reducer, ReducerContext};
///
/// # #[derive(spacetimedb::SpacetimeType)] struct ScheduledArgs {}
///
/// #[reducer]
/// fn scheduled(ctx: &ReducerContext, args: ScheduledArgs) -> Result<(), String> {
///     if ctx.sender != ctx.identity() {
///         return Err("Reducer `scheduled` may not be invoked by clients, only via scheduling.".into());
///     }
///     // Reducer body...
///     # Ok(())
/// }
/// # }
/// ```
///
/// <!-- TODO: SLAs? -->
///
/// [`&ReducerContext`]: `ReducerContext`
/// [clients]: https://spacetimedb.com/docs/#client
#[doc(inline)]
pub use spacetimedb_bindings_macro::reducer;

/// Marks a function as a SpacetimeDB procedure.
///
/// A procedure is a function that runs within the database and can be invoked remotely by [clients],
/// but unlike a [`reducer`], a  procedure is not automatically transactional.
/// This allows procedures to perform certain side-effecting operations,
/// but also means that module developers must be more careful not to corrupt the database state
/// when execution aborts or operations fail.
///
/// When in doubt, prefer writing [`reducer`]s unless you need to perform an operation only available to procedures.
///
/// The first argument of a procedure is always `&mut ProcedureContext`.
/// The [`ProcedureContext`] exposes information about the caller and allows side-effecting operations.
///
/// After this, a procedure can take any number of arguments.
/// These arguments must implement the [`SpacetimeType`], [`Serialize`], and [`Deserialize`] traits.
/// All of these traits can be derived at once by marking a type with `#[derive(SpacetimeType)]`.
///
/// A procedure may return any type that implements [`SpacetimeType`], [`Serialize`] and [`Deserialize`].
/// Unlike [reducer]s, SpacetimeDB does not assign any special semantics to [`Result`] return values.
///
/// If a procedure returns successfully (as opposed to panicking), its return value will be sent to the calling client.
/// If a procedure panics, its panic message will be sent to the calling client instead.
/// Procedure arguments and return values are not otherwise broadcast to clients.
///
/// ```no_run
/// # use spacetimedb::{procedure, SpacetimeType, ProcedureContext, Timestamp};
/// #[procedure]
/// fn return_value(ctx: &mut ProcedureContext, arg: MyArgument) -> MyReturnValue {
///     MyReturnValue {
///         a: format!("Hello, {}", ctx.sender),
///         b: ctx.timestamp,
///     }
/// }
///
/// #[derive(SpacetimeType)]
/// struct MyArgument {
///     val: u32,
/// }
///
/// #[derive(SpacetimeType)]
/// struct MyReturnValue {
///     a: String,
///     b: Timestamp,
/// }
/// ```
///
/// # Blocking operations
///
/// Procedures are allowed to perform certain operations which take time.
/// During the execution of these operations, the procedure's execution will be suspended,
/// allowing other database operations to run in parallel.
///
/// Procedures must not hold open a transaction while performing a blocking operation.
// TODO(procedure-http): add example with an HTTP request.
// TODO(procedure-transaction): document obtaining and using a transaction within a procedure.
///
/// # Scheduled procedures
// TODO(docs): after moving scheduled reducer docs into table secion, link there.
///
/// Like [reducer]s, procedures can be made **scheduled**.
/// This allows calling procedures at a particular time, or in a loop.
/// It also allows reducers to enqueue procedure runs.
///
/// Scheduled procedures are called on a best-effort basis and may be slightly delayed in their execution
/// when a database is under heavy load.
///
/// [clients]: https://spacetimedb.com/docs/#client
// TODO(procedure-async): update docs and examples with `async`-ness.
#[doc(inline)]
pub use spacetimedb_bindings_macro::procedure;

/// Marks a function as a spacetimedb view.
///
/// A view is a function with read-only access to the database.
///
/// The first argument of a view is always a [`&ViewContext`] or [`&AnonymousViewContext`].
/// The former can only read from the database whereas latter can also access info about the caller.
///
/// After this, a view can take any number of arguments just like reducers.
/// These arguments must implement the [`SpacetimeType`], [`Serialize`], and [`Deserialize`] traits.
/// All of these traits can be derived at once by marking a type with `#[derive(SpacetimeType)]`.
///
/// Views return `Vec<T>` or `Option<T>` where `T` is a `SpacetimeType`.
///
/// ```no_run
/// # mod demo {
/// use spacetimedb::{view, table, AnonymousViewContext, SpacetimeType, ViewContext};
/// use spacetimedb_lib::Identity;
///
/// #[table(name = player)]
/// struct Player {
///     #[auto_inc]
///     #[primary_key]
///     id: u64,
///
///     #[unique]
///     identity: Identity,
///
///     #[index(btree)]
///     level: u32,
/// }
///
/// impl Player {
///     fn merge(self, location: Location) -> PlayerAndLocation {
///         PlayerAndLocation {
///             player_id: self.id,
///             level: self.level,
///             x: location.x,
///             y: location.y,
///         }
///     }
/// }
///
/// #[derive(SpacetimeType)]
/// struct PlayerId {
///     id: u64,
/// }
///
/// #[table(name = location, index(name = coordinates, btree(columns = [x, y])))]
/// struct Location {
///     #[unique]
///     player_id: u64,
///     x: u64,
///     y: u64,
/// }
///
/// #[derive(SpacetimeType)]
/// struct PlayerAndLocation {
///     player_id: u64,
///     level: u32,
///     x: u64,
///     y: u64,
/// }
///
/// // A view that selects at most one row from a table
/// #[view(name = my_player, public)]
/// fn my_player(ctx: &ViewContext) -> Option<Player> {
///     ctx.db.player().identity().find(ctx.sender)
/// }
///
/// // An example of column projection
/// #[view(name = my_player_id, public)]
/// fn my_player_id(ctx: &ViewContext) -> Option<PlayerId> {
///     ctx.db.player().identity().find(ctx.sender).map(|Player { id, .. }| PlayerId { id })
/// }
///
/// // An example of a parameterized view
/// #[view(name = players_at_level, public)]
/// fn players_at_level(ctx: &AnonymousViewContext, level: u32) -> Vec<Player> {
///     ctx.db.player().level().filter(level).collect()
/// }
///
/// // An example that is analogous to a semijoin in sql
/// #[view(name = players_at_coordinates, public)]
/// fn players_at_coordinates(ctx: &AnonymousViewContext, x: u64, y: u64) -> Vec<Player> {
///     ctx
///         .db
///         .location()
///         .coordinates()
///         .filter((x, y))
///         .filter_map(|location| ctx.db.player().id().find(location.player_id))
///         .collect()
/// }
///
/// // An example of a join that combines fields from two different tables
/// #[view(name = players_with_coordinates, public)]
/// fn players_with_coordinates(ctx: &AnonymousViewContext, x: u64, y: u64) -> Vec<PlayerAndLocation> {
///     ctx
///         .db
///         .location()
///         .coordinates()
///         .filter((x, y))
///         .filter_map(|location| ctx
///             .db
///             .player()
///             .id()
///             .find(location.player_id)
///             .map(|player| player.merge(location))
///         )
///         .collect()
/// }
/// # }
/// ```
///
/// Just like reducers, views are limited in their ability to interact with the outside world.
/// They have no access to any network or filesystem interfaces.
/// Calling methods from [`std::io`], [`std::net`], or [`std::fs`] will result in runtime errors.
///
/// Views are callable by reducers and other views simply by passing their `ViewContext`..
/// This is a regular function call.
/// The callee will run within the caller's transaction.
///
///
/// [`&ViewContext`]: `ViewContext`
/// [`&AnonymousViewContext`]: `AnonymousViewContext`
#[doc(inline)]
pub use spacetimedb_bindings_macro::view;

/// One of two possible types that can be passed as the first argument to a `#[view]`.
/// The other is [`ViewContext`].
/// Use this type if the view does not depend on the caller's identity.
pub struct AnonymousViewContext {
    pub db: LocalReadOnly,
}

/// One of two possible types that can be passed as the first argument to a `#[view]`.
/// The other is [`AnonymousViewContext`].
/// Use this type if the view depends on the caller's identity.
pub struct ViewContext {
    pub sender: Identity,
    pub db: LocalReadOnly,
}

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

    /// The `ConnectionId` of the client that invoked the reducer.
    ///
    /// Will be `None` for certain reducers invoked automatically by the host,
    /// including `init` and scheduled reducers.
    pub connection_id: Option<ConnectionId>,

    /// Allows accessing the local database attached to a module.
    ///
    /// This slightly strange type appears to have no methods, but that is misleading.
    /// The `#[table]` macro uses the trait system to add table accessors to this type.
    /// These are generated methods that allow you to access specific tables.
    ///
    /// For a table named *table*, use `ctx.db.{table}()` to get a handle.
    /// For example:
    /// ```no_run
    /// # mod demo { // work around doctest+index issue
    /// # #![cfg(target_arch = "wasm32")]
    /// use spacetimedb::{table, reducer, ReducerContext};
    ///
    /// #[table(name = book)]
    /// #[derive(Debug)]
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
    ///     let book: &book__TableHandle = ctx.db.book();
    ///
    ///     log::debug!("looking up books by {author}...");
    ///     for book in book.author().filter(&author) {
    ///         log::debug!("- {book:?}");
    ///     }
    /// }
    /// # }
    /// ```
    /// See the [`#[table]`](macro@crate::table) macro for more information.
    pub db: Local,

    sender_auth: AuthCtx,

    #[cfg(feature = "rand08")]
    rng: std::cell::OnceCell<StdbRng>,
}

impl ReducerContext {
    #[doc(hidden)]
    pub fn __dummy() -> Self {
        Self {
            db: Local {},
            sender: Identity::__dummy(),
            timestamp: Timestamp::UNIX_EPOCH,
            connection_id: None,
            sender_auth: AuthCtx::internal(),
            #[cfg(feature = "rand08")]
            rng: std::cell::OnceCell::new(),
        }
    }

    #[doc(hidden)]
    fn new(db: Local, sender: Identity, connection_id: Option<ConnectionId>, timestamp: Timestamp) -> Self {
        let sender_auth = match connection_id {
            Some(cid) => AuthCtx::from_connection_id(cid),
            None => AuthCtx::internal(),
        };
        Self {
            db,
            sender,
            timestamp,
            connection_id,
            sender_auth,
            #[cfg(feature = "rand08")]
            rng: std::cell::OnceCell::new(),
        }
    }

    /// Returns the authorization information for the caller of this reducer.
    pub fn sender_auth(&self) -> &AuthCtx {
        &self.sender_auth
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

    /// Create an anonymous (no sender) read-only view context
    pub fn as_anonymous_read_only(&self) -> AnonymousViewContext {
        AnonymousViewContext { db: LocalReadOnly {} }
    }

    /// Create a sender-bound read-only view context using this reducer's caller.
    pub fn as_read_only(&self) -> ViewContext {
        ViewContext {
            sender: self.sender,
            db: LocalReadOnly {},
        }
    }
}

/// The context that any procedure is provided with.
///
/// Each procedure must accept `&mut ProcedureContext` as its first argument.
///
/// Includes information about the client calling the procedure and the time of invocation,
/// and exposes methods for running transactions and performing side-effecting operations.
pub struct ProcedureContext {
    /// The `Identity` of the client that invoked the procedure.
    pub sender: Identity,

    /// The time at which the procedure was started.
    pub timestamp: Timestamp,

    /// The `ConnectionId` of the client that invoked the procedure.
    ///
    /// Will be `None` for certain scheduled procedures.
    pub connection_id: Option<ConnectionId>,
    // TODO: Add rng?
    // Complex and requires design because we may want procedure RNG to behave differently from reducer RNG,
    // as it could actually be seeded by OS randomness rather than a deterministic source.
}

impl ProcedureContext {
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

    /// Suspend execution until approximately `Timestamp`.
    ///
    /// This will update `self.timestamp` to the new time after execution resumes.
    ///
    /// Actual time suspended may not be exactly equal to `duration`.
    /// Callers should read `self.timestamp` after resuming to determine the new time.
    ///
    /// ```no_run
    /// # use std::time::Duration;
    /// # use spacetimedb::{procedure, ProcedureContext};
    /// # #[procedure]
    /// # fn sleep_one_second(ctx: &mut ProcedureContext) {
    /// let prev_time = ctx.timestamp;
    /// let target = prev_time + Duration::from_secs(1);
    /// ctx.sleep_until(target);
    /// let new_time = ctx.timestamp;
    /// let actual_delta = new_time.duration_since(prev_time).unwrap();
    /// log::info!("Slept from {prev_time} to {new_time}, a total of {actual_delta:?}");
    /// # }
    /// ```
    // TODO(procedure-sleep-until): remove this method
    #[cfg(feature = "unstable")]
    pub fn sleep_until(&mut self, timestamp: Timestamp) {
        let new_time = sys::procedure::sleep_until(timestamp.to_micros_since_unix_epoch());
        let new_time = Timestamp::from_micros_since_unix_epoch(new_time);
        self.timestamp = new_time;
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

// `ProcedureContext` is *not* a `DbContext`. We will add a `TxContext`
// which can be obtained from `ProcedureContext::start_tx`,
// and that will be a `DbContext`.

/// Allows accessing the local database attached to the module.
///
/// This slightly strange type appears to have no methods, but that is misleading.
/// The `#[table]` macro uses the trait system to add table accessors to this type.
/// These are generated methods that allow you to access specific tables.
#[non_exhaustive]
pub struct Local {}

/// The [JWT] of an [`AuthCtx`].
///
/// [JWT]: https://en.wikipedia.org/wiki/JSON_Web_Token
#[non_exhaustive]
pub struct JwtClaims {
    payload: String,
    parsed: OnceCell<serde_json::Value>,
    audience: OnceCell<Vec<String>>,
}

/// Authentication information for the caller of a reducer.
pub struct AuthCtx {
    is_internal: bool,
    // NOTE(jsdt): cannot directly use a `LazyCell` without making this struct generic,
    // which would cause `ReducerContext` to become generic as well.
    jwt: Box<dyn Deref<Target = Option<JwtClaims>>>,
}

impl AuthCtx {
    fn new(is_internal: bool, jwt_fn: impl FnOnce() -> Option<JwtClaims> + 'static) -> Self {
        AuthCtx {
            is_internal,
            jwt: Box::new(LazyCell::new(jwt_fn)),
        }
    }

    /// Creates an [`AuthCtx`] for an internal call, with no [JWT].
    /// This represents a scheduled reducer.
    ///
    /// [JWT]: https://en.wikipedia.org/wiki/JSON_Web_Token
    pub fn internal() -> AuthCtx {
        Self::new(true, || None)
    }

    /// Creates an [`AuthCtx`] using the json claims from a [JWT].
    /// This can be used to write unit tests.
    ///
    /// [JWT]: https://en.wikipedia.org/wiki/JSON_Web_Token
    pub fn from_jwt_payload(jwt_payload: String) -> AuthCtx {
        Self::new(false, move || Some(JwtClaims::new(jwt_payload)))
    }

    /// Creates an [`AuthCtx`] that reads the [JWT] for the given connection id.
    ///
    /// [JWT]: https://en.wikipedia.org/wiki/JSON_Web_Token
    fn from_connection_id(connection_id: ConnectionId) -> AuthCtx {
        Self::new(false, move || rt::get_jwt(connection_id).map(JwtClaims::new))
    }

    /// Returns whether this reducer was spawned from inside the database.
    pub fn is_internal(&self) -> bool {
        self.is_internal
    }

    /// Checks if there is a [JWT] without loading it.
    /// If [`AuthCtx::is_internal`] returns true, this will return false.
    ///
    /// [JWT]: https://en.wikipedia.org/wiki/JSON_Web_Token
    pub fn has_jwt(&self) -> bool {
        self.jwt.is_some()
    }

    /// Loads the [JWT].
    ///
    /// [JWT]: https://en.wikipedia.org/wiki/JSON_Web_Token
    pub fn jwt(&self) -> Option<&JwtClaims> {
        self.jwt.as_ref().deref().as_ref()
    }
}

impl JwtClaims {
    fn new(jwt: String) -> Self {
        Self {
            payload: jwt,
            parsed: OnceCell::new(),
            audience: OnceCell::new(),
        }
    }

    fn get_parsed(&self) -> &serde_json::Value {
        self.parsed
            .get_or_init(|| serde_json::from_str(&self.payload).expect("Failed to parse JWT payload"))
    }

    /// Returns the tokens subject, from the sub claim.
    pub fn subject(&self) -> &str {
        self.get_parsed()
            .get("sub")
            .expect("Missing 'sub' claim")
            .as_str()
            .expect("Token 'sub' claim is not a string")
    }

    /// Returns the issuer for these credentials, from the iss claim.
    pub fn issuer(&self) -> &str {
        self.get_parsed().get("iss").unwrap().as_str().unwrap()
    }

    fn extract_audience(&self) -> Vec<String> {
        let Some(aud) = self.get_parsed().get("aud") else {
            return Vec::new();
        };
        match aud {
            serde_json::Value::String(s) => vec![s.clone()],
            serde_json::Value::Array(arr) => arr.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
            _ => panic!("Unexpected type for 'aud' claim in JWT"),
        }
    }

    /// Returns the audience for these credentials, from the aud claim.
    pub fn audience(&self) -> &[String] {
        self.audience.get_or_init(|| self.extract_audience())
    }

    /// Returns the identity for these credentials, which is
    /// based on the iss and sub claims.
    pub fn identity(&self) -> Identity {
        Identity::from_claims(self.issuer(), self.subject())
    }

    /// Get the whole JWT payload as a json string.
    ///
    /// This method is intended for parsing custom claims,
    /// beyond the methods offered by [`JwtClaims`].
    pub fn raw_payload(&self) -> &str {
        &self.payload
    }
}
/// The read-only version of [`Local`]
#[non_exhaustive]
pub struct LocalReadOnly {}

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
        panic!("Failed to get table with name: {table_name}");
    })
}

thread_local! {
    /// A global pool of buffers used for iteration.
    // This gets optimized away to a normal global since wasm32 doesn't have threads by default.
    static ITER_BUFS: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
}

struct IterBuf {
    buf: Vec<u8>,
}

impl IterBuf {
    /// Take a buffer from the pool of buffers for row iterators, if one exists. Otherwise, allocate a new one.
    fn take() -> Self {
        let buf = ITER_BUFS
            .with_borrow_mut(|v| v.pop())
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
        ITER_BUFS.with_borrow_mut(|v| v.push(buf));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_audience() {
        let example_payload = r#"
        {
          "iss": "https://securetoken.google.com/my-project-id",
          "aud": "my-project-id",
          "auth_time": 1695560000,
          "user_id": "abc123XYZ",
          "sub": "abc123XYZ",
          "iat": 1695560100,
          "exp": 1695563700,
          "email": "user@example.com",
          "email_verified": true,
          "firebase": {
            "identities": {
              "email": ["user@example.com"]
            },
            "sign_in_provider": "password"
          },
          "name": "Jane Doe",
          "picture": "https://lh3.googleusercontent.com/a-/profile.jpg"
        }
        "#;
        let auth = AuthCtx::from_jwt_payload(example_payload.to_string());
        let audience = auth.jwt().unwrap().audience();
        assert_eq!(audience.len(), 1);
        assert_eq!(audience, &["my-project-id".to_string()]);
    }
}
