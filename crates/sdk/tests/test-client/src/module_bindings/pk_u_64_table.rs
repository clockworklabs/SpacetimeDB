// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use super::pk_u_64_type::PkU64;
use spacetimedb_sdk::__codegen::{self as __sdk, __lib, __sats, __ws};

/// Table handle for the table `pk_u64`.
///
/// Obtain a handle from the [`PkU64TableAccess::pk_u_64`] method on [`super::RemoteTables`],
/// like `ctx.db.pk_u_64()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.pk_u_64().on_insert(...)`.
pub struct PkU64TableHandle<'ctx> {
    imp: __sdk::TableHandle<PkU64>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `pk_u64`.
///
/// Implemented for [`super::RemoteTables`].
pub trait PkU64TableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`PkU64TableHandle`], which mediates access to the table `pk_u64`.
    fn pk_u_64(&self) -> PkU64TableHandle<'_>;
}

impl PkU64TableAccess for super::RemoteTables {
    fn pk_u_64(&self) -> PkU64TableHandle<'_> {
        PkU64TableHandle {
            imp: self.imp.get_table::<PkU64>("pk_u64"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct PkU64InsertCallbackId(__sdk::CallbackId);
pub struct PkU64DeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for PkU64TableHandle<'ctx> {
    type Row = PkU64;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = PkU64> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = PkU64InsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkU64InsertCallbackId {
        PkU64InsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: PkU64InsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = PkU64DeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> PkU64DeleteCallbackId {
        PkU64DeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: PkU64DeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {
    let _table = client_cache.get_or_make_table::<PkU64>("pk_u64");
    _table.add_unique_constraint::<u64>("n", |row| &row.n);
}
pub struct PkU64UpdateCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::TableWithPrimaryKey for PkU64TableHandle<'ctx> {
    type UpdateCallbackId = PkU64UpdateCallbackId;

    fn on_update(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row, &Self::Row) + Send + 'static,
    ) -> PkU64UpdateCallbackId {
        PkU64UpdateCallbackId(self.imp.on_update(Box::new(callback)))
    }

    fn remove_on_update(&self, callback: PkU64UpdateCallbackId) {
        self.imp.remove_on_update(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __sdk::Result<__sdk::TableUpdate<PkU64>> {
    __sdk::TableUpdate::parse_table_update_with_primary_key::<u64>(raw_updates, |row: &PkU64| &row.n).map_err(|e| {
        __sdk::Error::Parse {
            ty: "TableUpdate<PkU64>",
            container: "TableUpdate",
            source: Box::new(e),
        }
    })
}

/// Access to the `n` unique index on the table `pk_u64`,
/// which allows point queries on the field of the same name
/// via the [`PkU64NUnique::find`] method.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.pk_u_64().n().find(...)`.
pub struct PkU64NUnique<'ctx> {
    imp: __sdk::UniqueConstraintHandle<PkU64, u64>,
    phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

impl<'ctx> PkU64TableHandle<'ctx> {
    /// Get a handle on the `n` unique index on the table `pk_u64`.
    pub fn n(&self) -> PkU64NUnique<'ctx> {
        PkU64NUnique {
            imp: self.imp.get_unique_constraint::<u64>("n"),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<'ctx> PkU64NUnique<'ctx> {
    /// Find the subscribed row whose `n` column value is equal to `col_val`,
    /// if such a row is present in the client cache.
    pub fn find(&self, col_val: &u64) -> Option<PkU64> {
        self.imp.find(col_val)
    }
}
