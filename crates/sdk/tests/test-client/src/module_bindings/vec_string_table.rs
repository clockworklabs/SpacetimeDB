// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::vec_string_type::VecString;
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

/// Table handle for the table `vec_string`.
///
/// Obtain a handle from the [`VecStringTableAccess::vec_string`] method on [`super::RemoteTables`],
/// like `ctx.db.vec_string()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.vec_string().on_insert(...)`.
pub struct VecStringTableHandle<'ctx> {
    imp: __sdk::TableHandle<VecString>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `vec_string`.
///
/// Implemented for [`super::RemoteTables`].
pub trait VecStringTableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`VecStringTableHandle`], which mediates access to the table `vec_string`.
    fn vec_string(&self) -> VecStringTableHandle<'_>;
}

impl VecStringTableAccess for super::RemoteTables {
    fn vec_string(&self) -> VecStringTableHandle<'_> {
        VecStringTableHandle {
            imp: self.imp.get_table::<VecString>("vec_string"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct VecStringInsertCallbackId(__sdk::CallbackId);
pub struct VecStringDeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for VecStringTableHandle<'ctx> {
    type Row = VecString;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = VecString> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = VecStringInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecStringInsertCallbackId {
        VecStringInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: VecStringInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = VecStringDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecStringDeleteCallbackId {
        VecStringDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: VecStringDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {
    let _table = client_cache.get_or_make_table::<VecString>("vec_string");
}
#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::TableUpdate<VecString>> {
    __sdk::TableUpdate::parse_table_update_no_primary_key(raw_updates)
        .context("Failed to parse table update for table \"vec_string\"")
}
