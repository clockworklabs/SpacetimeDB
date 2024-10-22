// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::vec_i_32_type::VecI32;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

/// Table handle for the table `vec_i32`.
///
/// Obtain a handle from the [`VecI32TableAccess::vec_i_32`] method on [`super::RemoteTables`],
/// like `ctx.db.vec_i_32()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.vec_i_32().on_insert(...)`.
pub struct VecI32TableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<VecI32>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `vec_i32`.
///
/// Implemented for [`super::RemoteTables`].
pub trait VecI32TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`VecI32TableHandle`], which mediates access to the table `vec_i32`.
    fn vec_i_32(&self) -> VecI32TableHandle<'_>;
}

impl VecI32TableAccess for super::RemoteTables {
    fn vec_i_32(&self) -> VecI32TableHandle<'_> {
        VecI32TableHandle {
            imp: self.imp.get_table::<VecI32>(75),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct VecI32InsertCallbackId(__sdk::callbacks::CallbackId);
pub struct VecI32DeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for VecI32TableHandle<'ctx> {
    type Row = VecI32;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = VecI32> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = VecI32InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecI32InsertCallbackId {
        VecI32InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: VecI32InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = VecI32DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecI32DeleteCallbackId {
        VecI32DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: VecI32DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<VecI32>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_no_primary_key(raw_updates)
        .context("Failed to parse table update for table \"vec_i32\"")
}
