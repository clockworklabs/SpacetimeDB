//! Internal structures for managing row and reducer callbacks.
//!
//! The SpacetimeDB Rust Client SDK embraces a callback-driven API,
//! where client authors register callbacks to later run in response to some event.
//!
//! Client authors may want to register multiple callbacks on the same event,
//! and then to remove specific callbacks while leaving others,
//! so we define a `CallbackId` type which uniquely identifies a registered callback,
//! and can be used to remove it.
//!
//! Callbacks may access the database context through an `EventContext`,
//! and may therefore add or remove callbacks on the same or other events,
//! query the client cache, add or remove subscriptions, and make many other mutations.
//! To prevent deadlocks or re-entrancy, the SDK arranges to defer all such mutations in a queue.
//!
//! This module is internal, and may incompatibly change without warning.

use crate::{
    client_cache::TableAppliedDiff,
    spacetime_module::{AbstractEventContext, Reducer, SpacetimeModule},
};
use spacetimedb_data_structures::map::HashMap;
use std::{
    any::Any,
    sync::atomic::{AtomicUsize, Ordering},
};

/// An identifier for a registered callback.
///
/// Registering a callback returns a `CallbackId`,
/// which can later be used to de-register the callback.
///
/// Exported because codegen needs to reference this type.
/// SDK users should not interact with [`CallbackId`] directly,
/// instead using specific generated callback ID types.
#[doc(hidden)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CallbackId {
    id: usize,
}

impl CallbackId {
    /// We maintain a global monotonic counter of [`CallbackId`]s,
    /// even though we only need local uniqueness,
    /// because it's easier than keeping track of a bunch of different counters.
    pub(crate) fn get_next() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        CallbackId {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
        }
    }
}

/// Manages row callbacks for a `DbContext`/`DbConnection`.
pub struct DbCallbacks<M: SpacetimeModule> {
    /// Maps table name to a set of callbacks.
    table_callbacks: HashMap<&'static str, TableCallbacks<M>>,
}

impl<M: SpacetimeModule> Default for DbCallbacks<M> {
    fn default() -> Self {
        Self {
            table_callbacks: HashMap::default(),
        }
    }
}

impl<M: SpacetimeModule> DbCallbacks<M> {
    /// Get the [`TableCallbacks`] for the table `table_name`.
    pub(crate) fn get_table_callbacks(&mut self, table_name: &'static str) -> &mut TableCallbacks<M> {
        self.table_callbacks.entry(table_name).or_default()
    }

    /// Invoke all row callbacks for rows modified by `applied_diff` for the table `table_name`.
    pub fn invoke_table_row_callbacks<Row: Any>(
        &mut self,
        table_name: &'static str,
        applied_diff: &TableAppliedDiff<Row>,
        event: &M::EventContext,
    ) {
        if applied_diff.is_empty() {
            return;
        }
        let table_callbacks = self.get_table_callbacks(table_name);
        for row in applied_diff.inserts() {
            table_callbacks.invoke_on_insert(event, row);
        }
        for row in applied_diff.deletes() {
            table_callbacks.invoke_on_delete(event, row);
        }
        for (del, ins) in applied_diff.updates() {
            table_callbacks.invoke_on_update(event, del, ins);
        }
    }
}

/// An insert or delete callback for a row defined by the module `M`.
///
/// Rows are passed to callbacks as `&dyn Any`,
/// and a wrapper inserted by the SDK will downcast to the actual row type
/// before invoking the user-supplied function.
pub(crate) type RowCallback<M> = Box<dyn FnMut(&<M as SpacetimeModule>::EventContext, &dyn Any) + Send + 'static>;

type InsertCallbackMap<M> = HashMap<CallbackId, RowCallback<M>>;
type DeleteCallbackMap<M> = HashMap<CallbackId, RowCallback<M>>;

/// An update callback for a row defined by the module `M`.
///
/// Rows are passed to callbacks as `&dyn Any`,
/// and a wrapper inserted by the SDK will downcast to the actual row type
/// before invoking the user-supplied function.
pub(crate) type UpdateCallback<M> =
    Box<dyn FnMut(&<M as SpacetimeModule>::EventContext, &dyn Any, &dyn Any) + Send + 'static>;

type UpdateCallbackMap<M> = HashMap<CallbackId, UpdateCallback<M>>;

/// A set of insert, delete and update callbacks for a particular table defined by the module `M`.
///
/// We store a set of update callbacks for all tables, even those which do not have a primary key field.
/// The public codegen interface makes it statically impossible to register or invoke such a callback.
pub(crate) struct TableCallbacks<M: SpacetimeModule> {
    on_insert: InsertCallbackMap<M>,
    on_delete: DeleteCallbackMap<M>,
    on_update: UpdateCallbackMap<M>,
}

impl<M: SpacetimeModule> Default for TableCallbacks<M> {
    fn default() -> Self {
        Self {
            on_insert: Default::default(),
            on_delete: Default::default(),
            on_update: Default::default(),
        }
    }
}

impl<M: SpacetimeModule> TableCallbacks<M> {
    pub(crate) fn register_on_insert(&mut self, callback_id: CallbackId, callback: RowCallback<M>) {
        self.on_insert.insert(callback_id, callback);
    }

    pub(crate) fn register_on_delete(&mut self, callback_id: CallbackId, callback: RowCallback<M>) {
        self.on_delete.insert(callback_id, callback);
    }

    pub(crate) fn register_on_update(&mut self, callback_id: CallbackId, callback: UpdateCallback<M>) {
        self.on_update.insert(callback_id, callback);
    }

    pub(crate) fn remove_on_insert(&mut self, callback_id: CallbackId) {
        // Ugly: `impl FnMut` is `must_use`.
        // If we don't `.expect` this, no diagnostic,
        // but we want to assert that we actually removed a callback,
        // we just don't want to invoke it.
        // So we have to `let _ =`.
        let _ = self
            .on_insert
            .remove(&callback_id)
            .expect("Attempt to remove non-existent insert callback");
    }

    pub(crate) fn remove_on_delete(&mut self, callback_id: CallbackId) {
        // Ugly: `impl FnMut` is `must_use`.
        // If we don't `.expect` this, no diagnostic,
        // but we want to assert that we actually removed a callback,
        // we just don't want to invoke it.
        // So we have to `let _ =`.
        let _ = self
            .on_delete
            .remove(&callback_id)
            .expect("Attempt to remove non-existent delete callback");
    }

    pub(crate) fn remove_on_update(&mut self, callback_id: CallbackId) {
        // Ugly: `impl FnMut` is `must_use`.
        // If we don't `.expect` this, no diagnostic,
        // but we want to assert that we actually removed a callback,
        // we just don't want to invoke it.
        // So we have to `let _ =`.
        let _ = self
            .on_update
            .remove(&callback_id)
            .expect("Attempt to remove non-existent update callback");
    }

    fn invoke_on_insert(&mut self, ctx: &M::EventContext, row: &dyn Any) {
        for callback in self.on_insert.values_mut() {
            callback(ctx, row);
        }
    }

    fn invoke_on_delete(&mut self, ctx: &M::EventContext, row: &dyn Any) {
        for callback in self.on_delete.values_mut() {
            callback(ctx, row);
        }
    }

    fn invoke_on_update(&mut self, ctx: &M::EventContext, old: &dyn Any, new: &dyn Any) {
        for callback in self.on_update.values_mut() {
            callback(ctx, old, new);
        }
    }
}

/// A reducer callback for a reducer defined by the module `M`.
///
/// Reducer arguments are passed to callbacks within the `EventContext`,
/// and a wrapper inserted by the SDK will destructure the contained `Event`
/// before invoking the user-supplied function.
pub(crate) type ReducerCallback<M> = Box<dyn FnMut(&<M as SpacetimeModule>::ReducerEventContext) + Send + 'static>;

type ReducerCallbackMap<M> = HashMap<CallbackId, ReducerCallback<M>>;

/// A collection of reducer callbacks.
///
/// References to this struct are autogenerated in the `handle_event`
/// function. Users should not reference this struct directly.
pub(crate) struct ReducerCallbacks<M: SpacetimeModule> {
    /// Maps reducer name to a set of callbacks.
    callbacks: HashMap<&'static str, ReducerCallbackMap<M>>,
}

impl<M: SpacetimeModule> Default for ReducerCallbacks<M> {
    fn default() -> Self {
        Self {
            callbacks: Default::default(),
        }
    }
}

impl<M: SpacetimeModule> ReducerCallbacks<M> {
    pub(crate) fn invoke_on_reducer(&mut self, ctx: &M::ReducerEventContext) {
        let reducer = ctx.event();
        let name = reducer.reducer.reducer_name();
        if let Some(callbacks) = self.callbacks.get_mut(name) {
            for callback in callbacks.values_mut() {
                callback(ctx);
            }
        }
    }

    pub(crate) fn register_on_reducer(
        &mut self,
        reducer: &'static str,
        callback_id: CallbackId,
        callback: ReducerCallback<M>,
    ) {
        self.callbacks.entry(reducer).or_default().insert(callback_id, callback);
    }

    pub(crate) fn remove_on_reducer(&mut self, reducer: &'static str, callback_id: CallbackId) {
        // Ugly: `impl FnMut` is `must_use`.
        // If we don't `.expect` this, no diagnostic,
        // but we want to assert that we actually removed a callback,
        // we just don't want to invoke it.
        // So we have to `let _ =`.
        let _ = self
            .callbacks
            .get_mut(reducer)
            .expect("Attempt to remove a callback from a reducer which doesn't have any")
            .remove(&callback_id)
            .expect("Attempt to remove non-existent reducer callback");
    }
}
