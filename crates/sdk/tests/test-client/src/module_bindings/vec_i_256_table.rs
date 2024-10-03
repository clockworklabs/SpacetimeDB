// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::vec_i_256_type::VecI256;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

/// Table handle for the table `vec_i256`.
///
/// Obtain a handle from the [`VecI256TableAccess::vec_i_256`] method on [`super::RemoteTables`],
/// like `ctx.db.vec_i_256()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.vec_i_256().on_insert(...)`.
pub struct VecI256TableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<VecI256>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `vec_i256`.
///
/// Implemented for [`super::RemoteTables`].
pub trait VecI256TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`VecI256TableHandle`], which mediates access to the table `vec_i256`.
    fn vec_i_256(&self) -> VecI256TableHandle<'_>;
}

impl VecI256TableAccess for super::RemoteTables {
    fn vec_i_256(&self) -> VecI256TableHandle<'_> {
        VecI256TableHandle {
            imp: self.imp.get_table::<VecI256>("vec_i256"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct VecI256InsertCallbackId(__sdk::callbacks::CallbackId);
pub struct VecI256DeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for VecI256TableHandle<'ctx> {
    type Row = VecI256;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = VecI256> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = VecI256InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecI256InsertCallbackId {
        VecI256InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: VecI256InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = VecI256DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecI256DeleteCallbackId {
        VecI256DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: VecI256DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<VecI256>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_no_primary_key(raw_updates)
        .context("Failed to parse table update for table \"vec_i256\"")
}