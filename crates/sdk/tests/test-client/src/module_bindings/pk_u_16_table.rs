// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
	self as __sdk,
	__lib,
	__sats,
	__ws,
};
use super::pk_u_16_type::PkU16;

/// Table handle for the table `pk_u16`.
///
/// Obtain a handle from the [`PkU16TableAccess::pk_u_16`] method on [`super::RemoteTables`],
/// like `ctx.db.pk_u_16()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.pk_u_16().on_insert(...)`.
pub struct PkU16TableHandle<'ctx> {
    imp: __sdk::TableHandle<PkU16>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `pk_u16`.
///
/// Implemented for [`super::RemoteTables`].
pub trait PkU16TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`PkU16TableHandle`], which mediates access to the table `pk_u16`.
    fn pk_u_16(&self) -> PkU16TableHandle<'_>;
}

impl PkU16TableAccess for super::RemoteTables {
    fn pk_u_16(&self) -> PkU16TableHandle<'_> {
        PkU16TableHandle {
            imp: self.imp.get_table::<PkU16>("pk_u16"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct PkU16InsertCallbackId(__sdk::CallbackId);
pub struct PkU16DeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for PkU16TableHandle<'ctx> {
    type Row = PkU16;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 { self.imp.count() }
    fn iter(&self) -> impl Iterator<Item = PkU16> + '_ { self.imp.iter() }

    type InsertCallbackId = PkU16InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkU16InsertCallbackId {
        PkU16InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: PkU16InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = PkU16DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkU16DeleteCallbackId {
        PkU16DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: PkU16DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {

        let _table = client_cache.get_or_make_table::<PkU16>("pk_u16");
    _table.add_unique_constraint::<u16>("n", |row| &row.n);
}
pub struct PkU16UpdateCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::TableWithPrimaryKey for PkU16TableHandle<'ctx> {
    type UpdateCallbackId = PkU16UpdateCallbackId;

    fn on_update(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row, &Self::Row) + Send + 'static,
    ) -> PkU16UpdateCallbackId {
        PkU16UpdateCallbackId(self.imp.on_update(Box::new(callback)))
    }

    fn remove_on_update(&self, callback: PkU16UpdateCallbackId) {
        self.imp.remove_on_update(callback.0)
    }
}


#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __sdk::Result<__sdk::TableUpdate<PkU16>> {
    __sdk::TableUpdate::parse_table_update(raw_updates).map_err(|e| {
        __sdk::InternalError::failed_parse(
            "TableUpdate<PkU16>",
            "TableUpdate",
        ).with_cause(e).into()
    })
}

        /// Access to the `n` unique index on the table `pk_u16`,
        /// which allows point queries on the field of the same name
        /// via the [`PkU16NUnique::find`] method.
        ///
        /// Users are encouraged not to explicitly reference this type,
        /// but to directly chain method calls,
        /// like `ctx.db.pk_u_16().n().find(...)`.
        pub struct PkU16NUnique<'ctx> {
            imp: __sdk::UniqueConstraintHandle<PkU16, u16>,
            phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
        }

        impl<'ctx> PkU16TableHandle<'ctx> {
            /// Get a handle on the `n` unique index on the table `pk_u16`.
            pub fn n(&self) -> PkU16NUnique<'ctx> {
                PkU16NUnique {
                    imp: self.imp.get_unique_constraint::<u16>("n"),
                    phantom: std::marker::PhantomData,
                }
            }
        }

        impl<'ctx> PkU16NUnique<'ctx> {
            /// Find the subscribed row whose `n` column value is equal to `col_val`,
            /// if such a row is present in the client cache.
            pub fn find(&self, col_val: &u16) -> Option<PkU16> {
                self.imp.find(col_val)
            }
        }
        