//! `CallbackMap`, a set of callbacks which run asynchronously in response to messages.
//!
//! The SpacetimeDB Rust Client SDK embraces a callback-driven API,
//! where client authors register callbacks to later run in response to some event.
//!
//! Client authors may want to register multiple callbacks on the same event,
//! and then to remove specific callbacks while leaving others,
//! so we define a `CallbackId` type which uniquely identifies a registered callback,
//! and can be used to remove it.
//!
//! Callbacks may access the global `CONNECTION`, individual `TableCache`s,
//! or register or remove other callbacks. This means that the event source,
//! e.g. a `TableCache`, cannot hold its callbacks directly; doing so would require
//! a `Mutex` or `RwLock` and cause deadlocks when the callbacks attempted to re-acquire it.
//! Instead, a `CallbackMap` holds a channel to a background worker,
//! which runs the callbacks without a lock held.

use crate::spacetime_module::{Reducer, SpacetimeModule, TableUpdate};
use spacetimedb_data_structures::map::HashMap;
use std::{
    any::Any,
    marker::PhantomData,
    sync::atomic::{AtomicUsize, Ordering},
};

/// An identifier for a registered callback of type `Callback<Args>`.
///
/// Registering a callback returns a `CallbackId`,
/// which can later be used to de-register the callback.
///
/// Exported because codegen needs to reference this type.
/// SDK users should not interact with [`CallbackId`] directly.
#[doc(hidden)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CallbackId {
    id: usize,
}

impl CallbackId {
    pub(crate) fn get_next() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        CallbackId {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
        }
    }
}

pub struct DbCallbacks<M: SpacetimeModule> {
    table_callbacks: HashMap<&'static str, TableCallbacks<M>>,
    _module: PhantomData<M>,
}

impl<M: SpacetimeModule> Default for DbCallbacks<M> {
    fn default() -> Self {
        Self {
            table_callbacks: HashMap::default(),
            _module: PhantomData,
        }
    }
}

impl<M: SpacetimeModule> DbCallbacks<M> {
    pub(crate) fn get_table_callbacks(&mut self, table_name: &'static str) -> &mut TableCallbacks<M> {
        self.table_callbacks.entry(table_name).or_default()
    }

    pub fn invoke_table_row_callbacks<Row: Any>(
        &mut self,
        table_name: &'static str,
        table_update: &TableUpdate<Row>,
        event: &M::EventContext,
    ) {
        if table_update.is_empty() {
            return;
        }
        let table_callbacks = self.get_table_callbacks(table_name);
        for insert in &table_update.inserts {
            table_callbacks.invoke_on_insert(event, &insert.row);
        }
        for delete in &table_update.deletes {
            table_callbacks.invoke_on_delete(event, &delete.row);
        }
        for update in &table_update.updates {
            table_callbacks.invoke_on_update(event, &update.delete.row, &update.insert.row);
        }
    }
}

pub(crate) type RowCallback<M> = Box<dyn FnMut(&<M as SpacetimeModule>::EventContext, &dyn Any) + Send + 'static>;

type InsertCallbackMap<M> = HashMap<CallbackId, RowCallback<M>>;
type DeleteCallbackMap<M> = HashMap<CallbackId, RowCallback<M>>;

pub(crate) type UpdateCallback<M> =
    Box<dyn FnMut(&<M as SpacetimeModule>::EventContext, &dyn Any, &dyn Any) + Send + 'static>;

type UpdateCallbackMap<M> = HashMap<CallbackId, UpdateCallback<M>>;

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

pub(crate) type ReducerCallback<M> = Box<dyn FnMut(&<M as SpacetimeModule>::EventContext, &dyn Any) + Send + 'static>;

type ReducerCallbackMap<M> = HashMap<CallbackId, ReducerCallback<M>>;

/// A collection of reducer callbacks.
///
/// References to this struct are autogenerated in the `handle_event`
/// function. Users should not reference this struct directly.
pub(crate) struct ReducerCallbacks<M: SpacetimeModule> {
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
    pub(crate) fn invoke_on_reducer(&mut self, ctx: &M::EventContext, reducer: &M::Reducer) {
        let name = reducer.reducer_name();
        let args = reducer.reducer_args();
        if let Some(callbacks) = self.callbacks.get_mut(name) {
            for callback in callbacks.values_mut() {
                callback(ctx, args);
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
