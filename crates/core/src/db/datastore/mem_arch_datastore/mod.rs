#![forbid(unsafe_op_in_unsafe_fn)]

pub mod blob_store;
pub mod btree_index;
pub mod committed_state;
pub mod datastore;
pub mod de;
pub mod eq;
pub mod indexes;
pub mod layout;
pub mod mut_tx;
pub mod page;
pub mod pages;
pub mod pointer_map;
pub mod row_hash;
pub mod row_type_visitor;
pub mod sequence;
pub mod ser;
pub mod state_view;
pub mod table;
pub mod tx;
pub mod tx_state;
pub mod var_len;

#[cfg(test)]
mod proptest_sats;

#[doc(hidden)] // Used in tests and benchmarks.
pub mod util;

use parking_lot::{
    lock_api::{ArcMutexGuard, ArcRwLockReadGuard, ArcRwLockWriteGuard},
    RawMutex, RawRwLock,
};

// Type aliases for lock gaurds
type SharedWriteGuard<T> = ArcRwLockWriteGuard<RawRwLock, T>;
type SharedMutexGuard<T> = ArcMutexGuard<RawMutex, T>;
type SharedReadGuard<T> = ArcRwLockReadGuard<RawRwLock, T>;
