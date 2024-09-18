//! The client cache, which stores a read-only replica of a subset of a remote database.
//!
//! Our representation is arguably too clever: each table is an [`im::HashMap`],
//! on which we perform a persistent clone-and-mutate after each transaction,
//! rather than just using a [`std::collections::HashMap`] which gets destructively modified.
//! This is mostly a leftover from a previous version of the SDK which was more concurrent.
//!
//! This module is internal, and may incompatibly change without warning.

use crate::spacetime_module::{InModule, SpacetimeModule, TableUpdate};
use anymap::{any::CloneAny, Map};
use bytes::Bytes;
use core::hash::{Hash, Hasher};
use core::ops::Deref;
use im::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

/// A local mirror of the subscribed rows of one table in the database.
#[derive(Clone)]
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

        if !new_subs
            .updates
            .iter()
            // (2) At this point we know that every update is uncompressed,
            // as we saw to that in (1).
            .filter_map(|cqu| match cqu {
                CompressableQueryUpdate::Uncompressed(qu) => Some(qu),
                _ => None,
            })
            .all(|u| u.deletes.is_empty())
        {
            log::error!(
                "Received non-`Insert` `TableRowOperation` for {:?} in new set",
                T::TABLE_NAME,
            );
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
    /// "keyed" on the type `HashMap<&'static str, Arc<TableCache<Row>>`.
    ///
    /// The strings are table names, since we may have multiple tables with the same row type.
    tables: Map<dyn CloneAny + Send + Sync>,

    _module: PhantomData<M>,
}

impl<M: SpacetimeModule> Clone for ClientCache<M> {
    fn clone(&self) -> Self {
        Self {
            tables: self.tables.clone(),
            _module: PhantomData,
        }
    }
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
    ) -> Option<&Arc<TableCache<Row>>> {
        self.tables
            .get::<HashMap<&'static str, Arc<TableCache<Row>>>>()
            .and_then(|tables_of_row_type| tables_of_row_type.get(table_name))
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

        // Clippy is incorrect here: `.cloned` will do `Arc::clone`, not `TableCache::clone`.
        // TODO: Do we need to be `Arc`ing these? `im::HashMap` should be doing sharing internally anyway.
        #[allow(clippy::map_clone)]
        let mut table = self
            .get_table::<Row>(table_name)
            .map(|tbl| TableCache::clone(tbl))
            .unwrap_or_default();

        table.apply_diff(diff);
        self.tables
            .entry::<HashMap<&'static str, Arc<TableCache<Row>>>>()
            .or_insert(HashMap::default())
            .insert(table_name, Arc::new(table));
    }
}

/// A shared view into a particular state of the `ClientCache`.
pub(crate) type ClientCacheView<M> = Arc<ClientCache<M>>;

/// A fake implementation of a unique index.
///
/// This struct should allow efficient point queries of a particular field in the table,
/// but our current implementation just does a full scan.
// TODO: Actual client-side indices.
pub struct UniqueConstraint<Row, Col> {
    pub(crate) get_unique_field: fn(&Row) -> &Col,
    pub(crate) table: Arc<TableCache<Row>>,
}

impl<Row: Clone, Col: PartialEq> UniqueConstraint<Row, Col> {
    pub fn find(&self, col_val: &Col) -> Option<Row> {
        self.table
            .entries
            .values()
            .find(|row| (self.get_unique_field)(row) == col_val)
            .cloned()
    }
}
