use crate::subscription::websocket_building::{BsatnRowListBuilder, BuildableWebsocketFormat, RowListBuilderSource};
use bytes::{Bytes, BytesMut};
use core::sync::atomic::{AtomicUsize, Ordering};
use derive_more::Deref;
use spacetimedb_client_api_messages::websocket::{BsatnFormat, JsonFormat};
use spacetimedb_data_structures::object_pool::{Pool, PooledObject};
use spacetimedb_memory_usage::MemoryUsage;

/// The default buffer capacity, currently 4 KiB.
const DEFAULT_BUFFER_CAPACITY: usize = 4096;

/// The pool can store at most 4 MiB worth of buffers.
/// NOTE(centril): This hasn't been measured yet,
/// but this should be a fairly good initial guestimate
/// as the server would need to handle half as many tables in total.
/// If there are two queries mentioning the same table,
/// that counts as two tables.
const DEFAULT_POOL_CAPACITY: usize = 1024;

/// New-type for `BytesMut` to deal with the orphan check.
pub struct PooledBuffer(BytesMut);

impl MemoryUsage for PooledBuffer {
    fn heap_usage(&self) -> usize {
        self.0.heap_usage()
    }
}

impl PooledObject for PooledBuffer {
    type ResidentBytesStorage = AtomicUsize;

    fn resident_object_bytes(storage: &Self::ResidentBytesStorage, _: usize) -> usize {
        storage.load(Ordering::Relaxed)
    }

    fn add_to_resident_object_bytes(storage: &Self::ResidentBytesStorage, bytes: usize) {
        storage.fetch_add(bytes, Ordering::Relaxed);
    }

    fn sub_from_resident_object_bytes(storage: &Self::ResidentBytesStorage, bytes: usize) {
        storage.fetch_sub(bytes, Ordering::Relaxed);
    }
}

/// The pool for [`BsatnRowListBuilder`]s.
#[derive(Clone, Deref, Debug)]
pub struct BsatnRowListBuilderPool {
    pool: Pool<PooledBuffer>,
}

impl BsatnRowListBuilderPool {
    /// Returns a new pool with the default maximum capacity.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let pool = Pool::new(DEFAULT_POOL_CAPACITY);
        Self { pool }
    }

    /// Tries to reclaim the allocation of `buffer` into the pool
    /// to be used when building a new list.
    ///
    /// In most calls, this method will do nothing,
    /// as `buffer` will be shared between clients subscribing to the same query.
    /// It's only on the last client that the refcount will be 1
    /// which will then cause `put` to add the allocation into the buffer.
    pub fn try_put(&self, buffer: Bytes) {
        if let Ok(bytes) = buffer.try_into_mut() {
            self.put(PooledBuffer(bytes));
        }
    }
}

impl RowListBuilderSource<BsatnFormat> for BsatnRowListBuilderPool {
    fn take_row_list_builder(&self) -> BsatnRowListBuilder {
        let PooledBuffer(buffer) = self.pool.take(
            |buffer| buffer.0.clear(),
            || PooledBuffer(BytesMut::with_capacity(DEFAULT_BUFFER_CAPACITY)),
        );
        BsatnRowListBuilder::new_from_bytes(buffer)
    }
}

/// The "pool" for the builder for the [`JsonFormat`].
pub(crate) struct JsonRowListBuilderFakePool;

impl RowListBuilderSource<JsonFormat> for JsonRowListBuilderFakePool {
    fn take_row_list_builder(&self) -> <JsonFormat as BuildableWebsocketFormat>::ListBuilder {
        Vec::new()
    }
}
