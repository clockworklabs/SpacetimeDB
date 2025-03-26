// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
	self as __sdk,
	__lib,
	__sats,
	__ws,
};
use super::pk_identity_type::PkIdentity;

/// Table handle for the table `pk_identity`.
///
/// Obtain a handle from the [`PkIdentityTableAccess::pk_identity`] method on [`super::RemoteTables`],
/// like `ctx.db.pk_identity()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.pk_identity().on_insert(...)`.
pub struct PkIdentityTableHandle<'ctx> {
    imp: __sdk::TableHandle<PkIdentity>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `pk_identity`.
///
/// Implemented for [`super::RemoteTables`].
pub trait PkIdentityTableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`PkIdentityTableHandle`], which mediates access to the table `pk_identity`.
    fn pk_identity(&self) -> PkIdentityTableHandle<'_>;
}

impl PkIdentityTableAccess for super::RemoteTables {
    fn pk_identity(&self) -> PkIdentityTableHandle<'_> {
        PkIdentityTableHandle {
            imp: self.imp.get_table::<PkIdentity>("pk_identity"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct PkIdentityInsertCallbackId(__sdk::CallbackId);
pub struct PkIdentityDeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for PkIdentityTableHandle<'ctx> {
    type Row = PkIdentity;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 { self.imp.count() }
    fn iter(&self) -> impl Iterator<Item = PkIdentity> + '_ { self.imp.iter() }

    type InsertCallbackId = PkIdentityInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkIdentityInsertCallbackId {
        PkIdentityInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: PkIdentityInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = PkIdentityDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkIdentityDeleteCallbackId {
        PkIdentityDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: PkIdentityDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {

        let _table = client_cache.get_or_make_table::<PkIdentity>("pk_identity");
    _table.add_unique_constraint::<__sdk::Identity>("i", |row| &row.i);
}
pub struct PkIdentityUpdateCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::TableWithPrimaryKey for PkIdentityTableHandle<'ctx> {
    type UpdateCallbackId = PkIdentityUpdateCallbackId;

    fn on_update(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row, &Self::Row) + Send + 'static,
    ) -> PkIdentityUpdateCallbackId {
        PkIdentityUpdateCallbackId(self.imp.on_update(Box::new(callback)))
    }

    fn remove_on_update(&self, callback: PkIdentityUpdateCallbackId) {
        self.imp.remove_on_update(callback.0)
    }
}


#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __sdk::Result<__sdk::TableUpdate<PkIdentity>> {
    __sdk::TableUpdate::parse_table_update(raw_updates).map_err(|e| {
        __sdk::InternalError::failed_parse(
            "TableUpdate<PkIdentity>",
            "TableUpdate",
        ).with_cause(e).into()
    })
}

        /// Access to the `i` unique index on the table `pk_identity`,
        /// which allows point queries on the field of the same name
        /// via the [`PkIdentityIUnique::find`] method.
        ///
        /// Users are encouraged not to explicitly reference this type,
        /// but to directly chain method calls,
        /// like `ctx.db.pk_identity().i().find(...)`.
        pub struct PkIdentityIUnique<'ctx> {
            imp: __sdk::UniqueConstraintHandle<PkIdentity, __sdk::Identity>,
            phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
        }

        impl<'ctx> PkIdentityTableHandle<'ctx> {
            /// Get a handle on the `i` unique index on the table `pk_identity`.
            pub fn i(&self) -> PkIdentityIUnique<'ctx> {
                PkIdentityIUnique {
                    imp: self.imp.get_unique_constraint::<__sdk::Identity>("i"),
                    phantom: std::marker::PhantomData,
                }
            }
        }

        impl<'ctx> PkIdentityIUnique<'ctx> {
            /// Find the subscribed row whose `i` column value is equal to `col_val`,
            /// if such a row is present in the client cache.
            pub fn find(&self, col_val: &__sdk::Identity) -> Option<PkIdentity> {
                self.imp.find(col_val)
            }
        }
        