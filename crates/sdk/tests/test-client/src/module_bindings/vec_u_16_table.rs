// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use super::vec_u_16_type::VecU16;
use spacetimedb_sdk::__codegen::{self as __sdk, __lib, __sats, __ws};

/// Table handle for the table `vec_u16`.
///
/// Obtain a handle from the [`VecU16TableAccess::vec_u_16`] method on [`super::RemoteTables`],
/// like `ctx.db.vec_u_16()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.vec_u_16().on_insert(...)`.
pub struct VecU16TableHandle<'ctx> {
    imp: __sdk::TableHandle<VecU16>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `vec_u16`.
///
/// Implemented for [`super::RemoteTables`].
pub trait VecU16TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`VecU16TableHandle`], which mediates access to the table `vec_u16`.
    fn vec_u_16(&self) -> VecU16TableHandle<'_>;
}

impl VecU16TableAccess for super::RemoteTables {
    fn vec_u_16(&self) -> VecU16TableHandle<'_> {
        VecU16TableHandle {
            imp: self.imp.get_table::<VecU16>("vec_u16"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct VecU16InsertCallbackId(__sdk::CallbackId);
pub struct VecU16DeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for VecU16TableHandle<'ctx> {
    type Row = VecU16;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = VecU16> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = VecU16InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecU16InsertCallbackId {
        VecU16InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: VecU16InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = VecU16DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecU16DeleteCallbackId {
        VecU16DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: VecU16DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {
    let _table = client_cache.get_or_make_table::<VecU16>("vec_u16");
}
#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __sdk::Result<__sdk::TableUpdate<VecU16>> {
    __sdk::TableUpdate::parse_table_update_no_primary_key(raw_updates).map_err(|e| __sdk::Error::Parse {
        ty: "TableUpdate<VecU16>",
        container: "TableUpdate",
        source: Box::new(e),
    })
}
