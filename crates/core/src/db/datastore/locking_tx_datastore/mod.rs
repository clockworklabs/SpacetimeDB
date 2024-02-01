#![forbid(unsafe_op_in_unsafe_fn)]

pub(crate) mod committed_state;
pub(crate) mod datastore;
pub(crate) mod mut_tx;
pub(crate) use mut_tx::MutTxId;
pub(crate) mod sequence;
pub(crate) mod state_view;
pub use state_view::{Iter, IterByColEq, IterByColRange};
pub(crate) mod tx;
pub(crate) mod tx_state;

use parking_lot::{
    lock_api::{ArcMutexGuard, ArcRwLockReadGuard, ArcRwLockWriteGuard},
    RawMutex, RawRwLock,
};

// Type aliases for lock guards
type SharedWriteGuard<T> = ArcRwLockWriteGuard<RawRwLock, T>;
type SharedMutexGuard<T> = ArcMutexGuard<RawMutex, T>;
type SharedReadGuard<T> = ArcRwLockReadGuard<RawRwLock, T>;
