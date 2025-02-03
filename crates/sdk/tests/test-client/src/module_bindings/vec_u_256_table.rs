// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use super::vec_u_256_type::VecU256;
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

/// Table handle for the table `vec_u256`.
///
/// Obtain a handle from the [`VecU256TableAccess::vec_u_256`] method on [`super::RemoteTables`],
/// like `ctx.db.vec_u_256()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.vec_u_256().on_insert(...)`.
pub struct VecU256TableHandle<'ctx> {
    imp: __sdk::TableHandle<VecU256>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `vec_u256`.
///
/// Implemented for [`super::RemoteTables`].
pub trait VecU256TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`VecU256TableHandle`], which mediates access to the table `vec_u256`.
    fn vec_u_256(&self) -> VecU256TableHandle<'_>;
}

impl VecU256TableAccess for super::RemoteTables {
    fn vec_u_256(&self) -> VecU256TableHandle<'_> {
        VecU256TableHandle {
            imp: self.imp.get_table::<VecU256>("vec_u256"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct VecU256InsertCallbackId(__sdk::CallbackId);
pub struct VecU256DeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for VecU256TableHandle<'ctx> {
    type Row = VecU256;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = VecU256> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = VecU256InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecU256InsertCallbackId {
        VecU256InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: VecU256InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = VecU256DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecU256DeleteCallbackId {
        VecU256DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: VecU256DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {
    let _table = client_cache.get_or_make_table::<VecU256>("vec_u256");
}
#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::TableUpdate<VecU256>> {
    __sdk::TableUpdate::parse_table_update_no_primary_key(raw_updates)
        .context("Failed to parse table update for table \"vec_u256\"")
}
