// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::every_vec_struct_type::EveryVecStruct;
use super::vec_every_vec_struct_type::VecEveryVecStruct;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

/// Table handle for the table `vec_every_vec_struct`.
///
/// Obtain a handle from the [`VecEveryVecStructTableAccess::vec_every_vec_struct`] method on [`super::RemoteTables`],
/// like `ctx.db.vec_every_vec_struct()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.vec_every_vec_struct().on_insert(...)`.
pub struct VecEveryVecStructTableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<VecEveryVecStruct>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `vec_every_vec_struct`.
///
/// Implemented for [`super::RemoteTables`].
pub trait VecEveryVecStructTableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`VecEveryVecStructTableHandle`], which mediates access to the table `vec_every_vec_struct`.
    fn vec_every_vec_struct(&self) -> VecEveryVecStructTableHandle<'_>;
}

impl VecEveryVecStructTableAccess for super::RemoteTables {
    fn vec_every_vec_struct(&self) -> VecEveryVecStructTableHandle<'_> {
        VecEveryVecStructTableHandle {
            imp: self.imp.get_table::<VecEveryVecStruct>(69),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct VecEveryVecStructInsertCallbackId(__sdk::callbacks::CallbackId);
pub struct VecEveryVecStructDeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for VecEveryVecStructTableHandle<'ctx> {
    type Row = VecEveryVecStruct;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = VecEveryVecStruct> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = VecEveryVecStructInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecEveryVecStructInsertCallbackId {
        VecEveryVecStructInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: VecEveryVecStructInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = VecEveryVecStructDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecEveryVecStructDeleteCallbackId {
        VecEveryVecStructDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: VecEveryVecStructDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<VecEveryVecStruct>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_no_primary_key(raw_updates)
        .context("Failed to parse table update for table \"vec_every_vec_struct\"")
}
