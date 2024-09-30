// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use super::option_simple_enum_type::OptionSimpleEnum;
use super::simple_enum_type::SimpleEnum;
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

/// Table handle for the table `option_simple_enum`.
///
/// Obtain a handle from the [`OptionSimpleEnumTableAccess::option_simple_enum`] method on [`super::RemoteTables`],
/// like `ctx.db.option_simple_enum()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.option_simple_enum().on_insert(...)`.
pub struct OptionSimpleEnumTableHandle<'ctx> {
    imp: __sdk::db_connection::TableHandle<OptionSimpleEnum>,
    ctx: std::marker::PhantomData<&'ctx super::RemoteTables>,
}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `option_simple_enum`.
///
/// Implemented for [`super::RemoteTables`].
pub trait OptionSimpleEnumTableAccess {
    #[allow(non_snake_case)]
    /// Obtain a [`OptionSimpleEnumTableHandle`], which mediates access to the table `option_simple_enum`.
    fn option_simple_enum(&self) -> OptionSimpleEnumTableHandle<'_>;
}

impl OptionSimpleEnumTableAccess for super::RemoteTables {
    fn option_simple_enum(&self) -> OptionSimpleEnumTableHandle<'_> {
        OptionSimpleEnumTableHandle {
            imp: self.imp.get_table::<OptionSimpleEnum>("option_simple_enum"),
            ctx: std::marker::PhantomData,
        }
    }
}

pub struct OptionSimpleEnumInsertCallbackId(__sdk::callbacks::CallbackId);
pub struct OptionSimpleEnumDeleteCallbackId(__sdk::callbacks::CallbackId);

impl<'ctx> __sdk::table::Table for OptionSimpleEnumTableHandle<'ctx> {
    type Row = OptionSimpleEnum;
    type EventContext = super::EventContext;

    fn count(&self) -> u64 {
        self.imp.count()
    }
    fn iter(&self) -> impl Iterator<Item = OptionSimpleEnum> + '_ {
        self.imp.iter()
    }

    type InsertCallbackId = OptionSimpleEnumInsertCallbackId;

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> OptionSimpleEnumInsertCallbackId {
        OptionSimpleEnumInsertCallbackId(self.imp.on_insert(Box::new(callback)))
    }

    fn remove_on_insert(&self, callback: OptionSimpleEnumInsertCallbackId) {
        self.imp.remove_on_insert(callback.0)
    }

    type DeleteCallbackId = OptionSimpleEnumDeleteCallbackId;

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> OptionSimpleEnumDeleteCallbackId {
        OptionSimpleEnumDeleteCallbackId(self.imp.on_delete(Box::new(callback)))
    }

    fn remove_on_delete(&self, callback: OptionSimpleEnumDeleteCallbackId) {
        self.imp.remove_on_delete(callback.0)
    }
}

#[doc(hidden)]
pub(super) fn parse_table_update(
    raw_updates: __ws::TableUpdate<__ws::BsatnFormat>,
) -> __anyhow::Result<__sdk::spacetime_module::TableUpdate<OptionSimpleEnum>> {
    __sdk::spacetime_module::TableUpdate::parse_table_update_no_primary_key(raw_updates)
        .context("Failed to parse table update for table \"option_simple_enum\"")
}
