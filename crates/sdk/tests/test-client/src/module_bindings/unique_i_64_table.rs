// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use super::unique_i_64_type::UniqueI64;
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

/// Table handle for the table `unique_i64`.
///
/// Obtain a handle from the [`UniqueI64TableAccess::unique_i_64`] method on [`super::RemoteTables`],
/// like `ctx.db.unique_i_64()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.unique_i_64().on_insert(...)`.
pub struct UniqueI64TableHandle<'ctx> {
    imp: __sdk::TableHandle<UniqueI64>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `unique_i64`.
///
/// Implemented for [`super::RemoteTables`].
pub trait UniqueI64TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`UniqueI64TableHandle`], which mediates access to the table `unique_i64`.
    fn unique_i_64(&self) -> UniqueI64TableHandle<'_>;
}

impl UniqueI64TableAccess for super::RemoteTables {
    fn unique_i_64(&self) -> UniqueI64TableHandle<'_> {
        UniqueI64TableHandle {
            imp: self.imp.get_table::<UniqueI64>("unique_i64"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct UniqueI64InsertCallbackId(__sdk::CallbackId);
pub struct UniqueI64DeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for UniqueI64TableHandle<'ctx> {
    type Row = UniqueI64;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = UniqueI64> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = UniqueI64InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> UniqueI64InsertCallbackId {
        UniqueI64InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: UniqueI64InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = UniqueI64DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> UniqueI64DeleteCallbackId {
        UniqueI64DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: UniqueI64DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {
    let _table = client_cache.get_or_make_table::<UniqueI64>("unique_i64");
    _table.add_unique_constraint::<i64>("n", |row| &row.n);
}
#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::TableUpdate<UniqueI64>> {
    __sdk::TableUpdate::parse_table_update_no_primary_key(raw_updates)
        .context("Failed to parse table update for table \"unique_i64\"")
}

/// Access to the `n` unique index on the table `unique_i64`,
/// which allows point queries on the field of the same name
/// via the [`UniqueI64NUnique::find`] method.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.unique_i_64().n().find(...)`.
pub struct UniqueI64NUnique<'ctx> {
    imp: __sdk::UniqueConstraintHandle<UniqueI64, i64>,
    phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

impl<'ctx> UniqueI64TableHandle<'ctx> {
    /// Get a handle on the `n` unique index on the table `unique_i64`.
    pub fn n(&self) -> UniqueI64NUnique<'ctx> {
        UniqueI64NUnique {
            imp: self.imp.get_unique_constraint::<i64>("n"),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<'ctx> UniqueI64NUnique<'ctx> {
    /// Find the subscribed row whose `n` column value is equal to `col_val`,
    /// if such a row is present in the client cache.
    pub fn find(&self, col_val: &i64) -> Option<UniqueI64> {
        self.imp.find(col_val)
    }
}
