// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use super::unique_bool_type::UniqueBool;
use spacetimedb_sdk::__codegen::{self as __sdk, __lib, __sats, __ws};

/// Table handle for the table `unique_bool`.
///
/// Obtain a handle from the [`UniqueBoolTableAccess::unique_bool`] method on [`super::RemoteTables`],
/// like `ctx.db.unique_bool()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.unique_bool().on_insert(...)`.
pub struct UniqueBoolTableHandle<'ctx> {
    imp: __sdk::TableHandle<UniqueBool>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `unique_bool`.
///
/// Implemented for [`super::RemoteTables`].
pub trait UniqueBoolTableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`UniqueBoolTableHandle`], which mediates access to the table `unique_bool`.
    fn unique_bool(&self) -> UniqueBoolTableHandle<'_>;
}

impl UniqueBoolTableAccess for super::RemoteTables {
    fn unique_bool(&self) -> UniqueBoolTableHandle<'_> {
        UniqueBoolTableHandle {
            imp: self.imp.get_table::<UniqueBool>("unique_bool"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct UniqueBoolInsertCallbackId(__sdk::CallbackId);
pub struct UniqueBoolDeleteCallbackId(__sdk::CallbackId);

impl<'ctx> __sdk::Table for UniqueBoolTableHandle<'ctx> {
    type Row = UniqueBool;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = UniqueBool> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = UniqueBoolInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> UniqueBoolInsertCallbackId {
        UniqueBoolInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: UniqueBoolInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = UniqueBoolDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> UniqueBoolDeleteCallbackId {
        UniqueBoolDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: UniqueBoolDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn register_table(client_cache: &mut __sdk::ClientCache<super::RemoteModule>) {
    let _table = client_cache.get_or_make_table::<UniqueBool>("unique_bool");
    _table.add_unique_constraint::<bool>("b", |row| &row.b);
}
#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __sdk::Result<__sdk::TableUpdate<UniqueBool>> {
    __sdk::TableUpdate::parse_table_update_no_primary_key(raw_updates).map_err(|e| __sdk::Error::Parse {
        ty: "TableUpdate<UniqueBool>",
        container: "TableUpdate",
        source: Box::new(e),
    })
}

/// Access to the `b` unique index on the table `unique_bool`,
/// which allows point queries on the field of the same name
/// via the [`UniqueBoolBUnique::find`] method.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.unique_bool().b().find(...)`.
pub struct UniqueBoolBUnique<'ctx> {
    imp: __sdk::UniqueConstraintHandle<UniqueBool, bool>,
    phantom: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

impl<'ctx> UniqueBoolTableHandle<'ctx> {
    /// Get a handle on the `b` unique index on the table `unique_bool`.
    pub fn b(&self) -> UniqueBoolBUnique<'ctx> {
        UniqueBoolBUnique {
            imp: self.imp.get_unique_constraint::<bool>("b"),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<'ctx> UniqueBoolBUnique<'ctx> {
    /// Find the subscribed row whose `b` column value is equal to `col_val`,
    /// if such a row is present in the client cache.
    pub fn find(&self, col_val: &bool) -> Option<UniqueBool> {
        self.imp.find(col_val)
    }
}
