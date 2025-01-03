//! Provides safe abstractions around `bindings-sys`
//! and re-exports `#[spacetimedb]` and `#[duration]`.

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
pub use spacetimedb_bindings_macro::{duration, filter, reducer, table};
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

/// A context that any reducer is provided with.
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
    /// as modules have only a single implementor of `DbContext`.
    fn db(&self) -> &Self::DbView;
}

impl DbContext for ReducerContext {
    type DbView = Local;

    fn db(&self) -> &Self::DbView {
        &self.db
    }
}

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
