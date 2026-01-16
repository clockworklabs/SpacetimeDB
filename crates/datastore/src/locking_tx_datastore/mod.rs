#![deny(unsafe_op_in_unsafe_fn)]

pub mod committed_state;
pub mod datastore;
mod mut_tx;
pub use mut_tx::{FuncCallType, MutTxId, ViewCallInfo};
mod sequence;
pub mod state_view;
pub use state_view::{IterByColEqTx, IterByColRangeTx};
pub mod delete_table;
mod tx;
pub use tx::{NumDistinctValues, TxId};
mod tx_state;
#[cfg(any(test, feature = "test"))]
pub use tx_state::PendingSchemaChange;

use parking_lot::{
    lock_api::{ArcMutexGuard, ArcRwLockReadGuard, ArcRwLockWriteGuard},
    RawMutex, RawRwLock,
};

// Type aliases for lock guards
type SharedWriteGuard<T> = ArcRwLockWriteGuard<RawRwLock, T>;
type SharedMutexGuard<T> = ArcMutexGuard<RawMutex, T>;
type SharedReadGuard<T> = ArcRwLockReadGuard<RawRwLock, T>;
