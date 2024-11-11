// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::vec_u_8_type::VecU8;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

/// Table handle for the table `vec_u8`.
///
/// Obtain a handle from the [`VecU8TableAccess::vec_u_8`] method on [`super::RemoteTables`],
/// like `ctx.db.vec_u_8()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.vec_u_8().on_insert(...)`.
pub struct VecU8TableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<VecU8>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `vec_u8`.
///
/// Implemented for [`super::RemoteTables`].
pub trait VecU8TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`VecU8TableHandle`], which mediates access to the table `vec_u8`.
    fn vec_u_8(&self) -> VecU8TableHandle<'_>;
}

impl VecU8TableAccess for super::RemoteTables {
    fn vec_u_8(&self) -> VecU8TableHandle<'_> {
        VecU8TableHandle {
            imp: self.imp.get_table::<VecU8>(86),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct VecU8InsertCallbackId(__sdk::callbacks::CallbackId);
pub struct VecU8DeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for VecU8TableHandle<'ctx> {
    type Row = VecU8;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = VecU8> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = VecU8InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecU8InsertCallbackId {
        VecU8InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: VecU8InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = VecU8DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecU8DeleteCallbackId {
        VecU8DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: VecU8DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<VecU8>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_no_primary_key(raw_updates)
        .context("Failed to parse table update for table \"vec_u8\"")
}
