#![doc = include_str!("../README.md")]

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

/// Generates code for registering a row-level security `SQL` function.
///
/// A row-level security function takes a `SQL` query expression that is used to filter rows.
/// <!-- TODO(1.0): what does this mean. -->
///
/// The query follows the same syntax as a subscription query.
///
/// **Example:**
///
/// ```rust,ignore
/// /// Players can only see what's in their chunk
/// spacetimedb::filter!("
///     SELECT * FROM LocationState WHERE chunk_index IN (
///         SELECT chunk_index FROM LocationState WHERE entity_id IN (
///             SELECT entity_id FROM UserState WHERE identity = @sender
///         )
///     )
/// ");
/// ```
///
/// **NOTE:** The `SQL` query expression is pre-parsed at compile time, but this only checks
/// that it is a valid subscription query *syntactically*, not that the query is valid when executed.
///
/// For example, it could refer to a non-existent table.
#[doc(inline)]
pub use spacetimedb_bindings_macro::filter;

/// Declares a table with a particular row format.
/// 
/// This attribute is applied to a struct type.
/// This derives [`Serialize`], [`Deserialize`], [`SpacetimeType`], and [`Debug`] for the annotated type.
///
/// Elements of the struct type are NOT automatically inserted into any global table.
/// They are regular structs, with no special behavior.
/// In particular, modifying them does not automatically modify the database!
///
/// Instead, a struct implementing [`Table<Row = Self>`] is generated. This can be looked up in a [`ReducerContext`]
/// using `ctx.db().table_name()`. This method represents a handle to a database table, and can be used to
/// iterate and modify the table's elements. It is a view of the entire table -- the entire set of rows at the time of the reducer call.
///
/// # Example
///
/// ```ignore
/// use spacetimedb::{table, ReducerCtx};
/// use log::debug;
///
/// #[table(name = users, public,
///         index(name = id_and_username, btree(id, username)))]
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
/// fn demo(ctx: &ReducerCtx) {
///     // Use the *name* of the table to get a struct
///     // implementing `spacetimedb::Table<Row = User>`.
///     let users = ctx.db().users();
///
///     // You can use methods from `spacetimedb::Table`
///     // on the table.
///     debug!("User count: {}", users.count());
///     for user in users.iter() {
///         debug!("{:?}", user);
///     }
///
///     // For every named `index`, the table has an extra method
///     // for getting a corresponding `spacetimedb::BTreeIndex`.
///     let by_id_and_username: spacetimedb::BTreeIndex<_, (u32, String), _> =
///         users.id_and_username();
///     by_id_and_username.delete((&57, &"Billy".to_string()));
///
///     // For every `#[unique]` or `#[primary_key]` field,
///     // the table has an extra method that allows getting a
///     // corresponding `spacetimedb::UniqueColumn`.
///     let by_username: spacetimedb::UniqueColumn<_, String, _> = users.id();
///     by_username.delete(&"test_user".to_string());
/// }
/// ```
///
/// # Macro arguments
///
/// * `name = my_table`
///
///    Specify the name of the table in the database, if you want it to be different from
///    the name of the struct.
///    Multiple `table` annotations can be present on the same type. This will generate
///    multiple tables of the same row type, but with different names.
///
/// * `public` and `private`
///
///    Tables are private by default. If you'd like to make your table publically
///    accessible by anyone, put `public` in the macro arguments (e.g.
///    `#[spacetimedb::table(public)]`). You can also specify `private` if
///    you'd like to be specific. This is fully separate from Rust's module visibility
///    system; `pub struct` or `pub(crate) struct` do not affect the table visibility, only
///    the visibility of the items in your own source code.
///
/// * `index(name = my_index, btree(columns = [a, b, c]))`
///
///    You can specify an index on one or more of the table's columns with the above syntax.
///    You can also just put `#[index(btree)]` on the field itself if you only need
///    a single-column attribute; see column attributes below.
///    Multiple indexes are permitted.
///
/// * `scheduled(reducer_name)`
///
///    Scheduled [reducers](macro@crate::reducer) need a table storing scheduling information.
///    The rows of this table store all information needed when invoking a scheduled reducer.
///    This can be any information you want, but we require that the tables store at least an
///    invocation ID field and timestamp field.
///
///    The corresponding reducer should accept a single argument
///
///    These can be declared like so:
///
/// ```ignore
/// #[table(name = train_schedule, scheduled(run_train))]
/// pub struct TrainSchedule {
///     // Required fields.
///     #[primary_key]
///     #[auto_inc]
///     scheduled_id: u64,
///     #[scheduled_at]
///     scheduled_at: spacetimedb::ScheduleAt,
///
///     // Any other fields needed.
///     train: TrainID,
///     source_station: StationID,
///     target_station: StationID
/// }
///
/// #[reducer]
/// pub fn run_train(ctx: &ReducerCtx, schedule: TrainSchedule) {
///     /* ... */
/// }
/// ```
///
/// # Column (field) attributes
///
/// * `#[auto_inc]`
///
///    Creates a database sequence.
///
///    When a row is inserted with the annotated field set to `0` (zero),
///    the sequence is incremented, and this value is used instead.
///    Can only be used on numeric types and may be combined with indexes.
///
///    Note that using `#[auto_inc]` on a field does not also imply `#[primary_key]` or `#[unique]`.
///    If those semantics are desired, those attributes should also be used.
///
/// * `#[unique]`
///
///    Creates an index and unique constraint for the annotated field.
///
/// * `#[primary_key]`
///
///    Similar to `#[unique]`, but generates additional CRUD methods.
///
/// * `#[index(btree)]`
///
///    Creates a single-column index with the specified algorithm.
///
/// * `#[scheduled_at]`
///    Used in scheduled reducer tables, see above.
///
/// * `#[scheduled_id]`
///    Used in scheduled reducer tables, see above.
///
/// # Generated code
///
/// For each `[table(name = {name})]` annotation on a type `{T}`, generates a struct
/// `{name}Handle` implementing `Table<Row={T}>`, and a trait that allows looking up such a
/// `{name}Handle` in a `ReducerContext`.
///
/// The struct `{name}Handle` is hidden in an anonymous scope and cannot be accessed.
///
/// For each named index declaration, add a method to `{name}Handle` for getting a corresponding
/// `BTreeIndex`.
///
/// For each field  with a `#[unique]` or `#[primary_key]` annotation,
/// add a method to `{name}Handle` for getting a corresponding `UniqueColumn`.
///
/// The following pseudocode illustrates the general idea. Curly braces are used to indicate templated
/// names.
///
/// ```ignore
/// use spacetimedb::{BTreeIndex, UniqueColumn, Table, DbView};
///
/// // This generated struct is hidden and cannot be directly accessed.
/// struct {name}Handle { /* ... */ };
///
/// // It is a table handle.
/// impl Table for {name}Handle {
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
/// Reducers may fail by returning a [`Result::Err`](`Result`) or by [panicking](`panic`).
/// Such a failure will be printed to the module logs and abort the active database transaction.
/// Any changes to the database will be rolled back.
///
/// Reducers are very limited in their ability to interact with the outside world.
/// They do not directly return data aside from errors, and have no access to any
/// network or filesystem interfaces.
/// Calling methods from [`std::io`], [`std::net`], or [`std::fs`]
/// inside a reducer will result in runtime errors.
/// 
/// Reducers can communicate information to the outside world in two ways:
/// - They can modify tables in the database.
/// - They can call logging macros from the [`log`] crate.
/// 
///
///
/// # Lifecycle Reducers
///
/// You can specify special lifecycle reducers that are run at set points in
/// the module's lifecycle. You can have one each per module.
///
/// ## `#[spacetimedb::reducer(init)]`
///
/// This reducer is run the first time a module is published
/// and anytime the database is cleared.
///
/// The reducer cannot be called manually
/// and may not have any parameters except for `ReducerContext`.
/// If an error occurs when initializing, the module will not be published.
///
/// ## `#[spacetimedb::reducer(client_connected)]`
///
/// This reducer is run when a client connects to the SpacetimeDB module.
/// Their identity can be found in the sender value of the `ReducerContext`.
///
/// The reducer cannot be called manually
/// and may not have any parameters except for `ReducerContext`.
/// If an error occurs in the reducer, the client will be disconnected.
///
///
/// ## `#[spacetimedb::reducer(client_disconnected)]`
///
/// This reducer is run when a client disconnects from the SpacetimeDB module.
/// Their identity can be found in the sender value of the `ReducerContext`.
///
/// The reducer cannot be called manually
/// and may not have any parameters except for `ReducerContext`.
/// If an error occurs in the disconnect reducer,
/// the client is still recorded as disconnected.
///
/// ## `#[spacetimedb::reducer(update)]`
///
/// This reducer is run when the module is updated,
/// i.e., when publishing a module for a database that has already been initialized.
///
/// The reducer cannot be called manually and may not have any parameters.
/// If an error occurs when initializing, the module will not be published.
///
///
/// [`&ReducerContext`]: `ReducerContext`
/// [clients]: https://spacetimedb.com/docs/#client
#[doc(inline)]
pub use spacetimedb_bindings_macro::reducer;

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
    /// Run `cargo doc` in your SpacetimeDB module project and browse the generated documentation
    /// to see the methods have been automatically added to this type.
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
/// Run `cargo doc` in your Rust module project and browse the generated documentation
/// to see the methods have been automatically added to this type.
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
