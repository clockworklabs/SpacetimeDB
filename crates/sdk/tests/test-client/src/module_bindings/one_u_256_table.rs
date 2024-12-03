// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::one_u_256_type::OneU256;
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

/// Table handle for the table `one_u256`.
///
/// Obtain a handle from the [`OneU256TableAccess::one_u_256`] method on [`super::RemoteTables`],
/// like `ctx.db.one_u_256()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.one_u_256().on_insert(...)`.
pub struct OneU256TableHandle<'ctx> {
    imp: __sdk::TableHandle<OneU256>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `one_u256`.
///
/// Implemented for [`super::RemoteTables`].
pub trait OneU256TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`OneU256TableHandle`], which mediates access to the table `one_u256`.
    fn one_u_256(&self) -> OneU256TableHandle<'_>;
}

impl OneU256TableAccess for super::RemoteTables {
    fn one_u_256(&self) -> OneU256TableHandle<'_> {
        OneU256TableHandle {
            imp: self.imp.get_table::<OneU256>("one_u256"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct OneU256InsertCallbackId(__sdk::CallbackId);
pub struct OneU256DeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for OneU256TableHandle<'ctx> {
    type Row = OneU256;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = OneU256> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = OneU256InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> OneU256InsertCallbackId {
        OneU256InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: OneU256InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = OneU256DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> OneU256DeleteCallbackId {
        OneU256DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: OneU256DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {
    let _table = client_cache.get_or_make_table::<OneU256>("one_u256");
}
#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::TableUpdate<OneU256>> {
    __sdk::TableUpdate::parse_table_update_no_primary_key(raw_updates)
        .context("Failed to parse table update for table \"one_u256\"")
}
