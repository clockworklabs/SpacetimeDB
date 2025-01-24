// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use super::unit_struct_type::UnitStruct;
use super::vec_unit_struct_type::VecUnitStruct;
use spacetimedb_sdk::__codegen::{self as __sdk, __lib, __sats, __ws};

/// Table handle for the table `vec_unit_struct`.
///
/// Obtain a handle from the [`VecUnitStructTableAccess::vec_unit_struct`] method on [`super::RemoteTables`],
/// like `ctx.db.vec_unit_struct()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.vec_unit_struct().on_insert(...)`.
pub struct VecUnitStructTableHandle<'ctx> {
    imp: __sdk::TableHandle<VecUnitStruct>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `vec_unit_struct`.
///
/// Implemented for [`super::RemoteTables`].
pub trait VecUnitStructTableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`VecUnitStructTableHandle`], which mediates access to the table `vec_unit_struct`.
    fn vec_unit_struct(&self) -> VecUnitStructTableHandle<'_>;
}

impl VecUnitStructTableAccess for super::RemoteTables {
    fn vec_unit_struct(&self) -> VecUnitStructTableHandle<'_> {
        VecUnitStructTableHandle {
            imp: self.imp.get_table::<VecUnitStruct>("vec_unit_struct"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct VecUnitStructInsertCallbackId(__sdk::CallbackId);
pub struct VecUnitStructDeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for VecUnitStructTableHandle<'ctx> {
    type Row = VecUnitStruct;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = VecUnitStruct> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = VecUnitStructInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecUnitStructInsertCallbackId {
        VecUnitStructInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: VecUnitStructInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = VecUnitStructDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> VecUnitStructDeleteCallbackId {
        VecUnitStructDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: VecUnitStructDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {
    let _table = client_cache.get_or_make_table::<VecUnitStruct>("vec_unit_struct");
}
#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __sdk::Result<__sdk::TableUpdate<VecUnitStruct>> {
    __sdk::TableUpdate::parse_table_update_no_primary_key(raw_updates).map_err(|e| __sdk::Error::Parse {
        ty: "TableUpdate<VecUnitStruct>",
        container: "TableUpdate",
        source: Box::new(e),
    })
}
