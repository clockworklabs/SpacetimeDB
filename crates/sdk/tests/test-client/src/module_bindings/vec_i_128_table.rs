// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::vec_i_128_type::VecI128;
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

/// Table handle for the table `vec_i128`.
///
/// Obtain a handle from the [`VecI128TableAccess::vec_i_128`] method on [`super::RemoteTables`],
/// like `ctx.db.vec_i_128()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.vec_i_128().on_insert(...)`.
pub struct VecI128TableHandle<'ctx> {
    imp: __sdk::TableHandle<VecI128>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `vec_i128`.
///
/// Implemented for [`super::RemoteTables`].
pub trait VecI128TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`VecI128TableHandle`], which mediates access to the table `vec_i128`.
    fn vec_i_128(&self) -> VecI128TableHandle<'_>;
}

impl VecI128TableAccess for super::RemoteTables {
    fn vec_i_128(&self) -> VecI128TableHandle<'_> {
        VecI128TableHandle {
            imp: self.imp.get_table::<VecI128>("vec_i128"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct VecI128InsertCallbackId(__sdk::CallbackId);
pub struct VecI128DeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for VecI128TableHandle<'ctx> {
    type Row = VecI128;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = VecI128> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = VecI128InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecI128InsertCallbackId {
        VecI128InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: VecI128InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = VecI128DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecI128DeleteCallbackId {
        VecI128DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: VecI128DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {
    let _table = client_cache.get_or_make_table::<VecI128>("vec_i128");
}
#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::TableUpdate<VecI128>> {
    __sdk::TableUpdate::parse_table_update_no_primary_key(raw_updates)
        .context("Failed to parse table update for table \"vec_i128\"")
}
