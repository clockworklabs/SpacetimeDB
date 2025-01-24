// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use super::one_u_128_type::OneU128;
use spacetimedb_sdk::__codegen::{self as __sdk, __lib, __sats, __ws};

/// Table handle for the table `one_u128`.
///
/// Obtain a handle from the [`OneU128TableAccess::one_u_128`] method on [`super::RemoteTables`],
/// like `ctx.db.one_u_128()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.one_u_128().on_insert(...)`.
pub struct OneU128TableHandle<'ctx> {
    imp: __sdk::TableHandle<OneU128>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `one_u128`.
///
/// Implemented for [`super::RemoteTables`].
pub trait OneU128TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`OneU128TableHandle`], which mediates access to the table `one_u128`.
    fn one_u_128(&self) -> OneU128TableHandle<'_>;
}

impl OneU128TableAccess for super::RemoteTables {
    fn one_u_128(&self) -> OneU128TableHandle<'_> {
        OneU128TableHandle {
            imp: self.imp.get_table::<OneU128>("one_u128"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct OneU128InsertCallbackId(__sdk::CallbackId);
pub struct OneU128DeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for OneU128TableHandle<'ctx> {
    type Row = OneU128;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = OneU128> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = OneU128InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> OneU128InsertCallbackId {
        OneU128InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: OneU128InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = OneU128DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> OneU128DeleteCallbackId {
        OneU128DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: OneU128DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {
    let _table = client_cache.get_or_make_table::<OneU128>("one_u128");
}
#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __sdk::Result<__sdk::TableUpdate<OneU128>> {
    __sdk::TableUpdate::parse_table_update_no_primary_key(raw_updates).map_err(|e| __sdk::Error::Parse {
        ty: "TableUpdate<OneU128>",
        container: "TableUpdate",
        source: Box::new(e),
    })
}
