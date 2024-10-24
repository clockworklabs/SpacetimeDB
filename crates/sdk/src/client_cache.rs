//! The client cache, which stores a read-only replica of a subset of a remote database.
//!
//! Our representation is arguably too clever: each table is an [`im::HashMap`],
//! on which we perform a persistent clone-and-mutate after each transaction,
//! rather than just using a [`std::collections::HashMap`] which gets destructively modified.
//! This is mostly a leftover from a previous version of the SDK which was more concurrent.
//!
//! This module is internal, and may incompatibly change without warning.

use crate::callbacks::CallbackId;
use crate::db_connection::{PendingMutation, SharedCell};
use crate::spacetime_module::{InModule, SpacetimeModule, TableUpdate};
use anymap::{any::Any, Map};
use bytes::Bytes;
use futures_channel::mpsc;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

/// A local mirror of the subscribed rows of one table in the database.
pub struct TableCache<Row> {
    /// A map of row-bytes to rows.
    ///
    /// The keys are BSATN-serialized representations of the values.
    /// Storing both the bytes and the deserialized rows allows us to have a `HashMap`
    /// even when `Row` is not `Hash + Eq`, e.g. for row types which contain floats.
    /// We also suspect that hashing and equality comparisons for byte arrays
    /// are more efficient than for domain types,
    /// as they can be implemented directly via SIMD without skipping padding
    /// or branching on enum variants.
    ///
    /// Note that this is an [`im::HashMap`], and so can be shared efficiently.
    pub(crate) entries: HashMap<Bytes, Row>,
}

// Can't derive this because the `Row` generic messes us up.
impl<Row> Default for TableCache<Row> {
    fn default() -> Self {
        Self {
            entries: Default::default(),
        }
    }
}

impl<Row: Clone> TableCache<Row> {
    /// Apply all the deletes, inserts and updates recorded in `diff`.
    pub fn apply_diff(&mut self, diff: &TableUpdate<Row>) {
        // Apply deletes strictly before inserts,
        // to avoid needlessly growing the `entries` map.

        for delete in &diff.deletes {
            self.entries.remove(&delete.bsatn);
        }
        for update in &diff.updates {
            self.entries.remove(&update.delete.bsatn);
        }
        for insert in &diff.inserts {
            self.entries.insert(insert.bsatn.clone(), insert.row.clone());
        }
        for update in &diff.updates {
            self.entries
                .insert(update.insert.bsatn.clone(), update.insert.row.clone());
        }
    }
}

/// A local mirror of the subscribed subset of the database.
pub struct ClientCache<M: SpacetimeModule> {
    /// "keyed" on the type `HashMap<&'static str, TableCache<Row>`.
    ///
    /// The strings are table names, since we may have multiple tables with the same row type.
    tables: Map<dyn Any + Send + Sync>,

    _module: PhantomData<M>,
}

impl<M: SpacetimeModule> Default for ClientCache<M> {
    fn default() -> Self {
        Self {
            tables: Map::new(),
            _module: PhantomData,
        }
    }
}

impl<M: SpacetimeModule> ClientCache<M> {
    /// Get a handle on the [`TableCache`] which stores rows of type `Row` for the table `table_name`.
    pub(crate) fn get_table<Row: InModule<Module = M> + Send + Sync + 'static>(
        &self,
        table_name: &'static str,
    ) -> Option<&TableCache<Row>> {
        self.tables
            .get::<HashMap<&'static str, TableCache<Row>>>()
            .and_then(|tables_of_row_type| tables_of_row_type.get(table_name))
    }

    fn get_or_make_table<Row: InModule<Module = M> + Send + Sync + 'static>(
        &mut self,
        table_name: &'static str,
    ) -> &mut TableCache<Row> {
        self.tables
            .entry::<HashMap<&'static str, TableCache<Row>>>()
            .or_insert_with(Default::default)
            .entry(table_name)
            .or_default()
    }

    /// Apply all the mutations in `diff`
    /// to the [`TableCache`] which stores rows of type `Row` for the table `table_name`.
    pub fn apply_diff_to_table<Row: InModule<Module = M> + Clone + Send + Sync + 'static>(
        &mut self,
        table_name: &'static str,
        diff: &TableUpdate<Row>,
    ) {
        if diff.is_empty() {
            return;
        }

        let table = self.get_or_make_table::<Row>(table_name);

        table.apply_diff(diff);
    }
}

/// Internal implementation of a generated `TableHandle` struct,
/// which mediates access to a table in the client cache.
///
/// `TableHandle`s don't actually hold a direct reference to the table they access,
/// as that would require both gnarly lifetimes and also a `MutexGuard` on the client cache.
/// Instead, they hold an `Arc<Mutex>` on the whole [`ClientCache`],
/// with every operation through the table handle
/// acquiring the lock only for the duration of the operation,
/// calling [`ClientCache::get_table`] and then discarding its reference before returning.
pub struct TableHandle<Row: InModule> {
    pub(crate) client_cache: SharedCell<ClientCache<Row::Module>>,
    /// Handle on the connection's `pending_mutations_send` channel,
    /// so we can send callback-related [`PendingMutation`] messages.
    pub(crate) pending_mutations: mpsc::UnboundedSender<PendingMutation<Row::Module>>,

    /// The name of the table.
    pub(crate) table_name: &'static str,
}

impl<Row: InModule> Clone for TableHandle<Row> {
    fn clone(&self) -> Self {
        Self {
            client_cache: Arc::clone(&self.client_cache),
            pending_mutations: self.pending_mutations.clone(),
            table_name: self.table_name,
        }
    }
}

impl<Row: InModule + Send + Sync + Clone + 'static> TableHandle<Row> {
    /// Read something out of the [`TableCache`] which this `TableHandle` accesses.
    ///
    /// If the table has never had any rows resident, the [`TableCache`] may not exist.
    /// In this case, `get` is never invoked, and `None` is returned.
    fn get<Res>(&self, get: impl FnOnce(&TableCache<Row>) -> Res) -> Option<Res> {
        let client_cache = self.client_cache.lock().unwrap();
        client_cache.get_table::<Row>(self.table_name).map(get)
    }

    /// Read something out of the [`TableCache`] which this `TableHandle` accesses,
    /// returning a default value if the table has not been constructed
    /// because no rows are resident.
    fn get_or_default<Res: Default>(&self, get: impl FnOnce(&TableCache<Row>) -> Res) -> Res {
        self.get(get).unwrap_or_default()
    }

    /// Called by the autogenerated implementation of the [`crate::Table`] method of the same name.
    pub fn count(&self) -> u64 {
        self.get_or_default(|table| table.entries.len() as u64)
    }

    /// Called by the autogenerated implementation of the [`crate::Table`] method of the same name.
    pub fn iter(&self) -> impl Iterator<Item = Row> {
        self.get_or_default(|table| table.entries.values().cloned().collect::<Vec<_>>())
            .into_iter()
    }

    /// See [`DbContextImpl::queue_mutation`].
    fn queue_mutation(&self, mutation: PendingMutation<Row::Module>) {
        self.pending_mutations.unbounded_send(mutation).unwrap();
    }

    /// Called by the autogenerated implementation of the [`crate::Table`] method of the same name.
    pub fn on_insert(
        &self,
        mut callback: impl FnMut(&<Row::Module as SpacetimeModule>::EventContext, &Row) + Send + 'static,
    ) -> CallbackId {
        let callback_id = CallbackId::get_next();
        self.queue_mutation(PendingMutation::AddInsertCallback {
            table: self.table_name,
            callback: Box::new(move |ctx, row| {
                let row = row.downcast_ref::<Row>().unwrap();
                callback(ctx, row);
            }),
            callback_id,
        });
        callback_id
    }

    /// Called by the autogenerated implementation of the [`crate::Table`] method of the same name.
    pub fn remove_on_insert(&self, callback: CallbackId) {
        self.queue_mutation(PendingMutation::RemoveInsertCallback {
            table: self.table_name,
            callback_id: callback,
        });
    }

    /// Called by the autogenerated implementation of the [`crate::Table`] method of the same name.
    pub fn on_delete(
        &self,
        mut callback: impl FnMut(&<Row::Module as SpacetimeModule>::EventContext, &Row) + Send + 'static,
    ) -> CallbackId {
        let callback_id = CallbackId::get_next();
        self.queue_mutation(PendingMutation::AddDeleteCallback {
            table: self.table_name,
            callback: Box::new(move |ctx, row| {
                let row = row.downcast_ref::<Row>().unwrap();
                callback(ctx, row);
            }),
            callback_id,
        });
        callback_id
    }

    /// Called by the autogenerated implementation of the [`crate::Table`] method of the same name.
    pub fn remove_on_delete(&self, callback: CallbackId) {
        self.queue_mutation(PendingMutation::RemoveDeleteCallback {
            table: self.table_name,
            callback_id: callback,
        });
    }

    /// Called by the autogenerated implementation of the [`crate::TableWithPrimaryKey`] method of the same name.
    pub fn on_update(
        &self,
        mut callback: impl FnMut(&<Row::Module as SpacetimeModule>::EventContext, &Row, &Row) + Send + 'static,
    ) -> CallbackId {
        let callback_id = CallbackId::get_next();
        self.queue_mutation(PendingMutation::AddUpdateCallback {
            table: self.table_name,
            callback: Box::new(move |ctx, old, new| {
                let old = old.downcast_ref::<Row>().unwrap();
                let new = new.downcast_ref::<Row>().unwrap();
                callback(ctx, old, new);
            }),
            callback_id,
        });
        callback_id
    }

    /// Called by the autogenerated implementation of the [`crate::TableWithPrimaryKey`] method of the same name.
    pub fn remove_on_update(&self, callback: CallbackId) {
        self.queue_mutation(PendingMutation::RemoveUpdateCallback {
            table: self.table_name,
            callback_id: callback,
        });
    }

    /// Called by autogenerated unique index access methods.
    pub fn get_unique_constraint<Col>(
        &self,
        _constraint_name: &'static str,
        get_unique_field: fn(&Row) -> &Col,
    ) -> UniqueConstraintHandle<Row, Col> {
        UniqueConstraintHandle {
            table_handle: self.clone(),
            get_unique_field,
        }
    }
}

/// A fake implementation of a unique index.
///
/// This struct should allow efficient point queries of a particular field in the table,
/// but our current implementation just does a full scan.
///
/// Like [`TableHandle`], unique constraint handles don't hold a direct reference to their table
/// or an index within it. (No such index currently exists, anyways.)
/// Instead, they hold a handle on the whole [`ClientCache`],
/// and acquire short-lived exclusive access to it during operations.
// TODO: Actual client-side indices.
pub struct UniqueConstraintHandle<Row: InModule, Col> {
    table_handle: TableHandle<Row>,
    pub(crate) get_unique_field: fn(&Row) -> &Col,
}

impl<Row: Clone + InModule + Send + Sync + 'static, Col: PartialEq> UniqueConstraintHandle<Row, Col> {
    pub fn find(&self, col_val: &Col) -> Option<Row> {
        self.table_handle.get_or_default(|table| {
            table
                .entries
                .values()
                .find(|row| (self.get_unique_field)(row) == col_val)
                .cloned()
        })
    }
}
