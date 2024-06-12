use crate::callbacks::DbCallbacks;
use crate::reducer::AnyReducerEvent;
use crate::spacetime_module::SpacetimeModule;
use crate::table::{TableType, TableWithPrimaryKey};
use crate::ws_messages;
use anymap::{
    any::{Any, CloneAny},
    Map,
};
use bytes::Bytes;
use im::HashMap;
use spacetimedb_data_structures::map::HashMap as StdHashMap;
use spacetimedb_sats::bsatn;
use std::sync::Arc;

/// A local mirror of the subscribed rows of one table in the database.
///
/// `T` should be a `TableType`.
///
/// References to this struct are autogenerated in the `handle_table_update` and
/// `handle_resubscribe` functions. Users should not reference this struct directly.
#[derive(Clone)]
pub struct TableCache<T: TableType> {
    /// A map of row-bytes to rows.
    ///
    /// The keys are BSATN-serialized representations of the values.
    /// Storing both the bytes and the deserialized rows allows us to have a `HashMap`
    /// even when `T` is not `Hash + Eq`, e.g. for row types which contain floats.
    /// We also suspect that hashing and equality comparisons for byte arrays
    /// are more efficient than for domain types,
    /// as they can be implemented directly via SIMD without skipping padding
    /// or branching on enum variants.
    ///
    /// Note that this is an [`im::HashMap`], and so can be shared efficiently.
    entries: HashMap<Bytes, T>,
}

impl<T: TableType> TableCache<T> {
    /// Returns the number of rows resident in the client cache for this `TableType`,
    /// i.e. the number of subscribed rows.
    pub(crate) fn count_subscribed_rows(&self) -> usize {
        self.entries.len()
    }

    /// Insert `value` into the cache and invoke any on-insert callbacks.
    ///
    /// `row_bytes` should be the BSATN-serialized representation of `value`.
    /// It will be used as the key in a `HashMap`.
    fn insert(&mut self, callbacks: &mut Vec<RowCallback<T>>, row_bytes: Bytes, value: T) {
        callbacks.push(RowCallback::Insert(value.clone()));

        if self.entries.insert(row_bytes, value).is_some() {
            log::warn!("Inserting a row already presint in table {:?}", T::TABLE_NAME);
        }
    }

    /// Delete `value` from the cache and invoke any on-delete callbacks.
    ///
    /// `row_bytes` should be the BSATN-serialized representation of `value`.
    /// It will be used as the key in a `HashMap`.
    fn delete(&mut self, callbacks: &mut Vec<RowCallback<T>>, row_bytes: Bytes, value: T) {
        callbacks.push(RowCallback::Delete(value));

        if self.entries.remove(&row_bytes).is_none() {
            log::error!(
                "Received delete for table {:?} row we weren't subscribed to",
                T::TABLE_NAME
            );
        };
    }

    fn new() -> TableCache<T> {
        TableCache {
            entries: HashMap::new(),
        }
    }

    fn decode_row(row: &Bytes) -> Option<T> {
        match bsatn::from_slice::<T>(row) {
            Ok(value) => Some(value),
            Err(e) => {
                log::error!(
                    "Error while deserializing row from TableRowOperation: {:?}. Row is {:?}",
                    e,
                    row
                );
                None
            }
        }
    }

    /// For each `insert` or `delete` in the `table_update`, insert or remove the row into
    /// or from the cache as appropriate. Do not handle primary keys, and do not generate
    /// `on_update` methods.
    fn handle_table_update_no_primary_key(
        &mut self,
        callbacks: &mut Vec<RowCallback<T>>,
        table_update: ws_messages::TableUpdate,
    ) {
        for row in table_update.deletes {
            if let Some(value) = Self::decode_row(&row.0) {
                self.delete(callbacks, row.0, value)
            }
        }
        for row in table_update.inserts {
            if let Some(value) = Self::decode_row(&row.0) {
                self.insert(callbacks, row.0, value)
            }
        }
    }

    // Implementing iteration over the client cache stinks.
    //
    // We want to provide `fn iter() -> impl Iterator<Item = Self>` (or `&Self`, or whatever)
    // as a method on the spacetimedb_client_sdk::TableType trait, analogous to the method
    // on spacetimedb_bindings::TableType.
    //
    // On the client, a table is effectively a global `Mutex<HashMap<_, T: TableType>>`.
    //
    // This means that the iterator returned by `iter` must hold a `MutexGuard` on the
    // table alongside a `HashMap` iterator into that mutex guard, i.e.:
    //
    // ```
    // struct TableIter<T> {
    //   lock: MutexGuard<'static, HashMap<Vec<u8>, T>>,
    //   iter: Cloned<Values<Vec<u8>, T>>,
    // }
    // ```
    //
    // But `iter` borrows from `lock`, and rustc doesn't like self-referential
    // structs. There's no actual interior pointer into `lock` involved, just a pointer to
    // the `HashMap` derived from `lock`, but rustc doesn't know that. We could hack
    // around this using `unsafe`, or [Rental](https://crates.io/crates/rental), but
    // Rental is unmaintained and `unsafe` is scary. Mazdak wrote a [sample version using
    // self_cell](https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=e0eba2569b1cbfb00a646cdb26b737a9),
    // but it's can't be generic over `T`. So until we figure out a better way, we just
    // copy out of the `HashMap` into a `Vec`, and `into_iter` that.
    pub(crate) fn values(&self) -> Vec<T> {
        self.entries.values().cloned().collect()
    }

    pub(crate) fn filter(&self, mut test: impl FnMut(&T) -> bool) -> Vec<T> {
        self.entries.values().filter(|&t| test(t)).cloned().collect()
    }

    pub(crate) fn find(&self, mut test: impl FnMut(&T) -> bool) -> Option<T> {
        self.entries.values().find(|&t| test(t)).cloned()
    }

    /// For each previously-subscribed row not in the `new_subs`, delete it from the cache
    /// and issue an `on_delete` event for it. For each new row in the `new_subs` not
    /// already in the cache, add it to the cache and issue an `on_insert` event for it.
    fn reinitialize_for_new_subscribed_set(
        &mut self,
        callbacks: &mut Vec<RowCallback<T>>,
        new_subs: ws_messages::TableUpdate,
    ) {
        // TODO: there should be a fast path where `self` is empty prior to this
        //       operation, where we avoid building a diff and just insert all the
        //       `new_subs`.
        enum DiffEntryKind {
            Insert,
            Delete,
            NoChange,
        }
        struct DiffEntry<T> {
            kind: DiffEntryKind,
            value: T,
        }
        let diffentry_delete = |value| DiffEntry {
            kind: DiffEntryKind::Delete,
            value,
        };
        let diffentry_insert = |value| DiffEntry {
            kind: DiffEntryKind::Insert,
            value,
        };

        let prev_subs = std::mem::take(&mut self.entries);

        let mut diff: StdHashMap<Bytes, DiffEntry<T>> = StdHashMap::with_capacity(
            // pre-allocate plenty of space to avoid hash conflicts
            (new_subs.inserts.len() + prev_subs.len()) * 2,
        );

        for (row_bytes, value) in prev_subs.into_iter() {
            log::trace!(
                "Initalizing table {:?}: row previously resident: {:?}",
                T::TABLE_NAME,
                value,
            );
            if diff.insert(row_bytes, diffentry_delete(value)).is_some() {
                // This should be impossible, but just in case...
                log::error!("Found duplicate row in existing `TableCache` for {:?}", T::TABLE_NAME);
            }
        }

        if !new_subs.deletes.is_empty() {
            log::error!(
                "Received non-`Insert` `TableRowOperation` for {:?} in new set",
                T::TABLE_NAME,
            );
        }

        for ws_messages::BsatnBytes(row_bytes) in new_subs.inserts {
            match diff.entry(row_bytes) {
                spacetimedb_data_structures::map::Entry::Vacant(v) => {
                    if let Some(row) = Self::decode_row(v.key()) {
                        log::trace!("Initializing table {:?}: got new row {:?}.", T::TABLE_NAME, row);
                        v.insert(diffentry_insert(row));
                    }
                }

                spacetimedb_data_structures::map::Entry::Occupied(mut o) => {
                    let entry = o.get_mut();
                    match entry.kind {
                        DiffEntryKind::Insert | DiffEntryKind::NoChange => {
                            log::warn!("Received duplicate `Insert` for {:?} in new set", T::TABLE_NAME);
                        }
                        DiffEntryKind::Delete => {
                            log::trace!(
                                "Initializing table {:?}: row {:?} remains present",
                                T::TABLE_NAME,
                                entry.value
                            );
                            entry.kind = DiffEntryKind::NoChange;
                        }
                    }
                }
            }
        }

        for (row_bytes, DiffEntry { kind, value }) in diff.into_iter() {
            match kind {
                DiffEntryKind::Delete => {
                    // Invoke `on_delete` callbacks; the row was previously resident but
                    // is going away.
                    callbacks.push(RowCallback::Delete(value));
                }
                DiffEntryKind::NoChange => {
                    // Insert into the new cache table, but do not invoke `on_insert`
                    // callbacks; the row was already resident.
                    self.entries.insert(row_bytes, value);
                }
                DiffEntryKind::Insert => {
                    // Insert into the new cache table and invoke `on_insert` callbacks;
                    // the row is new.
                    self.insert(callbacks, row_bytes, value);
                }
            }
        }
    }
}

impl<T: TableWithPrimaryKey> TableCache<T> {
    /// Generate a diff from the `TableRowOperation`s in the `table_update` in order to
    /// merge `delete` and `insert` operations into `update`s, then perform the operations
    /// specified in the diff and invoke callbacks as appropriate.
    fn handle_table_update_with_primary_key(
        &mut self,
        callbacks: &mut Vec<RowCallback<T>>,
        table_update: ws_messages::TableUpdate,
    ) {
        log::trace!("Handling TableUpdate for table {:?} with primary key", T::TABLE_NAME);

        enum DiffEntry<T> {
            Insert(Bytes, T),
            Delete(Bytes, T),
            Update {
                old: T,
                old_bytes: Bytes,
                new: T,
                new_bytes: Bytes,
            },
        }

        fn merge_diff_entries<T: std::fmt::Debug>(left: DiffEntry<T>, right: Option<DiffEntry<T>>) -> DiffEntry<T> {
            match (left, right) {
                (left, None) => left,
                (_, Some(u @ DiffEntry::Update { .. })) => {
                    log::warn!("Received a third `TableRowOperation` for a row which already has an `Update` within one `TableUpdate`");
                    u
                }
                (DiffEntry::Insert(new_bytes, new), Some(DiffEntry::Delete(old_bytes, old)))
                | (DiffEntry::Delete(old_bytes, old), Some(DiffEntry::Insert(new_bytes, new))) => DiffEntry::Update {
                    old,
                    old_bytes,
                    new,
                    new_bytes,
                },
                (DiffEntry::Insert(left_bytes, left), Some(DiffEntry::Insert(_, right))) => {
                    log::warn!(
                        "Received duplicate insert operations for a row within one `TableUpdate`: {:?}; {:?}",
                        left,
                        right,
                    );
                    DiffEntry::Insert(left_bytes, left)
                }
                (DiffEntry::Delete(left_bytes, left), Some(DiffEntry::Delete(_, right))) => {
                    log::warn!(
                        "Received duplicate delete operations for a row within one `TableUpdate`: {:?}; {:?}",
                        left,
                        right,
                    );
                    DiffEntry::Delete(left_bytes, left)
                }
                (DiffEntry::Update { .. }, _) => unreachable!(),
            }
        }

        fn primary_key<T: TableWithPrimaryKey>(entry: &DiffEntry<T>) -> &T::PrimaryKey {
            match entry {
                DiffEntry::Insert(_, new) => new.primary_key(),
                DiffEntry::Delete(_, old) => old.primary_key(),
                DiffEntry::Update { new, .. } => new.primary_key(),
            }
        }

        let mut diff: StdHashMap<T::PrimaryKey, DiffEntry<T>> = StdHashMap::with_capacity(
            // Pre-allocate plenty of space to minimize hash collisions.
            (table_update.inserts.len() + table_update.deletes.len()) * 2,
        );

        // Traverse the `table_update` to construct a diff, merging duplicated `Insert`
        // and `Delete` into `Update`.
        for ws_messages::BsatnBytes(row_bytes) in table_update.deletes {
            if let Some(value) = Self::decode_row(&row_bytes) {
                let diff_entry = DiffEntry::Delete(row_bytes, value);
                let pk: T::PrimaryKey = <T::PrimaryKey as Clone>::clone(primary_key(&diff_entry));
                let existing_entry = diff.remove(&pk);
                let new_entry = merge_diff_entries(diff_entry, existing_entry);
                diff.insert(pk, new_entry);
            }
        }
        for ws_messages::BsatnBytes(row_bytes) in table_update.inserts {
            if let Some(value) = Self::decode_row(&row_bytes) {
                let diff_entry = DiffEntry::Insert(row_bytes, value);
                let pk: T::PrimaryKey = <T::PrimaryKey as Clone>::clone(primary_key(&diff_entry));
                let existing_entry = diff.remove(&pk);
                let new_entry = merge_diff_entries(diff_entry, existing_entry);
                diff.insert(pk, new_entry);
            }
        }

        // Apply the `diff`.
        for diff_entry in diff.into_values() {
            match diff_entry {
                DiffEntry::Insert(row_bytes, row) => self.insert(callbacks, row_bytes, row),
                DiffEntry::Delete(row_bytes, row) => self.delete(callbacks, row_bytes, row),
                DiffEntry::Update {
                    new,
                    new_bytes,
                    old,
                    old_bytes,
                } => self.update(callbacks, old_bytes, old, new_bytes, new),
            }
        }
    }

    /// Remove `old` from the cache and replace it with `new`,
    /// and invoke any on-update callbacks.
    fn update(&mut self, callbacks: &mut Vec<RowCallback<T>>, old_bytes: Bytes, old: T, new_bytes: Bytes, new: T) {
        callbacks.push(RowCallback::Update(old, new.clone()));

        if self.entries.remove(&old_bytes).is_none() {
            log::warn!(
                "Received update for not previously resident row in table {:?}",
                T::TABLE_NAME,
            );
        }
        if self.entries.insert(new_bytes, new).is_some() {
            log::warn!(
                "Received update with already present new row in table {:?}",
                T::TABLE_NAME
            );
        }
    }
}

/// A single row callback saved in a `RowCallbackReminders`,
/// to be run after applying all row updates in the transaction.
enum RowCallback<T> {
    Insert(T),
    Delete(T),
    Update(T, T),
}

/// A collection of `RowCallback`s for each table,
/// accumulated during a transaction.
///
/// When invoking a callback, we need to pass a `db_state: Arc<ClientCache>`
/// as that callback's view of the database.
/// That `db_state` is the result of applying all of the row updates in a `SubscriptionUpdate`,
/// so we can't have it ready to go already at the point when we apply an individual row update.
/// Whenever we apply a row update, rather than immediately invoking the appropriate callback,
/// we add the callback to the transaction's `RowCallbackReminder`,
/// and then invoke all of them afterwards, once the post-transaction state is ready.
///
/// References to this struct are autogenerated by SpacetimeDB to handle messages from the database.
/// Users should not reference this struct directly.
pub struct RowCallbackReminders {
    /// "keyed" on the type `Vec<RowCallback<T>> where T: TableType`.
    /// For each table, a vec of row callbacks accumulated during this transaction.
    table_row_callbacks: Map<dyn Any>,
}

impl RowCallbackReminders {
    pub(crate) fn new() -> Self {
        RowCallbackReminders {
            table_row_callbacks: Map::new(),
        }
    }

    /// Construct a `RowCallbackReminder` with capacity
    /// appropriate for the number of table updates in `db_update`.
    pub(crate) fn new_for_database_update(db_update: &ws_messages::DatabaseUpdate) -> RowCallbackReminders {
        RowCallbackReminders {
            table_row_callbacks: Map::with_capacity(db_update.tables.len()),
        }
    }

    fn find_table_callback_reminders<T: TableType>(&mut self) -> &mut Vec<RowCallback<T>> {
        self.table_row_callbacks
            .entry::<Vec<RowCallback<T>>>()
            // TODO: Vec::with_capacity equal to the number of row updates in the `TableUpdate`.
            .or_insert(Vec::new())
    }

    /// Drain all of the callback reminders for `T` from `self` and pass them to `callbacks` for invocation.
    ///
    /// Calls to this method are autogenerated by SpacetimeDB with appropriate generic parameters
    /// in the `invoke_row_callbacks` function in `mod.rs`.
    /// Users should not call this method directly.
    pub fn invoke_callbacks<T: TableType>(
        &mut self,
        callbacks: &mut DbCallbacks,
        reducer_event: &Option<Arc<AnyReducerEvent>>,
        db_state: &ClientCacheView,
    ) {
        if let Some(callback_reminders) = self.table_row_callbacks.remove::<Vec<RowCallback<T>>>() {
            let table_callbacks = callbacks.find_table::<T>();
            for callback in callback_reminders.into_iter() {
                let db_state_handle = db_state.clone();
                match callback {
                    RowCallback::Insert(row) => {
                        table_callbacks.invoke_on_insert(row, reducer_event.clone(), db_state_handle)
                    }
                    RowCallback::Delete(row) => {
                        table_callbacks.invoke_on_delete(row, reducer_event.clone(), db_state_handle)
                    }
                    RowCallback::Update(old, new) => {
                        table_callbacks.invoke_on_update(old, new, reducer_event.clone(), db_state_handle)
                    }
                }
            }
        }
    }
}

/// A local mirror of the subscribed subset of the database.
///
/// References to this struct are autogenerated in the `handle_row_update` function.
/// Users should not reference this struct directly.
#[derive(Clone)]
pub struct ClientCache {
    /// "keyed" on the type `TableCache<T> where T: TableType`.
    tables: Map<dyn CloneAny + Send + Sync>,

    /// Contains functions autogenerated by the CLI,
    /// which handle dispatching on table names
    /// to select appropriate type parameters for various methods.
    module: Arc<dyn SpacetimeModule>,
}

impl ClientCache {
    /// Look up a type-specific `TableCache` for `T`, creating it if it does not exist.
    pub(crate) fn find_table<T: TableType>(&mut self) -> &mut TableCache<T> {
        self.tables
            .entry::<TableCache<T>>()
            .or_insert_with(|| TableCache::new())
    }

    /// Look up a type-specific `TableCache` for `T`, returning `None` if it does not exist.
    pub(crate) fn get_table<T: TableType>(&self) -> Option<&TableCache<T>> {
        self.tables.get::<TableCache<T>>()
    }

    /// Calls to this method are autogenerated in the `handle_row_update` function,
    /// which handles dispatching on the table's name
    /// to find the appropriate type `T` to `handle_table_update_` with or without `primary_key`.
    /// Users should not call this method directly.
    pub fn handle_table_update_no_primary_key<T: TableType>(
        &mut self,
        callback_reminders: &mut RowCallbackReminders,
        update: ws_messages::TableUpdate,
    ) {
        let table_cache = self.find_table::<T>();
        let table_callbacks = callback_reminders.find_table_callback_reminders::<T>();
        table_cache.handle_table_update_no_primary_key(table_callbacks, update);
    }

    /// Calls to this method are autogenerated in the `handle_row_update` function,
    /// which handles dispatching on the table's name
    /// to find the appropriate type `T` to `handle_table_update_` with or without `primary_key`.
    /// Users should not call this method directly.
    pub fn handle_table_update_with_primary_key<T: TableWithPrimaryKey>(
        &mut self,
        callback_reminders: &mut RowCallbackReminders,
        update: ws_messages::TableUpdate,
    ) {
        let table_cache = self.find_table::<T>();
        let table_callbacks = callback_reminders.find_table_callback_reminders::<T>();
        table_cache.handle_table_update_with_primary_key(table_callbacks, update);
    }

    /// Calls to this method are autogenerated in the `handle_resubscribe` function,
    /// which handles dispatching on the table's name
    /// to find the appropriate type `T` to `handle_resubscribe_for_type`.
    /// Users should not call this method directly.
    pub fn handle_resubscribe_for_type<T: TableType>(
        &mut self,
        callback_reminders: &mut RowCallbackReminders,
        new_subs: ws_messages::TableUpdate,
    ) {
        let table_cache = self.find_table::<T>();
        let table_callbacks = callback_reminders.find_table_callback_reminders::<T>();
        table_cache.reinitialize_for_new_subscribed_set(table_callbacks, new_subs);
    }

    pub(crate) fn new(module: Arc<dyn SpacetimeModule>) -> ClientCache {
        ClientCache {
            tables: Map::new(),
            module,
        }
    }

    /// Invoke the autogenerated `handle_table_update` function
    /// to dispatch on the table name in `table_update`,
    /// and invoke `ClientCache::find_table` with an apprpriate type arg.
    pub(crate) fn handle_table_update(
        &mut self,
        callback_reminders: &mut RowCallbackReminders,
        table_update: ws_messages::TableUpdate,
    ) {
        self.module
            .clone()
            .handle_table_update(table_update, self, callback_reminders);
    }

    /// Invoke the autogenerated `handle_resubscribe` function
    /// to dispatch on the table name in `new_subs`,
    /// and invoke `ClientCache::find_table` with an appropriate type arg.
    pub(crate) fn handle_table_reinitialize_for_new_subscribed_set(
        &mut self,
        callback_reminders: &mut RowCallbackReminders,
        new_subs: ws_messages::TableUpdate,
    ) {
        self.module
            .clone()
            .handle_resubscribe(new_subs, self, callback_reminders);
    }

    /// Invoke the autogenerated `invoke_row_callbacks` function
    /// to invoke all callbacks in `callback_reminders`
    /// in the state `self`.
    pub(crate) fn invoke_row_callbacks(
        self: &Arc<Self>,
        callback_reminders: &mut RowCallbackReminders,
        callback_worker: &mut DbCallbacks,
        reducer_event: Option<Arc<AnyReducerEvent>>,
    ) {
        self.module
            .invoke_row_callbacks(callback_reminders, callback_worker, reducer_event, self);
    }
}

/// A shared view into a particular state of the `ClientCache`.
pub(crate) type ClientCacheView = Arc<ClientCache>;
