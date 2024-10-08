// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::unique_string_type::UniqueString;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

/// Table handle for the table `unique_string`.
///
/// Obtain a handle from the [`UniqueStringTableAccess::unique_string`] method on [`super::RemoteTables`],
/// like `ctx.db.unique_string()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.unique_string().on_insert(...)`.
pub struct UniqueStringTableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<UniqueString>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `unique_string`.
///
/// Implemented for [`super::RemoteTables`].
pub trait UniqueStringTableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`UniqueStringTableHandle`], which mediates access to the table `unique_string`.
    fn unique_string(&self) -> UniqueStringTableHandle<'_>;
}

impl UniqueStringTableAccess for super::RemoteTables {
    fn unique_string(&self) -> UniqueStringTableHandle<'_> {
        UniqueStringTableHandle {
            imp: self.imp.get_table::<UniqueString>("unique_string"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct UniqueStringInsertCallbackId(__sdk::callbacks::CallbackId);
pub struct UniqueStringDeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for UniqueStringTableHandle<'ctx> {
    type Row = UniqueString;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = UniqueString> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = UniqueStringInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> UniqueStringInsertCallbackId {
        UniqueStringInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: UniqueStringInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = UniqueStringDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> UniqueStringDeleteCallbackId {
        UniqueStringDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: UniqueStringDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<UniqueString>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_no_primary_key(raw_updates)
        .context("Failed to parse table update for table \"unique_string\"")
}

/// Access to the `s` unique index on the table `unique_string`,
/// which allows point queries on the field of the same name
/// via the [`UniqueStringSUnique::find`] method.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.unique_string().s().find(...)`.
pub struct UniqueStringSUnique<'ctx> {
    imp: __sdk::client_cache::UniqueConstraint<UniqueString, String>,
    phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

impl<'ctx> UniqueStringTableHandle<'ctx> {
    /// Get a handle on the `s` unique index on the table `unique_string`.
    pub fn s(&self) -> UniqueStringSUnique<'ctx> {
        UniqueStringSUnique {
            imp: self.imp.get_unique_constraint::<String>("s", |row| &row.s),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<'ctx> UniqueStringSUnique<'ctx> {
    /// Find the subscribed row whose `s` column value is equal to `col_val`,
    /// if such a row is present in the client cache.
    pub fn find(&self, col_val: &String) -> Option<UniqueString> {
        self.imp.find(col_val)
    }
}
