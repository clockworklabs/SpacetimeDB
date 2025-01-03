// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::unique_i_128_type::UniqueI128;
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

/// Table handle for the table `unique_i128`.
///
/// Obtain a handle from the [`UniqueI128TableAccess::unique_i_128`] method on [`super::RemoteTables`],
/// like `ctx.db.unique_i_128()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.unique_i_128().on_insert(...)`.
pub struct UniqueI128TableHandle<'ctx> {
    imp: __sdk::TableHandle<UniqueI128>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `unique_i128`.
///
/// Implemented for [`super::RemoteTables`].
pub trait UniqueI128TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`UniqueI128TableHandle`], which mediates access to the table `unique_i128`.
    fn unique_i_128(&self) -> UniqueI128TableHandle<'_>;
}

impl UniqueI128TableAccess for super::RemoteTables {
    fn unique_i_128(&self) -> UniqueI128TableHandle<'_> {
        UniqueI128TableHandle {
            imp: self.imp.get_table::<UniqueI128>("unique_i128"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct UniqueI128InsertCallbackId(__sdk::CallbackId);
pub struct UniqueI128DeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for UniqueI128TableHandle<'ctx> {
    type Row = UniqueI128;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = UniqueI128> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = UniqueI128InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> UniqueI128InsertCallbackId {
        UniqueI128InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: UniqueI128InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = UniqueI128DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> UniqueI128DeleteCallbackId {
        UniqueI128DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: UniqueI128DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {
    let _table = client_cache.get_or_make_table::<UniqueI128>("unique_i128");
    _table.add_unique_constraint::<i128>("n", |row| &row.n);
}
#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::TableUpdate<UniqueI128>> {
    __sdk::TableUpdate::parse_table_update_no_primary_key(raw_updates)
        .context("Failed to parse table update for table \"unique_i128\"")
}

/// Access to the `n` unique index on the table `unique_i128`,
/// which allows point queries on the field of the same name
/// via the [`UniqueI128NUnique::find`] method.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.unique_i_128().n().find(...)`.
pub struct UniqueI128NUnique<'ctx> {
    imp: __sdk::UniqueConstraintHandle<UniqueI128, i128>,
    phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

impl<'ctx> UniqueI128TableHandle<'ctx> {
    /// Get a handle on the `n` unique index on the table `unique_i128`.
    pub fn n(&self) -> UniqueI128NUnique<'ctx> {
        UniqueI128NUnique {
            imp: self.imp.get_unique_constraint::<i128>("n"),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<'ctx> UniqueI128NUnique<'ctx> {
    /// Find the subscribed row whose `n` column value is equal to `col_val`,
    /// if such a row is present in the client cache.
    pub fn find(&self, col_val: &i128) -> Option<UniqueI128> {
        self.imp.find(col_val)
    }
}
