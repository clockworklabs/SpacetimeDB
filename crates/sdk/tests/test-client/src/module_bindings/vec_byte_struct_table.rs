// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
	self as __sdk,
	__lib,
	__sats,
	__ws,
};
use super::vec_byte_struct_type::VecByteStruct;
use super::byte_struct_type::ByteStruct;

/// Table handle for the table `vec_byte_struct`.
///
/// Obtain a handle from the [`VecByteStructTableAccess::vec_byte_struct`] method on [`super::RemoteTables`],
/// like `ctx.db.vec_byte_struct()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.vec_byte_struct().on_insert(...)`.
pub struct VecByteStructTableHandle<'ctx> {
    imp: __sdk::TableHandle<VecByteStruct>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `vec_byte_struct`.
///
/// Implemented for [`super::RemoteTables`].
pub trait VecByteStructTableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`VecByteStructTableHandle`], which mediates access to the table `vec_byte_struct`.
    fn vec_byte_struct(&self) -> VecByteStructTableHandle<'_>;
}

impl VecByteStructTableAccess for super::RemoteTables {
    fn vec_byte_struct(&self) -> VecByteStructTableHandle<'_> {
        VecByteStructTableHandle {
            imp: self.imp.get_table::<VecByteStruct>("vec_byte_struct"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct VecByteStructInsertCallbackId(__sdk::CallbackId);
pub struct VecByteStructDeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for VecByteStructTableHandle<'ctx> {
    type Row = VecByteStruct;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 { self.imp.count() }
    fn iter(&self) -> impl Iterator<Item = VecByteStruct> + '_ { self.imp.iter() }

    type InsertCallbackId = VecByteStructInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecByteStructInsertCallbackId {
        VecByteStructInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: VecByteStructInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = VecByteStructDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecByteStructDeleteCallbackId {
        VecByteStructDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: VecByteStructDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {

        let _table = client_cache.get_or_make_table::<VecByteStruct>("vec_byte_struct");
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __sdk::Result<__sdk::TableUpdate<VecByteStruct>> {
    __sdk::TableUpdate::parse_table_update(raw_updates).map_err(|e| {
        __sdk::InternalError::failed_parse(
            "TableUpdate<VecByteStruct>",
            "TableUpdate",
        ).with_cause(e).into()
    })
}
