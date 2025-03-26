// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
	self as __sdk,
	__lib,
	__sats,
	__ws,
};
use super::pk_i_16_type::PkI16;

/// Table handle for the table `pk_i16`.
///
/// Obtain a handle from the [`PkI16TableAccess::pk_i_16`] method on [`super::RemoteTables`],
/// like `ctx.db.pk_i_16()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.pk_i_16().on_insert(...)`.
pub struct PkI16TableHandle<'ctx> {
    imp: __sdk::TableHandle<PkI16>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `pk_i16`.
///
/// Implemented for [`super::RemoteTables`].
pub trait PkI16TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`PkI16TableHandle`], which mediates access to the table `pk_i16`.
    fn pk_i_16(&self) -> PkI16TableHandle<'_>;
}

impl PkI16TableAccess for super::RemoteTables {
    fn pk_i_16(&self) -> PkI16TableHandle<'_> {
        PkI16TableHandle {
            imp: self.imp.get_table::<PkI16>("pk_i16"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct PkI16InsertCallbackId(__sdk::CallbackId);
pub struct PkI16DeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for PkI16TableHandle<'ctx> {
    type Row = PkI16;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 { self.imp.count() }
    fn iter(&self) -> impl Iterator<Item = PkI16> + '_ { self.imp.iter() }

    type InsertCallbackId = PkI16InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkI16InsertCallbackId {
        PkI16InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: PkI16InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = PkI16DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkI16DeleteCallbackId {
        PkI16DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: PkI16DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {

        let _table = client_cache.get_or_make_table::<PkI16>("pk_i16");
    _table.add_unique_constraint::<i16>("n", |row| &row.n);
}
pub struct PkI16UpdateCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::TableWithPrimaryKey for PkI16TableHandle<'ctx> {
    type UpdateCallbackId = PkI16UpdateCallbackId;

    fn on_update(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row, &Self::Row) + Send + 'static,
    ) -> PkI16UpdateCallbackId {
        PkI16UpdateCallbackId(self.imp.on_update(Box::new(callback)))
    }

    fn remove_on_update(&self, callback: PkI16UpdateCallbackId) {
        self.imp.remove_on_update(callback.0)
    }
}


#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __sdk::Result<__sdk::TableUpdate<PkI16>> {
    __sdk::TableUpdate::parse_table_update(raw_updates).map_err(|e| {
        __sdk::InternalError::failed_parse(
            "TableUpdate<PkI16>",
            "TableUpdate",
        ).with_cause(e).into()
    })
}

        /// Access to the `n` unique index on the table `pk_i16`,
        /// which allows point queries on the field of the same name
        /// via the [`PkI16NUnique::find`] method.
        ///
        /// Users are encouraged not to explicitly reference this type,
        /// but to directly chain method calls,
        /// like `ctx.db.pk_i_16().n().find(...)`.
        pub struct PkI16NUnique<'ctx> {
            imp: __sdk::UniqueConstraintHandle<PkI16, i16>,
            phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
        }

        impl<'ctx> PkI16TableHandle<'ctx> {
            /// Get a handle on the `n` unique index on the table `pk_i16`.
            pub fn n(&self) -> PkI16NUnique<'ctx> {
                PkI16NUnique {
                    imp: self.imp.get_unique_constraint::<i16>("n"),
                    phantom: std::marker::PhantomData,
                }
            }
        }

        impl<'ctx> PkI16NUnique<'ctx> {
            /// Find the subscribed row whose `n` column value is equal to `col_val`,
            /// if such a row is present in the client cache.
            pub fn find(&self, col_val: &i16) -> Option<PkI16> {
                self.imp.find(col_val)
            }
        }
        