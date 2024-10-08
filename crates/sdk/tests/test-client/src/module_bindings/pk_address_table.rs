// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::pk_address_type::PkAddress;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

/// Table handle for the table `pk_address`.
///
/// Obtain a handle from the [`PkAddressTableAccess::pk_address`] method on [`super::RemoteTables`],
/// like `ctx.db.pk_address()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.pk_address().on_insert(...)`.
pub struct PkAddressTableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<PkAddress>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `pk_address`.
///
/// Implemented for [`super::RemoteTables`].
pub trait PkAddressTableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`PkAddressTableHandle`], which mediates access to the table `pk_address`.
    fn pk_address(&self) -> PkAddressTableHandle<'_>;
}

impl PkAddressTableAccess for super::RemoteTables {
    fn pk_address(&self) -> PkAddressTableHandle<'_> {
        PkAddressTableHandle {
            imp: self.imp.get_table::<PkAddress>("pk_address"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct PkAddressInsertCallbackId(__sdk::callbacks::CallbackId);
pub struct PkAddressDeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for PkAddressTableHandle<'ctx> {
    type Row = PkAddress;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = PkAddress> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = PkAddressInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkAddressInsertCallbackId {
        PkAddressInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: PkAddressInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = PkAddressDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkAddressDeleteCallbackId {
        PkAddressDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: PkAddressDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

pub struct PkAddressUpdateCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::TableWithPrimaryKey for PkAddressTableHandle<'ctx> {
    type UpdateCallbackId = PkAddressUpdateCallbackId;

    fn on_update(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row, &Self::Row) + Send + 'static,
    ) -> PkAddressUpdateCallbackId {
        PkAddressUpdateCallbackId(self.imp.on_update(Box::new(callback)))
    }

    fn remove_on_update(&self, callback: PkAddressUpdateCallbackId) {
        self.imp.remove_on_update(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<PkAddress>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_with_primary_key::<__sdk::Address>(
        raw_updates,
        |row: &PkAddress| &row.a,
    )
    .context("Failed to parse table update for table \"pk_address\"")
}

/// Access to the `a` unique index on the table `pk_address`,
/// which allows point queries on the field of the same name
/// via the [`PkAddressAUnique::find`] method.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.pk_address().a().find(...)`.
pub struct PkAddressAUnique<'ctx> {
    imp: __sdk::client_cache::UniqueConstraint<PkAddress, __sdk::Address>,
    phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

impl<'ctx> PkAddressTableHandle<'ctx> {
    /// Get a handle on the `a` unique index on the table `pk_address`.
    pub fn a(&self) -> PkAddressAUnique<'ctx> {
        PkAddressAUnique {
            imp: self.imp.get_unique_constraint::<__sdk::Address>("a", |row| &row.a),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<'ctx> PkAddressAUnique<'ctx> {
    /// Find the subscribed row whose `a` column value is equal to `col_val`,
    /// if such a row is present in the client cache.
    pub fn find(&self, col_val: &__sdk::Address) -> Option<PkAddress> {
        self.imp.find(col_val)
    }
}
