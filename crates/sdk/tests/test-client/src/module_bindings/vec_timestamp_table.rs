// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
	self as __sdk,
	__lib,
	__sats,
	__ws,
};
use super::vec_timestamp_type::VecTimestamp;

/// Table handle for the table `vec_timestamp`.
///
/// Obtain a handle from the [`VecTimestampTableAccess::vec_timestamp`] method on [`super::RemoteTables`],
/// like `ctx.db.vec_timestamp()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.vec_timestamp().on_insert(...)`.
pub struct VecTimestampTableHandle<'ctx> {
    imp: __sdk::TableHandle<VecTimestamp>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `vec_timestamp`.
///
/// Implemented for [`super::RemoteTables`].
pub trait VecTimestampTableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`VecTimestampTableHandle`], which mediates access to the table `vec_timestamp`.
    fn vec_timestamp(&self) -> VecTimestampTableHandle<'_>;
}

impl VecTimestampTableAccess for super::RemoteTables {
    fn vec_timestamp(&self) -> VecTimestampTableHandle<'_> {
        VecTimestampTableHandle {
            imp: self.imp.get_table::<VecTimestamp>("vec_timestamp"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct VecTimestampInsertCallbackId(__sdk::CallbackId);
pub struct VecTimestampDeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for VecTimestampTableHandle<'ctx> {
    type Row = VecTimestamp;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 { self.imp.count() }
    fn iter(&self) -> impl Iterator<Item = VecTimestamp> + '_ { self.imp.iter() }

    type InsertCallbackId = VecTimestampInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecTimestampInsertCallbackId {
        VecTimestampInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: VecTimestampInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = VecTimestampDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecTimestampDeleteCallbackId {
        VecTimestampDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: VecTimestampDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {

        let _table = client_cache.get_or_make_table::<VecTimestamp>("vec_timestamp");
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __sdk::Result<__sdk::TableUpdate<VecTimestamp>> {
    __sdk::TableUpdate::parse_table_update(raw_updates).map_err(|e| {
        __sdk::InternalError::failed_parse(
            "TableUpdate<VecTimestamp>",
            "TableUpdate",
        ).with_cause(e).into()
    })
}
