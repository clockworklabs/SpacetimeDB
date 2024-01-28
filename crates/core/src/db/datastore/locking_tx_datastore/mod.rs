mod btree_index;
mod committed_state;
mod datastore;
mod mut_tx;
mod sequence;
mod state_view;
mod table;
mod tx;
mod tx_state;

pub use self::mut_tx::MutTxId;
pub use datastore::{DataRef, Locking, RowId};
pub use state_view::{Iter, IterByColEq, IterByColRange, StateView as _};

use parking_lot::{
    lock_api::{ArcMutexGuard, ArcRwLockReadGuard, ArcRwLockWriteGuard},
    RawMutex, RawRwLock,
};

// Type aliases for lock guards
type SharedWriteGuard<T> = ArcRwLockWriteGuard<RawRwLock, T>;
type SharedMutexGuard<T> = ArcMutexGuard<RawMutex, T>;
type SharedReadGuard<T> = ArcRwLockReadGuard<RawRwLock, T>;
