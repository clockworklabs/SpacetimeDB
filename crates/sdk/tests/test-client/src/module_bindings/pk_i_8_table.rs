// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::pk_i_8_type::PkI8;
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

/// Table handle for the table `pk_i8`.
///
/// Obtain a handle from the [`PkI8TableAccess::pk_i_8`] method on [`super::RemoteTables`],
/// like `ctx.db.pk_i_8()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.pk_i_8().on_insert(...)`.
pub struct PkI8TableHandle<'ctx> {
    imp: __sdk::TableHandle<PkI8>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `pk_i8`.
///
/// Implemented for [`super::RemoteTables`].
pub trait PkI8TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`PkI8TableHandle`], which mediates access to the table `pk_i8`.
    fn pk_i_8(&self) -> PkI8TableHandle<'_>;
}

impl PkI8TableAccess for super::RemoteTables {
    fn pk_i_8(&self) -> PkI8TableHandle<'_> {
        PkI8TableHandle {
            imp: self.imp.get_table::<PkI8>("pk_i8"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct PkI8InsertCallbackId(__sdk::CallbackId);
pub struct PkI8DeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for PkI8TableHandle<'ctx> {
    type Row = PkI8;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = PkI8> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = PkI8InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkI8InsertCallbackId {
        PkI8InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: PkI8InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = PkI8DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkI8DeleteCallbackId {
        PkI8DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: PkI8DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {
    let _table = client_cache.get_or_make_table::<PkI8>("pk_i8");
    _table.add_unique_constraint::<i8>("n", |row| &row.n)
}
pub struct PkI8UpdateCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::TableWithPrimaryKey for PkI8TableHandle<'ctx> {
    type UpdateCallbackId = PkI8UpdateCallbackId;

    fn on_update(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row, &Self::Row) + Send + 'static,
    ) -> PkI8UpdateCallbackId {
        PkI8UpdateCallbackId(self.imp.on_update(Box::new(callback)))
    }

    fn remove_on_update(&self, callback: PkI8UpdateCallbackId) {
        self.imp.remove_on_update(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::TableUpdate<PkI8>> {
    __sdk::TableUpdate::parse_table_update_with_primary_key::<i8>(raw_updates, |row: &PkI8| &row.n)
        .context("Failed to parse table update for table \"pk_i8\"")
}

/// Access to the `n` unique index on the table `pk_i8`,
/// which allows point queries on the field of the same name
/// via the [`PkI8NUnique::find`] method.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.pk_i_8().n().find(...)`.
pub struct PkI8NUnique<'ctx> {
    imp: __sdk::UniqueConstraintHandle<PkI8, i8>,
    phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

impl<'ctx> PkI8TableHandle<'ctx> {
    /// Get a handle on the `n` unique index on the table `pk_i8`.
    pub fn n(&self) -> PkI8NUnique<'ctx> {
        PkI8NUnique {
            imp: self.imp.get_unique_constraint::<i8>("n"),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<'ctx> PkI8NUnique<'ctx> {
    /// Find the subscribed row whose `n` column value is equal to `col_val`,
    /// if such a row is present in the client cache.
    pub fn find(&self, col_val: &i8) -> Option<PkI8> {
        self.imp.find(col_val)
    }
}
