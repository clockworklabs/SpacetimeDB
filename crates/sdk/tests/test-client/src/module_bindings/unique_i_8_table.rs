// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
	self as __sdk,
	__lib,
	__sats,
	__ws,
};
use super::unique_i_8_type::UniqueI8;

/// Table handle for the table `unique_i8`.
///
/// Obtain a handle from the [`UniqueI8TableAccess::unique_i_8`] method on [`super::RemoteTables`],
/// like `ctx.db.unique_i_8()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.unique_i_8().on_insert(...)`.
pub struct UniqueI8TableHandle<'ctx> {
    imp: __sdk::TableHandle<UniqueI8>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `unique_i8`.
///
/// Implemented for [`super::RemoteTables`].
pub trait UniqueI8TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`UniqueI8TableHandle`], which mediates access to the table `unique_i8`.
    fn unique_i_8(&self) -> UniqueI8TableHandle<'_>;
}

impl UniqueI8TableAccess for super::RemoteTables {
    fn unique_i_8(&self) -> UniqueI8TableHandle<'_> {
        UniqueI8TableHandle {
            imp: self.imp.get_table::<UniqueI8>("unique_i8"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct UniqueI8InsertCallbackId(__sdk::CallbackId);
pub struct UniqueI8DeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for UniqueI8TableHandle<'ctx> {
    type Row = UniqueI8;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 { self.imp.count() }
    fn iter(&self) -> impl Iterator<Item = UniqueI8> + '_ { self.imp.iter() }

    type InsertCallbackId = UniqueI8InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> UniqueI8InsertCallbackId {
        UniqueI8InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: UniqueI8InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = UniqueI8DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> UniqueI8DeleteCallbackId {
        UniqueI8DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: UniqueI8DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {

        let _table = client_cache.get_or_make_table::<UniqueI8>("unique_i8");
    _table.add_unique_constraint::<i8>("n", |row| &row.n);
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __sdk::Result<__sdk::TableUpdate<UniqueI8>> {
    __sdk::TableUpdate::parse_table_update(raw_updates).map_err(|e| {
        __sdk::InternalError::failed_parse(
            "TableUpdate<UniqueI8>",
            "TableUpdate",
        ).with_cause(e).into()
    })
}

        /// Access to the `n` unique index on the table `unique_i8`,
        /// which allows point queries on the field of the same name
        /// via the [`UniqueI8NUnique::find`] method.
        ///
        /// Users are encouraged not to explicitly reference this type,
        /// but to directly chain method calls,
        /// like `ctx.db.unique_i_8().n().find(...)`.
        pub struct UniqueI8NUnique<'ctx> {
            imp: __sdk::UniqueConstraintHandle<UniqueI8, i8>,
            phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
        }

        impl<'ctx> UniqueI8TableHandle<'ctx> {
            /// Get a handle on the `n` unique index on the table `unique_i8`.
            pub fn n(&self) -> UniqueI8NUnique<'ctx> {
                UniqueI8NUnique {
                    imp: self.imp.get_unique_constraint::<i8>("n"),
                    phantom: std::marker::PhantomData,
                }
            }
        }

        impl<'ctx> UniqueI8NUnique<'ctx> {
            /// Find the subscribed row whose `n` column value is equal to `col_val`,
            /// if such a row is present in the client cache.
            pub fn find(&self, col_val: &i8) -> Option<UniqueI8> {
                self.imp.find(col_val)
            }
        }
        